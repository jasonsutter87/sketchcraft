//! Position Texture (`texture_position`, `texture_frame`): move, rotate, scale and skew a face's picture; on a plain
//! face, a painted-through one, the test-set crate and a projected can; undo, save and load, and the format guard.
use serde_json::{json, Value};
use sketchcraft::model::Id;
use sketchcraft::Engine;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn snapshot(e: &Engine) -> Value {
    serde_json::to_value(&e.model).unwrap()
}

/// a 2 x 1 m tile on a 4 x 2 m floor face, painted
fn floor() -> (Engine, Id) {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":PNG,"width":2.0,"height":1.0,"color":[0.6,0.3,0.2]}));
    let f = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[4,2,0]}))["face"].as_u64().unwrap() as Id;
    run(&mut e, json!({"cmd":"paint","ids":[f],"material":"Brick"}));
    (e, f)
}

fn pic(e: &mut Engine, f: Id) -> Vec<[f64; 2]> {
    let r = run(e, json!({"cmd":"texture_frame","face":f}));
    r["picture"][0].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect()
}

fn uv(e: &mut Engine, f: Id) -> Vec<[f64; 2]> {
    let r = run(e, json!({"cmd":"texture_uv","faces":[f]}));
    r["uv"][f.to_string()].as_array().unwrap_or_else(|| panic!("face {f} has no uv")).iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect()
}

fn near(a: &[[f64; 2]], b: &[[f64; 2]]) {
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(b) {
        assert!((p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5, "{a:?} vs {b:?}");
    }
}

#[test]
fn move_shifts_every_corner_by_the_offset() {
    let (mut e, f) = floor();
    let before = pic(&mut e, f);
    assert!(run(&mut e, json!({"cmd":"texture_frame","face":f}))["own"] == false);
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"offset":[0.5, -0.25]}}));
    let after = pic(&mut e, f);
    let want: Vec<[f64; 2]> = before.iter().map(|p| [p[0] + 0.5, p[1] - 0.25]).collect();
    near(&after, &want);
    // in tiles: 0.5 m over a 2 m tile is a quarter, 0.25 m over a 1 m tile is a quarter
    let t = uv(&mut e, f);
    let b: Vec<[f64; 2]> = before.iter().map(|p| [p[0] / 2.0 + 0.25, p[1] - 0.25]).collect();
    near(&t, &b);
}

#[test]
fn rotate_scale_and_skew_compose_and_reset_goes_home() {
    let (mut e, f) = floor();
    let start = pic(&mut e, f);
    // turn a quarter about the face's middle: the middle stays, a corner swings round it
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"rotation":90}}));
    let r = pic(&mut e, f);
    let mid = |p: &[[f64; 2]]| p.iter().fold([0.0, 0.0], |a, q| [a[0] + q[0] / 4.0, a[1] + q[1] / 4.0]);
    near(&[mid(&r)], &[mid(&start)]);
    let (c0, c1) = ((start[0][0] - mid(&start)[0], start[0][1] - mid(&start)[1]), (r[0][0] - mid(&r)[0], r[0][1] - mid(&r)[1]));
    assert!((c1.0 + c0.1).abs() < 1e-5 && (c1.1 - c0.0).abs() < 1e-5, "{c0:?} -> {c1:?}");
    // scale 2 about an explicit pivot, then a shear: the pivot stays put under both
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"pivot":[1.0, 1.0],"scale":2.0}}));
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"pivot":[1.0, 1.0],"skew":30}}));
    assert_ne!(pic(&mut e, f), r);
    assert!(e.model.faces[&f].uv.is_some());
    // the matrix form is the same thing spelled out
    let (mut e2, f2) = floor();
    run(&mut e2, json!({"cmd":"texture_position","face":f2,"matrix":[1, 0, 0, 1, 1, 0]}));
    run(&mut e2, json!({"cmd":"texture_position","face":f2,"transform":{"offset":[-1, 0]}}));
    near(&pic(&mut e2, f2), &start);
    // reset: the material's own placement again
    run(&mut e, json!({"cmd":"texture_position","face":f,"reset":true}));
    assert!(e.model.faces[&f].uv.is_none() && e.model.faces[&f].tex.is_none());
    near(&pic(&mut e, f), &start);
}

