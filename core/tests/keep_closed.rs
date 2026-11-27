//! The safety net (0.3.11): a push/pull, a door or window, an opening edit or a rebuild that would leave a closed solid
//! with a gap is refused and the model is put back, so a reply never says ok over a broken model. The cases are the
//! final re-test's (R0311-F-01..05); each either works and stays solid, or is refused and leaves the model as it was.

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

fn bad_edges(e: &Engine) -> usize {
    let mut uses = std::collections::HashMap::<(Option<u32>, u32, u32), usize>::new();
    for f in e.model.faces.values() {
        for l in f.loops() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                *uses.entry((f.group, a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    // used an odd number of times: a gap (a pinch, four faces at an edge where cuts meet at a corner, is closed)
    uses.values().filter(|&&n| n % 2 == 1).count()
}

/// Run a cutting command: either it works and every solid stays closed, or it's refused, says why, and the model
/// is exactly as before. Returns whether it worked.
fn cut(e: &mut Engine, v: Value) -> bool {
    assert_eq!(bad_edges(e), 0, "solid before {v}");
    let (faces, edges) = (e.model.faces.len(), e.model.edges.len());
    let r = run(e, v.clone());
    if r["ok"] == true {
        assert_eq!(bad_edges(e), 0, "{v} said ok over open edges: {r}");
        true
    } else {
        assert!(r["error"].as_str().unwrap_or("").contains("gap"), "{v} -> {r}");
        assert_eq!((e.model.faces.len(), e.model.edges.len(), bad_edges(e)), (faces, edges, 0), "{v}: refused but not put back");
        false
    }
}

fn top(e: &Engine, z: f64) -> u32 {
    *e.model.faces.keys().find(|&&k| e.model.normal(k).z > 0.99 && (e.model.center(k).z - z).abs() < 1e-6 && e.model.face_area(k) > 1.0).unwrap()
}

fn slab(e: &mut Engine, corners: &[[f64; 3]], h: f64) {
    let f = ok(e, json!({"cmd": "polygon", "points": corners}))["face"].as_u64().unwrap();
    let up = if e.model.normal(f as u32).z > 0.0 { h } else { -h };
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": up}));
}

/// The face of group `g` facing `dir` (in plan) with the biggest area.
fn side(e: &Engine, g: u64, dir: [f64; 2]) -> u32 {
    *e.model
        .faces
        .iter()
        .filter(|(&k, f)| f.group == Some(g as u32) && { let n = e.model.normal(k); n.x * dir[0] + n.y * dir[1] > 0.99 })
        .max_by(|a, b| e.model.face_area(*a.0).total_cmp(&e.model.face_area(*b.0)))
        .unwrap()
        .0
}

#[test]
fn f01_a_through_cut_across_a_turned_slab() {
    let (c, s) = (17.3f64.to_radians().cos(), 17.3f64.to_radians().sin());
    let r = |x: f64, y: f64, z: f64| [x * c - y * s, x * s + y * c, z];
    let mut e = Engine::new();
    slab(&mut e, &[r(0., 0., 0.), r(11., 0., 0.), r(11., 11., 0.), r(0., 11., 0.)], 2.3);
    ok(&mut e, json!({"cmd": "line", "points": [r(0., 1.07, 2.3), r(11., 1.07, 2.3)]}));
    ok(&mut e, json!({"cmd": "line", "points": [r(0., 4.31, 2.3), r(11., 4.31, 2.3)]}));
    let strip = *e.model.faces.keys().filter(|&&k| e.model.normal(k).z > 0.99).min_by(|a, b| e.model.face_area(**a).total_cmp(&e.model.face_area(**b))).unwrap();
    cut(&mut e, json!({"cmd": "pushpull", "face": strip, "distance": -2.3}));
}

#[test]
fn f02_doors_on_turned_walls() {
    for deg in (0..360).step_by(10) {
        let (c, s) = ((deg as f64).to_radians().cos(), (deg as f64).to_radians().sin());
        let mut e = Engine::new();
        let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [10.0 * c, 10.0 * s, 0]]}))["group"].as_u64().unwrap();
        let f = side(&e, g, [s, -c]);
        let m = e.model.center(f);
        cut(&mut e, json!({"cmd": "opening", "kind": "door", "host": f, "at": [m.x, m.y, 1.05], "width": 1, "height": 2.1}));
    }
}

