//! Door and Window tools: any size, in the wall that was clicked, facing out of it, cut through it, classed for IFC,
//! and resizable afterwards in a wall the Wall tool made (#264).

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should fail -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

/// A 6 m wall along x, 0.2 thick, 2.7 tall, and its face looking toward -y.
fn wall(e: &mut Engine) -> (u64, u32) {
    let g = ok(e, json!({"cmd": "wall", "points": [[0, 0, 0], [6, 0, 0]], "thickness": 0.2, "height": 2.7}))["group"].as_u64().unwrap();
    let front = *e.model.faces.iter().find(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).y < -0.99 && f.outer.len() == 4).unwrap().0;
    (g, front)
}

fn wall_faces(e: &Engine, g: u64) -> usize {
    e.model.faces.values().filter(|f| f.group == Some(g as u32)).count()
}

/// Is there wall at point `p` on the front face plane (y = -0.1)?
fn wall_at(e: &Engine, g: u64, x: f64, z: f64) -> bool {
    let p = DVec3::new(x, -0.1, z);
    e.model.faces.iter().any(|(&k, f)| f.group == Some(g as u32) && e.model.contains(k, p))
}

#[test]
fn a_window_goes_in_where_clicked_at_its_sill_and_through_the_wall() {
    let mut e = Engine::new();
    let (g, front) = wall(&mut e);
    let r = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [2, -0.1, 1.5], "width": 1.0, "height": 1.2, "sill": 0.9}));
    let inst = r["instance"].as_u64().unwrap() as u32;
    assert!(!wall_at(&e, g, 2.0, 1.5), "a hole where the window is");
    assert!(wall_at(&e, g, 2.0, 0.5), "wall under the sill");
    assert!(wall_at(&e, g, 2.0, 2.3), "wall over the head");
    assert!(wall_at(&e, g, 1.3, 1.5), "wall beside it");
    // through the wall: the back face has a hole too
    let back = DVec3::new(2.0, 0.1, 1.5);
    assert!(!e.model.faces.iter().any(|(&k, f)| f.group == Some(g as u32) && e.model.contains(k, back)), "the back is open too");
    // facing out of the wall: its local -Y is the face's outward normal
    let t = e.model.instances[&inst].transform;
    assert!((t.transform_vector3(-DVec3::Y) - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
    assert!((t.w_axis.x - 1.5).abs() < 1e-9 && (t.w_axis.z - 0.9).abs() < 1e-9, "centred on the click, at the sill: {:?}", t.w_axis);
    assert_eq!(e.model.classes.get(&inst).map(String::as_str), Some("IfcWindow"));
}

#[test]
fn a_door_stands_on_the_floor_and_is_classed_a_door() {
    let mut e = Engine::new();
    let (g, front) = wall(&mut e);
    let r = ok(&mut e, json!({"cmd": "opening", "kind": "door", "host": front, "at": [4, -0.1, 1.8], "width": 0.9, "height": 2.1, "sill": 1.0}));
    let inst = r["instance"].as_u64().unwrap() as u32;
    assert_eq!(r["sill"], 0.0, "a door ignores the sill");
    assert!((e.model.instances[&inst].transform.w_axis.z).abs() < 1e-9);
    assert!(!wall_at(&e, g, 4.0, 0.05), "open down to the floor");
    assert_eq!(e.model.classes.get(&inst).map(String::as_str), Some("IfcDoor"));
}

#[test]
fn near_the_end_it_moves_along_to_fit() {
    let mut e = Engine::new();
    let (_, front) = wall(&mut e);
    let r = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [5.9, -0.1, 1.5], "width": 1.2, "height": 1.0}));
    let x = e.model.instances[&(r["instance"].as_u64().unwrap() as u32)].transform.w_axis.x;
    assert!(x <= 6.0 - 1.2 + 1e-9, "kept inside the wall: {x}");
}

#[test]
fn what_does_not_fit_is_refused_and_changes_nothing() {
    let mut e = Engine::new();
    let (_, front) = wall(&mut e);
    let before = e.model.faces.clone();
    let err = fails(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [3, -0.1, 1], "width": 1.0, "height": 2.5, "sill": 0.9}));
    assert!(err.contains("top of the wall"), "{err}");
    fails(&mut e, json!({"cmd": "opening", "kind": "door", "host": front, "at": [3, -0.1, 1], "width": 9.0, "height": 2.1}));
    fails(&mut e, json!({"cmd": "opening", "kind": "skylight", "host": front, "at": [3, -0.1, 1], "width": 1.0, "height": 1.0}));
    let top = *e.model.faces.iter().find(|(&k, _)| e.model.normal(k).z > 0.99).unwrap().0;
    let err = fails(&mut e, json!({"cmd": "opening", "kind": "door", "host": top, "at": [3, 0, 2.7], "width": 0.9, "height": 2.1}));
    assert!(err.contains("side of a wall"), "{err}");
    assert_eq!(e.model.faces, before);
    assert!(e.model.instances.is_empty());
}