#[test]
fn flips_and_turns_are_transforms_about_the_tile_middle() {
    let (mut e, f) = floor();
    let a = pic(&mut e, f);
    // flip left/right about x = 1 (the middle of a 2 m tile): corner x goes to 2 - x
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"pivot":[1.0, 0.5],"scale":[-1, 1]}}));
    near(&pic(&mut e, f), &a.iter().map(|p| [2.0 - p[0], p[1]]).collect::<Vec<_>>());
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"pivot":[1.0, 0.5],"scale":[-1, 1]}}));
    near(&pic(&mut e, f), &a);
}

#[test]
fn refused_calls_change_nothing() {
    let (mut e, f) = floor();
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"scale":0}})).contains("flat"));
    fails(&mut e, json!({"cmd":"texture_position","face":f}));
    fails(&mut e, json!({"cmd":"texture_position","transform":{"rotation":5}}));
    fails(&mut e, json!({"cmd":"texture_position","face":9999,"transform":{"rotation":5}}));
    // a face without a picture
    let g = run(&mut e, json!({"cmd":"rect","from":[10,0,0],"to":[12,2,0]}))["face"].as_u64().unwrap() as Id;
    assert!(fails(&mut e, json!({"cmd":"texture_position","face":g,"transform":{"rotation":5}})).contains("picture"));
    let after = snapshot(&e);
    // the rect added a face; nothing else moved
    assert_eq!(before["faces"][f.to_string()], after["faces"][f.to_string()]);
}

#[test]
fn one_undo_step_and_redo() {
    let (mut e, f) = floor();
    let before = snapshot(&e);
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"rotation":30,"scale":1.5}}));
    let moved = snapshot(&e);
    assert_ne!(before, moved);
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(snapshot(&e), before);
    run(&mut e, json!({"cmd":"redo"}));
    assert_eq!(snapshot(&e), moved);
}

#[test]
fn a_painted_through_face_keeps_its_place_and_the_neighbour_stays_put() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":PNG,"width":2.0,"height":1.0,"color":[0.6,0.3,0.2]}));
    let a = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[8,4,0]}))["face"].as_u64().unwrap() as Id;
    let b = run(&mut e, json!({"cmd":"rect","from":[8,0,0],"to":[8,4,6]}))["face"].as_u64().unwrap() as Id;
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    assert!(e.model.faces[&b].tex.is_some());
    let wall = pic(&mut e, b);
    let floor_before = pic(&mut e, a);
    run(&mut e, json!({"cmd":"texture_position","face":b,"transform":{"offset":[1.0, 0.0]}}));
    near(&pic(&mut e, b), &wall.iter().map(|p| [p[0] + 1.0, p[1]]).collect::<Vec<_>>());
    near(&pic(&mut e, a), &floor_before);
    // the placement is still there (uv wins over it); reset drops both
    assert!(e.model.faces[&b].tex.is_some() && e.model.faces[&b].uv.is_some());
    run(&mut e, json!({"cmd":"texture_position","face":b,"reset":true}));
    assert!(e.model.faces[&b].tex.is_none());
}

#[test]
fn positions_a_crate_face_inside_its_component() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":"crate","at":[0.0, 0.0, 0.0]}));
    let inst = r["instance"].as_u64().unwrap() as Id;
    run(&mut e, json!({"cmd":"texture","name":"Label","data":PNG,"width":0.3,"height":0.3}));
    let def = r["component"].as_u64().unwrap() as Id;
    // a root material can't be painted from inside a component, but a projection lays it on (and paints)
    run(&mut e, json!({"cmd":"texture_project","entity":inst,"material":"Label","mode":"box"}));
    run(&mut e, json!({"cmd":"edit","id":inst}));
    let face = *e.model.definitions[&def].model.faces.keys().next().unwrap();
    let before = pic(&mut e, face);
    run(&mut e, json!({"cmd":"texture_position","face":face,"transform":{"offset":[0.1, 0.0]}}));
    near(&pic(&mut e, face), &before.iter().map(|p| [p[0] + 0.1, p[1]]).collect::<Vec<_>>());
    assert!(e.model.definitions[&def].model.faces[&face].uv.is_some());
}

