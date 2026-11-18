//! AI drafting (#10, design §6 and step 13): an agent lays out a permit set, dimensions the plans with attached chain
//! dimensions, hatches by tag, looks at the sheet and exports it. Every step is one ordinary undo step.

#[path = "pdftext/mod.rs"]
mod pdftext;

use serde_json::{json, Value};
use sketchcraft::drafting::{Dimension, DimMode, ItemKind, SheetItem};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    r["error"].as_str().unwrap().to_string()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-ai-drafting-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A one-storey house, 8 x 6 m (walls 0.2 thick on their centre lines): the outside walls and an interior wall at
/// x = 4 (both tagged "Walls"), a door and a window in the south wall, a stair, and two columns. `stairs: false`
/// leaves the stair out.
fn house_with(stairs: bool) -> Engine {
    let mut e = Engine::new();
    let w = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [8, 0, 0], [8, 6, 0], [0, 6, 0]], "closed": true, "height": 2.7}))["group"].clone();
    let inner = ok(&mut e, json!({"cmd": "wall", "points": [[4, 0.1, 0], [4, 5.9, 0]], "height": 2.7}))["group"].clone();
    ok(&mut e, json!({"cmd": "tag", "ids": [w, inner], "tag": "Walls"}));
    ok(&mut e, json!({"cmd": "insert", "asset": "door", "at": [1.0, -0.1, 0.0]}));
    ok(&mut e, json!({"cmd": "insert", "asset": "window", "at": [5.5, -0.1, 0.9]}));
    if stairs {
        ok(&mut e, json!({"cmd": "stairs", "at": [5.0, 2.0, 0.0], "rise": 1.0}));
    }
    ok(&mut e, json!({"cmd": "column", "at": [2, 3, 0], "size": 0.3, "height": 2.7}));
    ok(&mut e, json!({"cmd": "column", "at": [6, 4, 0], "size": 0.3, "height": 2.7}));
    e
}

fn house() -> Engine {
    house_with(true)
}

fn dims(e: &Engine) -> Vec<(u32, Dimension)> {
    e.model.sheet_items.iter().filter_map(|(k, i)| if let SheetItem::Known(ItemKind::Dimension(d)) = &**i { Some((*k, d.clone())) } else { None }).collect()
}

/// The permit set; returns (the reply, the first plan's viewport, its sheet).
fn permit(e: &mut Engine) -> (Value, u32, u32) {
    let r = ok(e, json!({"cmd": "layout_preset", "preset": "permit", "paper": "a2"}));
    let vp = r["plan_viewports"][0].as_u64().unwrap() as u32;
    let sheet = r["sheets"][1]["sheet"].as_u64().unwrap() as u32;
    (r, vp, sheet)
}

/// The measured values of a dimension, metres (None: broken).
fn values(e: &Engine, id: u32) -> Vec<Option<f64>> {
    let SheetItem::Known(ItemKind::Dimension(d)) = &*e.model.sheet_items[&id] else { panic!("{id} is not a dimension") };
    e.model.measure_dimension(d).segments.iter().map(|s| s.value).collect()
}

fn auto(e: &mut Engine, vp: u32, scope: Value) -> Value {
    ok(e, json!({"cmd": "dimension_auto", "viewport": vp, "scope": scope}))
}

