//! Drawing sheets saved in the model: create, change and delete sheets and what is on them, and read them back.
//! Arms of `Engine::run`. Sheets always live in the root model (never inside a component being edited), and every
//! command here is an ordinary model change: one undo step, or part of a batch.

use super::*;
use crate::drafting::*;
use crate::layout::{PageSpec, ViewportSpec};
use crate::policy::{MAX_PAGES_UI, MAX_PAGES_UNTRUSTED};
use super::command::{ExportPage, LineArgs, PaperArg, ScaleArg, ViewportArgs};
use crate::draft::linework;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The longest a sheet's name, number or label may be.
const MAX_NAME: usize = 200;

/// The most anchors one dimension (a chain) holds.
const MAX_DIM_ANCHORS: usize = 200;

impl Model {
    /// An anchor from what a command gave (see `AnchorArg`), checked against the live model: what it names must be
    /// there. `tol` is how far a `point` may be from geometry.
    fn anchor_from(&self, a: &super::command::AnchorArg, tol: f64) -> Result<Anchor> {
        let named = [a.point.is_some(), a.vertex.is_some(), a.between.is_some(), a.curve.is_some(), a.guide.is_some()].iter().filter(|x| **x).count();
        if named != 1 {
            bail!("an anchor is exactly one of point, vertex, between (with t), curve or guide");
        }
        if let Some(p) = a.point {
            if !a.path.is_empty() {
                bail!("a `point` anchor finds its own path; give `path` only with vertex, between, curve or guide");
            }
            return self.anchor_near(DVec3::from(p), tol).ok_or_else(|| anyhow::anyhow!("no vertex or edge within {tol} m of {p:?} to attach to"));
        }
        let at = if let Some(v) = a.vertex {
            AnchorAt::Vertex(v)
        } else if let Some([x, y]) = a.between {
            let t = a.t.unwrap_or(0.5);
            if !t.is_finite() || !(0.0..=1.0).contains(&t) {
                bail!("t is between 0 and 1");
            }
            AnchorAt::Between { a: x, b: y, t, extra: Extra::new() }
        } else if let Some(c) = a.curve {
            AnchorAt::CurveCenter(c)
        } else {
            AnchorAt::Guide(a.guide.unwrap_or(0))
        };
        let mut n = Anchor { path: a.path.clone(), at, last: [0.0; 3], extra: Extra::new() };
        match self.resolve_anchor(&n) {
            Some(w) => n.last = w.to_array(),
            None => bail!("there is nothing at that vertex, curve or guide (is the path right?)"),
        }
        Ok(n)
    }
}

impl Engine {
    /// After a command changed the model: anchors on sheets follow it (see `anchors`). Operations that ran inside a
    /// component being edited noted their remaps on its definition; they are lifted to the root here.
    pub(super) fn settle_anchors(&mut self) {
        if let Some(d) = self.ctx {
            if self.model.definitions.get(&d).is_some_and(|x| !x.model.remaps.is_empty()) {
                let inner = Arc::make_mut(&mut self.model.definitions.get_mut(&d).unwrap().model);
                for r in std::mem::take(&mut inner.remaps) {
                    if let crate::anchors::Remap::Vertices { from_path, to_path, map, .. } = r {
                        self.model.remaps.push(crate::anchors::Remap::Vertices { scope: Some(d), from_path, to_path, map });
                    }
                }
            }
        }
        self.model.settle_anchors();
    }
}

fn paper_of(a: &PaperArg) -> Result<Paper> {
    match a {
        PaperArg::Name(n) => Paper::named(n),
        PaperArg::Custom { w_mm, h_mm, name } => {
            if !(w_mm.is_finite() && h_mm.is_finite()) || *w_mm < 20.0 || *h_mm < 20.0 || *w_mm > 5000.0 || *h_mm > 5000.0 {
                bail!("a sheet is between 20 and 5000 mm on each side");
            }
            Ok(Paper { w_mm: *w_mm, h_mm: *h_mm, name: name.clone(), extra: Default::default() })
        }
    }
}

fn scale_n(a: &ScaleArg) -> Result<f64> {
    let n = match a {
        ScaleArg::N(n) => *n,
        ScaleArg::Text(t) => crate::layout::scale_of(t)?,
    };
    if !n.is_finite() || n <= 0.0 || n > 1e7 {
        bail!("scale is a positive number: 100 or \"1:100\"");
    }
    Ok(n)
}

fn short(what: &str, s: &str) -> Result<()> {
    if s.len() > MAX_NAME {
        bail!("{what} is too long ({} bytes, limit {MAX_NAME})", s.len());
    }
    Ok(())
}

/// The next 1-2-5 scale at or above `n` (a fitted viewport's scale is rounded up to one).
fn nice_scale(n: f64) -> f64 {
    let n = n.max(1.0);
    let p = 10f64.powf(n.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|k| k * p).find(|c| *c >= n * 0.9999).unwrap_or(10.0 * p)
}

/// "1:100" for 100 (and "1:12.5" for 12.5): the scale text the PDF export takes.
fn scale_text(n: f64) -> String {
    if n.fract() == 0.0 { format!("1:{}", n as u64) } else { format!("1:{n}") }
}

/// The model's bounds, worked out once per command (finding them walks every face and edge).
pub(super) type Bounds = Option<(DVec3, DVec3)>;

fn bounds_center(b: Bounds) -> DVec3 {
    b.map(|(lo, hi)| (lo + hi) / 2.0).unwrap_or(DVec3::ZERO)
}

/// A 3D view from a saved view's name or a standard one (iso, top, front, back, left, right).
fn view3d(m: &Model, b: Bounds, name: &str, parallel: bool) -> ViewKind {
    let cam = match m.views.iter().find(|v| v.name == name) {
        Some(v) => crate::render::CameraSpec { eye: v.eye, target: v.target, fov: v.fov },
        None => crate::render::camera_in(b, name),
    };
    ViewKind::View3d { eye: cam.eye, target: cam.target, fov: cam.fov, parallel, extra: Extra::new() }
}

/// What a viewport shows when nobody said: a floor plan cut at 1.2 m if the model is cut by that height (else halfway
/// up it), or a 3D view when the model has no height to cut (a flat site plan, nothing drawn yet). It always draws.
fn default_view(m: &Model, b: Bounds, parallel: bool) -> ViewKind {
    match b {
        Some((lo, hi)) if hi.z - lo.z >= 0.05 => {
            let cut = if lo.z + 0.01 < 1.2 && 1.2 < hi.z - 0.01 { 1.2 } else { (lo.z + hi.z) / 2.0 };
            ViewKind::Plan { cut_z: cut, floor_z: cut - 1.2192, depth: 0.0, extra: Extra::new() }
        }
        _ => view3d(m, b, "iso", parallel),
    }
}

