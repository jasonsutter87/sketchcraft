//! Print check: is the model ready to send to a slicer? See docs/dev/print-check.md.
//!
//! Read-only. `Model::print_check(&scope, &PrintOptions)` reports: is it a closed solid (reusing `fix::solid_check`),
//! walls thinner than a minimum, overhangs, the overall size in millimetres (with a hint when it looks like the wrong
//! units) and whether it fits a build volume. Geometry is meters inside; every number reported here is millimetres.

use crate::fix::{FixScope, SolidCheck};
use crate::geom;
use crate::model::{Id, Model};
use glam::DVec3;
use serde::Serialize;
use std::collections::BTreeMap;

/// What the person (or agent) can set.
#[derive(Clone, Debug)]
pub struct PrintOptions {
    /// walls thinner than this (mm) are flagged
    pub min_wall_mm: f64,
    /// how far a surface may lean out from vertical (degrees) before it needs support
    pub overhang_deg: f64,
    /// the printer's build volume (mm, x y z), when known
    pub build_mm: Option<[f64; 3]>,
}

impl Default for PrintOptions {
    fn default() -> Self {
        PrintOptions { min_wall_mm: 0.8, overhang_deg: 45.0, build_mm: None }
    }
}

/// Most findings listed per kind (the counts are always complete).
pub const MAX_LISTED: usize = 200;
/// Wall thickness: every triangle gets its ray (against a bounding-volume tree, so a ray costs about log n) up to this
/// many triangles. Past it the triangles are sampled at a fixed stride, the report says `sampled`, and the model is
/// not marked ready (a thin wall in an unsampled triangle could be missed). 600,000 triangles is a model far past
/// what the engine holds comfortably; a 200,000-triangle model is checked in full in a few seconds.
pub const MAX_FULL_TRIANGLES: usize = 600_000;
/// Faces whose every corner is within this of the lowest point sit on the bed (m).
const BED_TOL: f64 = 1e-5;
/// Looks too small below this largest side (mm), too large above it.
const TINY_MM: f64 = 1.0;
const HUGE_MM: f64 = 3000.0;

