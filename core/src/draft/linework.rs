//! Paper line work geometry (design step 10): intersections of lines and arcs, trim, extend and offset.
//!
//! Everything here is plain 2D geometry in paper millimetres (x right, y down, as the sheet is) and never panics:
//! a degenerate shape or a request that can't be done comes back as an `Err` with a sentence an agent or a person can
//! act on. The commands (`sheet_trim`, `sheet_extend`, `sheet_offset`) gather the boundaries and own the model edit.
//!
//! A shape is a `Chain`: line segments and arcs end to end, open or closed (a rectangle, or a polyline that ends where
//! it starts). A place on a chain is `g = i + t`: curve `i`, and how far along it (0 to 1). An arc is a circle with
//! a start angle and a signed sweep, made from the three points the saved item holds (start, a point on it, end).

use crate::drafting::{ItemKind, Shape};
use anyhow::{bail, Result};
use std::f64::consts::TAU;

pub type P = [f64; 2];

/// Two places closer than this (paper mm) are the same place.
pub const EPS: f64 = 1e-6;
/// A trimmed-off piece shorter than this is dropped rather than kept as a speck.
const SPECK: f64 = 1e-4;
/// The largest paper coordinate an item may use, in mm.
pub const MAX_COORD: f64 = 1e5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcC {
    pub c: P,
    pub r: f64,
    /// radians, in the sheet's own x/y (y down)
    pub a0: f64,
    /// signed, less than one full turn
    pub sweep: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Seg(P, P),
    Arc(ArcC),
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn len(a: P) -> f64 {
    a[0].hypot(a[1])
}
fn dist(a: P, b: P) -> f64 {
    len(sub(a, b))
}
fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

impl ArcC {
    /// The arc through three points (start, a point on it, end), or None when they are not three distinct points
    /// on a circle (two the same, or in a line).
    pub fn from_three(p0: P, pm: P, p1: P) -> Option<ArcC> {
        let (u, v) = (sub(pm, p0), sub(p1, p0));
        if len(u) < EPS || len(v) < EPS || dist(pm, p1) < EPS {
            return None;
        }
        let d = 2.0 * cross(u, v);
        // nearly in a line: the circle through them is enormous
        if d.abs() < 1e-9 * len(u) * len(v) {
            return None;
        }
        let (uu, vv) = (dot(u, u), dot(v, v));
        let ux = (v[1] * uu - u[1] * vv) / d;
        let uy = (u[0] * vv - v[0] * uu) / d;
        let c = [p0[0] + ux, p0[1] + uy];
        let r = len([ux, uy]);
        if !r.is_finite() || r > 1e7 || r < EPS {
            return None;
        }
        let ang = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]);
        let (a0, am, a1) = (ang(p0), ang(pm), ang(p1));
        let wrap = |x: f64| x.rem_euclid(TAU);
        let to_end = wrap(a1 - a0);
        let to_mid = wrap(am - a0);
        let sweep = if to_mid < to_end { to_end } else { to_end - TAU };
        (sweep.abs() > 1e-9).then_some(ArcC { c, r, a0, sweep })
    }
    pub fn at(&self, t: f64) -> P {
        let a = self.a0 + self.sweep * t;
        [self.c[0] + self.r * a.cos(), self.c[1] + self.r * a.sin()]
    }
    pub fn length(&self) -> f64 {
        self.r * self.sweep.abs()
    }
    /// The three points a saved arc keeps: start, middle, end.
    pub fn three(&self) -> [P; 3] {
        [self.at(0.0), self.at(0.5), self.at(1.0)]
    }
    /// Where along the arc (0 to 1) the angle `theta` falls, if it does (within `tol` mm of an end counts as the end).
    fn param_of(&self, theta: f64) -> Option<f64> {
        let delta = if self.sweep > 0.0 { theta - self.a0 } else { self.a0 - theta }.rem_euclid(TAU);
        let tol = EPS / self.r.max(EPS);
        let delta = if delta > TAU - tol { 0.0 } else { delta };
        let u = delta / self.sweep.abs();
        let ut = EPS / self.length().max(EPS);
        (u <= 1.0 + ut).then_some(u.min(1.0))
    }
}

