//! Work limits and crashes, round three: groups that hold each other, very deep nesting, repeated and many solids,
//! curves that cross themselves near other lines, and copying many groups. Bounds are generous (CI machines are slow);
//! each one is far below what the work used to take.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Edge, Face, Group, Model, Vertex};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::time::{Duration, Instant};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn as_agent(e: &mut Engine, v: Value) -> Value {
    e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &v.to_string())
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

fn loops(m: &Model) -> bool {
    m.has_group_loop()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-wl3-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ---- groups that hold each other ----

#[test]
fn grouping_the_open_group_or_one_around_it_is_refused() {
    let mut e = Engine::new();
    let a = run(&mut e, json!({"cmd": "begin_group", "name": "A"}))["group"].as_u64().unwrap() as u32;
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let before = e.model.groups.len();
    let r = e.exec_json(&json!({"cmd": "group", "ids": [a]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("group you are working in"), "{r}");
    assert_eq!(e.model.groups.len(), before, "nothing was made");
    assert!(!loops(&e.model));
    // one around the open group is refused too
    let b = run(&mut e, json!({"cmd": "begin_group", "name": "B"}))["group"].as_u64().unwrap() as u32;
    run(&mut e, json!({"cmd": "rect", "from": [3, 0, 0], "to": [4, 1, 0]}));
    for id in [a, b] {
        let r = e.exec_json(&json!({"cmd": "group", "ids": [id]}).to_string());
        assert_eq!(r["ok"], false, "{id}: {r}");
    }
    assert!(!loops(&e.model));
    // the group can still be copied, and the engine is fine
    run(&mut e, json!({"cmd": "end_group"}));
    run(&mut e, json!({"cmd": "end_group"}));
    run(&mut e, json!({"cmd": "duplicate", "ids": [a], "by": [0, 5, 0]}));
    assert!(!loops(&e.model));
}

#[test]
fn grouping_things_that_are_not_the_open_group_still_works() {
    let mut e = Engine::new();
    let a = run(&mut e, json!({"cmd": "begin_group", "name": "A"}))["group"].as_u64().unwrap() as u32;
    let b = run(&mut e, json!({"cmd": "begin_group", "name": "B"}))["group"].as_u64().unwrap() as u32;
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    run(&mut e, json!({"cmd": "end_group"}));
    // B (inside the open A) goes into a new group made inside A
    let g = run(&mut e, json!({"cmd": "group", "ids": [b]}))["group"].as_u64().unwrap() as u32;
    assert_eq!(e.model.groups[&b].parent, Some(g));
    assert_eq!(e.model.groups[&g].parent, Some(a));
    run(&mut e, json!({"cmd": "end_group"}));
    // and with nothing open, a group wraps another as before
    let w = run(&mut e, json!({"cmd": "group", "ids": [a]}))["group"].as_u64().unwrap() as u32;
    assert_eq!(e.model.groups[&a].parent, Some(w));
    assert!(!loops(&e.model));
}

#[test]
fn a_loop_in_the_model_is_found_and_refused_by_the_sanity_check() {
    let mut e = Engine::new();
    boxes(&mut e, 3, 2.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    assert!(e.model.check_sane().is_ok() && !loops(&e.model));
    e.model.groups.get_mut(&ids[0]).unwrap().parent = Some(ids[1]);
    e.model.groups.get_mut(&ids[1]).unwrap().parent = Some(ids[2]);
    assert!(!loops(&e.model), "a chain is not a loop");
    e.model.groups.get_mut(&ids[2]).unwrap().parent = Some(ids[0]);
    assert!(loops(&e.model));
    let err = e.model.check_sane().err().expect("refused").to_string();
    assert!(err.contains("loop"), "{err}");
}

#[test]
fn a_saved_file_with_groups_in_a_loop_opens_and_is_repaired() {
    let mut src = Engine::new();
    boxes(&mut src, 5, 2.0);
    let ids: Vec<u32> = src.model.groups.keys().copied().collect();
    // a three-group loop, a group inside itself, and one honest chain on the side
    src.model.groups.get_mut(&ids[0]).unwrap().parent = Some(ids[1]);
    src.model.groups.get_mut(&ids[1]).unwrap().parent = Some(ids[2]);
    src.model.groups.get_mut(&ids[2]).unwrap().parent = Some(ids[0]);
    src.model.groups.get_mut(&ids[3]).unwrap().parent = Some(ids[3]);
    src.model.groups.get_mut(&ids[4]).unwrap().parent = Some(ids[3]);
    let dir = scratch("loopfile");
    let path = dir.join("loop.sketchcraft");
    std::fs::write(&path, serde_json::to_string(&src.model).unwrap()).unwrap();
    let faces = src.model.faces.len();

    for who in ["person", "agent"] {
        let mut e = Engine::new();
        let line = json!({"cmd": "open", "path": path.to_str().unwrap()}).to_string();
        let r = if who == "person" { e.exec_json(&line) } else { e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &line) };
        assert_eq!(r["ok"], true, "{who}: {r}");
        assert_eq!(r["group_loops_repaired"], 2, "{who}: {r}");
        assert_eq!(e.model.faces.len(), faces, "nothing was lost");
        assert!(!loops(&e.model));
        // the very commands that used to run forever
        for &g in &ids {
            run(&mut e, json!({"cmd": "copy", "group": g, "by": [0, 20, 0]}));
            run(&mut e, json!({"cmd": "duplicate", "ids": [g], "by": [0, 40, 0]}));
        }
        run(&mut e, json!({"cmd": "array", "ids": [ids[0]], "by": [0, 3, 0], "count": 2}));
        assert!(!loops(&e.model));
        // saved and reopened, it stays repaired
        let out = dir.join(format!("{who}-out.sketchcraft"));
        run(&mut e, json!({"cmd": "save", "path": out.to_str().unwrap()}));
        let mut again = Engine::new();
        let r = again.exec_json(&json!({"cmd": "open", "path": out.to_str().unwrap()}).to_string());
        assert_eq!(r["ok"], true, "{r}");
        assert!(r["group_loops_repaired"].is_null(), "{r}");
    }
    // a file without loops says nothing about them
    let mut clean = Engine::new();
    boxes(&mut clean, 2, 2.0);
    let p2 = dir.join("clean.sketchcraft");
    std::fs::write(&p2, serde_json::to_string(&clean.model).unwrap()).unwrap();
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "open", "path": p2.to_str().unwrap()}));
    assert!(r["group_loops_repaired"].is_null(), "{r}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_restored_model_with_a_group_loop_is_repaired_too() {
    let mut src = Engine::new();
    boxes(&mut src, 2, 2.0);
    let ids: Vec<u32> = src.model.groups.keys().copied().collect();
    src.model.groups.get_mut(&ids[0]).unwrap().parent = Some(ids[1]);
    src.model.groups.get_mut(&ids[1]).unwrap().parent = Some(ids[0]);
    let mut e = Engine::new();
    e.replace_model(src.model.clone()).unwrap();
    assert!(!loops(&e.model));
    run(&mut e, json!({"cmd": "copy", "group": ids[0], "by": [0, 9, 0]}));
}

#[test]
fn copying_a_very_deeply_nested_group_does_not_overflow_the_stack() {
    // on a thread with a small stack: a copy that recursed once per level would be killed at a few hundred levels
    let t = std::thread::Builder::new()
        .stack_size(512 << 10)
        .spawn(|| {
            let mut e = Engine::new();
            let depth = 30_000u32;
            let m = &mut e.model;
            let mut parent = None;
            let mut top = 0;
            for i in 0..depth {
                let g = nid(m);
                m.groups.insert(g, Group { name: format!("L{i}"), parent, ..Default::default() });
                if i == 0 {
                    top = g;
                }
                parent = Some(g);
            }
            // a rectangle at the very bottom (the model itself: the program refuses nesting this deep, a copy must still be safe if one got here)
            let p = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
            let ids: Vec<u32> = p.iter().map(|q| { let id = nid(&mut e.model); e.model.vertices.insert(id, Vertex { p: *q, group: parent }); id }).collect();
            let f = nid(&mut e.model);
            e.model.faces.insert(f, Face { outer: ids, group: parent, ..Default::default() });
            let faces = e.model.faces.len();
            let t = Instant::now();
            e.model.copy_group(top, DVec3::new(0.0, 5.0, 0.0), None).expect("copy");
            let ids = e.model.duplicate(&[top], DVec3::new(0.0, 9.0, 0.0)).expect("duplicate");
            assert_eq!(e.model.faces.len(), faces * 3);
            assert_eq!(e.model.groups.len(), depth as usize * 3);
            assert!(!e.model.has_group_loop());
            // and what hangs off the deep copy goes with it
            e.model.delete(&[ids[0]]).expect("delete");
            assert_eq!(e.model.groups.len(), depth as usize * 2);
            assert!(t.elapsed().as_secs_f64() < 20.0, "{:?}", t.elapsed());
        })
        .unwrap();
    t.join().expect("a deep copy must not overflow the stack");
}

// ---- solid: repeats are one, and agents have a time limit ----

#[test]
fn solid_with_repeated_ids_is_refused_at_once_and_a_repeat_is_one() {
    let mut e = Engine::new();
    boxes(&mut e, 2, 2.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    let (faces, groups) = (e.model.faces.len(), e.model.groups.len());
    for op in ["union", "intersect", "outer_shell"] {
        let many: Vec<u32> = std::iter::repeat(ids[0]).take(150_000).collect();
        for caller_agent in [true, false] {
            let c = json!({"cmd": "solid", "op": op, "ids": many});
            let t = Instant::now();
            let r = if caller_agent { as_agent(&mut e, c) } else { e.exec_json(&c.to_string()) };
            assert_eq!(r["ok"], false, "{op}: {r}");
            assert!(r["error"].as_str().unwrap().contains("at least two"), "{op}: {r}");
            assert!(t.elapsed().as_secs_f64() < 1.0, "{op} took {:?}", t.elapsed());
            assert_eq!((e.model.faces.len(), e.model.groups.len()), (faces, groups), "put back");
        }
    }
    // a repeat among others is the same as leaving it out
    let r = run(&mut e, json!({"cmd": "solid", "op": "union", "ids": [ids[0], ids[1], ids[0], ids[1], ids[1]]}));
    assert_eq!(r["groups"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(e.model.groups.len(), groups - 1);
}

#[test]
fn solid_subtract_still_asks_for_two_different_solids() {
    let mut e = Engine::new();
    boxes(&mut e, 2, 2.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    for op in ["subtract", "trim", "split"] {
        let r = e.exec_json(&json!({"cmd": "solid", "op": op, "ids": [ids[0], ids[0]]}).to_string());
        assert_eq!(r["ok"], false, "{op}: {r}");
        assert!(r["error"].as_str().unwrap().contains("two"), "{op}: {r}");
    }
}

#[test]
fn solid_for_an_agent_stops_at_its_time_limit_and_is_put_back() {
    let mut e = Engine::new();
    boxes(&mut e, 3000, 2.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    let (faces, groups) = (e.model.faces.len(), e.model.groups.len());
    // an agent's call gets `SOLID_SECONDS`; here the clock is already nearly out
    e.model.deadline = Some(Instant::now() + Duration::from_millis(30));
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "solid", "op": "union", "ids": ids}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("too long"), "{r}");
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    assert_eq!((e.model.faces.len(), e.model.groups.len()), (faces, groups), "put back");
    e.model.deadline = None;
    // the engine is fine afterwards
    run(&mut e, json!({"cmd": "solid", "op": "union", "ids": [ids[0], ids[1]]}));
}

// ---- curves that cross themselves near other lines ----

fn star(n: usize, step: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * ((i * step) % n) as f64 / n as f64;
            json!([5.0 + 4.5 * a.cos(), 5.0 + 4.5 * a.sin(), 0])
        })
        .collect()
}

#[test]
fn a_self_crossing_star_near_a_stray_line_is_refused_fast_for_agents() {
    for near in [false, true] {
        let mut e = Engine::new();
        if near {
            // one stray 1 mm line inside the star's box used to switch the self-crossing count off
            run(&mut e, json!({"cmd": "line", "points": [[5, 5, 0], [5.001, 5, 0]]}));
        }
        let before = (e.model.edges.len(), e.model.vertices.len());
        for (n, step) in [(301usize, 100usize), (701, 300), (1501, 700), (1999, 900)] {
            for cmd in ["spline", "line"] {
                let mut c = json!({"cmd": cmd, "points": star(n, step)});
                if cmd == "spline" {
                    c["segments"] = json!(1);
                }
                let t = Instant::now();
                let r = as_agent(&mut e, c);
                assert_eq!(r["ok"], false, "near={near} {cmd} {n}: {r}");
                assert!(r["error"].as_str().unwrap().contains("itself"), "{r}");
                assert!(t.elapsed().as_secs_f64() < 2.0, "near={near} {cmd} {n} took {:?}", t.elapsed());
                assert_eq!((e.model.edges.len(), e.model.vertices.len()), before, "put back");
            }
        }
    }
}

#[test]
fn a_curve_that_crosses_itself_a_few_times_still_draws_near_other_lines() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "line", "points": [[5, 5, 0], [5.001, 5, 0]]}));
    // a loop-the-loop: a handful of self-crossings
    let pts: Vec<Value> = (0..60)
        .map(|i| {
            let t = i as f64 / 59.0 * 6.0 * std::f64::consts::PI;
            json!([t * 0.8 + 1.5 * t.cos(), 5.0 + 1.5 * t.sin(), 0])
        })
        .collect();
    let r = as_agent(&mut e, json!({"cmd": "spline", "points": pts, "segments": 1}));
    assert_eq!(r["ok"], true, "{r}");
}

