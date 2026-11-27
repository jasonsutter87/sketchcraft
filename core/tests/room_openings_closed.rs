//! A door or a window in a room's wall must not be refused by the keep-closed net (0.3.11): the room walls are closed
//! solids, so the net must see them as closed before and after, with rooms alone and sharing a wall.
use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn open_edges(e: &Engine) -> usize {
    // the floor alone in the room's group is open by design; the walls are what must be solid
    e.model.open_edges_by_group().iter().filter(|(g, _)| g.is_some_and(|g| e.model.groups[&g].name == "Walls")).map(|(_, n)| n).sum()
}

fn faces_looking(e: &Engine, d: DVec3, at: f64) -> Vec<u32> {
    e.model.faces.keys().copied().filter(|&f| e.model.normal(f).normalize().dot(d) > 0.9999 && (e.model.center(f).dot(d) - at).abs() < 1e-3 && e.model.face_area(f) > 1.0).collect()
}

#[test]
fn room_walls_are_closed_solids() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "room_rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    assert_eq!(open_edges(&e), 0, "one room");
    run(&mut e, json!({"cmd": "room_rect", "from": [8, 0, 0], "to": [4, 3, 0]}));
    assert_eq!(open_edges(&e), 0, "two rooms sharing a wall: {:?}", e.model.open_edges_by_group());
}

#[test]
fn door_and_window_in_a_room_wall_are_kept() {
    for kind in ["door", "window"] {
        for (dir, at) in [([0.0, 1.0, 0.0], 3.1), ([0.0, -1.0, 0.0], -2.9)] {
            let mut e = Engine::new();
            run(&mut e, json!({"cmd": "room_rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
            run(&mut e, json!({"cmd": "room_rect", "from": [8, 0, 0], "to": [4, 3, 0]}));
            let d = DVec3::from(dir);
            let hosts = faces_looking(&e, d, if dir[2] == 1.0 { 2.7 } else { at });
            let host = hosts.iter().copied().find(|&f| e.model.center(f).x > 4.5).unwrap_or_else(|| panic!("no host {dir:?}: {hosts:?}"));
            let r = e.exec_json(&json!({"cmd": "opening", "kind": kind, "host": host, "at": [6, 3.02, 2.7], "width": 0.9, "height": if kind == "door" { 2.0 } else { 1.2 }, "sill": if kind == "door" { 0.0 } else { 0.9 }}).to_string());
            assert_eq!(r["ok"], true, "{kind} on {dir:?}: {r}");
            assert_eq!(open_edges(&e), 0, "{kind} on {dir:?}: {:?}", e.model.open_edges_by_group());
        }
    }
}

/// What the Room tool sends for a click on a wall: `insert` of a door or window with the wall face as host.
#[test]
fn insert_door_and_window_in_a_shared_room_wall() {
    for asset in ["door", "window"] {
        for dir in [1.0, -1.0] {
            let mut e = Engine::new();
            run(&mut e, json!({"cmd": "room_rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
            run(&mut e, json!({"cmd": "room_rect", "from": [8, 0, 0], "to": [4, 3, 0]}));
            let d = DVec3::new(0.0, dir, 0.0);
            let host = faces_looking(&e, d, dir * if dir > 0.0 { 3.1 } else { 2.9 }).into_iter().find(|&f| e.model.center(f).x > 4.5).unwrap();
            let y = if dir > 0.0 { 3.1 } else { 2.9 };
            let z = if asset == "door" { 0.0 } else { 1.1 };
            let r = e.exec_json(&json!({"cmd": "insert", "asset": asset, "at": [6, y, z], "rotate": 90.0 + 90.0 * dir, "host": host}).to_string());
            assert_eq!(r["ok"], true, "{asset} {dir}: {r}");
            assert!(r["opening"]["error"].is_null(), "{asset} {dir}: {r}");
            assert_eq!(open_edges(&e), 0, "{asset} {dir}: {:?}", e.model.open_edges_by_group());
        }
    }
}

/// A window that pokes through the top of the wall is refused (the net doing its job), and the model is put back.
#[test]
fn a_window_through_the_top_of_a_wall_is_refused() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "room_rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let host = faces_looking(&e, DVec3::Y, 3.1).into_iter().next().unwrap();
    let before = e.model.faces.len();
    let r = e.exec_json(&json!({"cmd": "insert", "asset": "window", "at": [2, 3.1, 1.6], "rotate": 180, "host": host}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("gap"));
    assert_eq!((e.model.faces.len(), open_edges(&e)), (before, 0));
}