impl Curve {
    pub fn length(&self) -> f64 {
        match self {
            Curve::Seg(a, b) => dist(*a, *b),
            Curve::Arc(a) => a.length(),
        }
    }
    pub fn at(&self, t: f64) -> P {
        match self {
            Curve::Seg(a, b) => lerp(*a, *b, t),
            Curve::Arc(a) => a.at(t),
        }
    }
    /// The part from `t0` to `t1` (each 0 to 1, in either order).
    fn part(&self, t0: f64, t1: f64) -> Curve {
        match self {
            Curve::Seg(a, b) => Curve::Seg(lerp(*a, *b, t0), lerp(*a, *b, t1)),
            Curve::Arc(a) => Curve::Arc(ArcC { c: a.c, r: a.r, a0: a.a0 + a.sweep * t0, sweep: a.sweep * (t1 - t0) }),
        }
    }
    fn bbox(&self) -> [f64; 4] {
        match self {
            Curve::Seg(a, b) => [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])],
            Curve::Arc(a) => {
                // the ends and any axis extreme the arc passes
                let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
                let mut grow = |p: P| {
                    b[0] = b[0].min(p[0]);
                    b[1] = b[1].min(p[1]);
                    b[2] = b[2].max(p[0]);
                    b[3] = b[3].max(p[1]);
                };
                grow(a.at(0.0));
                grow(a.at(1.0));
                for k in 0..4 {
                    if let Some(u) = a.param_of(k as f64 * TAU / 4.0) {
                        grow(a.at(u));
                    }
                }
                b
            }
        }
    }
}

/// One crossing: how far along each curve, and where.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub ta: f64,
    pub tb: f64,
    pub p: P,
}

fn seg_seg(p: P, p2: P, q: P, q2: P) -> Vec<Hit> {
    let (r, s) = (sub(p2, p), sub(q2, q));
    let (lr, ls) = (len(r), len(s));
    let den = cross(r, s);
    // parallel or collinear: no single crossing
    if lr < EPS || ls < EPS || den.abs() < 1e-12 * lr * ls {
        return vec![];
    }
    let qp = sub(q, p);
    let t = cross(qp, s) / den;
    let u = cross(qp, r) / den;
    let (tt, tu) = (EPS / lr, EPS / ls);
    if t < -tt || t > 1.0 + tt || u < -tu || u > 1.0 + tu {
        return vec![];
    }
    let (t, u) = (t.clamp(0.0, 1.0), u.clamp(0.0, 1.0));
    vec![Hit { ta: t, tb: u, p: lerp(p, p2, t) }]
}

/// Where the segment `p`..`p2` meets the circle of `a`, with the arc's own parameter (only within its sweep).
fn seg_arc(p: P, p2: P, a: &ArcC) -> Vec<Hit> {
    let d = sub(p2, p);
    let l = len(d);
    if l < EPS {
        return vec![];
    }
    let to_c = sub(a.c, p);
    let h = cross(d, to_c).abs() / l;
    if h > a.r + EPS {
        return vec![];
    }
    let mut ts: Vec<f64> = Vec::new();
    if (h - a.r).abs() <= EPS {
        ts.push(dot(to_c, d) / (l * l)); // tangent: one point
    } else {
        let t0 = dot(to_c, d) / (l * l);
        let w = (a.r * a.r - h * h).max(0.0).sqrt() / l;
        ts.push(t0 - w);
        ts.push(t0 + w);
    }
    let tt = EPS / l;
    ts.into_iter()
        .filter(|t| *t >= -tt && *t <= 1.0 + tt)
        .filter_map(|t| {
            let t = t.clamp(0.0, 1.0);
            let pt = lerp(p, p2, t);
            let u = a.param_of((pt[1] - a.c[1]).atan2(pt[0] - a.c[0]))?;
            Some(Hit { ta: t, tb: u, p: pt })
        })
        .collect()
}

fn arc_arc(a: &ArcC, b: &ArcC) -> Vec<Hit> {
    let dv = sub(b.c, a.c);
    let d = len(dv);
    // the same centre: they never cross (they coincide, or they don't touch)
    if d < EPS || d > a.r + b.r + EPS || d < (a.r - b.r).abs() - EPS {
        return vec![];
    }
    let x = (a.r * a.r - b.r * b.r + d * d) / (2.0 * d);
    let h = (a.r * a.r - x * x).max(0.0).sqrt();
    let e = [dv[0] / d, dv[1] / d];
    let mid = [a.c[0] + e[0] * x, a.c[1] + e[1] * x];
    let pts = if h < EPS { vec![mid] } else { vec![[mid[0] - e[1] * h, mid[1] + e[0] * h], [mid[0] + e[1] * h, mid[1] - e[0] * h]] };
    pts.into_iter()
        .filter_map(|p| {
            let ta = a.param_of((p[1] - a.c[1]).atan2(p[0] - a.c[0]))?;
            let tb = b.param_of((p[1] - b.c[1]).atan2(p[0] - b.c[0]))?;
            Some(Hit { ta, tb, p })
        })
        .collect()
}

