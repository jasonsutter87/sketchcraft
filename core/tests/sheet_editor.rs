//! What the sheet editor (#9, design step 12) needs from the engine: paper text (`sheet_text`), and trim and extend
//! previews (`preview: true`) that say what would happen and change nothing. Commands, undo, save and open, the
//! drawing (SVG, PDF, the app's display list), refusals and policy.

use serde_json::{json, Value};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-sheet-editor-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn sheet_items(e: &Engine) -> Value {
    serde_json::to_value(&e.model.sheet_items).unwrap()
}

fn refused(e: &mut Engine, v: Value) -> String {
    let before = (e.undo_depth(), sheet_items(e));
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    assert_eq!((e.undo_depth(), sheet_items(e)), before, "a refused command changes nothing and takes no undo step");
    r["error"].as_str().unwrap().to_string()
}

fn item(e: &Engine, id: u64) -> Value {
    serde_json::to_value(&*e.model.sheet_items[&(id as u32)]).unwrap()
}

fn sheet() -> (Engine, u64) {
    let mut e = Engine::new();
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Notes", "paper": "a3"}))["sheet"].as_u64().unwrap();
    (e, s)
}

fn svg(e: &mut Engine, s: u64, dir: &std::path::Path) -> String {
    let p = dir.join("s.svg");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    std::fs::read_to_string(p).unwrap()
}

/// Every text run in the app's display list for the sheet: (text, at, size).
fn ir_texts(e: &mut Engine, s: u64) -> Vec<(String, [f64; 2], f64)> {
    let ir = ok(e, json!({"cmd": "sheet_ir", "sheet": s}));
    let mut out = vec![];
    fn walk(items: &[Value], out: &mut Vec<(String, [f64; 2], f64)>) {
        for i in items {
            if i["k"] == "t" {
                let at = [i["x"].as_f64().unwrap_or(f64::NAN), i["y"].as_f64().unwrap_or(f64::NAN)];
                out.push((i["text"].as_str().unwrap_or("").to_string(), at, i["size"].as_f64().unwrap_or(0.0)));
            }
            if let Some(c) = i["items"].as_array() {
                walk(c, out);
            }
        }
    }
    walk(ir["items"].as_array().unwrap(), &mut out);
    out
}

#[test]
fn text_is_placed_drawn_saved_and_undone_as_one_step() {
    let dir = scratch("text");
    let (mut e, s) = sheet();
    let depth = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [40, 60], "text": "GENERAL NOTES\nAll sizes in mm"}));
    let id = r["item"].as_u64().unwrap();
    assert_eq!(e.undo_depth(), depth + 1, "one undo step");
    let t = item(&e, id);
    assert_eq!(t["kind"], "text");
    assert_eq!(t["paragraphs"], json!(["GENERAL NOTES", "All sizes in mm"]));
    assert_eq!(t["at_mm"], json!([40.0, 60.0]));
    assert!(t.get("height_mm").is_none(), "the default height isn't written");
    // drawn: in the SVG and in the app's display list, one run per line, the second one line lower
    let out = svg(&mut e, s, &dir);
    assert!(out.contains("GENERAL NOTES") && out.contains("All sizes in mm"), "the SVG has both lines");
    let runs = ir_texts(&mut e, s);
    let a = runs.iter().find(|r| r.0 == "GENERAL NOTES").expect("the first line in the display list");
    let b = runs.iter().find(|r| r.0 == "All sizes in mm").expect("the second line");
    assert!((a.1[0] - 40.0).abs() < 0.02 && (a.1[1] - 60.0).abs() < 0.02, "the first line's baseline starts at at_mm: {:?}", a.1);
    assert!((b.1[1] - a.1[1] - 3.5 / 0.73 * 1.25).abs() < 0.05, "the next line is one line pitch down: {:?} {:?}", a.1, b.1);
    // save and open
    let path = dir.join("t.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": path}));
    assert_eq!(sheet_items(&g), sheet_items(&e), "paper text survives save and open");
    // undo and redo
    let after = sheet_items(&e);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(e.model.sheet_items.is_empty());
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(sheet_items(&e), after);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn text_is_edited_by_id_only_what_is_given_changes() {
    let (mut e, s) = sheet();
    let id = ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [40, 60], "text": "Old"}))["item"].as_u64().unwrap();
    let depth = e.undo_depth();
    ok(&mut e, json!({"cmd": "sheet_text", "id": id, "text": "New words"}));
    assert_eq!(e.undo_depth(), depth + 1);
    let t = item(&e, id);
    assert_eq!(t["paragraphs"], json!(["New words"]));
    assert_eq!(t["at_mm"], json!([40.0, 60.0]), "the place stays");
    ok(&mut e, json!({"cmd": "sheet_text", "id": id, "at_mm": [100.5, 20], "height_mm": 7}));
    let t = item(&e, id);
    assert_eq!(t["at_mm"], json!([100.5, 20.0]));
    assert_eq!(t["height_mm"], json!(7.0));
    assert_eq!(t["paragraphs"], json!(["New words"]), "the words stay");
    assert_eq!(e.model.sheet_items.len(), 1, "an edit adds nothing");
    let runs = ir_texts(&mut e, s);
    let r = runs.iter().find(|r| r.0 == "New words").unwrap();
    assert!((r.2 - 7.0 / 0.73).abs() < 0.05, "7 mm capitals: font size {}", r.2);
    ok(&mut e, json!({"cmd": "sheet_item_delete", "ids": [id]}));
    assert!(e.model.sheet_items.is_empty());
}

