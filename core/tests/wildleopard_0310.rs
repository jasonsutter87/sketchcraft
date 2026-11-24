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

// ----- many groups, soften, polygon / pushpull on big models, 3MF names, print check, laid-out view -----------------

use sketchcraft::model::Group;

/// `n` unit boxes, each its own group, built straight into the model.
fn many_boxes(e: &mut Engine, n: usize) {
    let m = &mut e.model;
    for i in 0..n {
        let g = nid(m);
        m.groups.insert(g, Group { name: format!("Part {i}"), ..Default::default() });
        let (ox, oy) = ((i % 100) as f64 * 2.0, (i / 100) as f64 * 2.0);
        let v: Vec<u32> = (0..8)
            .map(|k| {
                let id = nid(m);
                m.vertices.insert(id, Vertex { p: DVec3::new(ox + (k & 1) as f64, oy + ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64 * 0.5), group: Some(g) });
                id
            })
            .collect();
        let quads: [[usize; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [1, 3, 7, 5], [3, 2, 6, 7], [2, 0, 4, 6]];
        for q in quads {
            let outer: Vec<u32> = q.iter().map(|&k| v[k]).collect();
            for j in 0..4 {
                let k = nid(m);
                m.edges.insert(k, Edge { a: outer[j], b: outer[(j + 1) % 4], group: Some(g), ..Default::default() });
            }
            let f = nid(m);
            m.faces.insert(f, Face { outer, group: Some(g), ..Default::default() });
        }
    }
}

#[test]
fn parts_on_thousands_of_groups_is_fast_and_the_cut_plan_checks_the_count_first() {
    let mut e = Engine::new();
    many_boxes(&mut e, 8000);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "parts"}));
    let took = t.elapsed().as_secs_f64();
    assert_eq!(r["parts"].as_array().map_or(r.as_array().map_or(0, Vec::len), Vec::len), 8000, "{}", &r.to_string()[..200]);
    assert!(took < 2.0, "parts on 8,000 groups took {took:.2} s");
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "parts_layout"}));
    assert!(t.elapsed().as_secs_f64() < 3.0, "parts_layout took {:.2} s", t.elapsed().as_secs_f64());
    assert!(r["parts"].as_array().unwrap().len() == 8000);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "lay_out_parts", "on": true}));
    let took = t.elapsed().as_secs_f64();
    assert!(took < 4.0, "lay_out_parts took {took:.2} s");
    assert_eq!(r["parts"].as_array().unwrap().len(), 8000);
    run(&mut e, json!({"cmd": "lay_out_parts", "on": false}));
    // more parts than a cut plan takes
    let r = e.exec_json(&json!({"cmd": "cut_plan"}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("more than a cut plan takes"), "{r}");
}

#[test]
fn nested_groups_are_measured_with_everything_inside_them() {
    // an outer group holding a box and an inner group with a box: asked by id, the outer part spans both
    let mut e = Engine::new();
    many_boxes(&mut e, 2);
    let (a, b) = {
        let mut ids = e.model.groups.keys().copied();
        (ids.next().unwrap(), ids.next().unwrap())
    };
    e.model.groups.get_mut(&b).unwrap().parent = Some(a);
    let r = run(&mut e, json!({"cmd": "parts", "ids": [a]}));
    let list = r["parts"].as_array().cloned().or_else(|| r.as_array().cloned()).unwrap();
    assert_eq!(list.len(), 1);
    // box a spans x 0..1, box b x 2..3 (offset 2): together 3 long
    assert!((list[0]["size"][0].as_f64().unwrap() - 3.0).abs() < 1e-6, "{list:?}");
}

#[test]
fn soften_coplanar_is_fast_on_a_big_flat_grid() {
    let mut e = Engine::new();
    grid(&mut e, 283); // 80,089 faces
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "soften", "coplanar": true}));
    let took = t.elapsed().as_secs_f64();
    assert!(took < 2.0, "soften coplanar on 80k faces took {took:.2} s");
    // every edge between two faces: 2 * 283 * 282
    assert_eq!(r["edges"], 2 * 283 * 282);
}

#[test]
fn soften_and_edge_style_ignore_repeated_ids() {
    let mut e = Engine::new();
    many_boxes(&mut e, 1);
    let g = *e.model.groups.keys().next().unwrap();
    let once = run(&mut e, json!({"cmd": "soften", "ids": [g]}))["edges"].as_u64().unwrap();
    let t = Instant::now();
    let rep = run(&mut e, json!({"cmd": "soften", "ids": vec![g; 20000]}))["edges"].as_u64().unwrap();
    assert!(t.elapsed().as_secs_f64() < 1.0);
    assert_eq!(rep, once, "a repeated id is softened once");
    let f = *e.model.faces.keys().next().unwrap();
    let r = run(&mut e, json!({"cmd": "edge_style", "ids": vec![f; 20000], "weight_mm": 0.5}));
    assert_eq!(r["edges"], 4, "one face's four edges, not 20,000 times over");
}

