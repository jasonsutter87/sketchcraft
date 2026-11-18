//! Viewport to display list: a plan (the model's cut, drawn as vector fills), or an elevation,
//! section or parallel 3D view (vector line work with the hidden lines removed, see `hidden`), or
//! a picture for a perspective or unscaled view. Dimensions and the callouts' placement live here too.

use super::hidden::{Proj, View};
use super::ir::*;
use super::paper::{scale_label, scale_of};
use crate::layout::{dims_on_floor, plan_extras, swing_arc, PlanExtras, ViewportSpec};
use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};
use anyhow::{bail, Result};
use glam::DVec3;

// ---- plans -----------------------------------------------------------------------------

/// Where a plan's content lies (meters): the cut, its dimension strings and callouts.
fn plan_points(bm: &Model, v: &ViewportSpec, z: f64) -> Result<(Vec<(f64, f64)>, PlanExtras, f64)> {
    let cut = bm.section(DVec3::new(0.0, 0.0, z), DVec3::new(0.0, 0.0, -1.0))?;
    let mut pts: Vec<(f64, f64)> = cut.segments.iter().flat_map(|(a, b)| [(a.x, a.y), (b.x, b.y)]).collect();
    // dimension strings on this floor count toward the plan's size
    let floor = v.floor.unwrap_or(z - 1.22);
    pts.extend(dims_on_floor(bm, floor));
    let extras = if v.callouts { plan_extras(bm, floor, z, !v.marks.is_empty()) } else { PlanExtras::default() };
    pts.extend(extras.reach.iter().copied());
    Ok((pts, extras, floor))
}

