//! The engine side of the Layout workspace (#63, design step 11): `sheet_ir` (a saved sheet's display list as light
//! JSON, what the app's paper view draws), `sheet_duplicate` (the Pages panel's Duplicate) and `sheet_order` (its
//! reorder), plus the other page commands it sends (rename, delete), each one undo step.

use serde_json::{json, Value};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn bad(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

/// A box, a sheet with a front elevation and a plan of it at 1:50, a dimension measured in the plan, and a line.
fn house() -> (Engine, u64, u64, u64) {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let f = *e.model.faces.keys().next().unwrap();
    let up = e.model.normal(f).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.0 } else { -2.0 }}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plans", "number": "A-101", "paper": "a3", "fields": {"project": "Test house"}}))["sheet"].as_u64().unwrap();
    let front = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "front", "scale": 50, "rect_mm": [20, 20, 180, 120], "label": "Front"}))["viewport"].as_u64().unwrap();
    let plan = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0}, "scale": 50, "rect_mm": [210, 20, 180, 150], "label": "Plan"}))["viewport"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "viewport": plan, "anchors": [{"point": [0, 0, 0]}, {"point": [4, 0, 0]}]}));
    ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[20, 200], [200, 200]]}));
    (e, s, front, plan)
}

fn walk<'a>(items: &'a [Value], out: &mut Vec<&'a Value>) {
    for i in items {
        out.push(i);
        if let Some(c) = i["items"].as_array() {
            walk(c, out);
        }
    }
}

#[test]
fn sheet_ir_is_the_page_as_light_json() {
    let (mut e, s, _, _) = house();
    let r = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    assert_eq!(r["sheet"], s);
    assert_eq!(r["w"], 420.0);
    assert_eq!(r["h"], 297.0);
    assert_eq!(r["truncated"], false);
    let mut all = vec![];
    walk(r["items"].as_array().unwrap(), &mut all);
    // two viewports are two figure groups, clipped to their rects
    let groups: Vec<&&Value> = all.iter().filter(|i| i["k"] == "g" && i["alt"].is_string()).collect();
    assert_eq!(groups.len(), 2, "{:?}", all.iter().map(|i| &i["k"]).collect::<Vec<_>>());
    assert!(groups.iter().all(|g| g["clip"].as_array().is_some_and(|c| c.len() == 4)));
    // paths are SVG path strings with pens, numbers at 0.01 mm
    let paths: Vec<&&Value> = all.iter().filter(|i| i["k"] == "p").collect();
    assert!(paths.len() > 5);
    for p in &paths {
        let d = p["d"].as_str().unwrap();
        assert!(d.starts_with('M'), "{d}");
        assert!(d.chars().all(|c| "MLCZ0123456789.- ".contains(c)), "{d}");
        assert!(!d.split(|c: char| "MLCZ ".contains(c)).any(|n| n.split('.').nth(1).is_some_and(|f| f.len() > 2)), "two decimals at most: {d}");
        assert!(p["s"].is_object() || p["f"].is_object());
        if let Some(c) = p["s"]["c"].as_str() {
            assert!(c.len() == 7 && c.starts_with('#'));
        }
    }
    // stroke-only paths with one pen are joined: fewer paths than the list had
    assert!(r["counts"]["paths"].as_u64().unwrap() < r["counts"]["path_prims"].as_u64().unwrap(), "{}", r["counts"]);
    // the title block's text is there, and the project
    let texts: Vec<&str> = all.iter().filter(|i| i["k"] == "t").filter_map(|i| i["text"].as_str()).collect();
    assert!(texts.contains(&"Test house"), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("Plans")), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("Front")), "{texts:?}");
    // it changes nothing
    let undo_before = e.exec_json(r#"{"cmd":"undo"}"#);
    assert_eq!(undo_before["ok"], true, "the last real change (the line) is undone, not a read");
    let r2 = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    let mut all2 = vec![];
    walk(r2["items"].as_array().unwrap(), &mut all2);
    assert!(all2.len() < all.len(), "undoing the line drew one less thing");
}