fn validate(v: &Viewport) -> Result<()> {
    if v.rect_mm[2] <= 0.0 || v.rect_mm[3] <= 0.0 {
        bail!("a viewport needs a width and a height above 0 (rect_mm is x, y, width, height in millimetres)");
    }
    if !(0.0..=1.0).contains(&v.opacity) {
        bail!("opacity is between 0 and 1");
    }
    if v.crop.as_ref().is_some_and(|c| c.len() < 3) {
        bail!("a crop is an outline of at least 3 points");
    }
    short("a label", &v.label)?;
    Ok(())
}

impl Engine {
    pub(super) fn sheet_limit(&self) -> usize {
        if self.untrusted { MAX_PAGES_UNTRUSTED } else { MAX_PAGES_UI }
    }

    pub(super) fn made_by(&self) -> String {
        if self.untrusted { "agent".into() } else { String::new() }
    }

    /// A new viewport from what a command gave, with defaults for the rest (and the anchor fixed here, once).
    fn viewport_from(&self, a: &ViewportArgs, fallback_sheet: Option<Id>, b: Bounds) -> Result<Viewport> {
        let m = &self.model;
        let sheet = a.sheet.or(fallback_sheet).ok_or_else(|| anyhow::anyhow!("a viewport needs a sheet (the id `sheet_add` or `sheets` gave)"))?;
        let Some(s) = m.sheets.get(&sheet) else { bail!("no sheet {sheet}") };
        let scale = a.scale.as_ref().map(scale_n).transpose()?;
        let view = match (&a.view, &a.from_view) {
            (Some(v), _) => v.clone(),
            (None, Some(name)) => view3d(m, b, name, scale.is_some()),
            (None, None) => default_view(m, b, scale.is_some()),
        };
        let c = bounds_center(b);
        let anchor = a.anchor.unwrap_or(match &view {
            ViewKind::View3d { target, .. } => *target,
            ViewKind::Plan { cut_z, .. } => [c.x, c.y, *cut_z],
            _ => c.to_array(),
        });
        let (w, h) = (s.paper.w_mm, s.paper.h_mm);
        let scene = a.scene.clone().or_else(|| a.from_view.clone().filter(|n| m.views.iter().any(|v| &v.name == n)));
        let v = Viewport {
            sheet,
            z: a.z.unwrap_or(0),
            rect_mm: a.rect_mm.unwrap_or([15.0, 15.0, (w - 30.0).max(1.0), (h - 67.0).max(1.0)]),
            crop: a.crop.clone(),
            anchor,
            rotation: a.rotation.unwrap_or(0.0),
            scale: scale.unwrap_or(100.0),
            view,
            tags: a.tags.clone().unwrap_or_default(),
            opacity: a.opacity.unwrap_or(1.0),
            render: a.render.unwrap_or_default(),
            scene,
            label: a.label.clone().unwrap_or_default(),
            made_by: self.made_by(),
            ..Default::default()
        };
        validate(&v)?;
        Ok(v)
    }

    /// One page of a `sheets` description becomes a sheet with its viewports and table.
    pub(super) fn sheet_from_page(&mut self, page: &PageSpec, paper: &Paper, order: i32, fields: &BTreeMap<String, String>, b: Bounds) -> Result<Id> {
        short("a page title", &page.title)?;
        let id = self.model.new_id();
        self.model.sheets.insert(id, Sheet { name: page.title.clone(), paper: paper.clone(), order, fields: fields.clone(), ..Default::default() });
        let mut vps = page.viewports.clone();
        crate::layout::place(&mut vps, paper.w_mm, paper.h_mm);
        let who = self.made_by();
        for (i, spec) in vps.iter().enumerate() {
            let mut v = spec_to_viewport(&self.model, b, spec)?;
            v.sheet = id;
            v.z = i as i32;
            v.made_by = who.clone();
            validate(&v)?;
            let vid = self.model.new_id();
            self.model.sheet_items.insert(vid, Arc::new(SheetItem::Known(ItemKind::Viewport(v))));
        }
        if !page.table.is_empty() {
            let cols = page.table.iter().map(|r| r.len()).max().unwrap_or(0);
            let cols_mm: Vec<f64> = (0..cols).map(|c| page.table.iter().map(|r| r.get(c).map_or(0, |s| s.chars().count())).max().unwrap_or(0) as f64 * 1.6 + 6.0).collect();
            let t = Table { sheet: id, at_mm: [20.0, 24.0], cols_mm, rows: page.table.clone(), made_by: who, ..Default::default() };
            let tid = self.model.new_id();
            self.model.sheet_items.insert(tid, Arc::new(SheetItem::Known(ItemKind::Table(t))));
        }
        Ok(id)
    }

