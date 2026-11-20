//! UV unwrap: cut a surface into islands along seams, flatten each island and pack the islands into the 0..1 square.
//! Design notes: docs/dev/uv-editor.md.
//!
//! Pipeline (`Model::unwrap_uv`):
//! 1. the selected faces are read into a [`Soup`]: per face its corners (outer loop, then holes), triangles and normal, and
//!    the links between faces that share an edge (a link knows its seam flag and its dihedral angle);
//! 2. islands are the faces connected without crossing a cut link (a marked seam, or, with auto seams, a hard edge);
//! 3. each island is flattened. A disc-like island goes through LSCM (least-squares conformal maps, `lscm_solve`); an island
//!    that is not a disc (a closed surface, a handle), folds over itself, or stretches too much is split into regions that
//!    lean the same way (`cluster`, with a tightening angle) and those are flattened in turn; a flat island, and the
//!    last resort (one face), is projected onto its plane, which cannot fail;
//! 4. the islands are scaled to one texel density and packed (`pack`, maximal rectangles) with a margin.
//!
//! Coordinates: (u, v) in tiles, v up, the same as `Face::uv`. A face's corner list is its outer loop followed by each hole.

use crate::geom;
use crate::model::{uv32_ring, FaceUv, Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// split on hard edges (the angle threshold), then LSCM
    Angle,
    /// LSCM islands, split only where a surface can't be flattened in one piece
    Conformal,
    /// regions that lean the same way, each projected flat
    PlanarIslands,
}