fn part(r: &Value, p: &str) -> u32 {
    r["added"].as_array().unwrap().iter().find(|a| a["part"] == p).unwrap_or_else(|| panic!("no {p} in {r}"))["id"].as_u64().unwrap() as u32
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn every_scope_makes_attached_chains_on_a_plan() {
    let mut e = house();
    let (_, vp, sheet) = permit(&mut e);

    // exterior: the overall width (south) and depth (east), outside face to outside face
    let r = auto(&mut e, vp, json!("exterior"));
    assert_eq!(r["added"].as_array().unwrap().len(), 2, "{r}");
    let south = part(&r, "exterior/south");
    assert!(close(values(&e, south)[0].unwrap(), 8.2), "{:?}", values(&e, south));
    let east = part(&r, "exterior/east");
    assert!(values(&e, east)[0].unwrap() >= 6.2 - 1e-6, "{:?}", values(&e, east));

    // openings: one chain along the south face, both sides of the door and the window, corner to corner
    let r = auto(&mut e, vp, json!("openings"));
    assert_eq!(r["added"].as_array().unwrap().len(), 1, "{r}");
    let op = part(&r, "openings/south");
    let v: Vec<f64> = values(&e, op).into_iter().map(Option::unwrap).collect();
    assert_eq!(v.len(), 5, "{v:?}");
    assert!(close(v.iter().sum::<f64>(), 8.2), "{v:?}");

    // interior: every wall face across the building, along x (the interior wall's two faces among them) and y
    let r = auto(&mut e, vp, json!("interior"));
    let north = part(&r, "interior/north");
    let v: Vec<f64> = values(&e, north).into_iter().map(Option::unwrap).collect();
    assert!(close(v.iter().sum::<f64>(), 8.2), "{v:?}");
    assert!(v.iter().filter(|x| close(**x, 0.2)).count() >= 3, "outside walls and the interior wall are 0.2 thick: {v:?}");
    part(&r, "interior/west");

    // grids: the two columns' centres, along x and along y
    let r = auto(&mut e, vp, json!("grids"));
    let gx = part(&r, "grids/south");
    assert!(close(values(&e, gx)[0].unwrap(), 4.0), "{:?}", values(&e, gx));
    let gy = part(&r, "grids/east");
    assert!(close(values(&e, gy)[0].unwrap(), 1.0), "{:?}", values(&e, gy));

    // all of them: attached (every anchor resolves), marked as made by dimension_auto, measured in that plan
    let all = dims(&e);
    assert_eq!(all.len(), 2 + 1 + 2 + 2);
    for (id, d) in &all {
        assert_eq!(d.source, "auto");
        assert_eq!(d.viewport, Some(vp));
        assert_eq!(d.sheet, sheet);
        assert!(e.model.broken_anchors(d).is_empty(), "{id}");
        let along_x = d.part.ends_with("south") || d.part.ends_with("north");
        assert_eq!(d.mode, if along_x { DimMode::Horizontal } else { DimMode::Vertical }, "{}", d.part);
    }
    // sheets_info shows the mark and what each measures
    let info = ok(&mut e, json!({"cmd": "sheets_info", "sheet": sheet}));
    let it = info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == south).unwrap().clone();
    assert_eq!(it["source"], "auto");
    assert_eq!(it["part"], "exterior/south");
    assert_eq!(it["measured"]["spans"][0]["text"], "8.20 m");

    // a list of scopes in one go, on a fresh set, is one step too
    let mut f = house();
    let (_, vp2, _) = permit(&mut f);
    let depth = f.undo_depth();
    let r = auto(&mut f, vp2, json!(["exterior", "openings", "interior", "grids"]));
    assert_eq!(r["added"].as_array().unwrap().len(), 7, "{r}");
    assert_eq!(f.undo_depth(), depth + 1);
    // default scope: exterior and openings
    let mut g = house();
    let (_, vp3, _) = permit(&mut g);
    assert_eq!(ok(&mut g, json!({"cmd": "dimension_auto", "viewport": vp3}))["added"].as_array().unwrap().len(), 3);
}

#[test]
fn move_a_wall_and_the_automatic_values_follow() {
    let mut e = house();
    let (_, vp, _) = permit(&mut e);
    let r = auto(&mut e, vp, json!("interior"));
    let north = part(&r, "interior/north");
    let before: Vec<f64> = values(&e, north).into_iter().map(Option::unwrap).collect();
    // the interior wall moves 0.5 m east: the spans either side of it change by 0.5, the total stays
    let inner = e.model.groups.iter().filter(|(_, g)| g.tag.as_deref() == Some("Walls")).map(|(k, _)| *k).max().unwrap();
    ok(&mut e, json!({"cmd": "move", "ids": [inner], "by": [0.5, 0, 0]}));
    let after: Vec<f64> = values(&e, north).into_iter().map(|v| v.expect("still attached")).collect();
    assert_eq!(before.len(), after.len());
    assert!(close(after.iter().sum::<f64>(), before.iter().sum::<f64>()), "{before:?} -> {after:?}");
    let grew = before.iter().zip(&after).filter(|(b, a)| close(**a - **b, 0.5)).count();
    let shrank = before.iter().zip(&after).filter(|(b, a)| close(**b - **a, 0.5)).count();
    assert_eq!((grew, shrank), (1, 1), "{before:?} -> {after:?}");
    // and back with undo
    ok(&mut e, json!({"cmd": "undo"}));
    let back: Vec<f64> = values(&e, north).into_iter().map(Option::unwrap).collect();
    assert!(before.iter().zip(&back).all(|(a, b)| close(*a, *b)));
}

