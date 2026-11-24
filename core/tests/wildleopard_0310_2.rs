//! Work limits, round two: timing and equivalence tests. Bounds are generous (CI machines are slow); each one is far
//! below what the work used to take.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Edge, Face, Group, Model, Vertex};
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

fn as_agent(e: &mut Engine, v: Value) -> Value {
    e.exec_json_as(&agent(), &v.to_string())
}

fn nid(m: &mut Model) -> u32 {
    m.next_id += 1;
    m.next_id
}

/// `n` closed boxes, each in its own group, `spread` apart (0: all in one place).
fn boxes(e: &mut Engine, n: usize, spread: f64) {
    let m = &mut e.model;
    for i in 0..n {
        let g = nid(m);
        m.groups.insert(g, Group { name: format!("Part {i}"), ..Default::default() });
        let (ox, oy) = ((i % 100) as f64 * spread, (i / 100) as f64 * spread);
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

// ---- export_cnc: one pass over the faces, and a part cap checked first ----

#[test]
fn cnc_files_on_thousands_of_parts_is_fast_and_a_huge_count_is_refused_first() {
    let mut e = Engine::new();
    boxes(&mut e, 4096, 2.0);
    let t = Instant::now();
    let files = sketchcraft::cnc::files(&e.model, &[], 0.0, false).unwrap();
    let took = t.elapsed().as_secs_f64();
    assert_eq!(files.len(), 4096);
    assert!(took < 4.0, "4,096 CNC files took {took:.2} s");
    // numbered in order, one DXF each
    assert!(files[0].file.starts_with("0001-") && files[4095].file.starts_with("4096-"));
    // more than the cap: refused at once, before any measuring
    let mut big = Engine::new();
    boxes(&mut big, 5200, 2.0);
    let t = Instant::now();
    let err = sketchcraft::cnc::files(&big.model, &[], 0.0, false).err().expect("refused").to_string();
    assert!(err.contains("more than CNC files take"), "{err}");
    assert!(t.elapsed().as_secs_f64() < 3.0);
    // a far larger count is refused without measuring at all
    let mut huge = Engine::new();
    boxes(&mut huge, 21_000, 2.0);
    let t = Instant::now();
    assert!(sketchcraft::cnc::files(&huge.model, &[], 0.0, false).is_err());
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:?}", t.elapsed());
}

#[test]
fn cnc_files_of_a_nested_part_include_what_is_inside_it() {
    // an outer group holding a box and an inner group with a box: one part, drawn from both
    let mut e = Engine::new();
    boxes(&mut e, 2, 2.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    e.model.groups.get_mut(&ids[1]).unwrap().parent = Some(ids[0]);
    let files = sketchcraft::cnc::files(&e.model, &[ids[0]], 0.0, false).unwrap();
    assert_eq!(files.len(), 1);
    let text = String::from_utf8(files[0].bytes.clone()).unwrap();
    // the part spans both boxes (3 m = 3000 mm along x)
    assert!(text.contains("3000"), "{}", &text[..text.len().min(400)]);
}

// ---- curves and lines that meet what is drawn ----

#[test]
fn a_sawtooth_curve_across_many_lines_is_refused_quickly_for_agents() {
    let mut e = Engine::new();
    for j in 0..10 {
        run(&mut e, json!({"cmd": "line", "points": [[0, j, 0], [22, j, 0]]}));
    }
    let before = (e.model.edges.len(), e.model.faces.len());
    let pts: Vec<Value> = (0..200).map(|i| json!([i as f64 * 0.1 + 0.05, if i % 2 == 0 { 0.2 } else { 9.5 }, 0])).collect();
    for cmd in ["spline", "line"] {
        let mut c = json!({"cmd": cmd, "points": pts});
        if cmd == "spline" {
            c["segments"] = json!(1);
        }
        let t = Instant::now();
        let r = as_agent(&mut e, c.clone());
        assert_eq!(r["ok"], false, "{cmd}: {r}");
        assert!(r["error"].as_str().unwrap().contains("already drawn"), "{r}");
        assert!(t.elapsed().as_secs_f64() < 2.0, "{cmd} took {:?}", t.elapsed());
        assert_eq!((e.model.edges.len(), e.model.faces.len()), before, "nothing was drawn");
        let r = e.exec_json_as(&Caller::Plugin, &c.to_string());
        assert_eq!(r["ok"], false, "{cmd}: {r}");
    }
}

#[test]
fn a_few_crossings_still_draw_for_agents_and_the_person_is_not_limited() {
    let mut e = Engine::new();
    for j in 0..4 {
        run(&mut e, json!({"cmd": "line", "points": [[0, j, 0], [22, j, 0]]}));
    }
    let pts: Vec<Value> = (0..30).map(|i| json!([i as f64 * 0.5 + 0.05, if i % 2 == 0 { 0.2 } else { 3.5 }, 0])).collect();
    let r = as_agent(&mut e, json!({"cmd": "spline", "points": pts, "segments": 1}));
    assert_eq!(r["ok"], true, "{r}");
    // the person's own drawing has no crossing limit: the sawtooth over ten lines goes through
    let mut p = Engine::new();
    for j in 0..10 {
        run(&mut p, json!({"cmd": "line", "points": [[0, j, 0], [22, j, 0]]}));
    }
    let pts: Vec<Value> = (0..40).map(|i| json!([i as f64 * 0.5 + 0.05, if i % 2 == 0 { 0.2 } else { 9.5 }, 0])).collect();
    let r = run(&mut p, json!({"cmd": "line", "points": pts}));
    assert!(!r["edges"].as_array().unwrap().is_empty());
}

#[test]
fn a_single_piece_across_many_lines_is_within_the_limit_for_agents() {
    // 70 lines, one piece across all of them: under the crossing limit
    let mut e = Engine::new();
    let n = 70;
    for j in 0..n {
        run(&mut e, json!({"cmd": "line", "points": [[0, j, 0], [n, j, 0]]}));
    }
    // crossing the lines 69 times with a single straight piece is within the limit and must still work
    let r = as_agent(&mut e, json!({"cmd": "line", "points": [[0.5, -1, 0], [0.5, 80, 0]]}));
    assert_eq!(r["ok"], true, "{r}");
}

// ---- print check on stacked geometry ----

#[test]
fn print_check_on_thousands_of_stacked_parts_is_bounded_and_says_so() {
    let mut e = Engine::new();
    boxes(&mut e, 8192, 0.0);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "print_check"}));
    let took = t.elapsed().as_secs_f64();
    assert!(took < 12.0, "print_check on 8,192 stacked boxes took {took:.1} s");
    assert_eq!(r["thin_walls"]["sampled"], true, "{}", r["notes"]);
    assert_eq!(r["ready"], false);
    assert!(r["notes"].to_string().contains("overlap"), "{}", r["notes"]);
}

