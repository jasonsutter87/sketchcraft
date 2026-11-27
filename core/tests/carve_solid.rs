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

/// A cut drawn as a polygon (for parts not along the axes): its corners at z, pushed `d`.
fn cut_poly(e: &mut Engine, pts: &[[f64; 2]], z: f64, d: f64) {
    let p: Vec<[f64; 3]> = pts.iter().map(|q| [q[0], q[1], z]).collect();
    let f = ok(e, json!({"cmd": "polygon", "points": p}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": d}));
}

#[test]
fn a_pocket_beside_a_longer_cut_with_decimal_sizes() {
    let mut e = Engine::new();
    slab(&mut e, 14.0, 3.0);
    cut(&mut e, [0.0, 2.1], [1.4, 3.5], 3.0, -1.0);
    cut(&mut e, [0.0, 3.5], [2.1, 4.9], 3.0, -3.0);
    assert!(solid(&mut e), "{} bad edges", bad_edges(&e));
}

#[test]
fn the_grid_of_cut_pairs_at_odd_sizes() {
    // the same 0/4/6/10 grid, scaled by 0.7 (no whole numbers), on a 7 x 7 x 2.1 slab
    let k = 0.7;
    let xs = [0.0, 4.0 * k, 6.0 * k, 10.0 * k];
    let mut rects = vec![];
    for i in 0..4 {
        for j in i + 1..4 {
            for a in 0..4 {
                for b in a + 1..4 {
                    rects.push(([xs[i], xs[a]], [xs[j], xs[b]]));
                }
            }
        }
    }
    let full = |r: &([f64; 2], [f64; 2])| r.0 == [0.0, 0.0] && r.1 == [xs[3], xs[3]];
    let overlap = |a: &([f64; 2], [f64; 2]), b: &([f64; 2], [f64; 2])| a.0[0] < b.1[0] && b.0[0] < a.1[0] && a.0[1] < b.1[1] && b.0[1] < a.1[1];
    let corner_only = |a: &([f64; 2], [f64; 2]), b: &([f64; 2], [f64; 2])| (a.1[0] == b.0[0] || b.1[0] == a.0[0]) && (a.1[1] == b.0[1] || b.1[1] == a.0[1]);
    let mut bad = vec![];
    let mut tried = 0;
    for a in &rects {
        for b in &rects {
            if a == b || full(a) || full(b) || overlap(a, b) || corner_only(a, b) {
                continue;
            }
            for d1 in [-0.7, -2.1] {
                tried += 1;
                let mut e = Engine::new();
                slab(&mut e, xs[3], 2.1);
                cut(&mut e, a.0, a.1, 2.1, d1);
                cut(&mut e, b.0, b.1, 2.1, -2.1);
                if !solid(&mut e) {
                    bad.push((*a, d1, *b, bad_edges(&e)));
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
fn cuts_at_the_rim_of_a_turned_slab() {
    // a 10 x 10 x 1 slab turned about z; cuts drawn in its own axes
    let mut bad = vec![];
    let pairs: [([f64; 2], [f64; 2], [f64; 2], [f64; 2]); 4] = [
        ([6.8, 0.0], [7.8, 2.0], [0.0, 0.0], [3.0, 2.0]),
        ([0.0, 3.0], [2.0, 4.0], [0.0, 6.0], [2.0, 7.0]),
        ([4.0, 0.0], [6.0, 2.0], [6.0, 0.0], [8.0, 3.0]),
        ([0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 5.0]),
    ];
    for deg in [20.0_f64, 30.0, 45.5] {
        let (s, c) = deg.to_radians().sin_cos();
        let turn = |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
        let rect = |a: [f64; 2], b: [f64; 2]| [turn(a), turn([b[0], a[1]]), turn(b), turn([a[0], b[1]])];
        for (i, (a0, a1, b0, b1)) in pairs.iter().enumerate() {
            for d1 in [-1.0, -0.5] {
                let mut e = Engine::new();
                let base: Vec<[f64; 3]> = rect([0.0, 0.0], [10.0, 10.0]).iter().map(|q| [q[0], q[1], 0.0]).collect();
                let f = ok(&mut e, json!({"cmd": "polygon", "points": base}))["face"].clone();
                ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 1}));
                cut_poly(&mut e, &rect(*a0, *a1), 1.0, d1);
                cut_poly(&mut e, &rect(*b0, *b1), 1.0, -1.0);
                if !solid(&mut e) {
                    bad.push((deg, i, d1, bad_edges(&e)));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} not solid: {bad:?}", bad.len());
}

/// A small fixed-seed generator, so a failure repeats.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn random_decimal_notches_side_by_side_and_turned_rims() {
    let mut g = Lcg(0x5eed_0311);
    let mut bad = vec![];
    // adjacent edge notches with decimal sizes, the first part way or through
    for _ in 0..300 {
        let r = |g: &mut Lcg, lo: f64, hi: f64| ((lo + (hi - lo) * g.next()) * 100.0).round() / 100.0;
        let (y0, y1) = (r(&mut g, 0.5, 5.0), 0.0);
        let y1 = y1 + y0 + r(&mut g, 0.5, 3.0);
        let y2 = y1 + r(&mut g, 0.5, 3.0);
        let (x1, x2) = (r(&mut g, 0.5, 4.0), r(&mut g, 0.5, 4.0));
        let d1 = if g.next() < 0.5 { -1.3 } else { -3.0 };
        let mut e = Engine::new();
        slab(&mut e, 14.0, 3.0);
        cut(&mut e, [0.0, y0], [x1, y1], 3.0, d1);
        cut(&mut e, [0.0, y1], [x2, y2], 3.0, -3.0);
        if !solid(&mut e) {
            bad.push(format!("notches y {y0}-{y1}-{y2} x {x1}/{x2} d {d1}: {} bad", bad_edges(&e)));
        }
    }
    // the four rim pairs at many angles
    let pairs: [([f64; 2], [f64; 2], [f64; 2], [f64; 2]); 4] = [
        ([6.8, 0.0], [7.8, 2.0], [0.0, 0.0], [3.0, 2.0]),
        ([0.0, 3.0], [2.0, 4.0], [0.0, 6.0], [2.0, 7.0]),
        ([4.0, 0.0], [6.0, 2.0], [6.0, 0.0], [8.0, 3.0]),
        ([0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 5.0]),
    ];
    for deg in (0..90).step_by(7).map(|d| d as f64 + 0.37) {
        let (s, c) = deg.to_radians().sin_cos();
        let turn = |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
        let rect = |a: [f64; 2], b: [f64; 2]| [turn(a), turn([b[0], a[1]]), turn(b), turn([a[0], b[1]])];
        for (i, (a0, a1, b0, b1)) in pairs.iter().enumerate() {
            let mut e = Engine::new();
            let base: Vec<[f64; 3]> = rect([0.0, 0.0], [10.0, 10.0]).iter().map(|q| [q[0], q[1], 0.0]).collect();
            let f = ok(&mut e, json!({"cmd": "polygon", "points": base}))["face"].clone();
            ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 1}));
            cut_poly(&mut e, &rect(*a0, *a1), 1.0, -0.5);
            cut_poly(&mut e, &rect(*b0, *b1), 1.0, -1.0);
            if !solid(&mut e) {
                bad.push(format!("turned {deg} pair {i}: {} bad", bad_edges(&e)));
            }
        }
    }
    for b in &bad {
        eprintln!("NOT SOLID {b}");
    }
    assert!(bad.is_empty(), "{} not solid", bad.len());
}