/// Every crossing of two curves. `ta` and `tb` run 0 to 1 along `a` and `b`. Curves that run along each other (parallel
/// lines, concentric arcs) have no crossing; a touch at an end counts.
pub fn intersect(a: &Curve, b: &Curve) -> Vec<Hit> {
    match (a, b) {
        (Curve::Seg(p, p2), Curve::Seg(q, q2)) => seg_seg(*p, *p2, *q, *q2),
        (Curve::Seg(p, p2), Curve::Arc(x)) => seg_arc(*p, *p2, x),
        (Curve::Arc(x), Curve::Seg(p, p2)) => seg_arc(*p, *p2, x).into_iter().map(|h| Hit { ta: h.tb, tb: h.ta, p: h.p }).collect(),
        (Curve::Arc(x), Curve::Arc(y)) => arc_arc(x, y),
    }
}

/// Line segments and arcs end to end.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub curves: Vec<Curve>,
    pub closed: bool,
}

fn finite_pt(p: &P) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() <= MAX_COORD)
}

impl Chain {
    /// The chain a saved shape draws, or an error naming what is wrong with it.
    pub fn of(kind: &ItemKind) -> Result<Chain> {
        let (shape, which): (&Shape, &str) = match kind {
            ItemKind::Line(s) => (s, "line"),
            ItemKind::Polyline(s) => (s, "polyline"),
            ItemKind::Arc(s) => (s, "arc"),
            ItemKind::Rect(s) => (s, "rect"),
            other => bail!("a {} is not paper line work (lines, polylines, arcs and rectangles are)", other.name()),
        };
        let pts = &shape.pts_mm;
        if !pts.iter().all(finite_pt) {
            bail!("the {which} has a point that is not a sane paper position");
        }
        match which {
            "arc" => {
                if pts.len() != 3 {
                    bail!("an arc has 3 points (start, a point on it, end)");
                }
                let Some(a) = ArcC::from_three(pts[0], pts[1], pts[2]) else { bail!("the arc's three points are not on a circle (two are the same, or they are in a line)") };
                Ok(Chain { curves: vec![Curve::Arc(a)], closed: false })
            }
            "rect" => {
                if pts.len() != 2 {
                    bail!("a rect has 2 corners");
                }
                let [a, b] = [pts[0], pts[1]];
                if (a[0] - b[0]).abs() < EPS || (a[1] - b[1]).abs() < EPS {
                    bail!("the rect has no width or no height");
                }
                let c = [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]];
                Ok(Chain { curves: (0..4).map(|i| Curve::Seg(c[i], c[(i + 1) % 4])).collect(), closed: true })
            }
            _ => {
                if pts.len() < 2 || (which == "line" && pts.len() != 2) {
                    bail!("a {which} has {} points", if which == "line" { "2" } else { "at least 2" });
                }
                let curves: Vec<Curve> = pts.windows(2).filter(|w| dist(w[0], w[1]) >= EPS).map(|w| Curve::Seg(w[0], w[1])).collect();
                if curves.is_empty() {
                    bail!("the {which} has no length (all its points are the same)");
                }
                let closed = which == "polyline" && pts.len() >= 4 && dist(pts[0], pts[pts.len() - 1]) < EPS;
                Ok(Chain { curves, closed })
            }
        }
    }

    pub fn start(&self) -> P {
        self.curves[0].at(0.0)
    }
    pub fn end(&self) -> P {
        self.curves[self.curves.len() - 1].at(1.0)
    }
    fn n(&self) -> f64 {
        self.curves.len() as f64
    }
    pub fn length(&self) -> f64 {
        self.curves.iter().map(Curve::length).sum()
    }
    pub fn bbox(&self) -> [f64; 4] {
        self.curves.iter().map(Curve::bbox).fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
    }
    pub fn point_at(&self, g: f64) -> P {
        let i = (g.floor().max(0.0) as usize).min(self.curves.len() - 1);
        self.curves[i].at(g - i as f64)
    }

    /// The place on the chain nearest `p`, and how far `p` is from it.
    pub fn nearest(&self, p: P) -> (f64, f64) {
        let mut best = (0.0, f64::MAX);
        for (i, c) in self.curves.iter().enumerate() {
            let (t, d) = match c {
                Curve::Seg(a, b) => {
                    let v = sub(*b, *a);
                    let t = (dot(sub(p, *a), v) / dot(v, v)).clamp(0.0, 1.0);
                    (t, dist(p, lerp(*a, *b, t)))
                }
                Curve::Arc(a) => {
                    let t = a.param_of((p[1] - a.c[1]).atan2(p[0] - a.c[0]));
                    match t {
                        Some(t) => (t, (dist(p, a.c) - a.r).abs()),
                        None => {
                            let (d0, d1) = (dist(p, a.at(0.0)), dist(p, a.at(1.0)));
                            if d0 <= d1 { (0.0, d0) } else { (1.0, d1) }
                        }
                    }
                }
            };
            if d < best.1 {
                best = (i as f64 + t, d);
            }
        }
        best
    }

    /// The pieces of this chain that run from `g0` to `g1` (`g1` may pass the end of a closed chain, and wraps).
    fn between(&self, g0: f64, g1: f64) -> Vec<Curve> {
        let n = self.curves.len();
        let mut out = Vec::new();
        let first = g0.floor() as i64;
        let last = g1.ceil() as i64;
        for i in first..last {
            let (lo, hi) = ((g0 - i as f64).max(0.0), (g1 - i as f64).min(1.0));
            if hi <= lo {
                continue;
            }
            let c = self.curves[(i.rem_euclid(n as i64)) as usize].part(lo, hi);
            if c.length() >= EPS {
                out.push(c);
            }
        }
        out
    }

    /// Every place a boundary crosses the chain, in order along it. A crossing at an open chain's own ends is left out
    /// (a line that only touches an edge with its end has nothing there to cut at).
    pub fn crossings(&self, bounds: &[Curve]) -> Vec<(f64, P)> {
        let bb = self.bbox();
        let mut hits: Vec<(f64, P)> = Vec::new();
        for b in bounds {
            let c = b.bbox();
            if c[2] < bb[0] - EPS || c[0] > bb[2] + EPS || c[3] < bb[1] - EPS || c[1] > bb[3] + EPS {
                continue;
            }
            for (i, mine) in self.curves.iter().enumerate() {
                for h in intersect(mine, b) {
                    hits.push((i as f64 + h.ta, h.p));
                }
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f64, P)> = Vec::new();
        for h in hits {
            if out.last().is_some_and(|l| (h.0 - l.0).abs() < 1e-9 || dist(h.1, l.1) < EPS) {
                continue;
            }
            out.push(h);
        }
        if !self.closed {
            let (s, e) = (self.start(), self.end());
            out.retain(|(_, p)| dist(*p, s) >= EPS && dist(*p, e) >= EPS);
        } else if out.len() >= 2 && (out[0].0 + self.n() - out[out.len() - 1].0).abs() < 1e-9 {
            out.pop(); // the same place found at both ends of the loop
        }
        out
    }
}

