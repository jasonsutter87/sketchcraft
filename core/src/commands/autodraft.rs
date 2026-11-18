//! AI drafting (design §6, step 13): the commands an agent uses to draw a set without guessing. Arms of `Engine::run`.
//!
//! - `viewport_points`: what a viewport's drawing can be dimensioned to, on paper, with the anchor for each.
//! - `dimension_auto`: attached chain dimensions worked out from a plan (the same strings `callouts::auto_dims` draws,
//!   plus interior walls and grids), marked `source: "auto"` so a re-run never doubles them or touches anyone else's.
//! - `layout_preset`: a permit set of saved, editable sheets (`layout::set_specs`, the blueprint's own layout).
//! - `sheet_render`: a sheet as a PNG (`draft::raster`), to look at.
//!
//! Every one that changes the model is one ordinary undo step.

use super::command::{ScaleArg, ScopeArg};
use super::sheets::{page_of, Bounds};
use super::*;
use crate::callouts::{ChainKind, Side};
use crate::drafting::*;
use crate::layout::PageSpec;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The most points `viewport_points` lists.
const MAX_POINTS: usize = 5_000;
/// How far (metres, along the measured direction) a vertex may be from a stop the plan works out, to stand for it.
const STOP_TOL: f64 = 0.015;
/// Paper mm between dimension strings stacked out from a face.
const TIER_MM: f64 = 7.0;
/// The shortest stretch of cut (metres) the interior strings take for a wall's face.
const WALL_FACE: f64 = 0.45;

/// A vertex a dimension could attach to: where it is in the world, the instance path down to it, its id.
#[derive(Clone, Debug)]
struct Cand {
    w: DVec3,
    path: Vec<Id>,
    v: Id,
}

impl Cand {
    fn anchor(&self) -> Anchor {
        Anchor { path: self.path.clone(), at: AnchorAt::Vertex(self.v), last: self.w.to_array(), extra: Extra::new() }
    }
    fn arg(&self) -> Value {
        if self.path.is_empty() { json!({"vertex": self.v}) } else { json!({"vertex": self.v, "path": self.path}) }
    }
}

/// The ends of every visible edge (each vertex once).
fn candidates(m: &Model) -> Vec<Cand> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in m.world_edges() {
        for (w, v) in [(e.a, e.v[0]), (e.b, e.v[1])] {
            if seen.insert((e.path.to_vec(), v)) {
                out.push(Cand { w, path: e.path.to_vec(), v });
            }
        }
    }
    out
}

fn viewport_of(m: &Model, id: Id) -> Result<Viewport> {
    match m.sheet_items.get(&id).map(|i| &**i) {
        Some(SheetItem::Known(ItemKind::Viewport(v))) => Ok(v.clone()),
        Some(other) => bail!("sheet item {id} is a {}, not a viewport", other.kind_name()),
        None => bail!("no viewport {id} (sheets_info lists each sheet's items)"),
    }
}

fn view_name(v: &Viewport) -> &'static str {
    match v.view {
        ViewKind::Plan { .. } => "plan",
        ViewKind::Elevation { .. } => "elevation",
        ViewKind::Section { .. } => "section",
        ViewKind::View3d { .. } => "3D view",
    }
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::South => "south",
        Side::North => "north",
        Side::West => "west",
        Side::East => "east",
    }
}

/// One dimension string `dimension_auto` wants to make.
struct Want {
    part: String,
    side: Side,
    /// paper mm out from the face it is measured along
    tier: f64,
    /// the world point of the face (where the string's distance out is counted from)
    face: DVec3,
    anchors: Vec<Anchor>,
    /// stops that had no vertex to stand for them
    missed: usize,
}

