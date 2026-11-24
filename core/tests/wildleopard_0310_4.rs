//! Work limits, round four: groups nested very deep, repairing many group loops in one pass, quadratic scans in
//! make-component, explode and delete, copying a group and the group inside it together, one big boolean, and the
//! wording of the repair notice. Bounds are generous (CI machines are slow); each is far below what the work used to take.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Edge, Face, Group, GroupRepair, Model, Vertex, MAX_GROUP_DEPTH};
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

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-wl4-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A unit square face in group `g` (None: loose) at x offset `x`.
fn quad(m: &mut Model, g: Option<u32>, x: f64, y: f64) -> u32 {
    let ids: Vec<u32> = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        .iter()
        .map(|(dx, dy)| {
            let id = nid(m);
            m.vertices.insert(id, Vertex { p: DVec3::new(x + dx, y + dy, 0.0), group: g });
            id
        })
        .collect();
    for j in 0..4 {
        let k = nid(m);
        m.edges.insert(k, Edge { a: ids[j], b: ids[(j + 1) % 4], group: g, ..Default::default() });
    }
    let f = nid(m);
    m.faces.insert(f, Face { outer: ids, group: g, ..Default::default() });
    f
}

/// `n` loose squares in a grid; returns their face ids.
fn grid(m: &mut Model, n: usize, y0: f64) -> Vec<u32> {
    (0..n).map(|i| quad(m, None, (i % 200) as f64 * 2.0, y0 + (i / 200) as f64 * 2.0)).collect()
}

/// A chain of `depth` groups (each inside the one before) with a square at the bottom; the ids from the top down.
fn chain(m: &mut Model, depth: usize) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut parent = None;
    for i in 0..depth {
        let g = nid(m);
        m.groups.insert(g, Group { name: format!("Level {i}"), parent, ..Default::default() });
        ids.push(g);
        parent = Some(g);
    }
    quad(m, parent, 0.0, 0.0);
    ids
}

fn write_model(dir: &std::path::Path, name: &str, m: &Model) -> String {
    let p = dir.join(name);
    std::fs::write(&p, serde_json::to_string(m).unwrap()).unwrap();
    p.to_str().unwrap().to_string()
}

// ---- nesting depth ----

#[test]
fn groups_nest_to_the_limit_and_no_further() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let face = e.model.faces.keys().next().copied().unwrap();
    let mut top = run(&mut e, json!({"cmd": "group", "ids": [face]}))["group"].as_u64().unwrap() as u32;
    let deepest = top;
    for _ in 1..MAX_GROUP_DEPTH {
        top = run(&mut e, json!({"cmd": "group", "ids": [top]}))["group"].as_u64().unwrap() as u32;
    }
    assert_eq!(e.model.group_nesting(), Some(MAX_GROUP_DEPTH as u32));
    let before = (e.model.groups.len(), e.model.faces.len());
    // one more is refused, for the person and for an agent, and nothing changed
    for agent in [false, true] {
        let c = json!({"cmd": "group", "ids": [top]});
        let r = if agent { as_agent(&mut e, c) } else { e.exec_json(&c.to_string()) };
        assert_eq!(r["ok"], false, "{r}");
        assert!(r["error"].as_str().unwrap().contains("levels deep"), "{r}");
        assert_eq!((e.model.groups.len(), e.model.faces.len()), before);
    }
    // so is opening a group inside the deepest one
    e.model.active_group = Some(deepest);
    let r = e.exec_json(&json!({"cmd": "begin_group", "name": "one too many"}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("levels deep"), "{r}");
    assert_eq!(e.model.groups.len(), before.0);
    e.model.active_group = None;
    // copying the whole chain beside itself is fine (same depth)
    let r = run(&mut e, json!({"cmd": "copy", "group": top, "by": [5, 0, 0]}));
    assert!(r["group"].is_number());
    assert_eq!(e.model.group_nesting(), Some(MAX_GROUP_DEPTH as u32));
    assert_eq!(e.model.groups.len(), before.0 * 2);
}

