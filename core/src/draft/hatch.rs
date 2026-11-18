//! Hatch generation (design §3.3): a pattern laid over a region (an outer ring and its holes) as clipped line
//! segments, dashes applied.
//!
//! Coordinates here are millimetres with y **up** (the way `.pat` angles count, counter-clockwise), and the
//! pattern's origin is the frame's origin. On a sheet that frame is the page with its origin at the bottom left,
//! which is also the DXF's, so a CAD program regenerating the DXF `HATCH` from its pattern data draws the same
//! lines (`HatchPrim` keeps both).
//!
//! For each family: step across the region's extent one line at a time, cut each line against every ring edge,
//! keep the even-odd intervals (so holes stay open), then lay the dash pattern along the line from the line's own
//! origin (so dashes line up the same in every region). Pathological input (a tiny scale, a huge region) is caught
//! before any work by an estimate, and again while emitting: over `limit` segments, or lines closer than
//! `MIN_SPACING`, the result is empty with a reason, and the caller draws a solid tint instead.

use super::pat::{Family, Pattern};

/// Most segments one region may get (design §3.3).
pub const MAX_SEGMENTS: usize = 200_000;
/// Most line-against-edge tests one region may take.
const MAX_WORK: f64 = 5.0e7;
/// Closest two lines of a family may be, in paper mm (design §3.3).
pub const MIN_SPACING: f64 = 0.05;

/// A family placed: scaled, turned, in the frame's millimetres. `offset` is the step from one line to the next,
/// in the frame's x and y (not the line's own frame): what a DXF `HATCH` stores.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub angle: f64,
    pub base: [f64; 2],
    pub offset: [f64; 2],
    pub dash: Vec<f64>,
}

/// The pattern's families scaled by `k` (mm in the frame per mm of pattern) and turned by `angle` degrees about
/// the origin.
pub fn place(p: &Pattern, k: f64, angle: f64) -> Vec<Placed> {
    let (s, c) = angle.to_radians().sin_cos();
    let rot = |v: [f64; 2]| [v[0] * c - v[1] * s, v[0] * s + v[1] * c];
    p.families
        .iter()
        .map(|f: &Family| {
            let a = (f.angle + angle).rem_euclid(360.0);
            let (fs, fc) = f.angle.to_radians().sin_cos();
            // the step in the family's own frame (along, across) to the pattern's x, y; then turned with the rest
            let step = [f.delta[0] * fc - f.delta[1] * fs, f.delta[0] * fs + f.delta[1] * fc];
            let o = rot([f.origin[0] * k, f.origin[1] * k]);
            let st = rot([step[0] * k, step[1] * k]);
            Placed { angle: a, base: o, offset: st, dash: f.dash.iter().map(|d| d * k).collect() }
        })
        .collect()
}

