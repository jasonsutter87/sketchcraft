//! Edge tools: Bevel (chamfer) and Round (fillet) the outside edges of a
//! solid, Divide an edge into equal parts, and Slice a solid in two.
//!
//! Bevel and Round cut a long wedge along each edge with Solid Tools: for a
//! chamfer the wedge's end is a triangle (corner, and a point `size` along
//! each face); for a round, the corner and an arc tangent to both faces.

use crate::csg::{self, Op, Poly};
use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use std::collections::{BTreeMap, HashMap};

/// Sort cutters into batches in which no two are near each other (their boxes, grown a little, don't touch), and
/// join each batch into one polygon soup. Cutting a batch at once gives the same solid as cutting its members one
/// at a time, since they don't meet.
fn apart_batches(m: &Model, cutters: Vec<Vec<Poly>>) -> Result<Vec<Vec<Poly>>> {
    const GROW: f64 = 1e-3;
    let boxes: Vec<(DVec3, DVec3)> = cutters
        .iter()
        .map(|c| {
            let mut lo = DVec3::splat(f64::INFINITY);
            let mut hi = DVec3::splat(f64::NEG_INFINITY);
            for q in c.iter().flat_map(|p| p.pts.iter()) {
                lo = lo.min(*q);
                hi = hi.max(*q);
            }
            (lo - DVec3::splat(GROW), hi + DVec3::splat(GROW))
        })
        .collect();
    let mut batches: Vec<(Vec<usize>, Vec<Poly>)> = Vec::new();
    let mut work = 0usize;
    for (i, c) in cutters.into_iter().enumerate() {
        let (lo, hi) = boxes[i];
        let mut put = None;
        for (k, (members, _)) in batches.iter().enumerate() {
            work += members.len();
            if work >= 4096 {
                work = 0;
                m.check_deadline()?;
            }
            if members.iter().all(|&j| {
                let (l2, h2) = boxes[j];
                lo.x > h2.x || hi.x < l2.x || lo.y > h2.y || hi.y < l2.y || lo.z > h2.z || hi.z < l2.z
            }) {
                put = Some(k);
                break;
            }
        }
        match put {
            Some(k) => {
                batches[k].0.push(i);
                batches[k].1.extend(c);
            }
            None => batches.push((vec![i], c)),
        }
    }
    Ok(batches.into_iter().map(|(_, polys)| polys).collect())
}

impl Model {
    /// Chamfer (`segments` 1) or round (`segments` > 1) the picked edges by
    /// `size` meters. The edges must be outside edges of solid groups.
    pub fn bevel(&mut self, edges: &[Id], size: f64, segments: usize) -> Result<Vec<Id>> {
        if size <= 0.0 {
            bail!("the bevel size must be positive");
        }
        let segments = segments.clamp(1, 64);
        // cutters per group
        let mut by_group: BTreeMap<Id, Vec<Vec<Poly>>> = BTreeMap::new();
        // every world face once, and where each plain face sits in that list (so an edge finds its face without a scan)
        let mut world: Option<(Vec<crate::components::WFace>, std::collections::HashMap<Id, usize>)> = None;
        let mut by_edge: Option<HashMap<(Id, Id), Vec<Id>>> = None;
        for (i, &eid) in edges.iter().enumerate() {
            if i % 64 == 0 {
                self.check_deadline()?;
            }
            let Some(e) = self.edges.get(&eid).cloned() else { bail!("{eid} is not an edge") };
            let Some(g) = e.group.filter(|g| self.groups.contains_key(g)) else {
                bail!("bevel works on solids: make the shape a group first (select it, right-click, Make group)")
            };
            let (a, b) = (self.p(e.a), self.p(e.b));
            let along = b - a;
            if along.length() < 1e-6 {
                continue;
            }
            let dir = along.normalize();
            let index = by_edge.get_or_insert_with(|| {
                let mut ix: HashMap<(Id, Id), Vec<Id>> = HashMap::new();
                for (&fid, f) in &self.faces {
                    for l in f.loops() {
                        for (x, y) in crate::model::pairs(l) {
                            ix.entry((x.min(y), x.max(y))).or_default().push(fid);
                        }
                    }
                }
                ix
            });
            let mut faces = index.get(&(e.a.min(e.b), e.a.max(e.b))).cloned().unwrap_or_default();
            faces.dedup();
            if faces.len() != 2 {
                bail!("an edge to bevel must be shared by two faces");
            }
            // each face's direction away from the edge, along its surface
            let mid = (a + b) / 2.0;
            let mut ds = Vec::new();
            let mut normals = Vec::new();
            for f in &faces {
                let n = self.normal(*f);
                let mut d = n.cross(dir).normalize();
                let c = geom::centroid(&self.pts(&self.faces[f].outer));
                if (c - mid).dot(d) < 0.0 {
                    d = -d;
                }
                ds.push(d);
                normals.push(n);
            }
            let (d1, d2) = (ds[0], ds[1]);
            if normals[0].dot(d2) > 1e-6 {
                bail!("only outside (convex) edges can be beveled for now");
            }
            let theta = d1.dot(d2).clamp(-1.0, 1.0).acos();
            if theta < 1e-3 || theta > std::f64::consts::PI - 1e-3 {
                continue; // flat: nothing to bevel
            }
            // nudge the corner out a hair so the cutter isn't flush with the faces
            let corner = a - (d1 + d2).normalize() * 1e-4;
            let profile: Vec<DVec3> = if segments == 1 {
                vec![corner, a + d1 * size, a + d2 * size]
            } else {
                let s = size / (theta / 2.0).tan();
                let (t1, t2) = (a + d1 * s, a + d2 * s);
                let o = a + (d1 + d2).normalize() * (size / (theta / 2.0).sin());
                let (u1, u2) = ((t1 - o) / size, (t2 - o) / size);
                let ang = u1.dot(u2).clamp(-1.0, 1.0).acos();
                let mut p = vec![corner, t1];
                for k in 1..segments {
                    let t = k as f64 / segments as f64;
                    // slerp between the tangent points around the center
                    let v = (u1 * ((1.0 - t) * ang).sin() + u2 * (t * ang).sin()) / ang.sin();
                    p.push(o + v * size);
                }
                p.push(t2);
                p
            };
            let ext = 1e-4;
            let start: Vec<DVec3> = profile.iter().map(|q| *q - dir * ext).collect();
            let (wfs, at) = world.get_or_insert_with(|| {
                let all = self.world_faces();
                let mut at = std::collections::HashMap::new();
                for (i, w) in all.iter().enumerate() {
                    if let Some(f) = w.face {
                        at.entry(f).or_insert(i);
                    }
                }
                (all, at)
            });
            let mat = at.get(&faces[0]).and_then(|&i| csg::material_key(self, &wfs[i]));
            by_group.entry(g).or_default().push(csg::prism(&start, along + dir * 2.0 * ext, mat));
        }
        let mut out = Vec::new();
        // each group's faces, from the one list of world faces made above (not a new list for every group)
        let mut faces_of_group: HashMap<Id, Vec<&crate::components::WFace>> = HashMap::new();
        if let Some((wfs, _)) = &world {
            for w in wfs {
                if let (Some(g), Some(_)) = (w.group, w.face) {
                    faces_of_group.entry(g).or_default().push(w);
                }
            }
        }
        for (g, cutters) in by_group {
            self.check_deadline()?;
            // Wedges that overlap (where edges meet at a corner) go one batch at a time; wedges far apart are
            // cut together, so a shape with hundreds of edges needs a handful of booleans, not one per edge.
            let mut result = match faces_of_group.get(&g) {
                Some(fs) if self.groups.contains_key(&g) => csg::solid_polys_of(self, g, fs, false)?,
                _ => csg::solid_polys(self, g)?,
            };
            for batch in apart_batches(self, cutters)? {
                result = csg::boolean_for(self, result, batch, Op::Subtract)?;
            }
            let name = csg::name_of(self, g);
            let parent = self.groups.get(&g).and_then(|x| x.parent);
            self.delete(&[g])?;
            let ng = csg::to_group(self, &result, &name)?;
            self.groups.get_mut(&ng).unwrap().parent = parent;
            out.push(ng);
        }
        Ok(out)
    }

