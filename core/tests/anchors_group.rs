//! Dimensions follow `group` (#18, design step 5): when Group gives the new group its own copies of the corners it
//! shared with outside geometry, a dimension whose measured geometry all went into the group follows the copies;
//! any other stays on the original corners. One undo step with the group.

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn item(e: &Engine, id: u32) -> Value {
    serde_json::to_value(&*e.model.sheet_items[&id]).unwrap()
}

fn read(e: &mut Engine, sheet: u32, id: u32) -> (Vec<String>, Vec<Value>) {
    let info = ok(e, json!({"cmd": "sheets_info", "sheet": sheet}));
    let it = info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone();
    let spans = it["measured"]["spans"].as_array().unwrap().iter().map(|s| s["text"].as_str().unwrap().to_string()).collect();
    (spans, it["measured"]["broken"].as_array().unwrap().clone())
}

fn dim(e: &mut Engine, sheet: u32, points: &[[f64; 3]], mode: &str) -> u32 {
    let anchors: Vec<Value> = points.iter().map(|p| json!({"point": p})).collect();
    ok(e, json!({"cmd": "sheet_dim", "sheet": sheet, "anchors": anchors, "mode": mode}))["dimension"].as_u64().unwrap() as u32
}

struct Scene {
    e: Engine,
    sheet: u32,
    /// the face that will be grouped (x 0..2); the neighbour (x 2..4) stays outside and shares the x = 2 corners
    a: u32,
}

fn scene() -> Scene {
    let mut e = Engine::new();
    let a = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["face"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "rect", "from": [2, 0, 0], "to": [4, 2, 0]}));
    let sheet = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plan", "paper": "a3"}))["sheet"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": sheet, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "anchor": [2.0, 1.0, 0.0], "rect_mm": [15, 15, 380, 200], "crop": [[0, 0], [380, 0], [380, 200]]}));
    Scene { e, sheet, a }
}

/// the three dimensions of the scene: both ends on shared corners, one end shared and one unshared (both in the
/// group), and one end shared and the other on the neighbour outside
fn dims(s: &mut Scene) -> (u32, u32, u32) {
    let both = dim(&mut s.e, s.sheet, &[[2.0, 0.0, 0.0], [2.0, 2.0, 0.0]], "vertical");
    let mixed = dim(&mut s.e, s.sheet, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]], "horizontal");
    let outside = dim(&mut s.e, s.sheet, &[[2.0, 0.0, 0.0], [4.0, 0.0, 0.0]], "horizontal");
    (both, mixed, outside)
}

fn group_and_move(s: &mut Scene) -> u32 {
    let g = ok(&mut s.e, json!({"cmd": "group", "ids": [s.a]}))["group"].as_u64().unwrap() as u32;
    ok(&mut s.e, json!({"cmd": "move", "ids": [g], "by": [10.0, 0.0, 0.0]}));
    g
}

#[test]
fn a_dimension_between_two_copied_corners_follows_the_group() {
    let mut s = scene();
    let (both, ..) = dims(&mut s);
    let before = item(&s.e, both);
    group_and_move(&mut s);
    assert_ne!(item(&s.e, both)["anchors"], before["anchors"], "anchors were rewritten to the copies");
    let last = item(&s.e, both)["anchors"][0]["last"].clone();
    assert!((last[0].as_f64().unwrap() - 12.0).abs() < 1e-9, "followed the group's move: {last}");
    assert_eq!(read(&mut s.e, s.sheet, both), (vec!["2.00 m".to_string()], vec![]));
}

#[test]
fn a_corner_shared_with_a_neighbour_that_stays_outside_stays_put() {
    let mut s = scene();
    let (_, _, outside) = dims(&mut s);
    let before = item(&s.e, outside)["anchors"].clone();
    group_and_move(&mut s);
    assert_eq!(item(&s.e, outside)["anchors"], before);
    assert_eq!(read(&mut s.e, s.sheet, outside), (vec!["2.00 m".to_string()], vec![]));
}

#[test]
fn mixed_case_one_follows_one_stays_and_a_shared_plus_unshared_pair_follows() {
    let mut s = scene();
    let (both, mixed, outside) = dims(&mut s);
    group_and_move(&mut s);
    // the whole face moved 10 m: the pair inside the group is still 2 m, the one tied to the neighbour is too
    assert_eq!(read(&mut s.e, s.sheet, mixed).0, ["2.00 m"]);
    assert_eq!(read(&mut s.e, s.sheet, outside).0, ["2.00 m"]);
    assert_eq!(read(&mut s.e, s.sheet, both).0, ["2.00 m"]);
    // had `mixed` stayed on the original corner it would now read 12 m
    let last = item(&s.e, mixed)["anchors"][1]["last"].clone();
    assert!((last[0].as_f64().unwrap() - 12.0).abs() < 1e-9, "{last}");
}

#[test]
fn it_is_one_undo_step_and_redo_restores_it() {
    let mut s = scene();
    let (both, mixed, outside) = dims(&mut s);
    let start = [item(&s.e, both), item(&s.e, mixed), item(&s.e, outside)];
    let depth = s.e.undo_depth();
    ok(&mut s.e, json!({"cmd": "group", "ids": [s.a]}));
    assert_eq!(s.e.undo_depth(), depth + 1);
    let after = [item(&s.e, both), item(&s.e, mixed), item(&s.e, outside)];
    assert_ne!(after[0], start[0]);
    assert_ne!(after[1], start[1]);
    assert_eq!(after[2], start[2]);
    ok(&mut s.e, json!({"cmd": "undo"}));
    assert_eq!([item(&s.e, both), item(&s.e, mixed), item(&s.e, outside)], start);
    ok(&mut s.e, json!({"cmd": "redo"}));
    assert_eq!([item(&s.e, both), item(&s.e, mixed), item(&s.e, outside)], after);
}

#[test]
fn no_broken_anchors_after_group_and_the_result_survives_save_and_open() {
    let mut s = scene();
    let (both, mixed, outside) = dims(&mut s);
    group_and_move(&mut s);
    for d in [both, mixed, outside] {
        assert!(read(&mut s.e, s.sheet, d).1.is_empty(), "dimension {d} has a broken anchor");
    }
    let dir = std::env::temp_dir().join(format!("sc-anchors-group-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("m.sketchcraft");
    ok(&mut s.e, json!({"cmd": "save", "path": p}));
    let mut f = Engine::new();
    ok(&mut f, json!({"cmd": "open", "path": p}));
    for d in [both, mixed, outside] {
        assert_eq!(item(&f, d), item(&s.e, d));
        assert_eq!(read(&mut f, s.sheet, d), read(&mut s.e, s.sheet, d));
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_group_with_no_shared_corners_changes_no_anchor() {
    let mut s = scene();
    let far = ok(&mut s.e, json!({"cmd": "rect", "from": [20, 0, 0], "to": [22, 2, 0]}))["face"].as_u64().unwrap();
    let d = dim(&mut s.e, s.sheet, &[[20.0, 0.0, 0.0], [22.0, 0.0, 0.0]], "horizontal");
    let before = item(&s.e, d)["anchors"].clone();
    ok(&mut s.e, json!({"cmd": "group", "ids": [far]}));
    assert_eq!(item(&s.e, d)["anchors"], before);
    assert!(read(&mut s.e, s.sheet, d).1.is_empty());
}
