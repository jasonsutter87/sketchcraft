//! Drawing with lines, the way a push/pull modeler does it:
//!
//!  - A line ending on the middle of an edge splits that edge there.
//!  - A line drawn across a face, edge to edge, cuts the face in two
//!    (that's how you make a roof ridge or divide a wall into floors).
//!  - A line that closes a flat loop of lines makes a face.

use crate::geom;
use crate::model::{Face, Id, Model, WELD};
use anyhow::Result;
use glam::DVec3;
use std::collections::{BTreeSet, HashMap};

const ON_EDGE: f64 = 1e-6;

pub struct LineResult {
    pub edges: Vec<Id>,
    pub faces: Vec<Id>,
}

impl Model {
    /// Edge (in the active group) whose interior contains `p`.
    pub fn edge_through(&self, p: DVec3) -> Option<Id> {
        let g = self.active_group;
        self.edges.iter().find_map(|(&id, e)| {
            if e.group != g {
                return None;
            }
            let (a, b) = (self.p(e.a), self.p(e.b));
            let ab = b - a;
            let t = (p - a).dot(ab) / ab.length_squared();
            (t > 1e-9 && t < 1.0 - 1e-9 && (a + ab * t).distance(p) < ON_EDGE).then_some(id)
        })
    }

    /// Insert a vertex at `p` on edge `eid` (and in every face loop using it).
    pub fn split_edge(&mut self, eid: Id, p: DVec3) -> Id {
        let e = self.edges[&eid].clone();
        let v = self.vertex_in(p, e.group);
        if v == e.a || v == e.b {
            return v;
        }
        self.edges.remove(&eid);
        for (x, y) in [(e.a, v), (v, e.b)] {
            let id = self.edge(x, y);
            let ne = self.edges.get_mut(&id).unwrap();
            ne.soft = e.soft;
            ne.hidden = e.hidden;
        }
        for f in self.faces.values_mut() {
            for l in std::iter::once(&mut f.outer).chain(f.holes.iter_mut()) {
                let n = l.len();
                if let Some(i) = (0..n).find(|&i| {
                    let (x, y) = (l[i], l[(i + 1) % n]);
                    (x == e.a && y == e.b) || (x == e.b && y == e.a)
                }) {
                    l.insert(i + 1, v);
                }
            }
        }
        v
    }

    /// A vertex at `p`: an existing one, one splitting an edge, or a new one.
    pub fn vertex_on(&mut self, p: DVec3) -> Id {
        let g = self.active_group;
        if let Some((&id, _)) = self.vertices.iter().find(|(_, v)| v.group == g && v.p.distance(p) < WELD) {
            return id;
        }
        match self.edge_through(p) {
            Some(e) => self.split_edge(e, p),
            None => self.vertex(p),
        }
    }

    /// Draw a straight line. Splits edges it lands on, cuts faces it crosses
    /// edge-to-edge, and makes faces for loops it closes.
    pub fn draw_line(&mut self, p: DVec3, q: DVec3) -> Result<LineResult> {
        let a = self.vertex_on(p);
        let b = self.vertex_on(q);
        let mut out = LineResult { edges: vec![], faces: vec![] };
        if a == b {
            return Ok(out);
        }
        out.edges.push(self.edge(a, b));

        // a chord across an existing face?
        let mid = (self.p(a) + self.p(b)) * 0.5;
        let chord = self.faces.iter().find_map(|(&id, f)| {
            let n = f.outer.len();
            let ia = f.outer.iter().position(|&v| v == a)?;
            let ib = f.outer.iter().position(|&v| v == b)?;
            let adjacent = (ia + 1) % n == ib || (ib + 1) % n == ia;
            (!adjacent && self.contains(id, mid)).then_some((id, ia, ib))
        });
        if let Some((fid, ia, ib)) = chord {
            let (f1, f2) = self.split_face(fid, ia, ib);
            out.faces.extend([f1, f2]);
            return Ok(out);
        }

        // a closed flat loop?
        out.faces.extend(self.close_loops(a, b));
        Ok(out)
    }

    /// Cut face `fid` along the chord between outer-loop positions ia and ib.
    fn split_face(&mut self, fid: Id, ia: usize, ib: usize) -> (Id, Id) {
        let f = self.faces[&fid].clone();
        let n = f.outer.len();
        let walk = |from: usize, to: usize| -> Vec<Id> {
            let mut l = vec![];
            let mut i = from;
            loop {
                l.push(f.outer[i]);
                if i == to {
                    break;
                }
                i = (i + 1) % n;
            }
            l
        };
        let (l1, l2) = (walk(ia, ib), walk(ib, ia));
        let normal = self.normal(fid);
        let (u, v) = geom::basis(normal);
        let poly1 = geom::to2d(&self.pts(&l1), u, v);
        let (mut h1, mut h2) = (vec![], vec![]);
        for h in &f.holes {
            let q = geom::to2d(&[self.p(h[0])], u, v)[0];
            if geom::inside(q, &poly1) { h1.push(h.clone()) } else { h2.push(h.clone()) }
        }
        self.faces.insert(fid, Face { outer: l1, holes: h1, ..f.clone() });
        let other = self.insert_face(Face { outer: l2, holes: h2, ..f });
        (fid, other)
    }