    /// Split an edge into `count` equal parts (new points along it).
    pub fn divide(&mut self, edge: Id, count: usize) -> Result<Vec<Id>> {
        if !(2..=500).contains(&count) {
            bail!("divide into 2 to 500 parts");
        }
        let Some(e) = self.edges.get(&edge).cloned() else { bail!("{edge} is not an edge") };
        let (a, b) = (self.p(e.a), self.p(e.b));
        let prev = self.active_group;
        self.active_group = e.group;
        let mut cur = edge;
        let mut points = Vec::new();
        for k in 1..count {
            let p = a.lerp(b, k as f64 / count as f64);
            let v = self.split_edge(cur, p);
            points.push(v);
            match self.find_edge(v, e.b) {
                Some(next) => cur = next,
                None => break,
            }
        }
        self.active_group = prev;
        Ok(points)
    }

    /// Cut a solid group / component in two along a plane. Returns the piece
    /// on the side `normal` points to, then the other.
    pub fn slice(&mut self, id: Id, origin: DVec3, normal: DVec3) -> Result<Vec<Id>> {
        if normal.length() < 1e-9 {
            bail!("the slice needs a direction");
        }
        let n = normal.normalize();
        let target = csg::solid_polys(self, id)?;
        let Some((lo, hi)) = self.world_bounds() else { bail!("nothing to slice") };
        let r = lo.distance(hi) + origin.distance((lo + hi) / 2.0) + 1.0;
        // a big box filling the half-space in front of the plane
        let (u, v) = geom::basis(n);
        let square = vec![origin - u * r - v * r, origin + u * r - v * r, origin + u * r + v * r, origin - u * r + v * r];
        let half = csg::prism(&square, n * r, None);
        let front = csg::boolean_for(self, target.clone(), half.clone(), Op::Intersect)?;
        let back = csg::boolean_for(self, target, half, Op::Subtract)?;
        if front.is_empty() || back.is_empty() {
            bail!("the plane doesn't go through it");
        }
        let name = csg::name_of(self, id);
        let parent = self.groups.get(&id).and_then(|x| x.parent).or_else(|| self.instances.get(&id).and_then(|i| i.parent));
        self.delete(&[id])?;
        let out = vec![csg::to_group(self, &front, &format!("{name} (1)"))?, csg::to_group(self, &back, &format!("{name} (2)"))?];
        for g in &out {
            self.groups.get_mut(g).unwrap().parent = parent;
        }
        Ok(out)
    }
}
