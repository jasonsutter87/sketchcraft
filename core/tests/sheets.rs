//! Drawing sheets saved in the model (#61, design step 1): the data model, file format 2, the commands, the policy
//! lists and limits, and per-item undo across another program's save.

use serde_json::{json, Value};
use sketchcraft::live::Live;
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;
use std::path::PathBuf;
use std::time::Duration;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-sheets-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn pause() {
    std::thread::sleep(Duration::from_millis(1100));
}

#[path = "pdftext/mod.rs"]
mod pdftext;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn live_ok(l: &mut Live, v: Value) -> Value {
    let r = l.exec(&v);
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn file(p: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}

/// A model with a box, a saved view, and two sheets with viewports.
fn with_sheets() -> Engine {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    ok(&mut e, json!({"cmd": "save_view", "name": "Corner", "eye": [10, -10, 8], "target": [2, 1.5, 0]}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Ground floor", "number": "A-101", "paper": "a3", "fields": {"project": "House"}}))["sheet"].clone();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "rect_mm": [15, 15, 250, 180], "label": "Plan"}));
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "Corner", "scale": 100, "rect_mm": [270, 15, 130, 100], "opacity": 0.4, "crop": [[0, 0], [130, 0], [130, 100]]}));
    ok(&mut e, json!({"cmd": "sheet_add", "name": "Elevations", "paper": {"w_mm": 600, "h_mm": 400, "name": "Custom"}}));
    e
}

fn sheet_items(e: &Engine) -> Value {
    serde_json::to_value(&e.model.sheet_items).unwrap()
}

// ---- round trip ------------------------------------------------------------------------------

