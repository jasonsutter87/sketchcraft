//! Drawing sheets (a small LayOut): pages with model viewports (saved views,
//! optionally at a true scale like 1:100), a frame, labels with scale bars,
//! and a title block. Written as a vector PDF; the views are images.

use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};
use anyhow::{bail, Context, Result};
use glam::DVec3;
use serde::Deserialize;

/// Sheet sizes, landscape, in millimeters.
pub fn sheet_mm(name: &str) -> Result<(f64, f64)> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "a4" => (297.0, 210.0),
        "a3" => (420.0, 297.0),
        "a2" => (594.0, 420.0),
        "letter" => (279.4, 215.9),
        "tabloid" | "11x17" => (431.8, 279.4),
        _ => bail!("sheet is a4, a3, a2, letter or tabloid"),
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ViewportSpec {
    /// a saved view by name, or a standard one: iso, top, front, back, left, right
    pub view: String,
    /// "1:100" style; makes the view parallel and to scale
    pub scale: Option<String>,
    /// position and size on the sheet, mm from the top left (auto when 0)
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// caption (default: the view's name)
    pub label: Option<String>,
    /// a floor plan instead of a view: the model cut this many meters up,
    /// drawn as vector fills (walls solid, openings open)
    pub cut: Option<f64>,
    /// a floor plan's floor height (default: 4 ft below the cut); what sits
    /// between it and the cut is drawn in thin lines (stairs, counters, fixtures)
    pub floor: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PageSpec {
    pub title: String,
    pub viewports: Vec<ViewportSpec>,
}

/// "1:100" -> 100
fn scale_of(s: &str) -> Result<f64> {
    let d = s.trim().split(':').nth(1).unwrap_or(s).trim().parse::<f64>().context("scale looks like 1:100")?;
    if d <= 0.0 {
        bail!("scale must be positive");
    }
    Ok(d)
}

struct Page {
    content: String,
    images: Vec<image::RgbImage>,
}

const PT: f64 = 72.0 / 25.4;

/// Lay viewports out in a grid where they have no size of their own.
fn place(vps: &mut [ViewportSpec], sw: f64, sh: f64) {
    let n = vps.len().max(1);
    let cols = if n == 1 { 1 } else { 2 };
    let rows = n.div_ceil(cols);
    let (left, top, right, bottom) = (15.0, 15.0, 15.0, 52.0); // room for the title block
    let (cw, ch) = ((sw - left - right) / cols as f64, (sh - top - bottom) / rows as f64);
    for (i, v) in vps.iter_mut().enumerate() {
        if v.w <= 0.0 || v.h <= 0.0 {
            let (c, r) = (i % cols, i / cols);
            v.x = left + c as f64 * cw + 4.0;
            v.y = top + r as f64 * ch + 4.0;
            v.w = cw - 8.0;
            v.h = ch - 14.0; // caption below
        }
    }
}

fn esc(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).map(|c| if matches!(c, '(' | ')' | '\\') { format!("\\{c}") } else { c.to_string() }).collect()
}

