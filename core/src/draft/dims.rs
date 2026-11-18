//! Saved dimensions on a sheet, drawn from the live model (design step 5).
//!
//! `DimDraw` is a dimension worked out for paper: where each anchor is on the sheet now (or where it last was, when
//! it is broken) and what each span measures in the model. It is made fresh each time a sheet is built, so a moved
//! wall moves the dimension and its text. A broken anchor draws in magenta with a "!" badge at its last place, and
//! its span reads "?" (never a stale number, never silently moved).

use super::ir::*;
use crate::drafting::{DimMode, Dimension};
use crate::model::Model;
use glam::DVec3;

/// Broken anchors and their spans.
pub const BROKEN: Rgb = [214, 0, 143];

/// One dimension, ready to draw. Paper millimetres from the page's top left.
#[derive(Clone, Debug)]
pub struct DimDraw {
    /// where each anchor is on the sheet now; None: broken
    pub pts: Vec<Option<[f64; 2]>>,
    /// where each anchor last was on the sheet (where a broken one is drawn)
    pub last: Vec<[f64; 2]>,
    /// the spans: from anchor, to anchor, length in model metres (None: an end is broken)
    pub spans: Vec<(usize, usize, Option<f64>)>,
    pub mode: DimMode,
    pub offset_mm: f64,
    pub text_override: Option<String>,
    pub units: String,
}

/// The dimension worked out against the live model, or None when no viewport on its sheet can place it (a
/// perspective view has no scale to measure in).
pub fn prepare(m: &Model, d: &Dimension) -> Option<DimDraw> {
    let (_, _, proj) = m.dimension_viewport(d)?;
    let me = m.measure(d, Some((proj.right, proj.up)));
    let pts = me.points.iter().map(|p| p.map(|w| proj.xy(w))).collect();
    let last = d.anchors.iter().map(|a| proj.xy(DVec3::from(a.last))).collect();
    let spans = me.segments.iter().map(|s| (s.from, s.to, s.value)).collect();
    Some(DimDraw { pts, last, spans, mode: d.mode, offset_mm: d.offset_mm, text_override: d.text_override.clone(), units: if m.units.is_empty() { "m".into() } else { m.units.clone() } })
}

/// Draw one dimension: the line `offset_mm` out from the measured points, extension lines, ticks and the text.
pub fn draw(cv: &mut Cv, pens: &Pens, d: &DimDraw) {
    let n = d.pts.len();
    if n < 2 {
        return;
    }
    let pos: Vec<[f64; 2]> = (0..n).map(|i| d.pts[i].unwrap_or(d.last[i])).collect();
    let dir = match d.mode {
        DimMode::Horizontal => [1.0, 0.0],
        DimMode::Vertical => [0.0, 1.0],
        _ => {
            let (dx, dy) = (pos[n - 1][0] - pos[0][0], pos[n - 1][1] - pos[0][1]);
            let l = dx.hypot(dy);
            if l < 1e-9 { [1.0, 0.0] } else { [dx / l, dy / l] }
        }
    };
    // the side a positive offset goes: up the sheet for a level line, to the right for a plumb one
    let nrm = [dir[1], -dir[0]];
    let dot = |p: [f64; 2], v: [f64; 2]| p[0] * v[0] + p[1] * v[1];
    let sign = if d.offset_mm >= 0.0 { 1.0 } else { -1.0 };
    let nref = pos.iter().map(|p| dot(*p, nrm)).fold(if sign > 0.0 { f64::MIN } else { f64::MAX }, |a, b| if sign > 0.0 { a.max(b) } else { a.min(b) });
    let h_mm = cv.h_mm;
    let to = |p: [f64; 2]| (p[0] * PT, (h_mm - p[1]) * PT);
    for (k, (i, j, value)) in d.spans.iter().enumerate() {
        let broken = d.pts[*i].is_none() || d.pts[*j].is_none();
        let ink = if broken { BROKEN } else { pens.ink };
        let pen = |w: f64| Pen::new(w, ink);
        // a baseline set stacks one line per span
        let c = nref + d.offset_mm + if d.mode == DimMode::Baseline { sign * 7.0 * k as f64 } else { 0.0 };
        let on = |p: [f64; 2]| [dir[0] * dot(p, dir) + nrm[0] * c, dir[1] * dot(p, dir) + nrm[1] * c];
        let (a, b) = (on(pos[*i]), on(pos[*j]));
        let (pa, pb) = (to(a), to(b));
        cv.line(pa, pb, &pen(pens.thin));
        for (from, at) in [(pos[*i], a), (pos[*j], b)] {
            let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
            let l = dx.hypot(dy);
            if l > 1e-6 {
                let (ux, uy) = (dx / l, dy / l);
                let gap = 1.5f64.min(l * 0.3);
                cv.line(to([from[0] + ux * gap, from[1] + uy * gap]), to([at[0] + ux * 2.0, at[1] + uy * 2.0]), &pen(pens.hair));
            }
            // a 45 degree tick across the line
            let h = std::f64::consts::FRAC_1_SQRT_2 * 1.8;
            let t = [(dir[0] + nrm[0]) * h, (dir[1] + nrm[1]) * h];
            cv.line(to([at[0] - t[0], at[1] - t[1]]), to([at[0] + t[0], at[1] + t[1]]), &pen(pens.medium));
        }
        // the text, along the line, on the far side from the model
        let label = match (&d.text_override, k, value) {
            (Some(t), 0, _) => t.clone(),
            (_, _, Some(v)) => crate::units::format(*v, &d.units),
            _ => "?".to_string(),
        };
        let ang = (pb.1 - pa.1).atan2(pb.0 - pa.0);
        let ang = if !(-std::f64::consts::FRAC_PI_2..=std::f64::consts::FRAC_PI_2).contains(&ang) { ang + std::f64::consts::PI } else { ang };
        let (ca, sa) = (ang.cos(), ang.sin());
        let w = crate::callouts::text_w(&label, 6.0);
        let (mx, my) = ((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0);
        // `nrm` is in sheet millimetres (y down); the text works in points (y up)
        let out = (nrm[0] * sign, -nrm[1] * sign);
        let up = if -sa * out.0 + ca * out.1 >= 0.0 { 1.0 } else { -1.0 };
        let base = if up > 0.0 { 2.0 } else { -6.5 };
        cv.text(mx - ca * w / 2.0 - sa * base, my - sa * w / 2.0 + ca * base, &label, 6.0, broken, ink, ang);
        // a badge where a broken anchor last was
        for e in [*i, *j] {
            if d.pts[e].is_none() {
                let (bx, by) = to(d.last[e]);
                cv.circle(bx, by, 2.6, Some(&pen(pens.medium)), None);
                cv.text(bx - 0.8, by - 2.0, "!", 6.0, true, BROKEN, 0.0);
            }
        }
    }
}
