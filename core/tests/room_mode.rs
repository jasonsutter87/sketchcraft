//! Room mode: drag a room out and get four walls and a floor; a room dragged against another shares its wall; an overlap is
//! refused with the model untouched; pulling the shared wall resizes both rooms; one Undo per action.
use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;

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

fn all_flat(e: &Engine) {
    for &f in e.model.faces.keys() {
        assert!(e.model.is_flat(f), "face {f} is bent");
    }
}

/// faces of a group (and the groups inside it)
fn faces_in(e: &Engine, g: u32) -> Vec<u32> {
    let kids: Vec<u32> = e.model.groups.iter().filter(|(_, x)| x.parent == Some(g)).map(|(k, _)| *k).collect();
    e.model.faces.iter().filter(|(_, f)| f.group == Some(g) || f.group.is_some_and(|x| kids.contains(&x))).map(|(k, _)| *k).collect()
}

/// the upright face looking `dir` whose plane is at `at` along it, anywhere in the model
fn side(e: &Engine, dir: [f64; 3], at: f64) -> u32 {
    let d = DVec3::from(dir);
    *e.model
        .faces
        .keys()
        .find(|&&f| e.model.normal(f).normalize().dot(d) > 0.9999 && (e.model.center(f).dot(d) - at).abs() < 1e-6 && e.model.normal(f).z.abs() < 1e-6 && e.model.face_area(f) > 1.0)
        .unwrap_or_else(|| panic!("no wall face looking {dir:?} at {at}"))
}

fn room(e: &mut Engine, a: [f64; 2], b: [f64; 2]) -> Value {
    run(e, json!({"cmd": "room_rect", "from": [a[0], a[1], 0], "to": [b[0], b[1], 0]}))
}

#[test]
fn one_room_is_four_walls_and_a_floor() {
    let mut e = Engine::new();
    let r = room(&mut e, [0.0, 0.0], [4.0, 3.0]);
    assert_eq!(r["name"], "Room 1");
    // the inside is the centre-line rectangle less half a wall each side: 3.8 x 2.8
    assert!((r["width"].as_f64().unwrap() - 3.8).abs() < 1e-9 && (r["depth"].as_f64().unwrap() - 2.8).abs() < 1e-9, "{r}");
    assert!((r["area"].as_f64().unwrap() - 3.8 * 2.8).abs() < 1e-9);
    let g = r["group"].as_u64().unwrap() as u32;
    assert_eq!(e.model.groups[&g].tag.as_deref(), Some("Rooms"));
    let walls = r["walls"].as_array().unwrap();
    assert_eq!(walls.len(), 1, "an unshared room is one closed ring of walls");
    for w in walls {
        assert_eq!(e.model.groups[&(w.as_u64().unwrap() as u32)].tag.as_deref(), Some("Walls"));
    }
    // the floor is one face at the ground, facing up, the size of the inside
    let floor = r["floor"].as_u64().unwrap() as u32;
    assert!(e.model.normal(floor).normalize().z > 0.9999);
    assert!((e.model.face_area(floor) - 3.8 * 2.8).abs() < 1e-9);
    // the ring: an outer wall face on each side (4), an inner one (4), and they are 2.7 high and 0.2 thick
    let n = faces_in(&e, g).len();
    assert!(n > 4 + 4 + 2, "faces in the room: {n}");
    let top = e.model.vertices.values().map(|v| v.p.z).fold(f64::MIN, f64::max);
    assert!((top - 2.7).abs() < 1e-9);
    let outer = side(&e, [0.0, -1.0, 0.0], 0.1);
    assert!((e.model.face_area(outer) - 4.2 * 2.7).abs() < 1e-6, "the outside of the south wall is 4.2 x 2.7");
    // the list of rooms says the same
    let list = rooms(&mut e);
    assert_eq!(list.len(), 1);
    assert!((list[0]["area"].as_f64().unwrap() - 3.8 * 2.8).abs() < 1e-9);
    assert!((list[0]["thickness"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    all_flat(&e);
}

#[test]
fn a_second_room_against_it_shares_the_wall() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let faces_one = e.model.faces.len();
    let r2 = room(&mut e, [4.0, 0.0], [8.0, 4.0]);
    assert_eq!(r2["name"], "Room 2");
    let shared = r2["shared"].as_array().unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0]["side"], "west");
    assert_eq!(shared[0]["name"], "Room 1");
    // no double wall: room 2's walls start at x = 4.1 (the face of room 1's wall), so none of its points is inside that wall
    let r2g = r2["group"].as_u64().unwrap() as u32;
    let inside_slab = e
        .model
        .vertices
        .values()
        .filter(|v| v.group.is_some_and(|g| g == r2g || e.model.groups[&g].parent == Some(r2g)))
        .filter(|v| v.p.x > 3.9 + 1e-6 && v.p.x < 4.1 - 1e-6)
        .count();
    assert_eq!(inside_slab, 0, "room 2 has nothing inside room 1's wall");
    // room 2 has three runs of wall's worth of faces: an open U (south, east, north), so fewer than a full ring's
    let g2 = r2["group"].as_u64().unwrap() as u32;
    let wall_faces: usize = faces_in(&e, g2).len();
    let walls2 = r2["walls"].as_array().unwrap();
    assert_eq!(walls2.len(), 1, "one open run of three walls");
    // an open run of 3 walls (4 path points): 2 long sides x 3 + 2 end caps + top + bottom = 10 wall faces, plus the floor
    assert_eq!(wall_faces, 10 + 1, "wall faces + floor in room 2");
    assert!(e.model.faces.len() > faces_one);
    // its inside: x 4.1..7.9 (3.8), y 0.1..3.9
    let list = rooms(&mut e);
    assert_eq!(list.len(), 2);
    assert!((list[1]["area"].as_f64().unwrap() - 3.8 * 3.8).abs() < 1e-9, "{list:?}");
    all_flat(&e);
}