    pub(super) fn run_sheets(&mut self, cmd: Command) -> Result<Value> {
        // sheets are the root model's, whatever component is open for editing
        if !matches!(cmd, Command::SheetsInfo { .. } | Command::ExportSheets { .. }) && self.ctx.is_some() {
            bail!("close the component you're editing first: sheets belong to the whole model");
        }
        Ok(match cmd {
            Command::Sheets { path, sheet, project, pages, blueprint, notes } => {
                if path.is_some() {
                    bail!("\"sheets\" now creates saved sheets in the model and writes no file; use \"export_sheets\" (same arguments) to write the PDF");
                }
                let paper = Paper::named(&sheet)?;
                let mut pages = pages;
                if pages.is_empty() {
                    pages.push(PageSpec { title: "Views".into(), ..Default::default() });
                }
                let defaults = crate::layout::default_specs(&self.model);
                for p in &mut pages {
                    if p.viewports.is_empty() && p.table.is_empty() {
                        p.viewports = defaults.clone();
                    }
                }
                if self.model.sheets.len() + pages.len() > self.sheet_limit() {
                    bail!("that would make {} sheets; the limit is {}", self.model.sheets.len() + pages.len(), self.sheet_limit());
                }
                let mut fields = BTreeMap::new();
                if let Some(p) = project.as_ref().filter(|p| !p.is_empty()) {
                    short("the project name", p)?;
                    fields.insert("project".to_string(), p.clone());
                }
                if !notes.is_empty() {
                    fields.insert("notes".to_string(), notes.join("\n"));
                }
                if blueprint {
                    fields.insert("blueprint".to_string(), "true".to_string());
                }
                let first = self.model.sheets.values().map(|s| s.order).max().map_or(0, |o| o.saturating_add(1));
                let bounds = self.model.world_bounds();
                let mut ids = vec![];
                for (i, page) in pages.iter().enumerate() {
                    ids.push(self.sheet_from_page(page, &paper, first.saturating_add(i as i32), &fields, bounds)?);
                }
                json!({"sheets": ids, "sheet": ids.first()})
            }
            Command::SheetAdd { name, number, paper, order, fields, title_block } => {
                if self.model.sheets.len() >= self.sheet_limit() {
                    bail!("a model holds at most {} sheets", self.sheet_limit());
                }
                let paper = match &paper {
                    Some(p) => paper_of(p)?,
                    None => Paper::default(),
                };
                if let Some(t) = title_block.filter(|t| !self.model.title_blocks.contains_key(t)) {
                    bail!("no title block {t}");
                }
                let n = self.model.sheets.len() + 1;
                let name = name.unwrap_or_else(|| format!("Sheet {n}"));
                let number = number.unwrap_or_default();
                short("the name", &name)?;
                short("the number", &number)?;
                for (k, v) in &fields {
                    short("a field name", k)?;
                    short("a field", v)?;
                }
                let order = order.unwrap_or_else(|| self.model.sheets.values().map(|s| s.order).max().map_or(0, |o| o.saturating_add(1)));
                let id = self.model.new_id();
                self.model.sheets.insert(id, Sheet { name, number, paper, order, title_block, fields, ..Default::default() });
                json!({"sheet": id})
            }
            Command::SheetEdit { sheet, name, number, paper, order, fields, title_block } => {
                let paper = paper.as_ref().map(paper_of).transpose()?;
                if let Some(t) = title_block.filter(|t| *t != 0 && !self.model.title_blocks.contains_key(t)) {
                    bail!("no title block {t}");
                }
                let Some(s) = self.model.sheets.get_mut(&sheet) else { bail!("no sheet {sheet}") };
                if let Some(n) = name {
                    short("the name", &n)?;
                    s.name = n;
                }
                if let Some(n) = number {
                    short("the number", &n)?;
                    s.number = n;
                }
                if let Some(p) = paper {
                    s.paper = p;
                }
                if let Some(o) = order {
                    s.order = o;
                }
                if let Some(t) = title_block {
                    s.title_block = (t != 0).then_some(t);
                }
                for (k, v) in fields {
                    short("a field name", &k)?;
                    match v {
                        Some(v) => {
                            short("a field", &v)?;
                            s.fields.insert(k, v);
                        }
                        None => {
                            s.fields.remove(&k);
                        }
                    }
                }
                json!({"sheet": sheet})
            }
            Command::SheetDelete { sheet } => {
                if self.model.sheets.remove(&sheet).is_none() {
                    bail!("no sheet {sheet}");
                }
                let gone: Vec<Id> = self.model.sheet_items.iter().filter(|(_, i)| i.sheet() == Some(sheet)).map(|(k, _)| *k).collect();
                for k in &gone {
                    self.model.sheet_items.remove(k);
                }
                json!({"deleted": gone.len()})
            }
            Command::ViewportAdd { args, viewports } => {
                let fallback = args.sheet;
                let mut made = vec![];
                let list: Vec<&ViewportArgs> = if viewports.is_empty() { vec![&args] } else { viewports.iter().collect() };
                // check all of them before adding any (a bad one in the list adds nothing)
                let bounds = self.model.world_bounds();
                let built: Vec<Viewport> = list.iter().map(|a| self.viewport_from(a, fallback, bounds)).collect::<Result<_>>()?;
                for v in built {
                    let id = self.model.new_id();
                    self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Viewport(v))));
                    made.push(id);
                }
                json!({"viewport": made.first(), "viewports": made})
            }
            Command::ViewportEdit { id, args, clear_crop } => {
                let Some(item) = self.model.sheet_items.get(&id).cloned() else { bail!("no sheet item {id}") };
                let SheetItem::Known(ItemKind::Viewport(old)) = &*item else { bail!("sheet item {id} is a {}, not a viewport", item.kind_name()) };
                let mut v = old.clone();
                // the target sheet, if it is moving
                if let Some(s) = args.sheet {
                    if !self.model.sheets.contains_key(&s) {
                        bail!("no sheet {s}");
                    }
                    v.sheet = s;
                }
                let scale = args.scale.as_ref().map(scale_n).transpose()?;
                if let Some(r) = args.rect_mm {
                    v.rect_mm = r;
                }
                if let Some(s) = scale {
                    v.scale = s;
                    // a scale the person or agent set is the scale: the export no longer fits the picture, and a
                    // perspective 3D view becomes a parallel one (only a parallel view can be to scale)
                    drop_legacy(&mut v, "fit");
                    if let ViewKind::View3d { parallel, .. } = &mut v.view {
                        *parallel = true;
                    }
                }
                if let Some(a) = args.anchor {
                    v.anchor = a;
                }
                if let Some(r) = args.rotation {
                    v.rotation = r;
                }
                if let Some(c) = &args.crop {
                    v.crop = Some(c.clone());
                }
                if clear_crop {
                    v.crop = None;
                }
                let was_parallel = matches!(v.view, ViewKind::View3d { parallel: true, .. });
                if let Some(view) = &args.view {
                    v.view = view.clone();
                    v.extra.remove("legacy"); // every old-style option (fit, callouts, marks, levels, section mark, view name) described the old view
                } else if let Some(name) = &args.from_view {
                    v.view = view3d(&self.model, self.model.world_bounds(), name, scale.is_some() || was_parallel);
                    // it follows the saved view it names, and nothing if it names a standard one
                    v.scene = self.model.views.iter().any(|x| &x.name == name).then(|| name.clone());
                    v.extra.remove("legacy"); // every old-style option (fit, callouts, marks, levels, section mark, view name) described the old view
                }
                if let Some(z) = args.z {
                    v.z = z;
                }
                if let Some(o) = args.opacity {
                    v.opacity = o;
                }
                if let Some(r) = args.render {
                    v.render = r;
                }
                if let Some(s) = &args.scene {
                    v.scene = Some(s.clone());
                }
                if let Some(l) = &args.label {
                    v.label = l.clone();
                }
                if let Some(t) = &args.tags {
                    v.tags = t.clone();
                }
                validate(&v)?;
                self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Viewport(v))));
                json!({"viewport": id})
            }
            Command::SheetItemDelete { ids } => {
                if let Some(bad) = ids.iter().find(|i| !self.model.sheet_items.contains_key(i)) {
                    bail!("no sheet item {bad}");
                }
                for i in &ids {
                    self.model.sheet_items.remove(i);
                }
                json!({"deleted": ids.len()})
            }
            Command::SheetDim { sheet, anchors, mode, offset_mm, viewport, style, text_override, tol } => {
                if !self.model.sheets.contains_key(&sheet) {
                    bail!("no sheet {sheet}");
                }
                if anchors.len() < 2 || anchors.len() > MAX_DIM_ANCHORS {
                    bail!("a dimension has 2 to {MAX_DIM_ANCHORS} anchors (two make one, more make a chain)");
                }
                let tol = tol.unwrap_or(1e-3);
                if !(tol.is_finite() && tol > 0.0 && tol <= 1.0) {
                    bail!("tol is a distance in metres, above 0 and at most 1");
                }
                let offset = offset_mm.unwrap_or(8.0);
                if !offset.is_finite() || offset.abs() > 1000.0 {
                    bail!("offset_mm is between -1000 and 1000");
                }
                if let Some(v) = viewport {
                    let on_sheet = matches!(self.model.sheet_items.get(&v).map(|i| &**i), Some(SheetItem::Known(ItemKind::Viewport(x))) if x.sheet == sheet);
                    if !on_sheet {
                        bail!("{v} is not a viewport on sheet {sheet}");
                    }
                }
                if let Some(s) = &style {
                    short("a style", s)?;
                }
                if let Some(s) = &text_override {
                    short("the text", s)?;
                }
                let made: Vec<Anchor> = anchors.iter().enumerate().map(|(i, a)| self.model.anchor_from(a, tol).map_err(|e| anyhow::anyhow!("anchor {i}: {e}"))).collect::<Result<_>>()?;
                let d = Dimension { sheet, anchors: made, mode: mode.unwrap_or_default(), viewport, offset_mm: offset, style, text_override, made_by: self.made_by(), ..Default::default() };
                let measured = self.model.measure_dimension(&d);
                let values: Vec<Option<f64>> = measured.segments.iter().map(|s| s.value).collect();
                let id = self.model.new_id();
                self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
                json!({"dimension": id, "values_m": values})
            }
            Command::SheetReattach { item, sheet, anchor, to, all_exact, tol } => {
                if let Some(i) = item.filter(|i| !matches!(self.model.sheet_items.get(i).map(|x| &**x), Some(SheetItem::Known(ItemKind::Dimension(_))))) {
                    bail!("{i} is not a dimension");
                }
                if let Some(s) = sheet.filter(|s| !self.model.sheets.contains_key(s)) {
                    bail!("no sheet {s}");
                }
                match (to, all_exact) {
                    (Some(to), false) => {
                        let (Some(id), Some(idx)) = (item, anchor) else { bail!("to reattach one anchor give the dimension's `item` and the `anchor` index to attach") };
                        let new = self.model.anchor_from(&to, 1e-3)?;
                        let Some(SheetItem::Known(ItemKind::Dimension(d))) = self.model.sheet_items.get(&id).map(|i| (**i).clone()) else { bail!("{id} is not a dimension") };
                        let mut d = d;
                        let Some(slot) = d.anchors.get_mut(idx) else { bail!("dimension {id} has {} anchors; no anchor {idx}", d.anchors.len()) };
                        *slot = new;
                        self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
                        json!({"reattached": 1, "item": id, "anchor": idx})
                    }
                    (None, true) => {
                        let tol = tol.unwrap_or(crate::anchors::REATTACH_TOL);
                        if !(tol.is_finite() && tol > 0.0 && tol <= 0.01) {
                            bail!("tol is a distance in metres, above 0 and at most 0.01");
                        }
                        let ids: Vec<Id> = self.model.sheet_items.iter().filter(|(k, i)| matches!(&***i, SheetItem::Known(ItemKind::Dimension(d)) if item.is_none_or(|x| x == **k) && sheet.is_none_or(|s| s == d.sheet))).map(|(k, _)| *k).collect();
                        let (mut done, mut ambiguous, mut unmatched) = (vec![], vec![], vec![]);
                        for id in ids {
                            let Some(SheetItem::Known(ItemKind::Dimension(mut d))) = self.model.sheet_items.get(&id).map(|i| (**i).clone()) else { continue };
                            let mut touched = false;
                            for (idx, a) in d.anchors.iter_mut().enumerate() {
                                if self.model.resolve_anchor(a).is_some() {
                                    continue;
                                }
                                let mut near = self.model.anchors_at(DVec3::from(a.last), tol);
                                match near.len() {
                                    0 => unmatched.push(json!({"item": id, "anchor": idx})),
                                    1 => {
                                        *a = near.remove(0);
                                        touched = true;
                                        done.push(json!({"item": id, "anchor": idx}));
                                    }
                                    n => ambiguous.push(json!({"item": id, "anchor": idx, "vertices": n})),
                                }
                            }
                            if touched {
                                self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
                            }
                        }
                        if done.is_empty() {
                            bail!("nothing was reattached: {} broken anchors have no vertex where they were, {} have more than one (pick with `to`)", unmatched.len(), ambiguous.len());
                        }
                        json!({"reattached": done.len(), "anchors": done, "ambiguous": ambiguous, "unmatched": unmatched})
                    }
                    _ => bail!("give `to` (with `item` and `anchor`) to attach one anchor, or `all_exact: true` to attach every broken anchor that has exactly one vertex where it was"),
                }
            }
            Command::SheetLine { args, lines } => {
                let list: Vec<&LineArgs> = if lines.is_empty() { vec![&args] } else { lines.iter().collect() };
                if list.len() > MAX_LINES_PER_COMMAND {
                    bail!("one command adds at most {MAX_LINES_PER_COMMAND} lines");
                }
                // check all of them before changing anything
                let built: Vec<(Option<Id>, ItemKind)> = list.iter().map(|a| self.line_from(a, args.sheet)).collect::<Result<_>>()?;
                let mut made = vec![];
                for (id, kind) in built {
                    let id = match id {
                        Some(id) => id,
                        None => self.model.new_id(),
                    };
                    self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(kind)));
                    made.push(id);
                }
                json!({"item": made.first(), "items": made})
            }
            Command::SheetTrim { item, at_mm, pick_tol_mm, use_model } => {
                let (kind, chain, g, sheet) = self.pick_line_work(item, at_mm, pick_tol_mm)?;
                let b = self.boundaries(sheet, item, use_model.unwrap_or(true));
                let hits = chain.crossings(&b.curves);
                let t = linework::trim(&chain, g, &hits)?;
                let mut pieces: Vec<ItemKind> = Vec::new();
                for run in &t.kept {
                    match linework::item_of(&kind, run) {
                        Some(k) => pieces.push(k),
                        None => bail!("this line work can't be trimmed (it has a piece the item kinds can't hold)"),
                    }
                }
                if pieces.is_empty() {
                    bail!("trimming there would leave nothing of the item: delete it with sheet_item_delete instead");
                }
                let mut ids: Vec<Id> = Vec::new();
                let mut created: Vec<Id> = Vec::new();
                let mut it = pieces.into_iter();
                if let Some(first) = it.next() {
                    self.model.sheet_items.insert(item, Arc::new(SheetItem::Known(first)));
                    ids.push(item);
                }
                for k in it {
                    let id = self.model.new_id();
                    self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(k)));
                    ids.push(id);
                    created.push(id);
                }
                json!({"item": item, "kept": ids, "created": created, "cut": {"from": t.cut[0], "to": t.cut[1]}, "boundaries": b.json()})
            }
            Command::SheetExtend { item, at_mm, pick_tol_mm, use_model } => {
                let (kind, chain, _, sheet) = self.pick_line_work(item, at_mm, pick_tol_mm)?;
                let b = self.boundaries(sheet, item, use_model.unwrap_or(true));
                let (w, h) = self.model.sheets.get(&sheet).map_or((0.0, 0.0), |s| (s.paper.w_mm, s.paper.h_mm));
                let (grown, end) = linework::extend(&chain, at_mm, &b.curves, 2.0 * w.hypot(h).max(1.0))?;
                let Some(k) = linework::item_of(&kind, &grown.curves) else { bail!("this line work can't be extended") };
                self.model.sheet_items.insert(item, Arc::new(SheetItem::Known(k)));
                json!({"item": item, "end": if end { "end" } else { "start" }, "to": if end { grown.end() } else { grown.start() }, "boundaries": b.json()})
            }
            Command::SheetOffset { item, distance_mm, at_mm, side } => {
                let Some(old) = self.model.sheet_items.get(&item).cloned() else { bail!("no sheet item {item}") };
                let SheetItem::Known(kind @ (ItemKind::Line(_) | ItemKind::Polyline(_) | ItemKind::Arc(_) | ItemKind::Rect(_))) = &*old else { bail!("sheet item {item} is a {}, not paper line work", old.kind_name()) };
                let chain = linework::Chain::of(kind)?;
                let side = match (side.as_deref(), at_mm) {
                    (Some(_), Some(_)) => bail!("give `at_mm` or `side`, not both"),
                    (None, None) => bail!("say which side the copy goes to: `at_mm` (a point on that side) or `side` (left, right, inside or outside)"),
                    (None, Some(p)) => {
                        if !p.iter().all(|x| x.is_finite() && x.abs() <= linework::MAX_COORD) {
                            bail!("at_mm is a point on the sheet, in millimetres");
                        }
                        linework::Side::Toward(p)
                    }
                    (Some(s), None) => match s.to_ascii_lowercase().as_str() {
                        "left" => linework::Side::Left,
                        "right" => linework::Side::Right,
                        "inside" => linework::Side::Inside,
                        "outside" => linework::Side::Outside,
                        other => bail!("side is left, right, inside or outside (got {other:?})"),
                    },
                };
                let copy = linework::offset(&chain, distance_mm, &side)?;
                let made = match kind {
                    ItemKind::Rect(s) => {
                        let Some(c) = linework::rect_of(&copy).filter(|_| linework::is_axis_rect(&copy)) else { bail!("the rectangle can't be offset by that much") };
                        ItemKind::Rect(Shape { pts_mm: c.to_vec(), made_by: self.made_by(), ..s.clone() })
                    }
                    other => {
                        let Some(k) = linework::item_of(other, &copy.curves) else { bail!("this line work can't be offset") };
                        k
                    }
                };
                let made = match made {
                    ItemKind::Line(s) => ItemKind::Line(Shape { made_by: self.made_by(), ..s }),
                    ItemKind::Polyline(s) => ItemKind::Polyline(Shape { made_by: self.made_by(), ..s }),
                    ItemKind::Arc(s) => ItemKind::Arc(Shape { made_by: self.made_by(), ..s }),
                    other => other,
                };
                let id = self.model.new_id();
                self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(made)));
                json!({"item": id, "from": item})
            }
            Command::SheetsInfo { sheet } => {
                let m = &self.model;
                if let Some(s) = sheet.filter(|s| !m.sheets.contains_key(s)) {
                    bail!("no sheet {s}");
                }
                let mut order: Vec<(&Id, &Sheet)> = m.sheets.iter().filter(|(id, _)| sheet.is_none_or(|s| s == **id)).collect();
                order.sort_by_key(|(id, s)| (s.order, **id));
                let units = if m.units.is_empty() { "m" } else { m.units.as_str() };
                let item_json = |id: &Id, i: &SheetItem| {
                    let mut v = serde_json::to_value(i).unwrap_or(Value::Null);
                    if let Some(o) = v.as_object_mut() {
                        o.insert("id".into(), json!(id));
                        // a dimension says what it measures now, and which of its anchors are broken (with where they were)
                        if let SheetItem::Known(ItemKind::Dimension(d)) = i {
                            let me = m.measure_dimension(d);
                            let spans: Vec<Value> = me.segments.iter().map(|s| json!({"from": s.from, "to": s.to, "value_m": s.value, "text": s.value.map_or("?".to_string(), |v| crate::units::format(v, units))})).collect();
                            let broken: Vec<Value> = m.broken_anchors(d).into_iter().map(|a| json!({"anchor": a, "last": d.anchors[a].last, "suggestions": m.anchors_at(DVec3::from(d.anchors[a].last), crate::anchors::REATTACH_TOL).into_iter().take(5).collect::<Vec<_>>()})).collect();
                            o.insert("measured".into(), json!({"spans": spans, "broken": broken, "attached": broken.is_empty()}));
                        }
                    }
                    v
                };
                let sheets: Vec<Value> = order
                    .iter()
                    .map(|(id, s)| {
                        let items: Vec<Value> = m.sheet_items.iter().filter(|(_, i)| i.sheet() == Some(**id)).map(|(k, i)| item_json(k, i)).collect();
                        json!({"id": id, "name": s.name, "number": s.number, "paper": {"w_mm": s.paper.w_mm, "h_mm": s.paper.h_mm, "name": s.paper.name}, "order": s.order, "title_block": s.title_block, "fields": s.fields, "issued": s.issued.len(), "items": items})
                    })
                    .collect();
                let unsupported: Vec<Value> = m.sheet_items.iter().filter(|(_, i)| i.is_raw()).map(|(k, i)| json!({"id": k, "kind": i.kind_name(), "sheet": i.sheet()})).collect();
                let orphans: Vec<&Id> = m.sheet_items.iter().filter(|(_, i)| i.sheet().is_none_or(|s| !m.sheets.contains_key(&s))).map(|(k, _)| k).collect();
                json!({"sheets": sheets, "items": m.sheet_items.len(), "unsupported": unsupported, "orphans": orphans})
            }
            Command::ExportSheets { path, format, sheet, project, pages, blueprint, notes, page } => {
                if let Some(f) = format.as_deref().filter(|f| !matches!(f.to_ascii_lowercase().as_str(), "pdf" | "svg" | "dxf")) {
                    bail!("export_sheets writes pdf, svg or dxf (got {f:?})");
                }
                let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                if !matches!(ext.as_str(), "pdf" | "svg" | "dxf") {
                    bail!("export_sheets writes a .pdf (every page), or one page as .svg or .dxf");
                }
                if format.as_deref().is_some_and(|f| !f.eq_ignore_ascii_case(&ext)) {
                    bail!("format {:?} doesn't match the .{ext} path", format.unwrap_or_default());
                }
                let specs: Vec<&PageSpec> = pages.iter().filter_map(|p| if let ExportPage::Spec(s) = p { Some(s) } else { None }).collect();
                let saved: Vec<Id> = pages.iter().filter_map(|p| if let ExportPage::Saved(i) = p { Some(*i) } else { None }).collect();
                if !specs.is_empty() && !saved.is_empty() {
                    bail!("pages is either sheet ids (saved sheets) or page descriptions, not both");
                }
                let m = &self.model;
                let stem = || std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into());
                if !specs.is_empty() || (m.sheets.is_empty() && saved.is_empty()) {
                    // the one-shot set: nothing saved
                    let paper = sheet.unwrap_or_else(|| "a3".into());
                    let (n, warnings) = crate::layout::write_sheets_notes(m, &path, &paper, &project.unwrap_or_else(stem), specs.into_iter().cloned().collect(), blueprint.unwrap_or(false), &notes.unwrap_or_default(), true, page)?;
                    return Ok(json!({"path": path, "pages": n, "warnings": warnings}));
                }
                // from the saved sheets, in their order (or the ids given, in the order given)
                let mut chosen: Vec<Id> = if saved.is_empty() {
                    let mut all: Vec<(&Id, &Sheet)> = m.sheets.iter().collect();
                    all.sort_by_key(|(id, s)| (s.order, **id));
                    all.into_iter().map(|(id, _)| *id).collect()
                } else {
                    saved
                };
                if let Some(bad) = chosen.iter().find(|i| !m.sheets.contains_key(i)) {
                    bail!("no sheet {bad}");
                }
                chosen.dedup();
                let first = &m.sheets[&chosen[0]];
                // the writer prints every page on one standard paper: refuse a set it would print wrongly, naming the sheets
                let paper = match sheet {
                    Some(p) => p,
                    None => {
                        let same = |a: &Paper, b: &Paper| (a.w_mm - b.w_mm).abs() < 0.5 && (a.h_mm - b.h_mm).abs() < 0.5;
                        let known = crate::layout::sheet_mm(&first.paper.name).ok().filter(|(w, h)| (w - first.paper.w_mm).abs() < 0.5 && (h - first.paper.h_mm).abs() < 0.5);
                        if known.is_none() {
                            bail!("sheet {} is {} x {} mm ({}), not one of the standard sizes the writer knows (a4 to a0, letter, tabloid, arch a to e, ansi c to e); give `sheet` to print it on one of those", chosen[0], first.paper.w_mm, first.paper.h_mm, if first.paper.name.is_empty() { "custom" } else { &first.paper.name });
                        }
                        if let Some(odd) = chosen.iter().find(|id| !same(&m.sheets[*id].paper, &first.paper)) {
                            let o = &m.sheets[odd];
                            bail!("sheets {} and {} are different papers ({} x {} and {} x {} mm) and one PDF prints one size: export them separately with pages: [sheet ids], or give `sheet` to force one", chosen[0], odd, first.paper.w_mm, first.paper.h_mm, o.paper.w_mm, o.paper.h_mm);
                        }
                        first.paper.name.clone()
                    }
                };
                let project = project.or_else(|| first.fields.get("project").cloned()).unwrap_or_else(stem);
                let notes = notes.unwrap_or_else(|| first.fields.get("notes").map(|n| n.lines().map(str::to_string).collect()).unwrap_or_default());
                let blueprint = blueprint.unwrap_or_else(|| first.fields.get("blueprint").is_some_and(|b| b == "true"));
                let pages: Vec<PageSpec> = chosen.iter().map(|id| page_of(m, *id)).collect();
                // saved sheets are drawn as they are: an empty one stays empty
                let (n, warnings) = crate::layout::write_sheets_notes(m, &path, &paper, &project, pages, blueprint, &notes, false, page)?;
                json!({"path": path, "pages": n, "paper": paper, "warnings": warnings})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}

