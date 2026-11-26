//! Walls, stairs, roofs and columns remember their settings and can be changed after placing (#263, from Blender 3D
//! Architect's first look at 0.3.8: "we don't have any option to change the settings for the walls, for the stairs,
//! roof").

use serde_json::{json, Value};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should fail -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn id(v: &Value) -> u64 {
    v["group"].as_u64().unwrap()
}

/// The highest and lowest z of the group's own corners.
fn z_range(e: &Engine, g: u64) -> (f64, f64) {
    let zs: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(g as u32)).map(|v| v.p.z).collect();
    (zs.iter().cloned().fold(f64::MAX, f64::min), zs.iter().cloned().fold(f64::MIN, f64::max))
}

fn x_range(e: &Engine, g: u64) -> (f64, f64) {
    let xs: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(g as u32)).map(|v| v.p.x).collect();
    (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max))
}

fn wall(e: &mut Engine) -> u64 {
    id(&ok(e, json!({"cmd": "wall", "points": [[0, 0, 0], [4, 0, 0]], "thickness": 0.2, "height": 2.7})))
}

#[test]
fn a_wall_says_what_it_is_and_its_settings() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let r = ok(&mut e, json!({"cmd": "part_info", "group": g}));
    assert_eq!(r["kind"], "wall");
    assert_eq!(r["settings"], json!({"thickness": 0.2, "height": 2.7}));
    assert_eq!(r["edited"], false);
}

#[test]
fn a_wall_made_taller_rebuilds_in_place_in_the_same_group() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    ok(&mut e, json!({"cmd": "rename", "id": g, "name": "North wall"}));
    let r = ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.2}}));
    assert_eq!(r["settings"]["height"], 3.2);
    let (z0, z1) = z_range(&e, g);
    assert!(z0.abs() < 1e-9 && (z1 - 3.2).abs() < 1e-9, "{z0}..{z1}");
    assert_eq!(e.model.groups[&(g as u32)].name, "North wall", "same group, same name");
    assert_eq!(e.model.groups.len(), 1, "no stray group left behind");
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["edited"], false);
}

#[test]
fn a_moved_wall_rebuilds_where_it_now_stands() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    ok(&mut e, json!({"cmd": "move", "ids": [g], "by": [10, 0, 0]}));
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["edited"], false, "moving it whole isn't an edit");
    ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"thickness": 0.3}}));
    let (x0, x1) = x_range(&e, g);
    assert!((x0 - 10.0).abs() < 1e-6 && (x1 - 14.0).abs() < 1e-6, "{x0}..{x1}");
}

#[test]
fn a_copied_wall_keeps_its_settings_at_the_copy() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let r = ok(&mut e, json!({"cmd": "duplicate", "ids": [g], "by": [0, 5, 0]}));
    let copy = e.model.groups.keys().copied().find(|&k| k as u64 != g).unwrap_or_else(|| panic!("no copy: {r}")) as u64;
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": copy}))["edited"], false);
    ok(&mut e, json!({"cmd": "rebuild", "group": copy, "set": {"height": 1.0}}));
    let ys: Vec<f64> = e.model.vertices.values().filter(|v| v.group == Some(copy as u32)).map(|v| v.p.y).collect();
    assert!(ys.iter().all(|y| *y > 4.0), "the copy stays where it was put");
    assert!((z_range(&e, g).1 - 2.7).abs() < 1e-9, "the original is untouched");
}

#[test]
fn a_wall_edited_by_hand_is_not_rebuilt_unless_forced() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let top = *e.model.faces.iter().find(|(_, f)| f.group == Some(g as u32) && e.model.pts(&f.outer).iter().all(|p| (p.z - 2.7).abs() < 1e-9)).unwrap().0;
    ok(&mut e, json!({"cmd": "pushpull", "face": top, "distance": 0.5}));
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["edited"], true);
    let err = fails(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.0}}));
    assert!(err.contains("edited by hand"), "{err}");
    ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.0}, "force": true}));
    assert!((z_range(&e, g).1 - 3.0).abs() < 1e-9);
}