#[test]
fn a_room_overlapping_another_is_refused_and_changes_nothing() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let before = snapshot(&e);
    let why = fails(&mut e, json!({"cmd": "room_rect", "from": [3.0, 1.0, 0], "to": [7.0, 3.0, 0]}));
    assert!(why.contains("overlaps Room 1"), "{why}");
    assert_eq!(snapshot(&e), before);
    // a room swallowing it, and one cutting through its wall from outside, are refused too
    fails(&mut e, json!({"cmd": "room_rect", "from": [-2.0, -2.0, 0], "to": [6.0, 6.0, 0]}));
    assert_eq!(snapshot(&e), before);
    // too small to be a room
    fails(&mut e, json!({"cmd": "room_rect", "from": [10.0, 0.0, 0], "to": [10.3, 2.0, 0]}));
    assert_eq!(snapshot(&e), before);
    // the preview says the same, without touching anything
    let p = e.exec_json(&json!({"cmd": "room_preview", "from": [3.0, 1.0, 0], "to": [7.0, 3.0, 0]}).to_string());
    assert_eq!(p["ok"], false);
    assert_eq!(snapshot(&e), before);
}

#[test]
fn a_side_near_a_wall_snaps_onto_it() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    // dragged to 4.1 / 3.9 (a wall thickness or less away): it snaps onto the wall line at 4 and shares it
    let p = run(&mut e, json!({"cmd": "room_preview", "from": [4.1, 0.0, 0], "to": [8.0, 4.0, 0]}));
    assert_eq!(p["snapped"], true);
    assert!((p["rect"][0].as_f64().unwrap() - 4.0).abs() < 1e-12);
    let r = run(&mut e, json!({"cmd": "room_rect", "from": [3.9, 0.0, 0], "to": [8.0, 4.0, 0]}));
    assert_eq!(r["shared"].as_array().unwrap().len(), 1);
    assert_eq!(rooms(&mut e).len(), 2);
}

#[test]
fn a_smaller_room_against_a_long_wall_and_a_bigger_one_both_share() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    // a small one against the middle of the east wall: its west side lies fully on the wall
    let small = room(&mut e, [4.0, 1.0], [7.0, 3.0]);
    assert_eq!(small["shared"].as_array().unwrap().len(), 1);
    // a bigger one on the other side: its east side is only partly covered
    let big = room(&mut e, [-4.0, -1.0], [0.0, 6.0]);
    assert_eq!(big["shared"].as_array().unwrap().len(), 1);
    assert!((big["area"].as_f64().unwrap() - 3.8 * 6.8).abs() < 1e-9);
    assert_eq!(rooms(&mut e).len(), 3);
    all_flat(&e);
}

#[test]
fn pulling_the_shared_wall_resizes_both_rooms() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    room(&mut e, [4.0, 0.0], [8.0, 4.0]);
    let a0 = rooms(&mut e);
    // the shared wall's outer face looks east at x = 4.1 (room 1's east wall); pull it half a metre east
    let east = side(&e, [1.0, 0.0, 0.0], 4.1);
    run(&mut e, json!({"cmd": "wall_pull", "face": east, "by": 0.5}));
    let a1 = rooms(&mut e);
    let area = |l: &[Value], i: usize| l[i]["area"].as_f64().unwrap();
    assert!((area(&a1, 0) - 4.3 * 3.8).abs() < 1e-9, "room 1 grew from {} to {}", area(&a0, 0), area(&a1, 0));
    assert!((area(&a1, 1) - 3.3 * 3.8).abs() < 1e-9, "room 2 shrank from {} to {}", area(&a0, 1), area(&a1, 1));
    all_flat(&e);
    // one Undo puts both back
    run(&mut e, json!({"cmd": "undo"}));
    let a2 = rooms(&mut e);
    assert!((area(&a2, 0) - area(&a0, 0)).abs() < 1e-9 && (area(&a2, 1) - area(&a0, 1)).abs() < 1e-9);
}

