//! Associative dimensions (#18, design step 5): a dimension on a sheet is attached to vertices, so it follows the
//! model, goes broken when its geometry is deleted (and says so), follows the operations that move geometry to new
//! ids, comes back with undo, and is only ever attached again by an explicit command.

use serde_json::{json, Value};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;
use std::collections::BTreeSet;

#[path = "pdftext/mod.rs"]
mod pdftext;

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

/// A solid box (x0..x1, y0..y1, 2.5 high) as a group; returns the group.
fn wall(e: &mut Engine, x0: f64, x1: f64, y0: f64, y1: f64) -> u32 {
    let before: BTreeSet<u32> = e.model.faces.keys().copied().collect();
    let f = ok(e, json!({"cmd": "rect", "from": [x0, y0, 0], "to": [x1, y1, 0]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let new: Vec<u32> = e.model.faces.keys().copied().filter(|k| !before.contains(k)).collect();
    ok(e, json!({"cmd": "group", "ids": new}))["group"].as_u64().unwrap() as u32
}

/// A model with walls at the given x positions (each 0.2 thick, 3 long), and a sheet with a 1:50 plan viewport.
fn site(xs: &[f64]) -> (Engine, u32, Vec<u32>) {
    let mut e = Engine::new();
    let walls = xs.iter().map(|x| wall(&mut e, *x, *x + 0.2, 0.0, 3.0)).collect();
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plan", "paper": "a3"}))["sheet"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "anchor": [4.0, 1.5, 1.2], "rect_mm": [15, 15, 380, 200], "crop": [[0, 0], [380, 0], [380, 200]]}));
    (e, s, walls)
}

fn dim(e: &mut Engine, sheet: u32, points: &[[f64; 3]], mode: &str) -> u32 {
    let anchors: Vec<Value> = points.iter().map(|p| json!({"point": p})).collect();
    ok(e, json!({"cmd": "sheet_dim", "sheet": sheet, "anchors": anchors, "mode": mode}))["dimension"].as_u64().unwrap() as u32
}

fn item(e: &Engine, id: u32) -> Value {
    serde_json::to_value(&*e.model.sheet_items[&id]).unwrap()
}

/// What `sheets_info` says a dimension measures now: the span texts, and the broken anchors.
fn read(e: &mut Engine, sheet: u32, id: u32) -> (Vec<String>, Vec<Value>) {
    let info = ok(e, json!({"cmd": "sheets_info", "sheet": sheet}));
    let it = info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone();
    let spans = it["measured"]["spans"].as_array().unwrap().iter().map(|s| s["text"].as_str().unwrap().to_string()).collect();
    (spans, it["measured"]["broken"].as_array().unwrap().clone())
}

fn pdf_text(e: &mut Engine, sheet: u32, name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("sc-anchors-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("s.pdf");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [sheet]}));
    let pdf = pdftext::parse(&std::fs::read(&p).unwrap());
    let t = pdf.pages().iter().flat_map(|pg| pdf.text(*pg)).collect::<Vec<_>>().join("\n");
    std::fs::remove_dir_all(&dir).ok();
    t
}

#[test]
fn move_a_wall_and_the_value_follows() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    // the inside face of the left wall to the near face of the right one: 3.8 m
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    assert_eq!(read(&mut e, s, d).0, ["3.80 m"]);
    assert!(pdf_text(&mut e, s, "move").contains("3.80 m"));
    let depth = e.undo_depth();
    ok(&mut e, json!({"cmd": "move", "ids": [w[1]], "by": [1.0, 0.0, 0.0]}));
    // the dimension updated in the same undo step as the move, and stayed attached
    assert_eq!(e.undo_depth(), depth + 1);
    let (spans, broken) = read(&mut e, s, d);
    assert_eq!(spans, ["4.80 m"]);
    assert!(broken.is_empty());
    // "refresh the sheet": the PDF draws the new value, and only that
    let t = pdf_text(&mut e, s, "move2");
    assert!(t.contains("4.80 m") && !t.contains("3.80 m"), "{t}");
    // the last good point followed the geometry
    let last = item(&e, d)["anchors"][1]["last"].clone();
    assert!((last[0].as_f64().unwrap() - 5.0).abs() < 1e-9, "{last}");
    // a vertical move changes a horizontal dimension by nothing
    ok(&mut e, json!({"cmd": "move", "ids": [w[1]], "by": [0.0, 2.0, 0.0]}));
    assert_eq!(read(&mut e, s, d).0, ["4.80 m"]);
    // undo takes the move and the anchor fix together
    let before = item(&e, d);
    ok(&mut e, json!({"cmd": "undo"}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(read(&mut e, s, d).0, ["3.80 m"]);
    assert_ne!(item(&e, d), before);
    ok(&mut e, json!({"cmd": "redo"}));
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(item(&e, d), before, "redo brings the settled anchors back too");
}

#[test]
fn aligned_dimensions_measure_in_the_viewports_plane() {
    let (mut e, s, _) = site(&[0.0, 4.0]);
    // corner to corner across the plan, and the same with a height difference: the plan ignores z
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 3.0, 2.5]], "aligned");
    assert_eq!(read(&mut e, s, d).0, [format!("{:.2} m", (3.8f64).hypot(3.0))]);
    let v = dim(&mut e, s, &[[0.2, 0.0, 0.0], [0.2, 3.0, 2.5]], "vertical");
    assert_eq!(read(&mut e, s, v).0, ["3.00 m"]);
}

#[test]
fn split_an_edge_and_the_dimension_stays_attached() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let s = ok(&mut e, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 0.1, "floor_z": 0.0}, "scale": 50, "anchor": [2.0, 1.5, 0.0], "rect_mm": [15, 15, 380, 200]}));
    let corner = [4.0, 3.0, 0.0];
    // a point a quarter and a half of the way along the bottom edge: along the edge, not on a vertex
    let quarter = dim(&mut e, s, &[[1.0, 0.0, 0.0], corner], "aligned");
    let half = dim(&mut e, s, &[[2.0, 0.0, 0.0], corner], "aligned");
    assert_eq!(item(&e, quarter)["anchors"][0]["at"]["between"]["t"], 0.25);
    let (qv, hv) = (read(&mut e, s, quarter).0, read(&mut e, s, half).0);
    // draw a line from the middle of that edge up into the face: the edge is split at x = 2
    ok(&mut e, json!({"cmd": "line", "points": [[2, 0, 0], [2, 1, 0]]}));
    assert_eq!(e.model.vertices.values().filter(|v| (v.p.x - 2.0).abs() < 1e-9 && v.p.y == 0.0).count(), 1);
    // both stay attached, to the same place: the half one is on the new vertex now, the quarter one on the piece it is on
    assert_eq!(read(&mut e, s, quarter), (qv, vec![]));
    assert_eq!(read(&mut e, s, half), (hv, vec![]));
    assert!(item(&e, half)["anchors"][0]["at"]["vertex"].is_u64(), "{}", item(&e, half)["anchors"][0]);
    let q = item(&e, quarter)["anchors"][0]["at"]["between"].clone();
    assert!((q["t"].as_f64().unwrap() - 0.5).abs() < 1e-9, "a quarter of the whole is half of the first piece: {q}");
    // and the new vertex now drags the dimension with it
    let v = e.model.vertices.iter().find(|(_, v)| (v.p.x - 2.0).abs() < 1e-9 && v.p.y == 0.0).map(|(k, _)| *k).unwrap();
    ok(&mut e, json!({"cmd": "move", "ids": [v], "by": [0.0, -1.0, 0.0]}));
    assert_ne!(read(&mut e, s, half).0, read(&mut e, s, quarter).0);
}