#[test]
fn scaling_a_wall_forgets_its_settings() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    ok(&mut e, json!({"cmd": "scale", "ids": [g], "center": [0, 0, 0], "factor": [2, 1, 1]}));
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["kind"], Value::Null);
}

#[test]
fn bad_settings_change_nothing() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let before = e.model.clone();
    fails(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": -1}}));
    fails(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"points": [[0, 0, 0], [1, 0, 0]]}}));
    fails(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": "tall"}}));
    assert_eq!(e.model.faces, before.faces);
}

#[test]
fn stairs_roofs_and_columns_remember_theirs() {
    let mut e = Engine::new();
    let s = id(&ok(&mut e, json!({"cmd": "stairs", "at": [0, 0, 0], "rise": 2.7, "width": 1.0})));
    let r = ok(&mut e, json!({"cmd": "rebuild", "group": s, "set": {"rise": 1.0}}));
    assert_eq!(r["kind"], "stairs");
    assert!(r["risers"].as_u64().unwrap() < 10, "fewer risers for a lower stair: {r}");
    assert!((z_range(&e, s).1 - 1.0).abs() < 1e-9);

    let c = id(&ok(&mut e, json!({"cmd": "column", "at": [5, 5, 0], "size": 0.3, "height": 2.7})));
    ok(&mut e, json!({"cmd": "rebuild", "group": c, "set": {"height": 4, "round": true}}));
    assert!((z_range(&e, c).1 - 4.0).abs() < 1e-9);

    let f = ok(&mut e, json!({"cmd": "rect", "from": [10, 0, 3], "to": [16, 4, 3]}))["face"].clone();
    let roof = id(&ok(&mut e, json!({"cmd": "roof", "face": f, "kind": "gable", "pitch": 30, "overhang": 0.3})));
    let low = z_range(&e, roof).1;
    ok(&mut e, json!({"cmd": "rebuild", "group": roof, "set": {"pitch": 45}}));
    assert!(z_range(&e, roof).1 > low + 0.5, "steeper roof is taller");
    ok(&mut e, json!({"cmd": "rebuild", "group": roof, "set": {"style": "hip"}}));
    assert_eq!(e.model.groups[&(roof as u32)].name, "Gable roof", "the name it was given stays");
    fails(&mut e, json!({"cmd": "rebuild", "group": roof, "set": {"style": "dome"}}));
}

#[test]
fn a_plain_group_has_no_settings() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let faces: Vec<u32> = e.model.faces.keys().copied().collect();
    let g = ok(&mut e, json!({"cmd": "group", "ids": faces}))["group"].as_u64().unwrap();
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["kind"], Value::Null);
    let err = fails(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 1}}));
    assert!(err.contains("no settings"), "{err}");
}

#[test]
fn undo_puts_the_old_wall_back() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.5}}));
    ok(&mut e, json!({"cmd": "undo"}));
    assert!((z_range(&e, g).1 - 2.7).abs() < 1e-9);
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["settings"]["height"], 2.7);
}

#[test]
fn settings_survive_save_and_open_and_a_damaged_recipe_is_dropped() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let text = serde_json::to_string(&e.model).unwrap();
    let m: sketchcraft::model::Model = serde_json::from_str(&text).unwrap();
    assert!(m.groups[&(g as u32)].recipe.is_some());
    let bad = text.replace("\"thickness\":0.2", "\"thickness\":\"thick\"");
    assert_ne!(bad, text, "the test changed the recipe");
    let m: sketchcraft::model::Model = serde_json::from_str(&bad).expect("a bad recipe never stops a file opening");
    assert!(m.groups[&(g as u32)].recipe.is_none());
}