    /// Faces for the smallest flat loops through the new edge a-b: one on each
    /// side when the line divides a region.
    fn close_loops(&mut self, a: Id, b: Id) -> Vec<Id> {
        let g = self.active_group;
        let mut adj: HashMap<Id, Vec<Id>> = HashMap::new();
        for e in self.edges.values().filter(|e| e.group == g) {
            adj.entry(e.a).or_default().push(e.b);
            adj.entry(e.b).or_default().push(e.a);
        }
        // simple paths b -> a, not using a-b directly, up to 14 edges
        let mut cycles: Vec<Vec<Id>> = Vec::new();
        let mut path = vec![b];
        let mut seen: BTreeSet<Id> = [a, b].into_iter().collect();
        fn dfs(
            m: &Model,
            adj: &HashMap<Id, Vec<Id>>,
            a: Id,
            path: &mut Vec<Id>,
            seen: &mut BTreeSet<Id>,
            out: &mut Vec<Vec<Id>>,
            budget: &mut usize,
        ) {
            if *budget == 0 || path.len() > 14 || out.len() > 64 {
                return;
            }
            *budget -= 1;
            let cur = *path.last().unwrap();
            for &nx in adj.get(&cur).map(|v| v.as_slice()).unwrap_or(&[]) {
                if nx == a && path.len() >= 2 {
                    let mut cyc = vec![a];
                    cyc.extend(path.iter().copied());
                    out.push(cyc);
                    continue;
                }
                if seen.contains(&nx) {
                    continue;
                }
                // stay on one plane once three points fix it
                let mut pts = vec![m.p(a)];
                pts.extend(path.iter().map(|v| m.p(*v)));
                pts.push(m.p(nx));
                if !planar(&pts) {
                    continue;
                }
                seen.insert(nx);
                path.push(nx);
                dfs(m, adj, a, path, seen, out, budget);
                path.pop();
                seen.remove(&nx);
            }
        }
        let mut budget = 20_000;
        dfs(self, &adj, a, &mut path, &mut seen, &mut cycles, &mut budget);

        // valid: flat, not already a face, no shortcut edge inside (keeps it minimal)
        let mut cands: Vec<(f64, Vec<Id>)> = cycles
            .into_iter()
            .filter(|c| {
                let pts = self.pts(c);
                geom::area(&pts) > 1e-9 && planar(&pts) && !self.faces.values().any(|f| same(&f.outer, c))
            })
            .map(|c| (geom::area(&self.pts(&c)), c))
            .collect();
        cands.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

        let mut made: Vec<Id> = Vec::new();
        let mut used: Vec<Vec<DVec3>> = Vec::new();
        for (_, c) in cands {
            if made.len() == 2 {
                break;
            }
            let pts = self.pts(&c);
            let n = geom::newell(&pts);
            let (u, v) = geom::basis(n);
            let mid = geom::to2d(&[interior_point(&pts, n)], u, v)[0];
            let poly = geom::to2d(&pts, u, v);
            // must not overlap a loop we already used (other side of the line only)
            let overlaps = used.iter().any(|o| {
                let op = geom::to2d(o, u, v);
                geom::inside(mid, &op) || geom::inside(geom::to2d(&[interior_point(o, n)], u, v)[0], &poly)
            });
            // and must not cover an existing coplanar face
            let covers = self.faces.values().any(|f| {
                let fp = self.pts(&f.outer);
                geom::newell(&fp).dot(n).abs() > 1.0 - 1e-9
                    && (fp[0] - pts[0]).dot(n).abs() < 1e-6
                    && geom::inside(geom::to2d(&[interior_point(&fp, n)], u, v)[0], &poly)
            });
            if overlaps || covers {
                continue;
            }
            if let Ok(id) = self.add_face(c.clone()) {
                made.push(id);
                used.push(pts);
            }
        }
        made
    }
}

fn same(a: &[Id], b: &[Id]) -> bool {
    a.len() == b.len() && a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>()
}

fn planar(pts: &[DVec3]) -> bool {
    let n = geom::newell_raw(pts);
    if n.length() < 1e-12 {
        return true; // still collinear: any plane works
    }
    let n = n.normalize();
    pts.iter().all(|p| (*p - pts[0]).dot(n).abs() < 1e-6)
}

/// A point strictly inside a planar loop (centroid if it works, else a nudge
/// in from the middle of the first edge).
pub fn interior_point(pts: &[DVec3], n: DVec3) -> DVec3 {
    let (u, v) = geom::basis(n);
    let poly = geom::to2d(pts, u, v);
    let c = geom::centroid(pts);
    if geom::inside(geom::to2d(&[c], u, v)[0], &poly) {
        return c;
    }
    for (i, j) in pairs_idx(pts.len()) {
        let m = (pts[i] + pts[j]) * 0.5;
        let inward = n.cross(pts[j] - pts[i]).normalize() * 1e-4;
        let q = m + inward;
        if geom::inside(geom::to2d(&[q], u, v)[0], &poly) {
            return q;
        }
    }
    c
}

fn pairs_idx(n: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..n).map(move |i| (i, (i + 1) % n))
}