/// What trimming leaves.
#[derive(Debug)]
pub struct Trimmed {
    /// the pieces that stay, each a run of curves (0, 1 or 2 of them)
    pub kept: Vec<Vec<Curve>>,
    /// the two ends of the span that went
    pub cut: [P; 2],
}

/// Cut away the span of `chain` that holds `g_pick`, between the nearest crossings either side of it (an open chain's
/// span runs to its end where there is no crossing on that side). Refuses when no boundary crosses the chain.
pub fn trim(chain: &Chain, g_pick: f64, crossings: &[(f64, P)]) -> Result<Trimmed> {
    if crossings.is_empty() {
        bail!("nothing crosses it: no boundary line, arc or viewport edge meets this line work, so there is nothing to trim to");
    }
    let n = chain.n();
    if chain.closed {
        if crossings.len() < 2 {
            bail!("a closed shape needs two crossings to trim a span out of it, and only one boundary meets it");
        }
        let lo = crossings.iter().rev().find(|c| c.0 <= g_pick).copied().unwrap_or_else(|| (crossings[crossings.len() - 1].0 - n, crossings[crossings.len() - 1].1));
        let hi = crossings.iter().find(|c| c.0 > g_pick).copied().unwrap_or_else(|| (crossings[0].0 + n, crossings[0].1));
        let kept = chain.between(hi.0, lo.0 + n);
        return Ok(Trimmed { kept: if kept.is_empty() { vec![] } else { vec![kept] }, cut: [lo.1, hi.1] });
    }
    let lo = crossings.iter().rev().find(|c| c.0 <= g_pick).copied();
    let hi = crossings.iter().find(|c| c.0 > g_pick).copied();
    let mut kept = Vec::new();
    if let Some(lo) = lo {
        kept.push(chain.between(0.0, lo.0));
    }
    if let Some(hi) = hi {
        kept.push(chain.between(hi.0, n));
    }
    kept.retain(|k| k.iter().map(Curve::length).sum::<f64>() >= SPECK);
    Ok(Trimmed { kept, cut: [lo.map_or(chain.start(), |c| c.1), hi.map_or(chain.end(), |c| c.1)] })
}