#[test]
fn moving_a_group_under_one_that_would_pass_the_limit_is_refused_and_copies_stay_put() {
    let mut e = Engine::new();
    let ids = chain(&mut e.model, MAX_GROUP_DEPTH - 1);
    // a second, shallow group with a square in it, copied while a deep group is open: the copy lands in the open group
    let shallow = nid(&mut e.model);
    e.model.groups.insert(shallow, Group { name: "Shallow".into(), ..Default::default() });
    let inner = nid(&mut e.model);
    e.model.groups.insert(inner, Group { name: "Inner".into(), parent: Some(shallow), ..Default::default() });
    quad(&mut e.model, Some(inner), 9.0, 9.0);
    let before = (e.model.groups.len(), e.model.faces.len());
    // the person's paste and an agent's duplicate go through `duplicate`, which keeps the copy where the original is: depth 2
    run(&mut e, json!({"cmd": "duplicate", "ids": [shallow], "by": [1, 0, 0]}));
    assert_eq!(e.model.groups.len(), before.0 + 2);
    // a group with a group inside it, moved under the deepest level, would pass the limit: refused, nothing moved
    let deepest = *ids.last().unwrap();
    let r = e.exec_json(&json!({"cmd": "set_parent", "ids": [shallow], "parent": deepest}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("levels deep"), "{r}");
    assert_eq!(e.model.groups[&shallow].parent, None);
    // the same group, one level up, fits
    let up = ids[ids.len() - 2];
    run(&mut e, json!({"cmd": "set_parent", "ids": [shallow], "parent": up}));
    assert_eq!(e.model.group_nesting(), Some(MAX_GROUP_DEPTH as u32));
}

#[test]
fn opening_a_file_nested_too_deep_brings_it_up_to_the_limit() {
    let dir = scratch("deepfile");
    let depth = 5_000;
    let mut src = Model::default();
    let ids = chain(&mut src, depth);
    // a placement and a second branch hanging off a deep group
    let def_face_model = {
        let mut d = Model::default();
        quad(&mut d, None, 0.0, 0.0);
        d
    };
    let did = nid(&mut src);
    src.definitions.insert(did, sketchcraft::model::Definition { name: "Part".into(), model: std::sync::Arc::new(def_face_model), attrs: Default::default(), source: None, cuts: false, face_me: false });
    let iid = nid(&mut src);
    src.instances.insert(iid, sketchcraft::model::Instance { parent: Some(ids[depth - 3]), def: did, name: "Part".into(), transform: glam::DMat4::IDENTITY, tag: None, hidden: false, attrs: Default::default() });
    let faces = src.faces.len();
    let path = write_model(&dir, "deep.sketchcraft", &src);
    for who in ["person", "agent"] {
        let mut e = Engine::new();
        let line = json!({"cmd": "open", "path": path}).to_string();
        let t = Instant::now();
        let r = if who == "person" { e.exec_json(&line) } else { e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &line) };
        assert_eq!(r["ok"], true, "{who}: {r}");
        assert!(t.elapsed().as_secs_f64() < 3.0, "{who}: {:?}", t.elapsed());
        assert_eq!(r["groups_flattened"], depth - MAX_GROUP_DEPTH, "{who}: {r}");
        assert!(r["groups_flattened_names"].as_array().unwrap().len() <= GroupRepair::NAMED, "{r}");
        assert!(r["group_loops_repaired"].is_null(), "{r}");
        // every group and square is still there, the depth is the limit, the placement is where it was or the nearest group
        assert_eq!(e.model.groups.len(), depth);
        assert_eq!(e.model.faces.len(), faces);
        assert_eq!(e.model.group_nesting(), Some(MAX_GROUP_DEPTH as u32));
        assert!(e.model.instances[&iid].parent.is_some_and(|p| e.model.groups.contains_key(&p)));
        // the model works: copy, duplicate, save, reopen with nothing more to say
        run(&mut e, json!({"cmd": "copy", "group": ids[0], "by": [0, 20, 0]}));
        let out = dir.join(format!("{who}-out.sketchcraft"));
        run(&mut e, json!({"cmd": "save", "path": out.to_str().unwrap()}));
        let mut again = Engine::new();
        let r = run(&mut again, json!({"cmd": "open", "path": out.to_str().unwrap()}));
        assert!(r["groups_flattened"].is_null(), "{r}");
    }
    // the same file through the model's own repair, and what it says about itself
    let mut m = src.clone();
    let rep = m.repair_group_nesting();
    assert_eq!(rep.too_deep, depth - MAX_GROUP_DEPTH);
    assert_eq!(rep.deep_names.len(), GroupRepair::NAMED);
    let note = rep.notice("file").unwrap();
    assert!(note.contains("more"), "{note}");
    assert!(note.contains("Level 256"), "{note}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_very_deep_file_is_repaired_in_time_proportional_to_its_groups() {
    let dir = scratch("verydeep");
    let mut src = Model::default();
    chain(&mut src, 200_000);
    let path = write_model(&dir, "deeper.sketchcraft", &src);
    let mut e = Engine::new();
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "open", "path": path}));
    assert_eq!(r["groups_flattened"], 200_000 - MAX_GROUP_DEPTH, "{r}");
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_model_at_realistic_depth_opens_untouched() {
    let dir = scratch("shallow");
    let mut src = Model::default();
    chain(&mut src, 40);
    let path = write_model(&dir, "ok.sketchcraft", &src);
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "open", "path": path}));
    assert!(r["groups_flattened"].is_null() && r["group_loops_repaired"].is_null(), "{r}");
    assert_eq!(e.model.group_nesting(), Some(40));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_component_with_deep_groups_inside_is_repaired_and_a_restore_and_a_library_part_say_so() {
    let dir = scratch("viadef");
    let mut part = Model::default();
    chain(&mut part, MAX_GROUP_DEPTH + 40);
    let mut src = Model::default();
    let did = nid(&mut src);
    src.definitions.insert(did, sketchcraft::model::Definition { name: "Tower".into(), model: std::sync::Arc::new(part.clone()), attrs: Default::default(), source: None, cuts: false, face_me: false });
    let rep = {
        let mut m = src.clone();
        m.repair_group_nesting()
    };
    assert_eq!(rep.too_deep, 40);
    assert!(rep.deep_names[0].contains("(in Tower)"), "{:?}", rep.deep_names);
    // restore: the engine's replace_model reports it
    let mut e = Engine::new();
    let rep = e.replace_model(src).unwrap();
    assert_eq!(rep.too_deep, 40);
    assert!(e.model.definitions[&did].model.group_nesting().unwrap() <= MAX_GROUP_DEPTH as u32);
    // a library part: a file with the same chain; inserted, it says the same thing
    let p = write_model(&dir, "Tower.sketchcraft", &part);
    let (_, rep) = sketchcraft::library::load_file_reporting(&p).unwrap();
    assert_eq!(rep.too_deep, 40);
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd": "insert_file", "path": p, "at": [0, 0, 0]}));
    assert_eq!(r["groups_flattened"], 40, "{r}");
    std::fs::remove_dir_all(&dir).ok();
}