/// Forget one of the old-style options kept under `extra["legacy"]` (an edit has made it untrue).
fn drop_legacy(v: &mut Viewport, key: &str) {
    if let Some(Value::Object(l)) = v.extra.get_mut("legacy") {
        l.remove(key);
    }
}

/// A saved sheet as the page description the PDF writer takes: its viewports in stacking order, and its first table.
pub(super) fn page_of(m: &Model, sheet: Id) -> PageSpec {
    let s = &m.sheets[&sheet];
    let mut vps: Vec<(i32, Id, &Viewport)> = m.sheet_items.iter().filter_map(|(id, i)| if let SheetItem::Known(ItemKind::Viewport(v)) = &**i { (v.sheet == sheet).then_some((v.z, *id, v)) } else { None }).collect();
    vps.sort_by_key(|(z, id, _)| (*z, *id));
    let table = m.sheet_items.values().find_map(|i| if let SheetItem::Known(ItemKind::Table(t)) = &**i { (t.sheet == sheet).then(|| t.rows.clone()) } else { None }).unwrap_or_default();
    // dimensions are worked out against the model as it is now: a moved wall has moved them
    let mut dims: Vec<(i32, Id, &Dimension)> = m.sheet_items.iter().filter_map(|(id, i)| if let SheetItem::Known(ItemKind::Dimension(d)) = &**i { (d.sheet == sheet).then_some((d.z, *id, d)) } else { None }).collect();
    dims.sort_by_key(|(z, id, _)| (*z, *id));
    let dims = dims.iter().filter_map(|(_, _, d)| crate::draft::dims::prepare(m, d)).collect();
    let mut shapes: Vec<(i32, Id, &ItemKind)> = m.sheet_items.iter().filter_map(|(id, i)| if let SheetItem::Known(k @ (ItemKind::Line(_) | ItemKind::Polyline(_) | ItemKind::Arc(_) | ItemKind::Rect(_))) = &**i { (k.sheet() == sheet).then(|| (shape_z(k), *id, k)) } else { None }).collect();
    shapes.sort_by_key(|(z, id, _)| (*z, *id));
    let lines = shapes.iter().filter_map(|(_, _, k)| crate::draft::linework::draw(k)).collect();
    PageSpec { title: s.name.clone(), viewports: vps.iter().map(|(_, _, v)| viewport_to_spec(v)).collect(), table, dims, lines }
}