/// A floor plan viewport: the section at height `z` as vector paths, to scale (or fitted).
/// Returns its caption and the scale denominator.
pub(super) fn plan(cv: &mut Cv, m: &Model, bm: &Model, v: &ViewportSpec, z: f64, pens: &Pens) -> Result<(String, Option<f64>)> {
    let sh = cv.h_mm;
    let y = |mm: f64| (sh - mm) * PT;
    let cut = m.section(DVec3::new(0.0, 0.0, z), DVec3::new(0.0, 0.0, -1.0))?;
    let (pts, extras, floor) = plan_points(bm, v, z)?;
    if pts.is_empty() {
        bail!("nothing is cut at {z} m");
    }
    let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
    // meters -> mm on paper
    let denom = match &v.scale {
        Some(s) => scale_of(s)?,
        None => ((x1 - x0) * 1000.0 / (v.w * 0.9)).max((y1 - y0) * 1000.0 / (v.h * 0.9)),
    };
    let k = 1000.0 / denom;
    let (cx, cy) = (v.x + v.w / 2.0, v.y + v.h / 2.0);
    // the drawing stays where it was made: pinned to its anchor, or (when it has none yet) the middle
    // of the building
    let (mx, my) = v.anchor.map(|a| (a[0], a[1])).unwrap_or(((x0 + x1) / 2.0, (y0 + y1) / 2.0));
    // plan +Y is up the sheet
    let to = |px: f64, py: f64| -> (f64, f64) { ((cx + (px - mx) * k) * PT, y(cy - (py - my) * k)) };
    let units = if m.units.is_empty() { "m" } else { m.units.as_str() };
    cv.clipped(v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT, |cv| {
        // cut walls: solid dark on paper; pale with white outlines on a blueprint; holes left open (even-odd)
        let outline = pens.ink(pens.heavy);
        for (_, outer, holes) in &cut.regions {
            let mut segs = Vec::new();
            for ring in std::iter::once(outer).chain(holes.iter()) {
                for (i, p) in ring.iter().enumerate() {
                    let (a, b) = to(p.x, p.y);
                    let q = [a / PT, sh - b / PT];
                    segs.push(if i == 0 { Seg::M(q) } else { Seg::L(q) });
                }
                segs.push(Seg::Z);
            }
            cv.push(Prim::Path(PathPrim { segs, pen: Some(outline.clone()), fill: Some(Fill { rgb: pens.cut_fill, even_odd: true }), prov: None }));
        }
        // everything else the cut passes through (glass, frames): thin lines
        let thin = pens.ink(pens.thin);
        for (a, b) in &cut.segments {
            cv.line(to(a.x, a.y), to(b.x, b.y), &thin);
        }
        // what's below the cut on this floor, seen from above: stair treads, counters, fixtures
        // (finer and lighter than the cut)
        let faint = Pen::new(pens.hair, pens.faint());
        for e in m.world_edges().into_iter().filter(|e| !e.soft) {
            let (lo, hi) = (e.a.z.min(e.b.z), e.a.z.max(e.b.z));
            if lo < floor + 0.02 && hi < floor + 0.02 {
                continue; // lying on the floor: the floor's own outline, not a thing on it
            }
            if lo < floor - 0.01 || hi > z + 1e-3 {
                continue;
            }
            if (e.a.x - e.b.x).hypot(e.a.y - e.b.y) < 0.01 {
                continue; // vertical: a point from above
            }
            let pen = if e.tag.is_some() || e.draft.is_some() { pens.for_tag(&m.drafting, e.tag.as_deref(), e.draft.as_deref(), faint.clone(), false) } else { faint.clone() };
            cv.line(to(e.a.x, e.a.y), to(e.b.x, e.b.y), &pen);
        }
        stairs(cv, m, pens, floor, &to);
        overhead(cv, m, pens, floor, z, &to);
        annotations(cv, m, pens, floor, units, &to);
        // worked-out callouts: exterior dimension chains, door/window tags, elevation markers, north arrow;
        // short neighbours take turns lifting their labels
        let mut lift = false;
        for (a, b, off) in &extras.dims {
            let short = a.distance(*b) * 1000.0 / denom < 12.0;
            lift = short && !lift;
            dimension_lift(cv, pens, *a, *b, *off, &to, units, lift);
        }
        for (p, label, kind) in &extras.tags {
            let (tx, ty) = to(p.x, p.y);
            crate::callouts::tag(cv, pens, tx, ty, label, *kind);
        }
        if let (Some((p, _)), 4) = (extras.marks.first(), v.marks.len()) {
            let (mx, my) = to(p.x, p.y);
            crate::callouts::elevation_key(cv, pens, mx, my, &v.marks);
        }
        if let Some(sm) = &v.section_mark {
            let (o, look) = (DVec3::from(sm.origin), DVec3::new(sm.look[0], sm.look[1], 0.0).normalize_or_zero());
            // the line runs across the plan, past the building on both sides and every dimension and tag around it
            let along = DVec3::Z.cross(look);
            let reach: Vec<f64> = pts.iter().map(|(px, py)| (DVec3::new(*px, *py, 0.0) - DVec3::new(o.x, o.y, 0.0)).dot(along)).collect();
            let (s0, s1) = reach.iter().fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
            if s0 < s1 {
                let (a, b) = (o + along * (s0 - 0.7), o + along * (s1 + 0.7));
                // kept inside the frame (between the dimensions and its edge)
                let inside = |p: (f64, f64)| ((p.0).clamp(v.x * PT + 14.0, (v.x + v.w) * PT - 14.0), (p.1).clamp(y(v.y + v.h) + 14.0, y(v.y) - 14.0));
                let (pa, pb) = (inside(to(a.x, a.y)), inside(to(b.x, b.y)));
                cv.line(pa, pb, &pens.ink(pens.heavy).dashed(&[9.0 / PT, 2.0 / PT, 2.0 / PT, 2.0 / PT]));
                // heads at both ends: which way it looks, and where it's drawn
                let lp = {
                    let q = to(o.x + look.x, o.y + look.y);
                    let p0 = to(o.x, o.y);
                    let (dx, dy) = (q.0 - p0.0, q.1 - p0.1);
                    let l = dx.hypot(dy).max(1e-9);
                    (dx / l, dy / l)
                };
                let (num, sheet) = sm.label.split_once('/').unwrap_or((sm.label.as_str(), ""));
                for p in [pa, pb] {
                    crate::callouts::elevation_marker(cv, pens, p.0, p.1, lp, num, sheet);
                }
            }
        }
        if v.callouts {
            crate::callouts::north_arrow(cv, pens, (v.x + v.w - 9.0) * PT, y(v.y + v.h - 11.0));
        }
    });
    let label = v.label.clone().unwrap_or_else(|| format!("Plan at {z:.2} m"));
    let label = match &v.scale {
        Some(s) => format!("{label}   {}", scale_label(s, denom)),
        None => label,
    };
    Ok((label, Some(denom)))
}