/// Extend the end of an open chain that is nearest `at` along its own direction (a line's, a polyline's last
/// segment, an arc's circle) to the nearest boundary beyond it. `reach` is how far along a straight end the search goes
/// (the sheet's size). Returns the new chain and the end that moved (false: the start).
pub fn extend(chain: &Chain, at: P, bounds: &[Curve], reach: f64) -> Result<(Chain, bool)> {
    if chain.closed {
        bail!("a closed shape has no end to extend");
    }
    let to_end = dist(at, chain.end()) <= dist(at, chain.start());
    let last = chain.curves.len() - 1;
    // the piece to look along, starting at the end that moves, and how the chain changes if it reaches a hit
    let (ray, ray_len) = match (to_end, chain.curves[if to_end { last } else { 0 }]) {
        (true, Curve::Seg(a, b)) | (false, Curve::Seg(b, a)) => {
            let d = sub(b, a);
            let l = len(d);
            let u = [d[0] / l, d[1] / l];
            (Curve::Seg(b, [b[0] + u[0] * reach, b[1] + u[1] * reach]), reach)
        }
        (true, Curve::Arc(a)) => {
            let rest = TAU - a.sweep.abs() - 1e-9;
            (Curve::Arc(ArcC { c: a.c, r: a.r, a0: a.a0 + a.sweep, sweep: a.sweep.signum() * rest }), rest)
        }
        (false, Curve::Arc(a)) => {
            let rest = TAU - a.sweep.abs() - 1e-9;
            (Curve::Arc(ArcC { c: a.c, r: a.r, a0: a.a0, sweep: -a.sweep.signum() * rest }), rest)
        }
    };
    let bb = ray.bbox();
    let mut best: Option<Hit> = None;
    let along = |h: &Hit| match &ray {
        Curve::Seg(..) => h.ta * ray_len,
        Curve::Arc(a) => h.ta * ray_len * a.r,
    };
    for b in bounds {
        let c = b.bbox();
        if c[2] < bb[0] - EPS || c[0] > bb[2] + EPS || c[3] < bb[1] - EPS || c[1] > bb[3] + EPS {
            continue;
        }
        for h in intersect(&ray, b) {
            if along(&h) > EPS && best.as_ref().is_none_or(|x| along(&h) < along(x)) {
                best = Some(h);
            }
        }
    }
    let Some(hit) = best else { bail!("nothing to extend to: no boundary line, arc or viewport edge lies ahead of that end of the line work") };
    let mut out = chain.clone();
    match (&ray, to_end) {
        (Curve::Seg(..), true) => {
            if let Curve::Seg(a, _) = out.curves[last] {
                out.curves[last] = Curve::Seg(a, hit.p);
            }
        }
        (Curve::Seg(..), false) => {
            if let Curve::Seg(_, b) = out.curves[0] {
                out.curves[0] = Curve::Seg(hit.p, b);
            }
        }
        (Curve::Arc(r), true) => {
            if let Curve::Arc(a) = out.curves[last] {
                out.curves[last] = Curve::Arc(ArcC { sweep: a.sweep + r.sweep * hit.ta, ..a });
            }
        }
        (Curve::Arc(r), false) => {
            if let Curve::Arc(a) = out.curves[0] {
                let grown = r.sweep * hit.ta; // runs backwards from the start
                out.curves[0] = Curve::Arc(ArcC { a0: a.a0 + grown, sweep: a.sweep - grown, ..a });
            }
        }
    }
    Ok((out, to_end))
}

/// The left of a direction of travel as the sheet looks (y down).
fn left_of(d: P) -> P {
    [d[1], -d[0]]
}

/// Which side of a closed chain `p` is on: true when inside (even-odd, on the flattened outline).
fn inside(chain: &Chain, p: P) -> bool {
    let mut pts: Vec<P> = Vec::new();
    for c in &chain.curves {
        match c {
            Curve::Seg(a, _) => pts.push(*a),
            Curve::Arc(a) => (0..32).for_each(|k| pts.push(a.at(k as f64 / 32.0))),
        }
    }
    let mut hit = false;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            hit = !hit;
        }
    }
    hit
}

