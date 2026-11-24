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
fn picked(f: &WFace, ids: &std::collections::HashSet<Id>) -> bool {
    ids.contains(&f.owner) || f.group.is_some_and(|g| ids.contains(&g)) || f.face.is_some_and(|x| ids.contains(&x))
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

/// Where the plane `n · x = w` (n unit) cuts a face: pieces of line inside it.
pub fn plane_crossing(f: &WFace, n: DVec3, w: f64) -> Vec<(DVec3, DVec3)> {
    let n1 = f.normal.normalize_or_zero();
    let d = n1.cross(n);
    if d.length_squared() < 1e-12 {
        return vec![];
    }
    let w1 = n1.dot(f.outer[0]);
    let o = (n.cross(d) * w1 + d.cross(n1) * w) / d.length_squared();
    let d = d.normalize();
    inside_intervals(f, o, d).into_iter().filter(|(a, b)| b - a > 1e-9).map(|(a, b)| (o + d * a, o + d * b)).collect()
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

/// Most crossing lines one `intersect` will collect before it is refused (each is then drawn into the faces it crosses).
const MAX_LINES: usize = 200_000;

/// The box around a face (a little padded, so faces that only touch are still candidates).
fn face_box(f: &WFace) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in &f.outer {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (lo - DVec3::splat(1e-6), hi + DVec3::splat(1e-6))
}

fn boxes_meet(a: &(DVec3, DVec3), b: &(DVec3, DVec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

/// A uniform grid over boxes: which of them lie in each cell. A box that would fill more than `BIG_CELLS` cells is kept
/// aside and offered to every query.
struct Grid {
    cell: f64,
    origin: DVec3,
    cells: std::collections::HashMap<(i64, i64, i64), Vec<u32>>,
    big: Vec<u32>,
}

const BIG_CELLS: f64 = 512.0;

type Cells = ((i64, i64, i64), (i64, i64, i64));

impl Grid {
    fn new(boxes: &[(DVec3, DVec3)], members: &[usize]) -> Grid {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut longest = 0.0;
        for &i in members {
            lo = lo.min(boxes[i].0);
            hi = hi.max(boxes[i].1);
            longest += (boxes[i].1 - boxes[i].0).max_element();
        }
        let extent = if members.is_empty() { 1.0 } else { (hi - lo).max_element() };
        let avg = if members.is_empty() { 1.0 } else { longest / members.len() as f64 };
        // about the size of one face, and never so small that the grid has more than 256 cells along its longest side
        let cell = avg.max(extent / 256.0).max(1e-6);
        let mut g = Grid { cell, origin: if members.is_empty() { DVec3::ZERO } else { lo }, cells: Default::default(), big: vec![] };
        for (k, &i) in members.iter().enumerate() {
            match g.range(&boxes[i]) {
                Some((a, b)) => {
                    for x in a.0..=b.0 {
                        for y in a.1..=b.1 {
                            for z in a.2..=b.2 {
                                g.cells.entry((x, y, z)).or_default().push(k as u32);
                            }
                        }
                    }
                }
                None => g.big.push(k as u32),
            }
        }
        g
    }

    /// The cells a box covers, or None when it covers too many.
    fn range(&self, b: &(DVec3, DVec3)) -> Option<Cells> {
        let f = |p: DVec3| ((p - self.origin) / self.cell).floor();
        let (lo, hi) = (f(b.0), f(b.1));
        let n = (hi.x - lo.x + 1.0) * (hi.y - lo.y + 1.0) * (hi.z - lo.z + 1.0);
        if !n.is_finite() || n > BIG_CELLS {
            return None;
        }
        Some(((lo.x as i64, lo.y as i64, lo.z as i64), (hi.x as i64, hi.y as i64, hi.z as i64)))
    }
}

impl Model {
    /// Intersect the picked faces / groups with the model (or only each
    /// other). Returns how many lines were drawn.
    pub fn intersect(&mut self, ids: &[Id], with_model: bool) -> Result<usize> {
        let all = self.world_faces();
        let idset: std::collections::HashSet<Id> = ids.iter().copied().collect();
        let boxes: Vec<(DVec3, DVec3)> = all.iter().map(face_box).collect();
        let mine: Vec<usize> = (0..all.len()).filter(|&i| picked(&all[i], &idset)).collect();
        if mine.is_empty() {
            bail!("pick the faces, groups or components to intersect");
        }
        // the faces to test against, as indices into `all` (ascending, so pairs come in the same order as ever)
        let others: Vec<usize> = if with_model { (0..all.len()).collect() } else { mine.clone() };
        // within the selection each pair is done once: where a face sits in the selection
        let rank: std::collections::HashMap<usize, usize> = mine.iter().enumerate().map(|(r, &i)| (i, r)).collect();
        let grid = Grid::new(&boxes, &others);
        let mut stamp = vec![usize::MAX; others.len()];
        // (face to draw into, its context, segment)
        let mut lines: Vec<(Option<Id>, (DVec3, DVec3))> = Vec::new();
        let mut cand: Vec<u32> = Vec::new();
        for (r, &ai) in mine.iter().enumerate() {
            let a = &all[ai];
            // the candidates: what shares a grid cell with this face's box (and the big ones), by box overlap
            cand.clear();
            let mut take = |k: u32, cand: &mut Vec<u32>| {
                if stamp[k as usize] != r {
                    stamp[k as usize] = r;
                    if boxes_meet(&boxes[ai], &boxes[others[k as usize]]) {
                        cand.push(k);
                    }
                }
            };
            match grid.range(&boxes[ai]) {
                Some((lo, hi)) => {
                    for x in lo.0..=hi.0 {
                        for y in lo.1..=hi.1 {
                            for z in lo.2..=hi.2 {
                                if let Some(c) = grid.cells.get(&(x, y, z)) {
                                    for &k in c {
                                        take(k, &mut cand);
                                    }
                                }
                            }
                        }
                    }
                    for &k in &grid.big {
                        take(k, &mut cand);
                    }
                }
                None => {
                    for k in 0..others.len() as u32 {
                        take(k, &mut cand);
                    }
                }
            }
            cand.sort_unstable();
            for &k in &cand {
                crate::deadline::poll();
                self.check_deadline()?;
                let bi = others[k as usize];
                let b = &all[bi];
                if ai == bi || (a.owner == b.owner && a.face.is_none() && b.face.is_none()) {
                    continue; // same face, or both inside the same component
                }
                if !with_model && rank.get(&bi).is_some_and(|&j| j < r) {
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
                if lines.len() > MAX_LINES {
                    bail!("those faces cross in more than {MAX_LINES} places, which is too many to draw at once: pick fewer faces");
                }
            }
        }
        let prev = self.active_group;
        let mut drawn = 0;
        for (ctx, (p, q)) in lines {
            self.active_group = ctx;
            match self.draw_line(p, q) {
                Ok(_) => drawn += 1,
                Err(e) if self.over_deadline() => {
                    self.active_group = prev;
                    return Err(e);
                }
                Err(_) => {}
            }
        }
        self.active_group = prev;
        Ok(drawn)
    }

    /// Mirror the picked things across their own middle, along an axis
    /// (0 red / x, 1 green / y, 2 blue / z).
    pub fn flip(&mut self, ids: &[Id], axis: usize) -> Result<usize> {
        let idset: std::collections::HashSet<Id> = ids.iter().copied().collect();
        let faces: Vec<WFace> = self.world_faces().into_iter().filter(|f| picked(f, &idset)).collect();
        let pts: Vec<DVec3> = faces.iter().flat_map(|f| f.outer.iter().copied()).collect();
        let edges: Vec<DVec3> = self.edges.iter().filter(|(k, _)| idset.contains(k)).flat_map(|(_, e)| [self.p(e.a), self.p(e.b)]).collect();
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
