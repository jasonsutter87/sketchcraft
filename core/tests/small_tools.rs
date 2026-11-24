//! Small drawing tools (#25): spline and Bezier curves, make faces, flatten to a height (docs/dev/small-tools.md).
//! Each one is checked for its geometry, for what it reports, and for coming back with one Undo.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::policy::{permits, Caller};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should fail: {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

/// The model as data (the id counter only goes up, even through an undo, so it is left out).
fn snapshot(e: &Engine) -> Value {
    let mut v = serde_json::to_value(&e.model).unwrap();
    v.as_object_mut().unwrap().remove("next_id");
    v
}

/// A closed loop of loose lines from points (no faces, no checks): the line ids.
fn raw_loop(e: &mut Engine, pts: &[[f64; 3]], closed: bool) -> Vec<u32> {
    let v: Vec<u32> = pts.iter().map(|p| e.model.vertex_in(DVec3::from_array(*p), None)).collect();
    let n = if closed { v.len() } else { v.len() - 1 };
    (0..n).map(|i| e.model.edge(v[i], v[(i + 1) % v.len()])).collect()
}

fn sq(x0: f64, y0: f64, x1: f64, y1: f64, z: f64) -> [[f64; 3]; 4] {
    [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]
}

fn area_of(e: &Engine, f: u32) -> f64 {
    e.model.face_area(f)
}

fn undo_restores(e: &mut Engine, before: &Value) {
    run(e, json!({"cmd": "undo"}));
    assert_eq!(&snapshot(e), before, "one Undo puts the model back");
}

// ---- spline -------------------------------------------------------------------------------------------------------

const ZIGZAG: [[f64; 3]; 5] = [[0., 0., 0.], [2., 3., 0.], [5., 1., 1.], [8., 4., 0.5], [10., 0., 0.]];

#[test]
fn a_spline_passes_through_every_point_in_the_requested_number_of_pieces() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "spline", "points": ZIGZAG, "segments": 6}));
    // four spans of six pieces
    assert_eq!(r["edges"].as_array().unwrap().len(), 24, "{r}");
    let at: Vec<DVec3> = e.model.vertices.values().map(|v| v.p).collect();
    for p in ZIGZAG {
        assert!(at.iter().any(|q| q.distance(DVec3::from_array(p)) < 1e-9), "the curve goes through {p:?}");
    }
    // one curve: every edge carries the same curve id, so it is picked and followed as one
    let ids: Vec<_> = e.model.edges.values().map(|x| x.curve).collect();
    assert!(ids[0].is_some() && ids.iter().all(|c| *c == ids[0]), "{ids:?}");
    assert_eq!(r["curve"], json!(ids[0]));
    assert!(r["faces"].as_array().unwrap().is_empty(), "an open curve makes no face");
}

#[test]
fn a_spline_is_smooth_where_a_polyline_through_the_points_is_not() {
    let mut e = Engine::new();
    // the bend at (5, 0): the curve's turn between neighbouring pieces is small everywhere
    run(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0], [5, 0, 0], [5, 5, 0]], "segments": 16}));
    let ps: Vec<DVec3> = {
        let mut ps: Vec<(u32, DVec3)> = e.model.vertices.iter().map(|(k, v)| (*k, v.p)).collect();
        ps.sort_by_key(|x| x.0);
        ps.into_iter().map(|x| x.1).collect()
    };
    assert!(ps.len() >= 3);
    let mut worst = 0.0f64;
    for w in ps.windows(3) {
        let (a, b) = ((w[1] - w[0]).normalize(), (w[2] - w[1]).normalize());
        worst = worst.max(a.dot(b).clamp(-1.0, 1.0).acos());
    }
    assert!(worst < 0.5, "the sharpest turn between pieces is {worst} rad: a right angle would be 1.57");
}