#[test]
fn sheets_round_trip_through_save_and_open_as_format_2() {
    let dir = scratch("roundtrip");
    let mut e = with_sheets();
    let (a, b) = (dir.join("a.sketchcraft"), dir.join("b.sketchcraft"));
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f = file(&a);
    assert_eq!(f["format"], 2);
    assert_eq!(f["needs"], json!(["sheets"]));
    assert_eq!(f["sheets"].as_object().unwrap().len(), 2);
    assert_eq!(f["sheet_items"].as_object().unwrap().len(), 2);
    // viewports store their own anchor, scale, rotation and crop
    let vp = f["sheet_items"].as_object().unwrap().values().find(|v| v["opacity"] == 0.4).unwrap();
    assert_eq!(vp["kind"], "viewport");
    assert_eq!(vp["scale"], 100.0);
    assert_eq!(vp["crop"].as_array().unwrap().len(), 3);
    assert_eq!(vp["view"]["type"], "view3d");
    assert_eq!(vp["scene"], "Corner");
    assert!(vp["anchor"].is_array());

    let mut g = Engine::new();
    let r = ok(&mut g, json!({"cmd": "open", "path": a}));
    assert!(r["read_only"].is_null(), "this build reads its own format 2: {r}");
    assert!(g.read_only.is_none());
    assert_eq!(serde_json::to_value(&g.model.sheets).unwrap(), serde_json::to_value(&e.model.sheets).unwrap());
    assert_eq!(sheet_items(&g), sheet_items(&e));
    // and it can be edited and saved again, byte for byte when nothing changed
    ok(&mut g, json!({"cmd": "save", "path": b}));
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_viewport_is_never_re_fitted_when_the_model_changes() {
    let mut e = with_sheets();
    let before = sheet_items(&e);
    let tree = ok(&mut e, json!({"cmd": "rect", "from": [900, 900, 0], "to": [910, 910, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": tree, "distance": 40}));
    assert_eq!(sheet_items(&e), before, "a far tree moves nothing on paper");
}

#[test]
fn a_model_without_sheets_is_still_format_1_and_an_old_file_opens_unchanged() {
    let dir = scratch("old");
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f = file(&a);
    assert_eq!(f["format"], 1);
    for k in ["needs", "drafting", "sheets", "sheet_items", "title_blocks"] {
        assert!(f.get(k).is_none(), "{k} should not be written for a model without sheets");
    }
    // a 0.3.6-era file: no format, none of the new keys
    let mut old = f.clone();
    old.as_object_mut().unwrap().remove("format");
    let p = dir.join("old.sketchcraft");
    std::fs::write(&p, serde_json::to_vec(&old).unwrap()).unwrap();
    let mut g = Engine::new();
    let r = ok(&mut g, json!({"cmd": "open", "path": p}));
    assert!(r["read_only"].is_null());
    assert!(g.model.sheets.is_empty() && g.model.sheet_items.is_empty() && g.model.drafting.is_empty());
    let out = dir.join("out.sketchcraft");
    ok(&mut g, json!({"cmd": "save", "path": out}));
    // the same file, plus the format it always gets
    let mut w = file(&out);
    assert_eq!(w["format"], 1);
    w.as_object_mut().unwrap().remove("format");
    assert_eq!(w, old);
    // deleting every sheet takes the file back to format 1
    let mut h = with_sheets();
    let ids: Vec<Value> = h.model.sheets.keys().map(|k| json!(k)).collect();
    for s in ids {
        ok(&mut h, json!({"cmd": "sheet_delete", "sheet": s}));
    }
    let back = dir.join("back.sketchcraft");
    ok(&mut h, json!({"cmd": "save", "path": back}));
    assert_eq!(file(&back)["format"], 1);
    assert!(file(&back).get("needs").is_none());
    std::fs::remove_dir_all(&dir).ok();
}

// ---- the 0.3.7 guard -------------------------------------------------------------------------

/// What 0.3.7's guard did with a file's top level (format.rs at the 0.3.7 release): the format, the needs and the keys.
fn guard_0_3_7(bytes: &[u8]) -> Vec<String> {
    const FORMAT: u64 = 1;
    const KNOWN_NEEDS: &[&str] = &[];
    const KNOWN_KEYS: &[&str] = &[
        "format", "needs",
        "next_id", "vertices", "edges", "faces", "groups", "materials", "active_group", "tags", "environment", "tag_colors", "definitions",
        "instances", "guides", "photo_points", "cages", "photo_edges", "photo_guides", "views", "timeline", "annotations", "units", "classes",
        "location", "clouds", "sections", "active_section", "axes",
    ];
    let v: Value = serde_json::from_slice(bytes).unwrap();
    let o = v.as_object().unwrap();
    let mut why = vec![];
    match o.get("format") {
        None => {}
        Some(Value::Number(n)) if n.as_u64().is_some_and(|f| f <= FORMAT) => {}
        Some(Value::Number(n)) if n.as_u64().is_some() => why.push(format!("its format is {n}")),
        Some(_) => why.push("its format number isn't one this version knows".into()),
    }
    if let Some(Value::Array(a)) = o.get("needs") {
        for n in a {
            if !KNOWN_NEEDS.contains(&n.as_str().unwrap_or("")) {
                why.push(format!("it needs {n}"));
            }
        }
    }
    for k in o.keys() {
        if !KNOWN_KEYS.contains(&k.as_str()) {
            why.push(format!("it holds \"{k}\""));
        }
    }
    why
}

#[test]
fn a_format_2_file_is_refused_by_the_0_3_7_guard_and_a_plain_one_is_not() {
    let dir = scratch("guard");
    let mut e = with_sheets();
    let a = dir.join("sheets.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let why = guard_0_3_7(&std::fs::read(&a).unwrap());
    assert!(why.iter().any(|w| w.contains("format is 2")), "{why:?}");
    assert!(why.iter().any(|w| w.contains("needs \"sheets\"")), "{why:?}");
    assert!(why.iter().any(|w| w.contains("\"sheet_items\"")), "{why:?}");
    // ...for each reason alone, too: strip the format and the needs, the unknown keys still refuse it
    let mut f = file(&a);
    f.as_object_mut().unwrap().remove("format");
    f.as_object_mut().unwrap().remove("needs");
    assert!(!guard_0_3_7(&serde_json::to_vec(&f).unwrap()).is_empty());
    // a model without sheets is a file 0.3.7 opens for editing, as before
    let mut plain = Engine::new();
    ok(&mut plain, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let b = dir.join("plain.sketchcraft");
    ok(&mut plain, json!({"cmd": "save", "path": b}));
    assert_eq!(guard_0_3_7(&std::fs::read(&b).unwrap()), Vec::<String>::new());
    // and this build's own guard, read on the same bytes, has no complaint
    assert!(sketchcraft::format::check(&std::fs::read(&a).unwrap()).is_none());
    assert_eq!(sketchcraft::format::FORMAT, 2);
    assert!(sketchcraft::format::KNOWN_NEEDS.contains(&"sheets"));
    std::fs::remove_dir_all(&dir).ok();
}

// ---- unknown things survive ------------------------------------------------------------------

#[test]
fn unknown_item_kinds_and_fields_survive_a_round_trip() {
    let dir = scratch("unknown");
    let mut e = with_sheets();
    let sheet = *e.model.sheets.keys().next().unwrap();
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    // a newer build's file: an item kind we don't know, a known kind with a field we don't know, and extras on a
    // sheet and on the drafting styles (written as format 2, which this build opens)
    let mut f = file(&a);
    let next = f["next_id"].as_u64().unwrap();
    let ghost = json!({"kind": "leader_cloud", "sheet": sheet, "z": 3, "pts": [[0.5, 1.5], [2, 3]], "style": {"fancy": true}});
    f["sheet_items"][(next + 1).to_string()] = ghost.clone();
    let known = json!({"kind": "text", "sheet": sheet, "at_mm": [10.0, 20.0], "paragraphs": ["Hello"], "spell_checked": true});
    f["sheet_items"][(next + 2).to_string()] = known;
    f["sheets"][sheet.to_string()]["revision_cloud"] = json!({"count": 2});
    f["drafting"] = json!({"tag_styles": {"Walls": {"weight_mm": 0.5, "cut": {"fill": "solid"}, "future_field": 1}}, "marker_styles": {"x": 1}});
    f["next_id"] = json!(next + 2);
    let p = dir.join("newer.sketchcraft");
    std::fs::write(&p, serde_json::to_vec(&f).unwrap()).unwrap();

    let mut g = Engine::new();
    let r = ok(&mut g, json!({"cmd": "open", "path": p}));
    assert!(r["read_only"].is_null(), "{r}");
    // the unknown kind is held as raw JSON, and listed
    let info = ok(&mut g, json!({"cmd": "sheets_info"}));
    assert_eq!(info["unsupported"].as_array().unwrap().len(), 1);
    assert_eq!(info["unsupported"][0]["kind"], "leader_cloud");
    // editing something else and saving keeps all of it
    ok(&mut g, json!({"cmd": "sheet_edit", "sheet": sheet, "name": "Renamed"}));
    let out = dir.join("out.sketchcraft");
    ok(&mut g, json!({"cmd": "save", "path": out}));
    let w = file(&out);
    assert_eq!(w["sheet_items"][(next + 1).to_string()], ghost);
    assert_eq!(w["sheet_items"][(next + 2).to_string()]["spell_checked"], true);
    assert_eq!(w["sheet_items"][(next + 2).to_string()]["paragraphs"], json!(["Hello"]));
    assert_eq!(w["sheets"][sheet.to_string()]["revision_cloud"], json!({"count": 2}));
    assert_eq!(w["sheets"][sheet.to_string()]["name"], "Renamed");
    assert_eq!(w["drafting"]["tag_styles"]["Walls"]["future_field"], 1);
    assert_eq!(w["drafting"]["marker_styles"], json!({"x": 1}));
    // an unknown item can't be edited as a viewport, but the person may delete it
    let bad = g.exec_json(&json!({"cmd": "viewport_edit", "id": next + 1, "scale": 50}).to_string());
    assert_eq!(bad["ok"], false, "{bad}");
    ok(&mut g, json!({"cmd": "sheet_item_delete", "ids": [next + 1]}));
    std::fs::remove_dir_all(&dir).ok();
}

// ---- commands --------------------------------------------------------------------------------

#[test]
fn sheet_commands_make_one_undo_step_each() {
    let mut e = with_sheets();
    let s = *e.model.sheets.keys().next().unwrap();
    let n = e.model.sheet_items.len();
    let vp = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "iso", "scale": 20}))["viewport"].as_u64().unwrap();
    assert_eq!(e.model.sheet_items.len(), n + 1);
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "scale": "1:25", "rotation": 90, "clear_crop": true}));
    let after = serde_json::to_value(&e.model.sheet_items[&(vp as u32)]).unwrap();
    assert_eq!(after["scale"], 25.0);
    assert_eq!(after["rotation"], 90.0);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(serde_json::to_value(&e.model.sheet_items[&(vp as u32)]).unwrap()["scale"], 20.0);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), n);
    // deleting a sheet takes its items, and undo brings them all back
    let items = e.model.sheet_items.len();
    let r = ok(&mut e, json!({"cmd": "sheet_delete", "sheet": s}));
    assert_eq!(r["deleted"], items);
    assert!(e.model.sheet_items.is_empty());
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), items);
    // bad input changes nothing
    for bad in [
        json!({"cmd": "viewport_add", "sheet": 9999}),
        json!({"cmd": "viewport_add", "sheet": s, "scale": -5}),
        json!({"cmd": "viewport_add", "sheet": s, "rect_mm": [0, 0, 0, 10]}),
        json!({"cmd": "sheet_add", "paper": "a9"}),
        json!({"cmd": "sheet_edit", "sheet": 9999, "name": "x"}),
        json!({"cmd": "sheet_item_delete", "ids": [9999]}),
        json!({"cmd": "viewport_add", "viewports": [{"sheet": s}, {"sheet": 9999}]}),
    ] {
        let depth = e.undo_depth();
        let r = e.exec_json(&bad.to_string());
        assert_eq!(r["ok"], false, "{bad} -> {r}");
        assert_eq!(e.undo_depth(), depth);
    }
    assert_eq!(e.model.sheet_items.len(), items);
}

