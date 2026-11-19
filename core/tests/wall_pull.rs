//! Wall pull: grab a wall and pull it. Both sides move together, joined walls stretch, doors and windows ride along,
//! a pull that can't be done cleanly is refused with the model untouched, and one Undo puts everything back.
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

/// A closed 4 x 4 m room (path on the 0..4 square), walls 0.2 thick and 2.7 high: the outer faces sit at -0.1 and 4.1.
fn room(e: &mut Engine) -> Value {
    run(e, json!({"cmd": "wall", "points": [[0, 0, 0], [4, 0, 0], [4, 4, 0], [0, 4, 0]], "closed": true, "thickness": 0.2, "height": 2.7}))
}

/// The face looking `dir` whose plane is at `at` along that direction (the outer or inner side of a wall).
fn side(e: &mut Engine, dir: [f64; 3], at: f64) -> u32 {
    let d = DVec3::from(dir);
    let faces = run(e, json!({"cmd": "faces", "facing": dir}))["faces"].as_array().unwrap().clone();
    let f = faces
        .iter()
        .filter(|f| f["normal"].as_array().map(|n| DVec3::new(n[0].as_f64().unwrap(), n[1].as_f64().unwrap(), n[2].as_f64().unwrap()).dot(d) > 0.999).unwrap_or(false))
        .find(|f| {
            let c = f["center"].as_array().unwrap();
            (DVec3::new(c[0].as_f64().unwrap(), c[1].as_f64().unwrap(), c[2].as_f64().unwrap()).dot(d) - at).abs() < 1e-6
        })
        .unwrap_or_else(|| panic!("no face looking {dir:?} at {at}: {faces:?}"));
    f["id"].as_u64().map(|x| x as u32).unwrap()
}

fn snapshot(e: &Engine) -> String {
    format!("{:?} {:?} {:?}", e.model.vertices.iter().map(|(k, v)| (*k, v.p.to_array())).collect::<Vec<_>>(), e.model.instances.iter().map(|(k, i)| (*k, i.transform.to_cols_array())).collect::<Vec<_>>(), e.model.faces.len())
}

fn bounds(e: &Engine, f: u32) -> (DVec3, DVec3) {
    let pts = e.model.pts(&e.model.faces[&f].outer);
    (pts.iter().fold(DVec3::splat(f64::MAX), |a, p| a.min(*p)), pts.iter().fold(DVec3::splat(f64::MIN), |a, p| a.max(*p)))
}

fn all_flat(e: &Engine) {
    for &f in e.model.faces.keys() {
        assert!(e.model.is_flat(f), "face {f} is bent");
    }
}

#[test]
fn pulling_a_wall_of_a_room_out_grows_the_room_and_keeps_thickness() {
    let mut e = Engine::new();
    room(&mut e);
    let west_out = side(&mut e, [-1.0, 0.0, 0.0], 0.1); // looking -x: its plane is x = -0.1
    let west_in = side(&mut e, [1.0, 0.0, 0.0], 0.1); // the inside of the same wall (x = 0.1)
    let south_in = side(&mut e, [0.0, 1.0, 0.0], 0.1); // the inside of the south wall (y = 0.1)
    let before = (e.model.faces.len(), e.model.edges.len());
    let r = run(&mut e, json!({"cmd": "wall_pull", "face": west_out, "by": 1.0}));
    assert_eq!(r["ok"], true);
    // both sides moved a metre, 0.2 apart
    assert!((bounds(&e, west_out).0.x - -1.1).abs() < 1e-9);
    assert!((bounds(&e, west_in).0.x - -0.9).abs() < 1e-9);
    assert!((bounds(&e, west_out).0.x - bounds(&e, west_in).0.x + 0.2).abs() < 1e-9, "thickness kept");
    // the walls on either side of it grew by the same metre, and nothing else moved
    assert!((bounds(&e, south_in).0.x - -0.9).abs() < 1e-9);
    let (lo, hi) = e.model.faces.keys().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(lo, hi), &f| {
        let (a, b) = bounds(&e, f);
        (lo.min(a), hi.max(b))
    });
    assert!((lo.x + 1.1).abs() < 1e-9 && (hi.x - 4.1).abs() < 1e-9 && (hi.y - 4.1).abs() < 1e-9 && (lo.y + 0.1).abs() < 1e-9, "{lo} {hi}");
    assert_eq!((e.model.faces.len(), e.model.edges.len()), before, "no faces or edges were made or lost");
    all_flat(&e);
    // and it works the other way round, from the inside, shrinking the room again
    run(&mut e, json!({"cmd": "wall_pull", "face": west_in, "by": 1.0}));
    assert!((bounds(&e, west_in).0.x - 0.1).abs() < 1e-9);
    assert!((bounds(&e, west_out).0.x - -0.1).abs() < 1e-9);
}

