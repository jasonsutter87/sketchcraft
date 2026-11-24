//! Work limits and faster paths for big models: timing and equivalence tests. Bounds are generous (CI machines are
//! slow); each one is far below what the work used to take.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Edge, Face, Model, Vertex};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::time::Instant;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn nid(m: &mut Model) -> u32 {
    m.next_id += 1;
    m.next_id
}

fn vert(m: &mut Model, x: f64, y: f64, z: f64) -> u32 {
    let v = nid(m);
    m.vertices.insert(v, Vertex { p: DVec3::new(x, y, z), group: None });
    v
}

fn edge(m: &mut Model, a: u32, b: u32) {
    let k = nid(m);
    m.edges.insert(k, Edge { a, b, group: None, ..Default::default() });
}

/// An n x n flat grid of unit quads straight in the model (no drawing, so building it is cheap).
fn grid(e: &mut Engine, n: usize) {
    let m = &mut e.model;
    let mut id = vec![0u32; (n + 1) * (n + 1)];
    for j in 0..=n {
        for i in 0..=n {
            id[j * (n + 1) + i] = vert(m, i as f64, j as f64, 0.0);
        }
    }
    for j in 0..=n {
        for i in 0..=n {
            if i < n {
                edge(m, id[j * (n + 1) + i], id[j * (n + 1) + i + 1]);
            }
            if j < n {
                edge(m, id[j * (n + 1) + i], id[(j + 1) * (n + 1) + i]);
            }
        }
    }
    for j in 0..n {
        for i in 0..n {
            let f = nid(m);
            let outer = vec![id[j * (n + 1) + i], id[j * (n + 1) + i + 1], id[(j + 1) * (n + 1) + i + 1], id[(j + 1) * (n + 1) + i]];
            m.faces.insert(f, Face { outer, ..Default::default() });
        }
    }
}

#[test]
fn fix_model_merges_a_big_flat_grid_quickly() {
    let mut e = Engine::new();
    grid(&mut e, 150);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "fix_model"}));
    let took = t.elapsed().as_secs_f64();
    assert!(took < 2.0, "fix_model on a 150 x 150 flat grid took {took:.2} s");
    assert_eq!(e.model.faces.len(), 1, "one flat face");
    assert_eq!(r["merged_faces"].as_array().unwrap().len(), 1);
    assert_eq!(r["merged_faces"][0]["with"].as_array().unwrap().len(), 150 * 150 - 1);
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.faces.len(), 150 * 150);
}

#[test]
fn fix_model_merge_keeps_the_same_face_and_corners_on_a_strip() {
    // a 1 x 6 strip: the face kept is the first, the others are listed, every corner along the strip stays
    let mut s = Engine::new();
    {
        let m = &mut s.model;
        let mut top = vec![];
        let mut bot = vec![];
        for i in 0..=6 {
            bot.push(vert(m, i as f64, 0.0, 0.0));
            top.push(vert(m, i as f64, 1.0, 0.0));
        }
        for i in 0..6 {
            edge(m, bot[i], bot[i + 1]);
            edge(m, top[i], top[i + 1]);
        }
        for i in 0..=6 {
            edge(m, bot[i], top[i]);
        }
        for i in 0..6 {
            let f = nid(m);
            m.faces.insert(f, Face { outer: vec![bot[i], bot[i + 1], top[i + 1], top[i]], ..Default::default() });
        }
    }
    let first = *s.model.faces.keys().next().unwrap();
    let r = run(&mut s, json!({"cmd": "fix_model"}));
    assert_eq!(s.model.faces.len(), 1);
    assert_eq!(r["merged_faces"][0]["id"], first);
    assert_eq!(r["merged_faces"][0]["with"].as_array().unwrap().len(), 5);
    assert_eq!(s.model.faces[&first].outer.len(), 14);
}