// ---- loops: one pass ----

#[test]
fn fifty_thousand_group_loops_are_repaired_in_one_pass() {
    let dir = scratch("manyloops");
    let mut src = Model::default();
    for _ in 0..50_000 {
        let a = nid(&mut src);
        let b = nid(&mut src);
        src.groups.insert(a, Group { name: "A".into(), parent: Some(b), ..Default::default() });
        src.groups.insert(b, Group { name: "B".into(), parent: Some(a), ..Default::default() });
    }
    let path = write_model(&dir, "loops.sketchcraft", &src);
    for who in ["person", "agent"] {
        let mut e = Engine::new();
        let line = json!({"cmd": "open", "path": path}).to_string();
        let t = Instant::now();
        let r = if who == "person" { e.exec_json(&line) } else { as_agent(&mut e, json!({"cmd": "open", "path": path})) };
        assert_eq!(r["ok"], true, "{who}: {r}");
        assert_eq!(r["group_loops_repaired"], 50_000, "{who}: {r}");
        assert_eq!(r["group_loop_names"].as_array().unwrap().len(), GroupRepair::NAMED);
        // (193 seconds before; about a tenth of a second now, so this is only a very loose ceiling for slow machines)
        assert!(t.elapsed() < Duration::from_secs(2), "{who}: {:?}", t.elapsed());
        assert!(!e.model.has_group_loop());
        assert_eq!(e.model.groups.len(), 100_000);
        let _ = line;
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_group_cut_from_each_loop_is_its_lowest_id_whatever_the_order_of_the_walk() {
    let mut m = Model::default();
    let ids: Vec<u32> = (0..6).map(|_| nid(&mut m)).collect();
    for (i, id) in ids.iter().enumerate() {
        m.groups.insert(*id, Group { name: format!("G{i}"), parent: Some(ids[(i + 1) % 6]), ..Default::default() });
    }
    // a tail leading into the loop
    let tail = nid(&mut m);
    m.groups.insert(tail, Group { name: "Tail".into(), parent: Some(ids[3]), ..Default::default() });
    let rep = m.repair_group_nesting();
    assert_eq!(rep.loops, 1);
    assert_eq!(m.groups[&ids[0]].parent, None);
    assert_eq!(m.groups[&tail].parent, Some(ids[3]));
    assert!(!m.has_group_loop());
}

// ---- quadratic scans ----

#[test]
fn making_a_component_of_many_faces_among_many_others_is_fast() {
    let mut e = Engine::new();
    let others = grid(&mut e.model, 30_000, 0.0);
    let mine = grid(&mut e.model, 10_000, 1_000.0);
    assert_eq!(others.len(), 30_000);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "make_component", "ids": mine}));
    let took = t.elapsed();
    assert!(r["instance"].is_number());
    // (28 seconds before)
    assert!(took.as_secs_f64() < 5.0, "{took:?}");
    assert_eq!(e.model.faces.len(), 30_000, "the others are as they were");
    assert_eq!(e.model.edges.len(), 120_000);
}