#[test]
fn sheet_ir_follows_opacity_and_is_bounded() {
    let (mut e, s, front, _) = house();
    ok(&mut e, json!({"cmd": "viewport_edit", "id": front, "opacity": 0.4}));
    let r = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    let mut all = vec![];
    walk(r["items"].as_array().unwrap(), &mut all);
    let faded: Vec<&&Value> = all.iter().filter(|i| i["k"] == "g" && i["o"].as_f64().is_some_and(|o| (o - 0.4).abs() < 1e-9)).collect();
    assert_eq!(faded.len(), 1, "the front viewport's figure is drawn at 40%");
    // a small limit leaves the rest out and says so
    let r = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s, "limit_kb": 1}));
    assert_eq!(r["truncated"], true);
    assert!(r["bytes"].as_u64().unwrap() <= 1024);
    assert!(r.to_string().len() < 6 * 1024, "the answer stays near its limit");
    // refusals
    assert!(bad(&mut e, json!({"cmd": "sheet_ir", "sheet": 999_999})).contains("no sheet"));
    assert!(bad(&mut e, json!({"cmd": "sheet_ir", "sheet": s, "limit_kb": 0})).contains("limit_kb"));
    assert!(bad(&mut e, json!({"cmd": "sheet_ir", "sheet": s, "limit_kb": 1 << 20})).contains("limit_kb"));
}

#[test]
fn sheet_ir_of_an_empty_page_is_the_frame_and_title_block() {
    let mut e = Engine::new();
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Empty", "paper": "a4"}))["sheet"].as_u64().unwrap();
    let r = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    assert_eq!(r["w"], 297.0);
    let mut all = vec![];
    walk(r["items"].as_array().unwrap(), &mut all);
    assert!(all.iter().any(|i| i["k"] == "p"), "the frame");
    assert!(!all.iter().any(|i| i["k"] == "g" && i["alt"].is_string()), "no viewport");
}

#[test]
fn duplicate_copies_everything_right_after_the_original_in_one_undo_step() {
    let (mut e, s, _front, plan) = house();
    let other = ok(&mut e, json!({"cmd": "sheet_add", "name": "Elevations"}))["sheet"].as_u64().unwrap();
    let items_before = e.model.sheet_items.len();
    let r = ok(&mut e, json!({"cmd": "sheet_duplicate", "sheet": s}));
    let copy = r["sheet"].as_u64().unwrap();
    assert_eq!(r["items"].as_array().unwrap().len(), 4, "two viewports, a dimension and a line");
    assert_eq!(e.model.sheet_items.len(), items_before + 4);
    let info = ok(&mut e, json!({"cmd": "sheets_info"}));
    let names: Vec<&str> = info["sheets"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Plans", "Plans copy", "Elevations"], "the copy sits right after the original");
    let c = info["sheets"].as_array().unwrap().iter().find(|x| x["id"] == copy).unwrap();
    assert_eq!(c["number"], "A-101");
    assert_eq!(c["fields"]["project"], "Test house");
    // the copy's dimension measures in the copy's plan, not the original's
    let copied_plan = r["items"].as_array().unwrap().iter().find(|p| p[0] == plan).unwrap()[1].as_u64().unwrap();
    let dim = c["items"].as_array().unwrap().iter().find(|i| i["kind"] == "dimension").unwrap();
    assert_eq!(dim["viewport"], copied_plan);
    assert_eq!(dim["measured"]["attached"], true);
    // the copy draws what the original draws
    let a = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    let b = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": copy}));
    assert_eq!(a["counts"], b["counts"]);
    // one undo removes it all, redo brings it back
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheets.len(), 2);
    assert_eq!(e.model.sheet_items.len(), items_before);
    assert_eq!(e.model.sheets[&(other as u32)].order, 1, "the sheet after it moved back");
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(e.model.sheets.len(), 3);
    // a name, and refusals
    let r = ok(&mut e, json!({"cmd": "sheet_duplicate", "sheet": s, "name": "Plans, option B"}));
    assert_eq!(e.model.sheets[&(r["sheet"].as_u64().unwrap() as u32)].name, "Plans, option B");
    assert!(bad(&mut e, json!({"cmd": "sheet_duplicate", "sheet": 999_999})).contains("no sheet"));
    assert!(bad(&mut e, json!({"cmd": "sheet_duplicate", "sheet": s, "name": "x".repeat(300)})).contains("too long"));
}