impl Method {
    pub fn parse(s: &str) -> Result<Method> {
        Ok(match s {
            "angle" => Method::Angle,
            "conformal" | "lscm" => Method::Conformal,
            "planar_islands" | "planar" => Method::PlanarIslands,
            o => bail!("unknown unwrap method \"{o}\": use angle, conformal or planar_islands"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamMode {
    /// only the edges marked as seams
    Marked,
    /// the marked seams and every hard edge (dihedral angle above the threshold)
    Auto,
}

impl SeamMode {
    pub fn parse(s: &str) -> Result<SeamMode> {
        Ok(match s {
            "marked" => SeamMode::Marked,
            "auto" => SeamMode::Auto,
            o => bail!("unknown seams mode \"{o}\": use marked or auto"),
        })
    }
}

/// A corner held in place: face, index in its corner list, where it goes.
#[derive(Clone, Copy, Debug)]
pub struct Pin {
    pub face: Id,
    pub corner: usize,
    pub uv: [f64; 2],
}

#[derive(Clone, Debug)]
pub struct Opts {
    pub method: Method,
    pub seams: SeamMode,
    /// degrees: the hard-edge threshold, and the first cone when a surface has to be split
    pub angle: f64,
    /// space between islands and to the border, in tiles
    pub margin: f64,
    pub pins: Vec<Pin>,
    /// 1: `Face::uv`, 2: the lightmap channel `Face::uv2`
    pub set: u8,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { method: Method::Conformal, seams: SeamMode::Marked, angle: 60.0, margin: 0.01, pins: vec![], set: 1 }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub faces: usize,
    pub islands: usize,
    /// islands that had to be cut further because they could not be flattened whole
    pub split: usize,
    pub pinned: usize,
    pub ignored_pins: usize,
    /// the area-weighted mean of max(r, 1/r), r being a triangle's area in the picture over its area in the model
    /// (1 = the same density everywhere)
    pub stretch: f64,
    /// tiles per metre
    pub scale: f64,
}

// ---- the face soup ---------------------------------------------------------------------------------------------------

struct FaceD {
    id: Id,
    /// model vertex per corner
    verts: Vec<Id>,
    pts: Vec<DVec3>,
    normal: DVec3,
    /// triangles as corner indices
    tris: Vec<[usize; 3]>,
    area: f64,
}

/// one face's use of an edge: the face, its corners at the lower / higher vertex id, and whether it walks low to high
type EdgeUse = (usize, [usize; 2], bool);

struct Link {
    f: usize,
    g: usize,
    /// per face, the corner on the edge's lower / higher vertex id
    fc: [usize; 2],
    gc: [usize; 2],
    seam: bool,
    /// the dihedral angle (degrees) between the faces as one continuous surface
    angle: f64,
    /// both faces walk the edge the same way: g is oriented against f
    against: bool,
}

struct Soup {
    faces: Vec<FaceD>,
    links: Vec<Link>,
    nbr: Vec<Vec<usize>>,
}

fn tri_area(a: DVec3, b: DVec3, c: DVec3) -> f64 {
    (b - a).cross(c - a).length() * 0.5
}

impl Soup {
    fn new(m: &Model, ids: &[Id]) -> Soup {
        let mut faces = Vec::with_capacity(ids.len());
        let mut dirs: HashMap<(Id, Id), Vec<EdgeUse>> = HashMap::new();
        for &id in ids {
            let f = &m.faces[&id];
            let loops: Vec<&Vec<Id>> = f.loops().collect();
            let verts: Vec<Id> = loops.iter().flat_map(|l| l.iter().copied()).collect();
            let pts: Vec<DVec3> = verts.iter().map(|v| m.p(*v)).collect();
            let outer = &pts[..f.outer.len()];
            let mut normal = geom::newell(outer);
            if normal.length_squared() < 1e-24 {
                normal = DVec3::Z;
            }
            let mut holes = vec![];
            let mut off = f.outer.len();
            for h in &f.holes {
                holes.push(pts[off..off + h.len()].to_vec());
                off += h.len();
            }
            let mut first: HashMap<[u64; 3], usize> = HashMap::new();
            for (i, p) in pts.iter().enumerate() {
                first.entry([p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).or_insert(i);
            }
            let find = |p: &DVec3| first.get(&[p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).copied();
            let mut tris = vec![];
            for t in geom::triangulate(outer, &holes, normal) {
                if let (Some(a), Some(b), Some(c)) = (find(&t[0]), find(&t[1]), find(&t[2])) {
                    tris.push([a, b, c]);
                }
            }
            let area = tris.iter().map(|t| tri_area(pts[t[0]], pts[t[1]], pts[t[2]])).sum();
            let fi = faces.len();
            let mut off = 0;
            for l in &loops {
                for k in 0..l.len() {
                    let (ca, cb) = (off + k, off + (k + 1) % l.len());
                    let (a, b) = (verts[ca], verts[cb]);
                    if a == b {
                        continue;
                    }
                    let (lo, hi) = (a.min(b), a.max(b));
                    // corners ordered (lower id, higher id); the flag: does this face walk lo -> hi
                    let corners = if a < b { [ca, cb] } else { [cb, ca] };
                    dirs.entry((lo, hi)).or_default().push((fi, corners, a < b));
                }
                off += l.len();
            }
            faces.push(FaceD { id, verts, pts, normal: normal.normalize(), tris, area });
        }
        let edge_ids: HashMap<(Id, Id), Id> = m.edges.iter().map(|(&id, e)| ((e.a.min(e.b), e.a.max(e.b)), id)).collect();
        let mut links = vec![];
        let mut nbr = vec![Vec::new(); faces.len()];
        let mut keys: Vec<_> = dirs.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let v = &dirs[&key];
            if v.len() != 2 || v[0].0 == v[1].0 {
                continue;
            }
            let (f, g) = (v[0].0, v[1].0);
            let against = v[0].2 == v[1].2;
            let ng = if against { -faces[g].normal } else { faces[g].normal };
            let angle = faces[f].normal.dot(ng).clamp(-1.0, 1.0).acos().to_degrees();
            let seam = edge_ids.get(&key).is_some_and(|e| m.edges[e].seam);
            nbr[f].push(links.len());
            nbr[g].push(links.len());
            links.push(Link { f, g, fc: v[0].1, gc: v[1].1, seam, angle, against });
        }
        Soup { faces, links, nbr }
    }

    fn other(&self, li: usize, f: usize) -> usize {
        let l = &self.links[li];
        if l.f == f { l.g } else { l.f }
    }

    /// +1 / -1 per face of `comp`: which way it faces relative to the first one, going across shared edges.
    fn orient(&self, comp: &[usize]) -> HashMap<usize, f64> {
        let inside: HashSet<usize> = comp.iter().copied().collect();
        let mut sign: HashMap<usize, f64> = HashMap::new();
        for &s in comp {
            if sign.contains_key(&s) {
                continue;
            }
            sign.insert(s, 1.0);
            let mut q = VecDeque::from([s]);
            while let Some(f) = q.pop_front() {
                let sf = sign[&f];
                for &li in &self.nbr[f] {
                    let g = self.other(li, f);
                    if !inside.contains(&g) || sign.contains_key(&g) {
                        continue;
                    }
                    sign.insert(g, if self.links[li].against { -sf } else { sf });
                    q.push_back(g);
                }
            }
        }
        sign
    }

    /// Faces of `faces` joined without crossing a cut link, in a stable order.
    fn components(&self, faces: &[usize], cut: &dyn Fn(&Link) -> bool) -> Vec<Vec<usize>> {
        let inside: HashSet<usize> = faces.iter().copied().collect();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out = vec![];
        for &s in faces {
            if !seen.insert(s) {
                continue;
            }
            let mut comp = vec![s];
            let mut q = VecDeque::from([s]);
            while let Some(f) = q.pop_front() {
                for &li in &self.nbr[f] {
                    let g = self.other(li, f);
                    if cut(&self.links[li]) || !inside.contains(&g) || !seen.insert(g) {
                        continue;
                    }
                    comp.push(g);
                    q.push_back(g);
                }
            }
            out.push(comp);
        }
        out
    }

    /// Regions of `comp` whose faces lean within `theta` degrees of the region's mean normal.
    fn cluster(&self, comp: &[usize], theta: f64, cut: &dyn Fn(&Link) -> bool) -> Vec<Vec<usize>> {
        let sign = self.orient(comp);
        let inside: HashSet<usize> = comp.iter().copied().collect();
        let nrm = |f: usize| self.faces[f].normal * sign[&f];
        let mut seeds = comp.to_vec();
        seeds.sort_by(|a, b| self.faces[*b].area.partial_cmp(&self.faces[*a].area).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
        let cos = theta.to_radians().cos();
        let mut done: HashSet<usize> = HashSet::new();
        let mut out = vec![];
        for s in seeds {
            if !done.insert(s) {
                continue;
            }
            let mut region = vec![s];
            let mut sum = nrm(s) * self.faces[s].area.max(1e-12);
            let mut q = VecDeque::from([s]);
            while let Some(f) = q.pop_front() {
                for &li in &self.nbr[f] {
                    let g = self.other(li, f);
                    if cut(&self.links[li]) || !inside.contains(&g) || done.contains(&g) {
                        continue;
                    }
                    if nrm(g).dot(sum.normalize_or_zero()) >= cos {
                        done.insert(g);
                        sum += nrm(g) * self.faces[g].area.max(1e-12);
                        region.push(g);
                        q.push_back(g);
                    }
                }
            }
            out.push(region);
        }
        out
    }
}

// ---- flattening ------------------------------------------------------------------------------------------------------

/// One flattened island: per face (soup index), the corners' coordinates in the island's own frame (metres), or in
/// tiles when `fixed` (the island holds pins and keeps its place).
struct Piece {
    faces: Vec<(usize, Vec<[f64; 2]>)>,
    rotatable: bool,
    fixed: bool,
}

struct Ctx<'a> {
    soup: &'a Soup,
    opts: &'a Opts,
    cut: &'a dyn Fn(&Link) -> bool,
    pins: HashMap<usize, Vec<(usize, [f64; 2])>>,
    out: Vec<Piece>,
    split: usize,
    ignored: usize,
    pinned: usize,
}

const MAX_STRETCH: f64 = 3.0;

/// Orthonormal frame of a plane with normal `n`: u to the right and v up when seen from the side `n` points to (v is the
/// world's up where the plane isn't flat); u x v = n.
fn plane_frame(n: DVec3) -> (DVec3, DVec3) {
    if n.z.abs() < 0.9 {
        let v = (DVec3::Z - n * n.z).normalize();
        (v.cross(n), v)
    } else {
        let u = (DVec3::X - n * n.x).normalize();
        (u, n.cross(u))
    }
}

impl Ctx<'_> {
    fn planar(&self, comp: &[usize]) -> Piece {
        let sign = self.soup.orient(comp);
        let mut n = DVec3::ZERO;
        for &f in comp {
            let fd = &self.soup.faces[f];
            n += fd.normal * sign[&f] * fd.area.max(1e-12);
        }
        let n = n.try_normalize().unwrap_or(self.soup.faces[comp[0]].normal);
        let (u, v) = plane_frame(n);
        Piece {
            faces: comp.iter().map(|&f| (f, self.soup.faces[f].pts.iter().map(|p| [p.dot(u), p.dot(v)]).collect())).collect(),
            rotatable: self.opts.set == 2,
            fixed: false,
        }
    }

    /// Is every face of `comp` (nearly) in one plane, facing one way?
    fn is_flat(&self, comp: &[usize]) -> bool {
        let sign = self.soup.orient(comp);
        let n0 = self.soup.faces[comp[0]].normal * sign[&comp[0]];
        comp.iter().all(|&f| (self.soup.faces[f].normal * sign[&f]).dot(n0) > 0.9998)
    }

    fn solve(&mut self, comp: Vec<usize>, theta: f64) {
        if self.opts.method == Method::PlanarIslands {
            let cl = self.soup.cluster(&comp, self.opts.angle.clamp(1.0, 89.0), self.cut);
            self.split += (cl.len() > 1) as usize;
            for c in cl {
                let p = self.planar(&c);
                self.out.push(p);
            }
            return;
        }
        if self.is_flat(&comp) && self.pins.is_empty() {
            let p = self.planar(&comp);
            self.out.push(p);
            return;
        }
        if let Some(p) = self.lscm_piece(&comp) {
            self.out.push(p);
            return;
        }
        // not a disc, folds over, or stretches too much: split into regions that lean alike and try again
        let t = if theta == 0.0 { self.opts.angle.clamp(20.0, 80.0) } else { theta * 0.6 };
        if theta == 0.0 {
            self.split += 1;
        }
        if comp.len() == 1 || t < 2.0 {
            for f in comp {
                let p = self.planar(&[f]);
                self.out.push(p);
            }
            return;
        }
        let cl = self.soup.cluster(&comp, t, self.cut);
        if cl.len() == 1 {
            self.solve(cl.into_iter().next().unwrap(), t);
        } else {
            for c in cl {
                self.solve(c, t);
            }
        }
    }

    /// LSCM on a disc-like island; None when it isn't one or the result is no good.
    fn lscm_piece(&mut self, comp: &[usize]) -> Option<Piece> {
        let soup = self.soup;
        let sign = soup.orient(comp);
        let inside: HashSet<usize> = comp.iter().copied().collect();
        // corner numbering
        let mut base: HashMap<usize, usize> = HashMap::new();
        let mut total = 0;
        for &f in comp {
            base.insert(f, total);
            total += soup.faces[f].verts.len();
        }
        // wedges: corners of one vertex that meet through an uncut shared edge
        let mut uf: Vec<usize> = (0..total).collect();
        fn find(uf: &mut [usize], mut i: usize) -> usize {
            while uf[i] != i {
                uf[i] = uf[uf[i]];
                i = uf[i];
            }
            i
        }
        for &f in comp {
            for &li in &soup.nbr[f] {
                let l = &soup.links[li];
                let g = soup.other(li, f);
                if f != l.f || !inside.contains(&g) || (self.cut)(l) {
                    continue;
                }
                for k in 0..2 {
                    let a = find(&mut uf, base[&l.f] + l.fc[k]);
                    let b = find(&mut uf, base[&l.g] + l.gc[k]);
                    if a != b {
                        uf[a.max(b)] = a.min(b);
                    }
                }
            }
        }
        let mut wid: HashMap<usize, usize> = HashMap::new();
        let mut wedge_of = vec![0usize; total];
        for (c, w) in wedge_of.iter_mut().enumerate() {
            let r = find(&mut uf, c);
            let n = wid.len();
            *w = *wid.entry(r).or_insert(n);
        }
        let nw = wid.len();
        // triangles (all facing one way), with their 3D corners
        let mut tris: Vec<[usize; 3]> = vec![];
        let mut pos3: Vec<DVec3> = vec![DVec3::ZERO; nw];
        let mut face_tris: Vec<(usize, usize)> = vec![]; // (face, tri index) for later
        for &f in comp {
            let fd = &soup.faces[f];
            for t in &fd.tris {
                let mut w = [wedge_of[base[&f] + t[0]], wedge_of[base[&f] + t[1]], wedge_of[base[&f] + t[2]]];
                if sign[&f] < 0.0 {
                    w.swap(1, 2);
                }
                for k in 0..3 {
                    pos3[wedge_of[base[&f] + t[k]]] = fd.pts[t[k]];
                }
                tris.push(w);
                face_tris.push((f, tris.len() - 1));
            }
        }
        if tris.is_empty() || !is_disc(nw, &tris) {
            return None;
        }
        // pins: wedges held at a place
        let mut fixed: Vec<(usize, [f64; 2])> = vec![];
        for &f in comp {
            if let Some(ps) = self.pins.get(&f) {
                for &(corner, uv) in ps {
                    let w = wedge_of[base[&f] + corner];
                    if !fixed.iter().any(|(x, _)| *x == w) {
                        fixed.push((w, uv));
                    }
                }
            }
        }
        let n_pins = fixed.len();
        let pinned = n_pins >= 2 && fixed.iter().any(|(_, p)| *p != fixed[0].1);
        let mut res = None;
        if pinned {
            res = lscm_mesh_pins(&pos3, &tris, &fixed);
            if let Some(uv) = &res {
                if !valid(&pos3, &tris, uv, true).0 {
                    res = None;
                }
            }
        }
        let used_pins = res.is_some();
        if res.is_none() {
            res = lscm_mesh_pins(&pos3, &tris, &[]);
        }
        let mut uv = res?;
        if !used_pins && signed_area(&tris, &uv) < 0.0 {
            uv.iter_mut().for_each(|p| p[1] = -p[1]);
        }
        let (ok, stretch_ok) = valid(&pos3, &tris, &uv, used_pins);
        if !ok || !stretch_ok {
            return None;
        }
        if used_pins {
            self.pinned += n_pins;
        } else {
            self.ignored += n_pins;
        }
        if !used_pins {
            // a free island: put its long side along u so rectangles pack tight, and make it as big as it is in the model
            align_min_box(&mut uv);
        }
        let mut faces: Vec<(usize, Vec<[f64; 2]>)> = vec![];
        for &f in comp {
            faces.push((f, (0..soup.faces[f].verts.len()).map(|c| uv[wedge_of[base[&f] + c]]).collect()));
        }
        Some(Piece { faces, rotatable: true, fixed: used_pins })
    }
}

/// Is the triangle mesh a topological disc: manifold, connected boundary of one loop, Euler characteristic 1?
fn is_disc(n: usize, tris: &[[usize; 3]]) -> bool {
    let mut count: HashMap<(usize, usize), u32> = HashMap::new();
    for t in tris {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            return false;
        }
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    if count.values().any(|c| *c > 2) {
        return false;
    }
    let used: HashSet<usize> = tris.iter().flatten().copied().collect();
    let _ = n;
    let chi = used.len() as i64 - count.len() as i64 + tris.len() as i64;
    if chi != 1 {
        return false;
    }
    // boundary: every boundary vertex has exactly two boundary edges, all one loop
    let mut b: HashMap<usize, Vec<usize>> = HashMap::new();
    for ((a, c), k) in &count {
        if *k == 1 {
            b.entry(*a).or_default().push(*c);
            b.entry(*c).or_default().push(*a);
        }
    }
    if b.is_empty() || b.values().any(|v| v.len() != 2) {
        return false;
    }
    let start = *b.keys().min().unwrap();
    let (mut prev, mut cur, mut len) = (usize::MAX, start, 0);
    loop {
        let next = b[&cur].iter().copied().find(|x| *x != prev).unwrap_or(b[&cur][0]);
        prev = cur;
        cur = next;
        len += 1;
        if cur == start || len > b.len() {
            break;
        }
    }
    len == b.len()
}

struct LTri {
    v: [usize; 3],
    /// weights, real and imaginary parts, per corner
    w: [[f64; 2]; 3],
}

fn local_tris(pts: &[DVec3], tris: &[[usize; 3]]) -> Vec<LTri> {
    let mut out = vec![];
    for t in tris {
        let (p1, p2, p3) = (pts[t[0]], pts[t[1]], pts[t[2]]);
        let e = p2 - p1;
        let nrm = e.cross(p3 - p1);
        if nrm.length() < 1e-14 {
            continue;
        }
        let ex = e.normalize();
        let ey = nrm.normalize().cross(ex);
        let q = [[0.0, 0.0], [e.length(), 0.0], [(p3 - p1).dot(ex), (p3 - p1).dot(ey)]];
        let dt = q[0][0] * q[1][1] - q[0][1] * q[1][0] + q[1][0] * q[2][1] - q[1][1] * q[2][0] + q[2][0] * q[0][1] - q[2][1] * q[0][0];
        let s = dt.abs().sqrt();
        if s < 1e-12 {
            continue;
        }
        let w = |a: usize, b: usize| [(q[b][0] - q[a][0]) / s, (q[b][1] - q[a][1]) / s];
        out.push(LTri { v: *t, w: [w(1, 2), w(2, 0), w(0, 1)] });
    }
    out
}

/// LSCM of a disc with the given fixed vertices (at least two); none: two far apart boundary vertices are chosen.
/// The scale is that of the pins (or metres). None if the solve fails.
fn lscm_mesh_pins(pts: &[DVec3], tris: &[[usize; 3]], fixed: &[(usize, [f64; 2])]) -> Option<Vec<[f64; 2]>> {
    let n = pts.len();
    let lt = local_tris(pts, tris);
    if lt.is_empty() {
        return None;
    }
    let mut pin: Vec<(usize, [f64; 2])> = fixed.to_vec();
    if pin.len() < 2 {
        // two boundary vertices far apart
        let mut cnt: HashMap<(usize, usize), u32> = HashMap::new();
        for t in tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *cnt.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut bv: Vec<usize> = cnt.iter().filter(|(_, c)| **c == 1).flat_map(|((a, b), _)| [*a, *b]).collect();
        bv.sort_unstable();
        bv.dedup();
        if bv.len() < 2 {
            return None;
        }
        let c = bv.iter().fold(DVec3::ZERO, |s, v| s + pts[*v]) / bv.len() as f64;
        let far = |from: DVec3| bv.iter().copied().max_by(|a, b| pts[*a].distance_squared(from).partial_cmp(&pts[*b].distance_squared(from)).unwrap().then(b.cmp(a))).unwrap();
        let a = far(c);
        let b = far(pts[a]);
        let d = pts[a].distance(pts[b]);
        if a == b || d < 1e-12 {
            return None;
        }
        pin = vec![(a, [0.0, 0.0]), (b, [d, 0.0])];
    }
    lscm_solve(n, &lt, &pin)
}

fn lscm_solve(n: usize, tris: &[LTri], fixed: &[(usize, [f64; 2])]) -> Option<Vec<[f64; 2]>> {
    let mut fval: Vec<Option<[f64; 2]>> = vec![None; n];
    for (v, p) in fixed {
        fval[*v] = Some(*p);
    }
    let mut used = vec![false; n];
    for t in tris {
        for v in t.v {
            used[v] = true;
        }
    }
    let mut fidx = vec![usize::MAX; n];
    let mut nfree = 0;
    for v in 0..n {
        if used[v] && fval[v].is_none() {
            fidx[v] = nfree;
            nfree += 1;
        }
    }
    let m = 2 * nfree;
    let mut rows: Vec<Vec<(usize, f64)>> = Vec::with_capacity(tris.len() * 2);
    let mut b: Vec<f64> = Vec::with_capacity(tris.len() * 2);
    for t in tris {
        let (mut r1, mut r2) = (Vec::with_capacity(6), Vec::with_capacity(6));
        let (mut b1, mut b2) = (0.0, 0.0);
        for k in 0..3 {
            let (wr, wi) = (t.w[k][0], t.w[k][1]);
            let v = t.v[k];
            match fval[v] {
                Some(p) => {
                    b1 -= wr * p[0] - wi * p[1];
                    b2 -= wi * p[0] + wr * p[1];
                }
                None => {
                    let i = fidx[v];
                    r1.push((i, wr));
                    r1.push((nfree + i, -wi));
                    r2.push((i, wi));
                    r2.push((nfree + i, wr));
                }
            }
        }
        rows.push(r1);
        rows.push(r2);
        b.push(b1);
        b.push(b2);
    }
    // scale the columns to unit length (Jacobi preconditioning)
    let mut cn = vec![0.0f64; m];
    for r in &rows {
        for (c, v) in r {
            cn[*c] += v * v;
        }
    }
    let d: Vec<f64> = cn.iter().map(|x| if *x > 1e-30 { 1.0 / x.sqrt() } else { 0.0 }).collect();
    for r in &mut rows {
        for (c, v) in r.iter_mut() {
            *v *= d[*c];
        }
    }
    let a_mul = |x: &[f64], out: &mut [f64]| {
        for (i, r) in rows.iter().enumerate() {
            out[i] = r.iter().map(|(c, v)| v * x[*c]).sum();
        }
    };
    let at_mul = |r: &[f64], out: &mut [f64]| {
        out.iter_mut().for_each(|x| *x = 0.0);
        for (i, row) in rows.iter().enumerate() {
            for (c, v) in row {
                out[*c] += v * r[i];
            }
        }
    };
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut y = vec![0.0; m];
    let mut r = b.clone();
    let mut s = vec![0.0; m];
    at_mul(&r, &mut s);
    let mut p = s.clone();
    let mut gamma = dot(&s, &s);
    let g0 = gamma;
    let mut q = vec![0.0; rows.len()];
    let maxit = (4 * m + 200).min(30000);
    if g0 > 0.0 {
        for _ in 0..maxit {
            a_mul(&p, &mut q);
            let qq = dot(&q, &q);
            if qq <= 0.0 {
                break;
            }
            let alpha = gamma / qq;
            for i in 0..m {
                y[i] += alpha * p[i];
            }
            for i in 0..r.len() {
                r[i] -= alpha * q[i];
            }
            at_mul(&r, &mut s);
            let g2 = dot(&s, &s);
            if g2 <= 1e-26 * g0 {
                break;
            }
            let beta = g2 / gamma;
            for i in 0..m {
                p[i] = s[i] + beta * p[i];
            }
            gamma = g2;
        }
    }
    let mut out = vec![[0.0, 0.0]; n];
    for v in 0..n {
        if let Some(p) = fval[v] {
            out[v] = p;
        } else if fidx[v] != usize::MAX {
            let i = fidx[v];
            out[v] = [y[i] * d[i], y[nfree + i] * d[nfree + i]];
        }
    }
    out.iter().all(|p| p[0].is_finite() && p[1].is_finite()).then_some(out)
}

/// Public, for tests and callers that have a plain triangle mesh: flatten a disc with LSCM. The result is in the units of
/// `pts` and may be mirrored; None if it is not a disc or the solve fails.
pub fn lscm(pts: &[DVec3], tris: &[[usize; 3]]) -> Option<Vec<[f64; 2]>> {
    if !is_disc(pts.len(), tris) {
        return None;
    }
    let mut uv = lscm_mesh_pins(pts, tris, &[])?;
    // face the same way as the triangles
    if signed_area(tris, &uv) < 0.0 {
        uv.iter_mut().for_each(|p| p[1] = -p[1]);
    }
    Some(uv)
}

fn signed_area(tris: &[[usize; 3]], uv: &[[f64; 2]]) -> f64 {
    tris.iter().map(|t| cross2(uv[t[0]], uv[t[1]], uv[t[2]]) * 0.5).sum()
}

fn cross2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// (no triangle folded and no boundary crossing itself, stretch within bounds). `pinned`: the stretch is not judged
/// (the person chose the scale).
fn valid(pos: &[DVec3], tris: &[[usize; 3]], uv: &[[f64; 2]], pinned: bool) -> (bool, bool) {
    let mut uv = uv.to_vec();
    if signed_area(tris, &uv) < 0.0 {
        uv.iter_mut().for_each(|p| p[1] = -p[1]);
    }
    let mut a3 = 0.0;
    let mut a2 = 0.0;
    for t in tris {
        a3 += tri_area(pos[t[0]], pos[t[1]], pos[t[2]]);
        a2 += cross2(uv[t[0]], uv[t[1]], uv[t[2]]).abs() * 0.5;
    }
    if a3 <= 0.0 || a2 <= 0.0 {
        return (false, false);
    }
    let (mut wsum, mut ssum) = (0.0, 0.0);
    for t in tris {
        let t3 = tri_area(pos[t[0]], pos[t[1]], pos[t[2]]);
        let t2 = cross2(uv[t[0]], uv[t[1]], uv[t[2]]) * 0.5;
        if t3 < 1e-12 * a3 {
            continue;
        }
        if t2 <= 1e-9 * a2 {
            return (false, false);
        }
        let r = (t2 / a2) / (t3 / a3);
        ssum += r.max(1.0 / r) * t3;
        wsum += t3;
    }
    let stretch_ok = pinned || ssum / wsum.max(1e-30) <= MAX_STRETCH;
    (!boundary_crosses(tris, &uv), stretch_ok)
}

/// Does the outline of the flattened island cross itself?
fn boundary_crosses(tris: &[[usize; 3]], uv: &[[f64; 2]]) -> bool {
    let mut cnt: HashMap<(usize, usize), u32> = HashMap::new();
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *cnt.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut segs: Vec<(usize, usize)> = cnt.into_iter().filter(|(_, c)| *c == 1).map(|(k, _)| k).collect();
    segs.sort_unstable();
    if segs.len() < 4 {
        return false;
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for (a, b) in &segs {
        for p in [uv[*a], uv[*b]] {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    let cells = ((segs.len() as f64).sqrt().ceil() as usize).clamp(1, 256);
    let size = [((hi[0] - lo[0]) / cells as f64).max(1e-12), ((hi[1] - lo[1]) / cells as f64).max(1e-12)];
    let cell = |p: [f64; 2]| (((p[0] - lo[0]) / size[0]) as usize).min(cells - 1) * cells + (((p[1] - lo[1]) / size[1]) as usize).min(cells - 1);
    let mut grid: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        let (c0, c1) = (cell(uv[*a]), cell(uv[*b]));
        let (x0, y0, x1, y1) = (c0 / cells, c0 % cells, c1 / cells, c1 % cells);
        for x in x0.min(x1)..=x0.max(x1) {
            for y in y0.min(y1)..=y0.max(y1) {
                grid.entry(x * cells + y).or_default().push(i);
            }
        }
    }
    let cross = |s: (usize, usize), t: (usize, usize)| -> bool {
        if s.0 == t.0 || s.0 == t.1 || s.1 == t.0 || s.1 == t.1 {
            return false;
        }
        let (a, b, c, d) = (uv[s.0], uv[s.1], uv[t.0], uv[t.1]);
        let (d1, d2, d3, d4) = (cross2(a, b, c), cross2(a, b, d), cross2(c, d, a), cross2(c, d, b));
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    for list in grid.values() {
        for (i, &x) in list.iter().enumerate() {
            for &y in &list[i + 1..] {
                if cross(segs[x], segs[y]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Turn the points so their bounding box is the smallest (at 3 degree steps) and its long side runs along u.
fn align_min_box(uv: &mut [[f64; 2]]) {
    let mut best = (f64::MAX, 0.0);
    for k in 0..30 {
        let a = (k as f64 * 3.0).to_radians();
        let (s, c) = a.sin_cos();
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in uv.iter() {
            let q = [p[0] * c + p[1] * s, -p[0] * s + p[1] * c];
            for i in 0..2 {
                lo[i] = lo[i].min(q[i]);
                hi[i] = hi[i].max(q[i]);
            }
        }
        let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
        if area < best.0 - 1e-12 {
            best = (area, a);
        }
    }
    let (s, c) = best.1.sin_cos();
    let mut w = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for p in uv.iter_mut() {
        *p = [p[0] * c + p[1] * s, -p[0] * s + p[1] * c];
        w = [w[0].min(p[0]), w[1].max(p[0]), w[2].min(p[1]), w[3].max(p[1])];
    }
    if w[3] - w[2] > w[1] - w[0] {
        for p in uv.iter_mut() {
            *p = [p[1], -p[0]];
        }
    }
}

// ---- packing ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w - 1e-12 && o.x < self.x + self.w - 1e-12 && self.y < o.y + o.h - 1e-12 && o.y < self.y + self.h - 1e-12
    }
    fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x - 1e-12 && o.y >= self.y - 1e-12 && o.x + o.w <= self.x + self.w + 1e-12 && o.y + o.h <= self.y + self.h + 1e-12
    }
}

/// An item to place: its size (before the scale and margin are added) and whether it may be turned a quarter.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub w: f64,
    pub h: f64,
    pub rotatable: bool,
}

fn cut_free(free: &mut Vec<Rect>, used: &Rect) {
    let mut next: Vec<Rect> = Vec::with_capacity(free.len() + 4);
    for f in free.iter() {
        if !f.overlaps(used) {
            next.push(*f);
            continue;
        }
        if used.x > f.x {
            next.push(Rect { x: f.x, y: f.y, w: used.x - f.x, h: f.h });
        }
        if used.x + used.w < f.x + f.w {
            next.push(Rect { x: used.x + used.w, y: f.y, w: f.x + f.w - used.x - used.w, h: f.h });
        }
        if used.y > f.y {
            next.push(Rect { x: f.x, y: f.y, w: f.w, h: used.y - f.y });
        }
        if used.y + used.h < f.y + f.h {
            next.push(Rect { x: f.x, y: used.y + used.h, w: f.w, h: f.y + f.h - used.y - used.h });
        }
    }
    // drop rectangles that sit inside another
    let mut keep = vec![true; next.len()];
    for i in 0..next.len() {
        for j in 0..next.len() {
            if i != j && keep[j] && next[j].contains(&next[i]) && (next[i] != next[j] || j < i) {
                keep[i] = false;
                break;
            }
        }
    }
    *free = next.into_iter().zip(keep).filter(|(_, k)| *k).map(|(r, _)| r).collect();
}

/// Place `items` (sizes times `scale`, plus `margin` all round) in the unit square, around `blocked` rectangles. The
/// placements come back in item order: the rectangle (without the margin) and whether the item was turned.
pub fn pack_at(items: &[Item], scale: f64, margin: f64, blocked: &[Rect]) -> Option<Vec<(Rect, bool)>> {
    let mut free = vec![Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }];
    for b in blocked {
        cut_free(&mut free, &Rect { x: b.x - margin * 0.5, y: b.y - margin * 0.5, w: b.w + margin, h: b.h + margin });
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| (items[*b].w.max(items[*b].h)).partial_cmp(&items[*a].w.max(items[*a].h)).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b)));
    let mut out = vec![(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, false); items.len()];
    for i in order {
        let it = items[i];
        let mut best: Option<(f64, f64, Rect, bool)> = None;
        for rot in [false, true] {
            if rot && !it.rotatable {
                continue;
            }
            let (w, h) = if rot { (it.h * scale, it.w * scale) } else { (it.w * scale, it.h * scale) };
            let (pw, ph) = (w + margin, h + margin);
            for f in &free {
                if pw <= f.w + 1e-12 && ph <= f.h + 1e-12 {
                    let (lw, lh) = (f.w - pw, f.h - ph);
                    let (short, long) = (lw.min(lh), lw.max(lh));
                    if best.as_ref().is_none_or(|b| short < b.0 - 1e-12 || ((short - b.0).abs() <= 1e-12 && long < b.1)) {
                        best = Some((short, long, Rect { x: f.x, y: f.y, w: pw, h: ph }, rot));
                    }
                }
            }
        }
        let (_, _, r, rot) = best?;
        cut_free(&mut free, &r);
        out[i] = (Rect { x: r.x + margin * 0.5, y: r.y + margin * 0.5, w: r.w - margin, h: r.h - margin }, rot);
    }
    Some(out)
}

/// Refuse a pack of more islands than `MAX_PACK_ISLANDS`, before the work.
fn too_many_islands(n: usize) -> Result<()> {
    if n > MAX_PACK_ISLANDS {
        bail!("that is {n} UV islands to lay out at once (the most is {MAX_PACK_ISLANDS}): unwrap fewer faces at a time, or mark fewer seams so the islands are bigger");
    }
    Ok(())
}

/// (scale, margin used, placements)
pub type Packed = (f64, f64, Vec<(Rect, bool)>);

/// Past this many islands (pinned ones included) the packer lays them on shelves: rows, tallest first, which costs
/// O(n log n) per try. The free-rectangle packer (`pack_at`) fits a little tighter, but its cost grows much faster with
/// the count.
pub const SHELF_ABOVE: usize = 300;
/// The most islands one unwrap or pack places, pinned ones included.
pub const MAX_PACK_ISLANDS: usize = 50_000;

/// Cells on a side of the grid that finds blocked rectangles near a spot.
const GRID: usize = 64;
/// Cells on a side of the coarser grid that holds the blocked rectangles too big for the fine one.
const COARSE: usize = 8;
/// The most fine cells one blocked rectangle is listed in; bigger ones go in the coarse grid.
const FINE_CELLS: usize = 64;
/// How many blocked rectangles one pack may look at in all; past it the pack gives up (does not fit).
const BLOCK_LOOKS: u64 = 1 << 26;

/// Blocked rectangles and grids that find those near a spot. The grids are made once, at the widest margin; each try
/// sets its own (no wider) margin, so a rectangle's cells there are among the ones it is listed in.
struct Blocks {
    raw: Vec<Rect>,
    margin: f64,
    cells: Vec<Vec<u32>>,
    coarse: Vec<Vec<u32>>,
    looks: std::cell::Cell<u64>,
}

impl Blocks {
    fn new(blocked: &[Rect], margin: f64) -> Blocks {
        // the same rectangle twice blocks no more than once
        let mut raw: Vec<Rect> = blocked.to_vec();
        raw.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.w.total_cmp(&b.w)).then(a.h.total_cmp(&b.h)));
        raw.dedup_by(|a, b| a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits() && a.w.to_bits() == b.w.to_bits() && a.h.to_bits() == b.h.to_bits());
        let mut b = Blocks { raw, margin, cells: vec![vec![]; GRID * GRID], coarse: vec![vec![]; COARSE * COARSE], looks: Default::default() };
        for i in 0..b.raw.len() {
            if let Some((x0, x1, y0, y1)) = Blocks::span(&b.rect(i), GRID) {
                if (x1 - x0 + 1) * (y1 - y0 + 1) <= FINE_CELLS {
                    for cy in y0..=y1 {
                        for cx in x0..=x1 {
                            b.cells[cy * GRID + cx].push(i as u32);
                        }
                    }
                } else if let Some((x0, x1, y0, y1)) = Blocks::span(&b.rect(i), COARSE) {
                    for cy in y0..=y1 {
                        for cx in x0..=x1 {
                            b.coarse[cy * COARSE + cx].push(i as u32);
                        }
                    }
                }
            }
        }
        b
    }

    /// Blocked rectangle `i` with the margin added.
    fn rect(&self, i: usize) -> Rect {
        let (b, m) = (self.raw[i], self.margin);
        Rect { x: b.x - m * 0.5, y: b.y - m * 0.5, w: b.w + m, h: b.h + m }
    }

    /// Whether one blocked rectangle covers the whole sheet (even without a margin), so nothing can go on it.
    fn covers_the_sheet(&self) -> bool {
        self.raw.iter().any(|b| b.x <= 0.0 && b.y <= 0.0 && b.x + b.w >= 1.0 && b.y + b.h >= 1.0)
    }

    /// Whether this pack has looked at as many blocked rectangles as it may.
    fn spent(&self) -> bool {
        self.looks.get() > BLOCK_LOOKS
    }

    /// The cells of an `n` x `n` grid a rectangle touches (clipped to the unit square); None when it is outside it.
    fn span(r: &Rect, n: usize) -> Option<(usize, usize, usize, usize)> {
        if !(r.x < 1.0 && r.y < 1.0 && r.x + r.w > 0.0 && r.y + r.h > 0.0) {
            return None;
        }
        let c = |v: f64| (v * n as f64).floor().clamp(0.0, (n - 1) as f64) as usize;
        Some((c(r.x), c(r.x + r.w), c(r.y), c(r.y + r.h)))
    }

    fn near(&self, r: Rect) -> impl Iterator<Item = Rect> + '_ {
        let fine = Blocks::span(&r, GRID).unwrap_or((1, 0, 1, 0));
        let coarse = Blocks::span(&r, COARSE).unwrap_or((1, 0, 1, 0));
        let (x0, x1, y0, y1) = fine;
        let a = (y0..=y1).flat_map(move |cy| (x0..=x1).flat_map(move |cx| self.cells[cy * GRID + cx].iter()));
        let (x0, x1, y0, y1) = coarse;
        let b = (y0..=y1).flat_map(move |cy| (x0..=x1).flat_map(move |cx| self.coarse[cy * COARSE + cx].iter()));
        a.chain(b)
            .map(|&i| {
                self.looks.set(self.looks.get() + 1);
                self.rect(i as usize)
            })
            .filter(move |b| Blocks::span(b, GRID).is_some() && b.overlaps(&r))
    }

    /// The furthest right edge of the blocked rectangles `r` overlaps, if it overlaps any.
    fn hit(&self, r: &Rect) -> Option<f64> {
        self.near(*r).map(|b| b.x + b.w).reduce(f64::max)
    }

    /// The lowest top edge among the blocked rectangles that reach into the band `y..y + h`.
    fn next_top(&self, y: f64, h: f64) -> Option<f64> {
        self.near(Rect { x: 0.0, y, w: 1.0, h }).map(|b| b.y + b.h).filter(|t| *t > y).reduce(f64::min)
    }
}

/// The order the shelf packer takes the items in: rotatable ones lying down (long side along u), tallest first.
fn shelf_order(items: &[Item]) -> (Vec<usize>, Vec<(f64, f64, bool)>) {
    let dims: Vec<(f64, f64, bool)> = items.iter().map(|it| if it.rotatable && it.h > it.w { (it.h, it.w, true) } else { (it.w, it.h, false) }).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| dims[*b].1.total_cmp(&dims[*a].1).then(dims[*b].0.total_cmp(&dims[*a].0)).then(a.cmp(b)));
    (order, dims)
}

/// `pack_at` on shelves: the items in `order` left to right along a row as tall as its first, around the blocked
/// rectangles, then the next row above. The placements come back in item order, as from `pack_at`.
fn shelf_at(order: &[usize], dims: &[(f64, f64, bool)], scale: f64, margin: f64, blocks: &Blocks) -> Option<Vec<(Rect, bool)>> {
    let mut out = vec![(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, false); dims.len()];
    let (mut x, mut y, mut shelf) = (0.0f64, 0.0f64, 0.0f64);
    for &i in order {
        let (w, h, rot) = dims[i];
        let (pw, ph) = (w * scale + margin, h * scale + margin);
        if pw > 1.0 + 1e-12 || ph > 1.0 + 1e-12 {
            return None;
        }
        loop {
            if blocks.spent() {
                return None;
            }
            if x + pw > 1.0 + 1e-12 {
                // the next row: above the tallest on this one, or (an empty row the blocked rectangles filled) above them
                y = if shelf > 0.0 { y + shelf } else { blocks.next_top(y, ph)? };
                x = 0.0;
                shelf = 0.0;
            }
            if y + ph > 1.0 + 1e-12 {
                return None;
            }
            let r = Rect { x, y, w: pw, h: ph };
            match blocks.hit(&r) {
                // past what is in the way (always further right: it overlaps)
                Some(right) => x = right,
                None => {
                    out[i] = (Rect { x: r.x + margin * 0.5, y: r.y + margin * 0.5, w: r.w - margin, h: r.h - margin }, rot);
                    x += pw;
                    shelf = shelf.max(ph);
                    break;
                }
            }
        }
    }
    Some(out)
}

/// The biggest scale at which the items fit, with the placements. The margin shrinks when the sheet is too crowded for it.
pub fn pack(items: &[Item], margin: f64, blocked: &[Rect]) -> Option<Packed> {
    if items.is_empty() {
        return Some((1.0, margin, vec![]));
    }
    let shelves = (items.len() + blocked.len() > SHELF_ABOVE).then(|| shelf_order(items));
    let mut margin = margin.clamp(0.0, 0.2);
    let mut blocks = shelves.as_ref().map(|_| Blocks::new(blocked, margin));
    if blocks.as_ref().is_some_and(|b| b.covers_the_sheet()) {
        return None;
    }
    for _ in 0..8 {
        if let Some(b) = blocks.as_mut() {
            b.margin = margin;
        }
        if blocks.as_ref().is_some_and(|b| b.spent()) {
            return None;
        }
        let try_at = |scale: f64| match (&shelves, &blocks) {
            (Some((order, dims)), Some(b)) => shelf_at(order, dims, scale, margin, b),
            _ => pack_at(items, scale, margin, blocked),
        };
        let top = items.iter().map(|i| i.w.max(i.h)).fold(1e-12, f64::max);
        let (mut lo, mut hi) = (0.0, 1.0 / top);
        let mut best = try_at(0.0).map(|p| (0.0, p));
        if best.is_some() {
            for _ in 0..28 {
                let mid = (lo + hi) * 0.5;
                match try_at(mid) {
                    Some(p) => {
                        lo = mid;
                        best = Some((mid, p));
                    }
                    None => hi = mid,
                }
            }
            if let Some((s, p)) = best {
                if s > 0.0 {
                    return Some((s, margin, p));
                }
            }
        }
        margin *= 0.5;
    }
    None
}

// ---- the Model methods -----------------------------------------------------------------------------------------------

type Corners = Vec<[f64; 2]>;
/// a face's use of an edge: the face and the coordinates at the edge's two ends (lower vertex id first)
type FaceEnds = (usize, [[f64; 2]; 2]);

/// The corners as the face keeps them (f32 per loop); refused when one isn't a number or is out of range.
fn to_face_uv(f: &crate::model::Face, c: &Corners) -> Result<FaceUv> {
    let mut off = f.outer.len();
    let outer = uv32_ring(&c[..off])?;
    let mut holes = vec![];
    for h in &f.holes {
        holes.push(uv32_ring(&c[off..off + h.len()])?);
        off += h.len();
    }
    Ok(FaceUv { outer, holes })
}

fn corners_of(u: &FaceUv) -> Corners {
    u.outer.iter().chain(u.holes.iter().flatten()).map(|p| [p[0] as f64, p[1] as f64]).collect()
}

impl Model {
    fn uv_of(&self, id: Id, set: u8) -> Option<&FaceUv> {
        let f = self.faces.get(&id)?;
        let u = if set == 2 { f.uv2.as_ref() } else { f.uv.as_ref() };
        u.filter(|u| u.fits(f))
    }

    fn set_uv(&mut self, id: Id, set: u8, uv: Option<FaceUv>) {
        let f = self.faces.get_mut(&id).unwrap();
        if set == 2 {
            f.uv2 = uv;
        } else {
            f.uv = uv;
        }
    }

    /// The faces named, expanding groups, sorted and without repeats; refuses locked ones.
    pub fn uv_faces(&self, ids: &[Id]) -> Result<Vec<Id>> {
        self.check_unlocked(ids)?;
        let mut f = self.faces_of(ids)?;
        f.sort_unstable();
        f.dedup();
        if f.is_empty() {
            bail!("select some faces (or a group) first");
        }
        Ok(f)
    }

    /// Mark or clear seams on edges. Returns how many changed.
    pub fn set_seams(&mut self, edges: &[Id], on: bool) -> Result<usize> {
        self.check_unlocked(edges)?;
        for e in edges {
            if !self.edges.contains_key(e) {
                bail!("{e} is not an edge");
            }
        }
        let mut n = 0;
        for e in edges {
            let ed = self.edges.get_mut(e).unwrap();
            n += (ed.seam != on) as usize;
            ed.seam = on;
        }
        Ok(n)
    }

    /// The edges inside `faces` (shared by two of them) whose faces meet at more than `angle` degrees.
    pub fn hard_edges(&self, faces: &[Id], angle: f64) -> Result<Vec<Id>> {
        let faces = self.uv_faces(faces)?;
        let soup = Soup::new(self, &faces);
        let mut out: Vec<Id> = vec![];
        let by_key: HashMap<(Id, Id), Id> = self.edges.iter().map(|(&id, e)| ((e.a.min(e.b), e.a.max(e.b)), id)).collect();
        for l in &soup.links {
            if l.angle > angle {
                let a = soup.faces[l.f].verts[l.fc[0]];
                let b = soup.faces[l.f].verts[l.fc[1]];
                if let Some(e) = by_key.get(&(a.min(b), a.max(b))) {
                    out.push(*e);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// Unwrap `ids` (faces or groups): islands, flatten, pack. One call, one result; nothing changes if it is refused.
    pub fn unwrap_uv(&mut self, ids: &[Id], opts: &Opts) -> Result<Report> {
        let faces = self.uv_faces(ids)?;
        if !opts.margin.is_finite() || !(0.0..=0.2).contains(&opts.margin) {
            bail!("margin must be between 0 and 0.2");
        }
        if !opts.angle.is_finite() || opts.angle <= 0.0 || opts.angle >= 180.0 {
            bail!("angle must be between 0 and 180 degrees");
        }
        let soup = Soup::new(self, &faces);
        let auto = opts.seams == SeamMode::Auto || opts.method == Method::Angle;
        let angle = opts.angle;
        let cut = move |l: &Link| l.seam || (auto && l.angle > angle);
        let index: HashMap<Id, usize> = soup.faces.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
        let mut pins: HashMap<usize, Vec<(usize, [f64; 2])>> = HashMap::new();
        for p in &opts.pins {
            let Some(&fi) = index.get(&p.face) else { bail!("pin: face {} is not in the selection", p.face) };
            if p.corner >= soup.faces[fi].verts.len() || !p.uv[0].is_finite() || !p.uv[1].is_finite() {
                bail!("pin: face {} has no corner {} (or the place isn't a number)", p.face, p.corner);
            }
            pins.entry(fi).or_default().push((p.corner, p.uv));
        }
        let all: Vec<usize> = (0..soup.faces.len()).collect();
        let comps = soup.components(&all, &cut);
        too_many_islands(comps.len())?;
        let mut ctx = Ctx { soup: &soup, opts, cut: &cut, pins, out: vec![], split: 0, ignored: 0, pinned: 0 };
        for c in comps {
            ctx.solve(c, 0.0);
        }
        let (mut pieces, split, ignored, pinned) = (std::mem::take(&mut ctx.out), ctx.split, ctx.ignored, ctx.pinned);
        too_many_islands(pieces.len())?;
        let stats = finish_pieces(&soup, &mut pieces);
        // pinned islands keep their place; the others are packed around them
        let blocked: Vec<Rect> = pieces.iter().filter(|p| p.fixed).map(|p| bbox(&p.faces)).collect();
        let items: Vec<Item> = pieces.iter().filter(|p| !p.fixed).map(|p| {
            let b = bbox(&p.faces);
            Item { w: b.w.max(1e-9), h: b.h.max(1e-9), rotatable: p.rotatable }
        }).collect();
        let Some((scale, _, place)) = pack(&items, opts.margin, &blocked) else {
            bail!("the islands do not fit the sheet: use a smaller margin or fewer pieces");
        };
        let mut k = 0;
        let mut result: Vec<(Id, Corners)> = vec![];
        for p in &pieces {
            if p.fixed {
                for (f, c) in &p.faces {
                    result.push((soup.faces[*f].id, c.clone()));
                }
                continue;
            }
            let b = bbox(&p.faces);
            let (r, rot) = place[k];
            k += 1;
            for (f, c) in &p.faces {
                let pts = c.iter().map(|q| {
                    // to the box's corner, turned a quarter if the packer did, scaled, placed
                    let (x, y) = (q[0] - b.x, q[1] - b.y);
                    let (x, y) = if rot { (b.h - y, x) } else { (x, y) };
                    [r.x + x * scale, r.y + y * scale]
                });
                result.push((soup.faces[*f].id, pts.collect()));
            }
        }
        let n_islands = pieces.len();
        for (id, c) in result {
            let uv = to_face_uv(&self.faces[&id], &c)?;
            self.set_uv(id, opts.set, Some(uv));
        }
        Ok(Report { faces: faces.len(), islands: n_islands, split, pinned, ignored_pins: ignored, stretch: stats, scale })
    }

    /// The islands among faces that already have coordinates: faces joined across an edge whose corners agree, in order of
    /// the lowest face id; faces without coordinates are left out.
    pub fn uv_islands(&self, ids: &[Id], set: u8) -> Vec<Vec<Id>> {
        let faces: Vec<Id> = ids.iter().copied().filter(|f| self.uv_of(*f, set).is_some()).collect();
        let index: HashMap<Id, usize> = faces.iter().enumerate().map(|(i, f)| (*f, i)).collect();
        let mut uf: Vec<usize> = (0..faces.len()).collect();
        fn find(uf: &mut [usize], mut i: usize) -> usize {
            while uf[i] != i {
                uf[i] = uf[uf[i]];
                i = uf[i];
            }
            i
        }
        // edge -> (face, uv of each end)
        let mut ends: HashMap<(Id, Id), Vec<FaceEnds>> = HashMap::new();
        for (fi, &id) in faces.iter().enumerate() {
            let f = &self.faces[&id];
            let c = corners_of(self.uv_of(id, set).unwrap());
            let mut off = 0;
            for l in f.loops() {
                for k in 0..l.len() {
                    let (ia, ib) = (off + k, off + (k + 1) % l.len());
                    let (a, b) = (l[k], l[(k + 1) % l.len()]);
                    let (key, uvs) = if a < b { ((a, b), [c[ia], c[ib]]) } else { ((b, a), [c[ib], c[ia]]) };
                    ends.entry(key).or_default().push((fi, uvs));
                }
                off += l.len();
            }
        }
        let same = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5;
        for v in ends.values() {
            if v.len() == 2 && v[0].0 != v[1].0 && same(v[0].1[0], v[1].1[0]) && same(v[0].1[1], v[1].1[1]) {
                let (a, b) = (find(&mut uf, v[0].0), find(&mut uf, v[1].0));
                if a != b {
                    uf[a.max(b)] = a.min(b);
                }
            }
        }
        let _ = index;
        let mut groups: Vec<(usize, Vec<Id>)> = vec![];
        let mut at: HashMap<usize, usize> = HashMap::new();
        for (i, &id) in faces.iter().enumerate() {
            let r = find(&mut uf, i);
            let g = *at.entry(r).or_insert_with(|| {
                groups.push((i, vec![]));
                groups.len() - 1
            });
            groups[g].1.push(id);
        }
        groups.sort_by_key(|g| g.0);
        groups.into_iter().map(|g| g.1).collect()
    }

    /// Repack the islands of faces that already have coordinates, as they are (not turned), as big as the sheet lets them be.
    pub fn pack_uv(&mut self, ids: &[Id], margin: f64, set: u8) -> Result<usize> {
        let faces = self.uv_faces(ids)?;
        if !(0.0..=0.2).contains(&margin) {
            bail!("margin must be between 0 and 0.2");
        }
        let islands = self.uv_islands(&faces, set);
        if islands.is_empty() {
            bail!("these faces have no UVs yet: unwrap them first");
        }
        too_many_islands(islands.len())?;
        let boxes: Vec<Rect> = islands.iter().map(|isl| {
            let pts: Vec<[f64; 2]> = isl.iter().flat_map(|f| corners_of(self.uv_of(*f, set).unwrap())).collect();
            bbox_pts(&pts)
        }).collect();
        let items: Vec<Item> = boxes.iter().map(|b| Item { w: b.w.max(1e-9), h: b.h.max(1e-9), rotatable: false }).collect();
        let Some((scale, _, place)) = pack(&items, margin, &[]) else { bail!("the islands do not fit the sheet: use a smaller margin") };
        for (i, isl) in islands.iter().enumerate() {
            let (b, (r, _)) = (boxes[i], place[i]);
            for f in isl {
                let c: Corners = corners_of(self.uv_of(*f, set).unwrap()).iter().map(|p| [r.x + (p[0] - b.x) * scale, r.y + (p[1] - b.y) * scale]).collect();
                let uv = to_face_uv(&self.faces[f], &c)?;
                self.set_uv(*f, set, Some(uv));
            }
        }
        Ok(islands.len())
    }

    /// Set the corners of faces: per face every corner (outer loop, then each hole), or None to drop the coordinates.
    pub fn set_uv_corners(&mut self, uv: &[(Id, Option<Vec<[f64; 2]>>)], set: u8) -> Result<usize> {
        let ids: Vec<Id> = uv.iter().map(|(f, _)| *f).collect();
        self.check_unlocked(&ids)?;
        for (id, c) in uv {
            let Some(f) = self.faces.get(id) else { bail!("no face {id} here (open the component to edit its faces)") };
            if let Some(c) = c {
                let n = f.outer.len() + f.holes.iter().map(Vec::len).sum::<usize>();
                if c.len() != n || c.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
                    bail!("face {id} has {n} corners: give one [u, v] per corner, outer loop first");
                }
            }
        }
        for (id, c) in uv {
            let new = c.as_ref().map(|c| to_face_uv(&self.faces[id], c)).transpose()?;
            if set == 1 && new.is_none() {
                self.faces.get_mut(id).unwrap().tex = None;
            }
            self.set_uv(*id, set, new);
        }
        Ok(uv.len())
    }

    /// Move corners of faces by an affine map of the picture (tiles). `only`: just the corners at these model vertices.
    pub fn transform_uv(&mut self, ids: &[Id], only: Option<&[Id]>, m: [f64; 6], set: u8) -> Result<usize> {
        let faces = self.uv_faces(ids)?;
        if m.iter().any(|v| !v.is_finite()) || (m[0] * m[3] - m[1] * m[2]).abs() < 1e-12 {
            bail!("the transform squashes the picture flat (or isn't a number)");
        }
        let only: Option<HashSet<Id>> = only.map(|o| o.iter().copied().collect());
        let mut done = vec![];
        for id in faces {
            let Some(u) = self.uv_of(id, set) else { bail!("face {id} has no UVs yet: unwrap it first") };
            let f = &self.faces[&id];
            let verts: Vec<Id> = f.loops().flatten().copied().collect();
            let c: Corners = corners_of(u).iter().zip(&verts).map(|(p, v)| {
                if only.as_ref().is_none_or(|o| o.contains(v)) {
                    [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]]
                } else {
                    *p
                }
            }).collect();
            done.push((id, to_face_uv(f, &c)?));
        }
        let n = done.len();
        for (id, uv) in done {
            self.set_uv(id, set, Some(uv));
        }
        Ok(n)
    }

    /// What the UV view draws: per face its corners' model vertices, coordinates, the loop sizes and the island number.
    pub fn uv_info(&self, ids: &[Id], set: u8) -> Result<serde_json::Value> {
        let mut faces = self.faces_of(ids)?;
        faces.sort_unstable();
        faces.dedup();
        let islands = self.uv_islands(&faces, set);
        let mut island_of: HashMap<Id, usize> = HashMap::new();
        for (i, isl) in islands.iter().enumerate() {
            for f in isl {
                island_of.insert(*f, i);
            }
        }
        let mut out = vec![];
        let mut without = vec![];
        for id in &faces {
            let f = &self.faces[id];
            let Some(u) = self.uv_of(*id, set) else {
                without.push(*id);
                continue;
            };
            out.push(serde_json::json!({
                "id": id,
                "material": f.material,
                "verts": f.loops().flatten().collect::<Vec<_>>(),
                "loops": f.loops().map(Vec::len).collect::<Vec<_>>(),
                "uv": corners_of(u),
                "island": island_of[id],
            }));
        }
        let seams: Vec<Id> = self.edges.iter().filter(|(_, e)| e.seam).map(|(&id, _)| id).collect();
        Ok(serde_json::json!({"faces": out, "without": without, "islands": islands.len(), "seams": seams}))
    }

    /// Edges of `faces` that are marked seams.
    pub fn seam_edges_of(&self, faces: &[Id]) -> Vec<Id> {
        let verts: HashSet<(Id, Id)> = faces.iter().filter_map(|f| self.faces.get(f)).flat_map(|f| f.loops().flat_map(|l| (0..l.len()).map(move |k| (l[k].min(l[(k + 1) % l.len()]), l[k].max(l[(k + 1) % l.len()]))))).collect();
        let mut v: Vec<Id> = self.edges.iter().filter(|(_, e)| e.seam && verts.contains(&(e.a.min(e.b), e.a.max(e.b)))).map(|(&id, _)| id).collect();
        v.sort_unstable();
        v
    }
}

fn bbox(faces: &[(usize, Corners)]) -> Rect {
    let pts: Vec<[f64; 2]> = faces.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    bbox_pts(&pts)
}

fn bbox_pts(pts: &[[f64; 2]]) -> Rect {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if lo[0] > hi[0] {
        return Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
    }
    Rect { x: lo[0], y: lo[1], w: hi[0] - lo[0], h: hi[1] - lo[1] }
}

/// Put every free island at the density of the model (picture area = model area) and return the area-weighted mean
/// stretch over all of them.
fn finish_pieces(soup: &Soup, pieces: &mut [Piece]) -> f64 {
    let (mut wsum, mut ssum) = (0.0, 0.0);
    for p in pieces.iter_mut() {
        let (mut a3, mut a2) = (0.0, 0.0);
        for (f, c) in &p.faces {
            let fd = &soup.faces[*f];
            for t in &fd.tris {
                a3 += tri_area(fd.pts[t[0]], fd.pts[t[1]], fd.pts[t[2]]);
                a2 += cross2(c[t[0]], c[t[1]], c[t[2]]).abs() * 0.5;
            }
        }
        if p.fixed {
            continue;
        }
        let s = if a2 > 1e-18 && a3 > 1e-18 { (a3 / a2).sqrt() } else { 1.0 };
        for (_, c) in &mut p.faces {
            for q in c.iter_mut() {
                *q = [q[0] * s, q[1] * s];
            }
        }
        for (f, c) in &p.faces {
            let fd = &soup.faces[*f];
            for t in &fd.tris {
                let t3 = tri_area(fd.pts[t[0]], fd.pts[t[1]], fd.pts[t[2]]);
                let t2 = cross2(c[t[0]], c[t[1]], c[t[2]]).abs() * 0.5;
                if t3 > 1e-15 && t2 > 1e-15 {
                    let r = t2 / t3;
                    ssum += r.max(1.0 / r) * t3;
                    wsum += t3;
                }
            }
        }
    }
    if wsum > 0.0 { ssum / wsum } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(nx: usize, ny: usize, f: impl Fn(f64, f64) -> DVec3) -> (Vec<DVec3>, Vec<[usize; 3]>) {
        let mut pts = vec![];
        for j in 0..=ny {
            for i in 0..=nx {
                pts.push(f(i as f64 / nx as f64, j as f64 / ny as f64));
            }
        }
        let mut tris = vec![];
        for j in 0..ny {
            for i in 0..nx {
                let a = j * (nx + 1) + i;
                let (b, c, d) = (a + 1, a + nx + 1, a + nx + 2);
                tris.push([a, b, d]);
                tris.push([a, d, c]);
            }
        }
        (pts, tris)
    }

    fn edge_len_ratios(pts: &[DVec3], tris: &[[usize; 3]], uv: &[[f64; 2]]) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for t in tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let l3 = pts[a].distance(pts[b]);
                let l2 = ((uv[a][0] - uv[b][0]).powi(2) + (uv[a][1] - uv[b][1]).powi(2)).sqrt();
                lo = f64::min(lo, l2 / l3);
                hi = f64::max(hi, l2 / l3);
            }
        }
        (lo, hi)
    }

    #[test]
    fn a_flat_square_stays_undistorted() {
        let (pts, tris) = grid(6, 6, |u, v| DVec3::new(u * 2.0, v * 2.0, 0.0));
        let uv = lscm(&pts, &tris).unwrap();
        let (lo, hi) = edge_len_ratios(&pts, &tris, &uv);
        assert!((hi / lo - 1.0).abs() < 1e-6, "{lo} {hi}");
        assert!(signed_area(&tris, &uv) > 0.0);
        // a square stays a square: the box is as wide as it is high
        let b = bbox_pts(&uv);
        let rot_ok = (b.w - b.h).abs() < 1e-6 || {
            // turned 45 degrees its box is bigger, but the sides stay equal
            let d = ((uv[1][0] - uv[0][0]).powi(2) + (uv[1][1] - uv[0][1]).powi(2)).sqrt();
            (d * 6.0 - 2.0).abs() < 1e-6
        };
        assert!(rot_ok);
    }

    #[test]
    fn a_cylinder_strip_unrolls_to_a_rectangle() {
        let (r, h, turn) = (1.0, 2.0, 1.5 * std::f64::consts::PI);
        let (pts, tris) = grid(24, 4, |u, v| DVec3::new(r * (u * turn).cos(), r * (u * turn).sin(), v * h));
        let uv = lscm(&pts, &tris).unwrap();
        let (lo, hi) = edge_len_ratios(&pts, &tris, &uv);
        assert!(hi / lo < 1.01, "lengths are kept to a ratio of {}", hi / lo);
        // a rectangle of the arc length times the height
        let arc = r * turn;
        let want = [arc, h];
        let scale = lo;
        let (mut w, mut hh) = (0.0, 0.0);
        let nx = 24;
        // first row's length, and first column's
        for i in 0..nx {
            let (a, b) = (i, i + 1);
            w += ((uv[a][0] - uv[b][0]).powi(2) + (uv[a][1] - uv[b][1]).powi(2)).sqrt();
        }
        for j in 0..4 {
            let (a, b) = (j * (nx + 1), (j + 1) * (nx + 1));
            hh += ((uv[a][0] - uv[b][0]).powi(2) + (uv[a][1] - uv[b][1]).powi(2)).sqrt();
        }
        assert!((w / scale - want[0]).abs() < 0.03 * want[0] && (hh / scale - want[1]).abs() < 0.02 * want[1], "{} {}", w / scale, hh / scale);
        // the corners are right angles
        let (a, b, c) = (uv[0], uv[nx], uv[(nx + 1) * 4]);
        let dot = (b[0] - a[0]) * (c[0] - a[0]) + (b[1] - a[1]) * (c[1] - a[1]);
        let n = (((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)) * ((c[0] - a[0]).powi(2) + (c[1] - a[1]).powi(2))).sqrt();
        assert!((dot / n).abs() < 0.02, "{}", dot / n);
    }

    #[test]
    fn a_closed_or_pinched_surface_is_not_a_disc() {
        // a tetrahedron: closed
        let pts = vec![DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z];
        let tris = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]];
        assert!(!is_disc(4, &tris));
        assert!(lscm(&pts, &tris).is_none());
        // a ring (annulus) has two boundary loops
        let (_, tris) = grid(8, 1, |_, _| DVec3::ZERO);
        let mut ring = tris;
        // glue the first and last column
        for t in &mut ring {
            for v in t.iter_mut() {
                if *v % 9 == 8 {
                    *v -= 8;
                }
            }
        }
        ring.retain(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
        assert!(!is_disc(18, &ring));
        // a plain strip is
        let (_, strip) = grid(4, 1, |_, _| DVec3::ZERO);
        assert!(is_disc(10, &strip));
    }