#[test]
fn exploding_a_big_component_is_fast_and_exact() {
    let mut e = Engine::new();
    let ids = grid(&mut e.model, 20_000, 0.0);
    let inst = run(&mut e, json!({"cmd": "make_component", "ids": ids}))["instance"].as_u64().unwrap();
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "explode", "id": inst}));
    let took = t.elapsed();
    assert!(r["group"].is_number());
    // (11 seconds before)
    assert!(took.as_secs_f64() < 5.0, "{took:?}");
    assert_eq!(e.model.faces.len(), 20_000);
    assert_eq!(e.model.edges.len(), 80_000);
}

#[test]
fn deleting_many_edges_is_fast_and_takes_their_faces_with_them() {
    let mut e = Engine::new();
    grid(&mut e.model, 20_000, 0.0);
    let edges: Vec<u32> = e.model.edges.keys().copied().collect();
    assert_eq!(edges.len(), 80_000);
    let t = Instant::now();
    run(&mut e, json!({"cmd": "delete", "ids": edges}));
    let took = t.elapsed();
    // (8.8 seconds before)
    assert!(took.as_secs_f64() < 3.0, "{took:?}");
    assert!(e.model.faces.is_empty() && e.model.edges.is_empty());
    // one edge takes only the faces on it
    let mut e = Engine::new();
    let f = grid(&mut e.model, 5, 0.0);
    let one = e.model.edges.iter().find(|(_, d)| e.model.faces[&f[2]].outer.contains(&d.a)).map(|(k, _)| *k).unwrap();
    run(&mut e, json!({"cmd": "delete", "ids": [one]}));
    assert_eq!(e.model.faces.len(), 4);
}

#[test]
fn deleting_several_groups_at_once_works_and_a_group_with_a_group_inside_may_both_be_named() {
    let mut e = Engine::new();
    let a = nid(&mut e.model);
    e.model.groups.insert(a, Group { name: "A".into(), ..Default::default() });
    let b = nid(&mut e.model);
    e.model.groups.insert(b, Group { name: "B".into(), parent: Some(a), ..Default::default() });
    let c = nid(&mut e.model);
    e.model.groups.insert(c, Group { name: "C".into(), ..Default::default() });
    quad(&mut e.model, Some(a), 0.0, 0.0);
    quad(&mut e.model, Some(b), 3.0, 0.0);
    quad(&mut e.model, Some(c), 6.0, 0.0);
    quad(&mut e.model, None, 9.0, 0.0);
    run(&mut e, json!({"cmd": "delete", "ids": [a, b, c]}));
    assert!(e.model.groups.is_empty());
    assert_eq!(e.model.faces.len(), 1);
}

