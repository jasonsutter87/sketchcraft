//! Work limits, round five: bevel / round / slice / joinery cuts under the agent clock and piece budget, one pass
//! over the faces for the scene request, and a few details of names in replies. Bounds are generous (CI machines are
//! slow); each is far below what the work used to take.

use serde_json::{json, Value};
use glam::DVec3;
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

/// A regular `n`-gon prism (radius 5, height 4) as one group; returns the group id.
fn prism(e: &mut Engine, n: usize) -> u32 {
    let pts: Vec<Value> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            json!([5.0 * a.cos(), 5.0 * a.sin(), 0.0])
        })
        .collect();
    let f = run(e, json!({"cmd": "polygon", "points": pts}))["face"].as_u64().unwrap();
    run(e, json!({"cmd": "pushpull", "face": f, "distance": 4.0, "toward": [0, 0, 1]}));
    let ids: Vec<u32> = e.model.faces.keys().copied().collect();
    run(e, json!({"cmd": "group", "ids": ids}))["group"].as_u64().unwrap() as u32
}

fn edges_of(e: &Engine, g: u32) -> Vec<u32> {
    e.model.edges.iter().filter(|(_, x)| x.group == Some(g)).map(|(k, _)| *k).collect()
}

fn volume(e: &mut Engine, g: u32) -> f64 {
    run(e, json!({"cmd": "volume", "id": g}))["volume"].as_f64().unwrap()
}

#[test]
fn bevel_every_edge_of_a_big_prism_is_quick_and_the_same_solid() {
    let mut e = Engine::new();
    let g = prism(&mut e, 100);
    let before = volume(&mut e, g);
    let edges = edges_of(&e, g);
    assert_eq!(edges.len(), 300);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "bevel", "edges": edges, "size": 0.05}));
    eprintln!("bevel of 300 edges: {:?}", t.elapsed());
    assert!(t.elapsed() < Duration::from_secs(15), "bevel took {:?}", t.elapsed());
    let after = volume(&mut e, r["group"].as_u64().unwrap() as u32);
    assert!(after < before && after > before * 0.97, "{before} -> {after}");
    // undo puts the original back in one step
    run(&mut e, json!({"cmd": "undo"}));
    assert_eq!(edges_of(&e, g).len(), 300);
}

#[test]
fn an_agent_bevel_of_a_huge_prism_is_stopped_and_rolled_back() {
    let mut e = Engine::new();
    let g = prism(&mut e, 600);
    let edges = edges_of(&e, g);
    let faces = e.model.faces.len();
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "bevel", "edges": edges, "size": 0.01, "segments": 8}));
    assert!(t.elapsed() < Duration::from_secs(40), "took {:?}", t.elapsed());
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(e.model.faces.len(), faces, "nothing changed");
    assert!(e.model.groups.contains_key(&g));
}

#[test]
fn an_agent_slice_runs_under_the_budget() {
    let mut e = Engine::new();
    let g = prism(&mut e, 40);
    let r = as_agent(&mut e, json!({"cmd": "slice", "id": g, "origin": [0, 0, 2], "normal": [0, 0, 1]}));
    assert_eq!(r["ok"], true, "{r}");
    let mut e = Engine::new();
    let g = prism(&mut e, 1999);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "slice", "id": g, "origin": [0, 0, 2], "normal": [0, 0, 1]}));
    assert!(t.elapsed() < Duration::from_secs(40), "took {:?}", t.elapsed());
    // one boolean bounded by the operand: it works or is refused, never hangs
    assert!(r["ok"] == true || r["error"].is_string(), "{r}");
}

fn nid(m: &mut Model) -> u32 {
    m.next_id += 1;
    m.next_id
}

/// A unit square face in group `g` at (x, y).
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

#[test]
fn the_scene_of_64k_groups_is_built_in_one_pass() {
    let mut e = Engine::new();
    for i in 0..64_000usize {
        let g = nid(&mut e.model);
        e.model.groups.insert(g, Group { name: format!("G{i}"), ..Default::default() });
        quad(&mut e.model, Some(g), (i % 250) as f64 * 2.0, (i / 250) as f64 * 2.0);
    }
    let t = Instant::now();
    let scene = sketchcraft::scene::build(&e.model, None);
    eprintln!("scene of 64,000 groups: {:?}", t.elapsed());
    assert!(t.elapsed() < Duration::from_secs(8), "scene took {:?}", t.elapsed());
    let groups = scene["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 64_000);
    assert!(groups.iter().all(|g| g["faces"] == 1));
}

#[test]
fn duplicating_a_face_with_its_group_copies_it_once() {
    let mut e = Engine::new();
    let g = nid(&mut e.model);
    e.model.groups.insert(g, Group { name: "A".into(), ..Default::default() });
    let f = quad(&mut e.model, Some(g), 0.0, 0.0);
    run(&mut e, json!({"cmd": "duplicate", "ids": [f, g], "by": [5, 0, 0]}));
    assert_eq!(e.model.faces.len(), 2, "one face in the copy, none extra");
    assert!(e.model.faces.values().all(|f| f.group.is_some()), "no loose copy");
}

#[test]
fn hidden_characters_in_group_names_stay_out_of_the_notice() {
    let mut m = Model::default();
    let (a, b) = (nid(&mut m), nid(&mut m));
    m.groups.insert(a, Group { name: "Bad\u{202E}name\u{200B}\u{E0041}\u{3164}x".into(), parent: Some(b), ..Default::default() });
    m.groups.insert(b, Group { name: "Other".into(), parent: Some(a), ..Default::default() });
    let r = m.repair_group_nesting();
    assert_eq!(r.loops, 1);
    let text = r.notice("file").unwrap();
    let mut reply = json!({});
    r.add_to(&mut reply);
    for t in [text, reply.to_string()] {
        assert!(!t.chars().any(|c| matches!(c, '\u{202E}' | '\u{200B}' | '\u{E0041}' | '\u{3164}')), "{t}");
        assert!(t.contains("Badnamex") || t.contains("Other"), "{t}");
    }
}