pub fn write_sheets(m: &Model, path: &str, sheet: &str, project: &str, mut pages: Vec<PageSpec>, blueprint: bool, notes: &[String]) -> Result<usize> {
    let (sw, sh) = sheet_mm(sheet)?;
    // saved views in a grid, or the four standard views
    let defaults: Vec<ViewportSpec> = if m.views.is_empty() { ["iso", "top", "front", "right"].iter().map(|s| s.to_string()).collect::<Vec<_>>() } else { m.views.iter().take(4).map(|v| v.name.clone()).collect() }
        .into_iter()
        .map(|view| ViewportSpec { view, ..Default::default() })
        .collect();
    if pages.is_empty() {
        pages.push(PageSpec { title: "Views".into(), viewports: vec![] });
    }
    for p in &mut pages {
        if p.viewports.is_empty() {
            p.viewports = defaults.clone();
        }
    }
    if pages.len() > 50 {
        bail!("at most 50 pages");
    }
    let date = today();
    let total = pages.len();
    let mut out_pages = Vec::new();
    for (pi, page) in pages.iter_mut().enumerate() {
        place(&mut page.viewports, sw, sh);
        let y = |mm: f64| (sh - mm) * PT; // top-left mm -> PDF points
        let mut c = String::new();
        if blueprint {
            // the whole sheet blueprint blue; frames, text and lines in white
            let b = crate::render::BLUEPRINT;
            c += &format!("{} {} {} rg 0 0 {:.2} {:.2} re f 0.93 0.96 1 rg 0.93 0.96 1 RG\n", b[0], b[1], b[2], sw * PT, sh * PT);
        }
        let mut images = Vec::new();
        // sheet border
        c += &format!("0.6 w {:.2} {:.2} {:.2} {:.2} re S\n", 10.0 * PT, 10.0 * PT, (sw - 20.0) * PT, (sh - 20.0) * PT);
        for v in &page.viewports {
            if let Some(z) = v.cut {
                c += &plan_cut(m, v, z, &y, blueprint)?;
                continue;
            }
            let mut o = RenderOpts { paper: true, grid: false, blueprint, ..Default::default() };
            let px_per_mm = (6.0f64).min(4000.0 / v.w.max(v.h));
            o.width = (v.w * px_per_mm) as u32;
            o.height = (v.h * px_per_mm) as u32;
            let mut label = v.label.clone().unwrap_or_else(|| v.view.clone());
            if let Some(saved) = m.views.iter().find(|s| s.name == v.view) {
                o.camera = Some(CameraSpec { eye: saved.eye, target: saved.target, fov: saved.fov });
            } else {
                o.view = v.view.clone();
            }
            let mut denom = None;
            if let Some(s) = &v.scale {
                let d = scale_of(s)?;
                o.parallel = true;
                o.ortho_height = Some(v.h * d / 1000.0);
                label = format!("{label}   1:{d}");
                denom = Some(d);
            }
            images.push(render::render(m, &o));
            let n = images.len();
            c += &format!("q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{n} Do Q\n", v.w * PT, v.h * PT, v.x * PT, y(v.y + v.h));
            c += &format!("0.3 w {:.2} {:.2} {:.2} {:.2} re S\n", v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT);
            c += &format!("BT /F2 8 Tf {:.2} {:.2} Td ({}) Tj ET\n", v.x * PT, y(v.y + v.h + 5.0), esc(&label));
            if let Some(d) = denom {
                c += &scale_bar(v, d, &y);
            }
        }
        // notes (areas, general notes) in a box above the title block
        if !notes.is_empty() {
            // beside the title block, bottom-aligned with it
            let (tw, lh) = (70.0f64, 4.2);
            let nh = 6.0 + notes.len() as f64 * lh;
            let title_w = 110.0f64.min(sw * 0.4);
            let (nx, ny) = (sw - 10.0 - title_w - tw, sh - 10.0 - nh);
            c += &format!("0.6 w {:.2} {:.2} {:.2} {:.2} re S\n", nx * PT, y(ny + nh), tw * PT, nh * PT);
            for (i, line) in notes.iter().enumerate() {
                let bold = line.starts_with('#');
                let text = line.trim_start_matches('#').trim();
                c += &format!("BT /{} 6.5 Tf {:.2} {:.2} Td ({}) Tj ET\n", if bold { "F2" } else { "F1" }, (nx + 4.0) * PT, y(ny + 5.5 + i as f64 * lh), esc(text));
            }
        }
        // title block, bottom right
        let (tw, th) = (110.0f64.min(sw * 0.4), 32.0);
        let (tx, ty) = (sw - 10.0 - tw, sh - 10.0 - th);
        c += &format!("0.6 w {:.2} {:.2} {:.2} {:.2} re S\n", tx * PT, y(ty + th), tw * PT, th * PT);
        c += &format!("0.3 w {:.2} {:.2} m {:.2} {:.2} l S\n", tx * PT, y(ty + 14.0), (tx + tw) * PT, y(ty + 14.0));
        c += &format!("BT /F2 12 Tf {:.2} {:.2} Td ({}) Tj ET\n", (tx + 4.0) * PT, y(ty + 9.0), esc(project));
        c += &format!("BT /F1 9 Tf {:.2} {:.2} Td ({}) Tj ET\n", (tx + 4.0) * PT, y(ty + 21.0), esc(&page.title));
        c += &format!("BT /F1 7 Tf {:.2} {:.2} Td ({}   Sheet {} of {}   Drawn in SketchCraft) Tj ET\n", (tx + 4.0) * PT, y(ty + 28.5), date, pi + 1, total);
        out_pages.push(Page { content: c, images });
    }
    write_pdf(path, sw * PT, sh * PT, &out_pages)?;
    Ok(total)
}

