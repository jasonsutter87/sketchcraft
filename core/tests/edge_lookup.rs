//! `soften` and `edge_style` given faces find each side's edge through one index per command.
use serde_json::{json, Value};
use sketchcraft::model::Id;
use sketchcraft::Engine;
use std::time::{Duration, Instant};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn ids(v: &Value) -> Vec<Id> {
    v.as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as Id).collect()
}

fn grid(e: &mut Engine, side: usize) -> Vec<Id> {
    let one = run(e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}))["face"].as_u64().unwrap() as Id;
    let mut row = vec![one];
    row.extend(ids(&run(e, json!({"cmd": "array", "ids": [one], "by": [3, 0, 0], "count": side - 1}))["ids"]));
    let mut all = row.clone();
    all.extend(ids(&run(e, json!({"cmd": "array", "ids": row, "by": [0, 3, 0], "count": side - 1}))["ids"]));
    all
}

#[test]
fn a_big_grid_is_softened_and_styled_by_face_quickly() {
    let mut e = Engine::new();
    let all = grid(&mut e, 200);
    assert_eq!(all.len(), 40_000);
    let every = all.clone();
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "soften", "ids": all, "soft": true}));
    assert_eq!(r["edges"], 160_000);
    let r = run(&mut e, json!({"cmd": "edge_style", "ids": every, "weight_mm": 0.5}));
    assert_eq!(r["edges"], 160_000);
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
}

/// The edge each face side lands on, found the slow way.
fn expected(e: &Engine, faces: &[Id]) -> Vec<Id> {
    let mut out = Vec::new();
    for f in faces {
        let face = e.model.faces[f].clone();
        for l in face.loops() {
            for (a, b) in sketchcraft::model::pairs(l) {
                out.extend(e.model.find_edge(a, b));
            }
        }
    }
    out
}

#[test]
fn face_sides_resolve_to_the_same_edges_as_a_full_scan() {
    let mut e = Engine::new();
    // a box (shared edges), a wall with a window (hole), a grouped box
    run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [8, 8, 0]}));
    run(&mut e, json!({"cmd": "rect", "from": [2, 2, 0], "to": [4, 4, 0]}));
    let floor: Vec<Id> = e.model.faces.keys().copied().collect();
    run(&mut e, json!({"cmd": "pushpull", "face": *floor.last().unwrap(), "distance": 3}));
    run(&mut e, json!({"cmd": "begin_group", "name": "G"}));
    run(&mut e, json!({"cmd": "rect", "from": [20, 0, 0], "to": [24, 4, 0]}));
    let top = *e.model.faces.keys().last().unwrap();
    run(&mut e, json!({"cmd": "pushpull", "face": top, "distance": 2}));
    run(&mut e, json!({"cmd": "end_group"}));
    let faces: Vec<Id> = e.model.faces.keys().copied().collect();
    assert!(e.model.faces.values().any(|f| !f.holes.is_empty()));
    let want = expected(&e, &faces);
    assert!(!want.is_empty());

    let r = run(&mut e, json!({"cmd": "soften", "ids": faces, "soft": true}));
    assert_eq!(r["edges"], want.len());
    let soft: Vec<Id> = e.model.edges.iter().filter(|(_, x)| x.soft).map(|(&k, _)| k).collect();
    let mut w = want.clone();
    w.sort();
    w.dedup();
    assert_eq!(soft, w);

    let r = run(&mut e, json!({"cmd": "edge_style", "ids": faces, "weight_mm": 0.5}));
    assert_eq!(r["edges"], want.len());
    let styled: Vec<Id> = e.model.edges.iter().filter(|(_, x)| x.draft.is_some()).map(|(&k, _)| k).collect();
    assert_eq!(styled, w);
}
