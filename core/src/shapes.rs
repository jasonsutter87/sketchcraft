//! Shape tools: circle / regular polygon, arc, offset, follow-me.

use crate::geom;
use crate::model::{Face, Id, Model};
use anyhow::{bail, Result};
use glam::{DVec2, DVec3};
use std::f64::consts::TAU;

impl Model {
    /// A circle (or regular polygon with `segments` sides) as a face.
    pub fn circle(&mut self, center: DVec3, normal: DVec3, radius: f64, segments: usize) -> Result<Id> {
        if radius <= 0.0 {
            bail!("radius must be positive");
        }
        let n = normal.normalize_or_zero();
        if n == DVec3::ZERO {
            bail!("normal is zero");
        }
        let (u, v) = geom::basis(n);
        let k = segments.clamp(3, 360);
        let pts: Vec<DVec3> = (0..k)
            .map(|i| {
                let a = TAU * i as f64 / k as f64;
                center + (u * a.cos() + v * a.sin()) * radius
            })
            .collect();
        self.add_polygon(&pts)
    }

    /// Points of an arc from `start` to `end` bowing out by `bulge` (the
    /// sagitta; negative bows the other way) in the plane with `normal`.
    pub fn arc_points(start: DVec3, end: DVec3, bulge: f64, normal: DVec3, segments: usize) -> Result<Vec<DVec3>> {
        let chord = end - start;
        let l = chord.length();
        if l < 1e-9 {
            bail!("arc ends are the same point");
        }
        let n = normal.normalize();
        let side = n.cross(chord / l);
        if side.length() < 1e-9 {
            bail!("arc normal is along the chord");
        }
        if bulge.abs() < 1e-9 {
            return Ok(vec![start, end]);
        }
        let s = bulge.abs();
        let r = (l * l / 4.0 + s * s) / (2.0 * s);
        let dir = side * bulge.signum();
        let mid = (start + end) * 0.5;
        let center = mid + dir * (s - r);
        let (a, b) = ((start - center).normalize(), (end - center).normalize());
        let axis = a.cross(dir).normalize_or_zero();
        let axis = if axis == DVec3::ZERO { n } else { axis };
        // sweep from a to b passing through the apex
        let mut sweep = a.angle_between(b);
        if s > r {
            sweep = TAU - sweep;
        }
        let k = segments.clamp(2, 360);
        let q_of = |t: f64| glam::DQuat::from_axis_angle(axis, sweep * t);
        let mut pts: Vec<DVec3> = (0..=k).map(|i| center + q_of(i as f64 / k as f64) * (start - center)).collect();
        // the chosen axis might sweep the wrong way: check we reach `end`
        if pts.last().unwrap().distance(end) > 1e-6 {
            pts = (0..=k)
                .map(|i| center + glam::DQuat::from_axis_angle(-axis, sweep * i as f64 / k as f64) * (start - center))
                .collect();
        }
        *pts.last_mut().unwrap() = end;
        pts[0] = start;
        Ok(pts)
    }

    /// Offset a face's outline: positive = inward (a new face inside it, like
    /// a window frame), negative = outward (a ring around it, like a curb).
    pub fn offset(&mut self, fid: Id, d: f64) -> Result<Id> {
        let f = self.face(fid)?.clone();
        let pts = self.pts(&f.outer);
        let n = geom::newell(&pts);
        let (u, v) = geom::basis(n);
        let depth = pts[0].dot(n);
        let p2 = geom::to2d(&pts, u, v);
        let k = p2.len();
        let left = |e: DVec2| DVec2::new(-e.y, e.x).normalize();
        let moved: Vec<DVec3> = (0..k)
            .map(|i| {
                let (a, b, c) = (p2[(i + k - 1) % k], p2[i], p2[(i + 1) % k]);
                let (n1, n2) = (left(b - a), left(c - b));
                let m = (n1 + n2) * (d / (1.0 + n1.dot(n2)).max(0.2));
                let q = b + m;
                u * q.x + v * q.y + n * depth
            })
            .collect();
        if d > 0.0 {
            return self.add_polygon(&moved);
        }
        let outer: Vec<Id> = moved.iter().map(|p| self.vertex_in(*p, f.group)).collect();
        let mut hole = f.outer.clone();
        hole.reverse();
        Ok(self.insert_face(Face { outer, holes: vec![hole], material: f.material.clone(), group: f.group, ..Default::default() }))
    }

