//! Room booleans: drag a room over another and combine with it (union: an L or other compound outline) or cut a notch out of
//! it (subtract). Walls are rebuilt along the new outline, other rooms' walls stay, doors and windows keep or lose their wall,
//! refusals leave the model untouched, and each is one Undo.
use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;
use std::collections::BTreeSet;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should have been refused: {r}");
    r["error"].as_str().unwrap().to_string()
}

fn snapshot(e: &Engine) -> String {
    format!(
        "{:?} {:?} {:?} {:?}",
        e.model.vertices.iter().map(|(k, v)| (*k, v.p.to_array())).collect::<Vec<_>>(),
        e.model.faces.keys().collect::<Vec<_>>(),
        e.model.groups.iter().map(|(k, g)| (*k, g.name.clone(), g.tag.clone())).collect::<Vec<_>>(),
        e.model.instances.iter().map(|(k, i)| (*k, i.transform.to_cols_array())).collect::<Vec<_>>()
    )
}

fn rooms(e: &mut Engine) -> Vec<Value> {
    run(e, json!({"cmd": "rooms"}))["rooms"].as_array().unwrap().clone()
}

fn room(e: &mut Engine, a: [f64; 2], b: [f64; 2]) -> Value {
    run(e, json!({"cmd": "room_rect", "from": [a[0], a[1], 0], "to": [b[0], b[1], 0]}))
}

fn boolean(e: &mut Engine, mode: &str, a: [f64; 2], b: [f64; 2]) -> Value {
    run(e, json!({"cmd": "room_rect", "mode": mode, "from": [a[0], a[1], 0], "to": [b[0], b[1], 0]}))
}

fn all_flat(e: &Engine) {
    for &f in e.model.faces.keys() {
        assert!(e.model.is_flat(f), "face {f} is bent");
    }
}

/// faces of a group (and the groups inside it)
fn faces_in(e: &Engine, g: u32) -> BTreeSet<u32> {
    let kids: Vec<u32> = e.model.groups.iter().filter(|(_, x)| x.parent == Some(g)).map(|(k, _)| *k).collect();
    e.model.faces.iter().filter(|(_, f)| f.group == Some(g) || f.group.is_some_and(|x| kids.contains(&x))).map(|(k, _)| *k).collect()
}

fn side_opt(e: &Engine, dir: [f64; 3], at: f64) -> Option<u32> {
    let d = DVec3::from(dir);
    e.model
        .faces
        .keys()
        .find(|&&f| e.model.normal(f).normalize().dot(d) > 0.9999 && (e.model.center(f).dot(d) - at).abs() < 1e-6 && e.model.normal(f).z.abs() < 1e-6 && e.model.face_area(f) > 0.5)
        .copied()
}

fn area(v: &Value) -> f64 {
    v["area"].as_f64().unwrap()
}

fn poly_area(v: &Value) -> f64 {
    let p: Vec<[f64; 2]> = v.as_array().unwrap().iter().map(|q| [q[0].as_f64().unwrap(), q[1].as_f64().unwrap()]).collect();
    (0..p.len()).map(|i| p[i][0] * p[(i + 1) % p.len()][1] - p[(i + 1) % p.len()][0] * p[i][1]).sum::<f64>() / 2.0
}

fn door(e: &mut Engine, at: [f64; 3], rotate: f64, host: u32) -> Value {
    run(e, json!({"cmd": "insert", "asset": "door", "at": at, "rotate": rotate, "host": host}))
}