#[test]
fn make_component_explode_and_delete_stop_at_an_agents_time_limit_and_are_put_back() {
    let mut e = Engine::new();
    let ids = grid(&mut e.model, 3_000, 0.0);
    let before = (e.model.faces.len(), e.model.edges.len(), e.model.vertices.len());
    // an agent's call gets a time limit; here the clock is already out
    e.model.deadline = Some(Instant::now());
    let edges: Vec<u32> = e.model.edges.keys().copied().collect();
    for c in [json!({"cmd": "make_component", "ids": ids}), json!({"cmd": "delete", "ids": edges})] {
        let r = as_agent(&mut e, c.clone());
        assert_eq!(r["ok"], false, "{c}: {r}");
        assert!(r["error"].as_str().unwrap().contains("too long"), "{r}");
        assert_eq!((e.model.faces.len(), e.model.edges.len(), e.model.vertices.len()), before, "put back");
    }
    e.model.deadline = None;
    let inst = run(&mut e, json!({"cmd": "make_component", "ids": ids}))["instance"].as_u64().unwrap() as u32;
    e.model.deadline = Some(Instant::now());
    let r = as_agent(&mut e, json!({"cmd": "explode", "id": inst}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("too long"), "{r}");
    assert!(e.model.instances.contains_key(&inst), "put back");
    e.model.deadline = None;
    run(&mut e, json!({"cmd": "explode", "id": inst}));
    assert_eq!(e.model.faces.len(), 3_000);
}

// ---- copying a group together with the group inside it ----

/// A (one square) with B inside it (one square); ids (A, B).
fn nested_pair(e: &mut Engine) -> (u32, u32) {
    let a = nid(&mut e.model);
    e.model.groups.insert(a, Group { name: "A".into(), ..Default::default() });
    let b = nid(&mut e.model);
    e.model.groups.insert(b, Group { name: "B".into(), parent: Some(a), ..Default::default() });
    quad(&mut e.model, Some(a), 0.0, 0.0);
    quad(&mut e.model, Some(b), 3.0, 0.0);
    (a, b)
}

fn counts(m: &Model) -> (usize, usize) {
    (m.groups.len(), m.faces.len())
}

#[test]
fn naming_a_group_and_the_group_inside_it_copies_the_outer_one_once_with_everything() {
    for order in [0, 1] {
        let mut e = Engine::new();
        let (a, b) = nested_pair(&mut e);
        let ids = if order == 0 { json!([b, a]) } else { json!([a, b]) };
        let r = run(&mut e, json!({"cmd": "duplicate", "ids": ids, "by": [0, 10, 0]}));
        assert_eq!(r["ids"].as_array().unwrap().len(), 1, "{r}");
        // A, B and a copy of A that holds a copy of B: four groups, four squares, no empty group
        assert_eq!(counts(&e.model), (4, 4), "order {order}");
        for (id, g) in &e.model.groups {
            assert!(e.model.faces.values().any(|f| f.group == Some(*id)), "group {id} {} is empty", g.name);
        }
        // the copy of B sits in the copy of A
        let copy_a = r["ids"][0].as_u64().unwrap() as u32;
        assert_eq!(e.model.groups.values().filter(|g| g.parent == Some(copy_a)).count(), 1);
    }
}

#[test]
fn arrays_and_pastes_of_a_group_and_its_inner_group_make_whole_copies_too() {
    let mut e = Engine::new();
    let (a, b) = nested_pair(&mut e);
    let r = run(&mut e, json!({"cmd": "array", "ids": [b, a], "by": [0, 10, 0], "count": 3}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(counts(&e.model), (2 + 6, 2 + 6));
    for (id, g) in &e.model.groups {
        assert!(e.model.faces.values().any(|f| f.group == Some(*id)), "group {id} {} is empty", g.name);
    }
    // the copy command takes one group: unchanged
    let mut e = Engine::new();
    let (a, _) = nested_pair(&mut e);
    run(&mut e, json!({"cmd": "copy", "group": a, "by": [0, 10, 0]}));
    assert_eq!(counts(&e.model), (4, 4));
    // two groups side by side, neither inside the other: both copied, as before
    let mut e = Engine::new();
    let (a, _) = nested_pair(&mut e);
    let c = nid(&mut e.model);
    e.model.groups.insert(c, Group { name: "C".into(), ..Default::default() });
    quad(&mut e.model, Some(c), 20.0, 0.0);
    let r = run(&mut e, json!({"cmd": "duplicate", "ids": [a, c], "by": [0, 10, 0]}));
    assert_eq!(r["ids"].as_array().unwrap().len(), 2);
    assert_eq!(counts(&e.model), (3 + 3, 3 + 3));
}

// ---- one big boolean ----

/// A prism of `sides` sides in its own group, around `at`.
fn prism(e: &mut Engine, name: &str, at: [f64; 3], sides: usize) -> u32 {
    let g = run(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap() as u32;
    let f = run(e, json!({"cmd": "circle", "center": at, "radius": 1.0, "segments": sides}))["face"].as_u64().unwrap();
    run(e, json!({"cmd": "pushpull", "face": f, "distance": 1.0, "toward": [0, 0, 1]}));
    run(e, json!({"cmd": "end_group"}));
    g
}

/// A prism lying along x, through the upright one at the origin.
fn crossing_prism(e: &mut Engine, name: &str, sides: usize) -> u32 {
    let g = run(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap() as u32;
    let f = run(e, json!({"cmd": "circle", "center": [-1.0, 0.0, 0.5], "radius": 1.0, "segments": sides, "normal": [1, 0, 0]}))["face"].as_u64().unwrap();
    run(e, json!({"cmd": "pushpull", "face": f, "distance": 2.0, "toward": [1, 0, 0]}));
    run(e, json!({"cmd": "end_group"}));
    g
}

#[test]
fn one_big_boolean_for_an_agent_is_stopped_and_put_back() {
    let mut e = Engine::new();
    let a = prism(&mut e, "A", [0.0, 0.0, 0.0], 1_999);
    let b = crossing_prism(&mut e, "B", 1_999);
    let before = counts(&e.model);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "solid", "op": "union", "ids": [a, b]}));
    let took = t.elapsed();
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("too complex") || r["error"].as_str().unwrap().contains("too long"), "{r}");
    assert_eq!(counts(&e.model), before, "put back");
    // (about 1.5 seconds and 1.4 GB before; it is now refused after a fraction of that)
    assert!(took.as_secs_f64() < 1.5, "{took:?}");
    // the engine is fine afterwards, and the same call from the person is not limited
    let r = as_agent(&mut e, json!({"cmd": "info"}));
    assert_eq!(r["ok"], true, "{r}");
    let r = e.exec_json(&json!({"cmd": "solid", "op": "union", "ids": [a, b]}).to_string());
    assert_eq!(r["ok"], true, "the person's call is not held to the agent's budget: {r}");
}

#[test]
fn a_boolean_stops_inside_one_step_when_the_time_is_up() {
    let mut e = Engine::new();
    let a = prism(&mut e, "A", [0.0, 0.0, 0.0], 600);
    let b = crossing_prism(&mut e, "B", 600);
    let before = counts(&e.model);
    // a limit that passes while the one boolean is still working (it takes far longer than this)
    e.model.deadline = Some(Instant::now() + Duration::from_millis(5));
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "solid", "op": "union", "ids": [a, b]}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("too long"), "{r}");
    assert!(t.elapsed().as_secs_f64() < 3.0, "{:?}", t.elapsed());
    assert_eq!(counts(&e.model), before, "put back");
    e.model.deadline = None;
    run(&mut e, json!({"cmd": "solid", "op": "union", "ids": [a, b]}));
}

