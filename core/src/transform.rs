//! Move, rotate, scale, copy and array, for any mix of faces, edges,
//! vertices, groups and component instances.
//!
//! Moving part of a solid can bend a face out of flat (pull one corner of a
//! roof up). Like other push/pull modelers we "auto-fold": such a face is
//! split into flat triangles joined by soft edges.

use crate::geom;
use crate::model::{pairs, Face, Id, Model, WELD};
use anyhow::{bail, Result};
use glam::{DMat4, DQuat, DVec3};
use std::collections::{BTreeSet, HashMap};

impl Model {
    /// Apply `t` to everything named in `ids`. Returns how many vertices and
    /// instances moved.
    pub fn transform(&mut self, ids: &[Id], t: DMat4) -> Result<usize> {
        self.check_unlocked(ids)?;
        // a group carries the components placed inside it
        let mut ids = ids.to_vec();
        for g in ids.clone().iter().filter(|g| self.groups.contains_key(g)) {
            let tree = self.subtree(*g);
            ids.extend(self.instances.iter().filter(|(_, i)| i.parent.is_some_and(|p| tree.contains(&p))).map(|(&k, _)| k));
        }
        ids.sort();
        ids.dedup();
        let (inst, geo): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| self.instances.contains_key(id));
        let vs = if geo.is_empty() { BTreeSet::new() } else { self.vertices_of(&geo)? };
        let opening_folds = self.plan_opening_folds(&vs, t)?;
        // subdivision cages follow their groups (and find their mirror plane again)
        let mut moved_cages = std::collections::BTreeSet::new();
        for g in ids.iter().filter(|g| self.groups.contains_key(g)) {
            moved_cages.extend(self.subtree(*g).into_iter().filter(|x| self.cages.contains_key(x)));
        }
        for g in moved_cages {
            let c = self.cages.get_mut(&g).unwrap();
            for p in c.points.iter_mut() {
                *p = t.transform_point3(DVec3::from(*p)).to_array();
            }
            if t.determinant() < 0.0 {
                for f in c.faces.iter_mut() {
                    f.reverse();
                }
            }
            if c.symmetry.is_some() {
                c.detect_symmetry();
            }
        }
        for id in &inst {
            let i = self.instances.get_mut(id).unwrap();
            i.transform = t * i.transform;
        }
        for v in &vs {
            let p = self.vertices[v].p;
            self.vertices.get_mut(v).unwrap().p = t.transform_point3(p);
        }
        if t.determinant() < 0.0 {
            // a mirror turns faces inside out; flip them back
            let faces: Vec<Id> = self
                .faces
                .iter()
                .filter(|(_, f)| f.loops().flatten().all(|v| vs.contains(v)))
                .map(|(&k, _)| k)
                .collect();
            for f in faces {
                self.faces.get_mut(&f).unwrap().reverse();
            }
        }
        for (face, triangles) in opening_folds {
            self.replace_folded_face(face, triangles);
        }
        self.fold(&vs);
        Ok(vs.len() + inst.len())
    }

    pub fn rotate(&mut self, ids: &[Id], center: DVec3, axis: DVec3, degrees: f64) -> Result<usize> {
        if axis.length() < 1e-12 {
            bail!("rotation axis is zero");
        }
        let q = DQuat::from_axis_angle(axis.normalize(), degrees.to_radians());
        let t = DMat4::from_translation(center) * DMat4::from_quat(q) * DMat4::from_translation(-center);
        self.transform(ids, t)
    }

    pub fn scale(&mut self, ids: &[Id], center: DVec3, factor: DVec3) -> Result<usize> {
        if factor.x.abs() < 1e-9 || factor.y.abs() < 1e-9 || factor.z.abs() < 1e-9 {
            bail!("scale factor can't be zero");
        }
        let t = DMat4::from_translation(center) * DMat4::from_scale(factor) * DMat4::from_translation(-center);
        self.transform(ids, t)
    }

    // Plan on the original plane before touching a vertex. A projection
    // of a bent ring can bridge an opening or change its boundary.
    fn plan_opening_folds(&self, moved: &BTreeSet<Id>, transform: DMat4) -> Result<Vec<(Id, Vec<[Id; 3]>)>> {
        let point = |id| {
            let p = self.p(id);
            if moved.contains(&id) { transform.transform_point3(p) } else { p }
        };
        let mut folds = Vec::new();
        for (&id, face) in &self.faces {
            if face.holes.is_empty() || !face.loops().flatten().any(|v| moved.contains(v)) {
                continue;
            }
            let outer: Vec<_> = face.outer.iter().map(|&v| point(v)).collect();
            let normal = geom::newell(&outer);
            if normal == DVec3::ZERO {
                bail!("transform would collapse a face with an opening");
            }
            if face.loops().flatten().all(|&v| (point(v) - outer[0]).dot(normal).abs() < WELD) {
                continue;
            }
            let triangles = self.fold_openings(face)?;
            for &[a, b, c] in &triangles {
                if (point(b) - point(a)).cross(point(c) - point(a)).length_squared() <= WELD.powi(4) {
                    bail!("transform would collapse part of a face with an opening");
                }
            }
            folds.push((id, triangles));
        }
        Ok(folds)
    }

    /// Split faces touching `moved` that are no longer flat into triangles.
    pub(crate) fn fold(&mut self, moved: &BTreeSet<Id>) {
        let bent: Vec<Id> = self
            .faces
            .iter()
            .filter(|(_, f)| f.loops().flatten().any(|v| moved.contains(v)))
            .map(|(&k, _)| k)
            .filter(|&k| !self.is_flat(k))
            .collect();
        for fid in bent {
            let f = self.faces[&fid].clone();
            if !f.holes.is_empty() {
                continue; // Openings are planned before the transform.
            }
            let pts = self.pts(&f.outer);
            let n = geom::newell(&pts);
            let lookup: HashMap<[i64; 3], Id> = f.outer.iter().map(|&v| (key(self.p(v)), v)).collect();
            let tris = geom::triangulate(&pts, &[], n);
            self.replace_folded_face(fid, tris.into_iter().map(|t| t.map(|p| lookup[&key(p)])));
        }
    }

    /// Triangulate the original flat face and make sure all outline and hole
    /// edges still constrain the fold. Every triangle reuses the model's IDs.
    fn fold_openings(&self, face: &Face) -> Result<Vec<[Id; 3]>> {
        let outer = self.pts(&face.outer);
        let holes: Vec<_> = face.holes.iter().map(|h| self.pts(h)).collect();
        let key = |p: DVec3| p.to_array().map(f64::to_bits);
        let lookup: HashMap<_, _> = face.loops().flatten().map(|&id| (key(self.p(id)), id)).collect();
        let triangles: Vec<_> = geom::triangulate(&outer, &holes, geom::newell(&outer))
            .into_iter().map(|t| t.map(|p| lookup[&key(p)])).collect();
        let boundary: BTreeSet<_> = face.loops().flat_map(|l| pairs(l)).map(|(a, b)| (a.min(b), a.max(b))).collect();
        let mut counts = HashMap::new();
        for tri in &triangles {
            for (a, b) in pairs(tri) {
                *counts.entry((a.min(b), a.max(b))).or_insert(0usize) += 1;
            }
        }
        if !boundary.iter().all(|e| counts.get(e) == Some(&1))
            || counts.into_iter().any(|(e, count)| count != if boundary.contains(&e) { 1 } else { 2 })
        {
            bail!("cannot fold a face without changing its opening");
        }
        Ok(triangles)
    }

    fn replace_folded_face(&mut self, fid: Id, triangles: impl IntoIterator<Item = [Id; 3]>) {
        let f = self.faces.remove(&fid).unwrap();
        let boundary: BTreeSet<_> = f.loops().flat_map(|l| pairs(l)).map(|(a, b)| (a.min(b), a.max(b))).collect();
        for tri in triangles {
            self.insert_face(Face { outer: tri.to_vec(), holes: Vec::new(), uv: None, ..f.clone() });
            for (a, b) in pairs(&tri) {
                if !boundary.contains(&(a.min(b), a.max(b))) {
                    if let Some(e) = self.find_edge(a, b) {
                        self.edges.get_mut(&e).unwrap().soft = true;
                    }
                }
            }
        }
    }

    pub fn is_flat(&self, fid: Id) -> bool {
        let f = &self.faces[&fid];
        let pts: Vec<DVec3> = f.loops().flatten().map(|v| self.p(*v)).collect();
        let n = geom::newell(&self.pts(&f.outer));
        n != DVec3::ZERO && pts.iter().all(|p| (*p - pts[0]).dot(n).abs() < 1e-6)
    }

    /// Copy faces, groups or instances, offset by `by`. Returns the new ids.
    pub fn duplicate(&mut self, ids: &[Id], by: DVec3) -> Result<Vec<Id>> {
        self.check_room(self.copy_cost(ids))?;
        let mut out = Vec::new();
        let mut loose = Vec::new();
        for &id in ids {
            if self.groups.contains_key(&id) {
                out.push(self.copy_group(id, by, None)?);
            } else if let Some(i) = self.instances.get(&id).cloned() {
                let nid = self.new_id();
                self.instances.insert(nid, crate::model::Instance { transform: DMat4::from_translation(by) * i.transform, ..i });
                out.push(nid);
            } else if self.faces.contains_key(&id) {
                loose.push(id);
            } else {
                bail!("{id} can't be copied (pick faces, groups or components)");
            }
        }
        // loose faces: new vertices, so the copy isn't stuck to the original
        let mut map: HashMap<Id, Id> = HashMap::new();
        for fid in loose {
            let f = self.faces[&fid].clone();
            let mut remap = |m: &mut Model, l: &[Id]| -> Vec<Id> {
                l.iter()
                    .map(|v| {
                        *map.entry(*v).or_insert_with(|| {
                            let p = m.p(*v) + by;
                            if by.length() < crate::model::WELD {
                                // a copy in place (to rotate, or paste): its own
                                // vertices, or moving it would drag the original
                                let id = m.new_id();
                                m.vertices.insert(id, crate::model::Vertex { p, group: f.group });
                                id
                            } else {
                                m.vertex_in(p, f.group)
                            }
                        })
                    })
                    .collect()
            };
            let outer = remap(self, &f.outer);
            let holes = f.holes.iter().map(|h| remap(self, h)).collect();
            out.push(self.insert_face(Face { outer, holes, ..f.clone() }));
        }
        Ok(out)
    }

    /// `count` more copies, each `by` further along.
    pub fn array(&mut self, ids: &[Id], by: DVec3, count: usize) -> Result<Vec<Id>> {
        let count = count.min(500);
        if ids.len().saturating_mul(count) > 100_000 {
            bail!("that array would make over 100,000 copies: use fewer items or a smaller count");
        }
        // by what is inside the ids, not how many ids there are
        self.check_room(self.copy_cost(ids).saturating_mul(count))?;
        let mut out = Vec::new();
        for k in 1..=count {
            out.extend(self.duplicate(ids, by * k as f64)?);
        }
        Ok(out)
    }
}

fn key(p: DVec3) -> [i64; 3] {
    [(p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64]
}
