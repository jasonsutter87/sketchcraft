//! Move, rotate, scale, copy and array, for any mix of faces, edges,
//! vertices, groups and component instances.
//!
//! Moving part of a solid can bend a face out of flat (pull one corner of a
//! roof up). Like other push/pull modelers we "auto-fold": such a face is
//! split into flat triangles joined by soft edges.

use crate::geom;
use crate::model::{pairs, Face, Id, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DQuat, DVec3};
use std::collections::{BTreeSet, HashMap};

impl Model {
    /// Apply `t` to everything named in `ids`. Returns how many vertices and
    /// instances moved.
    pub fn transform(&mut self, ids: &[Id], t: DMat4) -> Result<usize> {
        let (inst, geo): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| self.instances.contains_key(id));
        for id in &inst {
            let i = self.instances.get_mut(id).unwrap();
            i.transform = t * i.transform;
        }
        let vs = if geo.is_empty() { BTreeSet::new() } else { self.vertices_of(&geo)? };
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

    /// Split faces touching `moved` that are no longer flat into triangles.
    fn fold(&mut self, moved: &BTreeSet<Id>) {
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
                continue; // rare; leave it rather than guess
            }
            let pts = self.pts(&f.outer);
            let n = geom::newell(&pts);
            let lookup: HashMap<[i64; 3], Id> = f.outer.iter().map(|&v| (key(self.p(v)), v)).collect();
            let tris = geom::triangulate(&pts, &[], n);
            self.faces.remove(&fid);
            let boundary: BTreeSet<(Id, Id)> = pairs(&f.outer).flat_map(|(a, b)| [(a, b), (b, a)]).collect();
            for t in tris {
                let ids: Vec<Id> = t.iter().map(|p| lookup[&key(*p)]).collect();
                let mut tri = ids.clone();
                // keep each triangle facing the same way as the face it came from
                let tn = (t[1] - t[0]).cross(t[2] - t[0]);
                if tn.dot(n) < 0.0 {
                    tri.reverse();
                }
                self.insert_face(Face { outer: tri.clone(), ..f.clone() });
                for (a, b) in pairs(&tri) {
                    if !boundary.contains(&(a, b)) {
                        if let Some(e) = self.find_edge(a, b) {
                            self.edges.get_mut(&e).unwrap().soft = true;
                        }
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
        let mut out = Vec::new();
        for k in 1..=count.min(500) {
            out.extend(self.duplicate(ids, by * k as f64)?);
        }
        Ok(out)
    }
}

fn key(p: DVec3) -> [i64; 3] {
    [(p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64]
}