#[test]
fn delete_and_it_is_broken_and_undo_restores_it() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    let intact = item(&e, d);
    ok(&mut e, json!({"cmd": "delete", "ids": [w[1]]}));
    // the dimension is still there, says it is broken, where the anchor was, and has no value
    let (spans, broken) = read(&mut e, s, d);
    assert_eq!(spans, ["?"]);
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0]["anchor"], 1);
    assert_eq!(broken[0]["last"], json!([4.0, 0.0, 0.0]));
    // nothing moved it, and the surviving anchor is untouched
    assert_eq!(item(&e, d)["anchors"][1]["last"], json!([4.0, 0.0, 0.0]));
    assert_eq!(item(&e, d)["anchors"][0], intact["anchors"][0]);
    // the sheet draws it broken: a "?" for the value and a "!" badge
    let t = pdf_text(&mut e, s, "broken");
    assert!(t.contains('?') && t.contains('!') && !t.contains("3.80 m"), "{t}");
    // undo brings the wall and the dimension back as they were
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(item(&e, d), intact);
    assert_eq!(read(&mut e, s, d), (vec!["3.80 m".to_string()], vec![]));
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(read(&mut e, s, d).0, ["?"]);
}

#[test]
fn a_broken_anchor_is_never_attached_again_by_itself() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    ok(&mut e, json!({"cmd": "delete", "ids": [w[1]]}));
    // a new wall exactly where it was: the dimension stays broken (decision 8) ...
    let w2 = wall(&mut e, 4.0, 4.2, 0.0, 3.0);
    let (spans, broken) = read(&mut e, s, d);
    assert_eq!(spans, ["?"]);
    // ... but the vertex that is there is offered
    let sug = broken[0]["suggestions"].as_array().unwrap();
    assert_eq!(sug.len(), 1, "{broken:?}");
    assert!(sug[0]["at"]["vertex"].is_u64());
    // an explicit command attaches it, in one undoable step
    let depth = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_reattach", "all_exact": true}));
    assert_eq!(r["reattached"], 1);
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(read(&mut e, s, d), (vec!["3.80 m".to_string()], vec![]));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(read(&mut e, s, d).0, ["?"]);
    // `to` attaches one anchor to exactly the vertex given, wherever it is
    let v = e.model.vertices.iter().find(|(_, v)| (v.p.x - 4.2).abs() < 1e-9 && v.p.y == 0.0 && v.p.z == 0.0).map(|(k, _)| *k).unwrap();
    ok(&mut e, json!({"cmd": "sheet_reattach", "item": d, "anchor": 1, "to": {"vertex": v}}));
    assert_eq!(read(&mut e, s, d).0, ["4.00 m"]);
    ok(&mut e, json!({"cmd": "undo"}));
    // two vertices where it was: nothing is guessed
    let w3 = wall(&mut e, 4.0, 4.2, 0.0, 3.0);
    let _ = (w2, w3);
    let err = refused(&mut e, json!({"cmd": "sheet_reattach", "all_exact": true}));
    assert!(err.contains("more than one"), "{err}");
    assert_eq!(read(&mut e, s, d).0, ["?"]);
    // and bad calls say what to do
    assert!(refused(&mut e, json!({"cmd": "sheet_reattach"})).contains("all_exact"));
    assert!(refused(&mut e, json!({"cmd": "sheet_reattach", "item": 1, "anchor": 0, "to": {"vertex": v}})).contains("not a dimension"));
}