#[test]
fn text_refuses_what_it_cannot_hold() {
    let (mut e, s) = sheet();
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "at_mm": [1, 1], "text": "x"})).contains("which sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": 999, "at_mm": [1, 1], "text": "x"})).contains("no sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "text": "x"})).contains("at_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "  \n "})).contains("empty"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "a\u{7}b"})).contains("control"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "x".repeat(4001)})).contains("4000"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "x", "height_mm": 0.2})).contains("height_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1e9, 1], "text": "x"})).contains("at_mm"));
    let line = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [10, 10]]}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "id": line, "text": "x"})).contains("not text"));
    let t = ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "x"}))["item"].as_u64().unwrap();
    let s2 = ok(&mut e, json!({"cmd": "sheet_add", "name": "Other"}))["sheet"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "id": t, "sheet": s2, "text": "y"})).contains("stays on its sheet"));
    // non-ASCII text is fine
    ok(&mut e, json!({"cmd": "sheet_text", "id": t, "text": "Øresund — café"}));
}

/// Two crossing lines and one short of a third: the trim and extend previews.
fn crossing() -> (Engine, u64, u64, u64) {
    let (mut e, s) = sheet();
    let h = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[20, 100], [200, 100]]}))["item"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[60, 80], [60, 120]]}));
    ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[120, 80], [120, 120]]}));
    let short = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[20, 150], [100, 150]]}))["item"].as_u64().unwrap();
    let wall = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[160, 130], [160, 170]]}))["item"].as_u64().unwrap();
    (e, h, short, wall)
}