#[derive(Clone, Debug, Serialize)]
pub struct ThinWall {
    /// the face (or, inside a component, the placement that owns it)
    pub id: Id,
    pub in_component: bool,
    pub at: [f64; 3],
    pub thickness_mm: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Overhang {
    pub id: Id,
    pub in_component: bool,
    pub at: [f64; 3],
    /// how far the surface leans out from vertical, degrees (90 = a flat ceiling)
    pub lean_deg: f64,
    pub area_mm2: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThinWalls {
    pub count: usize,
    pub min_wall_mm: f64,
    /// the thinnest found, mm (none when nothing was flagged)
    pub thinnest_mm: Option<f64>,
    /// the faces were sampled, not all measured (a very large model)
    pub sampled: bool,
    pub truncated: bool,
    pub items: Vec<ThinWall>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Overhangs {
    pub count: usize,
    pub angle_deg: f64,
    pub area_mm2: f64,
    pub truncated: bool,
    pub items: Vec<Overhang>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnitsHint {
    /// "too_small" or "too_large"
    pub kind: &'static str,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Fit {
    pub fits: bool,
    /// it does not fit as modelled but would if turned
    pub fits_if_turned: bool,
    pub build_mm: [f64; 3],
    /// mm over on each side (x, y, z)
    pub over_mm: [f64; 3],
}

#[derive(Clone, Debug, Serialize)]
pub struct PrintReport {
    /// closed, consistently facing out, nothing shared by three faces (components too, for a whole-model check)
    pub solid: bool,
    pub solid_check: SolidCheck,
    pub thin_walls: ThinWalls,
    pub overhangs: Overhangs,
    /// overall size x y z in mm (None: no geometry)
    pub size_mm: Option<[f64; 3]>,
    pub units_hint: Option<UnitsHint>,
    pub fit: Option<Fit>,
    /// solid, no thin walls, a sensible size and it fits: overhangs do not block (they need supports)
    pub ready: bool,
    pub notes: Vec<String>,
}

struct Tri {
    a: DVec3,
    e1: DVec3,
    e2: DVec3,
    n: DVec3,
    face: usize,
}

struct PFace {
    id: Id,
    in_component: bool,
    normal: DVec3,
    tris: Vec<[DVec3; 3]>,
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn mm(p: DVec3) -> [f64; 3] {
    [round3(p.x * 1000.0), round3(p.y * 1000.0), round3(p.z * 1000.0)]
}

fn scope_has(scope: &FixScope, g: Option<Id>) -> bool {
    scope.groups.as_ref().is_none_or(|s| s.contains(&g))
}

impl Model {
    pub fn print_check(&self, scope: &FixScope, opts: &PrintOptions) -> PrintReport {
        let mut notes = Vec::new();
        let sc = self.solid_check(scope);
        let mut solid = sc.solid;
        if solid && scope.groups.is_none() && !self.everything_is_solid() {
            solid = false;
            notes.push("a component's own geometry is not solid (run Fix model on the whole model)".into());
        }
        // the faces to measure: the scope's own faces, plus placed components when looking at the whole model
        let mut faces: Vec<PFace> = Vec::new();
        for f in self.world_faces() {
            if let Some(fid) = f.face {
                if !self.faces.get(&fid).is_some_and(|x| scope_has(scope, x.group)) {
                    continue;
                }
            } else if scope.groups.is_some() {
                continue;
            }
            let n = f.normal.normalize_or_zero();
            if n == DVec3::ZERO {
                continue;
            }
            let tris: Vec<[DVec3; 3]> = geom::triangulate(&f.outer, &f.holes, n).into_iter().filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-18).collect();
            if tris.is_empty() {
                continue;
            }
            faces.push(PFace { id: f.face.unwrap_or(f.owner), in_component: f.face.is_none(), normal: n, tris });
        }
        if faces.is_empty() {
            notes.push("there is no geometry to check".into());
            return PrintReport {
                solid: false,
                solid_check: sc,
                thin_walls: ThinWalls { count: 0, min_wall_mm: opts.min_wall_mm, thinnest_mm: None, sampled: false, truncated: false, items: vec![] },
                overhangs: Overhangs { count: 0, angle_deg: opts.overhang_deg, area_mm2: 0.0, truncated: false, items: vec![] },
                size_mm: None,
                units_hint: None,
                fit: None,
                ready: false,
                notes,
            };
        }
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for p in faces.iter().flat_map(|f| f.tris.iter().flatten()) {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let size = (hi - lo) * 1000.0;
        let size_mm = [round3(size.x), round3(size.y), round3(size.z)];

        let thin_walls = thin_walls(&faces, opts.min_wall_mm, &mut notes);
        let overhangs = overhangs(&faces, lo.z, opts.overhang_deg);

        let largest = size.max_element();
        let units_hint = if largest < TINY_MM {
            Some(UnitsHint { kind: "too_small", text: format!("The largest side is only {} mm. If this was meant to be bigger, the model may be in the wrong units (scale it up before printing).", round3(largest)) })
        } else if largest > HUGE_MM {
            Some(UnitsHint { kind: "too_large", text: format!("The largest side is {} m. If this is a small object, the model may be in the wrong units (scale it down before printing).", round3(largest / 1000.0)) })
        } else {
            None
        };
        let fit = opts.build_mm.map(|b| {
            let s = [size.x, size.y, size.z];
            let over = [0, 1, 2].map(|i| round3((s[i] - b[i]).max(0.0)));
            let (mut ss, mut bb) = (s, b);
            ss.sort_by(|a, b| a.total_cmp(b));
            bb.sort_by(|a, b| a.total_cmp(b));
            let fits = over.iter().all(|o| *o == 0.0);
            Fit { fits, fits_if_turned: !fits && (0..3).all(|i| ss[i] <= bb[i] + 1e-9), build_mm: b, over_mm: over }
        });
        let ready = solid && thin_walls.count == 0 && units_hint.is_none() && fit.as_ref().is_none_or(|f| f.fits) && !thin_walls.sampled;
        if thin_walls.sampled && solid && thin_walls.count == 0 && units_hint.is_none() && fit.as_ref().is_none_or(|f| f.fits) {
            notes.push("not marked ready: no thin wall was found, but walls were only checked at some places on a model this large, so a thin one could be missed".into());
        }
        PrintReport { solid, solid_check: sc, thin_walls, overhangs, size_mm: Some(size_mm), units_hint, fit, ready, notes }
    }
}

/// Thin walls by ray casting: from the middle of each triangle of each face, a ray goes straight into the material
/// (against the face's outward normal) and the first surface it leaves through is the other side of the wall.
/// The distance is the wall's thickness there; a face is flagged by its thinnest sample.
fn thin_walls(faces: &[PFace], min_mm: f64, notes: &mut Vec<String>) -> ThinWalls {
    thin_walls_with(faces, min_mm, notes, true)
}

/// `indexed` false walks every triangle for every ray (the plain way; tests compare the two).
fn thin_walls_with(faces: &[PFace], min_mm: f64, notes: &mut Vec<String>, indexed: bool) -> ThinWalls {
    let tris: Vec<Tri> = faces
        .iter()
        .enumerate()
        .flat_map(|(fi, f)| f.tris.iter().map(move |t| Tri { a: t[0], e1: t[1] - t[0], e2: t[2] - t[0], n: f.normal, face: fi }))
        .collect();
    let total = tris.len();
    let stride = total.div_ceil(MAX_FULL_TRIANGLES).max(1);
    let sampled = stride > 1;
    if sampled {
        notes.push(format!("a very large model: wall thickness was measured at about {} of {} triangles", total.div_ceil(stride), total));
    }
    let min_m = min_mm / 1000.0;
    let bvh = if indexed { Some(Bvh::build(&tris)) } else { None };
    let mut best: BTreeMap<usize, (f64, DVec3)> = BTreeMap::new();
    for t in tris.iter().step_by(stride) {
        let origin = t.a + (t.e1 + t.e2) / 3.0;
        let dir = -t.n;
        let nearest = match &bvh {
            Some(b) => b.nearest(&tris, origin, dir, t.face, min_m),
            None => nearest_plain(&tris, origin, dir, t.face),
        };
        if nearest < min_m {
            let e = best.entry(t.face).or_insert((nearest, origin));
            if nearest < e.0 {
                *e = (nearest, origin);
            }
        }
    }
    let mut items: Vec<ThinWall> = best
        .into_iter()
        .map(|(fi, (d, at))| ThinWall { id: faces[fi].id, in_component: faces[fi].in_component, at: mm(at), thickness_mm: round3(d * 1000.0) })
        .collect();
    items.sort_by(|a, b| a.thickness_mm.total_cmp(&b.thickness_mm).then(a.id.cmp(&b.id)));
    let count = items.len();
    let thinnest_mm = items.first().map(|i| i.thickness_mm);
    items.truncate(MAX_LISTED);
    ThinWalls { count, min_wall_mm: min_mm, thinnest_mm, sampled, truncated: count > MAX_LISTED, items }
}

/// Is `o` a surface the ray from a triangle of face `face` could leave through? (Not its own face, and facing back at it.)
fn counts(o: &Tri, face: usize, dir: DVec3) -> bool {
    o.face != face && o.n.dot(dir) > 1e-6
}

/// The distance to the nearest far side, testing every triangle.
fn nearest_plain(tris: &[Tri], origin: DVec3, dir: DVec3, face: usize) -> f64 {
    let mut nearest = f64::INFINITY;
    for o in tris {
        if !counts(o, face, dir) {
            continue;
        }
        if let Some(d) = ray_tri(origin, dir, o) {
            nearest = nearest.min(d);
        }
    }
    nearest
}

/// A bounding-volume tree over the triangles (median split on the longest axis, small leaves).
struct Bvh {
    nodes: Vec<Node>,
    /// triangle indexes, grouped by leaf
    order: Vec<u32>,
}

struct Node {
    lo: DVec3,
    hi: DVec3,
    /// leaf: first index in `order`; inner: the left child (the right child is `right`)
    a: u32,
    /// leaf: how many; inner: unused
    n: u32,
    right: u32,
}

const LEAF: usize = 4;

impl Bvh {
    fn build(tris: &[Tri]) -> Bvh {
        // each triangle's box, a hair wider than the triangle (the ray test accepts a hair outside an edge)
        let boxes: Vec<(DVec3, DVec3, DVec3)> = tris
            .iter()
            .map(|t| {
                let (b, c) = (t.a + t.e1, t.a + t.e2);
                let (lo, hi) = (t.a.min(b).min(c), t.a.max(b).max(c));
                let pad = DVec3::splat(1e-8 * (1.0 + (hi - lo).max_element()));
                (lo - pad, hi + pad, (lo + hi) * 0.5)
            })
            .collect();
        let mut order: Vec<u32> = (0..tris.len() as u32).collect();
        let mut nodes = Vec::with_capacity(tris.len() / 2 + 1);
        if !order.is_empty() {
            Self::split(&boxes, &mut order, 0, tris.len(), &mut nodes);
        }
        Bvh { nodes, order }
    }

    fn split(boxes: &[(DVec3, DVec3, DVec3)], order: &mut [u32], start: usize, end: usize, nodes: &mut Vec<Node>) -> u32 {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        let (mut clo, mut chi) = (lo, hi);
        for &i in &order[start..end] {
            let b = &boxes[i as usize];
            lo = lo.min(b.0);
            hi = hi.max(b.1);
            clo = clo.min(b.2);
            chi = chi.max(b.2);
        }
        let me = nodes.len() as u32;
        if end - start <= LEAF {
            nodes.push(Node { lo, hi, a: start as u32, n: (end - start) as u32, right: 0 });
            return me;
        }
        nodes.push(Node { lo, hi, a: 0, n: 0, right: 0 });
        let ext = chi - clo;
        let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
        let mid = (start + end) / 2;
        order[start..end].select_nth_unstable_by(mid - start, |p, q| boxes[*p as usize].2[axis].total_cmp(&boxes[*q as usize].2[axis]));
        let left = Self::split(boxes, order, start, mid, nodes);
        let right = Self::split(boxes, order, mid, end, nodes);
        nodes[me as usize].a = left;
        nodes[me as usize].right = right;
        me
    }

    /// The distance to the nearest far side. Only a hit nearer than `stop_below` matters to the caller, so boxes
    /// farther than that are skipped (a farther answer may come back as infinity); below it the answer is the same
    /// as `nearest_plain`.
    fn nearest(&self, tris: &[Tri], origin: DVec3, dir: DVec3, face: usize, stop_below: f64) -> f64 {
        let mut nearest = f64::INFINITY;
        if self.nodes.is_empty() {
            return nearest;
        }
        let reach = stop_below * 1.000001 + 1e-9;
        let inv = DVec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
        let mut stack = [0u32; 128];
        let mut sp = 1;
        stack[0] = 0;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            if !hit_box(node, origin, dir, inv, nearest.min(reach)) {
                continue;
            }
            if node.n > 0 {
                for &ti in &self.order[node.a as usize..(node.a + node.n) as usize] {
                    let o = &tris[ti as usize];
                    if !counts(o, face, dir) {
                        continue;
                    }
                    if let Some(d) = ray_tri(origin, dir, o) {
                        nearest = nearest.min(d);
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = node.a;
                stack[sp + 1] = node.right;
                sp += 2;
            }
        }
        nearest
    }
}

/// Does the ray (from `o`, along `dir`, no farther than `limit`) pass through the node's box?
fn hit_box(n: &Node, o: DVec3, dir: DVec3, inv: DVec3, limit: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, limit);
    for k in 0..3 {
        if dir[k] == 0.0 {
            if o[k] < n.lo[k] || o[k] > n.hi[k] {
                return false;
            }
            continue;
        }
        let (a, b) = ((n.lo[k] - o[k]) * inv[k], (n.hi[k] - o[k]) * inv[k]);
        let (near, far) = if a < b { (a, b) } else { (b, a) };
        t0 = t0.max(near);
        t1 = t1.min(far);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Moller-Trumbore: distance along `dir` (a unit vector) to the triangle, or None. A hit closer than 0.1 micron is the origin's own surface.
fn ray_tri(o: DVec3, dir: DVec3, t: &Tri) -> Option<f64> {
    let p = dir.cross(t.e2);
    let det = t.e1.dot(p);
    if det.abs() < 1e-30 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t.a;
    let u = s.dot(p) * inv;
    if !(-1e-9..=1.0 + 1e-9).contains(&u) {
        return None;
    }
    let q = s.cross(t.e1);
    let v = dir.dot(q) * inv;
    if v < -1e-9 || u + v > 1.0 + 1e-9 {
        return None;
    }
    let d = t.e2.dot(q) * inv;
    (d > 1e-7).then_some(d)
}

/// Faces that lean out past `angle_deg` from vertical, facing down, and are not on the bed (every corner within a hair of the lowest point).
fn overhangs(faces: &[PFace], min_z: f64, angle_deg: f64) -> Overhangs {
    let limit = angle_deg.clamp(0.0, 90.0).to_radians().sin();
    let mut items = Vec::new();
    let mut total = 0.0;
    for f in faces {
        let down = -f.normal.z;
        if down <= limit + 1e-9 {
            continue;
        }
        if f.tris.iter().flatten().all(|p| p.z - min_z <= BED_TOL) {
            continue;
        }
        let area: f64 = f.tris.iter().map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() / 2.0).sum::<f64>() * 1e6;
        let n = f.tris.iter().flatten().count() as f64;
        let c = f.tris.iter().flatten().fold(DVec3::ZERO, |a, p| a + *p) / n;
        total += area;
        items.push(Overhang { id: f.id, in_component: f.in_component, at: mm(c), lean_deg: (down.clamp(-1.0, 1.0).asin().to_degrees() * 10.0).round() / 10.0, area_mm2: (area * 100.0).round() / 100.0 });
    }
    items.sort_by(|a, b| b.area_mm2.total_cmp(&a.area_mm2).then(a.id.cmp(&b.id)));
    let count = items.len();
    items.truncate(MAX_LISTED);
    Overhangs { count, angle_deg, area_mm2: (total * 100.0).round() / 100.0, truncated: count > MAX_LISTED, items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// A closed box as six quad faces (two triangles each), `thick` along z.
    fn add_box(faces: &mut Vec<PFace>, x: f64, y: f64, thick: f64) {
        let (lo, hi) = (DVec3::new(x, y, 0.0), DVec3::new(x + 0.01, y + 0.01, thick));
        let v = |i: usize| DVec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
        let quads: [([usize; 4], DVec3); 6] = [
            ([0, 2, 6, 4], -DVec3::X),
            ([1, 3, 7, 5], DVec3::X),
            ([0, 1, 5, 4], -DVec3::Y),
            ([2, 3, 7, 6], DVec3::Y),
            ([0, 1, 3, 2], -DVec3::Z),
            ([4, 5, 7, 6], DVec3::Z),
        ];
        for (q, n) in quads {
            faces.push(PFace { id: faces.len() as Id + 1, in_component: false, normal: n, tris: vec![[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]] });
        }
    }

    /// `boxes` boxes in a grid, every 7th one a 0.5 mm plate (too thin at the default 0.8 mm).
    fn grid(boxes: usize) -> Vec<PFace> {
        let side = (boxes as f64).sqrt().ceil() as usize;
        let mut faces = Vec::new();
        for i in 0..boxes {
            let thick = if i % 7 == 3 { 0.0005 } else { 0.01 };
            add_box(&mut faces, (i % side) as f64 * 0.02, (i / side) as f64 * 0.02, thick);
        }
        faces
    }

    fn same(a: &ThinWalls, b: &ThinWalls) -> bool {
        serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
    }

    #[test]
    fn the_index_gives_the_same_walls_as_testing_every_triangle() {
        let faces = grid(500); // 6,000 triangles, a few of them close together (touching boxes)
        let (mut n1, mut n2) = (vec![], vec![]);
        let t = Instant::now();
        let plain = thin_walls_with(&faces, 0.8, &mut n1, false);
        let t_plain = t.elapsed();
        let t = Instant::now();
        let fast = thin_walls_with(&faces, 0.8, &mut n2, true);
        let t_fast = t.elapsed();
        eprintln!("6,000 triangles: every-triangle {t_plain:?}, indexed {t_fast:?}");
        assert!(plain.count > 50 && plain.count < 500 * 6, "{}", plain.count);
        assert!(same(&plain, &fast));
        assert_eq!(n1, n2);
        assert!(!fast.sampled);
        // a wide limit makes every wall "thin", so many rays hit (neighbouring boxes, own far side): still the same answer
        for min in [3.0, 15.0, 40.0] {
            let (a, b) = (thin_walls_with(&faces, min, &mut vec![], false), thin_walls_with(&faces, min, &mut vec![], true));
            assert!(same(&a, &b), "min {min}");
        }
    }

    #[test]
    fn a_two_hundred_thousand_triangle_model_is_checked_in_full_and_quickly() {
        let faces = grid(16_700); // 200,400 triangles
        let total: usize = faces.iter().map(|f| f.tris.len()).sum();
        assert!(total > 200_000);
        let mut notes = vec![];
        let t = Instant::now();
        let r = thin_walls_with(&faces, 0.8, &mut notes, true);
        let took = t.elapsed();
        eprintln!("{total} triangles: indexed {took:?}");
        assert!(!r.sampled, "{notes:?}");
        assert!(r.count > 0);
        // the 0.5 mm plates (one box in seven, minus the ones too thin to tell apart) are all found, nothing else
        assert!(r.items.iter().all(|i| i.thickness_mm < 0.8));
        assert!(took.as_secs() < 30, "{took:?}");
    }

    #[test]
    fn only_a_truly_huge_model_is_sampled() {
        let faces = grid(55_000); // 660,000 triangles
        let mut notes = vec![];
        let r = thin_walls_with(&faces, 0.8, &mut notes, true);
        assert!(r.sampled);
        assert!(notes.iter().any(|n| n.contains("very large")), "{notes:?}");
    }
}