#[test]
fn combining_two_overlapping_rooms_makes_one_l_room() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let before = snapshot(&e);
    let r = boolean(&mut e, "combine", [2.0, 2.0], [6.0, 6.0]);
    assert_eq!(r["mode"], "combine");
    assert_eq!(r["name"], "Room 1", "the existing room keeps its name");
    assert_eq!(r["target"]["name"], "Room 1");
    // the outline of the union's centre lines is an L-ish staircase of 8 corners; the inside is 0.1 m in all round
    assert_eq!(r["centre_outline"].as_array().unwrap().len(), 8);
    assert!((area(&r) - (28.0 - 0.1 * 24.0 + 0.01 * 4.0)).abs() < 1e-9, "{r}");
    assert!((poly_area(&r["outline"]) - area(&r)).abs() < 1e-9);
    // one room, one ring of walls (no wall inside the overlap), a floor of that area
    let list = rooms(&mut e);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Room 1");
    assert!((area(&list[0]) - area(&r)).abs() < 1e-9);
    assert_eq!(list[0]["outline"].as_array().unwrap().len(), 8);
    assert_eq!(r["walls"].as_array().unwrap().len(), 1);
    let floor = r["floor"].as_u64().unwrap() as u32;
    assert!((e.model.face_area(floor) - area(&r)).abs() < 1e-9);
    assert!(e.model.faces.keys().filter(|&&f| e.model.normal(f).z.abs() > 0.9999 && e.model.faces[&f].group.is_some_and(|g| e.model.groups[&g].name == "Room 1")).count() == 1);
    // no wall left where the two rooms overlapped: nothing stands inside the old east wall's line within the overlap
    assert!(e.model.vertices.values().all(|v| !(v.p.z > 0.1 && (v.p.x - 4.0).abs() < 0.11 && v.p.y > 2.2 && v.p.y < 3.8)), "an inner wall is left");
    // the label sits inside the L, not at the bounding box's middle (which is in the notch)
    let c = &list[0]["center"];
    let (cx, cy) = (c[0].as_f64().unwrap(), c[1].as_f64().unwrap());
    assert!(!(cx > 4.1 && cy < 1.9) && !(cx < 1.9 && cy > 4.1), "label ({cx}, {cy}) is in the notch");
    all_flat(&e);
    // ONE undo undoes the whole thing; redo brings it back
    let after = snapshot(&e);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), before);
    run(&mut e, json!({"cmd": "redo"}));
    assert_eq!(snapshot(&e), after);
}

#[test]
fn subtracting_cuts_a_notch_and_the_walls_follow() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let before = snapshot(&e);
    let r = boolean(&mut e, "subtract", [3.0, 1.0], [6.0, 3.0]);
    assert_eq!(r["mode"], "subtract");
    assert_eq!(r["centre_outline"].as_array().unwrap().len(), 8);
    assert!((area(&r) - (14.0 - 0.1 * 18.0 + 0.01 * 4.0)).abs() < 1e-9, "{r}");
    let list = rooms(&mut e);
    assert_eq!(list.len(), 1);
    assert!((area(&list[0]) - area(&r)).abs() < 1e-9);
    // the notch's walls are there (a face looking east at x = 2.9 is the notch's west wall seen from inside the notch: the
    // outer side of that wall, x = 3.1 looking +x)
    assert!(side_opt(&e, [1.0, 0.0, 0.0], 3.1).is_some(), "the notch has a wall along x = 3");
    // and no wall is left across the notch's mouth on the old east side between y = 1.1 and 2.9
    assert!(e.model.vertices.values().all(|v| !(v.p.z > 0.1 && v.p.x > 3.2 && v.p.y > 1.2 && v.p.y < 2.8)));
    all_flat(&e);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), before);
}

#[test]
fn a_subtract_that_trims_a_corner_leaves_an_l() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [6.0, 6.0]);
    let r = boolean(&mut e, "subtract", [3.0, 3.0], [9.0, 9.0]);
    assert_eq!(r["centre_outline"].as_array().unwrap().len(), 6);
    assert!((poly_area(&r["centre_outline"]) - 27.0).abs() < 1e-9);
    all_flat(&e);
}