/// Which side a request names, as "the offset goes to the left of the way the shape runs".
pub enum Side {
    Left,
    Right,
    Inside,
    Outside,
    /// the side `at` is on
    Toward(P),
}

/// The left-hand normal of `chain` at the place nearest `p`: where the offset's side is measured from.
fn left_normal_near(chain: &Chain, p: P) -> (P, P) {
    let (g, _) = chain.nearest(p);
    let i = (g.floor().max(0.0) as usize).min(chain.curves.len() - 1);
    let t = (g - i as f64).clamp(0.0, 1.0);
    let at = chain.curves[i].at(t);
    match chain.curves[i] {
        Curve::Seg(a, b) => {
            let d = sub(b, a);
            let l = len(d);
            (at, left_of([d[0] / l, d[1] / l]))
        }
        Curve::Arc(a) => {
            let r = sub(at, a.c);
            let l = len(r).max(EPS);
            // left of travel is outward for a positive sweep, inward for a negative one (see the derivation in `offset`)
            let s = a.sweep.signum();
            (at, [s * r[0] / l, s * r[1] / l])
        }
    }
}

/// Is the offset on the left of the shape's own direction?
fn is_left(chain: &Chain, side: &Side) -> Result<bool> {
    Ok(match side {
        Side::Left => true,
        Side::Right => false,
        Side::Inside | Side::Outside => {
            if !chain.closed {
                bail!("inside and outside are for closed shapes (a rect, or a polyline that ends where it starts); an open one has a left and a right");
            }
            let (at, n) = left_normal_near(chain, chain.start());
            let probe = [at[0] + n[0] * 1e-3, at[1] + n[1] * 1e-3];
            let left_is_inside = inside(chain, probe);
            left_is_inside == matches!(side, Side::Inside)
        }
        Side::Toward(p) => {
            let (at, n) = left_normal_near(chain, *p);
            let s = dot(sub(*p, at), n);
            if s.abs() < EPS {
                bail!("at_mm is on the line work itself: put it to the side the copy should go to");
            }
            s > 0.0
        }
    })
}

/// A copy of the chain `d` mm to one side (see `Side`): a line moves over; a polyline's corners are mitred (a
/// very sharp one is bevelled); an arc's circle grows or shrinks; a closed shape goes in or out. Refuses a distance
/// that would collapse the shape (a radius or a side reaching zero, or a segment turning back on itself).
pub fn offset(chain: &Chain, d: f64, side: &Side) -> Result<Chain> {
    if !(d.is_finite() && d > 0.0 && d <= MAX_COORD) {
        bail!("distance_mm is a distance above 0 and at most {MAX_COORD}");
    }
    let left = is_left(chain, side)?;
    let k = if left { d } else { -d };
    if let [Curve::Arc(a)] = chain.curves[..] {
        // left of the way it runs is outward for a positive sweep (x right, y down), inward for a negative one
        let r = if (a.sweep > 0.0) == left { a.r + d } else { a.r - d };
        if r <= EPS {
            bail!("the arc's radius is {:.3} mm: a copy {d} mm inside it would collapse to nothing", a.r);
        }
        return Ok(Chain { curves: vec![Curve::Arc(ArcC { r, ..a })], closed: false });
    }
    // straight segments only from here
    let segs: Vec<(P, P)> = chain.curves.iter().filter_map(|c| if let Curve::Seg(a, b) = c { Some((*a, *b)) } else { None }).collect();
    if segs.len() != chain.curves.len() {
        bail!("only straight and arc-only shapes can be offset");
    }
    let moved: Vec<(P, P, P)> = segs
        .iter()
        .map(|(a, b)| {
            let dv = sub(*b, *a);
            let l = len(dv);
            let n = left_of([dv[0] / l, dv[1] / l]);
            ([a[0] + n[0] * k, a[1] + n[1] * k], [b[0] + n[0] * k, b[1] + n[1] * k], [dv[0] / l, dv[1] / l])
        })
        .collect();
    // the corner between offset segments i and j: where the first one ends and the next begins (the same point for a
    // mitre, two for a bevel when the corner is too sharp)
    let corner = |i: usize, j: usize| -> (P, P) {
        let (a, b) = (&moved[i], &moved[j]);
        let c = cross(a.2, b.2);
        if c.abs() < 1e-9 {
            return if dot(a.2, b.2) > 0.0 { (a.1, a.1) } else { (a.1, b.0) }; // straight on, or folded back
        }
        let t = cross(sub(b.0, a.0), b.2) / c;
        let m = [a.0[0] + a.2[0] * t, a.0[1] + a.2[1] * t];
        if dist(m, segs[i].1) > 10.0 * d { (a.1, b.0) } else { (m, m) }
    };
    let n = moved.len();
    let mut pts: Vec<P> = Vec::new();
    // where each original segment starts and ends in the copy
    let mut spans: Vec<(P, P)> = Vec::new();
    for i in 0..n {
        let s = if i > 0 || chain.closed { corner((i + n - 1) % n, i).1 } else { moved[0].0 };
        let e = if i + 1 < n || chain.closed { corner(i, (i + 1) % n).0 } else { moved[n - 1].1 };
        spans.push((s, e));
    }
    if chain.closed {
        for i in 0..n {
            let (e, s) = corner(i, (i + 1) % n);
            pts.push(e);
            if dist(e, s) >= EPS {
                pts.push(s);
            }
        }
        pts.push(pts[0]);
    } else {
        pts.push(moved[0].0);
        for i in 0..n - 1 {
            let (e, s) = corner(i, i + 1);
            pts.push(e);
            if dist(e, s) >= EPS {
                pts.push(s);
            }
        }
        pts.push(moved[n - 1].1);
    }
    if !pts.iter().all(finite_pt) {
        bail!("the copy would run off the page: the distance is too large for this shape");
    }
    // each original segment must still run the way it did, or the distance is more than the shape's corners allow
    for ((a, b), (s, e)) in segs.iter().zip(&spans) {
        if dot(sub(*e, *s), sub(*b, *a)) <= EPS {
            bail!("a distance of {d} mm is too much for this shape: one of its sides would turn back on itself");
        }
    }
    let pts: Vec<P> = pts.into_iter().fold(Vec::new(), |mut v, p| {
        if v.last().is_none_or(|l| dist(*l, p) >= EPS) {
            v.push(p);
        }
        v
    });
    Ok(Chain { curves: pts.windows(2).map(|w| Curve::Seg(w[0], w[1])).collect(), closed: chain.closed })
}

