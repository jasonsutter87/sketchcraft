//! Wall pull: grab a wall and pull it, like a face, but with more connections.
//!
//! A wall here is a straight upright slab: one big face (the one you pick), the face behind it (the other side of the
//! wall, the same height and parallel to it), and whatever closes the slab between them (end caps, a top, a bottom).
//! Pulling moves both big faces together along the picked face's normal, so the thickness stays the same, and the
//! openings in them (doors, windows, with their reveals) go along. Everything that touches the wall and is not part of
//! it stretches to stay joined: a corner neighbour is lengthened or shortened along its own edge, a wall that ends on
//! this one (a T-junction, even one drawn as a separate group) slides its end with it. Nothing else moves.
//!
//! The work is a plan ([`WallPlan`]): for every vertex that moves, how far it travels *per metre pulled*. Sliding
//! along a neighbour's edge makes that a straight line, so the plan is linear in the distance: the app asks for it once
//! on hover and previews any distance with it, and the command applies the same plan after checking the result.
//! A pull that would bend a face, collapse one, or leave a door or window of a stretched neighbour outside it is
//! refused and the model is left exactly as it was.
//!
//! Room mode plugs in here: a room is a ring of walls, and dragging it out is one `wall_pull` per wall (or one plan
//! merged from several); the planner and the validation are shared.

use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DVec2, DVec3};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Thickest wall we will take as a wall (metres).
const MAX_THICKNESS: f64 = 1.5;
/// Edges shorter than this after a pull count as collapsed (metres).
const MIN_EDGE: f64 = 1e-3;
const TOL: f64 = 1e-4;

#[derive(Debug, Clone)]
pub struct WallPlan {
    /// the face that was picked, and the face on the other side of the wall
    pub a: Id,
    pub b: Id,
    /// the pull direction for a positive distance: the picked face's outward normal (horizontal, unit)
    pub n: DVec3,
    pub thickness: f64,
    /// every face of the wall itself (both sides, caps, top, bottom, reveals)
    pub faces: Vec<Id>,
    /// the top and bottom strips of the wall, as quads (for highlighting a wall that is part of a bigger top face)
    pub strips: Vec<[DVec3; 4]>,
    /// vertex -> travel per metre pulled
    pub moves: BTreeMap<Id, DVec3>,
    /// doors and windows that sit on the wall and ride along
    pub instances: Vec<Id>,
}

impl Model {
    /// The side of a wall for a click on its top (the way a plan view or a high camera sees a wall): of the upright
    /// faces that share an edge with the top face, the ones that make a wall with a thickness (a wall's long sides,
    /// not its end caps), and of those the one whose edge is nearest `at`.
    pub fn wall_side_from_top(&self, top: Id, at: DVec3) -> Result<Id> {
        let f = self.face(top)?;
        let n = self.normal(top);
        if n.z.abs() < 0.9999 {
            bail!("pick the side of a wall");
        }
        let mut sides: Vec<(f64, Id)> = Vec::new();
        for l in f.loops() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                let (pa, pb) = (self.p(a), self.p(b));
                let d = pb - pa;
                let t = if d.length_squared() > 0.0 { ((at - pa).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                let dist = DVec2::new(at.x - (pa + d * t).x, at.y - (pa + d * t).y).length();
                // the upright face that runs along this edge, the other way round
                let side = self.faces.iter().find(|(&id, g)| {
                    id != top && g.outer.len() >= 3 && self.normal(id).z.abs() < 1e-4 && (0..g.outer.len()).any(|k| g.outer[k] == b && g.outer[(k + 1) % g.outer.len()] == a)
                });
                if let Some((&id, _)) = side {
                    sides.push((dist, id));
                }
            }
        }
        sides.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut best: Option<(f64, Id)> = None;
        for &(_, id) in sides.iter().take(8) {
            if let Ok(p) = self.wall_plan(id) {
                if best.is_none_or(|(t, _)| p.thickness < t - 1e-6) {
                    best = Some((p.thickness, id));
                }
            }
        }
        best.map(|(_, id)| id).ok_or_else(|| anyhow::anyhow!("that isn't the top of a wall that can be pulled: Wall pull needs a straight wall with two parallel faces"))
    }

