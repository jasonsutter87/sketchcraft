//! Pages: a `PageSpec` to a display list. Each viewport is drawn by `project` (on its own thread);
//! this adds the frame, captions, scale bars, tables, notes and the title block.

use super::ir::*;
use super::paper::sheet_mm;
use crate::layout::{PageSpec, ViewportSpec};
use crate::model::Model;
use anyhow::{bail, Result};

/// Most pages a sheet set may have (the most sheets the person's model may hold, `policy::MAX_PAGES_UI`).
pub const MAX_PAGES: usize = crate::policy::MAX_PAGES_UI;

/// Where a set's drawings come from and how its pages are numbered.
#[derive(Clone, Copy, Default)]
pub struct Opts<'a> {
    /// drawings kept between builds (the engine's); None: every viewport is drawn
    pub cache: Option<&'a super::cache::Drawings>,
    /// the number of the first page, less one ("Sheet 5 of 52" for one page drawn alone)
    pub first: usize,
    /// how many pages the whole set has (None: these)
    pub total: Option<usize>,
}

/// Lay viewports out in a grid where they have no size of their own.
pub(crate) fn place(vps: &mut [ViewportSpec], sw: f64, sh: f64) {
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

/// The pages of a sheet set. `bm` is the model without its environment: what drawings are
/// fitted and anchored to, so a far-away tree never moves a drawing.
pub fn build(m: &Model, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String], date: &str) -> Result<Doc> {
    build_with(m, sheet, project, pages, blueprint, notes, date, true)
}

/// `build`; `fill` false: a page with nothing on it is drawn empty, not given the model's views.
#[allow(clippy::too_many_arguments)]
pub fn build_with(m: &Model, sheet: &str, project: &str, pages: Vec<PageSpec>, blueprint: bool, notes: &[String], date: &str, fill: bool) -> Result<Doc> {
    build_opts(m, sheet, project, pages, blueprint, notes, date, fill, Opts::default())
}

/// `build_with`, with kept drawings and page numbers (`Opts`).
#[allow(clippy::too_many_arguments)]
pub fn build_opts(m: &Model, sheet: &str, project: &str, mut pages: Vec<PageSpec>, blueprint: bool, notes: &[String], date: &str, fill: bool, o: Opts) -> Result<Doc> {
    let (sw, sh) = sheet_mm(sheet)?;
    if fill {
        // saved views in a grid, or the four standard views
        let defaults = crate::layout::default_specs(m);
        if pages.is_empty() {
            pages.push(PageSpec { title: "Views".into(), ..Default::default() });
        }
        for p in &mut pages {
            if p.viewports.is_empty() && p.table.is_empty() {
                p.viewports = defaults.clone();
            }
        }
    }
    if pages.len() > MAX_PAGES {
        bail!("a PDF holds at most {MAX_PAGES} pages here, and this set has {}: export part of it with pages: [sheet ids]", pages.len());
    }
    let bm = crate::environment::without(m);
    let mut pens = Pens::new(blueprint);
    pens.hatches = o.cache.map(|c| c.hatches.clone());
    // the model's world geometry once for every viewport (kept with the drawings between changes)
    pens.world = Some(o.cache.map_or_else(Default::default, |c| c.world()));
    let total = o.total.unwrap_or(pages.len());
    let mut doc = Doc { title: project.to_string(), lang: String::new(), pages: Vec::new() };
    for (pi, page) in pages.iter_mut().enumerate() {
        place(&mut page.viewports, sw, sh);
        let mut cv = Cv::new(sh);
        // sheet border
        cv.rect(10.0 * PT, 10.0 * PT, (sw - 20.0) * PT, (sh - 20.0) * PT, &pens.ink(pens.border));
        // the title block first: the project (H1) and the page's title (H2) lead the page in reading order
        title_block(&mut cv, &pens, project, &page.title, date, o.first + pi + 1, total, sw, sh);
        // kept drawings first; every other viewport on its own thread (a few at a time), drawn in order
        for d in draw_viewports(m, &bm, &page.viewports, &pens, sh, o.cache, None) {
            cv.out.extend(d?.iter().cloned());
        }
        // paper line work, over the viewports and under the dimensions
        for l in &page.lines {
            let pen = Pen { w: l.w, rgb: l.rgb.unwrap_or(pens.ink), dash: l.dash.clone() };
            cv.push(Prim::Path(PathPrim { segs: l.segs.clone(), pen: Some(pen), fill: None, prov: None }));
        }
        // dimensions attached to the model, drawn over the viewports
        for d in &page.dims {
            super::dims::draw(&mut cv, &pens, d);
        }
        table(&mut cv, &pens, page);
        notes_box(&mut cv, &pens, notes, sw, sh);
        doc.pages.push(Page { w: sw, h: sh, bg: blueprint.then_some(BP_PAPER), title: format!("{project}: {}", page.title), items: cv.out });
    }
    Ok(doc)
}