#[test]
fn fix_model_leaves_a_ring_of_flat_faces_with_a_hole_alone() {
    // a 3 x 3 grid with its middle square missing: the outline is two loops, which is not one face
    let mut e = Engine::new();
    grid(&mut e, 3);
    let mid = *e.model.faces.keys().nth(4).unwrap();
    e.model.faces.remove(&mid);
    run(&mut e, json!({"cmd": "fix_model", "stray_edges": false}));
    assert!(e.model.faces.len() >= 2 && e.model.faces.len() < 8, "{} faces", e.model.faces.len());
    let area: f64 = e.model.faces.keys().map(|&f| e.model.face_area(f)).sum();
    assert!((area - 8.0).abs() < 1e-9, "area {area}");
}

#[test]
fn fix_model_removes_a_long_chain_of_stray_lines_quickly() {
    let mut e = Engine::new();
    // one square face, and a chain of 60,000 loose lines from far away ending on its corner (ids ascend away from
    // the face: the worst order for a pass-by-pass peel)
    let m = &mut e.model;
    let sq: Vec<u32> = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]].iter().map(|p| vert(m, p[0], p[1], 0.0)).collect();
    for i in 0..4 {
        edge(m, sq[i], sq[(i + 1) % 4]);
    }
    let f = nid(m);
    m.faces.insert(f, Face { outer: sq.clone(), ..Default::default() });
    let n = 60_000usize;
    let vs: Vec<u32> = (0..n).map(|i| vert(m, 10.0 + (n - i) as f64, 5.0, 0.0)).collect();
    for i in 0..n {
        let b = if i == n - 1 { sq[2] } else { vs[i + 1] };
        edge(m, vs[i], b);
    }
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "fix_model", "dry_run": true}));
    let dry = t.elapsed().as_secs_f64();
    assert!(dry < 2.0, "dry run took {dry:.2} s");
    assert_eq!(r["changes"], n);
    let listed = r["stray_edges"].as_array().unwrap().len();
    assert!(listed <= 1000, "the reply lists a few and counts the rest");
    assert_eq!(r["more_stray_edges"], n - listed);
    let t = Instant::now();
    run(&mut e, json!({"cmd": "fix_model"}));
    let took = t.elapsed().as_secs_f64();
    assert!(took < 2.0, "fix_model took {took:.2} s");
    assert_eq!(e.model.edges.len(), 4);
}

