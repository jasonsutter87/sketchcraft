//! Fix model: one command that repairs a messy model and lists what it changed (see docs/dev/fix-model.md).
//! Each fixture is a unit cube (or two) broken in one way; each test shows the problem is found, fixed, listed, and that
//! undo brings the broken model back.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::fix::{FixOptions, FixScope};
use sketchcraft::model::Face;
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// Add a face straight from vertex ids (no merging or splitting, so a fixture can be as broken as it likes).
fn raw_face(e: &mut Engine, lp: &[u32]) -> u32 {
    for i in 0..lp.len() {
        e.model.edge(lp[i], lp[(i + 1) % lp.len()]);
    }
    e.model.next_id += 1;
    let id = e.model.next_id;
    e.model.faces.insert(id, Face { outer: lp.to_vec(), ..Default::default() });
    id
}

fn verts(e: &mut Engine, pts: &[[f64; 3]]) -> Vec<u32> {
    pts.iter().map(|p| e.model.vertex_in(DVec3::from_array(*p), None)).collect()
}

const CUBE: [[f64; 3]; 8] = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]];
/// bottom, top, front, right, back, left: all outward
const CUBE_FACES: [[usize; 4]; 6] = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];

/// A unit cube from raw faces: its vertex ids and face ids (in the order of `CUBE_FACES`).
fn cube(e: &mut Engine) -> (Vec<u32>, Vec<u32>) {
    let v = verts(e, &CUBE);
    let f = CUBE_FACES.iter().map(|q| raw_face(e, &q.iter().map(|&i| v[i]).collect::<Vec<_>>())).collect();
    (v, f)
}

fn fix(e: &mut Engine) -> Value {
    run(e, json!({"cmd": "fix_model"}))
}

fn ids(report: &Value, key: &str) -> Vec<u64> {
    report[key].as_array().unwrap().iter().map(|c| c["id"].as_u64().unwrap()).collect()
}

/// The model as data (the id counter only goes up, even through an undo, so it is left out).
fn snapshot(e: &Engine) -> Value {
    let mut v = serde_json::to_value(&e.model).unwrap();
    v.as_object_mut().unwrap().remove("next_id");
    v
}

fn solid(e: &Engine) -> bool {
    e.model.is_solid(&FixScope::all())
}

/// Every face's normal points away from the middle of the unit cube.
fn faces_out(e: &Engine) {
    for (&k, f) in &e.model.faces {
        let p = e.model.p(f.outer[0]);
        assert!(e.model.normal(k).dot(p - DVec3::splat(0.5)) > 0.0, "face {k} points in");
    }
}

#[test]
fn a_clean_cube_is_solid_and_untouched() {
    let mut e = Engine::new();
    cube(&mut e);
    assert!(solid(&e));
    let before = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(r["changes"], 0);
    assert_eq!(r["unchanged"], true);
    assert_eq!(snapshot(&e), before);
    let c = run(&mut e, json!({"cmd": "check_solid"}));
    assert_eq!(c["solid"], true);
}

#[test]
fn a_hole_is_closed() {
    let mut e = Engine::new();
    let (_, f) = cube(&mut e);
    e.model.faces.remove(&f[1]); // no lid
    assert!(!solid(&e));
    let check = run(&mut e, json!({"cmd": "check_solid"}));
    assert_eq!(check["solid"], false);
    assert_eq!(check["open_edges"].as_array().unwrap().len(), 4);
    let broken = snapshot(&e);

    // looking first changes nothing
    let dry = run(&mut e, json!({"cmd": "fix_model", "dry_run": true}));
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["filled_holes"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot(&e), broken);

    let r = fix(&mut e);
    assert_eq!(r["filled_holes"].as_array().unwrap().len(), 1);
    assert_eq!(r["filled_holes"][0]["at"], json!([0.5, 0.5, 1.0]));
    assert_eq!(e.model.faces.len(), 6);
    assert!(solid(&e));
    faces_out(&e);
    assert_eq!(r["remaining"]["solid"], true);

    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken, "one undo puts the hole back");
}

