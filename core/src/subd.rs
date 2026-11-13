//! Subdivision surfaces (Catmull-Clark): model a coarse cage of quads (and
//! any n-gons), and get a smooth surface, the way Blender's Subdivision
//! modifier works. Edges marked as creases stay sharp; open borders keep
//! their outline. Each cage face's material carries to the faces it becomes,
//! so glass painted on the cage is smooth glass on the result.

use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct Cage {
    pub points: Vec<DVec3>,
    pub faces: Vec<Vec<usize>>,
    /// per face (None: unpainted)
    pub materials: Vec<Option<String>>,
    /// sharp edges (vertex index pairs, either order)
    pub creases: HashSet<(usize, usize)>,
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// One Catmull-Clark step: every face becomes quads (one per corner).
pub fn step(c: &Cage) -> Cage {
    let n = c.points.len();
    // face points
    let fp: Vec<DVec3> = c.faces.iter().map(|f| f.iter().map(|&i| c.points[i]).sum::<DVec3>() / f.len() as f64).collect();
    // edges and the faces on them
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (fi, f) in c.faces.iter().enumerate() {
        for k in 0..f.len() {
            edge_faces.entry(key(f[k], f[(k + 1) % f.len()])).or_default().push(fi);
        }
    }
    let sharp = |e: &(usize, usize), fs: &Vec<usize>| fs.len() != 2 || c.creases.contains(e);
    // edge points
    let mut ep: HashMap<(usize, usize), usize> = HashMap::new();
    let mut points: Vec<DVec3> = Vec::with_capacity(n + edge_faces.len() + c.faces.len());
    // new positions for the old points come first (same indexes)
    let mut sum_f: Vec<DVec3> = vec![DVec3::ZERO; n];
    let mut cnt_f: Vec<usize> = vec![0; n];
    for (fi, f) in c.faces.iter().enumerate() {
        for &i in f {
            sum_f[i] += fp[fi];
            cnt_f[i] += 1;
        }
    }
    let mut mid_sum: Vec<DVec3> = vec![DVec3::ZERO; n];
    let mut valence: Vec<usize> = vec![0; n];
    let mut sharp_nb: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (e, fs) in &edge_faces {
        let mid = (c.points[e.0] + c.points[e.1]) / 2.0;
        for v in [e.0, e.1] {
            mid_sum[v] += mid;
            valence[v] += 1;
        }
        if sharp(e, fs) {
            sharp_nb[e.0].push(e.1);
            sharp_nb[e.1].push(e.0);
        }
    }
    for i in 0..n {
        let p = c.points[i];
        let v = match sharp_nb[i].len() {
            // a corner (3+ sharp edges) stays put
            k if k > 2 => p,
            // on a crease or a border: only the crease neighbors pull
            2 => (p * 6.0 + c.points[sharp_nb[i][0]] + c.points[sharp_nb[i][1]]) / 8.0,
            _ if valence[i] == 0 || cnt_f[i] == 0 => p,
            _ => {
                let nv = valence[i] as f64;
                let f = sum_f[i] / cnt_f[i] as f64;
                let r = mid_sum[i] / nv;
                (f + r * 2.0 + p * (nv - 3.0)) / nv
            }
        };
        points.push(v);
    }
    for (e, fs) in &edge_faces {
        let mid = (c.points[e.0] + c.points[e.1]) / 2.0;
        let p = if sharp(e, fs) { mid } else { (c.points[e.0] + c.points[e.1] + fp[fs[0]] + fp[fs[1]]) / 4.0 };
        ep.insert(*e, points.len());
        points.push(p);
    }
    let base = points.len();
    points.extend(fp.iter().copied());
    // new faces: one quad per corner of every old face
    let mut faces = Vec::with_capacity(c.faces.len() * 4);
    let mut materials = Vec::with_capacity(c.faces.len() * 4);
    for (fi, f) in c.faces.iter().enumerate() {
        let m = f.len();
        for k in 0..m {
            let (prev, cur, next) = (f[(k + m - 1) % m], f[k], f[(k + 1) % m]);
            faces.push(vec![cur, ep[&key(cur, next)], base + fi, ep[&key(prev, cur)]]);
            materials.push(c.materials[fi].clone());
        }
    }
    // a crease splits in two and stays a crease
    let mut creases = HashSet::new();
    for e in &c.creases {
        if let Some(&mid) = ep.get(&key(e.0, e.1)) {
            creases.insert(key(e.0, mid));
            creases.insert(key(mid, e.1));
        }
    }
    Cage { points, faces, materials, creases }
}

/// Subdivide `levels` times and build the result as a group of soft-edged
/// triangles. Returns the group.
pub fn build(m: &mut Model, cage: Cage, levels: usize, name: &str) -> Result<Id> {
    let g = m.begin_group(name.to_string());
    fill(m, cage, levels, g)?;
    m.active_group = m.groups.get(&g).and_then(|x| x.parent);
    Ok(g)
}

/// Replace a group's own geometry with the subdivided cage.
pub fn fill(m: &mut Model, mut cage: Cage, levels: usize, g: Id) -> Result<()> {
    if cage.faces.is_empty() {
        bail!("the cage has no faces");
    }
    for f in &cage.faces {
        if f.len() < 3 || f.iter().any(|&i| i >= cage.points.len()) {
            bail!("each cage face needs 3+ corners, all in the point list");
        }
    }
    let levels = levels.clamp(0, 5);
    for _ in 0..levels {
        cage = step(&cage);
        if cage.faces.len() > 400_000 {
            bail!("that's over 400,000 faces: use fewer levels or a lighter cage");
        }
    }
    // the group's own geometry goes (groups and parts nested in it stay)
    m.faces.retain(|_, f| f.group != Some(g));
    m.edges.retain(|_, e| e.group != Some(g));
    m.vertices.retain(|_, v| v.group != Some(g));
    let mut mesh = crate::mesh::Mesh::new(m, Some(g));
    for (f, mat) in cage.faces.iter().zip(&cage.materials) {
        let p: Vec<DVec3> = f.iter().map(|&i| cage.points[i]).collect();
        // two triangles per quad (always flat), split along the shorter diagonal
        if p.len() == 4 {
            let (a, b) = if p[0].distance(p[2]) <= p[1].distance(p[3]) { ([p[0], p[1], p[2]], [p[0], p[2], p[3]]) } else { ([p[0], p[1], p[3]], [p[1], p[2], p[3]]) };
            mesh.face(&a, &[], mat.clone());
            mesh.face(&b, &[], mat.clone());
        } else {
            let c = p.iter().copied().sum::<DVec3>() / p.len() as f64;
            for k in 0..p.len() {
                mesh.face(&[p[k], p[(k + 1) % p.len()], c], &[], mat.clone());
            }
        }
    }
    mesh.finish();
    Ok(())
}

// ---- editable cages: kept with the model, edited, rebuilt -----------------------------------

/// A mirror plane: points mirror across `axis` (0 x, 1 y, 2 z) at `center`.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Symmetry {
    pub axis: usize,
    pub center: f64,
}