#[test]
fn f03_rebuilding_a_long_wall_with_doors_taller() {
    let mut e = Engine::new();
    let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [13, 0, 0]]}))["group"].as_u64().unwrap();
    for x in [2.0, 6.0] {
        let f = side(&e, g, [0., -1.]);
        cut(&mut e, json!({"cmd": "opening", "kind": "door", "host": f, "at": [x, 0, 1.05], "width": 1, "height": 2.1}));
    }
    cut(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 2.8}}));
}

#[test]
fn f04_three_cuts_in_an_order_that_breaks() {
    for seq in [[[3., 7., 4., 8., -2.], [3., 6., 4., 7., -0.5], [3., 0., 6., 1., -1.]], [[0., 1., 5., 5., -2.], [0., 0., 5., 1., -2.], [0., 5., 1., 8., -2.]]] {
        let mut e = Engine::new();
        slab(&mut e, &[[0., 0., 0.], [8., 0., 0.], [8., 8., 0.], [0., 8., 0.]], 2.0);
        for [x0, y0, x1, y1, d] in seq {
            ok(&mut e, json!({"cmd": "rect", "from": [x0, y0, 2.0], "to": [x1, y1, 2.0]}));
            let f = *e.model.faces.keys().filter(|&&k| e.model.normal(k).z > 0.99 && (e.model.center(k).z - 2.0).abs() < 1e-6).find(|&&k| { let p = e.model.center(k); (p.x - (x0 + x1) / 2.0).abs() < 1e-6 && (p.y - (y0 + y1) / 2.0).abs() < 1e-6 }).unwrap();
            cut(&mut e, json!({"cmd": "pushpull", "face": f, "distance": d}));
        }
    }
    let _ = top;
}

#[test]
fn f05_an_opening_past_the_end_of_its_face_in_a_closed_wall() {
    for kind in ["window", "door"] {
        let mut e = Engine::new();
        let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [6.5, 0, 0], [6.5, 9.5, 0], [0, 9.5, 0]], "closed": true, "thickness": 0.3}))["group"].as_u64().unwrap();
        let f = *e.model.faces.iter().filter(|(&k, fc)| fc.group == Some(g as u32) && e.model.normal(k).x > 0.99 && e.model.center(k).x < 1.0).max_by(|a, b| e.model.face_area(*a.0).total_cmp(&e.model.face_area(*b.0))).unwrap().0;
        cut(&mut e, json!({"cmd": "opening", "kind": kind, "host": f, "at": [0.15, 0.6, 1.2], "width": 1.0, "height": if kind == "door" { 2.1 } else { 1.2 }, "sill": 0.9}));
    }
}

#[test]
fn random_cut_triples_never_say_ok_over_a_gap() {
    let mut s: u64 = 0x0311_5afe;
    let mut next = |n: u64| { s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (s >> 33) % n };
    let (mut done, mut refused) = (0, 0);
    for _ in 0..300 {
        let mut e = Engine::new();
        slab(&mut e, &[[0., 0., 0.], [8., 0., 0.], [8., 8., 0.], [0., 8., 0.]], 2.0);
        for _ in 0..3 {
            let (x0, y0) = (next(7) as f64, next(7) as f64);
            let (x1, y1) = (x0 + 1.0 + next((7 - x0 as u64).max(1)) as f64, y0 + 1.0 + next((7 - y0 as u64).max(1)) as f64);
            let (x1, y1) = (x1.min(8.0), y1.min(8.0));
            if run(&mut e, json!({"cmd": "rect", "from": [x0, y0, 2.0], "to": [x1, y1, 2.0]}))["ok"] != true || bad_edges(&e) > 0 {
                break;
            }
            let Some(f) = e.model.faces.keys().copied().filter(|&k| e.model.normal(k).z > 0.99 && (e.model.center(k).z - 2.0).abs() < 1e-6).find(|&k| { let p = e.model.center(k); (p.x - (x0 + x1) / 2.0).abs() < 1e-6 && (p.y - (y0 + y1) / 2.0).abs() < 1e-6 }) else { break };
            let d = [-2.0, -1.0, -0.5][next(3) as usize];
            if cut(&mut e, json!({"cmd": "pushpull", "face": f, "distance": d})) { done += 1 } else { refused += 1 }
        }
    }
    eprintln!("random triples: {done} cuts made, {refused} refused");
    assert!(done > 500 && refused * 20 < done, "cuts made {done}, refused {refused}");
}