    /// Work out what pulling the wall that `face` belongs to would move.
    pub fn wall_plan(&self, face: Id) -> Result<WallPlan> {
        let fa = self.face(face)?.clone();
        let outer_a = self.pts(&fa.outer);
        let na = geom::newell(&outer_a);
        if na == DVec3::ZERO || na.z.abs() > 1e-4 {
            bail!("Wall pull works on upright walls: pick the side of a wall");
        }
        let n = DVec3::new(na.x, na.y, 0.0).normalize();
        if !self.is_flat(face) {
            bail!("that face isn't flat, so it isn't a straight wall");
        }
        let w = n.dot(outer_a[0]);
        let along = DVec3::new(-n.y, n.x, 0.0);
        let (zmin, zmax) = z_range(&outer_a);
        if zmax - zmin < 0.05 {
            bail!("that face is too low to be a wall");
        }
        let interval = |pts: &[DVec3]| {
            let v: Vec<f64> = pts.iter().map(|p| p.dot(along)).collect();
            (v.iter().cloned().fold(f64::MAX, f64::min), v.iter().cloned().fold(f64::MIN, f64::max))
        };
        let ia = interval(&outer_a);

        // the other side of the wall: the nearest parallel face behind, same group, same height, overlapping along the wall
        let mut other: Option<(f64, Id)> = None;
        for (&id, g) in &self.faces {
            if id == face || g.group != fa.group || g.outer.len() < 3 {
                continue;
            }
            let pts = self.pts(&g.outer);
            let ng = geom::newell(&pts);
            if ng.dot(n) > -0.99999 {
                continue;
            }
            let t = w - n.dot(pts[0]);
            if !(0.005..=MAX_THICKNESS).contains(&t) || pts.iter().any(|p| (w - n.dot(*p) - t).abs() > TOL) {
                continue;
            }
            let (lo, hi) = z_range(&pts);
            if (lo - zmin).abs() > TOL || (hi - zmax).abs() > TOL {
                continue;
            }
            let ib = interval(&pts);
            if ib.1.min(ia.1) - ib.0.max(ia.0) < 1e-3 {
                continue;
            }
            if other.is_none_or(|(d, _)| t < d) {
                other = Some((t, id));
            }
        }
        let Some((thickness, b)) = other else {
            bail!("couldn't find the other side of this wall: Wall pull needs a straight wall with two parallel faces");
        };
        let outer_b = self.pts(&self.faces[&b].outer);
        // closed between the two faces: some face (a top or a bottom) runs from one side to the other
        let va: BTreeSet<Id> = fa.outer.iter().copied().collect();
        let vb: BTreeSet<Id> = self.faces[&b].outer.iter().copied().collect();
        let closed = self.faces.iter().any(|(&id, f)| {
            id != face && id != b && f.loops().flatten().filter(|v| va.contains(v)).count() >= 2 && f.loops().flatten().filter(|v| vb.contains(v)).count() >= 2
        });
        if !closed {
            bail!("this isn't a solid wall: its two sides aren't joined by a top or a bottom");
        }

        // the wall's own vertices: both sides, openings included
        let mut core: BTreeSet<Id> = BTreeSet::new();
        for f in [face, b] {
            core.extend(self.faces[&f].loops().flatten().copied());
        }
        let faces: Vec<Id> = self
            .faces
            .iter()
            .filter(|(_, f)| f.loops().flatten().all(|v| core.contains(v)))
            .map(|(&k, _)| k)
            .collect();

        // everything else that touches the wall: inside its slab and its outline (the end of a wall that meets this one)
        let ring_a = geom::to2d(&outer_a, along, DVec3::Z);
        let mut moving = core.clone();
        for (&v, vert) in &self.vertices {
            if core.contains(&v) {
                continue;
            }
            let d = n.dot(vert.p) - w;
            if d > TOL || d < -thickness - TOL {
                continue;
            }
            if geom::inside_or_on(DVec2::new(vert.p.dot(along), vert.p.z), &ring_a) {
                moving.insert(v);
            }
        }

        // who is joined to whom
        let mut adj: HashMap<Id, Vec<Id>> = HashMap::new();
        for e in self.edges.values() {
            adj.entry(e.a).or_default().push(e.b);
            adj.entry(e.b).or_default().push(e.a);
        }
        let mut moves = BTreeMap::new();
        for &v in &moving {
            let p = self.p(v);
            let mut slide: Option<DVec3> = None;
            let mut skewed = false;
            for &x in adj.get(&v).map(|a| a.as_slice()).unwrap_or(&[]) {
                if moving.contains(&x) {
                    continue;
                }
                let d = self.p(x) - p;
                if d.length() < 1e-9 {
                    continue;
                }
                let u = d.normalize();
                if u.z.abs() > 0.999999 {
                    continue; // straight up or down: no horizontal travel to give
                }
                if u.dot(n).abs() < 0.05 {
                    skewed = true; // runs alongside the wall: it can't follow
                    continue;
                }
                match slide {
                    None => slide = Some(u),
                    Some(s) if s.cross(u).length() < 1e-6 => {}
                    Some(_) => bail!("a corner of this wall joins other walls in different directions, so it can't stretch them"),
                }
            }
            let k = match slide {
                Some(u) => u / u.dot(n),
                None if skewed && core.contains(&v) => {
                    bail!("this wall carries on in a straight line past its end: pull the whole straight run instead")
                }
                None => n,
            };
            moves.insert(v, k);
        }

        // doors and windows that sit on either side of the wall
        let mut instances = Vec::new();
        for (&id, inst) in &self.instances {
            let Some(def) = self.definitions.get(&inst.def).filter(|d| d.cuts) else { continue };
            let Some(back) = crate::openings::back_outline(&def.model) else { continue };
            let world: Vec<DVec3> = back.iter().map(|p| inst.transform.transform_point3(*p)).collect();
            let on = |plane: f64| world.iter().all(|p| (n.dot(*p) - plane).abs() < 0.03);
            if !(on(w) || on(w - thickness)) {
                continue;
            }
            let c = geom::centroid(&world);
            let (s, z) = (c.dot(along), c.z);
            if s >= ia.0 - TOL && s <= ia.1 + TOL && z >= zmin - TOL && z <= zmax + TOL {
                instances.push(id);
            }
        }

        // the top and bottom strips, for a wall that is only part of a bigger top face
        let mut strips = Vec::new();
        for z in [zmin, zmax] {
            let ends = |pts: &[DVec3]| {
                let row: Vec<DVec3> = pts.iter().copied().filter(|p| (p.z - z).abs() < TOL).collect();
                let lo = row.iter().copied().min_by(|a, b| a.dot(along).total_cmp(&b.dot(along)));
                let hi = row.iter().copied().max_by(|a, b| a.dot(along).total_cmp(&b.dot(along)));
                lo.zip(hi)
            };
            if let (Some((a0, a1)), Some((b0, b1))) = (ends(&outer_a), ends(&outer_b)) {
                strips.push([a0, a1, b1, b0]);
            }
        }
        Ok(WallPlan { a: face, b, n, thickness, faces, strips, moves, instances })
    }