// ---- duplicate of many groups ----

#[test]
fn duplicating_thousands_of_groups_is_fast_and_makes_the_same_copies() {
    let mut e = Engine::new();
    boxes(&mut e, 8192, 3.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    let faces = e.model.faces.len();
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "duplicate", "ids": ids, "by": [0, 0, 5]}));
    let took = t.elapsed().as_secs_f64();
    assert_eq!(r["ids"].as_array().unwrap().len(), 8192);
    assert_eq!(e.model.faces.len(), faces * 2);
    assert_eq!(e.model.groups.len(), 16384);
    assert!(took < 8.0, "duplicating 8,192 groups took {took:.2} s");
    // each copy is a whole box moved up by 5 (6 faces, 8 corners at the new height)
    let c = r["ids"][100].as_u64().unwrap() as u32;
    assert_eq!(e.model.faces.values().filter(|f| f.group == Some(c)).count(), 6);
    let zs: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(c)).map(|v| v.p.z).collect();
    assert_eq!(zs.len(), 8);
    assert!(zs.iter().all(|z| *z >= 5.0 && *z <= 5.5), "{zs:?}");
}

#[test]
fn duplicating_nested_groups_copies_the_whole_tree_once() {
    let mut e = Engine::new();
    boxes(&mut e, 4, 3.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    // 1 holds 2 and 3; 3 holds 4
    e.model.groups.get_mut(&ids[1]).unwrap().parent = Some(ids[0]);
    e.model.groups.get_mut(&ids[2]).unwrap().parent = Some(ids[0]);
    e.model.groups.get_mut(&ids[3]).unwrap().parent = Some(ids[2]);
    let (groups, faces) = (e.model.groups.len(), e.model.faces.len());
    let r = run(&mut e, json!({"cmd": "duplicate", "ids": [ids[0]], "by": [0, 0, 5]}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 1);
    assert_eq!(e.model.groups.len(), groups * 2);
    assert_eq!(e.model.faces.len(), faces * 2);
    let top = r["ids"][0].as_u64().unwrap() as u32;
    let kids: Vec<u32> = e.model.groups.iter().filter(|(_, g)| g.parent == Some(top)).map(|(k, _)| *k).collect();
    assert_eq!(kids.len(), 2);
    assert!(!loops(&e.model));
    let r = run(&mut e, json!({"cmd": "array", "ids": [ids[0]], "by": [0, 0, 9], "count": 3}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 3);
    assert_eq!(e.model.groups.len(), groups * 5);
}

#[test]
fn duplicating_for_an_agent_stops_at_its_time_limit_and_is_put_back() {
    let mut e = Engine::new();
    boxes(&mut e, 4096, 3.0);
    let ids: Vec<u32> = e.model.groups.keys().copied().collect();
    let (faces, groups) = (e.model.faces.len(), e.model.groups.len());
    e.model.deadline = Some(Instant::now() + Duration::from_millis(5));
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "duplicate", "ids": ids, "by": [0, 0, 5]}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("too long"), "{r}");
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    assert_eq!((e.model.faces.len(), e.model.groups.len()), (faces, groups), "put back");
}