#[test]
fn a_closed_spline_on_one_plane_makes_a_face() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0], [6, 0, 0], [6, 6, 0], [0, 6, 0]], "closed": true, "segments": 8}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 32, "{r}");
    assert_eq!(e.model.faces.len(), 1, "{r}");
    let f = *e.model.faces.keys().next().unwrap();
    // through the four corners, so it bulges out past the 36 m2 square
    let a = area_of(&e, f);
    assert!(a > 36.0 && a < 60.0, "a smooth loop through the four corners bulges past the square: {a}");
    assert_eq!(e.model.faces[&f].outer.len(), 32);
}

#[test]
fn spline_segments_are_clamped_and_two_points_make_a_straight_run() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0], [4, 0, 0]], "segments": 0}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 1);
    let r = run(&mut e, json!({"cmd": "spline", "points": [[0, 1, 0], [4, 1, 0]], "segments": 100000}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 256, "capped at 256 pieces a span");
    assert!(fails(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0]]})).contains("at least 2"));
    assert!(fails(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0], [0, 0, 0]]})).contains("at least 2"));
    assert!(fails(&mut e, json!({"cmd": "spline", "points": [[0, 0, 0], [1, 0, 0]], "closed": true})).contains("at least 3"));
    // too many edges in one command
    let many: Vec<[f64; 3]> = (0..100).map(|i| [i as f64, (i % 7) as f64, 0.0]).collect();
    assert!(fails(&mut e, json!({"cmd": "spline", "points": many, "segments": 256})).contains("most is"));
}

#[test]
fn a_spline_is_one_undo_step() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}));
    let before = snapshot(&e);
    run(&mut e, json!({"cmd": "spline", "points": ZIGZAG}));
    assert_ne!(snapshot(&e), before);
    undo_restores(&mut e, &before);
}

// ---- bezier -------------------------------------------------------------------------------------------------------