#[test]
fn ordinary_solids_still_combine_for_agents_and_for_the_person() {
    let mut e = Engine::new();
    let a = prism(&mut e, "A", [0.0, 0.0, 0.0], 64);
    let b = prism(&mut e, "B", [0.5, 0.0, 0.0], 64);
    let c = prism(&mut e, "C", [5.0, 0.0, 0.0], 64);
    let d = prism(&mut e, "D", [5.5, 0.0, 0.0], 64);
    let r = as_agent(&mut e, json!({"cmd": "solid", "op": "union", "ids": [a, b]}));
    assert_eq!(r["ok"], true, "{r}");
    run(&mut e, json!({"cmd": "solid", "op": "union", "ids": [c, d]}));
}

// ---- the repair notice ----

#[test]
fn the_notice_says_itself_names_the_groups_and_caps_the_list() {
    let one = GroupRepair { loops: 1, loop_names: vec!["Roof".into()], ..Default::default() };
    let n = one.notice("file").unwrap();
    assert!(n.contains("1 group (Roof) that sat inside itself"), "{n}");
    assert!(!n.contains("itselves"), "{n}");
    let many = GroupRepair { loops: 12, loop_names: (0..5).map(|i| format!("G{i}")).collect(), ..Default::default() };
    let n = many.notice("backup").unwrap();
    assert!(n.starts_with("This backup had 12 groups (G0, G1, G2, G3, G4 and 7 more) that sat inside each other in loops"), "{n}");
    let both = GroupRepair { loops: 1, too_deep: 3, loop_names: vec!["A".into()], deep_names: vec!["B".into(), "C".into(), "D".into()] };
    let n = both.notice("file").unwrap();
    assert!(n.contains("and 3 groups (B, C, D)") && n.contains("levels deep"), "{n}");
    assert!(GroupRepair::default().notice("file").is_none());
    // a name with a line break or a very long one is tidied
    let m = {
        let mut m = Model::default();
        let a = nid(&mut m);
        let b = nid(&mut m);
        m.groups.insert(a, Group { name: format!("x\ny{}", "z".repeat(100)), parent: Some(b), ..Default::default() });
        m.groups.insert(b, Group { name: "b".into(), parent: Some(a), ..Default::default() });
        m
    };
    let mut m = m;
    let rep = m.repair_group_nesting();
    assert!(rep.loop_names[0].chars().count() <= 40 && !rep.loop_names[0].contains('\n'), "{:?}", rep.loop_names);
    // the reply carries the same facts the notice is built from
    let mut v = json!({});
    both.add_to(&mut v);
    assert_eq!(GroupRepair::from_reply(&v), both);
}