    /// Sweep a profile face along a path (cornice along a roof edge, a handrail,
    /// a curb). `closed` paths connect back to the start. Returns the new faces.
    pub fn follow_me(&mut self, fid: Id, path: &[DVec3], closed: bool) -> Result<Vec<Id>> {
        let f = self.face(fid)?.clone();
        let mut w: Vec<DVec3> = path.to_vec();
        w.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if closed && w.len() > 2 && w.first().unwrap().distance(*w.last().unwrap()) < 1e-9 {
            w.pop();
        }
        if w.len() < 2 {
            bail!("the path needs at least two points");
        }
        let prof = self.pts(&f.outer);
        let pn = geom::newell(&prof);
        let pc = geom::centroid(&prof);
        if closed {
            // start at the path corner nearest the profile, heading the way it faces
            let i0 = (0..w.len()).min_by(|&i, &j| w[i].distance(pc).partial_cmp(&w[j].distance(pc)).unwrap()).unwrap();
            w.rotate_left(i0);
            let fwd = (w[1] - w[0]).normalize();
            let back = (w[w.len() - 1] - w[0]).normalize();
            if back.dot(pn).abs() > fwd.dot(pn).abs() {
                w[1..].reverse();
            }
        } else if w.last().unwrap().distance(pc) < w[0].distance(pc) {
            w.reverse();
        }
        let m = w.len();
        let seg = |i: usize| (w[(i + 1) % m] - w[i]).normalize();
        let nseg = if closed { m } else { m - 1 };
        let miter = |k: usize| -> DVec3 {
            // plane normal at path point k
            if closed {
                (seg((k + m - 1) % m) + seg(k % m)).normalize()
            } else if k == 0 {
                seg(0)
            } else if k == m - 1 {
                seg(m - 2)
            } else {
                (seg(k - 1) + seg(k)).normalize()
            }
        };
        let project = |pts: &[DVec3], dir: DVec3, origin: DVec3, plane_n: DVec3| -> Vec<DVec3> {
            pts.iter()
                .map(|p| {
                    let t = (origin - *p).dot(plane_n) / dir.dot(plane_n);
                    *p + dir * t
                })
                .collect()
        };
        // rings of profile points at every path corner
        let mut rings: Vec<Vec<DVec3>> = Vec::new();
        let first = if closed { project(&prof, seg(0), w[0], miter(0)) } else { prof.clone() };
        rings.push(first);
        for k in 0..nseg {
            let next = (k + 1) % m;
            let ring = project(rings.last().unwrap(), seg(k), w[next], if closed && next == 0 { miter(0) } else { miter(next) });
            rings.push(ring);
        }
        if closed {
            rings.pop(); // the last ring is the first
        }
        let sign = pn.dot(seg(0));
        let g = f.group;
        let ids: Vec<Vec<Id>> = rings.iter().map(|r| r.iter().map(|p| self.vertex_in(*p, g)).collect()).collect();
        let mut out = Vec::new();
        let nr = ids.len();
        let steps = if closed { nr } else { nr - 1 };
        let np = prof.len();
        for k in 0..steps {
            let (r0, r1) = (&ids[k], &ids[(k + 1) % nr]);
            for i in 0..np {
                let j = (i + 1) % np;
                let quad = if sign > 0.0 { vec![r0[i], r0[j], r1[j], r1[i]] } else { vec![r0[i], r1[i], r1[j], r0[j]] };
                out.push(self.insert_face(Face { outer: quad, material: f.material.clone(), group: g, ..Default::default() }));
            }
        }
        if closed {
            self.faces.remove(&fid);
        } else {
            let mut end = Face { outer: ids[nr - 1].clone(), material: f.material.clone(), group: g, ..Default::default() };
            if sign < 0.0 {
                end.reverse();
            }
            out.push(self.insert_face(end));
            let start = self.faces.get_mut(&fid).unwrap();
            if sign > 0.0 {
                start.reverse();
            }
        }
        self.soften_smooth(&out);
        Ok(out)
    }

    /// Edges between nearly parallel neighbouring faces (a cylinder's sides)
    /// become soft, so curved shapes read as smooth.
    pub fn soften_smooth(&mut self, faces: &[Id]) {
        let set: std::collections::BTreeSet<Id> = faces.iter().copied().collect();
        let edges: Vec<(Id, Id, Id)> = self.edges.iter().map(|(&k, e)| (k, e.a, e.b)).collect();
        for (k, a, b) in edges {
            let fs = self.faces_using_edge(a, b);
            if fs.len() == 2 && fs.iter().any(|f| set.contains(f)) {
                let (n1, n2) = (self.normal(fs[0]), self.normal(fs[1]));
                if n1.dot(n2) > 0.0f64.max((25.0f64).to_radians().cos()) && n1.dot(n2) < 1.0 - 1e-9 {
                    self.edges.get_mut(&k).unwrap().soft = true;
                }
            }
        }
    }

    /// Ordered points along a chain of edges.
    pub fn edge_path(&self, edges: &[Id]) -> Result<(Vec<DVec3>, bool)> {
        let mut segs: Vec<(Id, Id)> = Vec::new();
        for e in edges {
            let Some(ed) = self.edges.get(e) else { bail!("{e} is not an edge") };
            segs.push((ed.a, ed.b));
        }
        if segs.is_empty() {
            bail!("no path edges");
        }
        let mut chain = vec![segs[0].0, segs[0].1];
        segs.remove(0);
        while !segs.is_empty() {
            let last = *chain.last().unwrap();
            let first = chain[0];
            if let Some(i) = segs.iter().position(|(a, b)| *a == last || *b == last) {
                let (a, b) = segs.remove(i);
                chain.push(if a == last { b } else { a });
            } else if let Some(i) = segs.iter().position(|(a, b)| *a == first || *b == first) {
                let (a, b) = segs.remove(i);
                chain.insert(0, if a == first { b } else { a });
            } else {
                bail!("path edges aren't connected");
            }
        }
        let closed = chain.len() > 2 && chain.first() == chain.last();
        if closed {
            chain.pop();
        }
        Ok((self.pts(&chain), closed))
    }
}