#[test]
fn a_cubic_bezier_follows_its_handles() {
    let mut e = Engine::new();
    let ctl = [[0., 0., 0.], [0., 4., 0.], [8., 4., 0.], [8., 0., 0.]];
    let r = run(&mut e, json!({"cmd": "bezier", "points": ctl, "segments": 8}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 8, "{r}");
    let at: Vec<DVec3> = e.model.vertices.values().map(|v| v.p).collect();
    // t = 1/2: (a + 3b + 3c + d) / 8
    let mid = DVec3::new(4.0, 3.0, 0.0);
    assert!(at.iter().any(|q| q.distance(mid) < 1e-9), "the middle of the curve is {mid:?}");
    // the ends are the anchors; the handles are not on the curve
    assert!(at.iter().any(|q| q.distance(DVec3::ZERO) < 1e-9) && at.iter().any(|q| q.distance(DVec3::new(8.0, 0.0, 0.0)) < 1e-9));
    assert!(!at.iter().any(|q| q.distance(DVec3::new(0.0, 4.0, 0.0)) < 1e-6));
    // it never leaves the control polygon's box
    assert!(at.iter().all(|q| q.y >= -1e-9 && q.y <= 4.0 + 1e-9 && q.x >= -1e-9 && q.x <= 8.0 + 1e-9));
    let c: Vec<_> = e.model.edges.values().map(|x| x.curve).collect();
    assert!(c[0].is_some() && c.iter().all(|x| *x == c[0]), "one curve");
}

#[test]
fn chained_beziers_share_their_joint_and_segment_counts_apply_per_piece() {
    let mut e = Engine::new();
    let ctl = [[0., 0., 0.], [1., 2., 0.], [3., 2., 0.], [4., 0., 0.], [5., -2., 0.], [7., -2., 0.], [8., 0., 0.]];
    let r = run(&mut e, json!({"cmd": "bezier", "points": ctl, "segments": 5}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 10, "{r}");
    let at: Vec<DVec3> = e.model.vertices.values().map(|v| v.p).collect();
    assert_eq!(at.iter().filter(|q| q.distance(DVec3::new(4.0, 0.0, 0.0)) < 1e-9).count(), 1, "the joint is one point");
    let r = run(&mut e, json!({"cmd": "bezier", "points": [[0, 9, 0], [1, 9, 0], [2, 9, 0], [3, 9, 0]]}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 16, "the default is 16 pieces a cubic");
}

#[test]
fn a_bezier_needs_the_right_number_of_points_and_a_closed_one_ends_where_it_starts() {
    let mut e = Engine::new();
    assert!(fails(&mut e, json!({"cmd": "bezier", "points": [[0, 0, 0], [1, 1, 0], [2, 1, 0], [3, 0, 0], [4, 0, 0]]})).contains("4, 7, 10"));
    assert!(fails(&mut e, json!({"cmd": "bezier", "points": [[0, 0, 0], [1, 1, 0]]})).contains("4, 7, 10"));
    assert!(fails(&mut e, json!({"cmd": "bezier", "points": [[0, 0, 0], [0, 4, 0], [8, 4, 0], [8, 0, 0]], "closed": true})).contains("closed bezier"));
    // a petal: out and back to the start, closed, makes a face
    let r = run(&mut e, json!({"cmd": "bezier", "points": [[0, 0, 0], [4, 6, 0], [8, 6, 0], [10, 0, 0], [6, -6, 0], [2, -6, 0], [0, 0, 0]], "closed": true, "segments": 10}));
    assert_eq!(r["edges"].as_array().unwrap().len(), 20, "{r}");
    assert_eq!(e.model.faces.len(), 1);
}

#[test]
fn a_bezier_is_one_undo_step() {
    let mut e = Engine::new();
    let before = snapshot(&e);
    run(&mut e, json!({"cmd": "bezier", "points": [[0, 0, 0], [0, 4, 0], [8, 4, 0], [8, 0, 0]]}));
    undo_restores(&mut e, &before);
}

// ---- make faces ---------------------------------------------------------------------------------------------------

#[test]
fn a_loop_of_loose_lines_gets_a_face_and_an_inner_loop_is_a_hole_in_it() {
    let mut e = Engine::new();
    raw_loop(&mut e, &sq(0., 0., 10., 6., 0.), true);
    raw_loop(&mut e, &sq(3., 2., 5., 4., 0.), true);
    assert!(e.model.faces.is_empty());
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    // the plate (with its hole) and the shape inside the hole, as the DXF importer makes them
    assert_eq!(r["created"], 2, "{r}");
    assert_eq!(r["with_holes"], 1);
    assert_eq!(r["holes"], 1);
    let areas: Vec<f64> = r["faces"].as_array().unwrap().iter().map(|f| area_of(&e, f.as_u64().unwrap() as u32)).collect();
    assert!(areas.iter().any(|a| (a - 56.0).abs() < 1e-9), "plate 60 minus the 2 by 2 hole: {areas:?}");
    assert!(areas.iter().any(|a| (a - 4.0).abs() < 1e-9), "{areas:?}");
    for f in e.model.faces.values() {
        assert!(nz(&e, f) > 0.99, "faces look up");
    }
}

#[test]
fn open_holes_leaves_the_inside_of_a_hole_empty_and_rings_alternate() {
    let mut e = Engine::new();
    raw_loop(&mut e, &sq(0., 0., 10., 10., 0.), true);
    raw_loop(&mut e, &sq(1., 1., 9., 9., 0.), true);
    raw_loop(&mut e, &sq(2., 2., 8., 8., 0.), true);
    raw_loop(&mut e, &sq(3., 3., 7., 7., 0.), true);
    let r = run(&mut e, json!({"cmd": "make_faces", "open_holes": true}));
    // faces: outer ring (100 - 64), then the ring 2..8 minus 3..7 (36 - 16); the 1..9 and 3..7 insides are empty
    let mut areas: Vec<f64> = r["faces"].as_array().unwrap().iter().map(|f| area_of(&e, f.as_u64().unwrap() as u32)).collect();
    areas.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(areas.len(), 2, "{r}");
    assert!((areas[0] - 20.0).abs() < 1e-9 && (areas[1] - 36.0).abs() < 1e-9, "{areas:?}");
}

#[test]
fn a_loop_that_is_not_flat_is_skipped_and_reported() {
    let mut e = Engine::new();
    raw_loop(&mut e, &[[0., 0., 0.], [4., 0., 0.], [4., 4., 3.], [0., 4., 0.]], true);
    raw_loop(&mut e, &sq(20., 0., 24., 4., 0.), true);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 1, "only the flat one: {r}");
    assert_eq!(r["non_planar"].as_array().unwrap().len(), 1, "{r}");
    let at = r["non_planar"][0]["at"].as_array().unwrap();
    assert!(at[0].as_f64().unwrap() < 5.0, "the bent loop is the one near the origin: {at:?}");
}

#[test]
fn a_tilted_loop_gets_a_face_on_its_own_plane() {
    let mut e = Engine::new();
    // a loop standing in the x-z plane
    raw_loop(&mut e, &[[0., 0., 0.], [4., 0., 0.], [4., 0., 3.], [0., 0., 3.]], true);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 1, "{r}");
    let f = r["faces"][0].as_u64().unwrap() as u32;
    assert!((area_of(&e, f) - 12.0).abs() < 1e-9);
    assert!(e.model.normal(f).y.abs() > 0.99);
}

#[test]
fn lines_that_cross_are_cut_where_they_cross_and_make_a_face_for_each_region() {
    let mut e = Engine::new();
    // a hash sign: two lines each way, none sharing a point
    raw_loop(&mut e, &[[0., 3., 0.], [10., 3., 0.]], false);
    raw_loop(&mut e, &[[0., 6., 0.], [10., 6., 0.]], false);
    raw_loop(&mut e, &[[3., 0., 0.], [3., 10., 0.]], false);
    raw_loop(&mut e, &[[6., 0., 0.], [6., 10., 0.]], false);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 1, "only the middle square is closed: {r}");
    assert_eq!(r["split"], 8, "{r}");
    let f = r["faces"][0].as_u64().unwrap() as u32;
    assert!((area_of(&e, f) - 9.0).abs() < 1e-9);
    assert_eq!(r["open_edges"], 8, "the eight arms are not part of a loop: {r}");
}