#[test]
fn a_rerun_adds_nothing_and_other_dimensions_are_never_touched() {
    let mut e = house();
    let (_, vp, sheet) = permit(&mut e);
    // the person's own dimension on the plan
    let mine = ok(&mut e, json!({"cmd": "sheet_dim", "sheet": sheet, "anchors": [{"point": [-0.1, -0.1, 0]}, {"point": [-0.1, 6.1, 0]}], "mode": "vertical", "offset_mm": -30}))["dimension"].as_u64().unwrap() as u32;
    let mine_before = e.model.sheet_items[&mine].clone();
    let first = auto(&mut e, vp, json!(["exterior", "openings"]));
    let n = dims(&e).len();
    assert_eq!(n, 1 + 3);
    // again: nothing new, all kept, no undo step, the model as it was
    let depth = e.undo_depth();
    let snapshot = e.model.sheet_items.clone();
    let again = auto(&mut e, vp, json!(["exterior", "openings"]));
    assert_eq!(again["added"].as_array().unwrap().len(), 0, "{again}");
    assert_eq!(again["kept"].as_array().unwrap().len(), 3, "{again}");
    assert_eq!(again["unchanged"], true);
    assert_eq!(e.undo_depth(), depth, "a run that changes nothing takes no undo step");
    assert_eq!(e.model.sheet_items, snapshot);
    // adding a scope adds only that scope
    auto(&mut e, vp, json!(["exterior", "openings", "interior"]));
    assert_eq!(dims(&e).len(), n + 2);
    // the person edits an automatic one: a re-run leaves it as they made it
    let south = part(&first, "exterior/south");
    let SheetItem::Known(ItemKind::Dimension(mut d)) = (*e.model.sheet_items[&south]).clone() else { panic!() };
    d.offset_mm = -40.0;
    e.model.sheet_items.insert(south, std::sync::Arc::new(SheetItem::Known(ItemKind::Dimension(d.clone()))));
    auto(&mut e, vp, json!("exterior"));
    assert_eq!(e.model.sheet_items[&south], std::sync::Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
    // refresh replaces its own for those scopes, and only its own
    let r = ok(&mut e, json!({"cmd": "dimension_auto", "viewport": vp, "scope": "exterior", "refresh": true}));
    assert_eq!(r["removed"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(r["added"].as_array().unwrap().len(), 2, "{r}");
    assert!(!e.model.sheet_items.contains_key(&south));
    assert_eq!(e.model.sheet_items[&mine], mine_before, "the person's dimension is untouched");
    assert_eq!(dims(&e).len(), n + 2);
    // a dimension another source made (not "auto") with the same part name is not dimension_auto's either
    let SheetItem::Known(ItemKind::Dimension(mut other)) = (*e.model.sheet_items[&mine]).clone() else { panic!() };
    other.part = "interior/west".into();
    e.model.sheet_items.insert(mine, std::sync::Arc::new(SheetItem::Known(ItemKind::Dimension(other.clone()))));
    ok(&mut e, json!({"cmd": "dimension_auto", "viewport": vp, "scope": "interior", "refresh": true}));
    assert_eq!(e.model.sheet_items[&mine], std::sync::Arc::new(SheetItem::Known(ItemKind::Dimension(other))));
}

#[test]
fn one_undo_takes_back_a_whole_dimension_auto() {
    let mut e = house();
    let (_, vp, _) = permit(&mut e);
    let vp_before = e.model.sheet_items[&vp].clone();
    let depth = e.undo_depth();
    auto(&mut e, vp, json!(["exterior", "openings", "interior", "grids"]));
    assert_eq!(e.undo_depth(), depth + 1);
    // the plan no longer draws its worked-out strings (the attached ones stand in for them)
    assert_eq!(serde_json::to_value(&*e.model.sheet_items[&vp]).unwrap()["legacy"]["auto_dims"], false);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(dims(&e).is_empty());
    assert_eq!(e.model.sheet_items[&vp], vp_before);
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(dims(&e).len(), 7);
}

#[test]
fn the_plan_draws_each_string_once() {
    let mut e = house();
    let (_, vp, sheet) = permit(&mut e);
    let d = scratch("once");
    let svg = |e: &mut Engine, n: &str| {
        let p = d.join(format!("{n}.svg"));
        ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [sheet]}));
        std::fs::read_to_string(&p).unwrap()
    };
    let count = |s: &str, t: &str| s.matches(&format!(">{t}</text>")).count();
    let before = svg(&mut e, "before");
    assert_eq!(count(&before, "8.20 m"), 1, "the worked-out overall width");
    auto(&mut e, vp, json!(["exterior", "openings"]));
    let after = svg(&mut e, "after");
    assert_eq!(count(&after, "8.20 m"), 1, "the attached one, not both");
    // the door and window tags and the north arrow stay
    assert!(after.contains(">D1</text>") && after.contains(">W1</text>") && after.contains(">N</text>"));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn dimension_auto_refuses_what_it_cannot_do() {
    let mut e = house();
    let (r, vp, sheet) = permit(&mut e);
    let elevation = r["sheets"][2]["viewports"][0].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "dimension_auto", "viewport": elevation})).contains("plan viewports"));
    assert!(refused(&mut e, json!({"cmd": "dimension_auto", "viewport": sheet})).contains("no viewport"));
    assert!(refused(&mut e, json!({"cmd": "dimension_auto", "viewport": vp, "scope": "everything"})).contains("scope is"));
    assert!(refused(&mut e, json!({"cmd": "dimension_auto", "viewport": vp, "scope": []})).contains("give a scope"));
    // a floor with nothing to find says why, and changes nothing
    let mut bare = Engine::new();
    ok(&mut bare, json!({"cmd": "wall", "points": [[0, 0, 0], [5, 0, 0], [5, 4, 0], [0, 4, 0]], "closed": true, "height": 2.7}));
    let (_, vp, _) = permit(&mut bare);
    let r = auto(&mut bare, vp, json!(["openings", "grids"]));
    assert_eq!(r["unchanged"], true);
    assert_eq!(r["nothing_to_dimension"].as_array().unwrap().len(), 2, "{r}");
    // inside a component being edited: refused (sheets are the whole model's)
    let door = *e.model.instances.keys().next().unwrap();
    ok(&mut e, json!({"cmd": "edit", "id": door}));
    assert!(refused(&mut e, json!({"cmd": "dimension_auto", "viewport": vp})).contains("close the component"));
}