#[test]
fn a_projected_can_face_has_its_coordinates_rewritten() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":"can","at":[0.0, 0.0, 0.0]}));
    let (inst, def) = (r["instance"].as_u64().unwrap() as Id, r["component"].as_u64().unwrap() as Id);
    run(&mut e, json!({"cmd":"texture","name":"Label","data":PNG,"width":1.0,"height":1.0}));
    run(&mut e, json!({"cmd":"texture_project","entity":inst,"material":"Label","mode":"cylinder"}));
    run(&mut e, json!({"cmd":"edit","id":inst}));
    let face = *e.model.definitions[&def].model.faces.iter().find(|(_, f)| f.uv.is_some()).unwrap().0;
    let before = uv(&mut e, face);
    run(&mut e, json!({"cmd":"texture_position","face":face,"transform":{"offset":[0.0, 0.5]}}));
    let after = uv(&mut e, face);
    near(&after, &before.iter().map(|p| [p[0], p[1] + 0.5]).collect::<Vec<_>>());
}

#[test]
fn survives_save_and_the_file_says_what_it_needs() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-pos-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("floor.sketchcraft").to_string_lossy().to_string();
    let (mut e, f) = floor();
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"rotation":20,"offset":[0.3, 0.1]}}));
    run(&mut e, json!({"cmd":"save","path":path}));
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["format"], 2);
    assert_eq!(file["needs"], json!(["face_uv"]));
    let mut e2 = Engine::new();
    run(&mut e2, json!({"cmd":"open","path":path}));
    assert_eq!(e2.model.faces[&f], e.model.faces[&f]);
    assert!(sketchcraft::format::check(&std::fs::read(&path).unwrap()).is_none());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn painted_through_placements_are_marked_in_the_file() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":PNG,"width":2.0,"height":1.0}));
    let a = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[8,4,0]}))["face"].as_u64().unwrap() as Id;
    // a lone painted face has no placement: still format 1
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick"}));
    let plain: Value = serde_json::from_slice(&sketchcraft::format::to_vec(&e.model).unwrap()).unwrap();
    assert_eq!(plain["format"], 1);
    run(&mut e, json!({"cmd":"rect","from":[8,0,0],"to":[8,4,6]}));
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    assert!(e.model.faces.values().any(|f| f.tex.is_some()));
    let bytes = sketchcraft::format::to_vec(&e.model).unwrap();
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(file["format"], 2);
    assert_eq!(file["needs"], json!(["face_tex"]));
    assert!(sketchcraft::format::check(&bytes).is_none());
    // an older build that doesn't know "face_tex" opens it read-only
    let mut old = file.clone();
    old["needs"] = json!(["face_tex", "face_next"]);
    assert!(sketchcraft::format::check(old.to_string().as_bytes()).is_some());
}

#[test]
fn reset_with_a_change_starts_from_the_materials_own_placement() {
    let (mut e, f) = floor();
    let home = pic(&mut e, f);
    run(&mut e, json!({"cmd":"texture_position","face":f,"transform":{"rotation":40}}));
    // the default frame is what a reset gives, whatever the face holds now
    let r = run(&mut e, json!({"cmd":"texture_frame","face":f,"default":true}));
    assert_eq!(r["own"], false);
    let d: Vec<[f64; 2]> = r["picture"][0].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect();
    near(&d, &home);
    // one command: reset, then move
    run(&mut e, json!({"cmd":"texture_position","face":f,"reset":true,"matrix":[1, 0, 0, 1, 0.5, 0]}));
    near(&pic(&mut e, f), &home.iter().map(|p| [p[0] + 0.5, p[1]]).collect::<Vec<_>>());
}