#[test]
fn one_undo_puts_the_room_back_exactly() {
    let mut e = Engine::new();
    room(&mut e);
    let west_out = side(&mut e, [-1.0, 0.0, 0.0], 0.1);
    let before = snapshot(&e);
    run(&mut e, json!({"cmd": "wall_pull", "face": west_out, "by": 1.25}));
    assert_ne!(snapshot(&e), before);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), before);
    run(&mut e, json!({"cmd": "redo"}));
    assert!((bounds(&e, west_out).0.x - -1.35).abs() < 1e-9);
}

#[test]
fn a_wall_that_ends_on_it_follows_a_t_junction() {
    let mut e = Engine::new();
    room(&mut e);
    // a partition drawn on its own, from the inside of the south wall to the inside of the north wall
    let part = run(&mut e, json!({"cmd": "wall", "points": [[2, 0.1, 0], [2, 3.9, 0]], "thickness": 0.2, "height": 2.7}))["group"].as_u64().map(|x| x as u32).unwrap();
    let south_out = side(&mut e, [0.0, -1.0, 0.0], 0.1);
    run(&mut e, json!({"cmd": "wall_pull", "face": south_out, "by": 1.0}));
    // the south wall is a metre further out; the partition's south end went with it, its north end stayed
    let ys: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(part)).map(|v| v.p.y).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    assert!((lo + 0.9).abs() < 1e-9 && (hi - 3.9).abs() < 1e-9, "{lo}..{hi}");
    all_flat(&e);
}

#[test]
fn pulling_the_stem_of_a_t_leaves_the_walls_it_touches_alone() {
    let mut e = Engine::new();
    room(&mut e);
    let part = run(&mut e, json!({"cmd": "wall", "points": [[2, 0.1, 0], [2, 3.9, 0]], "thickness": 0.2, "height": 2.7}))["group"].as_u64().map(|x| x as u32).unwrap();
    // the partition's own east face, pulled half a metre east: it just slides
    let east = e.model.faces.iter().filter(|(_, f)| f.group == Some(part)).map(|(k, _)| *k).find(|&k| e.model.normal(k).x > 0.999).unwrap();
    let walls_before: Vec<[f64; 3]> = e.model.vertices.values().filter(|v| v.group != Some(part)).map(|v| v.p.to_array()).collect();
    run(&mut e, json!({"cmd": "wall_pull", "face": east, "by": 0.5}));
    let walls_after: Vec<[f64; 3]> = e.model.vertices.values().filter(|v| v.group != Some(part)).map(|v| v.p.to_array()).collect();
    assert_eq!(walls_before, walls_after, "a wall that only touches the partition's ends is not dragged");
    assert!((bounds(&e, east).0.x - 2.6).abs() < 1e-9);
}

/// a window on the south wall, centred on x = 2
fn window_south(e: &mut Engine) -> u32 {
    let host = side(e, [0.0, -1.0, 0.0], 0.1);
    let r = run(e, json!({"cmd": "insert", "asset": "window", "at": [2.0, -0.1, 1.0], "rotate": 0, "host": host}));
    assert!(r["opening"]["faces"].as_u64().unwrap_or(0) >= 1, "the window cut its opening: {r}");
    r["instance"].as_u64().map(|x| x as u32).unwrap()
}

