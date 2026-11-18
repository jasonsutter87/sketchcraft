//! Drawing sheets (a small LayOut): pages with model viewports (saved views,
//! optionally at a true scale like 1:100), a frame, labels with scale bars,
//! and a title block. A page is built as a display list (`draft`: plans, elevations,
//! sections and parallel views are vector, with hidden lines removed) and written as a
//! PDF (or one page as SVG or DXF).

use crate::draft::ir::Doc;
pub use crate::draft::paper::sheet_mm;
use crate::model::Model;
use crate::render::CameraSpec;
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Deserialize;

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
    /// plans: door/window tags, automatic exterior dimensions, north arrow;
    /// elevations: true elevation (level camera)
    pub callouts: bool,
    /// plans: elevation markers, labels for the front, right, back and left
    /// elevations ("3/2" = drawing 3 on sheet 2)
    pub marks: Vec<String>,
    /// elevations: level datums (name, height in m)
    pub levels: Vec<(String, f64)>,
    /// a building section: [origin x,y,z, look x,y,z]; the model cut by the
    /// vertical plane through origin, seen looking along `look`
    pub section: Option<[f64; 6]>,
    /// plans: where a section cuts, marked with a section line
    pub section_mark: Option<SectionMark>,
    /// a camera of its own (a saved sheet's 3D viewport that follows no saved view)
    pub camera: Option<CameraSpec>,
    /// the model point pinned to the middle of the viewport, meters. A drawing keeps it: nothing
    /// re-centres it later, so adding a tree (or moving a far-away object) never moves the drawing.
    /// Left out, the middle of the building (without its environment) is used.
    pub anchor: Option<[f64; 3]>,
    /// elevations, sections and parallel views: also draw what's hidden behind other things, dashed
    pub beyond: bool,
    /// draw a scaled view as a picture instead of vector line work
    pub raster: bool,
    /// 0 to 1 (left out: 1). The whole viewport, lines and pictures together, shows through this much.
    pub opacity: Option<f64>,
    /// plans with callouts: leave out the worked-out exterior dimension strings (the sheet has saved, attached ones
    /// from `dimension_auto` instead). Tags, markers and the north arrow stay.
    #[serde(skip)]
    pub no_auto_dims: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SectionMark {
    pub origin: [f64; 3],
    pub look: [f64; 3],
    /// "A/3": section A, on sheet 3
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PageSpec {
    pub title: String,
    pub viewports: Vec<ViewportSpec>,
    /// a table on the page (first row is the header), e.g. a schedule
    pub table: Vec<Vec<String>>,
    /// saved dimensions worked out against the live model (set when a saved sheet is exported; never in a command)
    #[serde(skip)]
    pub dims: Vec<crate::draft::dims::DimDraw>,
    /// paper line work saved on the sheet, in stacking order (set when a saved sheet is exported)
    #[serde(skip)]
    pub lines: Vec<crate::draft::linework::ShapeDraw>,
}

// the scale parser and the grid placer, where the saved-sheets commands (#61) look for them
#[allow(unused_imports)]
pub(crate) use crate::draft::paper::scale_of;
#[allow(unused_imports)]
pub(crate) use crate::draft::sheet::place;

/// Saved views in a grid, or the four standard views: what a page with nothing on it shows.
pub(crate) fn default_specs(m: &Model) -> Vec<ViewportSpec> {
    if m.views.is_empty() { ["iso", "top", "front", "right"].iter().map(|s| s.to_string()).collect::<Vec<_>>() } else { m.views.iter().take(4).map(|v| v.name.clone()).collect() }
        .into_iter()
        .map(|view| ViewportSpec { view, ..Default::default() })
        .collect()
}

/// Sheets as a PDF. Returns how many pages the set has.
pub fn write_sheets(m: &Model, path: &str, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String]) -> Result<usize> {
    write_pages(m, path, sheet, project, pages, blueprint, notes, true)
}

/// `write_sheets`; `fill` false: a page with nothing on it is drawn empty, not given the model's views.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_pages(m: &Model, path: &str, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String], fill: bool) -> Result<usize> {
    write_sheets_page(m, path, sheet, project, pages, blueprint, notes, fill, None)
}