/// A floor plan viewport: the section at height `z` as vector paths, to
/// scale (or fitted), with its caption and scale bar.
fn plan_cut(m: &Model, v: &ViewportSpec, z: f64, y: &dyn Fn(f64) -> f64, blueprint: bool) -> Result<String> {
    let cut = m.section(glam::DVec3::new(0.0, 0.0, z), glam::DVec3::new(0.0, 0.0, -1.0))?;
    let mut pts: Vec<(f64, f64)> = cut.segments.iter().flat_map(|(a, b)| [(a.x, a.y), (b.x, b.y)]).collect();
    // dimension strings on this floor count toward the plan's size
    pts.extend(dims_on_floor(m, v.floor.unwrap_or(z - 1.22)));
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
    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // plan +Y is up the sheet
    let to = |px: f64, py: f64| -> (f64, f64) { ((cx + (px - mx) * k) * PT, y(cy - (py - my) * k)) };
    let mut c = String::new();
    c += &format!("q {:.2} {:.2} {:.2} {:.2} re W n\n", v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT);
    // cut walls: dark fill, holes left open (even-odd)
    // cut walls: solid dark on paper; pale with white outlines on a blueprint
    c += if blueprint { "0.62 0.74 0.88 rg 0.93 0.96 1 RG 0.5 w\n" } else { "0.16 0.17 0.2 rg 0.1 0.1 0.12 RG 0.4 w\n" };
    for (_, outer, holes) in &cut.regions {
        for ring in std::iter::once(outer).chain(holes.iter()) {
            for (i, p) in ring.iter().enumerate() {
                let (a, b) = to(p.x, p.y);
                c += &format!("{a:.2} {b:.2} {}\n", if i == 0 { "m" } else { "l" });
            }
            c += "h\n";
        }
        c += "B*\n";
    }
    // everything else the cut passes through (glass, frames): thin lines
    c += if blueprint { "0.25 w 0.93 0.96 1 RG\n" } else { "0.25 w 0.15 0.15 0.18 RG\n" };
    for (a, b) in &cut.segments {
        let (p, q) = (to(a.x, a.y), to(b.x, b.y));
        c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
    }
    // what's below the cut on this floor, seen from above: stair treads,
    // counters, fixtures (finer and lighter than the cut)
    let floor = v.floor.unwrap_or(z - 1.22);
    c += if blueprint { "0.2 w 0.75 0.84 0.95 RG\n" } else { "0.2 w 0.45 0.46 0.5 RG\n" };
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
        let (p, q) = (to(e.a.x, e.a.y), to(e.b.x, e.b.y));
        c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
    }
    // stairs: on the floor they start from, the treads up to the cut with UP;
    // on the floor they arrive at, the whole run seen through the opening with DN
    let ink = if blueprint { "0.93 0.96 1" } else { "0.1 0.1 0.12" };
    let lift = 1.22; // the cut height above each floor
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
            c += &format!("0.2 w {ink} RG\n");
            for (a, b) in &es {
                if (a.x - b.x).hypot(a.y - b.y) < 0.01 {
                    continue;
                }
                let (p, q) = (to(a.x, a.y), to(b.x, b.y));
                c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
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
        c += &format!("0.5 w {ink} RG {:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
        c += &format!("{ink} rg {:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l h f\n", q.0, q.1, q.0 - ux - uy * 0.5, q.1 - uy + ux * 0.5, q.0 - ux + uy * 0.5, q.1 - uy - ux * 0.5);
        let label = if starts_here { "UP" } else { "DN" };
        c += &format!("BT /F2 6 Tf {:.2} {:.2} Td ({label}) Tj ET\n", p.0 + 3.0, p.1 - 8.0);
        let _ = lift;
    }
    // overhead: shelves, closets and upper cabinets above the cut, dashed
    c += &format!("[2 1.5] 0 d 0.25 w {ink} RG\n");
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
            let (p, q) = (to(a.x, a.y), to(b.x, b.y));
            c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
        }
    }
    c += "[] 0 d\n";
    // notes and dimensions placed on this floor (room names, overall sizes)
    let units = if m.units.is_empty() { "m" } else { m.units.as_str() };
    let on_floor = |pz: f64| pz > floor - 0.05 && pz < floor + 1.0;
    for a in &m.annotations {
        match a {
            crate::model::Annotation::Text { at, text, .. } if on_floor(at.z) => {
                // centered, line by line
                let lines: Vec<&str> = text.lines().collect();
                let (px, py) = to(at.x, at.y);
                for (i, line) in lines.iter().enumerate() {
                    let size = if i == 0 { 7.0 } else { 5.5 };
                    let w = line.len() as f64 * size * 0.52;
                    let font = if i == 0 { "F2" } else { "F1" };
                    let yy = py + (lines.len() as f64 / 2.0 - i as f64 - 0.8) * 8.0;
                    c += &format!("{ink} rg BT /{font} {size} Tf {:.2} {:.2} Td ({}) Tj ET\n", px - w / 2.0, yy, esc(line));
                }
            }
            crate::model::Annotation::Swing { hinge, from, to: open, .. } if on_floor(hinge.z) => {
                let (h, o) = (to(hinge.x, hinge.y), to(open.x, open.y));
                c += &format!("0.45 w {ink} RG {:.2} {:.2} m {:.2} {:.2} l S\n", h.0, h.1, o.0, o.1);
                let arc = swing_arc(*hinge, *from, *open, 16);
                c += "0.2 w ";
                for (i, p) in arc.iter().enumerate() {
                    let q = to(p.x, p.y);
                    c += &format!("{:.2} {:.2} {} ", q.0, q.1, if i == 0 { "m" } else { "l" });
                }
                c += "S\n";
            }
            crate::model::Annotation::Dimension { a: da, b: db, offset, .. } if on_floor(da.z) && on_floor(db.z) => {
                let (pa, pb) = (*da + *offset, *db + *offset);
                let (p, q) = (to(pa.x, pa.y), to(pb.x, pb.y));
                let (o1, o2) = (to(da.x, da.y), to(db.x, db.y));
                c += &format!("0.3 w {ink} RG {:.2} {:.2} m {:.2} {:.2} l S\n", p.0, p.1, q.0, q.1);
                // extension lines and tick marks
                for (from, at) in [(o1, p), (o2, q)] {
                    let (dx, dy) = (at.0 - from.0, at.1 - from.1);
                    let l = dx.hypot(dy).max(1e-6);
                    let gap = 2.0f64.min(l * 0.3);
                    c += &format!("0.2 w {:.2} {:.2} m {:.2} {:.2} l S\n", from.0 + dx / l * gap, from.1 + dy / l * gap, at.0 + dx / l * 2.0, at.1 + dy / l * 2.0);
                    c += &format!("0.5 w {:.2} {:.2} m {:.2} {:.2} l S\n", at.0 - 2.5, at.1 - 2.5, at.0 + 2.5, at.1 + 2.5);
                }
                let len = da.distance(*db);
                let label = crate::units::format(len, units);
                let (mx, my) = ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
                let ang = (q.1 - p.1).atan2(q.0 - p.0);
                let ang = if ang > std::f64::consts::FRAC_PI_2 || ang < -std::f64::consts::FRAC_PI_2 { ang + std::f64::consts::PI } else { ang };
                let (ca, sa) = (ang.cos(), ang.sin());
                let w = label.len() as f64 * 6.0 * 0.5;
                // text along the line, just above it
                let (tx, ty) = (mx - ca * w / 2.0 - sa * 2.5, my - sa * w / 2.0 + ca * 2.5);
                c += &format!("{ink} rg BT /F1 6 Tf {ca:.4} {sa:.4} {:.4} {ca:.4} {tx:.2} {ty:.2} Tm ({}) Tj ET\n", -sa, esc(&label));
            }
            _ => {}
        }
    }
    c += if blueprint { "Q 0.93 0.96 1 rg 0.93 0.96 1 RG\n" } else { "Q 0 g 0 G\n" };
    let label = v.label.clone().unwrap_or_else(|| format!("Plan at {z:.2} m"));
    let label = if v.scale.is_some() { format!("{label}   1:{denom}") } else { label };
    c += &format!("0.3 w {:.2} {:.2} {:.2} {:.2} re S\n", v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT);
    c += &format!("BT /F2 8 Tf {:.2} {:.2} Td ({}) Tj ET\n", v.x * PT, y(v.y + v.h + 5.0), esc(&label));
    if v.scale.is_some() {
        c += &scale_bar(v, denom, y);
    }
    Ok(c)
}

