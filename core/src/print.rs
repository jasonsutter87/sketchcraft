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
/// Wall thickness work budget: rays x triangles. Past it the faces are sampled (a note says so).
const RAY_BUDGET: usize = 40_000_000;
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
        let ready = solid && thin_walls.count == 0 && units_hint.is_none() && fit.as_ref().is_none_or(|f| f.fits);
        PrintReport { solid, solid_check: sc, thin_walls, overhangs, size_mm: Some(size_mm), units_hint, fit, ready, notes }
    }
}

/// Thin walls by ray casting: from the middle of each triangle of each face, a ray goes straight into the material
/// (against the face's outward normal) and the first surface it leaves through is the other side of the wall.
/// The distance is the wall's thickness there; a face is flagged by its thinnest sample.
fn thin_walls(faces: &[PFace], min_mm: f64, notes: &mut Vec<String>) -> ThinWalls {
    let tris: Vec<Tri> = faces
        .iter()
        .enumerate()
        .flat_map(|(fi, f)| f.tris.iter().map(move |t| Tri { a: t[0], e1: t[1] - t[0], e2: t[2] - t[0], n: f.normal, face: fi }))
        .collect();
    let total = tris.len();
    let max_rays = (RAY_BUDGET / total.max(1)).max(64);
    let stride = total.div_ceil(max_rays).max(1);
    let sampled = stride > 1;
    if sampled {
        notes.push(format!("a large model: wall thickness was measured at about {} of {} triangles", total.div_ceil(stride), total));
    }
    let min_m = min_mm / 1000.0;
    let mut best: BTreeMap<usize, (f64, DVec3)> = BTreeMap::new();
    for t in tris.iter().step_by(stride) {
        let origin = t.a + (t.e1 + t.e2) / 3.0;
        let dir = -t.n;
        let mut nearest = f64::INFINITY;
        for o in &tris {
            // the same face, or a surface that is not the far side of material
            if o.face == t.face || o.n.dot(dir) <= 1e-6 {
                continue;
            }
            if let Some(d) = ray_tri(origin, dir, o) {
                nearest = nearest.min(d);
            }
        }
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