#[test]
fn a_permit_set_is_editable_sheets_and_one_undo_removes_it() {
    let mut e = house();
    let depth = e.undo_depth();
    let (r, vp, _) = permit(&mut e);
    assert_eq!(e.undo_depth(), depth + 1);
    let numbers: Vec<&str> = r["sheets"].as_array().unwrap().iter().map(|s| s["number"].as_str().unwrap()).collect();
    assert_eq!(numbers, ["G-001", "A-101", "A-201", "A-301", "A-601"]);
    assert_eq!(e.model.sheets.len(), 5);
    for s in e.model.sheets.values() {
        assert_eq!(s.paper.name, "A2");
    }
    // cover: the index lists every sheet; elevations: four; the section; the schedule is a table
    let info = ok(&mut e, json!({"cmd": "sheets_info"}));
    let sheets = info["sheets"].as_array().unwrap();
    let cover = &sheets[0]["items"];
    let table = cover.as_array().unwrap().iter().find(|i| i["kind"] == "table").unwrap();
    assert_eq!(table["rows"].as_array().unwrap().len(), 1 + 5);
    assert_eq!(table["rows"][3], json!(["A-201", "Elevations"]));
    assert_eq!(sheets[2]["items"].as_array().unwrap().iter().filter(|i| i["kind"] == "viewport" && i["view"]["type"] == "elevation").count(), 4);
    assert!(sheets[3]["items"].as_array().unwrap().iter().any(|i| i["view"]["type"] == "section"));
    assert!(sheets[4]["items"].as_array().unwrap().iter().any(|i| i["kind"] == "table"));
    // every item says the person made it here (an agent's would say "agent": see the MCP test)
    assert!(e.model.sheet_items.values().all(|i| serde_json::to_value(&**i).unwrap().get("made_by").is_none()));
    // they're ordinary saved sheets: rename, rescale, move, delete like any other
    let plan_sheet = r["sheets"][1]["sheet"].clone();
    ok(&mut e, json!({"cmd": "sheet_edit", "sheet": plan_sheet, "name": "Ground floor"}));
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "scale": "1:50"}));
    let SheetItem::Known(ItemKind::Viewport(v)) = &*e.model.sheet_items[&vp] else { panic!() };
    assert_eq!(v.scale, 50.0);
    ok(&mut e, json!({"cmd": "undo"}));
    ok(&mut e, json!({"cmd": "undo"}));
    // the preset is one step: one more undo and every sheet is gone
    assert_eq!(e.model.sheets.len(), 5);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(e.model.sheets.is_empty() && e.model.sheet_items.is_empty());
    assert_eq!(e.undo_depth(), depth);
    // a scale for every drawing
    let r = ok(&mut e, json!({"cmd": "layout_preset", "paper": "a1", "scale": "1:50", "project": "Lot 7"}));
    for s in r["sheets"].as_array().unwrap().iter().skip(1) {
        for v in s["viewports"].as_array().unwrap() {
            let SheetItem::Known(ItemKind::Viewport(v)) = &*e.model.sheet_items[&(v.as_u64().unwrap() as u32)] else { panic!() };
            assert_eq!(v.scale, 50.0);
        }
    }
    assert!(e.model.sheets.values().all(|s| s.fields.get("project").map(String::as_str) == Some("Lot 7")));
    // and it comes after sheets already there
    assert_eq!(e.model.sheets.len(), 5);
    ok(&mut e, json!({"cmd": "layout_preset", "paper": "a3"}));
    assert_eq!(e.model.sheets.len(), 10);
}