#[test]
fn one_undo_per_room_restores_exactly() {
    let mut e = Engine::new();
    let empty = snapshot(&e);
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let one = snapshot(&e);
    room(&mut e, [4.0, 0.0], [8.0, 4.0]);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), one, "one Undo removes the whole second room");
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), empty);
    run(&mut e, json!({"cmd": "redo"}));
    assert_eq!(rooms(&mut e).len(), 1);
}

#[test]
fn rooms_in_feet_use_feet_sizes_and_a_door_goes_in_a_room_wall() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "units", "units": "ft"}));
    // 12 ft x 10 ft (centre lines), with 6 in walls: 0.1524 m
    let ft = 0.3048;
    let r = run(&mut e, json!({"cmd": "room_rect", "from": [0, 0, 0], "to": [12.0 * ft, 10.0 * ft, 0], "thickness": 0.5 * ft}));
    let area_ft2 = r["area"].as_f64().unwrap() / (ft * ft);
    assert!((area_ft2 - 11.5 * 9.5).abs() < 1e-6, "{area_ft2} ft2");
    // a door in the south wall, the way the Place tool does it
    let host = side(&e, [0.0, -1.0, 0.0], 0.25 * ft);
    let d = run(&mut e, json!({"cmd": "insert", "asset": "door", "at": [6.0 * ft, -0.25 * ft, 0], "rotate": 0, "host": host}));
    assert!(d["opening"]["faces"].as_u64().unwrap_or(0) >= 1, "the door cut its opening: {d}");
    // pulling that wall takes the door along, and the room's area follows
    let door = d["instance"].as_u64().unwrap() as u32;
    let at = e.model.instances[&door].transform.transform_point3(DVec3::ZERO);
    run(&mut e, json!({"cmd": "wall_pull", "face": host, "by": 2.0 * ft}));
    let now = e.model.instances[&door].transform.transform_point3(DVec3::ZERO);
    assert!((now - (at + DVec3::new(0.0, -2.0 * ft, 0.0))).length() < 1e-9, "the door rode along: {at} -> {now}");
    let list = rooms(&mut e);
    assert!((list[0]["area"].as_f64().unwrap() / (ft * ft) - 11.5 * 11.5).abs() < 1e-6);
    run(&mut e, json!({"cmd": "undo"}));
    assert!((rooms(&mut e)[0]["area"].as_f64().unwrap() / (ft * ft) - 11.5 * 9.5).abs() < 1e-6);
}

#[test]
fn rooms_rename_and_stack_up_names() {
    let mut e = Engine::new();
    let r = room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    let g = r["group"].as_u64().unwrap();
    run(&mut e, json!({"cmd": "rename", "id": g, "name": "Kitchen"}));
    assert_eq!(rooms(&mut e)[0]["name"], "Kitchen");
    assert_eq!(room(&mut e, [5.0, 0.0], [9.0, 4.0])["name"], "Room 1");
    assert_eq!(room(&mut e, [10.0, 0.0], [14.0, 4.0])["name"], "Room 2");
}

#[test]
fn a_wall_in_line_with_a_neighbours_is_refused_with_the_model_untouched() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    room(&mut e, [4.0, 0.0], [8.0, 4.0]);
    let before = snapshot(&e);
    // room 1's south wall carries on as room 2's south wall: pulling one alone would shear the other
    let south = side(&e, [0.0, -1.0, 0.0], 0.1);
    let why = fails(&mut e, json!({"cmd": "wall_pull", "face": south, "by": 0.5}));
    assert!(!why.is_empty(), "{why}");
    assert_eq!(snapshot(&e), before);
}

#[test]
fn rooms_stay_off_each_other_across_a_gap_and_diagonally() {
    let mut e = Engine::new();
    room(&mut e, [0.0, 0.0], [4.0, 4.0]);
    // a gap of 1 m: nothing shared, a ring of its own
    let far = room(&mut e, [5.0, -6.0], [9.0, -2.0]);
    assert!(far["shared"].as_array().unwrap().is_empty());
    // touching only at a corner: allowed, the corner is filled by the first room's wall
    let diag = room(&mut e, [4.0, 4.0], [8.0, 8.0]);
    assert!(diag["shared"].as_array().unwrap().is_empty());
    all_flat(&e);
}
