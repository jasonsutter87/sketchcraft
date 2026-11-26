//! #231: a guide made while a component is open goes into the component, in its own
//! coordinates (the app maps the point there), and must still show up in the scene,
//! placed where the component is.

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn guides(e: &Engine) -> Vec<Value> {
    sketchcraft::scene::build(&e.model, e.ctx)["guides"].as_array().unwrap().clone()
}

#[test]
fn guides_made_inside_a_component_show_while_it_is_open() {
    let mut e = Engine::new();
    let face = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["face"].clone();
    let made = ok(&mut e, json!({"cmd": "make_component", "ids": [face], "name": "Part"}));
    let inst = made["instance"].clone();
    ok(&mut e, json!({"cmd": "move", "ids": [inst], "by": [10, 0, 0]}));
    ok(&mut e, json!({"cmd": "guide", "point": [0, 0, 5]}));
    assert_eq!(guides(&e).len(), 1);

    ok(&mut e, json!({"cmd": "edit", "id": inst}));
    // what the app sends after mapping into the component: a line along its x
    // through local (0, 1, 0), and a point at local (1, 1, 0)
    let local_line = ok(&mut e, json!({"cmd": "guide", "point": [0, 1, 0], "direction": [1, 0, 0]}))["guide"].clone();
    ok(&mut e, json!({"cmd": "guide", "point": [1, 1, 0]}));
    let g = guides(&e);
    assert_eq!(g.len(), 3, "the model's guide and both of the component's: {g:?}");
    let line = g.iter().find(|x| x["id"] == local_line).expect("the guide line made inside");
    let p: Vec<f64> = serde_json::from_value(line["p"].clone()).unwrap();
    let t = e.model.instances[&inst.as_u64().unwrap().try_into().unwrap()].transform;
    let want = t.transform_point3(glam::DVec3::new(0.0, 1.0, 0.0));
    assert!((p[0] - want.x).abs() < 1e-9 && (p[1] - want.y).abs() < 1e-9 && (p[2] - want.z).abs() < 1e-9, "{p:?} vs {want:?}");
    assert!(line["dir"].is_array());

    ok(&mut e, json!({"cmd": "close"}));
    assert_eq!(guides(&e).len(), 1, "closed: only the model's own guides");
}