#[test]
fn a_chain_of_continuous_dimensions_follows_every_wall() {
    let (mut e, s, w) = site(&[0.0, 4.0, 7.0]);
    let pts = [[0.2, 0.0, 0.0], [4.0, 0.0, 0.0], [7.0, 0.0, 0.0]];
    let chain = dim(&mut e, s, &pts, "horizontal");
    let base = dim(&mut e, s, &pts, "baseline");
    assert_eq!(read(&mut e, s, chain).0, ["3.80 m", "3.00 m"]);
    assert_eq!(read(&mut e, s, base).0, ["3.80 m", "6.80 m"], "a baseline set measures every span from the first anchor");
    // the middle wall moves: both spans of the chain change, the far one of the baseline set too
    ok(&mut e, json!({"cmd": "move", "ids": [w[1]], "by": [1.0, 0.0, 0.0]}));
    assert_eq!(read(&mut e, s, chain).0, ["4.80 m", "2.00 m"]);
    assert_eq!(read(&mut e, s, base).0, ["4.80 m", "6.80 m"]);
    // the middle wall goes: both spans of the chain are broken, the baseline's far span is fine
    ok(&mut e, json!({"cmd": "delete", "ids": [w[1]]}));
    assert_eq!(read(&mut e, s, chain).0, ["?", "?"]);
    assert_eq!(read(&mut e, s, chain).1.len(), 1);
    assert_eq!(read(&mut e, s, base).0, ["?", "6.80 m"]);
    // a chain draws all its spans
    let t = pdf_text(&mut e, s, "chain");
    assert!(t.contains("6.80 m"), "{t}");
}

#[test]
fn dimensions_follow_geometry_into_a_component_and_back_out() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    // the wall becomes a component: its points have new ids in the component, and the dimension follows them
    let depth = e.undo_depth();
    let inst = ok(&mut e, json!({"cmd": "make_component", "ids": [w[1]], "name": "Wall"}))["instance"].as_u64().unwrap();
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(read(&mut e, s, d), (vec!["3.80 m".to_string()], vec![]));
    assert_eq!(item(&e, d)["anchors"][1]["path"], json!([inst]));
    // moving the placement moves the dimension
    ok(&mut e, json!({"cmd": "move", "ids": [inst], "by": [1.0, 0.0, 0.0]}));
    assert_eq!(read(&mut e, s, d).0, ["4.80 m"]);
    // exploding it puts the points back in the model, with new ids again
    ok(&mut e, json!({"cmd": "explode", "id": inst}));
    assert_eq!(read(&mut e, s, d), (vec!["4.80 m".to_string()], vec![]));
    assert!(item(&e, d)["anchors"][1].get("path").is_none());
    // undo takes the explode and the remap together
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(item(&e, d)["anchors"][1]["path"], json!([inst]));
    assert_eq!(read(&mut e, s, d).0, ["4.80 m"]);
    // deleting the placement breaks it
    ok(&mut e, json!({"cmd": "delete", "ids": [inst]}));
    assert_eq!(read(&mut e, s, d).0, ["?"]);
}

