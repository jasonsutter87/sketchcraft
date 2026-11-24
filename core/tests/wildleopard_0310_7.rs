//! Work limits, round seven: the intersect plane cache stays right after a loop is added, so Intersect of many
//! overlapping solids works for the person and for an agent (a stale entry used to stop it with "SketchCraft hit a bug").

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Edge, Face, Id, Model, Vertex};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::time::{Duration, Instant};

fn as_agent(e: &mut Engine, v: Value) -> Value {
    e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &v.to_string())
}

fn nid(m: &mut Model) -> Id {
    m.next_id += 1;
    m.next_id
}

/// One box (8 points, 12 edges, 6 faces) of size `size` at `o`; returns its face ids.
fn add_box(m: &mut Model, o: DVec3, size: f64) -> Vec<Id> {
    let vs: Vec<Id> = (0..8)
        .map(|k| {
            let id = nid(m);
            m.vertices.insert(id, Vertex { p: o + DVec3::new((k & 1) as f64, ((k >> 1) & 1) as f64, ((k >> 2) & 1) as f64) * size, group: None });
            id
        })
        .collect();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        let k = nid(m);
        m.edges.insert(k, Edge { a: vs[a], b: vs[b], group: None, ..Default::default() });
    }
    let mut faces = Vec::new();
    for q in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
        let id = nid(m);
        m.faces.insert(id, Face { outer: q.iter().map(|i| vs[*i]).collect(), group: None, ..Default::default() });
        faces.push(id);
    }
    faces
}

/// A lattice of overlapping loose boxes: `n` per side, `spacing` apart, each of size 1.
fn lattice(e: &mut Engine, n: usize, spacing: f64) -> Vec<Id> {
    let mut faces = Vec::new();
    for x in 0..n {
        for y in 0..n {
            for z in 0..n {
                faces.extend(add_box(&mut e.model, DVec3::new(x as f64, y as f64, z as f64) * spacing, 1.0));
            }
        }
    }
    faces
}

#[test]
fn intersect_of_overlapping_boxes_works_for_the_person_and_for_an_agent() {
    // 27 boxes in a 3 x 3 x 3 lattice that overlap: lines are drawn into faces that are split as they go
    let mut person = Engine::new();
    let ids = lattice(&mut person, 3, 0.6);
    let r = person.exec_json(&json!({"cmd": "intersect", "ids": ids, "with": "selection"}).to_string());
    assert_eq!(r["ok"], true, "the person's intersect of 27 overlapping boxes: {r}");
    assert!(r["lines"].as_u64().unwrap() > 0, "{r}");
    // an agent gets the same answer
    let mut a = Engine::new();
    let ids = lattice(&mut a, 3, 0.6);
    let ra = as_agent(&mut a, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    assert_eq!(ra["ok"], true, "{ra}");
    assert_eq!(ra["lines"], r["lines"]);
    assert_eq!((a.model.faces.len(), a.model.edges.len(), a.model.vertices.len()), (person.model.faces.len(), person.model.edges.len(), person.model.vertices.len()));
}

#[test]
fn intersect_of_many_overlapping_boxes_does_not_hit_the_bug_net() {
    let mut e = Engine::new();
    let ids = lattice(&mut e, 4, 0.7);
    let t = Instant::now();
    let r = as_agent(&mut e, json!({"cmd": "intersect", "ids": ids, "with": "selection"}));
    let text = r.to_string();
    assert!(!text.contains("hit a bug"), "{text}");
    assert!(r["ok"] == true || r["timed_out"] == true, "{text}");
    assert!(t.elapsed() < Duration::from_secs(40), "{:?}", t.elapsed());
}