#[test]
fn a_window_rides_along_with_its_wall_and_stays_a_hole() {
    let mut e = Engine::new();
    room(&mut e);
    let win = window_south(&mut e);
    let south_out = side(&mut e, [0.0, -1.0, 0.0], 0.1);
    assert_eq!(e.model.faces[&south_out].holes.len(), 1, "the opening is a hole in the wall");
    let at = e.model.instances[&win].transform.transform_point3(DVec3::ZERO);
    run(&mut e, json!({"cmd": "wall_pull", "face": south_out, "by": 1.0}));
    let now = e.model.instances[&win].transform.transform_point3(DVec3::ZERO);
    assert!((now - (at + DVec3::new(0.0, -1.0, 0.0))).length() < 1e-9, "{at} -> {now}");
    // the hole came along: still one hole, still in the wall face, the wall's outside is now at y = -1.1
    assert_eq!(e.model.faces[&south_out].holes.len(), 1);
    assert!((bounds(&e, south_out).0.y + 1.1).abs() < 1e-9);
    let hole = e.model.pts(&e.model.faces[&south_out].holes[0]);
    assert!(hole.iter().all(|p| (p.y + 1.1).abs() < 1e-9), "the hole is still on the wall's face");
    all_flat(&e);
    // one Undo takes the wall and the window back together
    let moved = snapshot(&e);
    run(&mut e, json!({"cmd": "undo"}));
    assert_ne!(snapshot(&e), moved);
    assert!((e.model.instances[&win].transform.transform_point3(DVec3::ZERO) - at).length() < 1e-9);
}

#[test]
fn a_window_in_a_stretched_wall_stays_put_when_it_still_fits() {
    let mut e = Engine::new();
    room(&mut e);
    window_south(&mut e);
    let south_out = side(&mut e, [0.0, -1.0, 0.0], 0.1);
    let hole_before = e.model.pts(&e.model.faces[&south_out].holes[0]);
    // pull the WEST wall out: the south wall gets longer, its window does not move
    let west_out = side(&mut e, [-1.0, 0.0, 0.0], 0.1);
    run(&mut e, json!({"cmd": "wall_pull", "face": west_out, "by": 1.0}));
    assert_eq!(hole_before, e.model.pts(&e.model.faces[&south_out].holes[0]));
    all_flat(&e);
}

#[test]
fn a_pull_that_would_swallow_a_window_is_refused_and_changes_nothing() {
    let mut e = Engine::new();
    room(&mut e);
    window_south(&mut e); // x = 1.4 .. 2.6
    let west_in = side(&mut e, [1.0, 0.0, 0.0], 0.1);
    let before = snapshot(&e);
    // pushing the west wall in by 2 m would cut the south wall short of its window
    let why = fails(&mut e, json!({"cmd": "wall_pull", "face": west_in, "by": 2.0}));
    assert!(why.contains("door or window"), "{why}");
    assert_eq!(snapshot(&e), before);
    // and a pull that would collapse the room is refused too
    let why = fails(&mut e, json!({"cmd": "wall_pull", "face": west_in, "by": 9.0}));
    assert!(why.contains("can't pull that far"), "{why}");
    assert_eq!(snapshot(&e), before);
    // a smaller pull is fine
    run(&mut e, json!({"cmd": "wall_pull", "face": west_in, "by": 1.0}));
}