#[test]
fn polygon_and_pushpull_are_not_slowed_by_a_big_model() {
    let mut e = Engine::new();
    grid(&mut e, 200); // 40,401 vertices
    let ring: Vec<Value> = (0..2000).map(|i| {
        let a = i as f64 * std::f64::consts::TAU / 2000.0;
        json!([a.cos() * 40.0 + 500.0, a.sin() * 40.0, 0.0])
    }).collect();
    let t = Instant::now();
    let r = e.exec_json_as(&agent(), &json!({"cmd": "polygon", "points": ring}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    let face = r["face"].as_u64().unwrap();
    let r = e.exec_json_as(&agent(), &json!({"cmd": "pushpull", "face": face, "distance": 3.0, "toward": [0, 0, 1]}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    let took = t.elapsed().as_secs_f64();
    assert!(took < 2.0, "polygon + pushpull of 2,000 points on a 40k-vertex model took {took:.2} s");
    // a cylinder: 2,000 sides, a top and the bottom
    assert!(e.model.faces.len() >= 40_000 + 2_000);
}

#[test]
fn a_3mf_name_with_non_characters_is_still_well_formed() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "begin_group", "name": "A\u{FFFE}B\u{FFFF}C\u{FDD0}D\u{0007}E<&>\"F"}))["group"].as_u64().unwrap();
    let f = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [0.02, 0.02, 0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 0.02, "toward": [0, 0, 1]}));
    run(&mut e, json!({"cmd": "end_group"}));
    let _ = g;
    let dir = std::env::temp_dir().join(format!("sc-wl-3mf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.3mf");
    run(&mut e, json!({"cmd": "export_3mf", "path": path.to_str().unwrap()}));
    let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut z.by_name("3D/3dmodel.model").unwrap(), &mut xml).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(xml.chars().all(|c| c == '\n' || c == '\t' || c == '\r' || !c.is_control()), "a control character is in the XML");
    assert!(!xml.contains('\u{FFFE}') && !xml.contains('\u{FFFF}') && !xml.contains('\u{FDD0}'));
    assert!(xml.contains("name=\"ABCDE&lt;&amp;&gt;&quot;F\""), "{}", xml.lines().find(|l| l.contains("<object")).unwrap_or(""));
}

#[test]
fn print_check_measures_every_triangle_of_a_big_clean_model_and_calls_it_ready() {
    let mut e = Engine::new();
    // 1,200 separate solid cubes (14,400 triangles): the wall check measures every one of them, so a clean one is ready
    let m = &mut e.model;
    for i in 0..1200 {
        let (ox, oy) = ((i % 40) as f64 * 0.004, (i / 40) as f64 * 0.004);
        let v: Vec<u32> = (0..8).map(|k| vert(m, ox + (k & 1) as f64 * 0.003, oy + ((k >> 1) & 1) as f64 * 0.003, ((k >> 2) & 1) as f64 * 0.003)).collect();
        let quads: [[usize; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [1, 3, 7, 5], [3, 2, 6, 7], [2, 0, 4, 6]];
        for q in quads {
            let outer: Vec<u32> = q.iter().map(|&k| v[k]).collect();
            for j in 0..4 {
                edge(m, outer[j], outer[(j + 1) % 4]);
            }
            let f = nid(m);
            m.faces.insert(f, Face { outer, ..Default::default() });
        }
    }
    let r = run(&mut e, json!({"cmd": "print_check"}));
    assert_eq!(r["thin_walls"]["sampled"], false, "{}", r["notes"]);
    assert_eq!(r["thin_walls"]["count"], 0);
    assert_eq!(r["ready"], true, "{}", r["notes"]);
}

#[test]
fn a_person_edit_ends_a_laid_out_view_an_agent_switched_on() {
    let mut e = Engine::new();
    many_boxes(&mut e, 3);
    let r = e.exec_json_as(&agent(), &json!({"cmd": "lay_out_parts", "on": true}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    // an agent's own edits are still refused while it is on
    let r = e.exec_json_as(&agent(), &json!({"cmd": "rect", "from": [50, 50, 0], "to": [51, 51, 0]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(e.laid_out.is_some());
    // the person's edit ends the view and goes through
    let n = e.model.faces.len();
    run(&mut e, json!({"cmd": "rect", "from": [50, 50, 0], "to": [51, 51, 0]}));
    assert!(e.laid_out.is_none());
    assert_eq!(e.model.faces.len(), n + 1);
    // a view the person switched on keeps refusing edits
    run(&mut e, json!({"cmd": "lay_out_parts", "on": true}));
    let r = e.exec_json(&json!({"cmd": "rect", "from": [60, 60, 0], "to": [61, 61, 0]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
}
