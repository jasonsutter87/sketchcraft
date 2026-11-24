//! Spline and Bezier curves, drawn as chains of edges (one curve id, like an arc). See docs/dev/small-tools.md.
//!
//!  - `spline_points`: a smooth curve THROUGH the points (centripetal Catmull-Rom, so it never loops or cusps on
//!    unevenly spaced clicks). This is what the Curve tool draws.
//!  - `bezier_points`: chained cubic Beziers from control points (anchor, handle, handle, anchor, ...).
//!  - `Model::draw_curve`: lays the sampled points down as edges the way `arc` does and flags them as one curve.

use crate::model::{Edge, Id, Model, Vertex, WELD};
use anyhow::{bail, Result};
use glam::DVec3;

/// Most pieces one span (the stretch between two clicked points, or one cubic) is cut into.
pub const MAX_SPAN_SEGMENTS: usize = 256;
/// Most edges one curve command may make.
pub const MAX_CURVE_EDGES: usize = 20_000;
/// Two points closer than this are one point.
const SAME: f64 = 1e-9;
/// Curves with at least this many pieces are laid down in one pass (shorter ones go through line drawing).
const CHAIN_MIN: usize = 24;
/// Most places a curve may cross or touch itself: each one splits edges and grows the work.
pub const MAX_SELF_CROSSINGS: usize = 64;
/// Two parts of a curve closer than this are touching.
const TOUCH: f64 = 1e-5;
/// Most places a chain from a plugin or an agent may cross or touch what is already drawn: each one cuts edges and
/// faces, and every piece is drawn against the whole model, so the work grows fast. (The person's own drawing is not limited.)
pub const MAX_EXISTING_CROSSINGS: usize = 128;
/// How long a chain from a plugin or an agent may take to draw before it is stopped and put back.
const DRAW_SECONDS: std::time::Duration = std::time::Duration::from_secs(8);

fn distinct(points: &[DVec3]) -> Vec<DVec3> {
    let mut out: Vec<DVec3> = Vec::with_capacity(points.len());
    for p in points {
        if !p.is_finite() {
            continue;
        }
        if out.last().is_none_or(|q| q.distance(*p) > SAME) {
            out.push(*p);
        }
    }
    out
}

/// Points along a smooth curve through `points`: `segments` pieces per span, the given points included exactly.
/// `closed` joins the last point back to the first (the returned list then ends on the first point again).
pub fn spline_points(points: &[DVec3], closed: bool, segments: usize) -> Result<Vec<DVec3>> {
    let mut pts = distinct(points);
    if closed && pts.len() > 1 && pts[0].distance(*pts.last().unwrap()) <= SAME {
        pts.pop();
    }
    let n = pts.len();
    if n < 2 || (closed && n < 3) {
        bail!("a spline needs at least {} different points", if closed { 3 } else { 2 });
    }
    let k = segments.clamp(1, MAX_SPAN_SEGMENTS);
    let spans = if closed { n } else { n - 1 };
    if spans * k > MAX_CURVE_EDGES {
        bail!("that curve would have {} edges; the most is {MAX_CURVE_EDGES} (use fewer points or segments)", spans * k);
    }
    // the point `i` steps along, wrapping when closed and continuing the end tangent when open
    let at = |i: isize| -> DVec3 {
        let m = n as isize;
        if closed {
            return pts[i.rem_euclid(m) as usize];
        }
        if i < 0 {
            return pts[0] * 2.0 - pts[1];
        }
        if i >= m {
            return pts[n - 1] * 2.0 - pts[n - 2];
        }
        pts[i as usize]
    };
    let mut out = vec![pts[0]];
    for s in 0..spans as isize {
        let (p0, p1, p2, p3) = (at(s - 1), at(s), at(s + 1), at(s + 2));
        // centripetal knots (alpha 1/2)
        let knot = |a: DVec3, b: DVec3, t: f64| t + a.distance(b).sqrt().max(1e-6);
        let t0 = 0.0;
        let t1 = knot(p0, p1, t0);
        let t2 = knot(p1, p2, t1);
        let t3 = knot(p2, p3, t2);
        for j in 1..=k {
            let t = t1 + (t2 - t1) * j as f64 / k as f64;
            let a1 = p0 * ((t1 - t) / (t1 - t0)) + p1 * ((t - t0) / (t1 - t0));
            let a2 = p1 * ((t2 - t) / (t2 - t1)) + p2 * ((t - t1) / (t2 - t1));
            let a3 = p2 * ((t3 - t) / (t3 - t2)) + p3 * ((t - t2) / (t3 - t2));
            let b1 = a1 * ((t2 - t) / (t2 - t0)) + a2 * ((t - t0) / (t2 - t0));
            let b2 = a2 * ((t3 - t) / (t3 - t1)) + a3 * ((t - t1) / (t3 - t1));
            out.push(b1 * ((t2 - t) / (t2 - t1)) + b2 * ((t - t1) / (t2 - t1)));
        }
        // land exactly on the clicked point
        *out.last_mut().unwrap() = p2;
    }
    Ok(out)
}