#[test]
fn doors_on_a_wall_that_survives_are_kept_and_the_rest_removed() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    // a door in the south wall (outer face y = -0.1), and one in the east wall's middle (outer face x = 4.1)
    let south = side_opt(&e, [0.0, -1.0, 0.0], -0.1).or_else(|| side_opt(&e, [0.0, -1.0, 0.0], 0.1)).expect("south wall face");
    let south_plane = e.model.center(south).y;
    let d1 = door(&mut e, [2.0, south_plane, 0.0], 0.0, south);
    assert!(d1["opening"]["faces"].as_u64().unwrap_or(0) >= 1, "{d1}");
    let east = side_opt(&e, [1.0, 0.0, 0.0], 4.1).expect("east wall face");
    let d2 = door(&mut e, [4.1, 2.0, 0.0], 90.0, east);
    assert!(d2["opening"]["faces"].as_u64().unwrap_or(0) >= 1, "{d2}");
    assert_eq!(e.model.instances.len(), 2);
    let south_with_door = e.model.face_area(south);
    let before = snapshot(&e);

    // the notch takes the east wall's middle: the east door goes, the south door stays and its opening is cut again
    let r = boolean(&mut e, "subtract", [3.0, 1.0], [6.0, 3.0]);
    assert_eq!(r["kept_openings"], 1, "{r}");
    assert_eq!(r["removed_openings"], 1, "{r}");
    assert_eq!(e.model.instances.len(), 1);
    let new_south = side_opt(&e, [0.0, -1.0, 0.0], -south_plane).expect("a south wall face");
    assert!((e.model.face_area(new_south) - south_with_door).abs() < 1e-6, "the south wall still has its door opening: {} vs {}", e.model.face_area(new_south), south_with_door);
    all_flat(&e);

    // one Undo brings both doors back
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), before);
    assert_eq!(e.model.instances.len(), 2);
}

#[test]
fn another_rooms_walls_stay_and_a_shared_wall_is_not_built_twice() {
    let mut e = Engine::new();
    let r1 = room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let r2 = room(&mut e, [4.0, 0.0], [8.0, 4.0]);
    let (g1, g2) = (r1["group"].as_u64().unwrap() as u32, r2["group"].as_u64().unwrap() as u32);
    let faces2 = faces_in(&e, g2);
    let a2 = area(&rooms(&mut e)[1]);

    // Room 1 grows a bay to the north: its east wall (shared with Room 2, and Room 1's own) is rebuilt, Room 2 is untouched
    let r = boolean(&mut e, "combine", [1.0, 3.0], [3.0, 6.0]);
    assert_eq!(r["target"]["name"], "Room 1");
    assert_eq!(faces_in(&e, g2), faces2, "Room 2's walls and floor were not touched");
    assert!(faces_in(&e, g1).len() > 10);
    let list = rooms(&mut e);
    assert_eq!(list.len(), 2);
    assert!((area(&list[1]) - a2).abs() < 1e-9);
    assert!(side_opt(&e, [1.0, 0.0, 0.0], 4.1).is_some(), "the shared east wall is still there");
    all_flat(&e);

    // Room 2 grows a bay too: its west side runs along Room 1's wall, which is not rebuilt or doubled
    let faces1 = faces_in(&e, g1);
    let r = boolean(&mut e, "combine", [6.0, 3.0], [7.0, 6.0]);
    assert_eq!(r["target"]["name"], "Room 2");
    assert_eq!(faces_in(&e, g1), faces1, "Room 1's walls and floor were not touched");
    assert!(r["shared"].as_array().unwrap().iter().any(|s| s["name"] == "Room 1"));
    let inside_slab = e.model.vertices.values().filter(|v| v.group.is_some_and(|g| g == g2 || e.model.groups[&g].parent == Some(g2))).filter(|v| v.p.x > 3.9 + 1e-6 && v.p.x < 4.1 - 1e-6).count();
    assert_eq!(inside_slab, 0, "Room 2 has nothing inside Room 1's wall");
    all_flat(&e);
}

#[test]
fn a_cut_that_would_take_away_a_wall_another_room_leans_on_is_refused() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    room(&mut e, [4.0, 0.0], [8.0, 4.0]); // leans on Room 1's east wall
    let before = snapshot(&e);
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "subtract", "from": [3.0, 1.0, 0], "to": [4.0, 3.0, 0]}));
    assert!(why.contains("shares"), "{why}");
    assert_eq!(snapshot(&e), before);
}