/// Segments, or why there are none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hatched {
    pub lines: Vec<[[f64; 2]; 2]>,
    /// set when the region was too much to hatch (draw a tint instead)
    pub over: Option<String>,
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Hatch the region `rings` (the outer ring and its holes; even-odd, so their order and winding don't matter)
/// with `fams`. At most `limit` segments.
pub fn generate(rings: &[Vec<[f64; 2]>], fams: &[Placed], limit: usize) -> Hatched {
    let pts = rings.iter().flatten();
    let mut lo = [f64::MAX; 2];
    let mut hi = [f64::MIN; 2];
    for p in pts {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return Hatched::default();
        }
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if lo[0] > hi[0] || fams.is_empty() {
        return Hatched::default();
    }
    let corners = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    let edges: Vec<([f64; 2], [f64; 2])> = rings.iter().filter(|r| r.len() >= 3).flat_map(|r| (0..r.len()).map(move |i| (r[i], r[(i + 1) % r.len()]))).collect();
    // per family: its direction, normal, spacing and the range of line numbers that cross the extent
    struct Fam<'a> {
        d: [f64; 2],
        n: [f64; 2],
        base: [f64; 2],
        step: [f64; 2],
        i0: i64,
        i1: i64,
        dash: &'a [f64],
        period: f64,
    }
    let mut plan = Vec::new();
    let mut estimate = 0.0f64;
    let mut work = 0.0f64;
    for f in fams {
        let all_finite = f.angle.is_finite() && f.base.iter().chain(f.offset.iter()).chain(f.dash.iter()).all(|v| v.is_finite());
        if !all_finite {
            continue;
        }
        let (s, c) = f.angle.to_radians().sin_cos();
        let d = [c, s];
        let n = [-s, c];
        let spacing = dot(f.offset, n);
        if spacing.abs() < MIN_SPACING {
            return Hatched { lines: vec![], over: Some(format!("its lines would be {:.3} mm apart (closer than {MIN_SPACING} mm)", spacing.abs())) };
        }
        let across: Vec<f64> = corners.iter().map(|q| dot([q[0] - f.base[0], q[1] - f.base[1]], n) / spacing).collect();
        let (a0, a1) = across.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let (i0, i1) = (a0.ceil(), a1.floor());
        if i1 < i0 {
            continue;
        }
        let along: Vec<f64> = corners.iter().map(|q| dot(*q, d)).collect();
        let length = along.iter().fold(f64::MIN, |a, v| a.max(*v)) - along.iter().fold(f64::MAX, |a, v| a.min(*v));
        let lines = i1 - i0 + 1.0;
        let period: f64 = f.dash.iter().map(|x| x.abs()).sum();
        let per_line = if f.dash.is_empty() { 1.0 } else { (length / period.max(1e-9) + 2.0) * f.dash.iter().filter(|x| **x >= 0.0).count() as f64 };
        estimate += lines * per_line;
        work += lines * edges.len() as f64;
        if work > MAX_WORK {
            return Hatched { lines: vec![], over: Some("the region's outline is too detailed for this hatch at this scale".into()) };
        }
        if !estimate.is_finite() || estimate > limit as f64 * 4.0 || lines > limit as f64 {
            return Hatched { lines: vec![], over: Some(format!("about {} segments, more than the {limit} one region may have", fmt_count(estimate))) };
        }
        plan.push(Fam { d, n, base: f.base, step: f.offset, i0: i0 as i64, i1: i1 as i64, dash: &f.dash, period });
    }
    let mut out: Vec<[[f64; 2]; 2]> = Vec::new();
    let mut ts: Vec<f64> = Vec::new();
    let mut steps = 0usize;
    let over = |n: usize| Hatched { lines: vec![], over: Some(format!("more than {n} segments, the most one region may have")) };
    for f in &plan {
        for i in f.i0..=f.i1 {
            let p = [f.base[0] + f.step[0] * i as f64, f.base[1] + f.step[1] * i as f64];
            ts.clear();
            for (a, b) in &edges {
                let da = dot([a[0] - p[0], a[1] - p[1]], f.n);
                let db = dot([b[0] - p[0], b[1] - p[1]], f.n);
                // half-open, so a line through a vertex counts it once
                if (da > 0.0) != (db > 0.0) {
                    let t = da / (da - db);
                    let x = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    ts.push(dot([x[0] - p[0], x[1] - p[1]], f.d));
                }
            }
            if ts.len() < 2 {
                continue;
            }
            ts.sort_by(f64::total_cmp);
            let at = |t: f64| [p[0] + f.d[0] * t, p[1] + f.d[1] * t];
            for pair in ts.as_chunks::<2>().0 {
                let (t0, t1) = (pair[0], pair[1]);
                if t1 - t0 < 1e-9 {
                    continue;
                }
                if f.dash.is_empty() {
                    out.push([at(t0), at(t1)]);
                } else {
                    // the dash pattern starts at the line's own origin (t = 0) and repeats both ways
                    let mut pos = (t0 / f.period).floor() * f.period;
                    'walk: while pos <= t1 {
                        steps += 1;
                        if steps > 4 * limit {
                            return over(limit);
                        }
                        for &dl in f.dash {
                            let len = dl.abs();
                            if dl >= 0.0 {
                                let (s, e) = (pos.max(t0), (pos + len).min(t1));
                                if dl == 0.0 {
                                    if pos >= t0 && pos <= t1 {
                                        out.push([at(pos), at(pos)]);
                                    }
                                } else if e > s {
                                    out.push([at(s), at(e)]);
                                }
                                if out.len() > limit {
                                    return over(limit);
                                }
                            }
                            pos += len;
                            if pos > t1 {
                                break 'walk;
                            }
                        }
                    }
                }
                if out.len() > limit {
                    return over(limit);
                }
            }
        }
    }
    Hatched { lines: out, over: None }
}

