//! #266: groups and components keep their own drawing axes. Opening one draws on its axes, closing it goes back
//! to the model's; the axes move with the object; "align axes" puts them on a face (a pitched roof).

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused, got {r}");
    r["error"].as_str().unwrap().to_string()
}

fn shown(e: &Engine) -> Value {
    sketchcraft::scene::build(&e.model, e.ctx)["axes"].clone()
}

fn v3(v: &Value) -> [f64; 3] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

/// A group holding one roof plane 4 m wide, 3 m up the slope at 30°, rising along +y.
fn roof(e: &mut Engine) -> (u64, u64) {
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let g = ok(e, json!({"cmd": "begin_group", "name": "Roof"}))["group"].as_u64().unwrap();
    let face = ok(e, json!({"cmd": "polygon", "points": [[0, 0, 0], [4, 0, 0], [4, 3.0 * c, 3.0 * s], [0, 3.0 * c, 3.0 * s]]}))["face"].as_u64().unwrap();
    ok(e, json!({"cmd": "end_group"}));
    (g, face)
}

#[test]
fn a_group_on_a_30_degree_roof_draws_on_the_slope_and_the_model_axes_stay() {
    let mut e = Engine::new();
    let (g, face) = roof(&mut e);
    ok(&mut e, json!({"cmd": "edit", "id": g}));
    let r = ok(&mut e, json!({"cmd": "align_axes", "face": face}));
    assert_eq!(r["of"]["group"], g, "{r}");
    // red along the 4 m eave, blue out of the roof: green runs up the slope
    let ax = shown(&e);
    assert!(near(v3(&ax[1]), [1.0, 0.0, 0.0]), "{ax}");
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    assert!(near(v3(&ax[2]), [0.0, c, s]), "{ax}");
    // a rectangle drawn on those axes lies in the roof
    let (o, x, y) = (v3(&ax[0]), v3(&ax[1]), v3(&ax[2]));
    let at = |u: f64, w: f64| [o[0] + x[0] * u + y[0] * w, o[1] + x[1] * u + y[1] * w, o[2] + x[2] * u + y[2] * w];
    let f = ok(&mut e, json!({"cmd": "polygon", "points": [at(1.0, 1.0), at(2.0, 1.0), at(2.0, 2.0), at(1.0, 2.0)]}))["face"].as_u64().unwrap();
    assert_eq!(e.model.faces[&(f as u32)].group, Some(g as u32), "drawn inside the group");
    ok(&mut e, json!({"cmd": "close"}));
    // the model's axes are unchanged, and that is what is drawn on outside the group
    assert!(e.model.axes.is_none());
    assert!(shown(&e).is_null(), "{}", shown(&e));
    // back in: the group's axes are still there
    ok(&mut e, json!({"cmd": "edit", "id": g}));
    assert!(near(v3(&shown(&e)[2]), [0.0, c, s]));
}

#[test]
fn a_groups_axes_move_and_copy_with_it_and_reset() {
    let mut e = Engine::new();
    let (g, face) = roof(&mut e);
    ok(&mut e, json!({"cmd": "align_axes", "face": face, "id": g}));
    assert!(e.model.axes.is_none(), "naming the group leaves the model's axes alone");
    ok(&mut e, json!({"cmd": "move", "ids": [g], "by": [10, 0, 0]}));
    let ax = e.model.groups[&(g as u32)].axes.unwrap();
    assert!(near(ax[0], [10.0, 0.0, 0.0]), "{ax:?}");
    ok(&mut e, json!({"cmd": "rotate", "ids": [g], "center": [10, 0, 0], "axis": [0, 0, 1], "angle": 90}));
    let ax = e.model.groups[&(g as u32)].axes.unwrap();
    assert!(near(ax[1], [0.0, 1.0, 0.0]), "turned with it: {ax:?}");
    let copy = ok(&mut e, json!({"cmd": "copy", "group": g, "by": [0, 5, 0]}));
    let new = copy["ids"].as_array().and_then(|a| a.first()).or_else(|| copy.get("group")).and_then(|v| v.as_u64()).unwrap_or_else(|| panic!("{copy}"));
    let cax = e.model.groups[&(new as u32)].axes.unwrap();
    assert!(near(cax[0], [10.0, 5.0, 0.0]), "{cax:?}");
    ok(&mut e, json!({"cmd": "set_axes", "id": g, "reset": true}));
    assert!(e.model.groups[&(g as u32)].axes.is_none());
    assert!(e.model.groups[&(new as u32)].axes.is_some(), "the copy keeps its own");
}

