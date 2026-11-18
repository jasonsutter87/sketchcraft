//! The engine's kept drawings (design §4): what a change drops, one saved sheet drawn as a page, the background
//! redraw of the pages looked at last, and `drafting_status`. The store itself is `draft::cache`.

use super::Engine;
use crate::draft::ir::{Page, Pens};
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::sync::Arc;

impl Engine {
    /// The model changed from `before` to what it is now: drop the kept drawings the change reached, and redraw the
    /// pages looked at last in the background.
    pub(super) fn drawings_changed(&mut self, before: &Model) {
        if self.drawings.is_empty() {
            return;
        }
        let d = crate::draft::dirty::changes(before, &self.model);
        if self.drawings.invalidate(&d) > 0 {
            let recent: Vec<Id> = self.drawings.recent().into_iter().filter(|s| self.model.sheets.contains_key(s)).collect();
            if !recent.is_empty() {
                self.warm_sheets(&recent);
            }
        }
    }

    /// How a saved sheet prints: its paper, project name, notes and blueprint look (from the sheet's own fields).
    fn sheet_look(&self, sheet: Id) -> Result<(String, String, Vec<String>, bool, f64, f64)> {
        let Some(s) = self.model.sheets.get(&sheet) else { bail!("no sheet {sheet} (sheets_info lists them)") };
        let known = crate::layout::sheet_mm(&s.paper.name).ok().filter(|(w, h)| (w - s.paper.w_mm).abs() < 0.5 && (h - s.paper.h_mm).abs() < 0.5);
        let Some((w, h)) = known else { bail!("sheet {sheet} is {} x {} mm, not a standard size the drawing knows", s.paper.w_mm, s.paper.h_mm) };
        let project = s.fields.get("project").cloned().unwrap_or_default();
        let notes = s.fields.get("notes").map(|n| n.lines().map(str::to_string).collect()).unwrap_or_default();
        let blueprint = s.fields.get("blueprint").is_some_and(|b| b == "true");
        Ok((s.paper.name.clone(), project, notes, blueprint, w, h))
    }

    /// One saved sheet drawn as a page, as the editor shows it (`sheet_ir`) and `sheet_render` pictures it: viewports
    /// from the kept drawings (drawn and kept when they aren't), numbered as in the whole set. The sheet counts as
    /// looked at: after a change, it (and the two looked at before it) is redrawn in the background.
    pub fn sheet_page(&self, sheet: Id) -> Result<Page> {
        let page = self.draw_sheet(sheet, true)?;
        self.drawings.viewed(sheet);
        Ok(page)
    }

    /// `sheet_page` with every viewport drawn afresh and nothing kept (what a kept page must equal).
    pub fn sheet_page_fresh(&self, sheet: Id) -> Result<Page> {
        self.draw_sheet(sheet, false)
    }

    /// Every saved viewport, sheet by sheet, and whether its drawing is kept now (false: it is drawn when next
    /// viewed or exported).
    pub fn kept_viewports(&self) -> Vec<(Id, bool)> {
        let m = &self.model;
        let mut out = Vec::new();
        for s in m.sheets.keys() {
            let Ok((_, _, _, blueprint, w, h)) = self.sheet_look(*s) else { continue };
            let mut spec = super::sheets::page_of(m, *s);
            crate::draft::sheet::place(&mut spec.viewports, w, h);
            let mut ids: Vec<(i32, Id)> = m.sheet_items.iter().filter_map(|(id, i)| match &**i {
                crate::drafting::SheetItem::Known(crate::drafting::ItemKind::Viewport(v)) if v.sheet == *s => Some((v.z, *id)),
                _ => None,
            }).collect();
            ids.sort();
            let pens = Pens::new(blueprint);
            for ((_, id), v) in ids.into_iter().zip(&spec.viewports) {
                out.push((id, self.drawings.contains(&crate::draft::cache::key(v, &pens, h))));
            }
        }
        out
    }

    fn draw_sheet(&self, sheet: Id, kept: bool) -> Result<Page> {
        let (paper, project, notes, blueprint, ..) = self.sheet_look(sheet)?;
        let m = &self.model;
        let mut order: Vec<(i32, Id)> = m.sheets.iter().map(|(id, s)| (s.order, *id)).collect();
        order.sort();
        let n = order.iter().position(|(_, id)| *id == sheet).unwrap_or(0);
        let spec = super::sheets::page_of(m, sheet);
        let opts = crate::draft::sheet::Opts { cache: kept.then_some(&self.drawings), first: n, total: Some(order.len()) };
        let mut doc = crate::draft::sheet::build_opts(m, &paper, &project, vec![spec], blueprint, &notes, &crate::layout::today(), false, opts)?;
        doc.pages.pop().ok_or_else(|| anyhow::anyhow!("sheet {sheet} has no page"))
    }

    /// Draw what isn't kept of these sheets' viewports on the background worker, from a copy of the model as it is
    /// now. Nothing waits for it; a change made meanwhile makes it throw its work away (`Drawings::put`).
    pub fn warm_sheets(&self, sheets: &[Id]) {
        let mut pages = Vec::new();
        let world: std::sync::Arc<crate::draft::cache::World> = Default::default();
        for s in sheets {
            let Ok((_, _, _, blueprint, w, h)) = self.sheet_look(*s) else { continue };
            let mut spec = super::sheets::page_of(&self.model, *s);
            crate::draft::sheet::place(&mut spec.viewports, w, h);
            let mut pens = Pens::new(blueprint);
            pens.hatches = Some(self.drawings.hatches.clone());
            // (the worker's own: its copy of the model is not the engine's)
            pens.world = Some(world.clone());
            let todo: Vec<_> = spec.viewports.into_iter().filter(|v| !self.drawings.contains(&crate::draft::cache::key(v, &pens, h))).collect();
            if !todo.is_empty() {
                pages.push((todo, pens, h));
            }
        }
        if pages.is_empty() {
            return;
        }
        let epoch = self.drawings.epoch();
        let snapshot = Arc::new(self.model.clone());
        let cache = self.drawings.clone();
        self.drawings.background(move || {
            let m = &*snapshot;
            let bm = crate::environment::without(m);
            for (vps, pens, h) in pages {
                if cache.epoch() != epoch {
                    return;
                }
                crate::draft::sheet::draw_viewports(m, &bm, &vps, &pens, h, Some(&cache), Some(epoch));
            }
        });
    }

    /// `drafting_status`: how the kept drawings stand (the editor polls `revision`; it moves when drawings are
    /// dropped by a change).
    pub(super) fn drafting_status(&self) -> Value {
        json!({
            "revision": self.drawings.epoch(),
            "drawing": self.drawings.busy(),
            "kept": self.drawings.len(),
            "bytes": self.drawings.bytes(),
            "stats": self.drawings.stats(),
        })
    }
}