// ---- the limit is far above real models ----

/// The deepest nesting of groups (and of groups inside components) in the model.
fn deepest(m: &Model) -> u32 {
    m.group_nesting().unwrap_or(0).max(m.definitions.values().map(|d| deepest(&d.model)).max().unwrap_or(0))
}

#[test]
fn real_models_nest_a_few_levels_not_hundreds() {
    // the SketchUp files of the test corpus, and every SketchCraft model kept in the repository
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut most = 0;
    let mut seen = 0;
    for entry in std::fs::read_dir(root.join("tests/skp_corpus")).unwrap().flatten() {
        if entry.path().extension().is_some_and(|x| x == "skp") {
            let mut m = Model::default();
            sketchcraft::skp::import(&mut m, &std::fs::read(entry.path()).unwrap(), DVec3::ZERO, "corpus").unwrap();
            most = most.max(deepest(&m));
            seen += 1;
        }
    }
    for dir in [root.join("../examples"), root.join("../docs")] {
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "sketchcraft") {
                    if let Ok(m) = serde_json::from_slice::<Model>(&std::fs::read(&p).unwrap()) {
                        most = most.max(deepest(&m));
                        seen += 1;
                    }
                }
            }
        }
    }
    assert!(seen >= 6, "{seen} files");
    eprintln!("deepest nesting in {seen} real files: {most}");
    assert!(most as usize * 10 < MAX_GROUP_DEPTH, "real models nest {most} levels; the limit is {MAX_GROUP_DEPTH}");
}