#[test]
fn a_line_ending_on_the_middle_of_a_side_divides_the_loop() {
    let mut e = Engine::new();
    raw_loop(&mut e, &sq(0., 0., 10., 6., 0.), true);
    raw_loop(&mut e, &[[5., 0., 0.], [5., 6., 0.]], false);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 2, "{r}");
    assert_eq!(r["split"], 2, "{r}");
    for f in r["faces"].as_array().unwrap() {
        assert!((area_of(&e, f.as_u64().unwrap() as u32) - 30.0).abs() < 1e-9);
    }
}

#[test]
fn lines_with_loose_ends_are_counted_and_make_no_face() {
    let mut e = Engine::new();
    raw_loop(&mut e, &[[0., 0., 0.], [4., 0., 0.], [4., 4., 0.]], false);
    raw_loop(&mut e, &sq(10., 0., 12., 2., 0.), true);
    raw_loop(&mut e, &[[12., 2., 0.], [15., 5., 0.]], false);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 1, "{r}");
    assert_eq!(r["open_edges"], 3, "the L and the spur: {r}");
}

#[test]
fn running_it_again_changes_nothing_and_faces_that_exist_are_left_alone() {
    let mut e = Engine::new();
    raw_loop(&mut e, &sq(0., 0., 4., 4., 0.), true);
    let first = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(first["created"], 1);
    let again = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(again["created"], 0, "the lines of a face are not loose: {again}");
    assert_eq!(again["unchanged"], true);
    assert_eq!(e.model.faces.len(), 1);
    // asking for the lines by id finds the existing face and says so
    let ids: Vec<u32> = e.model.edges.keys().copied().collect();
    let named = run(&mut e, json!({"cmd": "make_faces", "ids": ids}));
    assert_eq!(named["created"], 0);
    assert_eq!(named["existing"], 1, "{named}");
}