#[test]
fn print_check_of_spread_out_parts_is_still_measured_in_full() {
    let mut e = Engine::new();
    boxes(&mut e, 600, 0.5);
    let r = run(&mut e, json!({"cmd": "print_check"}));
    assert_eq!(r["thin_walls"]["sampled"], false, "{}", r["notes"]);
}

// ---- ids named twice ----

/// A box of six faces wrapped in `depth` nested groups; returns the outermost group.
fn nested_box(e: &mut Engine, depth: usize) -> u32 {
    boxes(e, 1, 2.0);
    let mut inner = *e.model.groups.keys().next().unwrap();
    for i in 0..depth {
        let g = nid(&mut e.model);
        e.model.groups.insert(g, Group { name: format!("Wrap {i}"), ..Default::default() });
        e.model.groups.get_mut(&inner).unwrap().parent = Some(g);
        inner = g;
    }
    inner
}

#[test]
fn repeated_ids_cost_nothing_extra_for_the_commands_that_walk_a_group() {
    let mut e = Engine::new();
    let outer = nested_box(&mut e, 1000);
    let many: Vec<u32> = vec![outer; 5000];
    for c in ["fix_model", "check_solid"] {
        let t = Instant::now();
        let r = e.exec_json(&json!({"cmd": c, "ids": many, "dry_run": true}).to_string());
        assert_eq!(r["ok"], true, "{c}: {r}");
        assert!(t.elapsed().as_secs_f64() < 3.0, "{c} with 5,000 repeated ids took {:?}", t.elapsed());
    }
    let t = Instant::now();
    run(&mut e, json!({"cmd": "move", "ids": vec![outer; 500], "by": [1, 0, 0]}));
    assert!(t.elapsed().as_secs_f64() < 3.0, "move took {:?}", t.elapsed());
    let r = run(&mut e, json!({"cmd": "parts", "ids": vec![outer; 5000]}));
    let list = r["parts"].as_array().cloned().or_else(|| r.as_array().cloned()).unwrap();
    assert_eq!(list.len(), 1, "a part listed twice is one part");
}

