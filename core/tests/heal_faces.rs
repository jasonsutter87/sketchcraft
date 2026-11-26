//! Deleting the edge between two faces that lie flat together heals them back into one face, as in SketchUp; any
//! other edge still takes the faces that need it (#262, from Blender 3D Architect's first look at 0.3.8: "if I delete
//! this edge, it will remove the entire face").

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// The edge running from `p` to `q` (either way).
fn edge_at(e: &Engine, p: [f64; 3], q: [f64; 3]) -> u32 {
    let (p, q) = (DVec3::from(p), DVec3::from(q));
    let at = |v: u32| e.model.vertices[&v].p;
    *e.model
        .edges
        .iter()
        .find(|(_, ed)| (at(ed.a).distance(p) < 1e-6 && at(ed.b).distance(q) < 1e-6) || (at(ed.a).distance(q) < 1e-6 && at(ed.b).distance(p) < 1e-6))
        .unwrap_or_else(|| panic!("no edge {p} - {q}"))
        .0
}

/// Faces lying on z = `z`.
fn faces_at_z(e: &Engine, z: f64) -> Vec<u32> {
    e.model.faces.iter().filter(|(_, f)| e.model.pts(&f.outer).iter().all(|p| (p.z - z).abs() < 1e-6)).map(|(&k, _)| k).collect()
}

fn area_at_z(e: &Engine, z: f64) -> f64 {
    faces_at_z(e, z).iter().map(|&f| e.model.face_area(f)).sum()
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

/// A 4 x 2 rectangle on the ground with a line across it at x = 1.5.
fn split_rect() -> Engine {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 2, 0]}));
    ok(&mut e, json!({"cmd": "line", "points": [[1.5, 0, 0], [1.5, 2, 0]]}));
    assert_eq!(faces_at_z(&e, 0.0).len(), 2, "the line splits the rectangle");
    e
}

#[test]
fn deleting_the_line_across_a_rectangle_heals_it() {
    let mut e = split_rect();
    let line = edge_at(&e, [1.5, 0.0, 0.0], [1.5, 2.0, 0.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [line]}));
    let faces = faces_at_z(&e, 0.0);
    assert_eq!(faces.len(), 1, "one face again");
    assert!((area_at_z(&e, 0.0) - 8.0).abs() < 1e-9, "the whole rectangle");
    assert!(!e.model.edges.contains_key(&line), "the line is gone");
    let n = e.model.normal(faces[0]);
    assert!(n.z.abs() > 0.999, "still lies flat");
}

#[test]
fn the_bigger_face_keeps_its_id_and_material() {
    let mut e = split_rect();
    let big = *faces_at_z(&e, 0.0).iter().max_by(|&&a, &&b| e.model.face_area(a).total_cmp(&e.model.face_area(b))).unwrap();
    ok(&mut e, json!({"cmd": "paint", "ids": [big], "color": [0.8, 0.2, 0.2]}));
    let mat = e.model.faces[&big].material.clone();
    assert!(mat.is_some());
    let line = edge_at(&e, [1.5, 0.0, 0.0], [1.5, 2.0, 0.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [line]}));
    assert_eq!(faces_at_z(&e, 0.0), vec![big]);
    assert_eq!(e.model.faces[&big].material, mat);
}

#[test]
fn a_split_box_side_heals_and_the_box_stays_closed() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2}));
    let before = e.model.faces.len();
    ok(&mut e, json!({"cmd": "line", "points": [[0, 1, 2], [4, 1, 2]]}));
    assert_eq!(e.model.faces.len(), before + 1, "the top is split");
    let line = edge_at(&e, [0.0, 1.0, 2.0], [4.0, 1.0, 2.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [line]}));
    assert_eq!(e.model.faces.len(), before, "the top is one face again");
    assert!((area_at_z(&e, 2.0) - 12.0).abs() < 1e-9);
    assert!(closed(&e), "still a closed solid");
}

#[test]
fn an_edge_on_a_fold_still_takes_its_faces() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2}));
    let before = e.model.faces.len();
    let corner = edge_at(&e, [0.0, 0.0, 2.0], [4.0, 0.0, 2.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [corner]}));
    assert_eq!(e.model.faces.len(), before - 2, "the top and the front go, as before");
}

#[test]
fn deleting_an_edge_of_a_shape_drawn_inside_a_face_fills_the_hole() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 4, 0]}));
    ok(&mut e, json!({"cmd": "rect", "from": [1, 1, 0], "to": [2, 2, 0]}));
    assert_eq!(faces_at_z(&e, 0.0).len(), 2, "the inner square is its own face, a hole in the outer one");
    let side = edge_at(&e, [1.0, 1.0, 0.0], [2.0, 1.0, 0.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [side]}));
    let faces = faces_at_z(&e, 0.0);
    assert_eq!(faces.len(), 1);
    assert!(e.model.faces[&faces[0]].holes.is_empty(), "no hole left");
    assert!((area_at_z(&e, 0.0) - 16.0).abs() < 1e-9);
    // the square's other three sides stay behind as lines
    edge_at(&e, [2.0, 1.0, 0.0], [2.0, 2.0, 0.0]);
}

#[test]
fn two_lines_deleted_together_heal_three_faces_into_one() {
    let mut e = split_rect();
    ok(&mut e, json!({"cmd": "line", "points": [[3, 0, 0], [3, 2, 0]]}));
    assert_eq!(faces_at_z(&e, 0.0).len(), 3);
    let a = edge_at(&e, [1.5, 0.0, 0.0], [1.5, 2.0, 0.0]);
    let b = edge_at(&e, [3.0, 0.0, 0.0], [3.0, 2.0, 0.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [a, b]}));
    assert_eq!(faces_at_z(&e, 0.0).len(), 1);
    assert!((area_at_z(&e, 0.0) - 8.0).abs() < 1e-9);
}

#[test]
fn undo_brings_the_line_and_both_faces_back() {
    let mut e = split_rect();
    let line = edge_at(&e, [1.5, 0.0, 0.0], [1.5, 2.0, 0.0]);
    ok(&mut e, json!({"cmd": "delete", "ids": [line]}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(faces_at_z(&e, 0.0).len(), 2);
    edge_at(&e, [1.5, 0.0, 0.0], [1.5, 2.0, 0.0]);
}