/// Stairs: on the floor they start from, the treads up to the cut with UP; on the floor they arrive
/// at, the whole run seen through the opening with DN.
fn stairs(cv: &mut Cv, m: &Model, pens: &Pens, floor: f64, to: &dyn Fn(f64, f64) -> (f64, f64)) {
    let ink = pens.ink;
    for (gid, g) in &m.groups {
        if !g.name.to_ascii_lowercase().contains("stair") || g.name.to_ascii_lowercase().contains("rail") {
            continue;
        }
        let es: Vec<(DVec3, DVec3)> = m.edges.values().filter(|e| e.group == Some(*gid) && !e.soft).map(|e| (m.p(e.a), m.p(e.b))).collect();
        let Some(first) = es.first() else { continue };
        let (mut lo, mut hi) = (first.0, first.0);
        for (a, b) in &es {
            lo = lo.min(*a).min(*b);
            hi = hi.max(*a).max(*b);
        }
        let starts_here = (lo.z - floor).abs() < 0.3;
        let arrives_here = (hi.z - floor).abs() < 0.3 && !starts_here;
        if !starts_here && !arrives_here {
            continue;
        }
        if arrives_here {
            // the run below, through the floor opening
            let hair = pens.ink(pens.hair);
            for (a, b) in &es {
                if (a.x - b.x).hypot(a.y - b.y) < 0.01 {
                    continue;
                }
                cv.line(to(a.x, a.y), to(b.x, b.y), &hair);
            }
        }
        // which way it climbs: toward the side its high treads are on
        let long_x = hi.x - lo.x > hi.y - lo.y;
        let mid_lo = |p: DVec3| if long_x { p.x < (lo.x + hi.x) / 2.0 } else { p.y < (lo.y + hi.y) / 2.0 };
        let (mut zl, mut nl, mut zh, mut nh) = (0.0, 0, 0.0, 0);
        for (a, b) in &es {
            for p in [a, b] {
                if mid_lo(*p) {
                    zl += p.z;
                    nl += 1;
                } else {
                    zh += p.z;
                    nh += 1;
                }
            }
        }
        let climbs_up = zh / nh.max(1) as f64 >= zl / nl.max(1) as f64;
        let (cx, cy) = ((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let half = if long_x { (hi.x - lo.x) * 0.35 } else { (hi.y - lo.y) * 0.35 };
        let (sx, sy, ex, ey) = if long_x { (cx - half, cy, cx + half, cy) } else { (cx, cy - half, cx, cy + half) };
        // arrow points up the stair (UP) or down it (DN)
        let ((ax, ay), (bx, by)) = if climbs_up == starts_here { ((sx, sy), (ex, ey)) } else { ((ex, ey), (sx, sy)) };
        let (p, q) = (to(ax, ay), to(bx, by));
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let l = dx.hypot(dy).max(1e-6);
        let (ux, uy) = (dx / l * 5.0, dy / l * 5.0);
        cv.line(p, q, &pens.ink(pens.medium));
        cv.path(&[q, (q.0 - ux - uy * 0.5, q.1 - uy + ux * 0.5), (q.0 - ux + uy * 0.5, q.1 - uy - ux * 0.5)], true, None, Some(Fill { rgb: ink, even_odd: false }));
        cv.text(p.0 + 3.0, p.1 - 8.0, if starts_here { "UP" } else { "DN" }, 6.0, true, ink, 0.0);
    }
}

/// Overhead: shelves, closets and upper cabinets above the cut, dashed.
fn overhead(cv: &mut Cv, m: &Model, pens: &Pens, floor: f64, z: f64, to: &dyn Fn(f64, f64) -> (f64, f64)) {
    let pen = pens.ink(pens.thin).dashed(&[2.0 / PT, 1.5 / PT]);
    for (gid, g) in &m.groups {
        let n = g.name.to_ascii_lowercase();
        let furniture = matches!(m.classes.get(gid).map(|s| s.as_str()), Some("IfcFurniture") | Some("IfcCovering"));
        if !(furniture || ["shelf", "closet", "cabinet", "upper"].iter().any(|w| n.contains(w))) {
            continue;
        }
        for e in m.edges.values().filter(|e| e.group == Some(*gid) && !e.soft) {
            let (a, b) = (m.p(e.a), m.p(e.b));
            if a.z.min(b.z) <= z || a.z.max(b.z) > floor + 2.8 || (a.x - b.x).hypot(a.y - b.y) < 0.01 {
                continue;
            }
            cv.line(to(a.x, a.y), to(b.x, b.y), &pen);
        }
    }
}

/// Notes and dimensions placed on this floor (room names, overall sizes, door swings).
fn annotations(cv: &mut Cv, m: &Model, pens: &Pens, floor: f64, units: &str, to: &dyn Fn(f64, f64) -> (f64, f64)) {
    let ink = pens.ink;
    let on_floor = |pz: f64| pz > floor - 0.05 && pz < floor + 1.0;
    for a in &m.annotations {
        match a {
            crate::model::Annotation::Text { at, text, leader, .. } if on_floor(at.z) => {
                // centered, line by line
                let lines: Vec<&str> = text.lines().collect();
                let (px, py) = to(at.x, at.y);
                // a leader: from the edge of the note to a dot on what it's about
                if let Some(l) = leader {
                    let (lx, ly) = to(l.x, l.y);
                    let (dx, dy) = (lx - px, ly - py);
                    let d = dx.hypot(dy);
                    if d > 4.0 {
                        let w = lines.iter().map(|s| crate::callouts::text_w(s, 7.0)).fold(0.0, f64::max) / 2.0 + 3.0;
                        let h = lines.len() as f64 * 4.5 + 3.0;
                        // leave the text box where the line crosses it
                        let t = (w / dx.abs().max(1e-6)).min(h / dy.abs().max(1e-6)).min(0.9);
                        cv.line((px + dx * t, py + dy * t), (lx, ly), &pens.ink(pens.thin));
                        cv.circle(lx, ly, 1.3, None, Some(ink));
                    }
                }
                for (i, line) in lines.iter().enumerate() {
                    let size = if i == 0 { 7.0 } else { 5.5 };
                    let w = super::font::width(line, size, i == 0);
                    let yy = py + (lines.len() as f64 / 2.0 - i as f64 - 0.8) * 8.0;
                    cv.text(px - w / 2.0, yy, line, size, i == 0, ink, 0.0);
                }
            }
            crate::model::Annotation::Swing { hinge, from, to: open, .. } if on_floor(hinge.z) => {
                let (h, o) = (to(hinge.x, hinge.y), to(open.x, open.y));
                cv.line(h, o, &pens.ink(pens.medium));
                let arc: Vec<(f64, f64)> = swing_arc(*hinge, *from, *open, 16).iter().map(|p| to(p.x, p.y)).collect();
                cv.pline(&arc, false, &pens.ink(pens.hair));
            }
            crate::model::Annotation::Dimension { a: da, b: db, offset, .. } if on_floor(da.z) && on_floor(db.z) => {
                dimension_lift(cv, pens, *da, *db, *offset, to, units, false);
            }
            _ => {}
        }
    }
}

/// A dimension: the line `offset` out from a-b, extension lines, ticks and the length along it.
/// `lift`: a short segment whose text sits a line further out (so neighbouring short ones don't collide).
#[allow(clippy::too_many_arguments)]
pub fn dimension_lift(cv: &mut Cv, pens: &Pens, da: DVec3, db: DVec3, offset: DVec3, to: &dyn Fn(f64, f64) -> (f64, f64), units: &str, lift: bool) {
    let (pa, pb) = (da + offset, db + offset);
    let (p, q) = (to(pa.x, pa.y), to(pb.x, pb.y));
    let (o1, o2) = (to(da.x, da.y), to(db.x, db.y));
    cv.line(p, q, &pens.ink(pens.thin));
    // extension lines and tick marks
    for (from, at) in [(o1, p), (o2, q)] {
        let (dx, dy) = (at.0 - from.0, at.1 - from.1);
        let l = dx.hypot(dy).max(1e-6);
        let gap = 2.0f64.min(l * 0.3);
        cv.line((from.0 + dx / l * gap, from.1 + dy / l * gap), (at.0 + dx / l * 2.0, at.1 + dy / l * 2.0), &pens.ink(pens.hair));
        cv.line((at.0 - 2.5, at.1 - 2.5), (at.0 + 2.5, at.1 + 2.5), &pens.ink(pens.medium));
    }
    let label = crate::units::format(da.distance(db), units);
    let (mx, my) = ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
    let ang = (q.1 - p.1).atan2(q.0 - p.0);
    let ang = if !(-std::f64::consts::FRAC_PI_2..=std::f64::consts::FRAC_PI_2).contains(&ang) { ang + std::f64::consts::PI } else { ang };
    let (ca, sa) = (ang.cos(), ang.sin());
    let w = crate::callouts::text_w(&label, 6.0);
    // text along the line, on the side away from the building; a short segment's text can be lifted
    // a line further out
    let (ox, oy) = {
        let (a, b) = (to(0.0, 0.0), to(offset.x, offset.y));
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = dx.hypot(dy).max(1e-9);
        (dx / l, dy / l)
    };
    // the text's "up" (-sa, ca) points outward, or flip which side it hangs on
    let up = if -sa * ox + ca * oy >= 0.0 { 1.0 } else { -1.0 };
    let len = (q.0 - p.0).hypot(q.1 - p.1);
    let raise = if lift && w > len - 3.0 { 7.0 } else { 0.0 };
    let base = if up > 0.0 { 2.0 + raise } else { -6.5 - raise };
    let (tx, ty) = (mx - ca * w / 2.0 - sa * base, my - sa * w / 2.0 + ca * base);
    cv.text(tx, ty, &label, 6.0, false, pens.ink, ang);
}

// ---- elevations, sections, 3D views ---------------------------------------------------------

/// The way a named standard view looks (from outside, in), as the picture views have always had it.
fn standard_look(name: &str) -> DVec3 {
    let eye = match name {
        "front" => DVec3::new(0.0, -1.0, 0.12),
        "back" => DVec3::new(0.0, 1.0, 0.12),
        "left" => DVec3::new(-1.0, 0.0, 0.12),
        "right" => DVec3::new(1.0, 0.0, 0.12),
        "top" => DVec3::new(0.0, -0.001, 1.0),
        _ => DVec3::new(1.0, -1.35, 0.95),
    };
    -eye.normalize()
}

pub(super) fn view(cv: &mut Cv, m: &Model, bm: &Model, v: &ViewportSpec, pens: &Pens) -> Result<(String, Option<f64>)> {
    let sh = cv.h_mm;
    let y = |mm: f64| (sh - mm) * PT;
    let mut label = v.label.clone().unwrap_or_else(|| v.view.clone());
    // a true elevation: looking dead level at the side, from the middle
    let elev_dir = match v.view.as_str() {
        "front" => Some(DVec3::new(0.0, -1.0, 0.0)),
        "back" => Some(DVec3::new(0.0, 1.0, 0.0)),
        "left" => Some(DVec3::new(-1.0, 0.0, 0.0)),
        "right" => Some(DVec3::new(1.0, 0.0, 0.0)),
        _ => None,
    }
    .filter(|_| v.callouts && v.scale.is_some());
    // a section looks along `look` from the far side of its plane
    let sec = v.section.map(|s| (DVec3::new(s[0], s[1], s[2]), DVec3::new(s[3], s[4], 0.0).normalize_or_zero()));
    let elev_dir = if let Some((_, look)) = sec { Some(-look) } else { elev_dir };
    let bounds = bm.world_bounds();
    let denom = match &v.scale {
        Some(s) => Some(scale_of(s)?),
        None => None,
    };
    if let (Some(s), Some(d)) = (&v.scale, denom) {
        label = format!("{label}   {}", scale_label(s, d));
    }
    // a perspective or unscaled view is a picture
    let Some(d) = denom.filter(|_| !v.raster) else {
        raster(cv, m, v, pens, sec, bounds, elev_dir);
        return Ok((label, denom));
    };
    let saved = m.views.iter().find(|s| s.name == v.view);
    let center = bounds.map(|(lo, hi)| (lo + hi) / 2.0).unwrap_or(DVec3::ZERO);
    let (look, auto_anchor) = if let Some((_, look)) = sec {
        (look, center)
    } else if let Some(c) = &v.camera {
        // a saved sheet's viewport with a camera of its own
        (DVec3::from(c.target) - DVec3::from(c.eye), DVec3::from(c.target))
    } else if let Some(s) = saved {
        (DVec3::from(s.target) - DVec3::from(s.eye), DVec3::from(s.target))
    } else if let Some(dir) = elev_dir {
        (-dir, center)
    } else {
        (standard_look(&v.view), center)
    };
    if look.length_squared() < 1e-12 {
        bail!("the view \"{}\" looks at nothing", v.view);
    }
    let anchor = v.anchor.map(DVec3::from).unwrap_or(auto_anchor);
    let k = 1000.0 / d;
    let proj = Proj::new(anchor, look, k, [v.x + v.w / 2.0, v.y + v.h / 2.0]);
    let rect = [v.x, v.y, v.w, v.h];
    let hv = View { m, proj: proj.clone(), rect, clip: sec, beyond: v.beyond, pens, inject_panic: false };
    let lines = hv.lines()?;
    cv.clipped(v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT, |cv| {
        cv.out.extend(lines);
        // a section: what the plane cuts through, filled solid over the picture
        if let Some((origin, look)) = sec {
            if let Ok(cut) = m.section(origin, look) {
                // the cut is drawn in the same frame as the lines
                let to = |p: DVec3| proj.xy(p);
                for (_, outer, holes) in &cut.regions {
                    let mut segs = Vec::new();
                    for ring in std::iter::once(outer).chain(holes.iter()) {
                        for (i, p) in ring.iter().enumerate() {
                            segs.push(if i == 0 { Seg::M(to(*p)) } else { Seg::L(to(*p)) });
                        }
                        segs.push(Seg::Z);
                    }
                    cv.push(Prim::Path(PathPrim { segs, pen: Some(pens.ink(pens.heavy)), fill: Some(Fill { rgb: pens.cut_fill, even_odd: true }), prov: None }));
                }
                let thin = pens.ink(pens.thin);
                for (a, b) in &cut.segments {
                    cv.push(Prim::Path(PathPrim { segs: vec![Seg::M(to(*a)), Seg::L(to(*b))], pen: Some(thin.clone()), fill: None, prov: None }));
                }
            }
        }
        // level datums across a true elevation
        if let (Some(_), Some((lo, hi))) = (elev_dir, bounds) {
            if !v.levels.is_empty() {
                let units = if m.units.is_empty() { "m" } else { m.units.as_str() };
                let base = v.levels.first().map(|l| l.1).unwrap_or(0.0);
                // the datum lines run the width of the building, or (pinned drawings) the whole frame, so
                // what's elsewhere in the model can't change them
                let (smin, smax) = if v.anchor.is_some() {
                    let half = v.w / 2.0 / k;
                    (-half, half)
                } else {
                    // screen right, seen from outside looking in
                    let span = |p: DVec3| (p - anchor).dot(proj.right);
                    let corners = [lo, hi, DVec3::new(lo.x, hi.y, 0.0), DVec3::new(hi.x, lo.y, 0.0)];
                    corners.iter().map(|p| span(*p)).fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(s), b.max(s)))
                };
                let x0 = ((v.x + v.w / 2.0 + (smin - 0.6) * k) * PT).max(v.x * PT + 2.0);
                let x1 = ((v.x + v.w / 2.0 + (smax + 0.6) * k) * PT).min((v.x + v.w) * PT - 60.0);
                for (name, h) in &v.levels {
                    let yy = y(proj.xy(anchor + DVec3::Z * (h - anchor.z))[1]);
                    crate::callouts::level(cv, pens, x0, x1, yy, name, &format!("{}{}", if h - base < -1e-3 { "-" } else { "+" }, crate::units::format((h - base).abs(), units)));
                }
            }
        }
    });
    Ok((label, Some(d)))
}