/// A subdivision object's cage, as stored in the model (`Model::cages`, by group).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Editable {
    pub points: Vec<[f64; 3]>,
    pub faces: Vec<Vec<usize>>,
    #[serde(default)]
    pub materials: Vec<Option<String>>,
    #[serde(default)]
    pub creases: Vec<[usize; 2]>,
    pub levels: usize,
    #[serde(default)]
    pub symmetry: Option<Symmetry>,
}

impl Editable {
    pub fn cage(&self) -> Cage {
        Cage {
            points: self.points.iter().map(|p| DVec3::from(*p)).collect(),
            faces: self.faces.clone(),
            materials: (0..self.faces.len()).map(|i| self.materials.get(i).cloned().flatten()).collect(),
            creases: self.creases.iter().map(|[a, b]| key(*a, *b)).collect(),
        }
    }

    fn size(&self) -> f64 {
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for p in &self.points {
            lo = lo.min(DVec3::from(*p));
            hi = hi.max(DVec3::from(*p));
        }
        (hi - lo).length().max(1e-6)
    }

    /// Each point's mirror partner (itself when on the plane), if symmetric.
    pub fn mirrors(&self) -> Vec<Option<usize>> {
        let Some(sym) = self.symmetry else { return vec![None; self.points.len()] };
        let tol = self.size() * 1e-5 + 1e-7;
        let cell = |p: [f64; 3]| ((p[0] / tol / 4.0).round() as i64, (p[1] / tol / 4.0).round() as i64, (p[2] / tol / 4.0).round() as i64);
        let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in self.points.iter().enumerate() {
            grid.entry(cell(*p)).or_default().push(i);
        }
        self.points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut q = *p;
                q[sym.axis] = 2.0 * sym.center - p[sym.axis];
                if (q[sym.axis] - p[sym.axis]).abs() <= tol {
                    return Some(i);
                }
                let c = cell(q);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            if let Some(list) = grid.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) {
                                for &j in list {
                                    if DVec3::from(self.points[j]).distance(DVec3::from(q)) <= tol {
                                        return Some(j);
                                    }
                                }
                            }
                        }
                    }
                }
                None
            })
            .collect()
    }

    /// Is the cage symmetric across a plane through its middle? Finds the axis.
    pub fn detect_symmetry(&mut self) {
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for p in &self.points {
            lo = lo.min(DVec3::from(*p));
            hi = hi.max(DVec3::from(*p));
        }
        let c = (lo + hi) / 2.0;
        // prefer y (across a car or a building), then x, then z
        for axis in [1, 0, 2] {
            self.symmetry = Some(Symmetry { axis, center: c[axis] });
            if self.mirrors().iter().all(|m| m.is_some()) {
                return;
            }
        }
        self.symmetry = None;
    }

    /// Move points (mirroring each move to its partner when symmetric; points
    /// on the mirror plane stay on it).
    pub fn move_points(&mut self, moves: &[(usize, [f64; 3])], mirror: bool) -> Result<()> {
        let partners = if mirror { self.mirrors() } else { vec![None; self.points.len()] };
        let sym = self.symmetry;
        for &(i, p) in moves {
            if i >= self.points.len() {
                bail!("no cage point {i}");
            }
            let mut p = p;
            if let (Some(s), Some(j)) = (sym, partners[i]) {
                if j == i {
                    p[s.axis] = s.center; // on the plane: stays on it
                } else {
                    let mut q = p;
                    q[s.axis] = 2.0 * s.center - p[s.axis];
                    self.points[j] = q;
                }
            }
            self.points[i] = p;
        }
        Ok(())
    }

    /// Insert an edge loop across the edge a-b at `t` (0..1 from a): it runs
    /// through quads both ways until it meets a border, a non-quad, or itself.
    /// Returns the new points.
    pub fn insert_loop(&mut self, a: usize, b: usize, t: f64) -> Result<Vec<usize>> {
        let t = t.clamp(0.02, 0.98);
        // directed edge -> the face it's in
        let mut owner: HashMap<(usize, usize), usize> = HashMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for k in 0..f.len() {
                owner.insert((f[k], f[(k + 1) % f.len()]), fi);
            }
        }
        if !owner.contains_key(&(a, b)) && !owner.contains_key(&(b, a)) {
            bail!("{a}-{b} isn't an edge of the cage");
        }
        // walk: each step crosses a quad from edge (p, q) to its opposite (r, s),
        // keeping "t from the first point" consistent
        let mut cut: Vec<(usize, usize, f64)> = Vec::new(); // edges to split: (from, to, t)
        let mut split_faces: Vec<(usize, usize, usize)> = Vec::new(); // face, k of the entry edge, ...
        let mut seen_faces = HashSet::new();
        for (start_from, start_to, tt) in [(a, b, t), (b, a, 1.0 - t)] {
            let (mut p, mut q, tt) = (start_from, start_to, tt);
            cut.push((p, q, tt));
            loop {
                let Some(&fi) = owner.get(&(p, q)) else { break };
                if !seen_faces.insert(fi) {
                    break;
                }
                let f = &self.faces[fi];
                if f.len() != 4 {
                    break;
                }
                let k = f.iter().position(|&v| v == p).unwrap();
                split_faces.push((fi, k, 0));
                // quad (p, q, r, s): opposite edge runs s -> r, parallel to p -> q
                let (r, s) = (f[(k + 2) % 4], f[(k + 3) % 4]);
                cut.push((s, r, tt));
                // the neighbor across that edge runs it the other way (s -> r),
                // and the point sits the same fraction from s
                p = s;
                q = r;
            }
        }
        // new points, one per cut edge (shared both ways round)
        let mut at: HashMap<(usize, usize), usize> = HashMap::new();
        let mut new_points = Vec::new();
        for (u, v, tt) in cut {
            if at.contains_key(&(u, v)) || at.contains_key(&(v, u)) {
                continue;
            }
            let (pu, pv) = (DVec3::from(self.points[u]), DVec3::from(self.points[v]));
            let idx = self.points.len();
            self.points.push(pu.lerp(pv, tt).to_array());
            at.insert((u, v), idx);
            at.insert((v, u), idx);
            new_points.push(idx);
            // a creased edge stays creased in both halves
            if let Some(ci) = self.creases.iter().position(|c| key(c[0], c[1]) == key(u, v)) {
                self.creases.remove(ci);
                self.creases.push([u, idx]);
                self.creases.push([idx, v]);
            }
        }
        // split each crossed quad in two
        let mut add = Vec::new();
        let mut gone = HashSet::new();
        for (fi, k, _) in split_faces {
            let f = self.faces[fi].clone();
            let (p, q, r, s) = (f[k], f[(k + 1) % 4], f[(k + 2) % 4], f[(k + 3) % 4]);
            let (Some(&m1), Some(&m2)) = (at.get(&(p, q)), at.get(&(s, r))) else { continue };
            add.push((vec![p, m1, m2, s], self.materials.get(fi).cloned().flatten()));
            add.push((vec![m1, q, r, m2], self.materials.get(fi).cloned().flatten()));
            gone.insert(fi);
        }
        self.replace_faces(&gone, add);
        Ok(new_points)
    }

    fn replace_faces(&mut self, gone: &HashSet<usize>, add: Vec<(Vec<usize>, Option<String>)>) {
        let mut faces = Vec::new();
        let mut mats = Vec::new();
        for (i, f) in self.faces.iter().enumerate() {
            if !gone.contains(&i) {
                faces.push(f.clone());
                mats.push(self.materials.get(i).cloned().flatten());
            }
        }
        for (f, m) in add {
            faces.push(f);
            mats.push(m);
        }
        self.faces = faces;
        self.materials = mats;
    }

    /// Extrude faces `distance` along their average normal (sides built from
    /// the region's border). Returns the moved faces' new indexes.
    pub fn extrude(&mut self, sel: &[usize], distance: f64) -> Result<Vec<usize>> {
        let set: HashSet<usize> = sel.iter().copied().filter(|&f| f < self.faces.len()).collect();
        if set.is_empty() {
            bail!("pick faces to extrude");
        }
        let normal = |f: &Vec<usize>| crate::geom::newell(&f.iter().map(|&i| DVec3::from(self.points[i])).collect::<Vec<_>>()).normalize_or_zero();
        // per region vertex: the average normal of its selected faces
        let mut nsum: HashMap<usize, DVec3> = HashMap::new();
        let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();
        for &fi in &set {
            let f = &self.faces[fi];
            let n = normal(f);
            for k in 0..f.len() {
                *nsum.entry(f[k]).or_default() += n;
                *edge_count.entry(key(f[k], f[(k + 1) % f.len()])).or_default() += 1;
            }
        }
        let mut moved: HashMap<usize, usize> = HashMap::new();
        for (&v, n) in &nsum {
            let p = DVec3::from(self.points[v]) + n.normalize_or_zero() * distance;
            moved.insert(v, self.points.len());
            self.points.push(p.to_array());
        }
        let mut add = Vec::new();
        for &fi in &set {
            let f = self.faces[fi].clone();
            let mat = self.materials.get(fi).cloned().flatten();
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                if edge_count[&key(a, b)] == 1 {
                    add.push((vec![a, b, moved[&b], moved[&a]], mat.clone()));
                }
            }
        }
        let first = self.faces.len() - set.len() + add.len();
        let tops: Vec<(Vec<usize>, Option<String>)> = set.iter().map(|&fi| (self.faces[fi].iter().map(|v| moved[v]).collect(), self.materials.get(fi).cloned().flatten())).collect();
        let n_tops = tops.len();
        add.extend(tops);
        self.replace_faces(&set, add);
        Ok((first..first + n_tops).collect())
    }

    /// Delete faces (and points nothing uses any more).
    pub fn delete_faces(&mut self, sel: &[usize]) {
        let set: HashSet<usize> = sel.iter().copied().collect();
        self.replace_faces(&set, vec![]);
        self.compact();
    }

    /// Merge points into one at their middle.
    pub fn merge(&mut self, sel: &[usize]) -> Result<()> {
        let sel: Vec<usize> = sel.iter().copied().filter(|&i| i < self.points.len()).collect();
        if sel.len() < 2 {
            bail!("pick two or more points to merge");
        }
        let mid = sel.iter().map(|&i| DVec3::from(self.points[i])).sum::<DVec3>() / sel.len() as f64;
        let keep = sel[0];
        self.points[keep] = mid.to_array();
        let gone: HashSet<usize> = sel[1..].iter().copied().collect();
        for f in self.faces.iter_mut() {
            for v in f.iter_mut() {
                if gone.contains(v) {
                    *v = keep;
                }
            }
            f.dedup();
            while f.len() > 1 && f.first() == f.last() {
                f.pop();
            }
        }
        for c in self.creases.iter_mut() {
            for v in c.iter_mut() {
                if gone.contains(v) {
                    *v = keep;
                }
            }
        }
        self.creases.retain(|c| c[0] != c[1]);
        let degenerate: HashSet<usize> = self.faces.iter().enumerate().filter(|(_, f)| f.len() < 3).map(|(i, _)| i).collect();
        self.replace_faces(&degenerate, vec![]);
        self.compact();
        Ok(())
    }

    /// Drop points no face uses (renumbering the rest).
    fn compact(&mut self) {
        let mut used = vec![false; self.points.len()];
        for f in &self.faces {
            for &v in f {
                used[v] = true;
            }
        }
        let mut map = vec![usize::MAX; self.points.len()];
        let mut points = Vec::new();
        for (i, p) in self.points.iter().enumerate() {
            if used[i] {
                map[i] = points.len();
                points.push(*p);
            }
        }
        self.points = points;
        for f in self.faces.iter_mut() {
            for v in f.iter_mut() {
                *v = map[*v];
            }
        }
        self.creases = self.creases.iter().filter(|c| map[c[0]] != usize::MAX && map[c[1]] != usize::MAX).map(|c| [map[c[0]], map[c[1]]]).collect();
    }

    /// Crease or uncrease edges.
    pub fn set_creases(&mut self, edges: &[[usize; 2]], on: bool) {
        for e in edges {
            let k = key(e[0], e[1]);
            let has = self.creases.iter().position(|c| key(c[0], c[1]) == k);
            match (on, has) {
                (true, None) => self.creases.push([k.0, k.1]),
                (false, Some(i)) => {
                    self.creases.remove(i);
                }
                _ => {}
            }
        }
    }
}