#[test]
fn a_listed_id_does_its_work_once_and_keeps_its_place() {
    // a repeat used to mean the same work again: now each id is used once, first mention first
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let f = *e.model.faces.keys().next().unwrap();
    let before = e.model.faces.len();
    run(&mut e, json!({"cmd": "duplicate", "ids": [f, f, f], "by": [5, 0, 0]}));
    assert_eq!(e.model.faces.len(), before + 1, "one copy, not three");
    // moving by a vector moves a thing once, however often it is named
    let x0 = e.model.p(e.model.faces[&f].outer[0]).x;
    run(&mut e, json!({"cmd": "move", "ids": [f, f], "by": [2, 0, 0]}));
    assert!((e.model.p(e.model.faces[&f].outer[0]).x - x0 - 2.0).abs() < 1e-9);
    // lists that are not plain ids are left as they are
    let r = e.exec_json(&json!({"cmd": "move", "ids": [f, "x"], "by": [1, 0, 0]}).to_string());
    assert_eq!(r["ok"], false);
}

#[test]
fn the_subtree_of_a_deep_chain_is_found_in_one_pass() {
    let mut e = Engine::new();
    let outer = nested_box(&mut e, 3000);
    let t = Instant::now();
    for _ in 0..200 {
        assert_eq!(e.model.subtree(outer).len(), 3001);
    }
    assert!(t.elapsed().as_secs_f64() < 3.0, "{:?}", t.elapsed());
    let leaf = *e.model.groups.iter().find(|(_, g)| g.name == "Part 0").map(|(k, _)| k).unwrap();
    assert_eq!(e.model.subtree(leaf).len(), 1);
}

// ---- fix_model on faces from a DXF grid ----

fn dxf_grid(n: usize) -> String {
    let mut d = String::from("0\nSECTION\n2\nENTITIES\n");
    for j in 0..=n {
        d += &format!("0\nLINE\n8\n0\n10\n0\n20\n{j}\n30\n0\n11\n{n}\n21\n{j}\n31\n0\n");
        d += &format!("0\nLINE\n8\n0\n10\n{j}\n20\n0\n30\n0\n11\n{j}\n21\n{n}\n31\n0\n");
    }
    d + "0\nENDSEC\n0\nEOF\n"
}

#[test]
fn fix_model_merges_a_dxf_grid_into_one_face_every_time() {
    for n in [10usize, 45, 100] {
        let d = dxf_grid(n);
        for _ in 0..4 {
            let mut e = Engine::new();
            run(&mut e, json!({"cmd": "import_dxf", "text": d, "units": "m"}));
            assert_eq!(e.model.faces.len(), n * n);
            let t = Instant::now();
            let r = run(&mut e, json!({"cmd": "fix_model"}));
            assert!(t.elapsed().as_secs_f64() < 5.0);
            assert_eq!(e.model.faces.len(), 1, "{n} x {n}: {}", r["notes"]);
            let f = e.model.faces.values().next().unwrap();
            assert!((e.model.face_area(*e.model.faces.keys().next().unwrap()) - (n * n) as f64).abs() < 1e-6);
            assert_eq!(f.outer.len(), n * 4);
        }
    }
}

// ---- lay_out_parts ownership ----

#[test]
fn an_agent_asking_again_does_not_take_the_persons_laid_out_view() {
    let mut e = Engine::new();
    boxes(&mut e, 3, 2.0);
    run(&mut e, json!({"cmd": "lay_out_parts", "on": true}));
    let r = as_agent(&mut e, json!({"cmd": "lay_out_parts", "on": true}));
    assert_eq!(r["ok"], true, "{r}");
    // still the person's view: the person's own edit is refused, not allowed to end it
    let r = e.exec_json(&json!({"cmd": "rect", "from": [60, 60, 0], "to": [61, 61, 0]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(e.laid_out.is_some());
    // a view an agent switched on is still ended by the person's edit
    run(&mut e, json!({"cmd": "lay_out_parts", "on": false}));
    as_agent(&mut e, json!({"cmd": "lay_out_parts", "on": true}));
    run(&mut e, json!({"cmd": "rect", "from": [60, 60, 0], "to": [61, 61, 0]}));
    assert!(e.laid_out.is_none());
}

// ---- place: the undo list is measured once per step, not on every command ----

#[test]
fn placing_thousands_of_components_one_by_one_is_not_quadratic() {
    let mut e = Engine::new();
    boxes(&mut e, 1, 2.0);
    let g = *e.model.groups.keys().next().unwrap();
    run(&mut e, json!({"cmd": "make_component", "ids": [g], "name": "Box"}));
    let t = Instant::now();
    for i in 0..3000 {
        run(&mut e, json!({"cmd": "place", "component": "Box", "at": [i as f64 * 3.0, 0, 0]}));
    }
    let took = t.elapsed().as_secs_f64();
    assert_eq!(e.model.instances.len(), 3001);
    assert!(took < 10.0, "3,000 placements took {took:.1} s");
    // undo still steps back one placement at a time
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.instances.len(), 3000);
}