/// A perspective (or unscaled) view as a picture on the page.
fn raster(cv: &mut Cv, m: &Model, v: &ViewportSpec, pens: &Pens, sec: Option<(DVec3, DVec3)>, bounds: Option<(DVec3, DVec3)>, elev_dir: Option<DVec3>) {
    // drawings carry their own notes; the model's plan labels don't belong on elevations
    let mut o = RenderOpts { paper: true, grid: false, blueprint: pens.blueprint, annotations: !v.callouts, ..Default::default() };
    let px_per_mm = (6.0f64).min(4000.0 / v.w.max(v.h));
    o.width = (v.w * px_per_mm) as u32;
    o.height = (v.h * px_per_mm) as u32;
    if let (Some((origin, look)), Some((lo, hi))) = (sec, bounds) {
        let center = (lo + hi) / 2.0;
        let eye = center - look * ((hi - lo).length() + 10.0);
        o.camera = Some(CameraSpec { eye: eye.to_array(), target: center.to_array(), fov: 40.0 });
        o.clip = Some([origin.x, origin.y, origin.z, look.x, look.y, look.z]);
    } else if let Some(c) = &v.camera {
        o.camera = Some(c.clone());
    } else if let Some(saved) = m.views.iter().find(|s| s.name == v.view) {
        o.camera = Some(CameraSpec { eye: saved.eye, target: saved.target, fov: saved.fov });
    } else if let (Some(d), Some((lo, hi))) = (elev_dir, bounds) {
        let center = (lo + hi) / 2.0;
        let eye = center + d * ((hi - lo).length() + 10.0);
        o.camera = Some(CameraSpec { eye: eye.to_array(), target: center.to_array(), fov: 40.0 });
    } else {
        o.view = v.view.clone();
    }
    if let Some(s) = &v.scale {
        if let Ok(d) = scale_of(s) {
            o.parallel = true;
            o.ortho_height = Some(v.h * d / 1000.0);
        }
    }
    let img = render::render(m, &o);
    cv.push(Prim::Group(Group { clip: Some([v.x, v.y, v.w, v.h]), alt: None, opacity: 1.0, children: vec![Prim::Image(ImagePrim { rgb: std::sync::Arc::new(img), rect: [v.x, v.y, v.w, v.h] })] }));
}