/// Points along a door's swing, from closed to open, around the hinge.
pub fn swing_arc(hinge: DVec3, from: DVec3, to: DVec3, steps: usize) -> Vec<DVec3> {
    let (a, b) = (from - hinge, to - hinge);
    let r = a.length();
    let ang = a.normalize_or_zero().dot(b.normalize_or_zero()).clamp(-1.0, 1.0).acos();
    let axis = a.cross(b).normalize_or_zero();
    (0..=steps)
        .map(|k| {
            let q = glam::DQuat::from_axis_angle(if axis == DVec3::ZERO { DVec3::Z } else { axis }, ang * k as f64 / steps as f64);
            hinge + (q * a).normalize_or_zero() * r
        })
        .collect()
}

/// Where a floor's dimension lines run (with a margin for their labels).
fn dims_on_floor(m: &Model, floor: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for a in &m.annotations {
        if let crate::model::Annotation::Dimension { a, b, offset, .. } = a {
            if a.z > floor - 0.05 && a.z < floor + 1.0 {
                for p in [*a + *offset * 1.25, *b + *offset * 1.25] {
                    out.push((p.x, p.y));
                }
            }
        }
    }
    out
}

/// A scale bar under a viewport: a round length no longer than 40 mm.
fn scale_bar(v: &ViewportSpec, d: f64, y: &dyn Fn(f64) -> f64) -> String {
    let max_m = 40.0 * d / 1000.0;
    let len_m = [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0].iter().copied().filter(|l| *l <= max_m).last().unwrap_or(max_m);
    let len_mm = len_m * 1000.0 / d;
    let (bx, by) = (v.x + v.w - len_mm, v.y + v.h + 3.5);
    let mut c = format!("0.5 w {:.2} {:.2} m {:.2} {:.2} l S\n", bx * PT, y(by), (bx + len_mm) * PT, y(by));
    for k in 0..=4 {
        let tx = bx + len_mm * k as f64 / 4.0;
        c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", tx * PT, y(by - 1.0), tx * PT, y(by + 1.0));
    }
    c += &format!("BT /F1 6 Tf {:.2} {:.2} Td ({} m) Tj ET\n", (bx + len_mm + 1.0) * PT, y(by + 1.0), len_m);
    c
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = yoe + era * 400 + i64::from(mo <= 2);
    format!("{yr:04}-{mo:02}-{d:02}")
}