#[test]
fn a_batch_of_viewports_is_one_step() {
    let mut e = with_sheets();
    let s = *e.model.sheets.keys().next().unwrap();
    let n = e.model.sheet_items.len();
    let r = ok(&mut e, json!({"cmd": "viewport_add", "viewports": [{"sheet": s, "from_view": "top", "scale": 100}, {"sheet": s, "from_view": "front", "scale": 100}, {"sheet": s, "view": {"type": "elevation", "dir": [0, -1, 0]}}]}));
    assert_eq!(r["viewports"].as_array().unwrap().len(), 3);
    assert_eq!(e.model.sheet_items.len(), n + 3);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), n);
}

#[test]
fn sheets_creates_saved_sheets_and_export_sheets_writes_the_pdf() {
    let dir = scratch("export");
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let r = ok(&mut e, json!({"cmd": "sheets", "sheet": "a4", "project": "Box", "notes": ["# Notes", "one"], "pages": [
        {"title": "Plan", "viewports": [{"view": "top", "scale": "1:50"}]},
        {"title": "Views"},
        {"title": "Schedule", "table": [["Mark", "Qty"], ["D1", "2"]]}
    ]}));
    assert_eq!(r["sheets"].as_array().unwrap().len(), 3);
    assert_eq!(e.model.sheets.len(), 3);
    // one step: undo takes all three pages
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(e.model.sheets.is_empty() && e.model.sheet_items.is_empty());
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(e.model.sheets.len(), 3);
    // the old one-shot "path" is refused with where to go instead
    let p = dir.join("set.pdf");
    let r = e.exec_json(&json!({"cmd": "sheets", "path": p}).to_string());
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("export_sheets"), "{r}");
    assert!(!p.exists());
    // the PDF is written from the saved sheets (and saves nothing in the model)
    let revision = e.revision();
    let r = ok(&mut e, json!({"cmd": "export_sheets", "path": p}));
    assert_eq!(r["pages"], 3);
    assert_eq!(e.revision(), revision);
    let pdf = pdftext::parse(&std::fs::read(&p).unwrap());
    let text = pdf.pages().iter().flat_map(|pg| pdf.text(*pg)).collect::<Vec<_>>().join("\n");
    assert!(pdf.pages().len() == 3 && text.contains("Sheet 1 of 3") && text.contains("1:50"), "{text}");
    // a subset, by id, in the order asked
    let ids: Vec<u32> = e.model.sheets.keys().copied().collect();
    let q = dir.join("two.pdf");
    let r = ok(&mut e, json!({"cmd": "export_sheets", "path": q, "pages": [ids[1], ids[0]]}));
    assert_eq!(r["pages"], 2);
    // sheets_info reads them back
    let info = ok(&mut e, json!({"cmd": "sheets_info"}));
    assert_eq!(info["sheets"].as_array().unwrap().len(), 3);
    assert_eq!(info["sheets"][0]["name"], "Plan");
    assert_eq!(info["sheets"][0]["fields"]["project"], "Box");
    assert_eq!(info["sheets"][0]["items"][0]["kind"], "viewport");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn edits_made_after_sheets_are_what_the_pdf_draws() {
    // review findings: a scale set by viewport_edit wins over a fitted view; an empty saved sheet exports empty; a standard
    // view clears the scene; the elevation dir is the viewer's side
    let dir = scratch("edits");
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    ok(&mut e, json!({"cmd": "save_view", "name": "Corner", "eye": [10, -10, 8], "target": [2, 1.5, 0]}));
    let r = ok(&mut e, json!({"cmd": "sheets", "pages": [{"title": "A", "viewports": [{"view": "iso"}]}, {"title": "Front", "viewports": [{"view": "front", "scale": "1:50", "callouts": true}]}]}));
    let (first, second) = (r["sheets"][0].as_u64().unwrap(), r["sheets"][1].as_u64().unwrap());
    let vp = *e.model.sheet_items.iter().find(|(_, i)| i.sheet() == Some(first as u32)).unwrap().0;
    // saved fitted: the PDF has no scale label; after the edit it says 1:50
    let p1 = dir.join("fit.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p1, "pages": [first]}));
    assert!(!String::from_utf8_lossy(&std::fs::read(&p1).unwrap()).contains("1:50"));
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "scale": 50}));
    let p2 = dir.join("edited.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p2, "pages": [first]}));
    assert!(String::from_utf8_lossy(&std::fs::read(&p2).unwrap()).contains("1:50"), "the edited scale is drawn");
    // a standard view clears the scene a saved one set
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "from_view": "Corner"}));
    assert_eq!(serde_json::to_value(&e.model.sheet_items[&(vp as u32)]).unwrap()["scene"], "Corner");
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "from_view": "iso"}));
    assert!(serde_json::to_value(&e.model.sheet_items[&(vp as u32)]).unwrap().get("scene").is_none());
    // an elevation's dir is from the model toward the viewer: the front is [0,-1,0]
    let el = e.model.sheet_items.iter().find(|(_, i)| i.sheet() == Some(second as u32)).map(|(_, i)| serde_json::to_value(&**i).unwrap()).unwrap();
    assert_eq!(el["view"]["type"], "elevation");
    assert_eq!(el["view"]["dir"], json!([0.0, -1.0, 0.0]));
    // an empty saved sheet is drawn empty: no views nobody placed
    let empty = ok(&mut e, json!({"cmd": "sheet_add", "name": "Empty"}))["sheet"].as_u64().unwrap();
    let p3 = dir.join("empty.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p3, "pages": [empty]}));
    let with = dir.join("one.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": with, "pages": [first]}));
    let figures = |f: &std::path::Path| pdftext::parse(&std::fs::read(f).unwrap()).objs.values().filter(|o| o.dict.contains("/S /Figure")).count();
    assert_eq!((figures(&p3), figures(&with)), (0, 1), "an empty sheet has no drawing in it, a sheet with a viewport has one");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn newer_values_inside_known_things_are_kept_and_never_stop_the_file_opening() {
    // review findings: unknown fields inside a viewport's view and an anchor, and unknown values in drafting styles
    let dir = scratch("fwd");
    let mut e = with_sheets();
    let sheet = *e.model.sheets.keys().next().unwrap();
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let mut f = file(&a);
    let next = f["next_id"].as_u64().unwrap();
    let plan = f["sheet_items"].as_object().unwrap().iter().find(|(_, v)| v["view"]["type"] == "plan").map(|(k, _)| k.clone()).unwrap();
    f["sheet_items"][&plan]["view"]["show_below"] = json!(true);
    let dim = json!({"kind": "dimension", "sheet": sheet, "anchors": [{"at": {"between": {"a": 1, "b": 2, "t": 0.5, "snap": "mid"}}, "last": [0, 0, 0], "stale": false}], "offset_mm": 8.0});
    f["sheet_items"][(next + 1).to_string()] = dim.clone();
    f["drafting"] = json!({
        "tag_styles": {"Walls": {"weight_mm": 0.5, "cut": {"fill": "gradient", "stops": 3}}, "Doors": {"weight_mm": 0.25}},
        "line_types": {"dash": {"dash_mm": "long-short"}},
    });
    f["next_id"] = json!(next + 1);
    let p = dir.join("newer.sketchcraft");
    std::fs::write(&p, serde_json::to_vec(&f).unwrap()).unwrap();
    let mut g = Engine::new();
    let r = ok(&mut g, json!({"cmd": "open", "path": p}));
    assert!(r["read_only"].is_null(), "{r}");
    ok(&mut g, json!({"cmd": "sheet_edit", "sheet": sheet, "name": "Renamed"}));
    let out = dir.join("out.sketchcraft");
    ok(&mut g, json!({"cmd": "save", "path": out}));
    let w = file(&out);
    assert_eq!(w["sheet_items"][&plan]["view"]["show_below"], true);
    // (a known kind is written with its defaults filled in; what this build doesn't know stays)
    let kept = &w["sheet_items"][(next + 1).to_string()]["anchors"][0];
    assert_eq!(kept["at"]["between"]["snap"], "mid");
    assert_eq!(kept["at"]["between"]["t"], 0.5);
    assert_eq!(kept["stale"], false);
    let _ = dim;
    assert_eq!(w["drafting"]["tag_styles"]["Walls"], f["drafting"]["tag_styles"]["Walls"], "a style with a value this build has no name for is written back as it was");
    assert_eq!(w["drafting"]["line_types"], f["drafting"]["line_types"]);
    assert_eq!(w["drafting"]["tag_styles"]["Doors"]["weight_mm"], 0.25);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_viewport_added_with_no_view_always_draws() {
    let dir = scratch("default-view");
    // a flat model (nothing 1.2 m high to cut), a low one, a tall one, and an empty one
    for (what, h) in [("flat", None), ("low", Some(0.5)), ("tall", Some(3.0)), ("empty", None)] {
        let mut e = Engine::new();
        if what != "empty" {
            let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
            if let Some(h) = h {
                ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": h}));
            }
        }
        let s = ok(&mut e, json!({"cmd": "sheet_add"}))["sheet"].clone();
        ok(&mut e, json!({"cmd": "viewport_add", "sheet": s}));
        let p = dir.join(format!("{what}.pdf"));
        let r = e.exec_json(&json!({"cmd": "export_sheets", "path": p}).to_string());
        assert_eq!(r["ok"], true, "{what}: {r}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unscaled_3d_viewport_is_exported_fitted_not_as_1_to_100() {
    let dir = scratch("perspective");
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let s = ok(&mut e, json!({"cmd": "sheet_add"}))["sheet"].clone();
    let vp = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "iso"}))["viewport"].clone();
    let p = dir.join("persp.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p}));
    assert!(!String::from_utf8_lossy(&std::fs::read(&p).unwrap()).contains("1:100"), "a perspective view has no scale");
    // give it a scale and it is a parallel view to that scale
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "scale": 100}));
    let item = serde_json::to_value(&e.model.sheet_items[&(vp.as_u64().unwrap() as u32)]).unwrap();
    assert_eq!(item["view"]["parallel"], true);
    let q = dir.join("scaled.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": q}));
    assert!(String::from_utf8_lossy(&std::fs::read(&q).unwrap()).contains("1:100"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_model_file_with_sheets_placed_as_a_part_brings_no_paper_along() {
    let dir = scratch("part");
    let mut e = with_sheets();
    let part = dir.join("part.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": part}));
    assert_eq!(file(&part)["format"], 2);
    let m = sketchcraft::library::load_file(part.to_str().unwrap()).unwrap();
    assert!(!m.has_paper(), "the part has no sheets, items, title blocks or drafting styles");
    let mut root = Engine::new();
    sketchcraft::library::adopt(&mut root.model, m, "Part", None);
    assert!(!root.model.has_paper());
    let out = dir.join("out.sketchcraft");
    ok(&mut root, json!({"cmd": "save", "path": out}));
    let f = file(&out);
    assert_eq!(f["format"], 1, "nothing in it needs sheets");
    assert!(!f.to_string().contains("sheet_items"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn changing_a_viewports_view_drops_the_old_style_options() {
    let dir = scratch("legacy");
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    // made by `sheets` with no scale (so "fit the picture" is remembered), then turned into an elevation
    ok(&mut e, json!({"cmd": "sheets", "pages": [{"title": "A", "viewports": [{"view": "iso", "callouts": true, "marks": ["1/2"]}]}]}));
    let vp = *e.model.sheet_items.keys().next().unwrap();
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "view": {"type": "elevation", "dir": [0, -1, 0]}}));
    let item = serde_json::to_value(&*e.model.sheet_items[&vp]).unwrap();
    assert!(item.get("legacy").is_none());
    let scale = item["scale"].as_f64().unwrap() as u64;
    let p = dir.join("e.pdf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p}));
    assert!(String::from_utf8_lossy(&std::fs::read(&p).unwrap()).contains(&format!("1:{scale}")), "the saved scale is what is drawn");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_refuses_papers_the_writer_would_print_wrongly() {
    let dir = scratch("paper");
    let mut e = with_sheets(); // an a3 sheet and a custom 600 x 400 one
    let ids: Vec<u32> = e.model.sheets.keys().copied().collect();
    // (the model is flat, so give the plan something the PDF can draw)
    let plans: Vec<u32> = e.model.sheet_items.iter().filter(|(_, i)| serde_json::to_value(&***i).unwrap()["view"]["type"] == "plan").map(|(k, _)| *k).collect();
    for v in plans {
        ok(&mut e, json!({"cmd": "viewport_edit", "id": v, "from_view": "iso"}));
    }
    let p = dir.join("x.pdf");
    // the custom one alone: named, and told how to go on
    let r = e.exec_json(&json!({"cmd": "export_sheets", "path": p, "pages": [ids[1]]}).to_string());
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains(&format!("sheet {}", ids[1])) && r["error"].as_str().unwrap().contains("600"), "{r}");
    // a mixed set: both named
    let r = e.exec_json(&json!({"cmd": "export_sheets", "path": p, "pages": [ids[0], ids[1]]}).to_string());
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("different papers"), "{r}");
    // forcing a paper is allowed; the a3 sheet alone exports
    ok(&mut e, json!({"cmd": "export_sheets", "path": p, "pages": [ids[0], ids[1]], "sheet": "a2"}));
    ok(&mut e, json!({"cmd": "export_sheets", "path": dir.join("y.pdf"), "pages": [ids[0]]}));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn more_sheets_than_one_pdf_holds_say_how_to_export_them() {
    let dir = scratch("many");
    let mut e = Engine::new();
    // one PDF holds as many pages as a model may have sheets (200, the person's limit; it was 50): 60 export whole
    ok(&mut e, json!({"cmd": "sheets", "pages": (0..60).map(|i| json!({"title": format!("S{i}"), "table": [["a"]]})).collect::<Vec<_>>()}));
    let p = dir.join("all.pdf");
    let r = ok(&mut e, json!({"cmd": "export_sheets", "path": p}));
    assert_eq!(r["pages"], 60, "{r}");
    let ids: Vec<u32> = e.model.sheets.keys().copied().take(50).collect();
    let r = e.exec_json(&json!({"cmd": "export_sheets", "path": p, "pages": ids}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    // past that, the set is refused, saying how to export it in parts
    assert_eq!(sketchcraft::draft::sheet::MAX_PAGES, sketchcraft::policy::MAX_PAGES_UI);
    let many: Vec<sketchcraft::layout::PageSpec> = (0..=sketchcraft::draft::sheet::MAX_PAGES).map(|i| sketchcraft::layout::PageSpec { title: format!("S{i}"), table: vec![vec!["a".into()]], ..Default::default() }).collect();
    let err = sketchcraft::draft::sheet::build(&e.model, "a3", "P", many, false, &[], "today").unwrap_err().to_string();
    assert!(err.contains("pages: [sheet ids]"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sheets_info_is_read_only() {
    let mut e = with_sheets();
    let (rev, depth) = (e.revision(), e.undo_depth());
    let r = ok(&mut e, json!({"cmd": "sheets_info"}));
    assert_eq!(r["items"], 2);
    let first = *e.model.sheets.keys().next().unwrap();
    let one = ok(&mut e, json!({"cmd": "sheets_info", "sheet": first}));
    assert_eq!(one["sheets"].as_array().unwrap().len(), 1);
    assert_eq!((e.revision(), e.undo_depth()), (rev, depth));
    // and it works on a model that is read-only (a newer file)
    let dir = scratch("info-ro");
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let mut f = file(&a);
    f["format"] = json!(3);
    std::fs::write(&a, serde_json::to_vec(&f).unwrap()).unwrap();
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": a}));
    assert!(g.read_only.is_some());
    assert_eq!(ok(&mut g, json!({"cmd": "sheets_info"}))["sheets"].as_array().unwrap().len(), 2);
    assert_eq!(g.exec_json(&json!({"cmd": "sheet_add"}).to_string())["ok"], false);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- policy ----------------------------------------------------------------------------------

#[test]
fn the_sheet_commands_are_on_the_right_allowlists() {
    let dir = scratch("policy");
    let ws = Workspace::at(&dir);
    let agent = Caller::Mcp(ws);
    let plugin = Caller::Plugin;
    let ui = Caller::Ui;
    // model edits and reads: plugins and agents
    for c in ["sheet_add", "sheet_edit", "sheet_delete", "viewport_add", "viewport_edit", "sheet_item_delete", "sheets_info", "sheets"] {
        assert!(permits(&agent, c) && permits(&plugin, c) && permits(&ui, c), "{c}");
    }
    // the PDF writer is a file command: agents inside their workspace, never plugins
    assert!(permits(&agent, "export_sheets") && permits(&ui, "export_sheets") && !permits(&plugin, "export_sheets"));
    // only the person: the issue command and fonts (not built yet, but never an agent's when they are)
    for c in ["sheet_issue", "font_add", "made_up_sheet_command"] {
        assert!(!permits(&agent, c) && !permits(&plugin, c), "{c}");
    }
    // an agent's PDF goes in its workspace, to a new .pdf name
    let mut good = json!({"cmd": "export_sheets", "path": "set.pdf"});
    assert!(vet(&agent, &mut good).is_ok());
    assert!(good["path"].as_str().unwrap().contains(dir.file_name().unwrap().to_str().unwrap()));
    for bad in [json!({"cmd": "export_sheets", "path": "set.sh"}), json!({"cmd": "export_sheets", "path": "../set.pdf"}), json!({"cmd": "export_sheets", "path": "/etc/set.pdf"}), json!({"cmd": "export_sheets", "path": "http://x/set.pdf"})] {
        assert!(vet(&agent, &mut bad.clone()).is_err(), "{bad}");
    }
    // a plugin may not pass a path at all
    assert!(vet(&plugin, &mut json!({"cmd": "export_sheets", "path": "x.pdf"})).is_err());
    // an agent may not replace a PDF it didn't make
    std::fs::write(dir.join("mine.pdf"), b"%PDF").unwrap();
    assert!(vet(&agent, &mut json!({"cmd": "export_sheets", "path": "mine.pdf"})).is_err());
    // `sheets` no longer takes a path to write: it is a model edit (the engine says where to go)
    let mut e = Engine::new();
    let r = e.exec_value_as(&agent, json!({"cmd": "sheets", "path": "x.pdf"}));
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("export_sheets"), "{r}");
    // an agent's sheet commands run as model edits, and mark what they make
    let s = e.exec_value_as(&agent, json!({"cmd": "sheet_add", "name": "By agent"}));
    assert_eq!(s["ok"], true, "{s}");
    let v = e.exec_value_as(&agent, json!({"cmd": "viewport_add", "sheet": s["sheet"], "scale": 50}));
    assert_eq!(v["ok"], true, "{v}");
    let item = serde_json::to_value(&e.model.sheet_items[&(v["viewport"].as_u64().unwrap() as u32)]).unwrap();
    assert_eq!(item["made_by"], "agent");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- limits ----------------------------------------------------------------------------------

#[test]
fn page_and_item_limits_in_vet() {
    let agent = Caller::Mcp(Workspace::anywhere());
    let ui = Caller::Ui;
    let pages = |n: usize| json!({"cmd": "sheets", "pages": (0..n).map(|i| json!({"title": format!("P{i}")})).collect::<Vec<_>>()});
    // pages: person 200, agent 120
    assert!(vet(&agent, &mut pages(120)).is_ok());
    let e = vet(&agent, &mut pages(121)).unwrap_err();
    assert!(e.contains("too many pages") && e.contains("120"), "{e}");
    assert!(vet(&ui, &mut pages(200)).is_ok());
    assert!(vet(&ui, &mut pages(201)).unwrap_err().contains("200"));
    assert!(vet(&Caller::Plugin, &mut pages(121)).is_err());
    // items per command: agent 5,000 (viewports added, ids deleted, viewports on described pages)
    let vps = |n: usize| json!({"cmd": "viewport_add", "viewports": (0..n).map(|_| json!({"sheet": 1})).collect::<Vec<_>>()});
    assert!(vet(&agent, &mut vps(5000)).is_ok());
    let e = vet(&agent, &mut vps(5001)).unwrap_err();
    assert!(e.contains("too many sheet items") && e.contains("5000"), "{e}");
    assert!(vet(&ui, &mut vps(5001)).is_ok(), "the person has no per-command item cap");
    let ids = |n: usize| json!({"cmd": "sheet_item_delete", "ids": (1..=n).collect::<Vec<_>>()});
    assert!(vet(&agent, &mut ids(5000)).is_ok());
    assert!(vet(&agent, &mut ids(5001)).is_err());
    let page_vps = |n: usize| json!({"cmd": "sheets", "pages": [{"title": "x", "viewports": (0..n).map(|_| json!({"view": "top"})).collect::<Vec<_>>()}]});
    assert!(vet(&agent, &mut page_vps(4998)).is_ok());
    assert!(vet(&agent, &mut page_vps(5000)).is_err());
}

#[test]
fn the_model_holds_at_most_the_sheet_limit_for_the_caller() {
    let agent = Caller::Mcp(Workspace::anywhere());
    let mut e = Engine::new();
    for i in 0..120 {
        let r = e.exec_value_as(&agent, json!({"cmd": "sheet_add", "name": format!("S{i}")}));
        assert_eq!(r["ok"], true, "{i}: {r}");
    }
    let r = e.exec_value_as(&agent, json!({"cmd": "sheet_add"}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("120"), "{r}");
    // `sheets` counts the sheets already there
    let r = e.exec_value_as(&agent, json!({"cmd": "sheets", "pages": [{"title": "one more"}]}));
    assert_eq!(r["ok"], false, "{r}");
    // the person's limit is higher
    assert_eq!(e.exec_json(&json!({"cmd": "sheet_add"}).to_string())["ok"], true);
    assert_eq!(e.model.sheets.len(), 121);
}

// ---- broken references and ids ---------------------------------------------------------------

#[test]
fn broken_refs_and_repair_ids_know_about_sheets() {
    let e = with_sheets();
    assert!(e.model.broken_refs(true).is_empty(), "{:?}", e.model.broken_refs(true));
    let s = *e.model.sheets.keys().next().unwrap();
    let item = *e.model.sheet_items.keys().next().unwrap();

    // an item on a sheet that isn't there; a sheet with a title block that isn't there
    let mut m = e.model.clone();
    m.sheets.remove(&s);
    let broken = m.broken_refs(true);
    assert!(broken.contains(&format!("sheet item {item}")), "{broken:?}");
    assert!(m.broken_refs(false).is_empty(), "only the full check looks at paper (nothing here can panic a draw)");
    let mut m = e.model.clone();
    m.sheets.get_mut(&s).unwrap().title_block = Some(777);
    assert!(m.broken_refs(true).contains(&format!("sheet {s} (title block)")));
    let mut m = e.model.clone();
    m.sheet_items.insert(
        900,
        std::sync::Arc::new(serde_json::from_value(json!({"kind": "title_block_ref", "sheet": s, "title_block": 55})).unwrap()),
    );
    assert!(m.broken_refs(true).contains(&"sheet item 900 (title block)".to_string()));
    // a raw item (unknown kind) with no sheet at all
    let mut m = e.model.clone();
    m.sheet_items.insert(901, std::sync::Arc::new(serde_json::from_value(json!({"kind": "future_thing"})).unwrap()));
    assert!(m.broken_refs(true).contains(&"sheet item 901".to_string()));

    // repair_ids: a counter lower than any id in use is raised past sheets, items and title blocks
    let mut m = e.model.clone();
    let top = *m.sheet_items.keys().max().unwrap();
    m.next_id = 1;
    assert!(m.repair_ids());
    assert!(m.next_id >= top, "{} < {top}", m.next_id);
    let mut m = e.model.clone();
    m.sheets.insert(5000, Default::default());
    m.title_blocks.insert(6000, Default::default());
    m.sheet_items.insert(7000, std::sync::Arc::new(serde_json::from_value(json!({"kind": "text", "sheet": 5000})).unwrap()));
    m.next_id = 10;
    assert!(m.repair_ids());
    assert_eq!(m.next_id, 7000);
    // a new thing never takes an id in use
    let mut g = Engine::new();
    g.model = m;
    let r = ok(&mut g, json!({"cmd": "sheet_add"}));
    assert!(r["sheet"].as_u64().unwrap() > 7000);
}

#[test]
fn undoing_a_sheet_is_refused_while_another_program_has_put_something_on_it() {
    let dir = scratch("depends");
    let doc = dir.join("live.sketchcraft");
    let mut a = Live::open(doc.clone());
    let s = live_ok(&mut a, json!({"cmd": "sheet_add", "name": "A-101"}))["sheet"].clone();
    pause();
    let mut b = Live::open(doc.clone());
    live_ok(&mut b, json!({"cmd": "viewport_add", "sheet": s, "scale": 50}));
    pause();
    a.sync();
    let r = a.exec(&json!({"cmd": "undo"}));
    assert_eq!(r["ok"], false, "the sheet now has B's viewport on it: {r}");
    assert!(r["error"].as_str().unwrap().contains("sheet item"), "{r}");
    assert_eq!(file(&doc)["sheets"].as_object().unwrap().len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- undo across another program's save ------------------------------------------------------

fn items_on_disk(doc: &std::path::Path) -> Value {
    file(doc).get("sheet_items").cloned().unwrap_or(json!({}))
}

fn scales_on_disk(doc: &std::path::Path) -> Vec<i64> {
    let mut v: Vec<i64> = items_on_disk(doc).as_object().unwrap().values().map(|i| i["scale"].as_f64().unwrap() as i64).collect();
    v.sort();
    v
}

#[test]
fn per_item_undo_works_across_another_programs_save() {
    let dir = scratch("undo");
    let doc = dir.join("live.sketchcraft");
    let mut a = Live::open(doc.clone());
    let s = live_ok(&mut a, json!({"cmd": "sheet_add", "name": "A-101"}))["sheet"].clone();
    let v1 = live_ok(&mut a, json!({"cmd": "viewport_add", "sheet": s, "scale": 50}))["viewport"].clone();
    pause();
    // B, another program, opens the file and edits a different item on the same sheet, then saves
    let mut b = Live::open(doc.clone());
    let v2 = live_ok(&mut b, json!({"cmd": "viewport_add", "sheet": s, "scale": 200}))["viewport"].clone();
    pause();
    // A picks up B's save, then adds a third viewport
    let v3 = live_ok(&mut a, json!({"cmd": "viewport_add", "sheet": s, "scale": 20}))["viewport"].clone();
    assert_eq!(scales_on_disk(&doc), vec![20, 50, 200]);
    pause();
    // A undoes its own last step: only v3 goes; B's v2 stays
    live_ok(&mut a, json!({"cmd": "undo"}));
    assert_eq!(scales_on_disk(&doc), vec![50, 200], "A's v3 went, B's v2 stayed");
    pause();
    // B edits v1 (A's item) and saves; A's undo of the add of v1 is now refused: it was changed since
    live_ok(&mut b, json!({"cmd": "viewport_edit", "id": v1, "scale": 75}));
    pause();
    a.sync();
    let r = a.exec(&json!({"cmd": "undo"}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("changed"), "{r}");
    assert_eq!(scales_on_disk(&doc), vec![75, 200], "nothing of B's was lost");
    pause();
    // B takes its edit back; now A's undo works and removes v1, leaving B's v2
    live_ok(&mut b, json!({"cmd": "undo"}));
    pause();
    a.sync();
    live_ok(&mut a, json!({"cmd": "undo"}));
    assert_eq!(scales_on_disk(&doc), vec![200]);
    let _ = (v2, v3);
    // and redo puts A's viewport back, again touching nothing else
    pause();
    live_ok(&mut a, json!({"cmd": "redo"}));
    assert_eq!(scales_on_disk(&doc), vec![50, 200]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sheet_deletion_undoes_per_item_across_a_reload() {
    let dir = scratch("undo-delete");
    let doc = dir.join("live.sketchcraft");
    let mut a = Live::open(doc.clone());
    let s = live_ok(&mut a, json!({"cmd": "sheet_add", "name": "A-101"}))["sheet"].clone();
    live_ok(&mut a, json!({"cmd": "viewport_add", "sheet": s, "scale": 50}));
    live_ok(&mut a, json!({"cmd": "viewport_add", "sheet": s, "scale": 100}));
    pause();
    // another program adds an unrelated sheet and saves
    let mut b = Live::open(doc.clone());
    live_ok(&mut b, json!({"cmd": "sheet_add", "name": "B-201"}));
    pause();
    live_ok(&mut a, json!({"cmd": "sheet_delete", "sheet": s}));
    assert!(items_on_disk(&doc).as_object().unwrap().is_empty());
    pause();
    let mut c = Live::open(doc.clone());
    live_ok(&mut c, json!({"cmd": "sheet_add", "name": "C-301"}));
    pause();
    // A undoes the deletion: its sheet and both viewports return, next to B's and C's sheets
    a.sync();
    live_ok(&mut a, json!({"cmd": "undo"}));
    let f = file(&doc);
    assert_eq!(f["sheets"].as_object().unwrap().len(), 3);
    assert_eq!(scales_on_disk(&doc), vec![50, 100]);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- sanity ----------------------------------------------------------------------------------

#[test]
fn paper_numbers_must_be_finite() {
    let mut e = with_sheets();
    let item = *e.model.sheet_items.keys().next().unwrap();
    assert!(e.model.check_sane().is_ok());
    let mut bad: Value = serde_json::to_value(&*e.model.sheet_items[&item]).unwrap();
    bad["rect_mm"] = json!([0.0, 0.0, 1.0e12, 10.0]);
    e.model.sheet_items.insert(item, std::sync::Arc::new(serde_json::from_value(bad).unwrap()));
    assert!(e.model.check_sane().is_err());
    // a command can't get one in either (vet bounds every number; the engine checks what is left)
    let sheet = *e.model.sheets.keys().next().unwrap();
    let r = e.exec_json(&json!({"cmd": "viewport_add", "sheet": sheet, "rect_mm": [0, 0, 1e12, 10]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    // a NaN scale in memory is caught too
    let mut e = with_sheets();
    let item = *e.model.sheet_items.keys().next().unwrap();
    let mut v: Value = serde_json::to_value(&*e.model.sheet_items[&item]).unwrap();
    v["scale"] = json!(0.0);
    e.model.sheet_items.insert(item, std::sync::Arc::new(serde_json::from_value(v).unwrap()));
    assert!(e.model.check_sane().is_err(), "scale must be above 0");
}

#[test]
fn sheets_count_toward_the_text_and_size_budgets() {
    let plain = Engine::new();
    let mut e = Engine::new();
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "n".repeat(150), "fields": {"project": "p".repeat(150)}}))["sheet"].clone();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "label": "l".repeat(150)}));
    assert!(e.model.text_bytes() >= plain.model.text_bytes() + 450);
    assert!(e.model.size_estimate() > plain.model.size_estimate() + 800);
    // the estimate stays on the high side of the real file
    let dir = scratch("size");
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    assert!(e.model.size_estimate() as u64 >= std::fs::metadata(&a).unwrap().len(), "{} < {}", e.model.size_estimate(), std::fs::metadata(&a).unwrap().len());
    std::fs::remove_dir_all(&dir).ok();
}
