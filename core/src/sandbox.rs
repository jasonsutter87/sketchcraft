//! Sandbox: terrain. A terrain is an ordinary group of triangles with its
//! inside edges softened, so every other tool still works on it.
//!
//! - From Contours: contour lines -> triangulated surface (Delaunay in plan)
//! - From Scratch: a flat grid to sculpt
//! - Smoove: push a soft round area up or down
//! - Stamp: flatten a building pad into the terrain, with a sloped edge
//! - Drape: lay lines (paths, property lines) onto the surface
//! - Flip Edge: swap the diagonal shared by two triangles

use crate::geom;
use crate::mesh::Mesh;
use crate::model::{Id, Model};
use anyhow::{anyhow, bail, Result};
use glam::{DVec2, DVec3};
use std::collections::{BTreeSet, HashMap};

/// Most points a terrain is built from (keeps triangulation and the scene sane).
const MAX_POINTS: usize = 200_000;

impl Model {
    /// The faces of a terrain group (it must be a group of triangles).
    fn terrain_faces(&self, group: Id) -> Result<Vec<Id>> {
        if !self.groups.contains_key(&group) {
            bail!("{group} is not a group (terrains are groups)");
        }
        let fs: Vec<Id> = self.faces.iter().filter(|(_, f)| f.group == Some(group)).map(|(&k, _)| k).collect();
        if fs.is_empty() {
            bail!("that group has no faces");
        }
        Ok(fs)
    }

    /// Build a terrain group from triangles over points (soft inside edges).
    pub(crate) fn terrain_group(&mut self, name: &str, pts: &[DVec3], tris: &[[usize; 3]]) -> Result<Id> {
        let prev = self.active_group;
        let gid = self.begin_group(name.to_string());
        let mut mesh = Mesh::new(self, Some(gid));
        for t in tris {
            let mut tri = [pts[t[0]], pts[t[1]], pts[t[2]]];
            if geom::newell(&tri).z < 0.0 {
                tri.swap(1, 2); // face up
            }
            mesh.face(&tri, &[], None);
        }
        mesh.finish();
        self.active_group = prev;
        self.soften_terrain(gid);
        Ok(gid)
    }

    /// Every edge shared by two faces of the group is soft (smooth ground).
    fn soften_terrain(&mut self, gid: Id) {
        self.soften_terrain_except(gid, |_, _| false);
    }