/// A run of curves as the points a polyline keeps (every curve must be a straight one).
pub fn run_points(run: &[Curve]) -> Option<Vec<P>> {
    let mut pts: Vec<P> = Vec::new();
    for c in run {
        let Curve::Seg(a, b) = c else { return None };
        if pts.is_empty() {
            pts.push(*a);
        }
        pts.push(*b);
    }
    Some(pts)
}

/// A run of curves as a saved item. An arc stays an arc; straight pieces become a line when there is one segment of
/// something that was a line or a rectangle, and a polyline otherwise.
pub fn item_of(was: &ItemKind, run: &[Curve]) -> Option<ItemKind> {
    let base = match was {
        ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s) => s.clone(),
        _ => return None,
    };
    if let [Curve::Arc(a)] = run {
        return Some(ItemKind::Arc(Shape { pts_mm: a.three().to_vec(), ..base }));
    }
    let pts = run_points(run)?;
    Some(if pts.len() == 2 && matches!(was, ItemKind::Line(_) | ItemKind::Rect(_)) { ItemKind::Line(Shape { pts_mm: pts, ..base }) } else { ItemKind::Polyline(Shape { pts_mm: pts, ..base }) })
}

/// The two corners (lowest, then highest) of a closed four-sided chain: what an offset rectangle is kept as.
pub fn rect_of(chain: &Chain) -> Option<[P; 2]> {
    if chain.curves.len() != 4 {
        return None;
    }
    let (a, c) = (chain.curves[0].at(0.0), chain.curves[2].at(0.0));
    Some([[a[0].min(c[0]), a[1].min(c[1])], [a[0].max(c[0]), a[1].max(c[1])]])
}

/// Does the chain still look like an axis-aligned rectangle (for offsetting a rect back into a rect)?
pub fn is_axis_rect(chain: &Chain) -> bool {
    chain.closed && chain.curves.len() == 4 && chain.curves.iter().all(|c| matches!(c, Curve::Seg(a, b) if (a[0] - b[0]).abs() < EPS || (a[1] - b[1]).abs() < EPS))
}

/// One piece of paper line work, ready to draw: path segments in paper mm (y down) and its pen.
#[derive(Clone, Debug)]
pub struct ShapeDraw {
    pub segs: Vec<super::ir::Seg>,
    /// paper mm
    pub w: f64,
    /// None: the sheet's ink (a pen left at the default black draws in the ink, so blueprints stay readable)
    pub rgb: Option<super::ir::Rgb>,
    pub dash: Vec<f64>,
}