/// Points along chained cubic Beziers. `control` is anchor, handle, handle, anchor, handle, handle, anchor...
/// (3 per span + 1). `closed` needs the last anchor to be the first.
pub fn bezier_points(control: &[DVec3], closed: bool, segments: usize) -> Result<Vec<DVec3>> {
    if control.iter().any(|p| !p.is_finite()) {
        bail!("a bezier point is not a number");
    }
    if control.len() < 4 || !(control.len() - 1).is_multiple_of(3) {
        bail!("a bezier takes 4, 7, 10... control points: anchor, handle, handle, anchor (then handle, handle, anchor for each next piece); got {}", control.len());
    }
    if closed && control[0].distance(*control.last().unwrap()) > 1e-6 {
        bail!("a closed bezier ends on the point it starts from: make the last control point the same as the first");
    }
    let k = segments.clamp(1, MAX_SPAN_SEGMENTS);
    let spans = (control.len() - 1) / 3;
    if spans * k > MAX_CURVE_EDGES {
        bail!("that curve would have {} edges; the most is {MAX_CURVE_EDGES} (use fewer pieces or segments)", spans * k);
    }
    let mut out = vec![control[0]];
    for s in 0..spans {
        let (a, b, c, d) = (control[3 * s], control[3 * s + 1], control[3 * s + 2], control[3 * s + 3]);
        for j in 1..=k {
            let t = j as f64 / k as f64;
            let u = 1.0 - t;
            out.push(a * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t));
        }
        *out.last_mut().unwrap() = d;
    }
    if closed {
        *out.last_mut().unwrap() = control[0];
    }
    Ok(out)
}

/// What a curve command made.
pub struct CurveResult {
    pub edges: Vec<Id>,
    pub faces: Vec<Id>,
    pub curve: Option<Id>,
    pub points: usize,
}

impl Model {
    /// Draw sampled points as edges (a closed loop on one plane makes a face, as lines do) and flag them as one curve.
    /// `closed`: the list ends on its first point.
    pub fn draw_curve(&mut self, pts: &[DVec3], closed: bool) -> Result<CurveResult> {
        self.draw_curve_for(pts, closed, false)
    }

    /// `draw_curve`; `untrusted` (a plugin or an agent) limits how often the curve may meet what is already drawn and
    /// how long it may take.
    pub fn draw_curve_for(&mut self, pts: &[DVec3], closed: bool, untrusted: bool) -> Result<CurveResult> {
        let mut pts = distinct(pts);
        if closed && pts.len() > 1 {
            *pts.last_mut().unwrap() = pts[0];
        }
        if pts.len() < 2 {
            bail!("that curve has no length");
        }
        if pts.len() > CHAIN_MIN {
            match chain_state(self, &pts, closed) {
                Chain::Crossed(n) => bail!("that curve crosses or touches itself {n} times; the most is {MAX_SELF_CROSSINGS}"),
                Chain::Clear => return Ok(self.lay_chain(&pts, closed)),
                Chain::Busy => {}
            }
        }
        let r = self.draw_pieces(&pts, untrusted)?;
        let (edges, mut faces) = (r.edges, r.faces);
        let ring = if closed { &pts[..pts.len() - 1] } else { &pts[..] };
        let curve = self.curve_through(ring, closed);
        // a long closed loop is past what line drawing looks for: make its face from the points
        if closed && faces.is_empty() && ring.len() >= 3 {
            let g = self.active_group;
            let ids: Vec<Id> = ring.iter().filter_map(|p| self.vertices.iter().find(|(_, v)| v.group == g && v.p.distance(*p) < 1e-7).map(|(&k, _)| k)).collect();
            if ids.len() == ring.len() {
                if let Ok(f) = self.add_face(ids) {
                    faces.push(f);
                }
            }
        }
        Ok(CurveResult { edges, faces, curve, points: pts.len() })
    }
}