#[test]
fn windows_of_one_size_share_a_definition() {
    let mut e = Engine::new();
    let (_, front) = wall(&mut e);
    ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [1.5, -0.1, 1.5], "width": 0.8, "height": 1.0}));
    let front2 = *e.model.faces.iter().find(|(&k, f)| e.model.normal(k).y < -0.99 && e.model.contains(k, DVec3::new(4.5, -0.1, 1.5)) && !f.outer.is_empty()).unwrap().0;
    ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front2, "at": [4.5, -0.1, 1.5], "width": 0.8, "height": 1.0}));
    assert_eq!(e.model.definitions.len(), 1);
}

#[test]
fn a_window_in_a_tool_wall_can_be_resized_and_its_sill_raised() {
    let mut e = Engine::new();
    let (g, front) = wall(&mut e);
    let inst = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [3, -0.1, 1.5], "width": 1.0, "height": 1.0, "sill": 0.9}))["instance"].as_u64().unwrap();
    let info = ok(&mut e, json!({"cmd": "opening_info", "instance": inst}));
    assert_eq!(info["resizable"], true, "{info}");
    assert_eq!(info["sill"], 0.9);
    let r = ok(&mut e, json!({"cmd": "opening_edit", "instance": inst, "width": 2.0, "sill": 1.2}));
    assert_eq!(r["width"], 2.0);
    assert_eq!(r["sill"], 1.2);
    assert!(!wall_at(&e, g, 2.2, 1.5), "the wider hole");
    assert!(wall_at(&e, g, 3.0, 1.0), "wall under the raised sill");
    assert!((e.model.instances[&(inst as u32)].transform.w_axis.x - 2.0).abs() < 1e-9, "still centred where it was");
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["edited"], false);
}

#[test]
fn a_resize_that_does_not_fit_changes_nothing() {
    let mut e = Engine::new();
    let (g, front) = wall(&mut e);
    let inst = ok(&mut e, json!({"cmd": "opening", "kind": "door", "host": front, "at": [3, -0.1, 1], "width": 0.9, "height": 2.1}))["instance"].as_u64().unwrap();
    let faces = wall_faces(&e, g);
    fails(&mut e, json!({"cmd": "opening_edit", "instance": inst, "height": 3.0}));
    assert_eq!(wall_faces(&e, g), faces);
    assert_eq!(ok(&mut e, json!({"cmd": "opening_info", "instance": inst}))["height"], 2.1);
}

#[test]
fn in_a_hand_drawn_wall_it_goes_in_but_cannot_be_resized() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [6, 0.2, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.7}));
    let front = *e.model.faces.iter().find(|(&k, _)| e.model.normal(k).y < -0.99).unwrap().0;
    let inst = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [3, 0, 1.5], "width": 1.0, "height": 1.0}))["instance"].as_u64().unwrap();
    assert_eq!(ok(&mut e, json!({"cmd": "opening_info", "instance": inst}))["resizable"], false);
    let err = fails(&mut e, json!({"cmd": "opening_edit", "instance": inst, "width": 1.5}));
    assert!(err.contains("Wall tool"), "{err}");
}

#[test]
fn undo_takes_a_window_back_out() {
    let mut e = Engine::new();
    let (g, front) = wall(&mut e);
    let faces = wall_faces(&e, g);
    ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [3, -0.1, 1.5], "width": 1.0, "height": 1.0}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(wall_faces(&e, g), faces);
    assert!(e.model.instances.is_empty());
}

#[test]
fn a_planted_definition_with_the_same_name_is_not_reused() {
    let mut e = Engine::new();
    let (_, front) = wall(&mut e);
    // a definition that claims to be a 1 x 1 window but holds something else
    let fake = sketchcraft::model::Definition { name: "Window".into(), model: std::sync::Arc::new(sketchcraft::model::Model::default()), attrs: Default::default(), source: Some("builtin:window@1.000x1.000".into()), cuts: true, face_me: false };
    e.model.definitions.insert(9_000, fake);
    let inst = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": front, "at": [3, -0.1, 1.5], "width": 1.0, "height": 1.0}))["instance"].as_u64().unwrap();
    assert_ne!(e.model.instances[&(inst as u32)].def, 9_000);
}
