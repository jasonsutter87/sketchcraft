//! `faces` with `outlines: true` gives each face's corners (the SketchUp Ruby layer reads the model through it).
use serde_json::{json, Value};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    e.exec_json(&v.to_string())
}

#[test]
fn faces_can_carry_their_outlines() {
    let home = std::env::temp_dir().join(format!("sc-faces-outlines-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("SKETCHCRAFT_DOC", home.join("live.sketchcraft"));
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "new"}));
    let r = run(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 1, 0]}));
    assert_eq!(r["ok"], true, "{r}");

    // without the flag: counts only, as before
    let plain = run(&mut e, json!({"cmd": "faces"}));
    assert!(plain["faces"][0].get("outer").is_none());

    let r = run(&mut e, json!({"cmd": "faces", "outlines": true}));
    let f = &r["faces"][0];
    let outer = f["outer"].as_array().expect("outer corners");
    assert_eq!(outer.len(), 4);
    assert_eq!(f["points"], 4);
    let mut xs: Vec<f64> = outer.iter().map(|p| p[0].as_f64().unwrap()).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(xs, vec![0.0, 0.0, 2.0, 2.0]);
    assert_eq!(f["inner"].as_array().unwrap().len(), 0);
}