    /// Same, but edges `keep(a, b)` says to show stay hard (a pad's outline).
    fn soften_terrain_except(&mut self, gid: Id, keep: impl Fn(DVec3, DVec3) -> bool) {
        let mut uses: HashMap<(Id, Id), u32> = HashMap::new();
        for f in self.faces.values().filter(|f| f.group == Some(gid)) {
            for i in 0..f.outer.len() {
                let (a, b) = (f.outer[i], f.outer[(i + 1) % f.outer.len()]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let pos: HashMap<Id, DVec3> = self.vertices.iter().filter(|(_, v)| v.group == Some(gid)).map(|(&k, v)| (k, v.p)).collect();
        for e in self.edges.values_mut().filter(|e| e.group == Some(gid)) {
            let shared = uses.get(&(e.a.min(e.b), e.a.max(e.b))).copied().unwrap_or(0) >= 2;
            let shown = match (pos.get(&e.a), pos.get(&e.b)) {
                (Some(a), Some(b)) => keep(*a, *b),
                _ => false,
            };
            e.soft = shared && !shown;
        }
    }

    /// From Contours: points along the picked edges (and the edges of picked
    /// groups), every `spacing` meters, triangulated in plan.
    pub fn terrain_from_contours(&mut self, ids: &[Id], spacing: f64) -> Result<Id> {
        let spacing = spacing.max(0.05);
        let mut segs: Vec<(DVec3, DVec3)> = Vec::new();
        for id in ids {
            if let Some(e) = self.edges.get(id) {
                segs.push((self.p(e.a), self.p(e.b)));
            } else if self.groups.contains_key(id) {
                segs.extend(self.edges.values().filter(|e| e.group == Some(*id)).map(|e| (self.p(e.a), self.p(e.b))));
            }
        }
        if segs.is_empty() {
            bail!("pick the contour lines (edges, or groups of them)");
        }
        let mut pts: Vec<DVec3> = Vec::new();
        let mut seen: BTreeSet<(i64, i64)> = BTreeSet::new();
        let mut add = |p: DVec3, pts: &mut Vec<DVec3>| {
            // one height per spot in plan
            if seen.insert(((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64)) {
                pts.push(p);
            }
        };
        for (a, b) in &segs {
            let n = ((a.distance(*b) / spacing).ceil() as usize).max(1);
            for k in 0..=n {
                add(a.lerp(*b, k as f64 / n as f64), &mut pts);
            }
            if pts.len() > MAX_POINTS {
                bail!("too many points: use a bigger spacing");
            }
        }
        if pts.len() < 3 {
            bail!("need at least three distinct points");
        }
        let flat: Vec<delaunator::Point> = pts.iter().map(|p| delaunator::Point { x: p.x, y: p.y }).collect();
        let tri = delaunator::triangulate(&flat);
        if tri.triangles.is_empty() {
            bail!("the contours are all in a line; nothing to triangulate");
        }
        let tris: Vec<[usize; 3]> = tri.triangles.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
        self.terrain_group("Terrain", &pts, &tris)
    }

    /// From Scratch: a flat grid `width` x `depth` of `spacing` squares (each
    /// split in two triangles), corner at `at`.
    pub fn terrain_grid(&mut self, at: DVec3, width: f64, depth: f64, spacing: f64) -> Result<Id> {
        if width <= 0.0 || depth <= 0.0 || spacing <= 0.0 {
            bail!("width, depth and spacing must be positive");
        }
        let (nx, ny) = ((width / spacing).round().max(1.0) as usize, (depth / spacing).round().max(1.0) as usize);
        if (nx + 1) * (ny + 1) > MAX_POINTS {
            bail!("that grid is too fine: use a bigger spacing");
        }
        let (sx, sy) = (width / nx as f64, depth / ny as f64);
        let pts: Vec<DVec3> = (0..=ny).flat_map(|j| (0..=nx).map(move |i| at + DVec3::new(i as f64 * sx, j as f64 * sy, 0.0))).collect();
        let idx = |i: usize, j: usize| j * (nx + 1) + i;
        let mut tris = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                tris.push([idx(i, j), idx(i + 1, j), idx(i + 1, j + 1)]);
                tris.push([idx(i, j), idx(i + 1, j + 1), idx(i, j + 1)]);
            }
        }
        self.terrain_group("Terrain", &pts, &tris)
    }

    /// Smoove: raise (or lower, negative `height`) the terrain around `at`
    /// within `radius`, smoothly (full height at the middle, none at the rim).
    pub fn smoove(&mut self, group: Id, at: DVec3, radius: f64, height: f64) -> Result<usize> {
        if radius <= 0.0 {
            bail!("radius must be positive");
        }
        let fs = self.terrain_faces(group)?;
        let vs: BTreeSet<Id> = fs.iter().flat_map(|f| self.faces[f].outer.clone()).collect();
        let mut touched = BTreeSet::new();
        for v in vs {
            let p = self.p(v);
            let d = DVec2::new(p.x - at.x, p.y - at.y).length();
            if d < radius {
                let k = 0.5 * (1.0 + (std::f64::consts::PI * d / radius).cos());
                self.vertices.get_mut(&v).unwrap().p.z += height * k;
                touched.insert(v);
            }
        }
        let moved = touched.len();
        self.fold(&touched);
        self.soften_terrain(group);
        Ok(moved)
    }

    /// Stamp: flatten the terrain under a footprint face to the footprint's
    /// height, blending back to the ground over `offset` meters around it.
    pub fn stamp(&mut self, group: Id, footprint: Id, offset: f64) -> Result<usize> {
        let fp = self.face(footprint)?.outer.clone();
        let pts = self.pts(&fp);
        let z = pts.iter().map(|p| p.z).sum::<f64>() / pts.len() as f64;
        let poly: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
        // cut the footprint's outline into the terrain first, so the pad has
        // edges of its own even where the terrain is coarse
        let outline: Vec<(DVec3, DVec3)> = (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect();
        self.drape_segments(group, &outline)?;
        let fs = self.terrain_faces(group)?;
        let vs: BTreeSet<Id> = fs.iter().flat_map(|f| self.faces[f].outer.clone()).collect();
        let dist = |q: DVec2| -> f64 {
            (0..poly.len())
                .map(|i| {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    let t = ((q - a).dot(b - a) / (b - a).length_squared().max(1e-18)).clamp(0.0, 1.0);
                    q.distance(a + (b - a) * t)
                })
                .fold(f64::MAX, f64::min)
        };
        let mut touched = BTreeSet::new();
        for v in vs {
            let p = self.p(v);
            let q = p.truncate();
            let new_z = if geom::inside_or_on(q, &poly) {
                z
            } else {
                let d = dist(q);
                if offset <= 0.0 || d >= offset {
                    continue;
                }
                let k = 0.5 * (1.0 + (std::f64::consts::PI * d / offset).cos());
                p.z + (z - p.z) * k
            };
            self.vertices.get_mut(&v).unwrap().p.z = new_z;
            touched.insert(v);
        }
        let moved = touched.len();
        self.fold(&touched);
        // the pad's outline stays visible
        let on_outline = |p: DVec2| {
            (0..poly.len()).any(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let t = ((p - a).dot(b - a) / (b - a).length_squared().max(1e-18)).clamp(0.0, 1.0);
                p.distance(a + (b - a) * t) < 1e-5
            })
        };
        self.soften_terrain_except(group, |a, b| on_outline(a.truncate()) && on_outline(b.truncate()) && on_outline((a.truncate() + b.truncate()) / 2.0));
        Ok(moved)
    }

    /// Drape: lay the picked edges onto the terrain (straight down), drawn
    /// into the terrain so they split its faces.
    pub fn drape(&mut self, group: Id, ids: &[Id]) -> Result<usize> {
        let segs: Vec<(DVec3, DVec3)> = ids.iter().filter_map(|id| self.edges.get(id)).map(|e| (self.p(e.a), self.p(e.b))).collect();
        if segs.is_empty() {
            bail!("pick the edges to drape");
        }
        self.drape_segments(group, &segs)
    }

    fn drape_segments(&mut self, group: Id, segs: &[(DVec3, DVec3)]) -> Result<usize> {
        let fs = self.terrain_faces(group)?;
        let world: Vec<_> = self.world_faces().into_iter().filter(|f| f.face.map_or(false, |id| fs.contains(&id))).collect();
        let mut pieces: Vec<(DVec3, DVec3)> = Vec::new();
        for &(a, b) in segs {
            let along = DVec3::new(b.x - a.x, b.y - a.y, 0.0);
            if along.length() < 1e-9 {
                continue;
            }
            // the vertical plane through the edge
            let n = DVec3::Z.cross(along).normalize();
            let w = n.dot(a);
            let (t0, t1) = (a.dot(along.normalize()), b.dot(along.normalize()));
            let dir = along.normalize();
            for f in &world {
                for (p, q) in crate::intersect::plane_crossing(f, n, w) {
                    // keep the part over the edge itself
                    let (tp, tq) = (p.dot(dir), q.dot(dir));
                    let (lo, hi) = (tp.min(tq).max(t0.min(t1)), tp.max(tq).min(t0.max(t1)));
                    if hi - lo < 1e-6 {
                        continue;
                    }
                    let at = |t: f64| p + (q - p) * ((t - tp) / (tq - tp));
                    pieces.push((at(lo), at(hi)));
                }
            }
        }
        let prev = self.active_group;
        self.active_group = Some(group);
        let mut drawn = 0;
        for (p, q) in pieces {
            if self.draw_line(p, q).is_ok() {
                drawn += 1;
            }
        }
        self.active_group = prev;
        Ok(drawn)
    }

    /// Flip Edge: the two triangles sharing `edge` swap to the other diagonal.
    pub fn flip_edge(&mut self, edge: Id) -> Result<()> {
        let e = self.edges.get(&edge).ok_or_else(|| anyhow!("{edge} is not an edge"))?.clone();
        let fs = self.faces_using_edge(e.a, e.b);
        if fs.len() != 2 || fs.iter().any(|f| self.faces[f].outer.len() != 3 || !self.faces[f].holes.is_empty()) {
            bail!("pick an edge between two triangles");
        }
        let other = |f: Id| *self.faces[&f].outer.iter().find(|v| **v != e.a && **v != e.b).unwrap();
        let (c, d) = (other(fs[0]), other(fs[1]));
        let template = self.faces[&fs[0]].clone();
        let up = self.normal(fs[0]);
        for f in &fs {
            self.faces.remove(f);
        }
        self.edges.remove(&edge);
        for tri in [[c, d, e.a], [d, c, e.b]] {
            let mut t = tri.to_vec();
            if geom::newell(&self.pts(&t)).dot(up) < 0.0 {
                t.reverse();
            }
            self.insert_face(crate::model::Face { outer: t, holes: vec![], ..template.clone() });
        }
        // the new diagonal is as soft as the old one was
        if let Some(id) = self.find_edge(c, d) {
            self.edges.get_mut(&id).unwrap().soft = e.soft;
        }
        Ok(())
    }
}
