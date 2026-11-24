//! Spline and Bezier curves, drawn as chains of edges (one curve id, like an arc). See docs/dev/small-tools.md.
//!
//!  - `spline_points`: a smooth curve THROUGH the points (centripetal Catmull-Rom, so it never loops or cusps on
//!    unevenly spaced clicks). This is what the Curve tool draws.
//!  - `bezier_points`: chained cubic Beziers from control points (anchor, handle, handle, anchor, ...).
//!  - `Model::draw_curve`: lays the sampled points down as edges the way `arc` does and flags them as one curve.

use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;

/// Most pieces one span (the stretch between two clicked points, or one cubic) is cut into.
pub const MAX_SPAN_SEGMENTS: usize = 256;
/// Most edges one curve command may make.
pub const MAX_CURVE_EDGES: usize = 20_000;
/// Two points closer than this are one point.
const SAME: f64 = 1e-9;

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
        let mut pts = distinct(pts);
        if closed && pts.len() > 1 {
            *pts.last_mut().unwrap() = pts[0];
        }
        if pts.len() < 2 {
            bail!("that curve has no length");
        }
        let (mut edges, mut faces) = (vec![], vec![]);
        for w in pts.windows(2) {
            let r = self.draw_line(w[0], w[1])?;
            edges.extend(r.edges);
            faces.extend(r.faces);
        }
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