#[test]
fn duplicate_keeps_the_sheet_limit() {
    let mut e = Engine::new();
    let s = ok(&mut e, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    for _ in 1..sketchcraft::policy::MAX_PAGES_UI {
        ok(&mut e, json!({"cmd": "sheet_add"}));
    }
    assert!(bad(&mut e, json!({"cmd": "sheet_duplicate", "sheet": s})).contains("at most"));
}

#[test]
fn a_reorder_is_one_undo_step() {
    let mut e = Engine::new();
    let ids: Vec<u64> = (0..3).map(|i| ok(&mut e, json!({"cmd": "sheet_add", "name": format!("P{}", i + 1)}))["sheet"].as_u64().unwrap()).collect();
    let names = |e: &mut Engine| -> Vec<String> { ok(e, json!({"cmd": "sheets_info"}))["sheets"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect() };
    assert_eq!(names(&mut e), ["P1", "P2", "P3"]);
    // what the Pages panel sends to move P3 to the top: the whole order, one command
    let r = ok(&mut e, json!({"cmd": "sheet_order", "sheets": [ids[2], ids[0], ids[1]]}));
    assert_eq!(r["renumbered"], 3);
    assert_eq!(names(&mut e), ["P3", "P1", "P2"]);
    assert_eq!(e.model.sheets[&(ids[2] as u32)].order, 0);
    assert_eq!(e.model.sheets[&(ids[1] as u32)].order, 20);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(names(&mut e), ["P1", "P2", "P3"]);
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(names(&mut e), ["P3", "P1", "P2"]);
    // rename and delete are one step each
    ok(&mut e, json!({"cmd": "sheet_edit", "sheet": ids[0], "name": "Ground floor"}));
    ok(&mut e, json!({"cmd": "sheet_delete", "sheet": ids[1]}));
    assert_eq!(names(&mut e), ["P3", "Ground floor"]);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(names(&mut e), ["P3", "Ground floor", "P2"]);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(names(&mut e), ["P3", "P1", "P2"]);
    // the same order again changes nothing; a list that leaves one out, names one twice or names a stranger is refused
    assert_eq!(ok(&mut e, json!({"cmd": "sheet_order", "sheets": [ids[2], ids[0], ids[1]]}))["renumbered"], 0);
    assert!(bad(&mut e, json!({"cmd": "sheet_order", "sheets": [ids[2], ids[0]]})).contains("every sheet once"));
    assert!(bad(&mut e, json!({"cmd": "sheet_order", "sheets": [ids[2], ids[0], ids[0]]})).contains("twice"));
    assert!(bad(&mut e, json!({"cmd": "sheet_order", "sheets": [ids[2], ids[0], 999_999]})).contains("no sheet"));
    assert_eq!(names(&mut e), ["P3", "P1", "P2"]);
}

#[test]
fn the_new_commands_are_model_commands_for_everyone() {
    let agent = Caller::Mcp(Workspace::anywhere());
    for c in ["sheet_ir", "sheet_duplicate", "sheet_order"] {
        assert!(permits(&Caller::Ui, c) && permits(&Caller::Plugin, c) && permits(&agent, c), "{c}");
        assert!(vet(&agent, &mut json!({"cmd": c, "sheet": 1, "sheets": [1, 2]})).is_ok(), "{c}");
    }
}