#[test]
fn a_viewport_keeps_its_crop_and_scale_after_model_edits() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    let vp = *e.model.sheet_items.iter().find(|(_, i)| matches!(&***i, sketchcraft::drafting::SheetItem::Known(sketchcraft::drafting::ItemKind::Viewport(_)))).unwrap().0;
    let before = item(&e, vp);
    assert_eq!(before["scale"], 50.0);
    assert_eq!(before["crop"], json!([[0.0, 0.0], [380.0, 0.0], [380.0, 200.0]]));
    // a far-off box (the model's centre moves), a moved wall, a deleted wall, a component: none re-fits the viewport
    wall(&mut e, 900.0, 901.0, 900.0, 901.0);
    ok(&mut e, json!({"cmd": "move", "ids": [w[1]], "by": [2.0, 1.0, 0.0]}));
    ok(&mut e, json!({"cmd": "make_component", "ids": [w[0]], "name": "Left"}));
    ok(&mut e, json!({"cmd": "delete", "ids": [w[1]]}));
    assert_eq!(item(&e, vp), before);
    // and the dimension's value comes from the model, not from the paper: the scale never enters it
    ok(&mut e, json!({"cmd": "viewport_edit", "id": vp, "scale": 100}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(item(&e, vp), before);
    let _ = d;
}

#[test]
fn an_agent_can_dimension_and_its_work_is_one_undo_step() {
    let (mut e, s, _) = site(&[0.0, 4.0]);
    let agent = Caller::Mcp(Workspace::anywhere());
    for c in ["sheet_dim", "sheet_reattach"] {
        assert!(permits(&agent, c) && permits(&Caller::Plugin, c), "{c}");
    }
    let depth = e.undo_depth();
    let r = e.exec_json_as(&agent, &json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"point": [0.2, 0, 0]}, {"point": [4, 0, 0]}], "mode": "horizontal"}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(item(&e, r["dimension"].as_u64().unwrap() as u32)["made_by"], "agent");
    // too many anchors, a point on nothing, unknown fields, a non-finite offset: refused, nothing made
    let many: Vec<Value> = (0..201).map(|_| json!({"point": [0.2, 0, 0]})).collect();
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": many})).contains("anchors"));
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"point": [0.2, 0, 0]}, {"point": [50, 50, 50]}]})).contains("anchor 1"));
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"point": [0.2, 0, 0]}, {"vertex": 999999}]})).contains("nothing at that vertex"));
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"point": [0.2, 0, 0]}, {"vertex": 1, "point": [0, 0, 0]}]})).contains("exactly one"));
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": [{"point": [0.2, 0, 0]}], "mode": "aligned"})).contains("2 to 200"));
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": 12345, "anchors": [{"point": [0.2, 0, 0]}, {"point": [4, 0, 0]}]})).contains("no sheet"));
    assert!(vet(&Caller::Ui, &mut json!({"cmd": "sheet_dim", "sheet": s, "anchors": [], "offset_mm": 1e300})).is_err());
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(e.model.sheet_items.values().all(|i| !matches!(&**i, sketchcraft::drafting::SheetItem::Known(sketchcraft::drafting::ItemKind::Dimension(_)))));
}

#[test]
fn anchor_near_is_the_api_for_importers() {
    // #59 attaches imported dimensions through `Model::anchor_near`: a vertex within the tolerance, else a point along an edge
    let (e, _, _) = site(&[0.0]);
    let a = e.model.anchor_near(glam::DVec3::new(0.2, 0.0, 0.0), 1e-4).unwrap();
    assert!(matches!(a.at, sketchcraft::drafting::AnchorAt::Vertex(_)));
    let b = e.model.anchor_near(glam::DVec3::new(0.1, 0.0, 0.0), 1e-4).unwrap();
    assert!(matches!(b.at, sketchcraft::drafting::AnchorAt::Between { .. }));
    assert!((e.model.resolve_anchor(&b).unwrap() - glam::DVec3::new(0.1, 0.0, 0.0)).length() < 1e-9);
    assert!(e.model.anchor_near(glam::DVec3::new(9.0, 9.0, 9.0), 1e-3).is_none());
    // a 1 micrometre tolerance finds the vertex at the same place only
    assert_eq!(e.model.anchors_at(glam::DVec3::new(0.2, 0.0, 0.0), 1e-6).len(), 1);
}

#[test]
fn dimensions_survive_a_save_and_open() {
    let (mut e, s, w) = site(&[0.0, 4.0]);
    let d = dim(&mut e, s, &[[0.2, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    ok(&mut e, json!({"cmd": "move", "ids": [w[1]], "by": [1.0, 0.0, 0.0]}));
    let dir = std::env::temp_dir().join(format!("sc-anchors-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("m.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p}));
    let mut f = Engine::new();
    ok(&mut f, json!({"cmd": "open", "path": p}));
    assert_eq!(read(&mut f, s, d).0, ["4.80 m"]);
    // opening settles nothing and changes nothing
    assert_eq!(item(&f, d), item(&e, d));
    std::fs::remove_dir_all(&dir).ok();
}