#[test]
fn a_permit_set_has_a_section_only_through_stairs() {
    let mut e = house_with(false);
    let (r, _, _) = permit(&mut e);
    let numbers: Vec<&str> = r["sheets"].as_array().unwrap().iter().map(|s| s["number"].as_str().unwrap()).collect();
    assert_eq!(numbers, ["G-001", "A-101", "A-201", "A-601"]);
    // and no plan is marked with a section that isn't there
    let SheetItem::Known(ItemKind::Viewport(v)) = &*e.model.sheet_items[&(r["plan_viewports"][0].as_u64().unwrap() as u32)] else { panic!() };
    assert!(v.extra["legacy"].get("section_mark").is_none());
}

#[test]
fn layout_preset_refuses_an_empty_model_and_unknown_presets() {
    let mut e = Engine::new();
    assert!(refused(&mut e, json!({"cmd": "layout_preset", "preset": "permit"})).contains("model is empty"));
    assert!(e.model.sheets.is_empty());
    let mut h = house();
    assert!(refused(&mut h, json!({"cmd": "layout_preset", "preset": "brochure"})).contains("permit"));
    assert!(refused(&mut h, json!({"cmd": "layout_preset", "paper": "napkin"})).len() > 3);
    assert!(refused(&mut h, json!({"cmd": "layout_preset", "scale": "big"})).len() > 3);
    assert!(h.model.sheets.is_empty());
}