#[test]
fn n01_a_library_door_placed_into_a_turned_wall() {
    for deg in (5..360).step_by(15) {
        let (c, s) = ((deg as f64).to_radians().cos(), (deg as f64).to_radians().sin());
        let mut e = Engine::new();
        let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [10.0 * c, 10.0 * s, 0]]}))["group"].as_u64().unwrap();
        let f = side(&e, g, [s, -c]);
        let m = e.model.center(f);
        let r = run(&mut e, json!({"cmd": "insert", "asset": "door", "at": [m.x, m.y, 0], "rotate": deg, "host": f}));
        if r["ok"] == true {
            assert_eq!(bad_edges(&e), 0, "{deg}°: ok over a gap: {r}");
        } else {
            assert!(r["error"].as_str().unwrap_or("").contains("gap"), "{deg}° -> {r}");
            assert_eq!(bad_edges(&e), 0);
        }
    }
}

#[test]
fn n02_a_line_up_from_the_ground_then_a_cut_at_the_ground() {
    let mut e = Engine::new();
    slab(&mut e, &[[0., 0., 0.], [6., 0., 0.], [6., 4., 0.], [0., 4., 0.]], 3.0);
    ok(&mut e, json!({"cmd": "line", "points": [[1, 0, 0], [1, 0, 3]]}));
    let f = ok(&mut e, json!({"cmd": "rect", "from": [6, 1, 0], "to": [6, 2.5, 2]}))["face"].clone();
    assert!(cut(&mut e, json!({"cmd": "pushpull", "face": f, "distance": -0.2})), "an ordinary cut was refused");
}

#[test]
fn n03_offset_inside_a_closed_group_from_outside_it() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "begin_group", "name": "Box"}));
    slab(&mut e, &[[0., 0., 0.], [8., 0., 0.], [8., 6., 0.], [0., 6., 0.]], 3.0);
    ok(&mut e, json!({"cmd": "end_group"}));
    let t = top(&e, 3.0);
    ok(&mut e, json!({"cmd": "offset", "face": t, "distance": 1}));
    assert_eq!(bad_edges(&e), 0);
}

#[test]
fn n04_a_follow_me_groove_never_says_ok_over_a_fault() {
    let mut e = Engine::new();
    slab(&mut e, &[[0., 0., 0.], [6., 0., 0.], [6., 4., 0.], [0., 4., 0.]], 3.0);
    let p = ok(&mut e, json!({"cmd": "polygon", "points": [[0, 0.3, 2.5], [0, 0.3, 3], [0, 0.8, 3], [0, 0.8, 2.5]]}))["face"].clone();
    let r = run(&mut e, json!({"cmd": "follow_me", "face": p, "points": [[0, 0.5, 3], [6, 0.5, 3]]}));
    let mut uses = std::collections::HashMap::<(u32, u32), usize>::new();
    for f in e.model.faces.values() { for l in f.loops() { for i in 0..l.len() { let (a, b) = (l[i], l[(i + 1) % l.len()]); *uses.entry((a.min(b), a.max(b))).or_default() += 1; } } }
    if r["ok"] == true {
        assert!(uses.values().all(|&n| n == 2), "ok over a non-manifold result: {r}");
    } else {
        assert!(r["error"].as_str().unwrap_or("").contains("gap"), "{r}");
    }
}

#[test]
fn an_outward_offset_on_a_solid_makes_its_ledge() {
    let mut e = Engine::new();
    slab(&mut e, &[[0., 0., 0.], [8., 0., 0.], [8., 6., 0.], [0., 6., 0.]], 3.0);
    let t = top(&e, 3.0);
    let r = run(&mut e, json!({"cmd": "offset", "face": t, "distance": -0.2}));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn a_batch_whose_cut_was_refused_leaves_no_undo_step() {
    let mut e = Engine::new();
    let g = ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [6.5, 0, 0], [6.5, 9.5, 0], [0, 9.5, 0]], "closed": true, "thickness": 0.3}))["group"].as_u64().unwrap();
    let f = *e.model.faces.iter().filter(|(&k, fc)| fc.group == Some(g as u32) && e.model.normal(k).x > 0.99 && e.model.center(k).x < 1.0).max_by(|a, b| e.model.face_area(*a.0).total_cmp(&e.model.face_area(*b.0))).unwrap().0;
    ok(&mut e, json!({"cmd": "rect", "from": [20, 20, 0], "to": [22, 22, 0]}));
    let n = e.model.faces.len();
    ok(&mut e, json!({"cmd": "begin_batch"}));
    assert_eq!(run(&mut e, json!({"cmd": "opening", "kind": "window", "host": f, "at": [0.15, 0.6, 1.2], "width": 1.0, "height": 1.2, "sill": 0.9}))["ok"], false);
    ok(&mut e, json!({"cmd": "end_batch"}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.faces.len(), n - 1, "the one Undo takes back the rectangle, the last thing that happened");
}