#[test]
fn trim_and_extend_previews_say_what_would_happen_and_change_nothing() {
    let (mut e, h, short, _) = crossing();
    let depth = e.undo_depth();
    let before = sheet_items(&e);
    let p = ok(&mut e, json!({"cmd": "sheet_trim", "item": h, "at_mm": [90, 100.5], "preview": true}));
    assert_eq!(p["preview"], true);
    assert_eq!(p["cut"]["from"], json!([60.0, 100.0]));
    assert_eq!(p["cut"]["to"], json!([120.0, 100.0]));
    assert_eq!(p["pieces"], 2, "trimming the middle would leave two pieces");
    assert_eq!((e.undo_depth(), sheet_items(&e)), (depth, before.clone()), "a preview changes nothing and takes no undo step");
    // the real trim cuts exactly what the preview said
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": h, "at_mm": [90, 100.5]}));
    assert_eq!(r["cut"], p["cut"]);
    assert_eq!(e.undo_depth(), depth + 1);
    // extend
    let before = sheet_items(&e);
    let depth = e.undo_depth();
    let p = ok(&mut e, json!({"cmd": "sheet_extend", "item": short, "at_mm": [95, 150], "preview": true}));
    assert_eq!(p["from"], json!([100.0, 150.0]));
    assert_eq!(p["to"], json!([160.0, 150.0]));
    assert_eq!(p["end"], "end");
    assert_eq!((e.undo_depth(), sheet_items(&e)), (depth, before), "an extend preview changes nothing");
    let r = ok(&mut e, json!({"cmd": "sheet_extend", "item": short, "at_mm": [95, 150]}));
    assert_eq!(r["to"], p["to"]);
    // a preview that can't be done is refused with the same reason as the command
    let lone = ok(&mut e, json!({"cmd": "sheet_line", "sheet": 1, "pts_mm": [[300, 250], [320, 250]]}))["item"].as_u64().unwrap();
    let r = e.exec_json(&json!({"cmd": "sheet_trim", "item": lone, "at_mm": [310, 250], "preview": true}).to_string());
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn a_preview_is_read_only_inside_a_batch_too() {
    let (mut e, h, _, _) = crossing();
    let b = ok(&mut e, json!({"cmd": "begin_batch"}));
    ok(&mut e, json!({"cmd": "sheet_trim", "item": h, "at_mm": [90, 100], "preview": true}));
    ok(&mut e, json!({"cmd": "sheet_trim", "item": h, "at_mm": [90, 100]}));
    ok(&mut e, json!({"cmd": "end_batch", "id": b["batch"]}));
    let n = e.model.sheet_items.len();
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), n - 1, "the batch with a preview in it is still one undo step");
}

#[test]
fn sheets_info_says_where_a_dimension_is_on_the_paper() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plan", "paper": "a3"}))["sheet"].as_u64().unwrap();
    let v = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": 50, "anchor": [2, 1.5, 1.2], "rect_mm": [50, 50, 200, 150]}))["viewport"].as_u64().unwrap();
    let pts = ok(&mut e, json!({"cmd": "viewport_points", "viewport": v}))["points"].as_array().unwrap().clone();
    let at = |x: f64, y: f64| pts.iter().find(|p| p["world"][0].as_f64() == Some(x) && p["world"][1].as_f64() == Some(y)).unwrap_or_else(|| panic!("a point at {x},{y} in {pts:?}")).clone();
    let (a, b) = (at(0.0, 0.0), at(4.0, 0.0));
    let d = ok(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [a["anchor"], b["anchor"]], "viewport": v}))["dimension"].as_u64().unwrap();
    let info = ok(&mut e, json!({"cmd": "sheets_info", "sheet": s}));
    let dim = info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == d).unwrap().clone();
    let paper = dim["paper_mm"].as_array().unwrap();
    assert_eq!(paper.len(), 2);
    for (got, want) in paper.iter().zip([&a, &b]) {
        for k in 0..2 {
            assert!((got[k].as_f64().unwrap() - want["mm"][k].as_f64().unwrap()).abs() < 0.01, "{got} vs {}", want["mm"]);
        }
    }
}

#[test]
fn agents_and_plugins_may_write_text_and_preview() {
    let agent = Caller::Mcp(Workspace::anywhere());
    let c = "sheet_text";
    assert!(permits(&agent, c) && permits(&Caller::Plugin, c) && permits(&Caller::Ui, c), "{c}");
    assert!(vet(&agent, &mut json!({"cmd": "sheet_text", "sheet": 1, "at_mm": [1, 1], "text": "x"})).is_ok());
    let (mut e, s) = sheet();
    let r = e.exec_json_as(&agent, &json!({"cmd": "sheet_text", "sheet": s, "at_mm": [10, 10], "text": "by an agent"}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(item(&e, r["item"].as_u64().unwrap())["made_by"], "agent");
    let help = include_str!("../src/mcp.rs");
    assert!(help.contains("sheet_text") && help.contains("preview: true"), "the MCP help says how");
}