#[test]
fn viewport_points_lists_attachable_points_on_paper() {
    let mut e = house();
    let (_, vp, sheet) = permit(&mut e);
    let r = ok(&mut e, json!({"cmd": "viewport_points", "viewport": vp, "limit": 5000}));
    assert_eq!(r["view"], "plan");
    let pts = r["points"].as_array().unwrap();
    assert!(pts.len() > 20 && r["truncated"] == false);
    // the south-west outside corner is there, and its anchor makes a dimension that attaches to it
    let sw = pts.iter().find(|p| p["world"] == json!([-0.1, -0.1, 0.0])).expect("the outside corner");
    let se = pts.iter().find(|p| p["world"] == json!([8.1, -0.1, 0.0])).expect("the other corner");
    let d = ok(&mut e, json!({"cmd": "sheet_dim", "sheet": sheet, "anchors": [sw["anchor"], se["anchor"]], "mode": "horizontal"}));
    assert!(close(d["values_m"][0].as_f64().unwrap(), 8.2));
    // its paper position is where the drawing puts it: 8.2 m apart at the plan's scale
    let SheetItem::Known(ItemKind::Viewport(v)) = &*e.model.sheet_items[&vp] else { panic!() };
    let dx = se["mm"][0].as_f64().unwrap() - sw["mm"][0].as_f64().unwrap();
    assert!((dx - 8200.0 / v.scale).abs() < 0.01, "{dx}");
    // midpoints and openings
    let r = ok(&mut e, json!({"cmd": "viewport_points", "viewport": vp, "kinds": ["midpoint", "opening"], "limit": 5000}));
    let pts = r["points"].as_array().unwrap();
    assert!(pts.iter().any(|p| p["kind"] == "midpoint" && p["anchor"]["t"] == 0.5));
    assert_eq!(pts.iter().filter(|p| p["kind"] == "opening").count(), 4, "both sides of the door and the window");
    // a limit, and refusals
    let r = ok(&mut e, json!({"cmd": "viewport_points", "viewport": vp, "limit": 3}));
    assert_eq!(r["points"].as_array().unwrap().len(), 3);
    assert_eq!(r["truncated"], true);
    assert!(refused(&mut e, json!({"cmd": "viewport_points", "viewport": vp, "kinds": ["edges"]})).contains("kinds"));
    assert!(refused(&mut e, json!({"cmd": "viewport_points", "viewport": vp, "limit": 0})).contains("limit"));
    // read-only: no undo step
    let depth = e.undo_depth();
    ok(&mut e, json!({"cmd": "viewport_points", "viewport": vp}));
    assert_eq!(e.undo_depth(), depth);
}