/// The page description of an old-style viewport becomes a saved viewport. What the saved form has no field for
/// (callouts, plan marks, datum levels, the section mark, "fit the picture") is kept under `extra["legacy"]`.
fn spec_to_viewport(m: &Model, b: Bounds, spec: &ViewportSpec) -> Result<Viewport> {
    let scale = spec.scale.as_deref().map(crate::layout::scale_of).transpose()?;
    let c = bounds_center(b);
    let standard_dir = match spec.view.as_str() {
        "front" => Some([0.0, -1.0, 0.0]),
        "back" => Some([0.0, 1.0, 0.0]),
        "left" => Some([-1.0, 0.0, 0.0]),
        "right" => Some([1.0, 0.0, 0.0]),
        _ => None,
    };
    let (view, anchor) = if let Some(z) = spec.cut {
        (ViewKind::Plan { cut_z: z, floor_z: spec.floor.unwrap_or(z - 1.2192), depth: 0.0, extra: Extra::new() }, [c.x, c.y, z])
    } else if let Some(s) = spec.section {
        (ViewKind::Section { origin: [s[0], s[1], s[2]], normal: [s[3], s[4], 0.0], depth: 0.0, extra: Extra::new() }, c.to_array())
    } else if let (Some(dir), true, true) = (standard_dir, spec.callouts, scale.is_some()) {
        if m.views.iter().any(|v| v.name == spec.view) {
            let v = view3d(m, b, &spec.view, true);
            let t = if let ViewKind::View3d { target, .. } = &v { *target } else { c.to_array() };
            (v, t)
        } else {
            (ViewKind::Elevation { dir, depth: 0.0, extra: Extra::new() }, c.to_array())
        }
    } else {
        let v = view3d(m, b, &spec.view, scale.is_some());
        let t = if let ViewKind::View3d { target, .. } = &v { *target } else { c.to_array() };
        (v, t)
    };
    let fit = scale.is_none();
    let scale = scale.unwrap_or_else(|| {
        // not given: the model fitted to the picture once, at a round scale (saved, never re-fitted afterwards)
        let extent = b.map_or(10.0, |(lo, hi)| (hi - lo).length().max(1.0));
        nice_scale(extent * 1000.0 / (0.9 * spec.w.min(spec.h).max(1.0)))
    });
    let scene = m.views.iter().find(|v| v.name == spec.view).map(|v| v.name.clone());
    let mut extra = Extra::new();
    let mut legacy = serde_json::Map::new();
    legacy.insert("view".into(), json!(spec.view));
    if spec.callouts {
        legacy.insert("callouts".into(), json!(true));
    }
    if fit {
        legacy.insert("fit".into(), json!(true));
    }
    if !spec.marks.is_empty() {
        legacy.insert("marks".into(), json!(spec.marks));
    }
    if !spec.levels.is_empty() {
        legacy.insert("levels".into(), json!(spec.levels));
    }
    if let Some(sm) = &spec.section_mark {
        legacy.insert("section_mark".into(), json!({"origin": sm.origin, "look": sm.look, "label": sm.label}));
    }
    extra.insert("legacy".into(), Value::Object(legacy));
    Ok(Viewport { rect_mm: [spec.x, spec.y, spec.w, spec.h], anchor, scale, view, scene, label: spec.label.clone().unwrap_or_else(|| spec.view.clone()), extra, ..Default::default() })
}