fn write_pdf(path: &str, pw: f64, ph: f64, pages: &[Page]) -> Result<()> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;
    let mut objs: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()]; // 1 catalog, 2 pages: filled last
    let font = |name: &str| format!("<< /Type /Font /Subtype /Type1 /BaseFont /{name} >>").into_bytes();
    objs.push(font("Helvetica")); // 3
    objs.push(font("Helvetica-Bold")); // 4
    let mut kids = Vec::new();
    for p in pages {
        let mut xobj = String::new();
        for (i, img) in p.images.iter().enumerate() {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(img.as_raw())?;
            let data = enc.finish()?;
            let mut o = format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", img.width(), img.height(), data.len()).into_bytes();
            o.extend(&data);
            o.extend(b"\nendstream");
            objs.push(o);
            xobj += &format!("/Im{} {} 0 R ", i + 1, objs.len());
        }
        let mut cs = format!("<< /Length {} >>\nstream\n", p.content.len()).into_bytes();
        cs.extend(p.content.as_bytes());
        cs.extend(b"endstream");
        objs.push(cs);
        let content = objs.len();
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> /XObject << {xobj}>> >> /Contents {content} 0 R >>").into_bytes());
        kids.push(format!("{} 0 R", objs.len()));
    }
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()).into_bytes();
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in &offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// The floors of a building: heights of big upward faces (slabs, floors,
/// the ground) where a cut 4 ft above actually goes through something.
pub fn floor_levels(m: &Model) -> Vec<f64> {
    let faces = m.world_faces();
    let Some((lo, hi)) = m.world_bounds() else { return vec![] };
    let mut zs: Vec<(f64, f64)> = faces
        .iter()
        .filter(|f| f.normal.normalize_or_zero().z > 0.99)
        .filter_map(|f| {
            // a floor is a filled area: the top of a ring of walls isn't
            let net = crate::geom::area(&f.outer) - f.holes.iter().map(|h| crate::geom::area(h)).sum::<f64>();
            let (mut a, mut b) = (f.outer[0], f.outer[0]);
            for p in &f.outer {
                a = a.min(*p);
                b = b.max(*p);
            }
            let boxed = ((b.x - a.x) * (b.y - a.y)).max(1e-9);
            (net > 4.0 && net / boxed > 0.5).then_some((f.outer[0].z, net))
        })
        .collect();
    zs.push((lo.z.max(0.0).min(hi.z), f64::MAX)); // the ground
    zs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut levels: Vec<f64> = Vec::new();
    for (z, _) in zs {
        if levels.last().map_or(true, |l| z - l > 1.8) {
            let cut_at = z + 1.22;
            if cut_at < hi.z && m.section(glam::DVec3::new(0.0, 0.0, cut_at), glam::DVec3::new(0.0, 0.0, -1.0)).map_or(false, |c| !c.regions.is_empty()) {
                levels.push(z);
            }
        }
    }
    levels
}