#[test]
fn a_reversed_face_is_turned_back() {
    let mut e = Engine::new();
    let (_, f) = cube(&mut e);
    e.model.faces.get_mut(&f[3]).unwrap().reverse();
    let check = run(&mut e, json!({"cmd": "check_solid"}));
    assert_eq!(check["solid"], false);
    assert_eq!(check["inconsistent_edges"].as_array().unwrap().len(), 4);
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(ids(&r, "reversed_faces"), vec![f[3] as u64], "only the wrong face is named");
    assert!(solid(&e));
    faces_out(&e);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn a_cube_turned_inside_out_is_turned_out() {
    let mut e = Engine::new();
    let (_, f) = cube(&mut e);
    for k in &f {
        e.model.faces.get_mut(k).unwrap().reverse();
    }
    assert!(!solid(&e), "every face points in");
    let r = fix(&mut e);
    assert_eq!(ids(&r, "reversed_faces").len(), 6);
    assert!(solid(&e));
    faces_out(&e);
}

#[test]
fn a_stray_edge_is_removed() {
    let mut e = Engine::new();
    let (v, _) = cube(&mut e);
    let far = e.model.vertex_in(DVec3::new(2.0, 2.0, 2.0), None);
    let spur = e.model.edge(v[6], far);
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(ids(&r, "stray_edges"), vec![spur as u64]);
    assert!(!e.model.edges.contains_key(&spur));
    assert!(!e.model.vertices.contains_key(&far), "its loose end goes too");
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn a_line_apart_from_every_face_is_a_drawing_and_stays() {
    let mut e = Engine::new();
    cube(&mut e);
    let a = e.model.vertex_in(DVec3::new(5.0, 5.0, 5.0), None);
    let b = e.model.vertex_in(DVec3::new(6.0, 5.0, 5.0), None);
    let line = e.model.edge(a, b);
    let r = fix(&mut e);
    assert!(ids(&r, "stray_edges").is_empty());
    assert!(e.model.edges.contains_key(&line));
    assert_eq!(r["remaining"]["loose_edges"].as_array().unwrap().len(), 1);
    // and the option turns the spur removal off
    let far = e.model.vertex_in(DVec3::new(2.0, 2.0, 2.0), None);
    let spur = e.model.edge(a, far);
    let _ = spur;
    let v6 = e.model.vertices.iter().find(|(_, p)| p.p == DVec3::ONE).map(|(&k, _)| k).unwrap();
    let hang = e.model.edge(v6, far);
    let r = run(&mut e, json!({"cmd": "fix_model", "stray_edges": false}));
    assert!(ids(&r, "stray_edges").is_empty());
    assert!(e.model.edges.contains_key(&hang));
}

#[test]
fn a_face_inside_the_solid_is_removed() {
    // a box 2 long, split at x = 1 on its four long sides, with a wall across the middle
    let mut e = Engine::new();
    let v = verts(
        &mut e,
        &[
            [0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [0., 1., 0.], [1., 1., 0.], [2., 1., 0.], // 0..5 bottom
            [0., 0., 1.], [1., 0., 1.], [2., 0., 1.], [0., 1., 1.], [1., 1., 1.], [2., 1., 1.], // 6..11 top
        ],
    );
    let f = |e: &mut Engine, q: &[usize]| raw_face(e, &q.iter().map(|&i| v[i]).collect::<Vec<_>>());
    f(&mut e, &[0, 3, 4, 1]);
    f(&mut e, &[1, 4, 5, 2]); // bottom, two halves
    f(&mut e, &[6, 7, 10, 9]);
    f(&mut e, &[7, 8, 11, 10]); // top
    f(&mut e, &[0, 1, 7, 6]);
    f(&mut e, &[1, 2, 8, 7]); // front
    f(&mut e, &[4, 3, 9, 10]);
    f(&mut e, &[5, 4, 10, 11]); // back
    f(&mut e, &[3, 0, 6, 9]); // left end
    f(&mut e, &[2, 5, 11, 8]); // right end
    let wall = f(&mut e, &[1, 4, 10, 7]); // across the middle
    assert!(!solid(&e), "three faces on each middle edge");
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(ids(&r, "internal_faces"), vec![wall as u64]);
    assert_eq!(r["merged_faces"].as_array().unwrap().len(), 4, "the four split sides are one face each again");
    assert_eq!(e.model.faces.len(), 6);
    assert!(solid(&e));
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn two_cubes_sharing_a_wall_lose_the_wall() {
    let mut e = Engine::new();
    let v = verts(
        &mut e,
        &[
            [0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [0., 1., 0.], [1., 1., 0.], [2., 1., 0.],
            [0., 0., 1.], [1., 0., 1.], [2., 0., 1.], [0., 1., 1.], [1., 1., 1.], [2., 1., 1.],
        ],
    );
    let mut f = |q: &[usize]| raw_face(&mut e, &q.iter().map(|&i| v[i]).collect::<Vec<_>>());
    // cube A (x 0..1) and cube B (x 1..2), each closed, so the wall between them is there twice
    let a = [[0, 3, 4, 1], [6, 7, 10, 9], [0, 1, 7, 6], [4, 3, 9, 10], [3, 0, 6, 9], [1, 4, 10, 7]];
    let b = [[1, 4, 5, 2], [7, 8, 11, 10], [1, 2, 8, 7], [5, 4, 10, 11], [2, 5, 11, 8], [4, 1, 7, 10]];
    for q in a.iter().chain(b.iter()) {
        f(q);
    }
    let r = fix(&mut e);
    assert_eq!(r["internal_faces"].as_array().unwrap().len(), 2, "both sides of the wall");
    assert_eq!(e.model.faces.len(), 6);
    assert!(solid(&e));
}

#[test]
fn a_repeated_face_is_removed() {
    let mut e = Engine::new();
    let (v, f) = cube(&mut e);
    let twin = raw_face(&mut e, &[v[5], v[6], v[7], v[4]]); // the top again, from another corner
    assert!(!solid(&e));
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(ids(&r, "duplicate_faces"), vec![twin as u64]);
    assert_eq!(r["duplicate_faces"][0]["with"], json!([f[1]]));
    assert!(solid(&e));
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn split_coplanar_faces_are_joined() {
    let mut e = Engine::new();
    // the top of the cube cut in two halves along x = 0.5
    let v = verts(&mut e, &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0.5, 0., 1.], [0.5, 1., 1.]]);
    let f = |e: &mut Engine, q: &[usize]| raw_face(e, &q.iter().map(|&i| v[i]).collect::<Vec<_>>());
    f(&mut e, &[0, 3, 2, 1]);
    let h1 = f(&mut e, &[4, 8, 9, 7]);
    let h2 = f(&mut e, &[8, 5, 6, 9]);
    f(&mut e, &[0, 1, 5, 8, 4]); // front, with the split point on it
    f(&mut e, &[1, 2, 6, 5]);
    f(&mut e, &[2, 3, 7, 9, 6]);
    f(&mut e, &[3, 0, 4, 7]);
    assert!(solid(&e), "split faces are still a solid");
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(r["merged_faces"].as_array().unwrap().len(), 1);
    assert_eq!(r["merged_faces"][0]["id"], h1);
    assert_eq!(r["merged_faces"][0]["with"], json!([h2]));
    assert_eq!(e.model.faces.len(), 6);
    assert_eq!(e.model.faces[&h1].outer.len(), 6, "the corners the cut left on the long sides stay");
    assert!(solid(&e));
    faces_out(&e);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn faces_with_different_paint_are_not_joined() {
    let mut e = Engine::new();
    let v = verts(&mut e, &[[0., 0., 0.], [1., 0., 0.], [2., 0., 0.], [2., 1., 0.], [1., 1., 0.], [0., 1., 0.]]);
    let a = raw_face(&mut e, &[v[0], v[5], v[4], v[1]]);
    raw_face(&mut e, &[v[1], v[4], v[3], v[2]]);
    e.model.faces.get_mut(&a).unwrap().material = Some("red".into());
    let r = run(&mut e, json!({"cmd": "fix_model", "stray_edges": false}));
    assert_eq!(r["merged_faces"].as_array().unwrap().len(), 0);
    assert_eq!(e.model.faces.len(), 2);
}

#[test]
fn a_fin_on_a_shared_edge_is_removed() {
    let mut e = Engine::new();
    let (v, _) = cube(&mut e);
    let tip = e.model.vertex_in(DVec3::new(0.5, 1.5, 1.5), None);
    let fin = raw_face(&mut e, &[v[7], v[6], tip]);
    let check = run(&mut e, json!({"cmd": "check_solid"}));
    assert_eq!(check["non_manifold_edges"].as_array().unwrap().len(), 1);
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(ids(&r, "non_manifold_fins"), vec![fin as u64]);
    assert!(solid(&e));
    assert_eq!(e.model.faces.len(), 6);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
}

#[test]
fn a_hole_with_a_bent_rim_is_closed_with_triangles() {
    let mut e = Engine::new();
    // a box of triangles whose top corner is lifted, so the missing lid is not flat
    let v = verts(&mut e, &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0., 1.], [1., 0., 1.], [1., 1., 1.2], [0., 1., 1.]]);
    raw_face(&mut e, &[v[0], v[3], v[2], v[1]]);
    for q in [[0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
        raw_face(&mut e, &[v[q[0]], v[q[1]], v[q[2]]]);
        raw_face(&mut e, &[v[q[0]], v[q[2]], v[q[3]]]);
    }
    let r = fix(&mut e);
    let fill = &r["filled_holes"][0];
    assert_eq!(fill["with"].as_array().unwrap().len(), 1, "two triangles: the first and one more");
    assert!(solid(&e));
}

#[test]
fn a_flat_sheet_is_not_closed_into_a_solid() {
    let mut e = Engine::new();
    let v = verts(&mut e, &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]]);
    raw_face(&mut e, &[v[0], v[1], v[2], v[3]]);
    let before = snapshot(&e);
    let r = fix(&mut e);
    assert_eq!(r["changes"], 0);
    assert_eq!(snapshot(&e), before);
    assert_eq!(r["remaining"]["open_edges"].as_array().unwrap().len(), 4);
    assert!(r["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("open edge")));
}

#[test]
fn a_messy_model_is_one_step_and_every_change_is_listed() {
    let mut e = Engine::new();
    let (v, f) = cube(&mut e);
    e.model.faces.remove(&f[1]); // hole
    e.model.faces.get_mut(&f[2]).unwrap().reverse(); // reversed
    let far = e.model.vertex_in(DVec3::new(3.0, 3.0, 3.0), None);
    let spur = e.model.edge(v[6], far); // stray
    let twin = raw_face(&mut e, &[v[0], v[1], v[2], v[3]]); // the bottom again, turned the other way
    let tip = e.model.vertex_in(DVec3::new(0.5, 1.5, 0.5), None);
    let fin = raw_face(&mut e, &[v[3], v[2], tip]); // a fin on the bottom back edge
    assert!(!solid(&e));
    let broken = snapshot(&e);
    let r = fix(&mut e);
    assert!(solid(&e), "{r}");
    assert_eq!(r["remaining"]["solid"], true);
    // (the fin's own two free edges are left behind as lines when it goes, and are swept up too)
    assert_eq!(ids(&r, "stray_edges").len(), 3);
    assert!(ids(&r, "stray_edges").contains(&(spur as u64)));
    assert_eq!(ids(&r, "non_manifold_fins"), vec![fin as u64]);
    assert_eq!(ids(&r, "duplicate_faces"), vec![twin as u64]);
    assert_eq!(r["filled_holes"].as_array().unwrap().len(), 1);
    assert!(r["changes"].as_u64().unwrap() >= 4);
    faces_out(&e);
    // the whole repair is one undo step
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(snapshot(&e), broken);
    run(&mut e, json!({"cmd": "redo"}));
    assert!(solid(&e));
}

#[test]
fn it_can_be_limited_to_one_group() {
    let mut e = Engine::new();
    let open_box = |e: &mut Engine, x: f64, name: &str| -> u64 {
        let g = run(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
        let f = run(e, json!({"cmd": "rect", "from": [x, 0, 0], "to": [x + 1.0, 1, 0]}))["face"].as_u64().unwrap();
        run(e, json!({"cmd": "pushpull", "face": f, "distance": 1}));
        run(e, json!({"cmd": "end_group"}));
        let top = e.model.faces.iter().find(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).z > 0.5).map(|(&k, _)| k).unwrap();
        run(e, json!({"cmd": "delete", "ids": [top]}));
        g
    };
    let a = open_box(&mut e, 0.0, "A");
    let b = open_box(&mut e, 3.0, "B");
    let r = run(&mut e, json!({"cmd": "fix_model", "ids": [a]}));
    assert_eq!(r["filled_holes"].as_array().unwrap().len(), 1);
    assert!(e.model.is_solid(&e.model.fix_scope(&[a as u32]).unwrap()));
    assert!(!e.model.is_solid(&e.model.fix_scope(&[b as u32]).unwrap()), "B was not asked about");
    // the rest of the model goes through when no scope is given
    let r = fix(&mut e);
    assert_eq!(r["filled_holes"].as_array().unwrap().len(), 1);
    assert!(e.model.is_solid(&e.model.fix_scope(&[b as u32]).unwrap()));
}

#[test]
fn the_rust_api_lists_problems_and_fixes_them() {
    let mut e = Engine::new();
    let (_, f) = cube(&mut e);
    e.model.faces.remove(&f[0]);
    let scope = FixScope::all();
    let check = e.model.solid_check(&scope);
    assert!(!check.solid);
    assert_eq!(check.open_edges.len(), 4);
    let report = e.model.fix_model(&scope, &FixOptions::default());
    assert_eq!(report.filled_holes.len(), 1);
    assert!(e.model.is_solid(&scope));
    assert!(e.model.everything_is_solid());
}

#[test]
fn a_locked_group_is_left_alone() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "begin_group", "name": "Locked"}))["group"].as_u64().unwrap();
    let f = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 1}));
    run(&mut e, json!({"cmd": "end_group"}));
    let top = e.model.faces.iter().find(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).z > 0.5).map(|(&k, _)| k).unwrap();
    run(&mut e, json!({"cmd": "delete", "ids": [top]}));
    run(&mut e, json!({"cmd": "lock", "ids": [g], "locked": true}));
    let r = fix(&mut e);
    assert_eq!(r["changes"], 0);
    assert!(r["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("locked")));
}