fn fmt_count(v: f64) -> String {
    if v > 1e9 { "billions of".into() } else { format!("{}", v.round() as u64) }
}

/// A colour a little of the way from the paper toward `ink`: the tint drawn when a hatch is too dense to draw.
pub fn tint(ink: [u8; 3], paper: [u8; 3]) -> [u8; 3] {
    let m = |a: u8, b: u8| (a as f64 * 0.3 + b as f64 * 0.7).round() as u8;
    [m(ink[0], paper[0]), m(ink[1], paper[1]), m(ink[2], paper[2])]
}

/// Millimetres in the frame per millimetre of pattern: `HatchUse.scale`, and for a model-scaled hatch, divided by
/// the drawing's 1:N (`denom`), so a 100 mm brick course stays 100 mm of model at any scale.
pub fn scale_k(u: &crate::drafting::HatchUse, denom: f64) -> f64 {
    let s = if u.scale.is_finite() && u.scale > 0.0 { u.scale } else { 1.0 };
    match u.scale_mode {
        crate::drafting::ScaleMode::Paper => s,
        crate::drafting::ScaleMode::Model => s / if denom.is_finite() && denom > 0.0 { denom } else { 1.0 },
    }
}

/// A region of a drawing hatched with a tag's `HatchUse`: `rings` in paper mm, y down (the display list's
/// frame), on a page `page_h` tall, in a drawing at 1:`denom`. A pattern the file doesn't have, or a region too
/// dense to hatch, comes back as a tint with a note saying why.
pub fn on_sheet(d: &crate::drafting::Drafting, u: &crate::drafting::HatchUse, rings: Vec<Vec<[f64; 2]>>, page_h: f64, denom: f64, pens: &super::ir::Pens) -> super::ir::HatchPrim {
    use super::ir::{parse_hex, HatchPrim, Pen};
    let w = if u.weight_mm.is_finite() && u.weight_mm > 0.0 { u.weight_mm } else { pens.hair };
    let rgb = parse_hex(&u.color).unwrap_or(pens.ink);
    let pen = Pen::new(w, rgb);
    let k = scale_k(u, denom);
    let angle = if u.angle.is_finite() { u.angle } else { 0.0 };
    let up: Vec<Vec<[f64; 2]>> = rings.iter().map(|r| r.iter().map(|p| [p[0], page_h - p[1]]).collect()).collect();
    let (families, got) = match super::pat::lookup(d, &u.pattern) {
        Some(p) => {
            let fams = place(&p, k, angle);
            let got = generate(&up, &fams, MAX_SEGMENTS);
            (fams, got)
        }
        None => (Vec::new(), Hatched { lines: vec![], over: Some(format!("there is no hatch pattern {:?}", u.pattern)) }),
    };
    let lines = got.lines.iter().map(|[a, b]| [[a[0], page_h - a[1]], [b[0], page_h - b[1]]]).collect();
    HatchPrim { rings, name: u.pattern.clone(), families, angle, scale: k, pen, lines, tint: got.over.as_ref().map(|_| tint(rgb, pens.paper)), note: got.over }
}