#[test]
fn make_faces_takes_lines_or_groups_and_nothing_else() {
    let mut e = Engine::new();
    let a = raw_loop(&mut e, &sq(0., 0., 4., 4., 0.), true);
    let b = raw_loop(&mut e, &sq(10., 0., 14., 4., 0.), true);
    let r = run(&mut e, json!({"cmd": "make_faces", "ids": a}));
    assert_eq!(r["created"], 1, "only the lines named: {r}");
    let edge_of_b = b[0];
    let r = run(&mut e, json!({"cmd": "make_faces", "ids": [edge_of_b]}));
    assert_eq!(r["created"], 0, "one side of a loop is not a loop: {r}");
    let f = *e.model.faces.keys().next().unwrap();
    assert!(fails(&mut e, json!({"cmd": "make_faces", "ids": [f]})).contains("not a line or a group"));
    // a group's loose lines
    run(&mut e, json!({"cmd": "begin_group", "name": "CAD"}));
    run(&mut e, json!({"cmd": "line", "points": [[20, 0, 0], [24, 0, 0], [24, 4, 0], [20, 4, 0]]}));
    run(&mut e, json!({"cmd": "end_group"}));
    let g = *e.model.groups.keys().next().unwrap();
    let r = run(&mut e, json!({"cmd": "make_faces", "ids": [g]}));
    assert_eq!(r["created"], 0, "drawn with line, closing it made the face already: {r}");
}

#[test]
fn make_faces_is_one_undo_step() {
    let mut e = Engine::new();
    raw_loop(&mut e, &sq(0., 0., 10., 6., 0.), true);
    raw_loop(&mut e, &sq(3., 2., 5., 4., 0.), true);
    raw_loop(&mut e, &[[5., 0., 0.], [5., 6., 0.]], false);
    let before = snapshot(&e);
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert!(r["created"].as_u64().unwrap() >= 2);
    undo_restores(&mut e, &before);
}

// ---- flatten ------------------------------------------------------------------------------------------------------

#[test]
fn flatten_moves_the_points_of_the_lines_to_one_height() {
    let mut e = Engine::new();
    let ids = raw_loop(&mut e, &[[0., 0., 0.], [4., 0., 1.], [4., 4., 2.], [0., 4., 3.]], true);
    let r = run(&mut e, json!({"cmd": "flatten", "ids": ids, "z": 2.5}));
    assert_eq!(r["moved"], 4, "{r}");
    assert_eq!(r["merged"], 0);
    assert!(e.model.vertices.values().all(|v| v.p.z == 2.5));
    assert_eq!(e.model.edges.len(), 4);
    // x and y are untouched
    let mut xy: Vec<(i64, i64)> = e.model.vertices.values().map(|v| (v.p.x as i64, v.p.y as i64)).collect();
    xy.sort();
    assert_eq!(xy, vec![(0, 0), (0, 4), (4, 0), (4, 4)]);
}

#[test]
fn flatten_joins_points_that_land_together_and_drops_what_collapses() {
    let mut e = Engine::new();
    // a post drawn as a line straight up, with a square at its foot and another at its top
    let mut ids = raw_loop(&mut e, &sq(0., 0., 2., 2., 0.), true);
    ids.extend(raw_loop(&mut e, &sq(0., 0., 2., 2., 5.), true));
    ids.extend(raw_loop(&mut e, &[[0., 0., 0.], [0., 0., 5.]], false));
    assert_eq!(e.model.vertices.len(), 8);
    let r = run(&mut e, json!({"cmd": "flatten", "ids": ids, "z": 0}));
    assert_eq!(r["merged"], 4, "{r}");
    assert_eq!(e.model.vertices.len(), 4);
    // the 4 + 4 sides lie on top of each other and the post has no length: 9 lines become 4
    assert_eq!(e.model.edges.len(), 4, "{r}");
    assert_eq!(r["edges_removed"], 5, "{r}");
    for ed in e.model.edges.values() {
        assert_ne!(ed.a, ed.b);
    }
}