/// Most viewports drawn at once (each on its own thread; a drawing can hold a lot of memory while it is made).
pub const VIEWPORT_THREADS: usize = 4;
/// The same on the background worker, which shares the machine with the person's work.
pub const BACKGROUND_THREADS: usize = 2;

/// The drawings of `vps` (placed on their page), in order: kept ones from `cache`, the rest drawn (and kept).
/// `background`: the cache's epoch when the model `m` was copied for the background worker (what it draws is kept
/// only if nothing changed since); its drawings are counted apart and it uses fewer threads.
pub fn draw_viewports(m: &Model, bm: &Model, vps: &[ViewportSpec], pens: &Pens, sh: f64, cache: Option<&super::cache::Drawings>, background: Option<u64>) -> Vec<Result<std::sync::Arc<Vec<Prim>>>> {
    use std::sync::Arc;
    let epoch = background.unwrap_or_else(|| cache.map_or(0, |c| c.epoch()));
    let threads = if background.is_some() { BACKGROUND_THREADS } else { VIEWPORT_THREADS };
    let keys: Vec<String> = if cache.is_some() { vps.iter().map(|v| super::cache::key(v, pens, sh)).collect() } else { Vec::new() };
    let mut out: Vec<Option<Result<Arc<Vec<Prim>>>>> = (0..vps.len()).map(|i| cache.and_then(|c| c.get(&keys[i])).map(Ok)).collect();
    let missing: Vec<usize> = (0..vps.len()).filter(|i| out[*i].is_none()).collect();
    for group in missing.chunks(threads) {
        let got: Vec<Result<Vec<Prim>>> = std::thread::scope(|s| {
            let hs: Vec<_> = group.iter().map(|i| s.spawn(|| viewport(m, bm, &vps[*i], pens, sh))).collect();
            hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("a viewport's drawing stopped unexpectedly")))).collect()
        });
        for (i, r) in group.iter().zip(got) {
            let r = r.map(Arc::new);
            if let (Some(c), Ok(items)) = (cache, &r) {
                c.put(keys[*i].clone(), super::cache::Volume::of(m, &vps[*i]), items.clone(), epoch, background.is_some());
            }
            out[*i] = Some(r);
        }
    }
    out.into_iter().map(|r| r.unwrap_or_else(|| Err(anyhow::anyhow!("a viewport was not drawn")))).collect()
}