impl Engine {
    pub(super) fn run_autodraft(&mut self, cmd: Command) -> Result<Value> {
        if !matches!(cmd, Command::ViewportPoints { .. } | Command::SheetRender { .. } | Command::SheetIr { .. }) && self.ctx.is_some() {
            bail!("close the component you're editing first: sheets belong to the whole model");
        }
        Ok(match cmd {
            Command::ViewportPoints { viewport, kinds, limit } => self.viewport_points(viewport, &kinds, limit)?,
            Command::DimensionAuto { viewport, scope, refresh } => self.dimension_auto(viewport, scope, refresh)?,
            Command::LayoutPreset { preset, paper, scale, project } => self.layout_preset(&preset, paper.as_deref(), scale.as_ref(), project)?,
            Command::SheetRender { sheet, path, px } => {
                if !std::path::Path::new(&path).extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                    bail!("sheet_render writes a .png");
                }
                let img = self.sheet_picture(sheet, px.unwrap_or(1600))?;
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                img.save_with_format(&path, image::ImageFormat::Png)?;
                json!({"path": path, "width": img.width(), "height": img.height()})
            }
            Command::SheetIr { sheet, limit_kb } => {
                let limit = match limit_kb {
                    Some(k) if k == 0 || k > crate::draft::irjson::LIMIT >> 10 => bail!("limit_kb is 1 to {}", crate::draft::irjson::LIMIT >> 10),
                    Some(k) => k << 10,
                    None => crate::draft::irjson::LIMIT,
                };
                let page = self.sheet_page(sheet)?;
                let mut v = crate::draft::irjson::page(&page, Some(limit));
                v["sheet"] = json!(sheet);
                v
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }

    // (`sheet_page`, the one page `sheet_render` and `sheet_ir` show, is in `drawings.rs`: drawn from the kept drawings)

    /// A saved sheet drawn as a picture `px` wide (see `draft::raster`): what `sheet_render` writes and the MCP render
    /// tool shows.
    pub fn sheet_picture(&self, sheet: Id, px: u32) -> Result<image::RgbImage> {
        Ok(crate::draft::raster::page(&self.sheet_page(sheet)?, px))
    }

    fn viewport_points(&self, id: Id, kinds: &[String], limit: Option<usize>) -> Result<Value> {
        let m = &self.model;
        let v = viewport_of(m, id)?;
        let Some(proj) = crate::anchors::viewport_proj(&v) else { bail!("viewport {id} is a perspective view: it has no scale, so nothing on it can be dimensioned (make it parallel with viewport_edit scale)") };
        let limit = limit.unwrap_or(500);
        if limit == 0 || limit > MAX_POINTS {
            bail!("limit is 1 to {MAX_POINTS}");
        }
        let kinds: Vec<String> = if kinds.is_empty() { vec!["vertex".into()] } else { kinds.iter().map(|k| k.to_ascii_lowercase()).collect() };
        if let Some(bad) = kinds.iter().find(|k| !matches!(k.as_str(), "vertex" | "midpoint" | "opening")) {
            bail!("kinds are vertex, midpoint and opening (got {bad:?})");
        }
        let bm = crate::environment::without(m);
        let [rx, ry, rw, rh] = v.rect_mm;
        let on_paper = |q: [f64; 2]| q[0] >= rx && q[0] <= rx + rw && q[1] >= ry && q[1] <= ry + rh;
        // what the view shows: a plan, the floor up to the cut; a section, what is beyond the plane
        let shown = |w: DVec3| match &v.view {
            ViewKind::Plan { cut_z, floor_z, .. } => w.z >= floor_z - 0.05 && w.z <= cut_z + 0.05,
            ViewKind::Section { origin, normal, .. } => (w - DVec3::from(*origin)).dot(DVec3::new(normal[0], normal[1], 0.0)) >= -1e-6,
            _ => true,
        };
        // nearest the viewer first, so one paper point keeps the vertex in front (a plan: the lowest)
        let toward = -proj.look;
        let mut pts: Vec<(f64, [f64; 2], DVec3, &'static str, Value)> = Vec::new();
        if kinds.iter().any(|k| k == "vertex") {
            for c in candidates(&bm) {
                if shown(c.w) {
                    let depth = if matches!(v.view, ViewKind::Plan { .. }) { -c.w.z } else { c.w.dot(toward) };
                    pts.push((depth, proj.xy(c.w), c.w, "vertex", c.arg()));
                }
            }
        }
        if kinds.iter().any(|k| k == "midpoint") {
            for e in bm.world_edges() {
                let w = (e.a + e.b) / 2.0;
                if shown(w) && e.a.distance(e.b) > 1e-6 {
                    let depth = if matches!(v.view, ViewKind::Plan { .. }) { -w.z } else { w.dot(toward) };
                    let mut a = json!({"between": e.v, "t": 0.5});
                    if !e.path.is_empty() {
                        a["path"] = json!(e.path.to_vec());
                    }
                    pts.push((depth, proj.xy(w), w, "midpoint", a));
                }
            }
        }
        if kinds.iter().any(|k| k == "opening") {
            let ViewKind::Plan { cut_z, floor_z, .. } = v.view else { bail!("opening points are for plan viewports (this is a {})", view_name(&v)) };
            let cands: Vec<Cand> = candidates(&bm).into_iter().filter(|c| c.w.z >= floor_z - 0.05 && c.w.z <= cut_z + 0.05).collect();
            for o in crate::callouts::on_floor(&crate::callouts::openings(&bm), floor_z) {
                let c = o.center();
                for s in [-1.0, 1.0] {
                    let target = if o.along_x { DVec3::new(c.x + s * o.width / 2.0, c.y, floor_z) } else { DVec3::new(c.x, c.y + s * o.width / 2.0, floor_z) };
                    let best = cands.iter().filter(|k| k.w.truncate().distance(target.truncate()) < o.width.max(0.3)).min_by(|a, b| a.w.truncate().distance(target.truncate()).total_cmp(&b.w.truncate().distance(target.truncate())));
                    if let Some(b) = best {
                        let mut a = b.arg();
                        a["opening"] = json!(o.tag);
                        pts.push((-b.w.z, proj.xy(b.w), b.w, "opening", a));
                    }
                }
            }
        }
        pts.retain(|p| on_paper(p.1));
        pts.sort_by(|a, b| b.0.total_cmp(&a.0));
        // one point per place on the paper (0.05 mm), the one nearest the viewer
        let mut seen = std::collections::HashSet::new();
        pts.retain(|p| seen.insert((p.3, (p.1[0] * 20.0).round() as i64, (p.1[1] * 20.0).round() as i64)));
        let total = pts.len();
        pts.sort_by(|a, b| (a.1[1], a.1[0]).partial_cmp(&(b.1[1], b.1[0])).unwrap_or(std::cmp::Ordering::Equal));
        let list: Vec<Value> = pts.into_iter().take(limit).map(|(_, mm, w, kind, anchor)| json!({"kind": kind, "mm": [round3(mm[0]), round3(mm[1])], "world": [round4(w.x), round4(w.y), round4(w.z)], "anchor": anchor})).collect();
        Ok(json!({"viewport": id, "sheet": v.sheet, "view": view_name(&v), "points": list, "total": total, "truncated": total > limit}))
    }

    fn dimension_auto(&mut self, vid: Id, scope: Option<ScopeArg>, refresh: bool) -> Result<Value> {
        let v = viewport_of(&self.model, vid)?;
        let ViewKind::Plan { cut_z, floor_z, .. } = v.view else { bail!("dimension_auto works on plan viewports (viewport {vid} is a {}); add one with viewport_add view {{type: plan}} or layout_preset", view_name(&v)) };
        let Some(proj) = crate::anchors::viewport_proj(&v) else { bail!("viewport {vid} can't map the model onto its sheet") };
        let scopes: Vec<String> = match scope {
            None => vec!["exterior".into(), "openings".into()],
            Some(ScopeArg::One(s)) => vec![s.to_ascii_lowercase()],
            Some(ScopeArg::Many(l)) => l.iter().map(|s| s.to_ascii_lowercase()).collect(),
        };
        if scopes.is_empty() {
            bail!("give a scope: exterior, openings, interior or grids (or a list of them)");
        }
        if let Some(bad) = scopes.iter().find(|s| !matches!(s.as_str(), "exterior" | "openings" | "interior" | "grids")) {
            bail!("scope is exterior, openings, interior or grids (got {bad:?})");
        }
        let bm = crate::environment::without(&self.model);
        let Some(outline) = crate::callouts::outline(&bm, cut_z) else { bail!("this plan cuts nothing at {cut_z} m: there are no walls to dimension") };
        let (x0, y0, x1, y1) = outline;
        let ops = crate::callouts::openings(&bm);
        // what can stand for a stop: corners on this floor, inside the building's box
        let cands: Vec<Cand> = candidates(&bm).into_iter().filter(|c| c.w.z >= floor_z - 0.05 && c.w.z <= cut_z + 0.05 && c.w.x >= x0 - 0.05 && c.w.x <= x1 + 0.05 && c.w.y >= y0 - 0.05 && c.w.y <= y1 + 0.05).collect();
        // the vertex standing for a stop at `along` (x for a string along x) nearest the face it is measured on
        let pick = |horizontal: bool, along: f64, face: f64| -> Option<&Cand> {
            cands
                .iter()
                .filter(|c| ((if horizontal { c.w.x } else { c.w.y }) - along).abs() <= STOP_TOL)
                .min_by(|a, b| {
                    let score = |c: &Cand| ((if horizontal { c.w.x } else { c.w.y }) - along).abs() * 20.0 + ((if horizontal { c.w.y } else { c.w.x }) - face).abs() + (c.w.z - floor_z).abs() * 0.01 + c.path.len() as f64 * 1e-3;
                    score(a).total_cmp(&score(b))
                })
        };
        let face_of = |s: Side| match s {
            Side::South => y0,
            Side::North => y1,
            Side::West => x0,
            Side::East => x1,
        };
        let horizontal = |s: Side| matches!(s, Side::South | Side::North);
        let chain = |side: Side, stops: &[f64]| -> (Vec<Anchor>, usize) {
            let mut out: Vec<Anchor> = Vec::new();
            let mut missed = 0;
            for s in stops {
                match pick(horizontal(side), *s, face_of(side)) {
                    Some(c) if out.last().is_none_or(|l| l.at != AnchorAt::Vertex(c.v) || l.path != c.path) => out.push(c.anchor()),
                    Some(_) => {}
                    None => missed += 1,
                }
            }
            (out, missed)
        };
        let face_pt = |side: Side| match side {
            Side::South => DVec3::new((x0 + x1) / 2.0, y0, floor_z),
            Side::North => DVec3::new((x0 + x1) / 2.0, y1, floor_z),
            Side::West => DVec3::new(x0, (y0 + y1) / 2.0, floor_z),
            Side::East => DVec3::new(x1, (y0 + y1) / 2.0, floor_z),
        };
        // the first string clears the door and window tags (0.5 m out); each further one is a tier beyond
        let first = (500.0 / v.scale + 8.0).max(10.0);
        let mut wants: Vec<Want> = Vec::new();
        let mut empty: Vec<Value> = Vec::new();
        let (chains, _) = crate::callouts::auto_chains(&bm, &ops, floor_z, outline);
        for sc in &scopes {
            let before = wants.len();
            match sc.as_str() {
                "openings" | "exterior" => {
                    let kind = if sc == "openings" { ChainKind::Openings } else { ChainKind::Overall };
                    for c in chains.iter().filter(|c| c.kind == kind) {
                        let stops: Vec<f64> = c.stops.iter().map(|p| if horizontal(c.side) { p.x } else { p.y }).collect();
                        let (anchors, missed) = chain(c.side, &stops);
                        let tier = if kind == ChainKind::Openings { first } else { first + TIER_MM };
                        wants.push(Want { part: format!("{sc}/{}", side_name(c.side)), side: c.side, tier, face: face_pt(c.side), anchors, missed });
                    }
                }
                "interior" => {
                    // every wall face the plan cuts, across the building: along x (drawn north), along y (drawn west)
                    let cut = bm.section(DVec3::new(0.0, 0.0, cut_z), DVec3::new(0.0, 0.0, -1.0))?;
                    let (mut xs, mut ys) = (Vec::new(), Vec::new());
                    for r in &cut.regions {
                        for ring in std::iter::once(&r.outer).chain(r.holes.iter()) {
                            for i in 0..ring.len() {
                                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                                // a wall's face runs a way (a door's jamb, a frame or a post is shorter than this)
                                if (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() > WALL_FACE {
                                    xs.push(a.x);
                                }
                                if (a.y - b.y).abs() < 1e-3 && (a.x - b.x).abs() > WALL_FACE {
                                    ys.push(a.y);
                                }
                            }
                        }
                    }
                    for (side, mut vals) in [(Side::North, xs), (Side::West, ys)] {
                        vals.sort_by(|a, b| a.total_cmp(b));
                        vals.dedup_by(|a, b| (*a - *b).abs() < 0.02);
                        // two faces are only the outside of the building: that's the overall dimension, not interior
                        if vals.len() < 3 {
                            continue;
                        }
                        let (anchors, missed) = chain(side, &vals);
                        wants.push(Want { part: format!("interior/{}", side_name(side)), side, tier: first + TIER_MM, face: face_pt(side), anchors, missed });
                    }
                }
                "grids" => {
                    let (anchors_x, anchors_y) = grid_points(&bm, floor_z, cut_z);
                    for (side, list) in [(Side::South, anchors_x), (Side::East, anchors_y)] {
                        if list.len() >= 2 {
                            wants.push(Want { part: format!("grids/{}", side_name(side)), side, tier: first + 2.0 * TIER_MM, face: face_pt(side), anchors: list, missed: 0 });
                        }
                    }
                }
                _ => unreachable!(),
            }
            if wants[before..].iter().all(|w| w.anchors.len() < 2) {
                empty.push(json!({"scope": sc, "why": match sc.as_str() {
                    "openings" => "no doors or windows on this floor's outside walls (components named or classed door/window)",
                    "interior" => "no interior walls cut by this plan",
                    "grids" => "no columns (groups or components named or classed column) or guide points on this floor",
                    _ => "no outside corners to attach to",
                }}));
            }
        }
        // what it made before, for this viewport
        let mine: Vec<(Id, String)> = self
            .model
            .sheet_items
            .iter()
            .filter_map(|(id, i)| match &**i {
                SheetItem::Known(ItemKind::Dimension(d)) if d.source == "auto" && d.viewport == Some(vid) => Some((*id, d.part.clone())),
                _ => None,
            })
            .collect();
        let mut removed = vec![];
        if refresh {
            for (id, part) in &mine {
                if scopes.iter().any(|s| part.split('/').next() == Some(s.as_str())) {
                    self.model.sheet_items.remove(id);
                    removed.push(*id);
                }
            }
        }
        let who = self.made_by();
        let (mut added, mut kept, mut skipped) = (vec![], vec![], vec![]);
        for w in wants {
            if let Some((id, _)) = mine.iter().find(|(id, p)| *p == w.part && !removed.contains(id)) {
                kept.push(json!({"id": id, "part": w.part}));
                continue;
            }
            if w.anchors.len() < 2 {
                skipped.push(json!({"part": w.part, "why": format!("{} of its points had no corner to attach to", w.missed)}));
                continue;
            }
            let mode = if horizontal(w.side) { DimMode::Horizontal } else { DimMode::Vertical };
            // the string's line, `tier` mm out from the face on paper, counted the way `draft::dims` places it
            let dir = if mode == DimMode::Horizontal { [1.0, 0.0] } else { [0.0, 1.0] };
            let nrm = [dir[1], -dir[0]];
            let dot = |p: [f64; 2]| p[0] * nrm[0] + p[1] * nrm[1];
            let (fp, op) = (proj.xy(w.face), proj.xy(w.face + w.side.out()));
            let sign = if dot(op) - dot(fp) >= 0.0 { 1.0 } else { -1.0 };
            let pts: Vec<f64> = w.anchors.iter().map(|a| dot(proj.xy(DVec3::from(a.last)))).collect();
            let nref = pts.iter().copied().fold(if sign > 0.0 { f64::MIN } else { f64::MAX }, |a, b| if sign > 0.0 { a.max(b) } else { a.min(b) });
            let mut offset = dot(fp) + sign * w.tier - nref;
            if offset * sign < 2.0 {
                offset = sign * 2.0;
            }
            let d = Dimension { sheet: v.sheet, anchors: w.anchors, mode, viewport: Some(vid), offset_mm: offset, made_by: who.clone(), source: "auto".into(), part: w.part.clone(), ..Default::default() };
            let values: Vec<Option<f64>> = self.model.measure_dimension(&d).segments.iter().map(|s| s.value).collect();
            let id = self.model.new_id();
            self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
            added.push(json!({"id": id, "part": w.part, "values_m": values, "missed": w.missed}));
        }
        // the plan's worked-out exterior strings make way for the attached ones
        let mut legacy_off = false;
        if (!added.is_empty() || !kept.is_empty()) && scopes.iter().any(|s| s == "exterior" || s == "openings") {
            if let Some(Value::Object(l)) = v.extra.get("legacy") {
                if l.get("callouts") == Some(&json!(true)) && l.get("auto_dims") != Some(&json!(false)) {
                    let mut nv = v.clone();
                    if let Some(Value::Object(l)) = nv.extra.get_mut("legacy") {
                        l.insert("auto_dims".into(), json!(false));
                    }
                    self.model.sheet_items.insert(vid, Arc::new(SheetItem::Known(ItemKind::Viewport(nv))));
                    legacy_off = true;
                }
            }
        }
        let unchanged = added.is_empty() && removed.is_empty() && !legacy_off;
        let mut r = json!({"viewport": vid, "sheet": v.sheet, "added": added, "kept": kept, "skipped": skipped, "nothing_to_dimension": empty, "removed": removed});
        if unchanged {
            r["unchanged"] = json!(true);
        }
        Ok(r)
    }

    fn layout_preset(&mut self, preset: &str, paper: Option<&str>, scale: Option<&ScaleArg>, project: Option<String>) -> Result<Value> {
        if !preset.eq_ignore_ascii_case("permit") {
            bail!("layout_preset knows the \"permit\" preset (got {preset:?})");
        }
        let paper_name = paper.unwrap_or("a3");
        let paper = Paper::named(paper_name)?;
        let scale = match scale {
            None => None,
            Some(ScaleArg::Text(t)) => Some(t.clone()),
            Some(ScaleArg::N(n)) => Some(format!("1:{n}")),
        };
        let bm = crate::environment::without(&self.model);
        if bm.faces.is_empty() && bm.instances.is_empty() || bm.world_bounds().is_none() {
            bail!("the model is empty: there is nothing to lay out (draw or import a building first)");
        }
        let (pages, notes) = crate::layout::set_specs(&bm, paper_name, &[], &crate::layout::SetOpts { per_floor: true, scale })?;
        drop(bm);
        let (sw, sh) = (paper.w_mm, paper.h_mm);
        // the cover: the sheet index, and the building in 3D beside it
        let mut index = vec![vec!["SHEET".to_string(), "TITLE".to_string()], vec!["G-001".into(), "Cover and sheet index".into()]];
        index.extend(pages.iter().map(|(n, p)| vec![n.clone(), p.title.clone()]));
        let cover = PageSpec {
            title: "Cover and sheet index".into(),
            viewports: vec![crate::layout::ViewportSpec { view: "iso".into(), label: Some("3D view".into()), x: sw * 0.45, y: 20.0, w: sw * 0.55 - 20.0, h: (sh - 80.0).max(40.0), ..Default::default() }],
            table: index,
            ..Default::default()
        };
        let mut all = vec![("G-001".to_string(), cover)];
        all.extend(pages);
        if self.model.sheets.len() + all.len() > self.sheet_limit() {
            bail!("that would make {} sheets; the limit is {}", self.model.sheets.len() + all.len(), self.sheet_limit());
        }
        let mut fields = BTreeMap::new();
        if let Some(p) = project.filter(|p| !p.is_empty()) {
            if p.len() > 200 {
                bail!("the project name is too long ({} bytes, limit 200)", p.len());
            }
            fields.insert("project".to_string(), p);
        }
        if !notes.is_empty() {
            fields.insert("notes".to_string(), notes.join("\n"));
        }
        let first = self.model.sheets.values().map(|s| s.order).max().map_or(0, |o| o.saturating_add(1));
        let bounds: Bounds = crate::environment::without(&self.model).world_bounds();
        let mut made = vec![];
        for (i, (number, page)) in all.iter().enumerate() {
            let id = self.sheet_from_page(page, &paper, first.saturating_add(i as i32), &fields, bounds)?;
            if let Some(s) = self.model.sheets.get_mut(&id) {
                s.number = number.clone();
            }
            let vps: Vec<Id> = self.model.sheet_items.iter().filter(|(_, it)| matches!(&***it, SheetItem::Known(ItemKind::Viewport(v)) if v.sheet == id)).map(|(k, _)| *k).collect();
            let plans: Vec<Id> = vps.iter().copied().filter(|k| matches!(self.model.sheet_items.get(k).map(|x| &**x), Some(SheetItem::Known(ItemKind::Viewport(Viewport { view: ViewKind::Plan { .. }, .. }))))).collect();
            made.push(json!({"sheet": id, "number": number, "name": page.title, "viewports": vps, "plans": plans}));
        }
        let plans: Vec<Value> = made.iter().flat_map(|s| s["plans"].as_array().cloned().unwrap_or_default()).collect();
        Ok(json!({"preset": "permit", "paper": paper.name, "sheets": made, "plan_viewports": plans}))
    }
}

/// Grid points on a floor: the centre of each column (a group or component placement named or classed as one), at
/// its foot, and the guide points standing on the floor. Returns chains along x and along y (one anchor per distinct
/// position, in order).
fn grid_points(m: &Model, floor_z: f64, cut_z: f64) -> (Vec<Anchor>, Vec<Anchor>) {
    let named = |id: Id| {
        let name = m.groups.get(&id).map(|g| g.name.clone()).or_else(|| m.instances.get(&id).map(|i| if i.name.is_empty() { m.definitions.get(&i.def).map(|d| d.name.clone()).unwrap_or_default() } else { i.name.clone() })).unwrap_or_default();
        m.classes.get(&id).is_some_and(|c| c == "IfcColumn") || name.to_ascii_lowercase().contains("column")
    };
    // the column an edge belongs to: a group around it (a root edge's `owner` is the edge itself), or a placement
    let column_of = |e: &crate::components::WEdge| -> Option<Id> {
        if !e.path.is_empty() {
            return e.path.iter().copied().find(|i| named(*i));
        }
        let mut g = m.edges.get(&e.owner)?.group;
        let mut hops = 0;
        while let Some(id) = g {
            if named(id) {
                return Some(id);
            }
            g = m.groups.get(&id)?.parent;
            hops += 1;
            if hops > 64 {
                return None;
            }
        }
        None
    };
    // the corners at each column's foot
    let mut feet: BTreeMap<Id, Vec<Cand>> = BTreeMap::new();
    for e in m.world_edges() {
        let Some(col) = column_of(&e) else { continue };
        for (w, v) in [(e.a, e.v[0]), (e.b, e.v[1])] {
            if w.z >= floor_z - 0.05 && w.z <= cut_z {
                let list = feet.entry(col).or_default();
                if !list.iter().any(|c| c.v == v && c.path == *e.path) {
                    list.push(Cand { w, path: e.path.to_vec(), v });
                }
            }
        }
    }
    let mut points: Vec<(DVec3, Anchor)> = Vec::new();
    for list in feet.values() {
        let low = list.iter().map(|c| c.w.z).fold(f64::MAX, f64::min);
        let foot: Vec<&Cand> = list.iter().filter(|c| c.w.z < low + 1e-3).collect();
        // the centre: halfway along the longest diagonal of the foot (a square's corners, a circle's opposite points)
        let mut best: Option<(f64, &Cand, &Cand)> = None;
        for (i, a) in foot.iter().enumerate() {
            for b in &foot[i + 1..] {
                let d = a.w.distance(b.w);
                if a.path == b.path && best.is_none_or(|(bd, ..)| d > bd + 1e-9) {
                    best = Some((d, a, b));
                }
            }
        }
        if let Some((_, a, b)) = best {
            let w = (a.w + b.w) / 2.0;
            points.push((w, Anchor { path: a.path.clone(), at: AnchorAt::Between { a: a.v, b: b.v, t: 0.5, extra: Extra::new() }, last: w.to_array(), extra: Extra::new() }));
        }
    }
    for g in m.guides.iter().filter(|g| g.dir.is_none() && g.p.z >= floor_z - 0.05 && g.p.z <= cut_z) {
        points.push((g.p, Anchor { path: vec![], at: AnchorAt::Guide(g.id), last: g.p.to_array(), extra: Extra::new() }));
    }
    let along = |f: fn(&DVec3) -> f64| {
        let mut p: Vec<&(DVec3, Anchor)> = points.iter().collect();
        p.sort_by(|a, b| f(&a.0).total_cmp(&f(&b.0)));
        let mut out: Vec<(f64, Anchor)> = Vec::new();
        for (w, a) in p {
            if out.last().is_none_or(|(x, _)| (f(w) - x).abs() > 0.02) {
                out.push((f(w), a.clone()));
            }
        }
        out.into_iter().map(|(_, a)| a).collect::<Vec<_>>()
    };
    (along(|w| w.x), along(|w| w.y))
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}