/// A blueprint set: every floor's plan, then the four elevations, white on
/// blue, at the largest true scale that fits the sheet.
pub fn blueprint(m: &Model, path: &str, sheet: &str, project: &str, extra_notes: &[String]) -> Result<usize> {
    let (sw, sh) = sheet_mm(sheet)?;
    let Some((lo, hi)) = m.world_bounds() else { bail!("nothing to draw") };
    let size = hi - lo;
    let levels = floor_levels(m);
    if levels.is_empty() {
        bail!("no floors found to draw plans of (draw walls standing on a floor or the ground)");
    }
    let fit = |w_m: f64, h_m: f64, w_mm: f64, h_mm: f64| -> String {
        let d = [50.0, 75.0, 100.0, 150.0, 200.0, 250.0, 500.0, 1000.0].into_iter().find(|d| w_m * 1000.0 / d <= w_mm * 0.92 && h_m * 1000.0 / d <= h_mm * 0.92).unwrap_or(2000.0);
        format!("1:{d}")
    };
    let (left, right, top, bottom) = (15.0, 15.0, 15.0, 52.0);
    let usable_w = sw - left - right;
    let usable_h = sh - top - bottom;
    // plans side by side (wrap to two rows past three floors)
    let n = levels.len();
    let cols = n.min(3);
    let rows = n.div_ceil(cols);
    let (cw, ch) = (usable_w / cols as f64, usable_h / rows as f64);
    let ordinal = |i: usize| match i {
        0 => "Main floor".to_string(),
        1 => "Upper floor".to_string(),
        k => format!("Floor {}", k + 1),
    };
    // plans are sized by what the cuts go through, not the whole site
    let (mut px, mut py) = (0.0f64, 0.0f64);
    for z in &levels {
        if let Ok(c) = m.section(glam::DVec3::new(0.0, 0.0, z + 1.22), glam::DVec3::new(0.0, 0.0, -1.0)) {
            let mut pts: Vec<_> = c.segments.iter().flat_map(|(a, b)| [*a, *b]).collect();
            pts.extend(dims_on_floor(m, *z).into_iter().map(|(x, y)| DVec3::new(x, y, 0.0)));
            if let Some(first) = pts.first() {
                let (l, h) = pts.iter().fold((*first, *first), |(l, h), p| (l.min(*p), h.max(*p)));
                px = px.max(h.x - l.x);
                py = py.max(h.y - l.y);
            }
        }
    }
    let plan_scale = fit(px.max(1.0), py.max(1.0), cw - 8.0, ch - 14.0);
    let plans: Vec<ViewportSpec> = levels
        .iter()
        .enumerate()
        .map(|(i, z)| ViewportSpec {
            view: "top".into(),
            cut: Some(z + 1.22),
            floor: Some(*z),
            scale: Some(plan_scale.clone()),
            label: Some(format!("{}  {} plan", i + 1, ordinal(i))),
            x: left + (i % cols) as f64 * cw + 4.0,
            y: top + (i / cols) as f64 * ch + 4.0,
            w: cw - 8.0,
            h: ch - 14.0,
        })
        .collect();
    // elevations: 2 x 2
    let (ew, eh) = (usable_w / 2.0 - 8.0, usable_h / 2.0 - 14.0);
    let el_scale = fit(size.x.max(size.y), size.z, ew, eh);
    let elev: Vec<ViewportSpec> = [("front", "Front"), ("right", "Right side"), ("back", "Rear"), ("left", "Left side")]
        .iter()
        .enumerate()
        .map(|(i, (v, name))| ViewportSpec {
            view: v.to_string(),
            scale: Some(el_scale.clone()),
            label: Some(format!("{}  {name} elevation", n + i + 1)),
            x: left + (i % 2) as f64 * (usable_w / 2.0) + 4.0,
            y: top + (i / 2) as f64 * (usable_h / 2.0) + 4.0,
            w: ew,
            h: eh,
            ..Default::default()
        })
        .collect();
    // gross floor areas: inside the outside face of each floor's walls
    let units = if m.units.is_empty() { "m" } else { m.units.as_str() };
    let imperial = units == "ft" || units == "in";
    let fmt_area = |a: f64| if imperial { format!("{} SF", thousands((a * 10.763_910_4).round() as i64)) } else { format!("{:.1} m2", a) };
    let mut notes = vec!["# AREA".to_string()];
    let mut total = 0.0;
    for (i, z) in levels.iter().enumerate() {
        // measured just under the ceiling, above door and window heads, where
        // the outside walls make a closed ring (not broken by doorways)
        let area_at = |h: f64| {
            m.section(DVec3::new(0.0, 0.0, z + h), DVec3::new(0.0, 0.0, -1.0))
                .map(|c| c.regions.iter().filter(|(_, _, holes)| !holes.is_empty()).map(|(_, o, _)| crate::geom::area(o)).fold(0.0, f64::max))
                .unwrap_or(0.0)
        };
        let a = [2.4, 2.3, 2.5, 1.22].iter().map(|h| area_at(*h)).find(|a| *a > 0.0).unwrap_or(0.0);
        total += a;
        notes.push(format!("{}   {}", ordinal(i).to_uppercase(), fmt_area(a)));
    }
    if levels.len() > 1 {
        notes.push(format!("# TOTAL   {}", fmt_area(total)));
    }
    notes.extend(extra_notes.iter().cloned());
    write_sheets(m, path, sheet, project, vec![PageSpec { title: "Floor plans".into(), viewports: plans }, PageSpec { title: "Elevations".into(), viewports: elev }], true, &notes)
}

fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}