/// A group's faces as a cage (to subdivide what's already modeled).
pub fn cage_of(m: &Model, group: Id, keep_sharp: bool) -> Result<Cage> {
    let faces = m.faces_of(&[group])?;
    let mut index: HashMap<Id, usize> = HashMap::new();
    let mut points = Vec::new();
    let mut out = Vec::new();
    let mut materials = Vec::new();
    for f in faces {
        let face = &m.faces[&f];
        if !face.holes.is_empty() {
            bail!("faces with holes can't be subdivided: split them first");
        }
        let ring: Vec<usize> = face
            .outer
            .iter()
            .map(|v| {
                *index.entry(*v).or_insert_with(|| {
                    points.push(m.p(*v));
                    points.len() - 1
                })
            })
            .collect();
        out.push(ring);
        materials.push(face.material.clone());
    }
    // hard (not soft) edges between two faces at a sharp angle become creases
    let mut creases = HashSet::new();
    for e in m.edges.values().filter(|e| keep_sharp && !e.soft && e.group.is_some_and(|g| m.subtree(group).contains(&g))) {
        let (Some(&a), Some(&b)) = (index.get(&e.a), index.get(&e.b)) else { continue };
        let users = m.faces_using_edge(e.a, e.b);
        if users.len() == 2 {
            let (n1, n2) = (m.normal(users[0]), m.normal(users[1]));
            if n1.dot(n2) < 0.5 {
                creases.insert(key(a, b));
            }
        }
    }
    Ok(Cage { points, faces: out, materials, creases })
}