#[test]
fn a_door_in_a_wall_is_cut_again_after_a_rebuild() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let front = *e.model.faces.iter().find(|(_, f)| f.group == Some(g as u32) && e.model.pts(&f.outer).iter().all(|p| (p.y + 0.1).abs() < 1e-9)).unwrap().0;
    let r = ok(&mut e, json!({"cmd": "insert", "asset": "door", "at": [2, -0.1, 0], "host": front}));
    assert_eq!(ok(&mut e, json!({"cmd": "part_info", "group": g}))["edited"], false, "a door is part of the wall, not a hand edit: {r}");
    let solid_faces = e.model.faces.values().filter(|f| f.group == Some(g as u32)).count();
    let r = ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.0}}));
    assert_eq!(r["openings"], 1, "{r}");
    assert_eq!(e.model.faces.values().filter(|f| f.group == Some(g as u32)).count(), solid_faces, "the opening is there again");
}

#[test]
fn a_file_cannot_give_a_part_a_scaled_or_huge_frame() {
    let mut e = Engine::new();
    let g = wall(&mut e);
    let text = serde_json::to_string(&e.model).unwrap();
    let one = "\"frame\":[1.0,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0]";
    assert!(text.contains(one), "the test knows how a frame is written");
    for bad in ["[1e300,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0]", "[2.0,0.0,0.0,0.0,0.0,2.0,0.0,0.0,0.0,0.0,2.0,0.0,0.0,0.0,0.0,1.0]", "[1.0,0.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,0.0,1.0,0.0,1e300,0.0,0.0,1.0]"] {
        let m: sketchcraft::model::Model = serde_json::from_str(&text.replace(one, &format!("\"frame\":{bad}"))).unwrap();
        assert!(m.groups[&(g as u32)].recipe.is_none(), "{bad} is dropped");
    }
}

#[test]
fn a_rebuild_keeps_the_paint_and_edge_styles() {
    // R0311-R-E07: rebuild dropped paint and edge styles
    let mut e = Engine::new();
    let g = wall(&mut e);
    let front = *e.model.faces.iter().find(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).y < -0.99 && !f.outer.is_empty()).unwrap().0;
    ok(&mut e, json!({"cmd": "paint", "ids": [front], "color": [0.8, 0.3, 0.2]}));
    let mat = e.model.faces[&front].material.clone();
    let edge = *e.model.edges.iter().find(|(_, x)| x.group == Some(g as u32) && e.model.vertices[&x.a].p.z.abs() < 1e-9 && e.model.vertices[&x.b].p.z.abs() < 1e-9).unwrap().0;
    e.model.edges.get_mut(&edge).unwrap().soft = true;
    let ends = { let x = &e.model.edges[&edge]; (e.model.vertices[&x.a].p, e.model.vertices[&x.b].p) };
    // edited by hand? softening isn't geometry: no
    ok(&mut e, json!({"cmd": "rebuild", "group": g, "set": {"height": 3.0}}));
    let front = e.model.faces.iter().find(|(&k, f)| f.group == Some(g as u32) && e.model.normal(k).y < -0.99 && !f.outer.is_empty()).unwrap().1;
    assert_eq!(front.material, mat, "the front keeps its paint");
    let soft = e.model.edges.values().find(|x| { let (a, b) = (e.model.vertices[&x.a].p, e.model.vertices[&x.b].p); (a.distance(ends.0) < 1e-9 && b.distance(ends.1) < 1e-9) || (a.distance(ends.1) < 1e-9 && b.distance(ends.0) < 1e-9) });
    assert!(soft.is_some_and(|x| x.soft), "the same edge is still soft");
}

#[test]
fn a_file_recipe_with_too_many_corners_is_dropped() {
    // R0311-R-E05: a 4,096-corner recipe from a file made a forced rebuild run 8-29 s
    let mut e = Engine::new();
    let g = wall(&mut e);
    let text = serde_json::to_string(&e.model).unwrap();
    let pts: Vec<[f64; 3]> = (0..2500).map(|i| [i as f64 * 0.01, 0.0, 0.0]).collect();
    let bad = text.replace("\"points\":[[0.0,0.0,0.0],[4.0,0.0,0.0]]", &format!("\"points\":{}", serde_json::to_string(&pts).unwrap()));
    assert_ne!(bad, text, "the test found the wall's points");
    let m: sketchcraft::model::Model = serde_json::from_str(&bad).unwrap();
    assert!(m.groups[&(g as u32)].recipe.is_none());
}