/// An arc as Bezier curves, at most a quarter turn each.
fn arc_segs(a: &ArcC, out: &mut Vec<super::ir::Seg>) {
    use super::ir::Seg;
    let n = ((a.sweep.abs() / (TAU / 4.0)).ceil() as usize).max(1);
    let step = a.sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for i in 0..n {
        let (t0, t1) = (a.a0 + step * i as f64, a.a0 + step * (i + 1) as f64);
        let p = |t: f64| [a.c[0] + a.r * t.cos(), a.c[1] + a.r * t.sin()];
        let d = |t: f64| [-a.r * t.sin(), a.r * t.cos()];
        let (p0, p1) = (p(t0), p(t1));
        let (d0, d1) = (d(t0), d(t1));
        out.push(Seg::C([p0[0] + k * d0[0], p0[1] + k * d0[1]], [p1[0] - k * d1[0], p1[1] - k * d1[1]], p1));
    }
}

/// What a saved line, polyline, arc or rect draws, or None for anything else (or a shape that doesn't hold together).
pub fn draw(kind: &ItemKind) -> Option<ShapeDraw> {
    use super::ir::{parse_hex, Seg};
    let (ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s)) = kind else { return None };
    let chain = Chain::of(kind).ok()?;
    let mut segs = vec![Seg::M(chain.start())];
    let last = chain.curves.len() - 1;
    for (i, c) in chain.curves.iter().enumerate() {
        match c {
            // the last side of a closed shape is the close itself
            Curve::Seg(..) if chain.closed && i == last => {}
            Curve::Seg(_, b) => segs.push(Seg::L(*b)),
            Curve::Arc(a) => arc_segs(a, &mut segs),
        }
    }
    if chain.closed {
        segs.push(Seg::Z);
    }
    let rgb = if s.pen.color.eq_ignore_ascii_case("#000000") { None } else { parse_hex(&s.pen.color) };
    Some(ShapeDraw { segs, w: s.pen.w_mm, rgb, dash: s.pen.dash_mm.clone() })
}

/// The straight pieces of a path that a model viewport drew (Beziers flattened), for use as boundaries.
pub fn flatten(segs: &[super::ir::Seg], out: &mut Vec<Curve>) {
    use super::ir::Seg;
    let (mut cur, mut start): (Option<P>, Option<P>) = (None, None);
    for s in segs {
        match s {
            Seg::M(p) => {
                cur = Some(*p);
                start = Some(*p);
            }
            Seg::L(p) => {
                if let Some(c) = cur {
                    if dist(c, *p) >= EPS {
                        out.push(Curve::Seg(c, *p));
                    }
                }
                cur = Some(*p);
            }
            Seg::C(a, b, c) => {
                if let Some(p0) = cur {
                    let mut prev = p0;
                    for i in 1..=8 {
                        let t = i as f64 / 8.0;
                        let u = 1.0 - t;
                        let q = [
                            u * u * u * p0[0] + 3.0 * u * u * t * a[0] + 3.0 * u * t * t * b[0] + t * t * t * c[0],
                            u * u * u * p0[1] + 3.0 * u * u * t * a[1] + 3.0 * u * t * t * b[1] + t * t * t * c[1],
                        ];
                        if dist(prev, q) >= EPS {
                            out.push(Curve::Seg(prev, q));
                        }
                        prev = q;
                    }
                }
                cur = Some(*c);
            }
            Seg::Z => {
                if let (Some(c), Some(s)) = (cur, start) {
                    if dist(c, s) >= EPS {
                        out.push(Curve::Seg(c, s));
                    }
                    cur = Some(s);
                }
            }
        }
    }
}

/// The four edges of a rectangle (x, y, w, h) as boundaries.
pub fn frame(r: [f64; 4]) -> Vec<Curve> {
    let c = [[r[0], r[1]], [r[0] + r[2], r[1]], [r[0] + r[2], r[1] + r[3]], [r[0], r[1] + r[3]]];
    (0..4).map(|i| Curve::Seg(c[i], c[(i + 1) % 4])).collect()
}

/// The part of a segment inside a rectangle (x, y, w, h), or None.
pub fn clip_to(c: &Curve, r: [f64; 4]) -> Option<Curve> {
    let Curve::Seg(a, b) = c else { return Some(*c) };
    let (t0, t1) = super::ir::clip_line(*a, *b, &r)?;
    Some(Curve::Seg(lerp(*a, *b, t0), lerp(*a, *b, t1)))
}