impl Model {
    /// Draw a chain of points as lines, piece by piece (each is cut where it meets the drawing). For a plugin or an
    /// agent the chain may meet existing edges only `MAX_EXISTING_CROSSINGS` times and only `DRAW_SECONDS` of work are
    /// allowed; past either, nothing is drawn (the command is put back).
    pub fn draw_pieces(&mut self, pts: &[DVec3], untrusted: bool) -> Result<crate::topology::LineResult> {
        let before = self.deadline;
        if untrusted {
            let n = self.existing_touches(pts, MAX_EXISTING_CROSSINGS + 1);
            if n > MAX_EXISTING_CROSSINGS {
                bail!("that crosses or touches what is already drawn more than {MAX_EXISTING_CROSSINGS} times; draw it in smaller parts");
            }
            self.deadline = Some(std::time::Instant::now() + DRAW_SECONDS);
        }
        let mut out = crate::topology::LineResult { edges: vec![], faces: vec![] };
        let mut res = Ok(());
        for w in pts.windows(2) {
            match self.draw_line(w[0], w[1]) {
                Ok(r) => {
                    out.edges.extend(r.edges);
                    out.faces.extend(r.faces);
                }
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }
        self.deadline = before;
        res.map(|_| out)
    }

    /// How many (piece, edge) pairs of this chain and the active group's edges touch, counting up to `stop`. A grid
    /// over the edges keeps it to the pieces and the edges near them.
    fn existing_touches(&self, pts: &[DVec3], stop: usize) -> usize {
        if pts.len() < 2 {
            return 0;
        }
        let (mut lo, mut hi) = (pts[0], pts[0]);
        for p in pts {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let pad = DVec3::splat(TOUCH * 10.0 + WELD);
        let (blo, bhi) = (lo - pad, hi + pad);
        let g = self.active_group;
        let near: Vec<(DVec3, DVec3)> = self
            .edges
            .values()
            .filter(|e| e.group == g)
            .map(|e| (self.p(e.a), self.p(e.b)))
            .filter(|(a, b)| a.min(*b).cmple(bhi).all() && a.max(*b).cmpge(blo).all())
            .collect();
        if near.is_empty() {
            return 0;
        }
        let n = pts.len() - 1;
        let cell = (pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>() / n as f64 * 2.0).max(TOUCH * 100.0);
        let key = |p: DVec3| -> (i64, i64, i64) { (((p.x - lo.x) / cell).floor() as i64, ((p.y - lo.y) / cell).floor() as i64, ((p.z - lo.z) / cell).floor() as i64) };
        let span = |a: DVec3, b: DVec3| {
            let (k0, k1) = (key(a.min(b) - DVec3::splat(TOUCH)), key(a.max(b) + DVec3::splat(TOUCH)));
            let cells = (k1.0 - k0.0 + 1).max(1) as i128 * (k1.1 - k0.1 + 1).max(1) as i128 * (k1.2 - k0.2 + 1).max(1) as i128;
            (k0, k1, cells)
        };
        // edges (or pieces) spanning too many cells are kept apart and compared with everything
        let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<usize>> = std::collections::HashMap::new();
        let mut long: Vec<usize> = Vec::new();
        for (i, (a, b)) in near.iter().enumerate() {
            let (k0, k1, cells) = span(*a, *b);
            if cells > 4096 {
                long.push(i);
                continue;
            }
            for x in k0.0..=k1.0 {
                for y in k0.1..=k1.1 {
                    for z in k0.2..=k1.2 {
                        grid.entry((x, y, z)).or_default().push(i);
                    }
                }
            }
        }
        let mut hits = 0usize;
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for w in pts.windows(2) {
            seen.clear();
            let (k0, k1, cells) = span(w[0], w[1]);
            let mut cand: Vec<usize> = long.clone();
            if cells > 4096 {
                cand = (0..near.len()).collect();
            } else {
                for x in k0.0..=k1.0 {
                    for y in k0.1..=k1.1 {
                        for z in k0.2..=k1.2 {
                            cand.extend(grid.get(&(x, y, z)).into_iter().flatten());
                        }
                    }
                }
            }
            for i in cand {
                if seen.insert(i) && seg_dist(w[0], w[1], near[i].0, near[i].1) < TOUCH {
                    hits += 1;
                    if hits >= stop {
                        return hits;
                    }
                }
            }
        }
        hits
    }
}

enum Chain {
    /// touches nothing else: can be laid down directly
    Clear,
    /// meets existing geometry (or itself a few times): drawn piece by piece so edges are split where they meet
    Busy,
    /// meets itself too often
    Crossed(usize),
}

fn seg_dist(p: DVec3, q: DVec3, r: DVec3, s: DVec3) -> f64 {
    // closest distance between segments pq and rs (Ericson, Real-Time Collision Detection)
    let (d1, d2, w) = (q - p, s - r, p - r);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(w));
    let c = d1.dot(w);
    let b = d1.dot(d2);
    let denom = a * e - b * b;
    let clamp = |x: f64| x.clamp(0.0, 1.0);
    let mut sn = if denom > 1e-18 { clamp((b * f - c * e) / denom) } else { 0.0 };
    let mut t = if e > 1e-18 { (b * sn + f) / e } else { 0.0 };
    if t < 0.0 {
        t = 0.0;
        sn = if a > 1e-18 { clamp(-c / a) } else { 0.0 };
    } else if t > 1.0 {
        t = 1.0;
        sn = if a > 1e-18 { clamp((b - c) / a) } else { 0.0 };
    }
    (p + d1 * sn).distance(r + d2 * t)
}

/// Whether a chain of points touches the model's own geometry in the active group, or itself.
fn chain_state(m: &Model, pts: &[DVec3], closed: bool) -> Chain {
    let n = pts.len() - 1; // pieces
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for p in pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let pad = DVec3::splat(TOUCH * 10.0 + WELD);
    let (blo, bhi) = (lo - pad, hi + pad);
    let g = m.active_group;
    let near = |a: DVec3, b: DVec3| a.min(b).cmple(bhi).all() && a.max(b).cmpge(blo).all();
    if m.vertices.values().any(|v| v.group == g && v.p.cmpge(blo).all() && v.p.cmple(bhi).all()) {
        return Chain::Busy;
    }
    if m.edges.values().any(|e| e.group == g && near(m.p(e.a), m.p(e.b))) {
        return Chain::Busy;
    }
    // pieces too short to keep apart count as touching (line drawing would weld them)
    if pts.windows(2).any(|w| w[0].distance(w[1]) <= WELD * 4.0) {
        return Chain::Busy;
    }
    // the curve against itself: a grid over the pieces, so only close ones are compared
    let cell = (pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>() / n as f64 * 2.0).max(TOUCH * 100.0);
    let key = |p: DVec3| -> (i64, i64, i64) { (((p.x - lo.x) / cell).floor() as i64, ((p.y - lo.y) / cell).floor() as i64, ((p.z - lo.z) / cell).floor() as i64) };
    let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let (k0, k1) = (key(pts[i].min(pts[i + 1]) - DVec3::splat(TOUCH)), key(pts[i].max(pts[i + 1]) + DVec3::splat(TOUCH)));
        let cells = (k1.0 - k0.0 + 1).max(1) as i128 * (k1.1 - k0.1 + 1).max(1) as i128 * (k1.2 - k0.2 + 1).max(1) as i128;
        if cells > 4096 {
            return Chain::Busy; // a piece far longer than its neighbours: let line drawing sort it out
        }
        for x in k0.0..=k1.0 {
            for y in k0.1..=k1.1 {
                for z in k0.2..=k1.2 {
                    grid.entry((x, y, z)).or_default().push(i);
                }
            }
        }
    }
    let adjacent = |i: usize, j: usize| {
        let d = j - i;
        d <= 1 || (closed && d == n - 1)
    };
    let mut hits = 0usize;
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    for list in grid.values() {
        for (x, &i) in list.iter().enumerate() {
            for &j in &list[x + 1..] {
                let (i, j) = (i.min(j), i.max(j));
                if adjacent(i, j) || !seen.insert((i, j)) {
                    continue;
                }
                if seg_dist(pts[i], pts[i + 1], pts[j], pts[j + 1]) < TOUCH {
                    hits += 1;
                    if hits > MAX_SELF_CROSSINGS {
                        return Chain::Crossed(hits);
                    }
                }
            }
        }
    }
    // adjacent pieces that fold back on each other overlap too
    for i in 0..n.saturating_sub(1) {
        let (a, b, c) = (pts[i], pts[i + 1], pts[i + 2]);
        if (a - b).normalize().dot((c - b).normalize()) > 1.0 - 1e-9 {
            return Chain::Busy;
        }
    }
    if hits > 0 {
        Chain::Busy
    } else {
        Chain::Clear
    }
}

impl Model {
    /// Lay a chain that touches nothing down in one pass: its vertices and edges are made directly, so the cost grows
    /// with the number of pieces and not with the pieces times the model.
    fn lay_chain(&mut self, pts: &[DVec3], closed: bool) -> CurveResult {
        let g = self.active_group;
        let ring_len = if closed { pts.len() - 1 } else { pts.len() };
        let mut ids: Vec<Id> = Vec::with_capacity(ring_len);
        for p in &pts[..ring_len] {
            let id = self.new_id();
            self.vertices.insert(id, Vertex { p: *p, group: g });
            ids.push(id);
        }
        let curve = self.new_id();
        let pieces = if closed { ring_len } else { ring_len - 1 };
        let mut edges = Vec::with_capacity(pieces);
        for i in 0..pieces {
            let id = self.new_id();
            self.edges.insert(id, Edge { a: ids[i], b: ids[(i + 1) % ring_len], group: g, curve: Some(curve), ..Default::default() });
            edges.push(id);
        }
        let mut faces = vec![];
        if closed && ring_len >= 3 {
            if let Ok(f) = self.add_face(ids) {
                faces.push(f);
            }
        }
        CurveResult { edges, faces, curve: Some(curve), points: pts.len() }
    }
}
