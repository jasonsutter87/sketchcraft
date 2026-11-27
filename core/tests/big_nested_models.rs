//! #296: a model whose components multiply past what can be drawn is refused with the components that make it
//! so and what to do, and the document stays as it was. A texture that can't be used says why.

use serde_json::{json, Value};
use sketchcraft::Engine;

fn exec(e: &mut Engine, v: Value) -> Value {
    e.exec_json(&v.to_string())
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = exec(e, v.clone());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn placements(def: u64, from: u64, n: u64) -> Value {
    let mut m = serde_json::Map::new();
    for i in 0..n {
        m.insert((from + i).to_string(), json!({"parent": null, "def": def, "name": "", "transform": IDENTITY, "tag": null, "hidden": false, "attrs": {}}));
    }
    Value::Object(m)
}

/// A saved model: "Leaf" (a rectangle) placed 2,000 times in "Row", and "Row" placed 1,500 times at the top:
/// 3 million leaves, a few hundred KB on disk.
fn multiplying_model(dir: &std::path::Path) -> std::path::PathBuf {
    let mut e = Engine::new();
    let face = ok(
        &mut e,
        json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}),
    )["face"]
        .clone();
    let leaf = ok(
        &mut e,
        json!({"cmd": "make_component", "ids": [face], "name": "Leaf"}),
    )["component"]
        .as_u64()
        .unwrap();
    let p = dir.join("big.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p.to_str().unwrap()}));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let row = 90_000u64;
    let mut row_model = v["definitions"][leaf.to_string()]["model"].clone();
    for k in ["points", "edges", "faces"] {
        if row_model.get(k).is_some() {
            row_model[k] = json!({});
        }
    }
    row_model["instances"] = placements(leaf, 100_000, 2_000);
    v["definitions"][row.to_string()] = json!({"name": "Row", "model": row_model});
    v["instances"] = placements(row, 200_000, 1_500);
    if let Some(n) = v.get("next_id").and_then(|n| n.as_u64()) {
        v["next_id"] = json!(n.max(300_000));
    }
    std::fs::write(&p, v.to_string()).unwrap();
    p
}

#[test]
fn a_model_that_multiplies_too_far_names_its_heaviest_components_and_what_to_do() {
    let dir = std::env::temp_dir().join(format!("sc-296-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = multiplying_model(&dir);

    let mut e = Engine::new();
    ok(
        &mut e,
        json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}),
    );
    let before = e.model.faces.len();
    let r = exec(&mut e, json!({"cmd": "open", "path": p.to_str().unwrap()}));
    assert_eq!(r["ok"], false, "{r}");
    let why = r["error"].as_str().unwrap();
    // the size in words, the components responsible with their copies, and a way forward
    assert!(why.contains("million edges and faces"), "{why}");
    assert!(why.contains("\"Leaf\" (3,000,000 copies"), "{why}");
    assert!(why.contains("\"Row\" (1,500 copies"), "{why}");
    assert!(
        why.find("\"Leaf\"").unwrap() < why.find("\"Row\"").unwrap(),
        "heaviest first: {why}"
    );
    assert!(why.contains("lighter in the program that made it"), "{why}");
    // the document the person had is still there and still works
    assert_eq!(e.model.faces.len(), before);
    ok(
        &mut e,
        json!({"cmd": "rect", "from": [3, 0, 0], "to": [4, 1, 0]}),
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn heaviest_components_counts_copies_through_every_level() {
    let dir = std::env::temp_dir().join(format!("sc-296b-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = multiplying_model(&dir);
    let m: sketchcraft::model::Model =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let h = m.heaviest_components(5);
    assert_eq!(h.len(), 2, "{h:?}");
    assert_eq!((h[0].name.as_str(), h[0].copies), ("Leaf", 3_000_000));
    assert_eq!(
        (h[1].name.as_str(), h[1].copies, h[1].total),
        ("Row", 1_500, 3_000_000)
    );
    assert_eq!(
        h[0].total,
        3_000_000
            * (m.definitions
                .values()
                .find(|d| d.name == "Leaf")
                .map(|d| d.model.edges.len() + d.model.faces.len())
                .unwrap())
    );
    assert!(m.heaviest_components(1).len() == 1);
    std::fs::remove_dir_all(&dir).ok();
}