/// Sheets to a file by its extension: a PDF of every page, or (`.svg`, `.dxf`) one page of them,
/// `page` counting from 1. Returns how many pages the set has.
#[allow(clippy::too_many_arguments)]
pub fn write_sheets_page(m: &Model, path: &str, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String], fill: bool, page: Option<usize>) -> Result<usize> {
    Ok(write_sheets_notes(m, path, sheet, project, pages, blueprint, notes, fill, page)?.0)
}

/// `write_sheets_page`, and what the format couldn't carry (a DXF has no opacity: the export says so).
#[allow(clippy::too_many_arguments)]
pub fn write_sheets_notes(m: &Model, path: &str, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String], fill: bool, page: Option<usize>) -> Result<(usize, Vec<String>)> {
    let mut doc = crate::draft::sheet::build_with(m, sheet, project, pages, blueprint, notes, &today(), fill)?;
    doc.lang = locale_lang();
    write_doc(&doc, path, page)?;
    Ok((doc.pages.len(), export_warnings(&doc, path, page)))
}

/// What a file of this kind leaves out of `doc`'s page (so the export can say so).
pub fn export_warnings(doc: &Doc, path: &str, page: Option<usize>) -> Vec<String> {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    // a PDF holds every page; an SVG or DXF holds one
    let pages: Vec<&crate::draft::ir::Page> = if ext == "pdf" { doc.pages.iter().collect() } else { page.unwrap_or(1).checked_sub(1).and_then(|i| doc.pages.get(i)).into_iter().collect() };
    let mut out = hatch_warnings(&pages);
    if ext == "dxf" {
        let n = pages.first().map_or(0, |p| crate::draft::dxf::dropped_opacity(p));
        if n > 0 {
            out.push(format!("DXF has no opacity: {n} viewport{} drawn at full strength (PDF and SVG keep it)", if n == 1 { " is" } else { "s are" }));
        }
    }
    out
}

/// Hatches that were drawn as a plain tint (too dense, or a pattern the file lacks), each reason once with a count.
pub fn hatch_warnings(pages: &[&crate::draft::ir::Page]) -> Vec<String> {
    let mut why: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for p in pages {
        p.walk(&mut |x| {
            if let crate::draft::ir::Prim::Hatch(h) = x {
                if let Some(n) = &h.note {
                    *why.entry((h.name.clone(), n.clone())).or_default() += 1;
                }
            }
        });
    }
    why.into_iter().map(|((name, n), c)| format!("hatch {name:?} drawn as a plain tint in {c} region{}: {n}", if c == 1 { "" } else { "s" })).collect()
}

/// The language tag for the PDF (screen readers pronounce by it): from the machine's locale (`de_DE.UTF-8` is
/// `de`), English when it names none.
fn locale_lang() -> String {
    for k in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(k) {
            let lang: String = v.split(['_', '.', '@']).next().unwrap_or("").to_ascii_lowercase();
            if (2..=3).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_lowercase()) {
                return lang;
            }
        }
    }
    "en".into()
}

