//! Doors and windows keep a Wall-tool wall a closed solid: three doors at the foot, any door in a closed wall,
//! and openings brought into overlap by move, duplicate or array and then a rebuild (R0311-R2-01..03).

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    e.exec_json(&v.to_string())
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = run(e, v.clone());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// Edges not used by exactly two face loops (open or non-manifold), holes counted.
fn bad_edges(e: &Engine) -> usize {
    let mut uses = std::collections::HashMap::<(u32, u32), usize>::new();
    for f in e.model.faces.values() {
        for l in f.loops() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    uses.values().filter(|&&n| n != 2).count()
}

fn solid(e: &mut Engine) -> bool {
    bad_edges(e) == 0 && ok(e, json!({"cmd": "check_solid"}))["solid"] == true
}

fn zero_area_faces(e: &Engine) -> usize {
    e.model.faces.keys().filter(|&&k| e.model.face_area(k) < 1e-9).count()
}

fn wall_ring(e: &mut Engine) -> u64 {
    ok(e, json!({"cmd": "wall", "points": [[0, 0, 0], [8, 0, 0], [8, 6, 0], [0, 6, 0]], "closed": true}))["group"].as_u64().unwrap()
}

/// The biggest face of group `g` looking toward -y.
fn front_of(e: &Engine, g: u64) -> u32 {
    *e.model.faces.iter().filter(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).y < -0.99).max_by(|a, b| e.model.face_area(*a.0).total_cmp(&e.model.face_area(*b.0))).unwrap().0
}

fn door(e: &mut Engine, face: u32, x: f64) -> Value {
    run(e, json!({"cmd": "opening", "kind": "door", "host": face, "at": [x, 0, 1], "width": 0.9, "height": 2.1}))
}

#[test]
fn three_doors_at_the_foot_of_a_wall() {
    let xs = [2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 14.0, 16.0];
    let mut failed = vec![];
    for i in 0..xs.len() {
        for j in i + 1..xs.len() {
            for k in j + 1..xs.len() {
                let mut e = Engine::new();
                let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [20, 0, 0]]}))["group"].as_u64().unwrap();
                let mut all = true;
                for x in [xs[i], xs[j], xs[k]] {
                    let f = front_of(&e, g);
                    all &= door(&mut e, f, x)["ok"] == true;
                }
                if !all || !solid(&mut e) || zero_area_faces(&e) > 0 {
                    failed.push((xs[i], xs[j], xs[k]));
                }
            }
        }
    }
    assert!(failed.is_empty(), "{} of 220 triples not solid: {failed:?}", failed.len());
}

#[test]
fn six_doors_in_a_row() {
    let mut e = Engine::new();
    let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [20, 0, 0]]}))["group"].as_u64().unwrap();
    for x in [2.0, 5.0, 8.0, 11.0, 14.0, 17.0] {
        let f = front_of(&e, g);
        ok(&mut e, json!({"cmd": "opening", "kind": "door", "host": f, "at": [x, 0, 1], "width": 0.9, "height": 2.1}));
    }
    assert!(solid(&mut e), "{} bad edges", bad_edges(&e));
}

#[test]
fn a_door_in_a_closed_wall() {
    let mut tried = 0;
    // every upright side face of the ring (outer and inner), at a few places along it
    for side in 0..8 {
        for off in [-1.5, 0.0, 1.3] {
            let mut e = Engine::new();
            let g = wall_ring(&mut e);
            assert!(solid(&mut e), "the ring itself: {} bad edges", bad_edges(&e));
            let mut sides: Vec<u32> = e.model.faces.iter().filter(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).z.abs() < 1e-6).map(|(&k, _)| k).collect();
            sides.sort();
            let face = sides[side % sides.len()];
            let c = e.model.pts(&e.model.faces[&face].outer);
            let mid = c.iter().fold(DVec3::ZERO, |a, p| a + *p) / c.len() as f64;
            let at = mid + e.model.normal(face).cross(DVec3::Z) * off;
            let r = run(&mut e, json!({"cmd": "opening", "kind": "door", "host": face, "at": [at.x, at.y, 1.0], "width": 0.9, "height": 2.1}));
            if r["ok"] != true {
                continue;
            }
            tried += 1;
            assert!(solid(&mut e), "side {side} offset {off}: {} bad edges", bad_edges(&e));
            assert_eq!(zero_area_faces(&e), 0, "side {side} offset {off}: a zero-area face");
        }
    }
    assert!(tried >= 8, "only {tried} doors went in");
}

#[test]
fn a_door_in_a_closed_wall_from_the_front() {
    let mut e = Engine::new();
    let g = wall_ring(&mut e);
    let f = front_of(&e, g);
    ok(&mut e, json!({"cmd": "opening", "kind": "door", "host": f, "at": [3, 0, 1], "width": 0.9, "height": 2.1}));
    assert!(solid(&mut e), "{} bad edges", bad_edges(&e));
    assert_eq!(zero_area_faces(&e), 0);
}

fn two_windows() -> (Engine, u64, Vec<u32>) {
    let mut e = Engine::new();
    let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [8, 0, 0]]}))["group"].as_u64().unwrap();
    let face = front_of(&e, g);
    let mut ids = vec![];
    for x in [2.0, 4.5] {
        let r = ok(&mut e, json!({"cmd": "opening", "kind": "window", "host": face, "at": [x, 0, 1.5], "width": 1.0, "height": 1.2, "sill": 0.9}));
        ids.push(r["instance"].as_u64().unwrap() as u32);
    }
    assert!(solid(&mut e));
    (e, g, ids)
}

fn after_rebuild(e: &mut Engine, g: u64, set: Value) {
    let r = run(e, json!({"cmd": "rebuild", "group": g, "set": set}));
    assert!(solid(e), "{r} -> {} bad edges", bad_edges(e));
}

#[test]
fn move_then_rebuild_does_not_corrupt_the_wall() {
    let (mut e, g, ids) = two_windows();
    ok(&mut e, json!({"cmd": "move", "ids": [ids[1]], "by": [-2.2, 0, 0]}));
    after_rebuild(&mut e, g, json!({"thickness": 0.3}));
}

#[test]
fn duplicate_then_rebuild_does_not_corrupt_the_wall() {
    let (mut e, g, ids) = two_windows();
    ok(&mut e, json!({"cmd": "duplicate", "ids": [ids[0]], "by": [0.3, 0, 0]}));
    after_rebuild(&mut e, g, json!({"height": 2.8}));
}

#[test]
fn array_then_rebuild_does_not_corrupt_the_wall() {
    let (mut e, g, ids) = two_windows();
    ok(&mut e, json!({"cmd": "array", "ids": [ids[0]], "count": 3, "by": [0.5, 0, 0]}));
    after_rebuild(&mut e, g, json!({"height": 2.7}));
}

#[test]
fn a_rebuild_says_which_openings_it_left_out() {
    let (mut e, g, ids) = two_windows();
    ok(&mut e, json!({"cmd": "move", "ids": [ids[1]], "by": [-2.2, 0, 0]}));
    let r = ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"thickness": 0.3}}));
    assert_eq!(r["openings"], 1, "{r}");
    assert_eq!(r["openings_overlapping"], 1, "{r}");
}