    #[test]
    fn packing_stays_inside_without_overlaps_and_keeps_the_margin() {
        let items: Vec<Item> = (0..40).map(|i| Item { w: 0.3 + (i % 7) as f64 * 0.1, h: 0.2 + (i % 5) as f64 * 0.15, rotatable: i % 2 == 0 }).collect();
        let m = 0.01;
        let (scale, margin, place) = pack(&items, m, &[]).unwrap();
        assert!(scale > 0.0 && (margin - m).abs() < 1e-12);
        for (i, (r, rot)) in place.iter().enumerate() {
            assert!(r.x >= m * 0.5 - 1e-9 && r.y >= m * 0.5 - 1e-9 && r.x + r.w <= 1.0 - m * 0.5 + 1e-9 && r.y + r.h <= 1.0 - m * 0.5 + 1e-9, "{r:?} inside");
            let (w, h) = if *rot { (items[i].h, items[i].w) } else { (items[i].w, items[i].h) };
            assert!((r.w - w * scale).abs() < 1e-9 && (r.h - h * scale).abs() < 1e-9);
            if !items[i].rotatable {
                assert!(!rot);
            }
            for (j, (q, _)) in place.iter().enumerate().skip(i + 1) {
                let apart = r.x + r.w + m <= q.x + 1e-9 || q.x + q.w + m <= r.x + 1e-9 || r.y + r.h + m <= q.y + 1e-9 || q.y + q.h + m <= r.y + 1e-9;
                assert!(apart, "{i} and {j} are closer than the margin: {r:?} {q:?}");
            }
        }
        // and it uses the sheet well
        let used: f64 = place.iter().map(|(r, _)| r.w * r.h).sum();
        assert!(used > 0.35, "{used}");
    }