#[test]
fn spline_and_bezier_from_agents_are_limited_by_edges_not_points() {
    let mut e = Engine::new();
    let pts: Vec<Value> = (0..2000).map(|i| json!([(i as f64 * 0.05).cos() * 3.0, (i as f64 * 0.05).sin() * 3.0, i as f64 * 0.01])).collect();
    let t = Instant::now();
    let r = e.exec_json_as(&agent(), &json!({"cmd": "spline", "points": pts, "segments": 10}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(t.elapsed().as_secs_f64() < 1.0);
    assert!(r["error"].as_str().unwrap().contains("edges"), "{r}");
    assert!(e.model.edges.is_empty());
    let few: Vec<Value> = pts.iter().take(40).cloned().collect();
    let r = e.exec_json_as(&agent(), &json!({"cmd": "spline", "points": few, "segments": 10}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    let ctrl: Vec<Value> = (0..1000).map(|i| json!([i as f64, (i % 7) as f64, 0])).collect();
    let r = e.exec_json_as(&Caller::Plugin, &json!({"cmd": "bezier", "points": ctrl, "segments": 16}).to_string());
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn a_long_plain_curve_is_drawn_in_one_pass() {
    // the person's own curve may be longer than an agent's; it must still be fast
    let mut e = Engine::new();
    let pts: Vec<Value> = (0..1000).map(|i| json!([(i as f64 * 0.05).cos() * 3.0, (i as f64 * 0.05).sin() * 3.0, i as f64 * 0.01])).collect();
    let t = Instant::now();
    let r = e.exec_json(&json!({"cmd": "spline", "points": pts, "segments": 10}).to_string());
    let took = t.elapsed().as_secs_f64();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["edges"].as_array().unwrap().len(), 999 * 10);
    assert!(took < 3.0, "a 10,000-edge helix took {took:.2} s");
}

#[test]
fn a_curve_that_crosses_itself_a_lot_is_refused_quickly() {
    let mut e = Engine::new();
    // 200 points scattered in a small square: thousands of crossings
    let mut s = 12345u64;
    let mut rnd = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as f64) / (1u64 << 31) as f64
    };
    let pts: Vec<Value> = (0..200).map(|_| json!([rnd() * 10.0, rnd() * 10.0, 0])).collect();
    let t = Instant::now();
    let r = e.exec_json_as(&agent(), &json!({"cmd": "spline", "points": pts, "segments": 10}).to_string());
    let took = t.elapsed().as_secs_f64();
    assert!(took < 3.0, "took {took:.2} s");
    assert_eq!(r["ok"], false, "{r}");
    assert!(e.model.edges.is_empty());
}

fn curve_shape(e: &Engine) -> (usize, usize, usize, Vec<[i64; 3]>) {
    let mut pts: Vec<[i64; 3]> = e.model.vertices.values().map(|v| [(v.p.x * 1e6).round() as i64, (v.p.y * 1e6).round() as i64, (v.p.z * 1e6).round() as i64]).collect();
    pts.sort();
    (e.model.vertices.len(), e.model.edges.len(), e.model.faces.len(), pts)
}

#[test]
fn the_one_pass_curve_matches_line_by_line_drawing() {
    // the same curves, with and without a stray edge inside their box (which sends them down the piece-by-piece path)
    for closed in [false, true] {
        let pts: Vec<Value> = (0..40)
            .map(|i| {
                let a = i as f64 * 0.157;
                json!([a.cos() * 4.0, a.sin() * 4.0, 0.0])
            })
            .collect();
        let cmd = json!({"cmd": "spline", "points": pts, "closed": closed, "segments": 3});
        let mut fast = Engine::new();
        let r1 = run(&mut fast, cmd.clone());
        let mut slow = Engine::new();
        run(&mut slow, json!({"cmd": "line", "points": [[0.0, 0.0, 0.0], [0.0, 0.001, 0.0]]}));
        let r2 = run(&mut slow, cmd);
        assert_eq!(r1["edges"].as_array().unwrap().len(), r2["edges"].as_array().unwrap().len(), "closed {closed}");
        assert_eq!(r1["faces"].as_array().unwrap().len(), r2["faces"].as_array().unwrap().len(), "closed {closed}");
        let (mut a, mut b) = (curve_shape(&fast), curve_shape(&slow));
        // the stray edge adds two points to the slow model
        assert_eq!(a.0 + 2, b.0);
        assert_eq!(a.1 + 1, b.1);
        assert_eq!(a.2, b.2);
        b.3.retain(|p| !(p[0] == 0 && p[2] == 0 && p[1].abs() <= 1000));
        a.3.retain(|p| !(p[0] == 0 && p[2] == 0 && p[1].abs() <= 1000));
        assert_eq!(a.3, b.3);
        let curves = |e: &Engine| e.model.edges.values().filter(|x| x.curve.is_some()).count();
        assert_eq!(curves(&fast), curves(&slow));
    }
}

#[test]
fn a_curve_crossing_itself_a_few_times_still_draws() {
    let mut e = Engine::new();
    // a figure of eight, sampled finely: one crossing
    let pts: Vec<Value> = (0..=60)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 60.0;
            json!([a.sin() * 4.0, (2.0 * a).sin() * 2.0, 0.0])
        })
        .collect();
    let r = run(&mut e, json!({"cmd": "spline", "points": pts, "segments": 2}));
    assert!(r["edges"].as_array().unwrap().len() > 100, "{r}");
}