/// A saved viewport as the old-style description the PDF writer draws from (the reverse of `spec_to_viewport`).
pub(super) fn viewport_to_spec(v: &Viewport) -> ViewportSpec {
    let legacy = v.extra.get("legacy").cloned().unwrap_or(Value::Null);
    let l = |k: &str| legacy.get(k).cloned().unwrap_or(Value::Null);
    // a perspective view has no scale to draw at (the PDF takes a scale to mean parallel and to scale): fitted
    let perspective = matches!(v.view, ViewKind::View3d { parallel: false, .. });
    let fit = l("fit") == json!(true);
    // a scaled drawing stays pinned to its saved anchor (as its dimensions are, through `anchors::viewport_proj`);
    // a fitted or perspective one is fitted to what it shows each time
    let mut s = ViewportSpec { anchor: (!fit && !perspective).then_some(v.anchor), x: v.rect_mm[0], y: v.rect_mm[1], w: v.rect_mm[2], h: v.rect_mm[3], label: (!v.label.is_empty()).then(|| v.label.clone()), scale: (!fit && !perspective).then(|| scale_text(v.scale)), callouts: l("callouts") == json!(true), opacity: (v.opacity < 1.0).then_some(v.opacity), ..Default::default() };
    // `dimension_auto` put attached dimensions where the worked-out ones were
    s.no_auto_dims = l("auto_dims") == json!(false);
    s.marks = serde_json::from_value(l("marks")).unwrap_or_default();
    s.levels = serde_json::from_value(l("levels")).unwrap_or_default();
    s.section_mark = serde_json::from_value::<Value>(l("section_mark")).ok().filter(|v| v.is_object()).and_then(|sm| {
        Some(crate::layout::SectionMark { origin: serde_json::from_value(sm.get("origin")?.clone()).ok()?, look: serde_json::from_value(sm.get("look")?.clone()).ok()?, label: sm.get("label")?.as_str()?.to_string() })
    });
    let named = legacy.get("view").and_then(Value::as_str).map(str::to_string);
    match &v.view {
        ViewKind::Plan { cut_z, floor_z, .. } => {
            s.cut = Some(*cut_z);
            s.floor = Some(*floor_z);
            s.view = named.unwrap_or_else(|| "top".into());
        }
        ViewKind::Elevation { dir, .. } => {
            s.view = match (dir[0].abs() > dir[1].abs(), dir[0] > 0.0, dir[1] > 0.0) {
                (true, true, _) => "right",
                (true, false, _) => "left",
                (false, _, true) => "back",
                (false, _, false) => "front",
            }
            .into();
            s.callouts = true;
        }
        ViewKind::Section { origin, normal, .. } => {
            s.section = Some([origin[0], origin[1], origin[2], normal[0], normal[1], normal[2]]);
            s.view = named.unwrap_or_else(|| "front".into());
        }
        ViewKind::View3d { eye, target, fov, .. } => {
            s.camera = Some(crate::render::CameraSpec { eye: *eye, target: *target, fov: *fov });
            s.view = v.scene.clone().or(named).unwrap_or_else(|| "iso".into());
        }
    }
    s
}