    #[test]
    fn packing_goes_around_blocked_rectangles() {
        let items = vec![Item { w: 1.0, h: 1.0, rotatable: false }; 3];
        let block = Rect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 };
        let (_, _, place) = pack(&items, 0.02, &[block]).unwrap();
        for (r, _) in &place {
            assert!(!r.overlaps(&block), "{r:?}");
        }
    }

    fn check_packing(items: &[Item], blocked: &[Rect], m: f64) -> f64 {
        let (scale, margin, place) = pack(items, m, blocked).unwrap();
        assert!(scale > 0.0 && margin <= m + 1e-12 && place.len() == items.len());
        let pad = |r: &Rect| Rect { x: r.x - margin * 0.5, y: r.y - margin * 0.5, w: r.w + margin, h: r.h + margin };
        let mut seen: Vec<Rect> = blocked.to_vec();
        for (i, (r, rot)) in place.iter().enumerate() {
            assert!(r.x >= margin * 0.5 - 1e-9 && r.y >= margin * 0.5 - 1e-9 && r.x + r.w <= 1.0 - margin * 0.5 + 1e-9 && r.y + r.h <= 1.0 - margin * 0.5 + 1e-9, "{r:?} inside");
            let (w, h) = if *rot { (items[i].h, items[i].w) } else { (items[i].w, items[i].h) };
            assert!((r.w - w * scale).abs() < 1e-9 && (r.h - h * scale).abs() < 1e-9);
            assert!(items[i].rotatable || !rot);
            seen.push(*r);
        }
        // margins apart from each other and from the blocked rectangles (a grid finds the neighbours: thousands of items)
        let n = seen.len();
        let cell = 1.0 / 32.0;
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, r) in seen.iter().enumerate() {
            let p = pad(r);
            for cy in (p.y / cell).floor() as i64..=((p.y + p.h) / cell).floor() as i64 {
                for cx in (p.x / cell).floor() as i64..=((p.x + p.w) / cell).floor() as i64 {
                    grid.entry((cx, cy)).or_default().push(i);
                }
            }
        }
        for v in grid.values() {
            for (k, &i) in v.iter().enumerate() {
                for &j in &v[k + 1..] {
                    if i < blocked.len() && j < blocked.len() {
                        continue;
                    }
                    assert!(!pad(&seen[i]).overlaps(&pad(&seen[j])), "{i} and {j} of {n} are closer than the margin");
                }
            }
        }
        place.iter().map(|(r, _)| r.w * r.h).sum()
    }

    #[test]
    fn many_islands_pack_on_shelves_quickly_inside_and_apart() {
        let items: Vec<Item> = (0..4000).map(|i| Item { w: 0.3 + (i % 7) as f64 * 0.1, h: 0.2 + (i % 5) as f64 * 0.15, rotatable: i % 2 == 0 }).collect();
        let t = std::time::Instant::now();
        let used = check_packing(&items, &[], 0.002);
        assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
        assert!(used > 0.35, "{used}");
        // around blocked rectangles too, and the same every time
        let blocked = [Rect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 }, Rect { x: 0.7, y: 0.6, w: 0.2, h: 0.3 }];
        check_packing(&items[..1000], &blocked, 0.01);
        assert_eq!(format!("{:?}", pack(&items[..1000], 0.01, &blocked)), format!("{:?}", pack(&items[..1000], 0.01, &blocked)));
        // a blocked band across the whole sheet is stepped over
        let band = [Rect { x: 0.0, y: 0.0, w: 1.0, h: 0.25 }];
        check_packing(&items[..500], &band, 0.0);
    }

    #[test]
    fn a_pack_of_too_many_islands_is_refused_before_the_work() {
        assert!(too_many_islands(MAX_PACK_ISLANDS).is_ok());
        let e = too_many_islands(MAX_PACK_ISLANDS + 1).unwrap_err().to_string();
        assert!(e.contains("islands"), "{e}");
    }

    #[test]
    fn a_crowded_sheet_shrinks_its_margin() {
        let items = vec![Item { w: 1.0, h: 1.0, rotatable: false }; 2000];
        let (scale, margin, place) = pack(&items, 0.1, &[]).unwrap();
        assert!(scale > 0.0 && margin < 0.1 && place.len() == 2000);
    }

    #[test]
    fn many_large_blocked_rectangles_are_answered_quickly() {
        let items: Vec<Item> = (0..400).map(|i| Item { w: 0.3 + (i % 7) as f64 * 0.1, h: 0.2 + (i % 5) as f64 * 0.15, rotatable: i % 2 == 0 }).collect();
        // the whole sheet, many times: nothing fits
        let full = vec![Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }; 40_000];
        let t = std::time::Instant::now();
        assert!(pack(&items, 0.0, &full).is_none());
        assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
        // many different big ones over most of it: an answer either way, and the same every time
        let big: Vec<Rect> = (0..40_000).map(|i| Rect { x: 0.0, y: 0.0, w: 0.6 + i as f64 * 1e-6, h: 0.9 - i as f64 * 1e-6 }).collect();
        let t = std::time::Instant::now();
        let first = format!("{:?}", pack(&items, 0.0, &big));
        assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
        assert_eq!(first, format!("{:?}", pack(&items, 0.0, &big)));
        // a few big ones still pack around them
        check_packing(&items, &big[..20], 0.0);
    }

    #[test]
    fn the_plane_frame_reads_left_to_right_seen_from_outside() {
        for n in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z, DVec3::new(1.0, 2.0, 3.0).normalize()] {
            let (u, v) = plane_frame(n);
            assert!((u.cross(v) - n).length() < 1e-9 && u.dot(n).abs() < 1e-9 && v.dot(n).abs() < 1e-9);
        }
        // on a wall facing -y the picture's u runs along +x and v up
        let (u, v) = plane_frame(DVec3::NEG_Y);
        assert!((u - DVec3::X).length() < 1e-9 && (v - DVec3::Z).length() < 1e-9);
    }
}
