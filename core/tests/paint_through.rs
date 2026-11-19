//! Paint Bucket reach (connected / all / object) and paint-through: a picture material laid across touching faces
//! as one continuous sheet.
use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::texmap::Mapping;
use sketchcraft::Engine;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
const TILE: (f64, f64) = (2.0, 1.0);

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn engine() -> Engine {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":PNG,"width":TILE.0,"height":TILE.1,"color":[0.6,0.3,0.2]}));
    e
}

/// floor, a wall rising from its right edge, and a ceiling folding back from the wall's top: a Z-shaped strip
fn strip(e: &mut Engine) -> [u32; 3] {
    let a = run(e, json!({"cmd":"rect","from":[0,0,0],"to":[8,4,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    let b = run(e, json!({"cmd":"rect","from":[8,0,0],"to":[8,4,6]}))["face"].as_u64().map(|x| x as u32).unwrap();
    let c = run(e, json!({"cmd":"rect","from":[8,0,6],"to":[14,4,6]}))["face"].as_u64().map(|x| x as u32).unwrap();
    [a, b, c]
}

fn material(e: &Engine, id: u32) -> Option<String> {
    e.model.faces[&id].material.clone()
}

/// the texture coordinates of every corner of face `id`, as (position, uv)
fn uvs(e: &Engine, id: u32) -> Vec<(DVec3, [f64; 2])> {
    let tex = e.model.materials["Brick"].texture.clone().unwrap();
    let f = e.model.world_faces().into_iter().find(|f| f.face == Some(id)).unwrap();
    let map = Mapping::of_face(&tex, &f);
    f.outer.iter().map(|p| (*p, map.uv(*p))).collect()
}

fn uv_at(e: &Engine, id: u32, p: DVec3) -> [f64; 2] {
    uvs(e, id).into_iter().find(|(q, _)| (*q - p).length() < 1e-9).unwrap_or_else(|| panic!("no corner {p} on face {id}")).1
}

fn assert_continuous(e: &Engine, a: u32, b: u32, edge: [DVec3; 2]) {
    for p in edge {
        let (ua, ub) = (uv_at(e, a, p), uv_at(e, b, p));
        assert!((ua[0] - ub[0]).abs() < 1e-9 && (ua[1] - ub[1]).abs() < 1e-9, "uv at {p} differs: face {a} {ua:?} vs face {b} {ub:?}");
    }
}

/// one uv unit spans the tile (2 x 1 m) on the face: texture distance equals real distance
fn assert_tile_size(e: &Engine, id: u32) {
    let pts = uvs(e, id);
    for (p, uv) in &pts {
        for (q, vq) in &pts {
            let (du, dv) = ((uv[0] - vq[0]) * TILE.0, (uv[1] - vq[1]) * TILE.1);
            let d = (du * du + dv * dv).sqrt();
            assert!((d - (*p - *q).length()).abs() < 1e-9, "tile size drifted on face {id}: {d} vs {}", (*p - *q).length());
        }
    }
}

#[test]
fn paint_through_a_folded_strip_is_continuous() {
    let mut e = engine();
    let [a, b, c] = strip(&mut e);
    let r = run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 3);
    for id in [a, b, c] {
        assert_eq!(material(&e, id).as_deref(), Some("Brick"));
        assert_tile_size(&e, id);
    }
    assert_continuous(&e, a, b, [DVec3::new(8.0, 0.0, 0.0), DVec3::new(8.0, 4.0, 0.0)]);
    assert_continuous(&e, b, c, [DVec3::new(8.0, 0.0, 6.0), DVec3::new(8.0, 4.0, 6.0)]);
    // it really is a different placement than each face's own default (the wall's texture would restart)
    let plain = Mapping::of(e.model.materials["Brick"].texture.as_ref().unwrap(), DVec3::X);
    let wall = uv_at(&e, b, DVec3::new(8.0, 0.0, 6.0));
    let alone = plain.uv(DVec3::new(8.0, 0.0, 6.0));
    assert!(wall != alone, "the wall should continue the floor's picture, not restart: {wall:?}");
}

#[test]
fn paint_through_survives_save_and_the_scene_carries_it() {
    let mut e = engine();
    let [a, _, _] = strip(&mut e);
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    let text = serde_json::to_string(&e.model).unwrap();
    let back: sketchcraft::model::Model = serde_json::from_str(&text).unwrap();
    assert!(back.faces.values().all(|f| f.tex.is_some()));
    let scene = sketchcraft::scene::build(&e.model, None);
    assert_eq!(scene["triTex"].as_array().unwrap().iter().filter(|t| t.as_i64() == Some(0)).count(), 6);
}

#[test]
fn a_box_paints_all_six_faces_and_the_seed_edges_line_up() {
    let mut e = engine();
    let seed = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[8,12,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    run(&mut e, json!({"cmd":"pushpull","face":seed,"distance":6}));
    let front = run(&mut e, json!({"cmd":"pick","at":[4,0,3],"facing":[0,-1,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    let r = run(&mut e, json!({"cmd":"paint","ids":[front],"material":"Brick","scope":"connected"}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 6);
    assert!(e.model.faces.values().all(|f| f.material.as_deref() == Some("Brick")));
    let ids: Vec<u32> = e.model.faces.keys().copied().collect();
    for &id in &ids {
        assert_tile_size(&e, id);
    }
    // the four faces that touch the seed continue it across the shared edge
    for &id in ids.iter().filter(|&&i| i != front) {
        let shared: Vec<DVec3> = uvs(&e, front).iter().map(|x| x.0).filter(|p| uvs(&e, id).iter().any(|(q, _)| (*q - *p).length() < 1e-9)).collect();
        if shared.len() == 2 {
            assert_continuous(&e, front, id, [shared[0], shared[1]]);
        }
    }
}

#[test]
fn connected_stops_at_other_materials_and_object_crosses_them() {
    let mut e = engine();
    let [a, b, c] = strip(&mut e);
    run(&mut e, json!({"cmd":"paint","ids":[b],"color":[1,0,0]}));
    let r = run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    assert_eq!(r["faces"], json!([a]), "the red wall is not crossed");
    assert_eq!(material(&e, c), None);
    // object: same-material faces anywhere on the connected mesh, across the red wall
    run(&mut e, json!({"cmd":"undo"}));
    let r = run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"object"}));
    assert_eq!(r["faces"], json!([a, c]));
    assert_eq!(material(&e, b).as_deref(), Some("#ff0000"));
}

#[test]
fn all_replaces_a_material_everywhere_and_one_undo_restores() {
    let mut e = engine();
    let one = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[3,3,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    let two = run(&mut e, json!({"cmd":"rect","from":[10,0,0],"to":[13,3,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    let three = run(&mut e, json!({"cmd":"rect","from":[20,0,0],"to":[23,3,0]}))["face"].as_u64().map(|x| x as u32).unwrap();
    run(&mut e, json!({"cmd":"paint","ids":[three],"color":[0,0,1]}));
    run(&mut e, json!({"cmd":"paint","ids":[one],"color":[0,1,0],"scope":"connected"}));
    assert_eq!(material(&e, one).as_deref(), Some("#00ff00"));
    assert_eq!(material(&e, two), None, "connected: the other loose square is not touched");
    run(&mut e, json!({"cmd":"undo"}));
    let r = run(&mut e, json!({"cmd":"paint","ids":[one],"color":[0,1,0],"scope":"all"}));
    assert_eq!(r["faces"], json!([one, two]));
    assert_eq!(material(&e, three).as_deref(), Some("#0000ff"));
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(material(&e, one), None);
    assert_eq!(material(&e, two), None);
    assert_eq!(material(&e, three).as_deref(), Some("#0000ff"));
}

#[test]
fn paint_through_is_one_undo_and_colors_just_fill() {
    let mut e = engine();
    let [a, b, c] = strip(&mut e);
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    run(&mut e, json!({"cmd":"undo"}));
    for id in [a, b, c] {
        assert_eq!(material(&e, id), None);
        assert!(e.model.faces[&id].tex.is_none());
    }
    let r = run(&mut e, json!({"cmd":"paint","ids":[b],"color":[0.2,0.4,0.6],"scope":"connected"}));
    assert_eq!(r["faces"].as_array().unwrap().len(), 3);
    for id in [a, b, c] {
        assert_eq!(material(&e, id), r["material"].as_str().map(str::to_string));
        assert!(e.model.faces[&id].tex.is_none(), "a plain color needs no placement");
    }
    // painting a face again drops its placement
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick"}));
    assert!(e.model.faces[&a].tex.is_none());
}

#[test]
fn unknown_scope_is_refused() {
    let mut e = engine();
    let [a, _, _] = strip(&mut e);
    let r = e.exec_json(&json!({"cmd":"paint","ids":[a],"color":[1,0,0],"scope":"galaxy"}).to_string());
    assert_eq!(r["ok"], false);
}