/// A table (schedule) at the top left.
fn table(cv: &mut Cv, pens: &Pens, page: &PageSpec) {
    if page.table.is_empty() {
        return;
    }
    let size = 7.5;
    let w = |s: &str, bold: bool| super::font::width(s, size, bold);
    let cols = page.table.iter().map(|r| r.len()).max().unwrap_or(0);
    let widths: Vec<f64> = (0..cols).map(|i| page.table.iter().enumerate().map(|(ri, r)| r.get(i).map(|s| w(s, ri == 0 || i == 0)).unwrap_or(0.0)).fold(0.0, f64::max) + 16.0).collect();
    let (tx, ty, rh) = (20.0 * PT, 20.0, 6.5);
    let sh = cv.h_mm;
    let y = |mm: f64| (sh - mm) * PT;
    let total_w: f64 = widths.iter().sum();
    cv.role = Role::Caption;
    cv.text(tx, y(ty), &page.title.to_uppercase(), 11.0, true, pens.ink, 0.0);
    cv.role = Role::Paragraph;
    for (ri, row) in page.table.iter().enumerate() {
        let top = ty + 4.0 + ri as f64 * rh;
        cv.line((tx, y(top)), (tx + total_w, y(top)), &pens.ink(if ri <= 1 { pens.heavy } else { pens.thin }));
        let mut x = tx;
        for (ci, cell) in row.iter().enumerate() {
            cv.role = Role::Cell { row: ri as u16, col: ci as u16, header: ri == 0 || ci == 0 };
            cv.text(x + 6.0, y(top + rh - 2.0), cell, size, ri == 0 || ci == 0, pens.ink, 0.0);
            cv.role = Role::Paragraph;
            x += widths[ci];
        }
    }
    let bottom = ty + 4.0 + page.table.len() as f64 * rh;
    cv.line((tx, y(bottom)), (tx + total_w, y(bottom)), &pens.ink(pens.heavy));
    // column rules
    let mut x = tx;
    for w in std::iter::once(0.0).chain(widths.iter().copied()) {
        x += w;
        cv.line((x, y(ty + 4.0)), (x, y(bottom)), &pens.ink(pens.thin));
    }
}

/// Notes (areas, general notes) in a box beside the title block.
fn notes_box(cv: &mut Cv, pens: &Pens, notes: &[String], sw: f64, sh: f64) {
    if notes.is_empty() {
        return;
    }
    let y = |mm: f64| (sh - mm) * PT;
    let (tw, lh) = (70.0f64, 4.2);
    let nh = 6.0 + notes.len() as f64 * lh;
    let title_w = 110.0f64.min(sw * 0.4);
    let (nx, ny) = (sw - 10.0 - title_w - tw, sh - 10.0 - nh);
    cv.rect(nx * PT, y(ny + nh), tw * PT, nh * PT, &pens.ink(pens.heavy));
    for (i, line) in notes.iter().enumerate() {
        let bold = line.starts_with('#');
        let text = line.trim_start_matches('#').trim();
        cv.role = if bold { Role::Heading3 } else { Role::Paragraph };
        cv.text((nx + 4.0) * PT, y(ny + 5.5 + i as f64 * lh), text, 6.5, bold, pens.ink, 0.0);
        cv.role = Role::Paragraph;
    }
}

#[allow(clippy::too_many_arguments)]
fn title_block(cv: &mut Cv, pens: &Pens, project: &str, title: &str, date: &str, n: usize, total: usize, sw: f64, sh: f64) {
    let y = |mm: f64| (sh - mm) * PT;
    let (tw, th) = (110.0f64.min(sw * 0.4), 32.0);
    let (tx, ty) = (sw - 10.0 - tw, sh - 10.0 - th);
    cv.rect(tx * PT, y(ty + th), tw * PT, th * PT, &pens.ink(pens.heavy));
    cv.line((tx * PT, y(ty + 14.0)), ((tx + tw) * PT, y(ty + 14.0)), &pens.ink(pens.thin));
    cv.role = Role::Heading1;
    cv.text((tx + 4.0) * PT, y(ty + 9.0), project, 12.0, true, pens.ink, 0.0);
    cv.role = Role::Heading2;
    cv.text((tx + 4.0) * PT, y(ty + 21.0), title, 9.0, false, pens.ink, 0.0);
    cv.role = Role::Paragraph;
    cv.text((tx + 4.0) * PT, y(ty + 28.5), &format!("{date}   Sheet {n} of {total}   Drawn in SketchCraft"), 7.0, false, pens.ink, 0.0);
}