#[test]
fn flatten_drops_faces_that_collapse_and_turns_the_rest_to_face_up() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "begin_group", "name": "Box"}));
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]}));
    let top = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 3], "to": [4, 4, 3]}))["face"].as_u64().unwrap() as u32;
    run(&mut e, json!({"cmd": "polygon", "points": [[0, 0, 0], [4, 0, 0], [4, 0, 3], [0, 0, 3]]}));
    run(&mut e, json!({"cmd": "reverse", "ids": [top]}));
    run(&mut e, json!({"cmd": "end_group"}));
    let g = *e.model.groups.keys().next().unwrap();
    let faces_before = e.model.faces.len();
    assert!(faces_before >= 3);
    let r = run(&mut e, json!({"cmd": "flatten", "ids": [g], "z": 0}));
    assert!(r["faces_removed"].as_u64().unwrap() >= 1, "the wall seen from above is a line: {r}");
    assert!(e.model.faces.len() < faces_before);
    assert!(e.model.faces.values().all(|f| nz(&e, f) > 0.99), "every face left looks up");
    assert!(e.model.vertices.values().all(|v| v.p.z == 0.0));
}

#[test]
fn flatten_without_ids_takes_everything_and_leaves_locked_groups_alone() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "line", "points": [[0, 0, 1], [3, 0, 2]]}));
    run(&mut e, json!({"cmd": "begin_group", "name": "Keep"}));
    run(&mut e, json!({"cmd": "line", "points": [[0, 5, 1], [3, 5, 2]]}));
    run(&mut e, json!({"cmd": "end_group"}));
    let g = *e.model.groups.keys().next().unwrap();
    run(&mut e, json!({"cmd": "lock", "ids": [g]}));
    let r = run(&mut e, json!({"cmd": "flatten", "z": 0}));
    assert_eq!(r["moved"], 2, "{r}");
    let (flat, kept): (Vec<_>, Vec<_>) = e.model.vertices.values().partition(|v| v.group.is_none());
    assert!(flat.iter().all(|v| v.p.z == 0.0));
    assert!(kept.iter().any(|v| v.p.z == 1.0) && kept.iter().any(|v| v.p.z == 2.0));
    // asking for the locked group by name is refused
    assert!(fails(&mut e, json!({"cmd": "flatten", "ids": [g], "z": 0})).contains("locked"));
}

#[test]
fn flatten_refuses_what_it_cannot_do_and_changes_nothing_then() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "line", "points": [[0, 0, 1], [3, 0, 2]]}));
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd": "flatten", "ids": [99999], "z": 0})).contains("not a point"));
    let m = fails(&mut e, json!({"cmd": "flatten", "z": 1e30})); assert!(m.contains("height") || m.contains("range"), "{m}");
    assert_eq!(snapshot(&e), before);
    // already flat: nothing to do
    let ids: Vec<u32> = e.model.edges.keys().copied().collect();
    run(&mut e, json!({"cmd": "flatten", "ids": ids.clone(), "z": 1.5}));
    let r = run(&mut e, json!({"cmd": "flatten", "ids": ids, "z": 1.5}));
    assert_eq!(r["unchanged"], true, "{r}");
}

#[test]
fn flatten_is_one_undo_step() {
    let mut e = Engine::new();
    let mut ids = raw_loop(&mut e, &sq(0., 0., 2., 2., 0.), true);
    ids.extend(raw_loop(&mut e, &sq(0., 0., 2., 2., 5.), true));
    let before = snapshot(&e);
    let r = run(&mut e, json!({"cmd": "flatten", "ids": ids, "z": 1}));
    assert!(r["merged"].as_u64().unwrap() > 0);
    undo_restores(&mut e, &before);
}

// ---- imported CAD linework ----------------------------------------------------------------------------------------