#[test]
fn set_axes_with_a_group_open_changes_that_group_not_the_model() {
    let mut e = Engine::new();
    let (g, _) = roof(&mut e);
    ok(&mut e, json!({"cmd": "set_axes", "origin": [1, 1, 0], "x": [0, 1, 0], "y": [-1, 0, 0]}));
    ok(&mut e, json!({"cmd": "edit", "id": g}));
    // inside a group with none of its own, the model's axes are drawn on
    assert!(near(v3(&shown(&e)[0]), [1.0, 1.0, 0.0]));
    ok(&mut e, json!({"cmd": "set_axes", "origin": [2, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0]}));
    assert!(near(v3(&shown(&e)[0]), [2.0, 0.0, 0.0]));
    ok(&mut e, json!({"cmd": "set_axes", "reset": true}));
    assert!(e.model.groups[&(g as u32)].axes.is_none());
    ok(&mut e, json!({"cmd": "close"}));
    assert_eq!(e.model.axes.unwrap()[0], [1.0, 1.0, 0.0], "the model's axes were never touched");
}

#[test]
fn a_component_keeps_its_axes_in_its_own_coordinates_for_every_copy() {
    let mut e = Engine::new();
    let face = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 1, 0]}))["face"].clone();
    let made = ok(&mut e, json!({"cmd": "make_component", "ids": [face], "name": "Panel"}));
    let def = made["component"].as_u64().unwrap();
    let inst = made["instance"].as_u64().or_else(|| made["id"].as_u64()).unwrap_or_else(|| panic!("{made}"));
    ok(&mut e, json!({"cmd": "move", "ids": [inst], "by": [5, 0, 0]}));
    // world axes given for the placement are kept relative to the component
    ok(&mut e, json!({"cmd": "set_axes", "id": inst, "origin": [6, 0, 0], "x": [0, 1, 0], "y": [-1, 0, 0]}));
    let local = e.model.definitions[&(def as u32)].model.axes.unwrap();
    assert!(near(local[0], [1.0, 0.0, 0.0]), "{local:?}");
    ok(&mut e, json!({"cmd": "edit", "id": inst}));
    let ax = shown(&e);
    assert!(near(v3(&ax[0]), [6.0, 0.0, 0.0]) && near(v3(&ax[1]), [0.0, 1.0, 0.0]), "{ax}");
    ok(&mut e, json!({"cmd": "close"}));
    assert!(e.model.axes.is_none());
}

#[test]
fn align_axes_refuses_what_it_cannot_use() {
    let mut e = Engine::new();
    let why = refused(&mut e, json!({"cmd": "align_axes", "face": 999}));
    assert!(why.contains("not a face"), "{why}");
    let why = refused(&mut e, json!({"cmd": "set_axes", "id": 999, "origin": [0, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0]}));
    assert!(why.contains("not a group or component"), "{why}");
}

#[test]
fn a_saved_group_keeps_its_axes() {
    let dir = std::env::temp_dir().join(format!("sc-266-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut e = Engine::new();
    let (g, face) = roof(&mut e);
    ok(&mut e, json!({"cmd": "align_axes", "face": face, "id": g}));
    let p = dir.join("roof.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p.to_str().unwrap()}));
    let mut f = Engine::new();
    ok(&mut f, json!({"cmd": "open", "path": p.to_str().unwrap()}));
    assert_eq!(f.model.groups[&(g as u32)].axes, e.model.groups[&(g as u32)].axes);
    std::fs::remove_dir_all(&dir).ok();
}