/// The most line work items one `sheet_line` command adds.
const MAX_LINES_PER_COMMAND: usize = 5_000;
/// The most points one polyline holds.
const MAX_LINE_POINTS: usize = 10_000;
/// The most boundary pieces a trim or extend gathers from the viewports of a sheet.
const MAX_BOUNDARY_PIECES: usize = 400_000;

/// What a trim or extend found to cut to.
struct Boundaries {
    curves: Vec<linework::Curve>,
    paper: usize,
    model: usize,
    frames: usize,
    /// a viewport whose drawing couldn't be made (its model lines are not boundaries), or a cap that was hit
    notes: Vec<String>,
}

impl Boundaries {
    fn json(&self) -> Value {
        json!({"paper": self.paper, "model_lines": self.model, "viewport_frames": self.frames, "notes": self.notes})
    }
}

fn shape_z(k: &ItemKind) -> i32 {
    match k {
        ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s) => s.z,
        _ => 0,
    }
}

impl Engine {
    /// A line work item from what a command gave: for a new one, its sheet and points; for a change (`id`), the old
    /// one with what was given put over it. Everything is checked here, so a bad one changes nothing.
    fn line_from(&self, a: &LineArgs, fallback_sheet: Option<Id>) -> Result<(Option<Id>, ItemKind)> {
        let old = match a.id {
            Some(id) => match self.model.sheet_items.get(&id).map(|i| &**i) {
                Some(SheetItem::Known(k @ (ItemKind::Line(_) | ItemKind::Polyline(_) | ItemKind::Arc(_) | ItemKind::Rect(_)))) => Some(k.clone()),
                Some(other) => bail!("sheet item {id} is a {}, not paper line work", other.kind_name()),
                None => bail!("no sheet item {id}"),
            },
            None => None,
        };
        let name = |k: &ItemKind| k.name();
        let kind_name = match (&old, a.kind.as_deref()) {
            (Some(o), Some(k)) if !k.eq_ignore_ascii_case(name(o)) => bail!("an item's kind stays as it is ({} stays a {}); delete it and add a {k} instead", a.id.unwrap_or(0), name(o)),
            (Some(o), _) => name(o).to_string(),
            (None, Some(k)) => k.to_ascii_lowercase(),
            (None, None) => match a.pts_mm.as_ref().map(Vec::len) {
                Some(2) => "line".into(),
                Some(_) => "polyline".into(),
                None => bail!("give `pts_mm` (and `sheet`) to add line work"),
            },
        };
        let mut shape = match &old {
            Some(ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s)) => s.clone(),
            _ => {
                let Some(sheet) = a.sheet.or(fallback_sheet) else { bail!("give the `sheet` to add line work to") };
                if !self.model.sheets.contains_key(&sheet) {
                    bail!("no sheet {sheet}");
                }
                if a.pts_mm.is_none() {
                    bail!("give `pts_mm` to add line work");
                }
                Shape { sheet, made_by: self.made_by(), ..Default::default() }
            }
        };
        if let Some(p) = &a.pts_mm {
            if p.len() > MAX_LINE_POINTS {
                bail!("a polyline holds at most {MAX_LINE_POINTS} points");
            }
            if let Some(bad) = p.iter().position(|q| !q.iter().all(|x| x.is_finite() && x.abs() <= linework::MAX_COORD)) {
                bail!("point {bad} is not a paper position (millimetres from the sheet's top left, within {} of it)", linework::MAX_COORD);
            }
            shape.pts_mm = p.clone();
        }
        if let Some(w) = a.w_mm {
            if !(w.is_finite() && (WEIGHT_RANGE.0..=WEIGHT_RANGE.1).contains(&w)) {
                bail!("w_mm is a pen width between {} and {} mm", WEIGHT_RANGE.0, WEIGHT_RANGE.1);
            }
            shape.pen.w_mm = w;
        }
        if let Some(c) = &a.color {
            if crate::draft::ir::parse_hex(c).is_none() {
                bail!("color is \"#rrggbb\"");
            }
            shape.pen.color = c.clone();
        }
        if let Some(d) = &a.dash_mm {
            if d.len() > 16 || d.iter().any(|x| !(x.is_finite() && *x > 0.0 && *x <= 100.0)) {
                bail!("dash_mm is up to 16 lengths (on, off, ...), each above 0 and at most 100 mm; [] for a solid line");
            }
            shape.pen.dash_mm = d.clone();
        }
        if let Some(z) = a.z {
            shape.z = z;
        }
        let kind = match kind_name.as_str() {
            "line" => ItemKind::Line(shape),
            "polyline" => ItemKind::Polyline(shape),
            "arc" => ItemKind::Arc(shape),
            "rect" => ItemKind::Rect(shape),
            other => bail!("kind is line, polyline, arc or rect (got {other:?})"),
        };
        // the shape has to hold together: the right number of points, a length, three points on a circle
        linework::Chain::of(&kind)?;
        Ok((a.id, kind))
    }

    /// The line work a trim or extend is about: the item, its chain, the place nearest the pick, and its sheet.
    fn pick_line_work(&self, item: Id, at: [f64; 2], tol: Option<f64>) -> Result<(ItemKind, linework::Chain, f64, Id)> {
        let Some(old) = self.model.sheet_items.get(&item) else { bail!("no sheet item {item}") };
        let SheetItem::Known(kind @ (ItemKind::Line(_) | ItemKind::Polyline(_) | ItemKind::Arc(_) | ItemKind::Rect(_))) = &**old else {
            bail!("sheet item {item} is a {}, not paper line work: only lines, polylines, arcs and rectangles on a sheet are trimmed, extended and offset (what a viewport draws of the model never is)", old.kind_name())
        };
        if !at.iter().all(|x| x.is_finite() && x.abs() <= linework::MAX_COORD) {
            bail!("at_mm is a point on the sheet, in millimetres");
        }
        let tol = tol.unwrap_or(2.0);
        if !(tol.is_finite() && tol > 0.0 && tol <= 50.0) {
            bail!("pick_tol_mm is above 0 and at most 50");
        }
        let chain = linework::Chain::of(kind)?;
        let (g, d) = chain.nearest(at);
        if d > tol {
            bail!("at_mm is {d:.2} mm from item {item}: pick a point on it (within {tol} mm)");
        }
        Ok((kind.clone(), chain, g, kind.sheet()))
    }

    /// What a trim or extend of `item` cuts to: the other line work on its sheet, the edges of the sheet's viewports,
    /// and (when `use_model`) the lines each viewport draws of the model, cut to its frame.
    fn boundaries(&self, sheet: Id, item: Id, use_model: bool) -> Boundaries {
        let m = &self.model;
        let mut b = Boundaries { curves: Vec::new(), paper: 0, model: 0, frames: 0, notes: Vec::new() };
        let mut vps: Vec<(i32, Id, &Viewport)> = Vec::new();
        for (id, i) in &m.sheet_items {
            let SheetItem::Known(k) = &**i else { continue };
            if k.sheet() != sheet {
                continue;
            }
            match k {
                ItemKind::Line(_) | ItemKind::Polyline(_) | ItemKind::Arc(_) | ItemKind::Rect(_) if *id != item => {
                    if let Ok(c) = linework::Chain::of(k) {
                        b.paper += c.curves.len();
                        b.curves.extend(c.curves);
                    }
                }
                ItemKind::Viewport(v) => vps.push((v.z, *id, v)),
                _ => {}
            }
        }
        vps.sort_by_key(|(z, id, _)| (*z, *id));
        let sh = m.sheets.get(&sheet).map_or(297.0, |s| s.paper.h_mm);
        let bm = crate::environment::without(m);
        let pens = crate::draft::ir::Pens::new(false);
        for (_, id, v) in vps {
            if v.rect_mm[2] <= 0.0 || v.rect_mm[3] <= 0.0 {
                continue;
            }
            let fr = linework::frame(v.rect_mm);
            b.frames += fr.len();
            b.curves.extend(fr);
            if !use_model {
                continue;
            }
            let spec = viewport_to_spec(v);
            match crate::draft::sheet::viewport(m, &bm, &spec, &pens, sh) {
                Ok(prims) => {
                    let mut flat: Vec<linework::Curve> = Vec::new();
                    fn walk(items: &[crate::draft::ir::Prim], inside: bool, out: &mut Vec<linework::Curve>) {
                        for p in items {
                            match p {
                                crate::draft::ir::Prim::Group(g) => walk(&g.children, true, out),
                                crate::draft::ir::Prim::Path(pp) if inside && pp.pen.is_some() => linework::flatten(&pp.segs, out),
                                _ => {}
                            }
                        }
                    }
                    walk(&prims, false, &mut flat);
                    for c in flat {
                        if let Some(c) = linework::clip_to(&c, v.rect_mm) {
                            if b.model >= MAX_BOUNDARY_PIECES {
                                b.notes.push(format!("the model lines of viewport {id} were cut off at {MAX_BOUNDARY_PIECES} pieces"));
                                break;
                            }
                            b.model += 1;
                            b.curves.push(c);
                        }
                    }
                }
                Err(e) => b.notes.push(format!("viewport {id}'s model lines are not boundaries: {e}")),
            }
        }
        b
    }
}
