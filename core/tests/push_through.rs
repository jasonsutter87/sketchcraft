//! A shape drawn on a wall and pushed all the way to the far side opens a hole through the solid, as in other
//! push/pull modelers: the bottom of the recess and the far face it lands on both go (reported on Reddit: "it doesn't
//! push through to the other side cleanly, I still get a face on one side of the cutout").

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// A 4 x 3 x 2.5 box; returns the engine.
fn boxed() -> Engine {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    e
}

/// Faces lying on the plane y = `y` whose area covers point (x, z).
fn faces_covering(e: &Engine, y: f64, x: f64, z: f64) -> Vec<u32> {
    let p = glam::DVec3::new(x, y, z);
    e.model
        .faces
        .iter()
        .filter(|(_, f)| e.model.pts(&f.outer).iter().all(|q| (q.y - y).abs() < 1e-6))
        .filter(|(&id, _)| e.model.contains(id, p))
        .map(|(&id, _)| id)
        .collect()
}

/// Every edge is used by exactly two faces: the solid is closed.
fn closed(e: &Engine) -> bool {
    let mut uses = std::collections::HashMap::<(u32, u32), usize>::new();
    for f in e.model.faces.values() {
        for l in f.loops() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    uses.values().all(|&n| n == 2)
}

#[test]
fn a_window_pushed_to_the_far_wall_opens_a_hole() {
    let mut e = boxed();
    let w = ok(&mut e, json!({"cmd": "rect", "from": [1, 0, 0.5], "to": [3, 0, 2]}))["face"].clone();
    let r = ok(&mut e, json!({"cmd": "pushpull", "face": w, "distance": -3}));
    assert!(faces_covering(&e, 0.0, 2.0, 1.2).is_empty(), "front is open");
    assert!(faces_covering(&e, 3.0, 2.0, 1.2).is_empty(), "back is open too");
    assert_eq!(faces_covering(&e, 3.0, 0.5, 1.2).len(), 1, "the rest of the back wall stays");
    let id = r["face"].as_u64().unwrap() as u32;
    assert!(e.model.faces.contains_key(&id), "the reply names a face that exists");
    assert!(closed(&e), "the solid with its tunnel is closed");
}

#[test]
fn a_door_from_the_bottom_edge_pushed_through_splits_the_far_wall() {
    let mut e = boxed();
    let d = ok(&mut e, json!({"cmd": "rect", "from": [1, 0, 0], "to": [2, 0, 2]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": d, "distance": -3}));
    assert!(faces_covering(&e, 3.0, 1.5, 1.0).is_empty(), "back is open where the door is");
    assert_eq!(faces_covering(&e, 3.0, 3.0, 1.0).len(), 1, "the rest of the back wall stays");
    assert_eq!(faces_covering(&e, 3.0, 1.5, 2.3).len(), 1, "above the door stays");
}

#[test]
fn a_recess_short_of_the_far_wall_leaves_it_alone() {
    let mut e = boxed();
    let w = ok(&mut e, json!({"cmd": "rect", "from": [1, 0, 0.5], "to": [3, 0, 2]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": w, "distance": -1}));
    assert_eq!(faces_covering(&e, 3.0, 2.0, 1.2).len(), 1, "back wall whole");
    assert_eq!(faces_covering(&e, 1.0, 2.0, 1.2).len(), 1, "the recess has a bottom");
    assert!(closed(&e));
}

#[test]
fn undo_brings_the_far_wall_back() {
    let mut e = boxed();
    let w = ok(&mut e, json!({"cmd": "rect", "from": [1, 0, 0.5], "to": [3, 0, 2]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": w, "distance": -3}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(faces_covering(&e, 3.0, 2.0, 1.2).len(), 1);
}

#[test]
fn a_push_past_the_far_wall_stops_there_and_opens_it() {
    for d in [-5.0, -3.0004, -2.9996] {
        let mut e = boxed();
        let w = ok(&mut e, json!({"cmd": "rect", "from": [1, 0, 0.5], "to": [3, 0, 2]}))["face"].clone();
        ok(&mut e, json!({"cmd": "pushpull", "face": w, "distance": d}));
        assert!(faces_covering(&e, 3.0, 2.0, 1.2).is_empty(), "{d}: back is open");
        assert!(e.model.vertices.values().all(|v| v.p.y <= 3.0 + 1e-9), "{d}: nothing sticks out the back");
        assert!(closed(&e), "{d}: closed");
    }
}