/// A built set to a file, by its extension: PDF (every page), SVG or DXF (one page).
pub fn write_doc(doc: &Doc, path: &str, page: Option<usize>) -> Result<()> {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let bytes: Vec<u8> = match ext.as_str() {
        "svg" | "dxf" => {
            let n = page.unwrap_or(1);
            let Some(p) = n.checked_sub(1).and_then(|i| doc.pages.get(i)) else { bail!("there is no page {n} (the set has {})", doc.pages.len()) };
            if ext == "svg" { crate::draft::svg::page(p).into_bytes() } else { crate::draft::dxf::page(p).into_bytes() }
        }
        _ => crate::draft::pdf::write(doc)?,
    };
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// What a plan works out for itself: dimension chains, tags, where the
/// elevation markers go, and every point they reach (for fitting the plan).
#[derive(Default)]
pub(crate) struct PlanExtras {
    pub dims: Vec<(DVec3, DVec3, DVec3)>,
    /// (where, "W1", 'W')
    pub tags: Vec<(DVec3, String, char)>,
    /// front, right, back, left: where, and the way it looks (plan axes)
    pub marks: Vec<(DVec3, DVec3)>,
    pub reach: Vec<(f64, f64)>,
}

pub(crate) fn plan_extras(m: &Model, floor: f64, cut_z: f64, marks: bool) -> PlanExtras {
    let mut e = PlanExtras::default();
    let Some(box_) = crate::callouts::outline(m, cut_z) else { return e };
    let (x0, y0, x1, y1) = box_;
    let ops = crate::callouts::openings(m);
    e.dims = crate::callouts::auto_dims(m, &ops, floor, box_);
    // tags just outside the wall for exterior openings, beside interior doors
    for o in crate::callouts::on_floor(&ops, floor) {
        let c = o.center();
        let (dist_out, out) = if o.along_x {
            if (c.y - y0).abs() < (y1 - c.y).abs() { ((c.y - y0).abs(), DVec3::NEG_Y) } else { ((y1 - c.y).abs(), DVec3::Y) }
        } else if (c.x - x0).abs() < (x1 - c.x).abs() {
            ((c.x - x0).abs(), DVec3::NEG_X)
        } else {
            ((x1 - c.x).abs(), DVec3::X)
        };
        let p = if dist_out < 0.6 { DVec3::new(c.x, c.y, floor) + out * 0.5 } else { DVec3::new(c.x, c.y, floor) + if o.along_x { DVec3::Y } else { DVec3::X } * 0.45 };
        e.tags.push((p, o.tag.clone(), o.kind));
    }
    // how far out everything reaches, for the markers and the fit
    let mut far = 0.6f64;
    for (a, _, off) in &e.dims {
        let p = *a + *off;
        far = far.max(x0 - p.x).max(p.x - x1).max(y0 - p.y).max(p.y - y1);
    }
    for a in &m.annotations {
        if let crate::model::Annotation::Dimension { a, offset, .. } = a {
            if a.z > floor - 0.05 && a.z < floor + 1.0 {
                let p = *a + *offset;
                far = far.max(x0 - p.x).max(p.x - x1).max(y0 - p.y).max(p.y - y1);
            }
        }
    }
    if marks {
        // one key for all four elevations, beside the plan (east): each arrow
        // points the way its elevation looks
        e.marks = vec![(DVec3::new(x1 + far + 2.8, (y0 + y1) / 2.0, floor), DVec3::ZERO)];
    }
    for (a, b, off) in &e.dims {
        let out = off.normalize_or_zero() * 0.4;
        for p in [*a + *off + out, *b + *off + out] {
            e.reach.push((p.x, p.y));
        }
    }
    for (p, _) in &e.marks {
        e.reach.push((p.x + 1.4, p.y));
    }
    for (p, _, _) in &e.tags {
        e.reach.push((p.x, p.y));
    }
    e
}

/// Where a building section goes: along the stair run through its middle
/// (so the stair shows in profile), else down the building's middle;
/// looking toward the larger part of the building.
/// Also says whether it goes through a stair (false: down the building's middle, there is none).
pub(crate) fn section_place(m: &Model) -> Option<(DVec3, DVec3, bool)> {
    let (lo, hi) = m.world_bounds()?;
    let center = (lo + hi) / 2.0;
    let mut stair: Option<(DVec3, DVec3)> = None;
    for (gid, g) in &m.groups {
        let n = g.name.to_ascii_lowercase();
        if !n.contains("stair") || n.contains("rail") {
            continue;
        }
        for e in m.edges.values().filter(|e| e.group == Some(*gid)) {
            for p in [m.p(e.a), m.p(e.b)] {
                stair = Some(match stair {
                    None => (p, p),
                    Some((a, b)) => (a.min(p), b.max(p)),
                });
            }
        }
    }
    let through_stairs = stair.is_some();
    let (origin, look) = match stair {
        Some((a, b)) => {
            let c = (a + b) / 2.0;
            // the plane holds the run; it faces across it
            let look = if b.x - a.x < b.y - a.y { DVec3::X } else { DVec3::Y };
            let look = if (center - c).dot(look) >= 0.0 { look } else { -look };
            // just inside the stair's near side of center, so the cut goes through the treads
            (DVec3::new(c.x, c.y, 0.0), look)
        }
        None => {
            let size = hi - lo;
            let look = if size.x < size.y { DVec3::X } else { DVec3::Y };
            (DVec3::new(center.x, center.y, 0.0), look)
        }
    };
    Some((origin, look, through_stairs))
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
pub(crate) fn dims_on_floor(m: &Model, floor: f64) -> Vec<(f64, f64)> {
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

pub(crate) fn today() -> String {
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
    let (pages, notes) = blueprint_specs(m, sheet, extra_notes)?;
    write_sheets(m, path, sheet, project, pages, true, &notes)
}

/// The pages and notes of a blueprint set (what `blueprint` writes), before they're drawn.
pub fn blueprint_specs(m: &Model, sheet: &str, extra_notes: &[String]) -> Result<(Vec<PageSpec>, Vec<String>)> {
    let (pages, notes) = set_specs(m, sheet, extra_notes, &SetOpts::default())?;
    Ok((pages.into_iter().map(|(_, p)| p).collect(), notes))
}

/// How `set_specs` lays a set out: the blueprint's way (all plans on one page, a section always, pages numbered 1, 2,
/// 3...) by default; a permit set's way with `per_floor`.
#[derive(Default)]
pub(crate) struct SetOpts {
    /// one plan per page (filling it), sheets numbered A-101 (plans), A-201 (elevations), A-301 (section), A-601
    /// (schedule); a section only when there are stairs to cut through
    pub per_floor: bool,
    /// one scale for every drawing ("1:50"); none: the largest that fits each page
    pub scale: Option<String>,
}

/// A set's pages, each with its sheet number, and its notes.
pub(crate) type SetPages = (Vec<(String, PageSpec)>, Vec<String>);

/// The pages of a drawing set worked out from the building (what `blueprint` and `layout_preset` make), each with its
/// sheet number (empty for a blueprint's), and the notes (areas).
pub(crate) fn set_specs(m: &Model, sheet: &str, extra_notes: &[String], o: &SetOpts) -> Result<SetPages> {
    let (sw, sh) = sheet_mm(sheet)?;
    if let Some(s) = &o.scale {
        crate::draft::paper::scale_of(s)?;
    }
    // scales and the section's place come from the building, not the lawn and trees around it
    let building = crate::environment::without(m);
    let m = &*building;
    let Some((lo, hi)) = m.world_bounds() else { bail!("nothing to draw") };
    let size = hi - lo;
    let levels = floor_levels(m);
    if levels.is_empty() {
        bail!("no floors found to draw plans of (draw walls standing on a floor or the ground)");
    }
    let fit = |w_m: f64, h_m: f64, w_mm: f64, h_mm: f64| -> String {
        if let Some(s) = &o.scale {
            return s.clone();
        }
        let d = [50.0, 75.0, 100.0, 150.0, 200.0, 250.0, 500.0, 1000.0].into_iter().find(|d| w_m * 1000.0 / d <= w_mm * 0.92 && h_m * 1000.0 / d <= h_mm * 0.92).unwrap_or(2000.0);
        format!("1:{d}")
    };
    let (left, right, top, bottom) = (15.0, 15.0, 15.0, 52.0);
    let usable_w = sw - left - right;
    let usable_h = sh - top - bottom;
    // plans side by side (wrap to two rows past three floors)
    let n = levels.len();
    // a permit set gives each plan a page of its own
    let cols = if o.per_floor { 1 } else { n.min(3) };
    let rows = if o.per_floor { 1 } else { n.div_ceil(cols) };
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
            pts.extend(plan_extras(m, *z, z + 1.22, true).reach.into_iter().map(|(x, y)| DVec3::new(x, y, 0.0)));
            if let Some(first) = pts.first() {
                let (l, h) = pts.iter().fold((*first, *first), |(l, h), p| (l.min(*p), h.max(*p)));
                px = px.max(h.x - l.x);
                py = py.max(h.y - l.y);
            }
        }
    }
    let plan_scale = fit(px.max(1.0), py.max(1.0), cw - 8.0, ch - 14.0);
    let (elev_sheet, sec_sheet) = if o.per_floor { ("A-201", "A-301") } else { ("2", "3") };
    let plans: Vec<ViewportSpec> = levels
        .iter()
        .enumerate()
        .map(|(i, z)| ViewportSpec {
            view: "top".into(),
            cut: Some(z + 1.22),
            floor: Some(*z),
            scale: Some(plan_scale.clone()),
            label: Some(format!("{}  {} plan", i + 1, ordinal(i))),
            callouts: true,
            // markers for the four elevations (drawings n+1.. on sheet 2), on the main floor
            marks: if i == 0 { (0..4).map(|k| format!("{}/{elev_sheet}", n + k + 1)).collect() } else { vec![] },
            x: left + if o.per_floor { 0.0 } else { (i % cols) as f64 * cw } + 4.0,
            y: top + if o.per_floor { 0.0 } else { (i / cols) as f64 * ch } + 4.0,
            w: cw - 8.0,
            h: ch - 14.0,
            ..Default::default()
        })
        .collect();
    // level datums: each floor, and the top of the building
    let mut level_marks: Vec<(String, f64)> = levels.iter().enumerate().map(|(i, z)| (format!("{} FLOOR", ordinal(i).split(' ').next().unwrap_or("").to_uppercase()), *z)).collect();
    level_marks.push(("TOP".into(), hi.z));
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
            callouts: true,
            levels: level_marks.clone(),
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
                .map(|c| c.regions.iter().filter(|r| !r.holes.is_empty()).map(|r| crate::geom::area(&r.outer)).fold(0.0, f64::max))
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
    let mut pages: Vec<(String, PageSpec)> = if o.per_floor {
        plans.into_iter().enumerate().map(|(i, v)| (format!("A-{}", 101 + i), PageSpec { title: format!("{} plan", ordinal(i)), viewports: vec![v], ..Default::default() })).collect()
    } else {
        vec![(String::new(), PageSpec { title: "Floor plans".into(), viewports: plans, ..Default::default() })]
    };
    let plan_pages = pages.len();
    pages.push((if o.per_floor { elev_sheet.into() } else { String::new() }, PageSpec { title: "Elevations".into(), viewports: elev, ..Default::default() }));
    // a building section: through the stairs if there are any, looking at
    // the bigger part of the building; marked on every plan (a permit set has one only through stairs)
    if let Some((origin, look, _)) = section_place(m).filter(|s| s.2 || !o.per_floor) {
        let k = n + 5;
        let (sw_, sh_) = (usable_w - 8.0, usable_h - 14.0);
        let reach = (hi - lo).dot(DVec3::Z.cross(look).abs()).abs().max(1.0);
        let sec_scale = fit(reach, size.z, sw_, sh_);
        pages.push((if o.per_floor { sec_sheet.into() } else { String::new() }, PageSpec {
            title: "Building section".into(),
            viewports: vec![ViewportSpec {
                view: "section".into(),
                scale: Some(sec_scale),
                label: Some(format!("{k}  Section A")),
                x: left + 4.0,
                y: top + 4.0,
                w: sw_,
                h: sh_,
                callouts: true,
                levels: level_marks.clone(),
                section: Some([origin.x, origin.y, origin.z, look.x, look.y, look.z]),
                ..Default::default()
            }],
            ..Default::default()
        }));
        for (_, page) in pages.iter_mut().take(plan_pages) {
            for p in page.viewports.iter_mut() {
                p.section_mark = Some(SectionMark { origin: origin.to_array(), look: look.to_array(), label: format!("A/{sec_sheet}") });
            }
        }
    }
    // doors and windows: tagged on the plans, listed here
    let ops = crate::callouts::openings(m);
    if !ops.is_empty() {
        pages.push((if o.per_floor { "A-601".into() } else { String::new() }, PageSpec { title: "Door and window schedule".into(), table: crate::callouts::schedule(&ops, units, &levels), ..Default::default() }));
    }
    Ok((pages, notes))
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