#[test]
fn what_it_cannot_handle_is_refused_with_a_reason() {
    let mut e = Engine::new();
    // a floor, a roof-like slope and a lone sheet are not walls
    let floor = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    assert!(fails(&mut e, json!({"cmd": "wall_pull", "face": floor, "by": 1.0})).contains("upright"));
    let sheet = run(&mut e, json!({"cmd": "rect", "from": [10, 0, 0], "to": [10, 4, 2.7]}))["face"].as_u64().map(|x| x as u32).unwrap();
    assert!(fails(&mut e, json!({"cmd": "wall_pull", "face": sheet, "by": 1.0})).contains("other side"));
    // two walls drawn end to end in one straight line
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [3, 0, 0], [6, 0, 0]], "thickness": 0.2, "height": 2.7}));
    let before = snapshot(&e);
    let f = e.model.faces.keys().copied().find(|&k| e.model.normal(k).y < -0.999).unwrap();
    let why = fails(&mut e, json!({"cmd": "wall_pull", "face": f, "by": 0.5}));
    assert!(why.contains("straight line"), "{why}");
    assert_eq!(snapshot(&e), before);
    // zero distance
    let mut e = Engine::new();
    room(&mut e);
    let w = side(&mut e, [-1.0, 0.0, 0.0], 0.1);
    assert!(fails(&mut e, json!({"cmd": "wall_pull", "face": w, "by": 0.0})).contains("some distance"));
}

#[test]
fn a_plain_box_shaped_wall_works_and_wall_info_describes_the_plan() {
    let mut e = Engine::new();
    // a hand-made wall: a 6 x 0.3 x 2.7 box, ungrouped
    let base = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [6, 0.3, 0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": base, "distance": 2.7}));
    let front = e.model.faces.keys().copied().find(|&k| e.model.normal(k).y < -0.999).unwrap();
    let info = run(&mut e, json!({"cmd": "wall_info", "face": front}));
    assert_eq!(info["faces"].as_array().unwrap().len(), 6, "the whole box is the wall: both sides, caps, top, bottom");
    assert!((info["thickness"].as_f64().unwrap() - 0.3).abs() < 1e-9);
    assert_eq!(info["moves"].as_array().unwrap().len(), 8);
    run(&mut e, json!({"cmd": "wall_pull", "face": front, "by": 0.8}));
    assert!((bounds(&e, front).0.y + 0.8).abs() < 1e-9);
    // both sides moved: the box is now between y = -0.8 and y = -0.5, still 0.3 thick
    let ys: Vec<f64> = e.model.vertices.values().map(|v| v.p.y).collect();
    assert!((ys.iter().cloned().fold(f64::MIN, f64::max) + 0.5).abs() < 1e-9);
}

#[test]
fn a_click_on_the_top_of_a_wall_finds_the_wall_under_the_cursor() {
    let mut e = Engine::new();
    room(&mut e);
    // the one top face of the ring (it belongs to all four walls)
    let tops = run(&mut e, json!({"cmd": "faces", "facing": [0, 0, 1]}))["faces"].as_array().unwrap().clone();
    let top = tops.iter().find(|f| (f["center"][2].as_f64().unwrap() - 2.7).abs() < 1e-9).unwrap()["id"].as_u64().unwrap();
    // over the west wall, nearer its outside: the outside of the west wall; over the north wall, nearer its inside
    let west = run(&mut e, json!({"cmd": "wall_info", "face": top, "at": [-0.06, 2.0, 2.7]}));
    assert_eq!(west["normal"], json!([-1.0, 0.0, 0.0]));
    let north = run(&mut e, json!({"cmd": "wall_info", "face": top, "at": [2.0, 3.95, 2.7]}));
    assert_eq!(north["normal"], json!([0.0, -1.0, 0.0]), "the inside of the north wall looks south");
    // pulling what it found is the same as pulling that side
    run(&mut e, json!({"cmd": "wall_pull", "face": west["face"], "by": 1.0}));
    // a hand-made box: near its end, the answer is still the long side
    let mut e = Engine::new();
    let base = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1.2, 0.3, 0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": base, "distance": 2.7}));
    let top = e.model.faces.keys().copied().find(|&k| e.model.normal(k).z > 0.999).unwrap();
    let r = run(&mut e, json!({"cmd": "wall_info", "face": top, "at": [1.15, 0.15, 2.7]}));
    assert!((r["thickness"].as_f64().unwrap() - 0.3).abs() < 1e-9, "not the end cap: {r}");
}
