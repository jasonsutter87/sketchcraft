//! Bulk mesh building for imports. Drawing tools weld, split and merge as
//! they go, which means looking through the whole model for every point;
//! fine by hand, far too slow for a 200,000-triangle car. A finished mesh
//! needs none of that: points weld through a hash, edges through a map, and
//! faces go straight in. Smoothing is worked out once at the end.

use crate::geom;
use crate::model::{Edge, Face, Id, Model, Vertex};
use glam::DVec3;
use std::collections::HashMap;

pub struct Mesh<'a> {
    pub m: &'a mut Model,
    group: Option<Id>,
    points: HashMap<(i64, i64, i64), Id>,
    edges: HashMap<(Id, Id), Id>,
    /// loops already made into faces (sorted ids): exporters often write
    /// each triangle twice, once facing each way
    seen: std::collections::HashSet<Vec<Id>>,
    pub faces: usize,
    pub skipped: usize,
}

/// Points closer than this (meters) become one vertex.
const CELL: f64 = 1e-5;

impl<'a> Mesh<'a> {
    /// Build into `m` (its existing contents aren't welded to).
    pub fn new(m: &'a mut Model, group: Option<Id>) -> Self {
        Mesh { m, group, points: HashMap::new(), edges: HashMap::new(), seen: Default::default(), faces: 0, skipped: 0 }
    }

    pub fn vertex(&mut self, p: DVec3) -> Id {
        let key = ((p.x / CELL).round() as i64, (p.y / CELL).round() as i64, (p.z / CELL).round() as i64);
        if let Some(&id) = self.points.get(&key) {
            return id;
        }
        let id = self.m.new_id();
        self.m.vertices.insert(id, Vertex { p, group: self.group });
        self.points.insert(key, id);
        id
    }

    fn edge(&mut self, a: Id, b: Id) -> Id {
        let key = (a.min(b), a.max(b));
        if let Some(&id) = self.edges.get(&key) {
            return id;
        }
        let id = self.m.new_id();
        self.m.edges.insert(id, Edge { a, b, group: self.group, ..Default::default() });
        self.edges.insert(key, id);
        id
    }

    /// A loose line (kept, e.g. soft seams or wires).
    pub fn line(&mut self, a: DVec3, b: DVec3, soft: bool) {
        let (va, vb) = (self.vertex(a), self.vertex(b));
        if va != vb {
            let e = self.edge(va, vb);
            if soft {
                self.m.edges.get_mut(&e).unwrap().soft = true;
            }
        }
    }

    /// A polygon with optional holes. Not flat: split into a triangle fan.
    pub fn face(&mut self, outer: &[DVec3], holes: &[Vec<DVec3>], material: Option<String>) {
        let n = geom::newell(outer);
        if n == DVec3::ZERO {
            self.skipped += 1;
            return;
        }
        let c = geom::centroid(outer);
        let flat = outer.iter().all(|p| (*p - c).dot(n).abs() < 1e-4);
        if !flat && outer.len() > 3 {
            for k in 1..outer.len() - 1 {
                self.face(&[outer[0], outer[k], outer[k + 1]], &[], material.clone());
            }
            return;
        }
        let mut ring: Vec<Id> = outer.iter().map(|p| self.vertex(*p)).collect();
        ring.dedup();
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        if ring.len() < 3 {
            self.skipped += 1;
            return;
        }
        let mut key = ring.clone();
        key.sort_unstable();
        if holes.is_empty() && !self.seen.insert(key) {
            return; // the same face again (often the back of a double-sided one)
        }
        let holes: Vec<Vec<Id>> = holes
            .iter()
            .filter(|h| h.len() >= 3)
            .map(|h| {
                let mut r: Vec<Id> = h.iter().map(|p| self.vertex(*p)).collect();
                if geom::newell(h).dot(n) > 0.0 {
                    r.reverse(); // holes run clockwise about the normal
                }
                r
            })
            .collect();
        for l in std::iter::once(&ring).chain(holes.iter()) {
            for i in 0..l.len() {
                self.edge(l[i], l[(i + 1) % l.len()]);
            }
        }
        let id = self.m.new_id();
        self.m.faces.insert(id, Face { outer: ring, holes, material, group: self.group, hidden: false });
        self.faces += 1;
    }

    /// Soften edges between faces that meet at less than 35 degrees, so
    /// curved surfaces read as smooth instead of a net of triangles.
    pub fn finish(self) -> (usize, usize) {
        let m = self.m;
        let normal: HashMap<Id, DVec3> = m.faces.iter().map(|(&k, f)| (k, geom::newell(&m.pts(&f.outer)).normalize_or_zero())).collect();
        let mut users: HashMap<(Id, Id), Vec<Id>> = HashMap::new();
        for (&fid, f) in &m.faces {
            for l in f.loops() {
                for i in 0..l.len() {
                    let (a, b) = (l[i], l[(i + 1) % l.len()]);
                    users.entry((a.min(b), a.max(b))).or_default().push(fid);
                }
            }
        }
        let cos = 35f64.to_radians().cos();
        for (&key, &eid) in &self.edges {
            if let Some(fs) = users.get(&key) {
                if fs.len() == 2 {
                    // abs: neighbors wound the other way are still smooth
                    let d = normal[&fs[0]].dot(normal[&fs[1]]).abs();
                    if d > cos {
                        m.edges.get_mut(&eid).unwrap().soft = true;
                    }
                }
            }
        }
        (self.faces, self.skipped)
    }
}