#[test]
fn imported_3d_linework_becomes_faces_after_flatten_and_make_faces() {
    let mut e = Engine::new();
    let path = format!("{}/tests/data/plate_3d_linework.dxf", env!("CARGO_MANIFEST_DIR"));
    let r = run(&mut e, json!({"cmd": "import_dxf", "path": path}));
    assert_eq!(r["faces"], 0, "none of it is flat, so the importer made no faces: {r}");
    assert_eq!(e.model.faces.len(), 0);
    let lines = e.model.edges.len();
    assert_eq!(lines, 9, "plate, hole and the stray line");
    let after_import = snapshot(&e);

    // make faces on the raw drawing: every loop is bent a little, so nothing is made and each piece is reported
    let r = run(&mut e, json!({"cmd": "make_faces"}));
    assert_eq!(r["created"], 0, "{r}");
    assert_eq!(r["non_planar"].as_array().unwrap().len(), 2, "the plate and the hole, on one layer (the stray line hangs off the plate): {r}");

    // flatten everything the import made, then make faces
    let group = r_group(&after_import);
    let r = run(&mut e, json!({"cmd": "flatten", "ids": [group], "z": 0}));
    assert!(r["moved"].as_u64().unwrap() >= 8, "{r}");
    assert!(e.model.vertices.values().all(|v| v.p.z == 0.0));
    let r = run(&mut e, json!({"cmd": "make_faces", "ids": [group]}));
    // a plate with a hole, and the piece inside the hole; the stray line is reported as open
    assert_eq!(r["created"], 2, "{r}");
    assert_eq!(r["with_holes"], 1);
    assert_eq!(r["open_edges"], 1, "{r}");
    let mut areas: Vec<f64> = r["faces"].as_array().unwrap().iter().map(|f| area_of(&e, f.as_u64().unwrap() as u32)).collect();
    areas.sort_by(|a, b| a.total_cmp(b));
    assert!((areas[0] - 4.0).abs() < 1e-6 && (areas[1] - 56.0).abs() < 1e-6, "{areas:?}");

    // flatten and make faces are one undo each (the make faces that found nothing to do left no step); two undos are back at the import
    for _ in 0..2 {
        run(&mut e, json!({"cmd": "undo"}));
    }
    assert_eq!(snapshot(&e), after_import);
}

/// The id of the group the DXF import made.
fn r_group(model: &Value) -> u32 {
    let groups = model["groups"].as_object().unwrap();
    // the outer group has the layer groups inside it: it is the one with no parent
    groups.iter().find(|(_, g)| g["parent"].is_null()).map(|(k, _)| k.parse().unwrap()).unwrap()
}

// ---- agents -------------------------------------------------------------------------------------------------------

#[test]
fn agents_and_plugins_may_run_all_four() {
    for c in ["spline", "bezier", "make_faces", "flatten"] {
        assert!(permits(&Caller::Ui, c), "{c}");
        assert!(permits(&Caller::Plugin, c), "{c}");
        assert!(permits(&Caller::Mcp(sketchcraft::policy::Workspace::at(std::env::temp_dir())), c), "{c}");
    }
}

/// How far up a face looks (1: straight up).
fn nz(e: &Engine, f: &sketchcraft::model::Face) -> f64 {
    sketchcraft::geom::newell(&e.model.pts(&f.outer)).z
}

/// The same numbers are checked in app/tests/curvemath.test.ts: the Curve tool preview is the engines curve.
#[test]
fn spline_reference_values() {
    let pts = [DVec3::new(0., 0., 0.), DVec3::new(4., 3., 0.), DVec3::new(8., 0., 1.)];
    let open = sketchcraft::curves::spline_points(&pts, false, 2).unwrap();
    let closed = sketchcraft::curves::spline_points(&pts, true, 2).unwrap();
    let near = |got: &[DVec3], want: &[[f64; 3]]| {
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!(g.distance(DVec3::from_array(*w)) < 1e-9, "{g:?} vs {w:?}");
        }
    };
    near(&open, &[[0., 0., 0.], [2.0024273571957143, 1.871341025903462, -0.06158675133444605], [4., 3., 0.], [6.002475428341481, 1.8786950274558634, 0.4371935906333746], [8., 0., 1.]]);
    near(
        &closed,
        &[
            [0., 0., 0.],
            [1.4351977464294634, 1.7061299373954504, -0.10495527126222551],
            [4., 3., 0.],
            [6.576255683269741, 1.7125771720767702, 0.5366024317292559],
            [8., 0., 1.],
            [3.9925959761097607, -0.5290502671959056, 0.5872495415463711],
            [0., 0., 0.],
        ],
    );
}
