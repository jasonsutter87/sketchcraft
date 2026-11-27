//! A scene that shows only some things (#229): everything else is hidden when it is shown, including what is drawn
//! after the scene was saved, and an ordinary scene shown after it brings the rest back.

use serde_json::{json, Value};
use sketchcraft::Engine;

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

/// A box at `x` made into a group; returns the group.
fn boxed(e: &mut Engine, x: f64, name: &str) -> u32 {
    let before: std::collections::BTreeSet<u32> = e.model.faces.keys().copied().collect();
    let f = ok(e, json!({"cmd": "rect", "from": [x, 0, 0], "to": [x + 1.0, 1, 0]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": 1}));
    let new: Vec<u32> = e.model.faces.keys().copied().filter(|k| !before.contains(k)).collect();
    ok(e, json!({"cmd": "group", "ids": new, "name": name}))["group"].as_u64().unwrap() as u32
}

const CAM: [f64; 3] = [5.0, -8.0, 4.0];

fn save(e: &mut Engine, name: &str, only: Option<Vec<u32>>) -> Value {
    let mut c = json!({"cmd": "save_view", "name": name, "eye": CAM, "target": [0, 0, 0]});
    if let Some(o) = only {
        c["only"] = json!(o);
    }
    ok(e, c)
}

#[test]
fn shows_only_what_it_names_and_hides_what_comes_later() {
    let mut e = Engine::new();
    let kitchen = boxed(&mut e, 0.0, "Kitchen");
    let hall = boxed(&mut e, 3.0, "Hall");
    save(&mut e, "Kitchen only", Some(vec![kitchen]));
    save(&mut e, "Everything", None);
    // drawn after both scenes: a loose rectangle and another group
    ok(&mut e, json!({"cmd": "rect", "from": [10, 0, 0], "to": [11, 1, 0]}));
    let shed = boxed(&mut e, 6.0, "Shed");

    ok(&mut e, json!({"cmd": "show_view", "name": "Kitchen only"}));
    assert!(!e.model.groups[&kitchen].hidden, "the kitchen shows");
    assert!(e.model.groups[&hall].hidden, "the hall is hidden");
    assert!(e.model.groups[&shed].hidden, "the shed, drawn later, is hidden too");
    let loose_faces = e.model.faces.values().filter(|f| f.group.is_none());
    assert!(loose_faces.clone().count() > 0 && loose_faces.clone().all(|f| f.hidden), "loose faces drawn later are hidden");
    assert!(e.model.edges.values().filter(|x| x.group.is_none()).all(|x| x.hidden), "and their lines");
    assert!(e.model.faces.values().filter(|f| f.group == Some(kitchen)).all(|f| !f.hidden), "the kitchen's own faces show");

    ok(&mut e, json!({"cmd": "show_view", "name": "Everything"}));
    assert!(!e.model.groups[&hall].hidden && !e.model.groups[&shed].hidden, "an ordinary scene shows them again");
    assert!(e.model.edges.values().all(|x| !x.hidden) && e.model.faces.values().all(|f| !f.hidden), "lines and faces too");
}

#[test]
fn a_group_inside_shows_its_parent_and_loose_faces_bring_their_lines() {
    let mut e = Engine::new();
    let chair = boxed(&mut e, 0.0, "Chair");
    let table = boxed(&mut e, 3.0, "Table");
    let room = ok(&mut e, json!({"cmd": "group", "ids": [chair, table], "name": "Room"}))["group"].as_u64().unwrap() as u32;
    let f = ok(&mut e, json!({"cmd": "rect", "from": [10, 0, 0], "to": [11, 1, 0]}))["face"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "rect", "from": [20, 0, 0], "to": [21, 1, 0]}));
    save(&mut e, "Chair", Some(vec![chair, f]));
    ok(&mut e, json!({"cmd": "show_view", "name": "Chair"}));
    assert!(!e.model.groups[&room].hidden, "the room the chair sits in shows");
    assert!(!e.model.groups[&chair].hidden);
    assert!(e.model.groups[&table].hidden, "the table beside it doesn't");
    assert!(!e.model.faces[&f].hidden, "the named face shows");
    let corners: Vec<u32> = e.model.faces[&f].outer.clone();
    let shown_lines = e.model.edges.values().filter(|x| !x.hidden && x.group.is_none()).count();
    assert_eq!(shown_lines, corners.len(), "with its four lines, and no others loose");
}

#[test]
fn the_list_survives_save_and_open_and_bad_input_is_refused() {
    let mut e = Engine::new();
    let a = boxed(&mut e, 0.0, "A");
    save(&mut e, "Just A", Some(vec![a, a, 999_999]));
    assert_eq!(e.model.views[0].only, Some(vec![a]), "kept once, ids that aren't there dropped");
    let text = serde_json::to_string(&e.model).unwrap();
    let back: sketchcraft::model::Model = serde_json::from_str(&text).unwrap();
    assert_eq!(back.views[0].only, Some(vec![a]));
    assert!(refused(&mut e, json!({"cmd": "save_view", "name": "x", "eye": CAM, "target": [0, 0, 0], "only": [999_999]})).contains("nothing to show"));
    assert!(refused(&mut e, json!({"cmd": "save_view", "name": "x", "eye": CAM, "target": [0, 0, 0], "only": [a], "camera_only": true})).contains("camera only"));
    // an ordinary scene saved by an older version has no list and reads as before
    let old: sketchcraft::model::View = serde_json::from_value(json!({"name": "Old", "eye": CAM, "target": [0, 0, 0], "fov": 45.0, "hidden": []})).unwrap();
    assert_eq!(old.only, None);
}

#[test]
fn undo_after_showing_puts_visibility_back() {
    let mut e = Engine::new();
    let a = boxed(&mut e, 0.0, "A");
    let b = boxed(&mut e, 3.0, "B");
    save(&mut e, "Just A", Some(vec![a]));
    ok(&mut e, json!({"cmd": "show_view", "name": "Just A"}));
    assert!(e.model.groups[&b].hidden);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(!e.model.groups[&b].hidden);
}