/// One viewport: its content clipped to its frame, the frame, caption and scale bar.
pub(crate) fn viewport(m: &Model, bm: &Model, v: &ViewportSpec, pens: &Pens, sh: f64) -> Result<Vec<Prim>> {
    let mut cv = Cv::new(sh);
    let y = |mm: f64| (sh - mm) * PT;
    let (label, denom) = if let Some(z) = v.cut {
        super::project::plan(&mut cv, m, bm, v, z, pens)?
    } else {
        super::project::view(&mut cv, m, bm, v, pens)?
    };
    cv.rect(v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT, &pens.ink(pens.thin));
    cv.role = Role::Caption;
    cv.text(v.x * PT, y(v.y + v.h + 5.0), &label, 8.0, true, pens.ink, 0.0);
    cv.role = Role::Paragraph;
    // what the viewport is, for the PDF's figure description: its caption, then the labels, dimensions and
    // tags drawn in it (a screen reader reads a figure's description in place of what's inside)
    for p in cv.out.iter_mut() {
        if let Prim::Group(g) = p {
            let mut said: Vec<String> = Vec::new();
            fn gather(items: &[Prim], out: &mut Vec<String>) {
                for p in items {
                    match p {
                        Prim::Text(t) if !out.contains(&t.text) => out.push(t.text.clone()),
                        Prim::Group(g) => gather(&g.children, out),
                        _ => {}
                    }
                }
            }
            gather(&g.children, &mut said);
            let mut alt = label.clone();
            if !said.is_empty() {
                alt.push_str(". Marked on it: ");
                alt.push_str(&said.join(", "));
            }
            if alt.len() > 1500 {
                let mut cut = 1500;
                while !alt.is_char_boundary(cut) {
                    cut -= 1;
                }
                alt.truncate(cut);
                alt.push_str("...");
            }
            g.alt = Some(alt);
            g.opacity = v.opacity.map_or(1.0, |o| o.clamp(0.0, 1.0));
        }
    }
    if let Some(d) = denom {
        if v.scale.is_some() {
            scale_bar(&mut cv, pens, v, d, matches!(m.units.as_str(), "ft" | "in"));
        }
    }
    Ok(cv.out)
}

/// A scale bar under a viewport: a round length no longer than 40 mm.
pub fn scale_bar(cv: &mut Cv, pens: &Pens, v: &ViewportSpec, d: f64, imperial: bool) {
    let sh = cv.h_mm;
    let y = |mm: f64| (sh - mm) * PT;
    // model meters per foot when imperial; lengths are round in the units shown
    let unit = if imperial { 0.3048 } else { 1.0 };
    let max_u = 40.0 * d / 1000.0 / unit;
    let steps = if imperial { vec![0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0] } else { vec![0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0] };
    // (a scale finer than the shortest step gets a rounded length, never a long decimal)
    let len_u = steps.iter().copied().rfind(|l| *l <= max_u).unwrap_or_else(|| (max_u * 100.0).floor().max(1.0) / 100.0);
    let len_mm = len_u * unit * 1000.0 / d;
    let (bx, by) = (v.x + v.w - len_mm, v.y + v.h + 3.5);
    let pen = pens.ink(pens.medium);
    cv.line((bx * PT, y(by)), ((bx + len_mm) * PT, y(by)), &pen);
    for k in 0..=4 {
        let tx = bx + len_mm * k as f64 / 4.0;
        cv.line((tx * PT, y(by - 1.0)), (tx * PT, y(by + 1.0)), &pen);
    }
    cv.text((bx + len_mm + 1.0) * PT, y(by + 1.0), &format!("{} {}", len_u, if imperial { "ft" } else { "m" }), 6.0, false, pens.ink, 0.0);
}