#[test]
fn refusals_say_why_and_change_nothing() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [6.0, 6.0]);
    let before = snapshot(&e);
    // a hole
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "subtract", "from": [2.0, 2.0, 0], "to": [4.0, 4.0, 0]}));
    assert!(why.contains("hole"), "{why}");
    // a cut clean across splits the room
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "subtract", "from": [2.0, -2.0, 0], "to": [3.0, 8.0, 0]}));
    assert!(why.contains("two"), "{why}");
    // the whole room
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "subtract", "from": [-1.0, -1.0, 0], "to": [7.0, 7.0, 0]}));
    assert!(why.contains("nothing"), "{why}");
    // nothing to cut
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "subtract", "from": [10.0, 10.0, 0], "to": [12.0, 12.0, 0]}));
    assert!(why.contains("over a room"), "{why}");
    // the default mode still refuses an overlap
    let why = fails(&mut e, json!({"cmd": "room_rect", "from": [3.0, 1.0, 0], "to": [9.0, 3.0, 0]}));
    assert!(why.contains("overlaps Room 1"), "{why}");
    assert_eq!(snapshot(&e), before);

    // a box over two rooms
    room(&mut e, [6.0, 0.0], [12.0, 6.0]);
    let before = snapshot(&e);
    let why = fails(&mut e, json!({"cmd": "room_rect", "mode": "combine", "from": [4.0, 2.0, 0], "to": [8.0, 4.0, 0]}));
    assert!(why.contains("one room at a time"), "{why}");
    assert_eq!(snapshot(&e), before);
}

#[test]
fn combine_over_bare_ground_is_a_new_room_and_preview_says_what_would_happen() {
    let mut e = Engine::new();
    let r = boolean(&mut e, "combine", [0.0, 0.0], [4.0, 4.0]);
    assert_eq!(r["mode"], "new");
    assert_eq!(r["name"], "Room 1");
    let before = snapshot(&e);
    let p = run(&mut e, json!({"cmd": "room_preview", "mode": "combine", "from": [2.0, 2.0, 0], "to": [6.0, 6.0, 0]}));
    assert_eq!(p["mode"], "combine");
    assert_eq!(p["target"]["name"], "Room 1");
    assert_eq!(p["outline"].as_array().unwrap().len(), 8);
    assert!((poly_area(&p["outline"]) - area(&p)).abs() < 1e-9);
    let lab = (p["label"][0].as_f64().unwrap(), p["label"][1].as_f64().unwrap());
    assert!(lab.0 > 0.1 && lab.1 > 0.1 && !(lab.0 > 4.1 && lab.1 < 1.9), "{lab:?}");
    let s = run(&mut e, json!({"cmd": "room_preview", "mode": "subtract", "from": [2.0, 2.0, 0], "to": [6.0, 6.0, 0]}));
    assert_eq!(s["outline"].as_array().unwrap().len(), 6);
    let why = e.exec_json(&json!({"cmd": "room_preview", "mode": "subtract", "from": [1.5, 1.5, 0], "to": [2.5, 2.5, 0]}).to_string());
    assert_eq!(why["ok"], false);
    assert!(why["error"].as_str().unwrap().contains("hole"));
    // the default preview still says an overlap is refused
    let old = e.exec_json(&json!({"cmd": "room_preview", "from": [2.0, 2.0, 0], "to": [6.0, 6.0, 0]}).to_string());
    assert_eq!(old["ok"], false);
    assert_eq!(snapshot(&e), before, "previews change nothing");
}

#[test]
fn a_combined_room_can_be_combined_again_and_pulled_like_any_room() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    boolean(&mut e, "combine", [2.0, 3.0], [4.0, 7.0]); // an L (a bay to the north on the east half)
    let r = boolean(&mut e, "combine", [-2.0, 1.0], [1.0, 3.0]); // and a bay to the west
    assert_eq!(rooms(&mut e).len(), 1);
    assert_eq!(r["centre_outline"].as_array().unwrap().len(), 10);
    assert!((poly_area(&r["centre_outline"]) - 26.0).abs() < 1e-9, "{r}");
    all_flat(&e);
    // and a notch from the bay
    let c = boolean(&mut e, "subtract", [3.0, 5.0], [6.0, 8.0]);
    assert_eq!(c["mode"], "subtract");
    all_flat(&e);
}
