//! Intersect Faces and Flip Along.
//!
//! Intersect: where two faces cross, draw the line where they meet into
//! each face's own context (the model or its group), which splits the
//! faces there. "With model" checks the selection against everything
//! visible; "with selection" only against itself.

use crate::components::WFace;
use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::{DVec2, DVec3};

/// Does this world face belong to one of `ids` (a face, a group, an instance)?
fn picked(f: &WFace, ids: &[Id]) -> bool {
    ids.iter().any(|&id| f.owner == id || f.group == Some(id) || f.face == Some(id))
}

/// Parameter intervals of the line `o + d t` inside a face (outline minus holes).
fn inside_intervals(f: &WFace, o: DVec3, d: DVec3) -> Vec<(f64, f64)> {
    let n = f.normal;
    let (u, v) = geom::basis(n);
    let to2 = |p: DVec3| DVec2::new(p.dot(u), p.dot(v));
    let (o2, d2) = (to2(o), DVec2::new(d.dot(u), d.dot(v)));
    if d2.length_squared() < 1e-18 {
        return vec![];
    }
    let mut ts = Vec::new();
    for l in std::iter::once(&f.outer).chain(f.holes.iter()) {
        for i in 0..l.len() {
            let (a, b) = (to2(l[i]), to2(l[(i + 1) % l.len()]));
            let e = b - a;
            let den = d2.perp_dot(e);
            if den.abs() < 1e-15 {
                continue; // parallel to this edge
            }
            let w = a - o2;
            let t = w.perp_dot(e) / den;
            let s = w.perp_dot(d2) / den;
            if (-1e-9..=1.0 + 1e-9).contains(&s) {
                ts.push(t);
            }
        }
    }
    ts.sort_by(|a, b| a.total_cmp(b));
    ts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let outer: Vec<DVec2> = f.outer.iter().map(|p| to2(*p)).collect();
    let holes: Vec<Vec<DVec2>> = f.holes.iter().map(|h| h.iter().map(|p| to2(*p)).collect()).collect();
    let mut out: Vec<(f64, f64)> = Vec::new();
    for w in ts.windows(2) {
        let mid = o2 + d2 * ((w[0] + w[1]) / 2.0);
        if geom::inside(mid, &outer) && !holes.iter().any(|h| geom::inside(mid, h)) {
            match out.last_mut() {
                Some(last) if (last.1 - w[0]).abs() < 1e-9 => last.1 = w[1],
                _ => out.push((w[0], w[1])),
            }
        }
    }
    out
}

/// The pieces of line where two faces cross.
pub fn face_crossing(a: &WFace, b: &WFace) -> Vec<(DVec3, DVec3)> {
    let (n1, n2) = (a.normal.normalize_or_zero(), b.normal.normalize_or_zero());
    let d = n1.cross(n2);
    if d.length_squared() < 1e-12 {
        return vec![]; // parallel or coplanar
    }
    let (w1, w2) = (n1.dot(a.outer[0]), n2.dot(b.outer[0]));
    let o = (n2.cross(d) * w1 + d.cross(n1) * w2) / d.length_squared();
    let d = d.normalize();
    let (ia, ib) = (inside_intervals(a, o, d), inside_intervals(b, o, d));
    let mut out = Vec::new();
    for (a0, a1) in &ia {
        for (b0, b1) in &ib {
            let (t0, t1) = (a0.max(*b0), a1.min(*b1));
            if t1 - t0 > 1e-6 {
                out.push((o + d * t0, o + d * t1));
            }
        }
    }
    out
}

impl Model {
    /// Intersect the picked faces / groups with the model (or only each
    /// other). Returns how many lines were drawn.
    pub fn intersect(&mut self, ids: &[Id], with_model: bool) -> Result<usize> {
        let all = self.world_faces();
        let mine: Vec<&WFace> = all.iter().filter(|f| picked(f, ids)).collect();
        if mine.is_empty() {
            bail!("pick the faces, groups or components to intersect");
        }
        let others: Vec<&WFace> = if with_model { all.iter().collect() } else { mine.clone() };
        // (face to draw into, its context, segment)
        let mut lines: Vec<(Option<Id>, (DVec3, DVec3))> = Vec::new();
        for (i, a) in mine.iter().enumerate() {
            for b in &others {
                if std::ptr::eq(*a, *b) || (a.owner == b.owner && a.face.is_none() && b.face.is_none()) {
                    continue; // same face, or both inside the same component
                }
                // within the selection, each pair once
                if !with_model && mine.iter().position(|x| std::ptr::eq(*x, *b)).map_or(false, |j| j < i) {
                    continue;
                }
                let segs = face_crossing(a, b);
                if segs.is_empty() {
                    continue;
                }
                for f in [a, b] {
                    // only geometry we can edit: loose faces and faces in groups
                    if let Some(fid) = f.face {
                        let ctx = self.faces.get(&fid).and_then(|x| x.group);
                        for s in &segs {
                            lines.push((ctx, *s));
                        }
                    }
                }
            }
        }
        let prev = self.active_group;
        let mut drawn = 0;
        for (ctx, (p, q)) in lines {
            self.active_group = ctx;
            if self.draw_line(p, q).is_ok() {
                drawn += 1;
            }
        }
        self.active_group = prev;
        Ok(drawn)
    }

    /// Mirror the picked things across their own middle, along an axis
    /// (0 red / x, 1 green / y, 2 blue / z).
    pub fn flip(&mut self, ids: &[Id], axis: usize) -> Result<usize> {
        let faces: Vec<WFace> = self.world_faces().into_iter().filter(|f| picked(f, ids)).collect();
        let pts: Vec<DVec3> = faces.iter().flat_map(|f| f.outer.iter().copied()).collect();
        let edges: Vec<DVec3> = self.edges.iter().filter(|(k, _)| ids.contains(k)).flat_map(|(_, e)| [self.p(e.a), self.p(e.b)]).collect();
        let all: Vec<DVec3> = pts.into_iter().chain(edges).collect();
        if all.is_empty() {
            bail!("nothing to flip");
        }
        let (lo, hi) = all.iter().fold((all[0], all[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let mut f = DVec3::ONE;
        f[axis.min(2)] = -1.0;
        self.scale(ids, (lo + hi) / 2.0, f)
    }
}