#[test]
fn sheet_render_draws_the_sheet_as_a_picture() {
    let mut e = house();
    let (_, vp, sheet) = permit(&mut e);
    auto(&mut e, vp, json!(["exterior", "openings"]));
    let d = scratch("render");
    let p = d.join("plan.png");
    let r = ok(&mut e, json!({"cmd": "sheet_render", "sheet": sheet, "path": p, "px": 1200}));
    assert_eq!(r["width"], 1200);
    let img = image::open(&p).unwrap().to_rgb8();
    assert_eq!(img.width(), 1200);
    // A2 is 594 x 420 mm
    assert!((img.height() as f64 - 1200.0 * 420.0 / 594.0).abs() <= 1.0);
    // white paper with dark drawing on it: plenty of ink, mostly paper
    let dark = img.pixels().filter(|p| p.0.iter().map(|c| *c as u32).sum::<u32>() < 200).count();
    let white = img.pixels().filter(|p| p.0 == [255, 255, 255]).count();
    let n = (img.width() * img.height()) as usize;
    assert!(dark > n / 200 && white > n / 2, "dark {dark}, white {white} of {n}");
    // the same picture the MCP render tool shows
    assert_eq!(e.sheet_picture(sheet, 1200).unwrap().as_raw(), img.as_raw());
    // hatching the walls changes the plan's picture
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick"}, "cut": {"fill": "hatch"}}));
    let hatched = e.sheet_picture(sheet, 1200).unwrap();
    // SC_DRAFT_OUT=<dir> keeps the picture to look at
    if let Some(out) = std::env::var_os("SC_DRAFT_OUT") {
        hatched.save(std::path::Path::new(&out).join("ai-drafting-plan.png")).unwrap();
    }
    assert_ne!(hatched.as_raw(), img.as_raw());
    // refusals; read-only
    assert!(refused(&mut e, json!({"cmd": "sheet_render", "sheet": sheet, "path": d.join("x.jpg")})).contains(".png"));
    assert!(refused(&mut e, json!({"cmd": "sheet_render", "sheet": 99999, "path": d.join("x.png")})).contains("no sheet"));
    let depth = e.undo_depth();
    ok(&mut e, json!({"cmd": "sheet_render", "sheet": sheet, "path": p, "px": 300}));
    assert_eq!(e.undo_depth(), depth);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn policy_lets_agents_draft_and_keeps_files_in_their_workspace() {
    let agent = Caller::Mcp(Workspace::at(std::env::temp_dir()));
    for c in ["dimension_auto", "layout_preset", "viewport_points", "sheet_render", "tag_style", "hatch_pattern", "sheets_info"] {
        assert!(permits(&agent, c), "{c}");
    }
    for c in ["dimension_auto", "layout_preset", "viewport_points"] {
        assert!(permits(&Caller::Plugin, c), "{c} is a model edit or read: plugins too");
    }
    assert!(!permits(&Caller::Plugin, "sheet_render"), "it writes a file");
    for c in ["sheet_issue", "font_add"] {
        assert!(!permits(&agent, c), "{c} is the person's");
    }
    // the picture goes only into the workspace, and only as a .png
    let inside = std::env::temp_dir().join("sc-ai-drafting-policy.png");
    assert!(vet(&agent, &mut json!({"cmd": "sheet_render", "sheet": 1, "path": inside})).is_ok());
    assert!(vet(&agent, &mut json!({"cmd": "sheet_render", "sheet": 1, "path": "/etc/sc.png"})).is_err());
    assert!(vet(&agent, &mut json!({"cmd": "sheet_render", "sheet": 1, "path": std::env::temp_dir().join("x.sh")})).is_err());
}

// ---- end to end: a real agent over MCP ------------------------------------------------------------------------

struct Mcp {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
    n: u64,
}

impl Mcp {
    fn start(dir: &std::path::Path, doc: &std::path::Path) -> Mcp {
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_sketchcraft"))
            .args(["mcp", "--doc"])
            .arg(doc)
            .env("HOME", &home)
            .env("XDG_CACHE_HOME", dir.join("cache"))
            .env("SKETCHCRAFT_DOC", doc)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut m = Mcp { child, out, n: 0 };
        let r = m.call("initialize", json!({"protocolVersion": "2025-06-18"}));
        assert_eq!(r["serverInfo"]["name"], "sketchcraft");
        m
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.n += 1;
        let msg = json!({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params});
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(v["id"], self.n, "{v}");
        v["result"].clone()
    }

    /// One command through `sketchcraft_run`: its reply (the line after the data note).
    fn run(&mut self, cmd: Value) -> Value {
        let r = self.call("tools/call", json!({"name": "sketchcraft_run", "arguments": {"commands": [cmd]}}));
        let text = r["content"][0]["text"].as_str().unwrap().to_string();
        let reply: Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        reply
    }

    fn ok(&mut self, cmd: Value) -> Value {
        let r = self.run(cmd.clone());
        assert_eq!(r["ok"], true, "{cmd} -> {r}");
        r
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

#[test]
fn an_agent_over_mcp_makes_a_dimensioned_hatched_plan_sheet_one_undo_per_step() {
    let dir = scratch("mcp");
    // the agent's workspace is the model's folder (HOME is beside it, never inside)
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let doc = work.join("model.sketchcraft");
    let mut a = Mcp::start(&dir, &doc);
    let tools = a.call("tools/list", json!({}));
    let help = tools["tools"][0]["description"].as_str().unwrap();
    assert!(help.contains("DRAFTING RECIPE") && help.contains("dimension_auto") && help.contains("layout_preset") && help.contains("tag_style {tag: \"Walls\", hatch"), "the help gives the recipe");

    // the agent builds a small house
    let w = a.ok(json!({"cmd": "wall", "points": [[0, 0, 0], [8, 0, 0], [8, 6, 0], [0, 6, 0]], "closed": true, "height": 2.7}))["group"].clone();
    let inner = a.ok(json!({"cmd": "wall", "points": [[4, 0.1, 0], [4, 5.9, 0]], "height": 2.7}))["group"].clone();
    a.ok(json!({"cmd": "insert", "asset": "door", "at": [1.0, -0.1, 0.0]}));
    a.ok(json!({"cmd": "insert", "asset": "window", "at": [5.5, -0.1, 0.9]}));

    // the drafting steps, each one command and one undo step; the shared file after each is what undo goes back to
    let file = |doc: &std::path::Path| -> Value { serde_json::from_slice(&std::fs::read(doc).unwrap()).unwrap() };
    let mut states = vec![file(&doc)];
    // 1. tag the walls
    a.ok(json!({"cmd": "tag", "ids": [w, inner], "tag": "Walls"}));
    states.push(file(&doc));
    // 2. lay out the set
    let set = a.ok(json!({"cmd": "layout_preset", "preset": "permit", "paper": "a2", "project": "Agent house"}));
    let plan_vp = set["plan_viewports"][0].clone();
    let plan_sheet = set["sheets"][1]["sheet"].clone();
    states.push(file(&doc));
    // 3. inspect: what the plan can be dimensioned to
    let pts = a.ok(json!({"cmd": "viewport_points", "viewport": plan_vp, "limit": 20}));
    assert!(!pts["points"].as_array().unwrap().is_empty());
    // 4. dimension the plan
    let d = a.ok(json!({"cmd": "dimension_auto", "viewport": plan_vp, "scope": ["exterior", "openings", "interior"]}));
    assert_eq!(d["added"].as_array().unwrap().len(), 5, "{d}");
    states.push(file(&doc));
    // 5. hatch by tag
    a.ok(json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick", "scale": 1.5}, "cut": {"fill": "hatch", "outline_mm": 0.5}}));
    states.push(file(&doc));
    // 6. look at it: the render tool shows the sheet
    let pic = a.call("tools/call", json!({"name": "sketchcraft_render", "arguments": {"sheet": plan_sheet, "px": 800}}));
    assert_eq!(pic["content"][0]["type"], "image", "{pic}");
    assert_eq!(pic["content"][0]["mimeType"], "image/png");
    let png = base64_decode(pic["content"][0]["data"].as_str().unwrap());
    let img = image::load_from_memory(&png).unwrap();
    assert_eq!(img.width(), 800);
    // and as a file in its workspace (the render writes nothing to the model: no undo step)
    let png_path = work.join("plan.png");
    a.ok(json!({"cmd": "sheet_render", "sheet": plan_sheet, "path": png_path}));
    assert!(std::fs::metadata(&png_path).unwrap().len() > 1000);
    // never outside it
    let r = a.run(json!({"cmd": "sheet_render", "sheet": plan_sheet, "path": "/private/tmp/sc-ai-drafting-outside.png"}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(!std::path::Path::new("/private/tmp/sc-ai-drafting-outside.png").exists());
    // a person-only command stays the person's
    assert_eq!(a.run(json!({"cmd": "plugins"}))["ok"], false);

    // 7. export the set
    let pdf_path = work.join("set.pdf");
    let x = a.ok(json!({"cmd": "export_sheets", "path": pdf_path}));
    assert_eq!(x["pages"], 4, "no stairs: cover, plan, elevations, schedule");
    let pdf = pdftext::parse(&std::fs::read(&pdf_path).unwrap());
    let pages = pdf.pages();
    let plan_text = pdf.text(pages[1]).join("\n");
    assert!(plan_text.contains("8.20 m"), "the overall width on the plan: {plan_text}");
    assert!(plan_text.contains("Agent house"));
    // the plan page is hatched (a hatched cut is clipped, W* n, then stroked)
    let svg_path = work.join("plan.svg");
    a.ok(json!({"cmd": "export_sheets", "path": svg_path, "pages": [plan_sheet]}));
    let svg = std::fs::read_to_string(&svg_path).unwrap();
    assert!(svg.contains("data-hatch=\"brick\""), "the walls are hatched");
    assert_eq!(states.last().unwrap(), &file(&doc), "rendering and exporting changed nothing");

    // what the agent made says so
    let info = a.ok(json!({"cmd": "sheets_info", "sheet": plan_sheet}));
    let items = info["sheets"][0]["items"].as_array().unwrap();
    assert!(items.iter().filter(|i| i["kind"] == "dimension").all(|i| i["made_by"] == "agent" && i["source"] == "auto" && i["measured"]["attached"] == true));

    // one undo per step, back through each state (ids are never handed out twice, so the id counter stays ahead)
    for (i, want) in states.iter().rev().skip(1).enumerate() {
        let r = a.ok(json!({"cmd": "undo"}));
        let got = file(&doc);
        let differ: Vec<String> = got.as_object().unwrap().iter().filter(|(k, v)| *k != "next_id" && want.get(k.as_str()) != Some(v)).map(|(k, v)| format!("{k}: {} vs {}", v.to_string().chars().take(300).collect::<String>(), want.get(k.as_str()).map(|w| w.to_string().chars().take(300).collect::<String>()).unwrap_or_default())).collect();
        assert!(differ.is_empty(), "undo {} -> {r}: {differ:?}", i + 1);
    }
    let last = file(&doc);
    assert!(last.get("sheets").is_none_or(|s| s.as_object().is_none_or(|o| o.is_empty())), "no sheets left");
    drop(a);
    std::fs::remove_dir_all(&dir).ok();
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}