    /// Pull the wall that `face` belongs to `by` metres along the face's outward normal (negative: the other way).
    /// Everything is checked before anything moves; on a refusal the model is untouched.
    pub fn wall_pull(&mut self, face: Id, by: f64) -> Result<WallPlan> {
        if !by.is_finite() || by.abs() > 10_000.0 {
            bail!("that distance isn't one the wall can be pulled");
        }
        if by.abs() < 1e-9 {
            bail!("pull the wall some distance (not zero)");
        }
        let plan = self.wall_plan(face)?;
        let mut touched: Vec<Id> = plan.faces.clone();
        touched.extend(self.faces.iter().filter(|(_, f)| f.loops().flatten().any(|v| plan.moves.contains_key(v))).map(|(&k, _)| k));
        touched.sort();
        touched.dedup();
        let mut check = touched.clone();
        check.extend(plan.instances.iter().copied());
        self.check_unlocked(&check)?;

        let new_p = |m: &Model, v: Id| m.p(v) + plan.moves.get(&v).map_or(DVec3::ZERO, |k| *k * by);
        for &fid in &touched {
            let f = &self.faces[&fid];
            let all_moved = f.loops().flatten().all(|v| plan.moves.contains_key(v));
            let old_outer = self.pts(&f.outer);
            let new_outer: Vec<DVec3> = f.outer.iter().map(|&v| new_p(self, v)).collect();
            let (on, nn) = (geom::newell_raw(&old_outer), geom::newell_raw(&new_outer));
            if nn.length() < 1e-9 || nn.normalize().dot(on.normalize_or_zero()) < 0.9999 {
                bail!("can't pull that far: the wall beside it would turn inside out");
            }
            if self.is_flat(fid) {
                let nz = nn.normalize();
                if f.loops().flatten().any(|&v| (new_p(self, v) - new_outer[0]).dot(nz).abs() > 1e-5) {
                    bail!("a wall beside it would bend: pulling this wall would stretch it out of flat");
                }
            }
            if all_moved {
                continue; // it travels as one piece
            }
            for l in f.loops() {
                let pts: Vec<DVec3> = l.iter().map(|&v| new_p(self, v)).collect();
                if (0..pts.len()).any(|i| pts[i].distance(pts[(i + 1) % pts.len()]) < MIN_EDGE) {
                    bail!("can't pull that far: the wall beside it would shrink to nothing");
                }
            }
            if !f.holes.is_empty() {
                let (u, v) = geom::basis(nn.normalize());
                let ring = geom::to2d(&new_outer, u, v);
                let fits = f.holes.iter().flatten().all(|&h| {
                    let q = new_p(self, h);
                    geom::inside(DVec2::new(q.dot(u), q.dot(v)), &ring)
                });
                if !fits {
                    bail!("can't pull that far: a door or window in the wall beside it wouldn't fit any more");
                }
            }
        }

        for (&v, k) in &plan.moves {
            self.vertices.get_mut(&v).unwrap().p += *k * by;
        }
        let shift = DMat4::from_translation(plan.n * by);
        for id in &plan.instances {
            let i = self.instances.get_mut(id).unwrap();
            i.transform = shift * i.transform;
        }
        Ok(plan)
    }
}

fn z_range(pts: &[DVec3]) -> (f64, f64) {
    pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)))
}
