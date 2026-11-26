//! Cutting into a solid keeps it a solid: cuts at a part's edge around earlier holes, pockets inside a bigger cut, and
//! cuts side by side (a door beside a window, two notches in a row).

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn slab(e: &mut Engine, w: f64, t: f64) {
    let f = ok(e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [w, w, 0]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": t}));
}

fn cut(e: &mut Engine, from: [f64; 2], to: [f64; 2], z: f64, d: f64) {
    let f = ok(e, json!({"cmd": "rect", "from": [from[0], from[1], z], "to": [to[0], to[1], z]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": d}));
}

/// Open or over-used edges (each edge of a closed solid is used by exactly two faces).
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

#[test]
fn a_cut_at_the_edge_around_an_earlier_hole() {
    for (from, to) in [([0., 0.], [8., 8.]), ([0., 2.], [8., 8.])] {
        let mut e = Engine::new();
        slab(&mut e, 10.0, 2.0);
        cut(&mut e, [4., 4.], [5., 5.], 2.0, -2.0);
        cut(&mut e, from, to, 2.0, -2.0);
        assert!(solid(&mut e), "{from:?}-{to:?}: {} bad edges", bad_edges(&e));
    }
}

#[test]
fn a_cut_around_a_blind_pocket() {
    for (from, to) in [([2., 2.], [8., 8.]), ([0., 0.], [8., 8.])] {
        let mut e = Engine::new();
        slab(&mut e, 10.0, 2.0);
        cut(&mut e, [4., 4.], [5., 5.], 2.0, -1.0);
        cut(&mut e, from, to, 2.0, -2.0);
        assert!(solid(&mut e), "{from:?}-{to:?}: {} bad edges", bad_edges(&e));
    }
}

#[test]
fn cuts_side_by_side() {
    for (d1, d2, a, b) in [(-3.0, -1.0, ([5., 5.], [8., 8.]), ([8., 5.], [11., 8.])), (-1.0, -3.0, ([5., 5.], [8., 8.]), ([8., 5.], [11., 8.])), (-3.0, -3.0, ([0., 5.], [3., 8.]), ([3., 5.], [6., 8.]))] {
        let mut e = Engine::new();
        slab(&mut e, 20.0, 3.0);
        cut(&mut e, a.0, a.1, 3.0, d1);
        cut(&mut e, b.0, b.1, 3.0, d2);
        assert!(solid(&mut e), "{a:?} {d1} then {b:?} {d2}: {} bad edges", bad_edges(&e));
    }
}

#[test]
fn a_grid_of_cut_pairs_stays_solid() {
    // on a 10 x 10 x 3 slab, every pair of non-overlapping rectangles on a 0/4/6/10 grid: one cut part way or
    // through, then a second cut through
    let xs = [0.0, 4.0, 6.0, 10.0];
    let mut rects = vec![];
    for i in 0..4 {
        for j in i + 1..4 {
            for k in 0..4 {
                for l in k + 1..4 {
                    rects.push(([xs[i], xs[k]], [xs[j], xs[l]]));
                }
            }
        }
    }
    let overlap = |a: &([f64; 2], [f64; 2]), b: &([f64; 2], [f64; 2])| a.0[0] < b.1[0] && b.0[0] < a.1[0] && a.0[1] < b.1[1] && b.0[1] < a.1[1];
    // two cuts meeting only at a corner share one edge between four faces: that is right for two holes meeting
    // along a line (no solid can have it any other way), so those pairs aren't counted
    let corner_only = |a: &([f64; 2], [f64; 2]), b: &([f64; 2], [f64; 2])| {
        let touch_x = a.1[0] == b.0[0] || b.1[0] == a.0[0];
        let touch_y = a.1[1] == b.0[1] || b.1[1] == a.0[1];
        touch_x && touch_y
    };
    let (mut tried, mut bad) = (0, vec![]);
    for a in &rects {
        for b in &rects {
            if overlap(a, b) || corner_only(a, b) || a == b || (a.0 == [0.0, 0.0] && a.1 == [10.0, 10.0]) || (b.0 == [0.0, 0.0] && b.1 == [10.0, 10.0]) {
                continue;
            }
            for d1 in [-1.0, -3.0] {
                tried += 1;
                let mut e = Engine::new();
                slab(&mut e, 10.0, 3.0);
                cut(&mut e, a.0, a.1, 3.0, d1);
                cut(&mut e, b.0, b.1, 3.0, -3.0);
                if !solid(&mut e) {
                    bad.push((a.clone(), d1, b.clone(), bad_edges(&e)));
                }
            }
        }
    }
    for b in &bad {
        eprintln!("NOT SOLID {b:?}");
    }
    assert!(bad.is_empty(), "{} of {tried} pairs not solid", bad.len());
}

#[test]
fn a_cut_beside_a_face_with_thousands_of_corners_is_quick() {
    let mut e = Engine::new();
    // a disc of 4,000 corners, made a solid; a pocket cut into its top beside the rim
    let n = 4000;
    let pts: Vec<[f64; 3]> = (0..n).map(|i| {
        let a = i as f64 / n as f64 * std::f64::consts::TAU;
        [10.0 * a.cos(), 10.0 * a.sin(), 0.0]
    }).collect();
    let f = ok(&mut e, json!({"cmd": "polygon", "points": pts}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2}));
    let t = std::time::Instant::now();
    cut(&mut e, [-2.0, -2.0], [2.0, 2.0], 2.0, -1.0);
    assert!(t.elapsed().as_secs_f64() < 3.0, "{:?}", t.elapsed());
    assert!(solid(&mut e));
}
