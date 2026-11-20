//! Limits and checks around texture coordinates, painting and exports (WildLeopard): per-face texture coordinates and
//! colours stay within range (and a file holding unreadable ones still opens), brush strokes have a point cap and a work
//! budget, many UV islands pack quickly, scoped paint is quick on big groups, per-face data counts toward the size
//! budgets, `texture_pixels` checks its block, exported texture names never collide, and agents take turns with others
//! for batches. Limits are shown by their refusal, never by running the expensive case.

use serde_json::{json, Value};
use sketchcraft::model::{Id, Model};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::texpaint::{Brush, Canvas, Path};
use sketchcraft::Engine;
use std::time::{Duration, Instant};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(e: &mut Engine, v: Value) -> String {
    let before = serde_json::to_string(&e.model).unwrap();
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    assert_eq!(serde_json::to_string(&e.model).unwrap(), before, "a refused command changes nothing");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-paint-uv-limits-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rect(e: &mut Engine, x: f64, z: f64) -> Id {
    run(e, json!({"cmd": "rect", "from": [x, 0, z], "to": [x + 1.0, 1, z]}))["face"].as_u64().unwrap() as Id
}

/// One face with a 64-pixel paint texture (unwrapped), and the material's name.
fn painted(size: u32) -> (Engine, Id, String) {
    let mut e = Engine::new();
    let f = rect(&mut e, 0.0, 0.0);
    let m = run(&mut e, json!({"cmd": "texture_paint_new", "faces": [f], "size": size, "unwrap": true}))["material"].as_str().unwrap().to_string();
    (e, f, m)
}

fn picture(e: &Engine, m: &str) -> String {
    e.model.materials[m].texture.as_ref().unwrap().data.clone()
}

// ---- texture coordinates stay numbers --------------------------------------------------------------------------

#[test]
fn texture_coordinates_out_of_range_are_refused_and_the_model_still_saves_and_opens() {
    let (mut e, f, m) = painted(64);
    // a projection scale too small to mean anything
    let err = refused(&mut e, json!({"cmd": "texture_project", "faces": [f], "mode": "planar", "material": m, "scale": 1e-300}));
    assert!(err.contains("scale"), "{err}");
    // coordinates pushed out of range by repeated transforms: the step that would leave the range is refused
    let n = e.model.faces[&f].outer.len();
    run(&mut e, json!({"cmd": "uv_set", "uv": {f.to_string(): vec![[1e9, 1e9]; n]}}));
    let err = refused(&mut e, json!({"cmd": "uv_transform", "faces": [f], "matrix": [1e9, 0, 0, 1e9, 0, 0]}));
    assert!(err.contains("texture coordinates"), "{err}");
    run(&mut e, json!({"cmd": "uv_set", "uv": {f.to_string(): vec![[0.5, 0.5]; n]}}));
    let mut refusals = 0;
    for _ in 0..5 {
        let r = e.exec_json(&json!({"cmd": "texture_position", "faces": [f], "matrix": [1e9, 1e-9, 1e-9, 1e9, 1e9, 1e9]}).to_string());
        refusals += (r["ok"] == false) as usize;
    }
    assert!(refusals > 0, "five 1e9 scalings can't all fit");
    e.model.check_sane().unwrap();
    // and the model saves and opens again, coordinates and all
    let d = scratch("roundtrip");
    let p = d.join("m.sketchcraft");
    run(&mut e, json!({"cmd": "save", "path": p}));
    let uv = e.model.faces[&f].uv.clone();
    let mut back = Engine::new();
    run(&mut back, json!({"cmd": "open", "path": p}));
    assert_eq!(back.model.faces[&f].uv, uv);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_model_check_covers_per_face_data() {
    let (mut e, f, _) = painted(64);
    e.model.check_sane().unwrap();
    let good = e.model.faces[&f].uv.clone();
    e.model.faces.get_mut(&f).unwrap().uv.as_mut().unwrap().outer[0] = [f32::INFINITY, 0.0];
    assert!(e.model.check_sane().is_err());
    e.model.faces.get_mut(&f).unwrap().uv = good;
    e.model.faces.get_mut(&f).unwrap().vcol = Some(sketchcraft::model::FaceColors { outer: vec![[f32::NAN, 1.0, 1.0, 1.0]; e.model.faces[&f].outer.len()], holes: vec![] });
    assert!(e.model.check_sane().is_err());
    // saving refuses rather than write a file that can't be read back
    let d = scratch("nosave");
    let r = e.exec_json(&json!({"cmd": "save", "path": d.join("x.sketchcraft")}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_file_with_unreadable_face_data_opens_without_it_and_says_so() {
    let (mut e, f, _) = painted(64);
    let g = rect(&mut e, 5.0, 0.0);
    run(&mut e, json!({"cmd": "uv_unwrap", "faces": [g]}));
    run(&mut e, json!({"cmd": "vertex_paint", "faces": [f], "color": [1, 0, 0]}));
    let d = scratch("damaged");
    let p = d.join("m.sketchcraft");
    run(&mut e, json!({"cmd": "save", "path": p}));
    // what an older version could write: nulls where the numbers were (and one too big for the file's f32)
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let face = &mut v["faces"][f.to_string()];
    face["uv"]["outer"][0] = json!([null, null]);
    face["vcol"]["outer"][1][0] = Value::Null;
    v["faces"][g.to_string()]["uv"]["outer"][2][1] = json!(1e39);
    std::fs::write(&p, v.to_string()).unwrap();
    let mut back = Engine::new();
    let r = run(&mut back, json!({"cmd": "open", "path": p}));
    let notes = r["skipped_pictures"].to_string();
    assert!(notes.contains("2 faces") && notes.contains("texture coordinates"), "{r}");
    assert!(back.model.faces[&f].uv.is_none() && back.model.faces[&f].vcol.is_none());
    assert!(back.model.faces[&g].uv.is_none());
    assert!(back.model.faces[&f].material.is_some(), "the face keeps its material");
    back.model.check_sane().unwrap();
    // and it saves cleanly again
    run(&mut back, json!({"cmd": "save", "path": d.join("fixed.sketchcraft")}));
    // a restore from such a file is brought into range the same way
    let damaged: Model = serde_json::from_value(v).unwrap();
    let mut r2 = Engine::new();
    r2.replace_model(damaged).unwrap();
    assert!(r2.model.faces[&f].uv.is_none());
    std::fs::remove_dir_all(&d).ok();
}

// ---- brush strokes ----------------------------------------------------------------------------------------------

#[test]
fn a_stroke_has_a_point_cap_and_a_radius_cap() {
    let (mut e, _, m) = painted(64);
    let brush = Brush { color: [1.0, 0.0, 0.0], opacity: 1.0, hardness: 0.5, erase: false, base: [1.0; 3] };
    let many: Vec<[f64; 2]> = (0..sketchcraft::texpaint::MAX_STROKE_POINTS + 1).map(|i| [i as f64 * 1e-5, 0.5]).collect();
    let err = e.model.texture_paint(&m, None, &Path::Uv(many, 0.01), &brush).unwrap_err().to_string();
    assert!(err.contains("at most"), "{err}");
    let many3: Vec<glam::DVec3> = (0..sketchcraft::texpaint::MAX_STROKE_POINTS + 1).map(|i| glam::DVec3::new(i as f64 * 1e-5, 0.5, 0.0)).collect();
    let err = e.model.texture_paint(&m, None, &Path::World(Some(many3), 0.01), &brush).unwrap_err().to_string();
    assert!(err.contains("at most"), "{err}");
    let err = refused(&mut e, json!({"cmd": "texture_paint_stroke", "material": m, "color": [1, 0, 0], "uv_points": [[0.5, 0.5]], "uv_radius": 1.5}));
    assert!(err.contains("uv_radius"), "{err}");
    // agents and plugins get the drawing commands' point cap
    let agent = Caller::Mcp(Workspace::anywhere());
    let r = e.exec_json_as(&agent, &json!({"cmd": "texture_paint_stroke", "material": m, "color": [1, 0, 0], "uv_points": vec![[0.5, 0.5]; 2001]}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    // an ordinary stroke still paints
    let before = picture(&e, &m);
    run(&mut e, json!({"cmd": "texture_paint_stroke", "material": m, "color": [1, 0, 0], "uv_points": [[0.4, 0.5], [0.5, 0.5], [0.6, 0.5]], "uv_radius": 0.05}));
    assert_ne!(picture(&e, &m), before);
}

#[test]
fn a_stroke_over_the_work_budget_is_refused_before_painting() {
    let (mut e, f, m) = painted(2048);
    let before = picture(&e, &m);
    // 300 picture-wide dabs on a 2048 picture: about 1.26 billion texels to look at
    let pts: Vec<[f64; 2]> = (0..300).map(|i| [0.5 + i as f64 * 1e-4, 0.5]).collect();
    let t = Instant::now();
    let err = refused(&mut e, json!({"cmd": "texture_paint_stroke", "material": m, "color": [1, 0, 0], "uv_points": pts, "uv_radius": 1.0}));
    assert!(err.contains("too much work"), "{err}");
    // the same on the model: a brush far bigger than the face, many points
    let pts3: Vec<[f64; 3]> = (0..300).map(|i| [0.5 + i as f64 * 1e-4, 0.5, 0.0]).collect();
    let err = refused(&mut e, json!({"cmd": "texture_paint_stroke", "material": m, "faces": [f], "color": [1, 0, 0], "points": pts3, "radius": 100}));
    assert!(err.contains("too much work"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(5), "refused before the work: {:?}", t.elapsed());
    assert_eq!(picture(&e, &m), before);
    // a few picture-wide dabs are within it
    run(&mut e, json!({"cmd": "texture_paint_stroke", "material": m, "color": [1, 0, 0], "uv_points": [[0.5, 0.5], [0.6, 0.5]], "uv_radius": 1.0}));
}

#[test]
fn strokes_paint_only_pictures_of_paint_texture_size() {
    let mut e = Engine::new();
    let f = rect(&mut e, 0.0, 0.0);
    let big = Canvas::filled(4100, 2, [200, 200, 200]).to_data_url().unwrap();
    run(&mut e, json!({"cmd": "texture", "name": "wide", "data": big, "width": 1, "height": 1}));
    run(&mut e, json!({"cmd": "paint", "ids": [f], "material": "wide"}));
    let err = refused(&mut e, json!({"cmd": "texture_paint_stroke", "material": "wide", "color": [1, 0, 0], "uv_points": [[0.5, 0.5]], "uv_radius": 0.01}));
    assert!(err.contains("4100 x 2") && err.contains("texture_paint_new"), "{err}");
}

#[test]
fn a_new_paint_texture_decodes_only_the_pictures_it_bakes() {
    let mut e = Engine::new();
    let f = rect(&mut e, 0.0, 0.0);
    let g = rect(&mut e, 5.0, 0.0);
    // a material elsewhere in the model whose picture can't be decoded: it is never looked at
    let mut m: sketchcraft::model::Material = serde_json::from_value(json!({"color": [1, 1, 1]})).unwrap();
    m.texture = Some(serde_json::from_value(json!({"data": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAAA==", "width": 1, "height": 1})).unwrap());
    e.model.materials.insert("broken".into(), m);
    e.model.faces.get_mut(&g).unwrap().material = Some("broken".into());
    let r = run(&mut e, json!({"cmd": "texture_paint_new", "faces": [f], "size": 64, "unwrap": true}));
    assert!(r.get("skipped_pictures").is_none(), "{r}");
    let r = run(&mut e, json!({"cmd": "texture_paint_new", "faces": [f], "size": 64, "unwrap": true, "base": "material"}));
    assert!(r.get("skipped_pictures").is_none(), "{r}");
    // the face that wears it does have it decoded (and said when it can't be)
    let r = run(&mut e, json!({"cmd": "texture_paint_new", "faces": [g], "size": 64, "unwrap": true, "base": "material"}));
    assert!(r["skipped_pictures"].to_string().contains("broken"), "{r}");
}

#[test]
fn vertex_paint_has_a_point_cap_and_a_work_budget() {
    let mut e = Engine::new();
    let f = rect(&mut e, 0.0, 0.0);
    let stroke = |n: usize| sketchcraft::vertex_paint::Stroke { color: [1.0, 0.0, 0.0, 1.0], points: (0..n).map(|i| glam::DVec3::new(i as f64 * 1e-6, 0.0, 0.0)).collect(), radius: 1e3, strength: 1.0, falloff: sketchcraft::vertex_paint::Falloff::Smooth };
    let err = e.model.vertex_paint(&[f], &stroke(sketchcraft::vertex_paint::MAX_STROKE_POINTS + 1)).unwrap_err().to_string();
    assert!(err.contains("at most"), "{err}");
    // a terrain of 36,000 triangles against 10,000 points that all reach it: about a billion corner tests
    let g = run(&mut e, json!({"cmd": "terrain_grid", "at": [0, 10, 0], "width": 135, "depth": 135, "spacing": 1}))["group"].as_u64().unwrap();
    let pts: Vec<[f64; 3]> = (0..10_000).map(|i| [i as f64 * 1e-6, 0.0, 0.0]).collect();
    let t = Instant::now();
    let err = refused(&mut e, json!({"cmd": "vertex_paint", "entity": g, "color": [1, 0, 0], "points": pts, "radius": 1000}));
    assert!(err.contains("too much work"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(3), "refused before the work: {:?}", t.elapsed());
    // points far from every face cost no corner tests at all
    let far: Vec<[f64; 3]> = (0..10_000).map(|i| [1e5 + i as f64 * 1e-3, 0.0, 0.0]).collect();
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "vertex_paint", "entity": g, "color": [1, 0, 0], "points": far, "radius": 1}));
    assert_eq!(r["faces"], 0);
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    // an ordinary stroke paints as before
    let r = run(&mut e, json!({"cmd": "vertex_paint", "faces": [f], "color": [1, 0, 0], "points": [[0, 0, 0]], "radius": 0.5}));
    assert_eq!(r["faces"], 1);
}

// ---- packing, scoped paint ---------------------------------------------------------------------------------------

/// A flat terrain of about 2 n² triangles with every edge a seam (each triangle its own island).
fn islands(n: u32) -> (Engine, Id) {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "terrain_grid", "width": n, "depth": n, "spacing": 1}))["group"].as_u64().unwrap() as Id;
    for ed in e.model.edges.values_mut() {
        ed.seam = true;
    }
    (e, g)
}

#[test]
fn thousands_of_uv_islands_unwrap_and_pack_quickly() {
    let (mut e, g) = islands(39);
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "uv_unwrap", "entity": g, "margin": 0}));
    let took = t.elapsed();
    assert!(r["islands"].as_u64().unwrap() > 3000, "{r}");
    assert!(took < Duration::from_secs(10), "{took:?}");
    // every face got coordinates inside the sheet; repacking is quick too
    assert!(e.model.faces.values().all(|f| f.uv.as_ref().is_some_and(|u| u.outer.iter().all(|p| (-1e-6..=1.0 + 1e-6).contains(&p[0]) && (-1e-6..=1.0 + 1e-6).contains(&p[1])))));
    let t = Instant::now();
    run(&mut e, json!({"cmd": "uv_pack", "entity": g, "margin": 0.001}));
    assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
}

#[test]
fn too_many_islands_at_once_are_refused_before_the_work() {
    let (mut e, g) = islands(160);
    let t = Instant::now();
    let err = refused(&mut e, json!({"cmd": "uv_unwrap", "entity": g}));
    assert!(err.contains("islands"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn scoped_paint_on_a_big_group_is_quick() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "terrain_grid", "width": 160, "depth": 160, "spacing": 1}))["group"].as_u64().unwrap();
    for scope in ["all", "connected", "object"] {
        let t = Instant::now();
        let r = run(&mut e, json!({"cmd": "paint", "ids": [g], "color": [0.2, 0.5, 0.2], "scope": scope}));
        assert!(r["faces"].as_array().unwrap().len() > 50_000);
        assert!(t.elapsed() < Duration::from_secs(5), "{scope}: {:?}", t.elapsed());
    }
    // the clicked faces still come first, in their order
    let f: Vec<Id> = e.model.faces.keys().copied().take(3).collect();
    let r = run(&mut e, json!({"cmd": "paint", "ids": [f[2], f[0]], "color": [0.9, 0.1, 0.1], "scope": "connected"}));
    let got: Vec<u64> = r["faces"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(&got[..2], &[f[0] as u64, f[2] as u64]);
}

/// The faces the object scope reaches from each seed, one seed at a time: every face of the seed's group joined to it
/// through shared edges (crossing any material), kept when it wears the seed's material.
fn object_reach_one_seed_at_a_time(m: &Model, seeds: &[Id]) -> Vec<Id> {
    let edges = |id: Id| -> std::collections::BTreeSet<(Id, Id)> {
        let f = &m.faces[&id];
        std::iter::once(&f.outer).chain(f.holes.iter()).flat_map(|l| (0..l.len()).map(move |i| (l[i].min(l[(i + 1) % l.len()]), l[i].max(l[(i + 1) % l.len()])))).collect()
    };
    let mut out = std::collections::BTreeSet::new();
    for &s in seeds {
        let (group, mat) = (m.faces[&s].group, m.faces[&s].material.clone());
        let mut seen = std::collections::BTreeSet::from([s]);
        let mut todo = vec![s];
        while let Some(id) = todo.pop() {
            let mine = edges(id);
            for (&n, f) in &m.faces {
                if !f.hidden && f.group == group && !edges(n).is_disjoint(&mine) && seen.insert(n) {
                    todo.push(n);
                }
            }
        }
        out.extend(seen.into_iter().filter(|id| m.faces[id].material == mat));
    }
    out.into_iter().collect()
}

#[test]
fn object_scoped_paint_reaches_the_same_faces_as_one_seed_at_a_time() {
    let mut e = Engine::new();
    let g1 = run(&mut e, json!({"cmd": "terrain_grid", "width": 9, "depth": 9, "spacing": 1}))["group"].as_u64().unwrap() as Id;
    let g2 = run(&mut e, json!({"cmd": "terrain_grid", "width": 6, "depth": 6, "spacing": 1}))["group"].as_u64().unwrap() as Id;
    for (i, name) in ["red", "green", "blue"].iter().enumerate() {
        e.model.materials.insert(name.to_string(), serde_json::from_value(json!({"color": [i as f32 / 3.0, 0.5, 0.5]})).unwrap());
    }
    let ids: Vec<Id> = e.model.faces.keys().copied().collect();
    for (i, id) in ids.iter().enumerate() {
        let f = e.model.faces.get_mut(id).unwrap();
        f.material = (i % 4 != 3).then(|| ["red", "green", "blue"][(i * 7 / 3) % 3].to_string());
        // a band of hidden faces cuts the first grid in two
        f.hidden = f.group == Some(g1) && (40..58).contains(&i);
    }
    let of = |g: Id| -> Vec<Id> { ids.iter().copied().filter(|f| e.model.faces[f].group == Some(g)).collect() };
    let (a, b) = (of(g1), of(g2));
    for seeds in [vec![a[0]], vec![a[0], a[100], b[5]], vec![a[45], a[1], a[2], a[3]], vec![b[0], b[1], b[2], b[3], b[4], a[150]], a.clone(), b.clone()] {
        let got = e.model.paint_targets(&seeds, Some("object")).unwrap();
        assert_eq!(got, object_reach_one_seed_at_a_time(&e.model, &seeds), "seeds {seeds:?}");
    }
}

#[test]
fn object_scoped_paint_over_many_materials_is_quick() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "terrain_grid", "width": 70, "depth": 70, "spacing": 1}))["group"].as_u64().unwrap();
    // every face its own material
    let ids: Vec<Id> = e.model.faces.keys().copied().collect();
    for (i, id) in ids.iter().enumerate() {
        let name = format!("m{i}");
        e.model.materials.insert(name.clone(), serde_json::from_value(json!({"color": [0.5, 0.5, 0.5]})).unwrap());
        e.model.faces.get_mut(id).unwrap().material = Some(name);
    }
    let t = Instant::now();
    let r = run(&mut e, json!({"cmd": "paint", "ids": [g], "color": [0.2, 0.5, 0.2], "scope": "object"}));
    assert_eq!(r["faces"].as_array().unwrap().len(), ids.len());
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn many_large_pinned_islands_are_answered_quickly() {
    let (mut e, _) = islands(110);
    let faces: Vec<Id> = e.model.faces.keys().copied().collect();
    let pins: Vec<Value> = faces[1..].iter().flat_map(|f| [json!([f, 0, 0, 0]), json!([f, 2, 1, 1])]).collect();
    let t = Instant::now();
    let err = refused(&mut e, json!({"cmd": "uv_unwrap", "faces": faces, "margin": 0, "pins": pins}));
    assert!(err.contains("fit"), "{err}");
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn an_array_into_a_big_model_is_quick_and_welds_as_before() {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd": "terrain_grid", "width": 200, "depth": 200, "spacing": 1}));
    let rects: Vec<Id> = (0..20).map(|k| rect(&mut e, 3.0 * k as f64, 250.0)).collect();
    let count = |e: &Engine| (e.model.vertices.len(), e.model.edges.len(), e.model.faces.len());
    let (v0, e0, f0) = count(&e);
    let t = Instant::now();
    run(&mut e, json!({"cmd": "array", "ids": rects, "by": [0, 0, 2], "count": 300}));
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    // copies apart from each other: each its own corners and sides
    assert_eq!(count(&e), (v0 + 24_000, e0 + 24_000, f0 + 6_000));
    // copies side by side share the side between them
    let one = rect(&mut e, 0.0, -20.0);
    let (v0, e0, f0) = count(&e);
    run(&mut e, json!({"cmd": "array", "ids": [one], "by": [1, 0, 0], "count": 50}));
    assert_eq!(count(&e), (v0 + 100, e0 + 150, f0 + 50));
    // and a copy onto an existing one reuses its corners
    let (v0, e0, f0) = count(&e);
    run(&mut e, json!({"cmd": "duplicate", "ids": [one], "by": [2, 0, 0]}));
    assert_eq!(count(&e), (v0, e0, f0 + 1));
}

// ---- size budgets ------------------------------------------------------------------------------------------------

#[test]
fn per_face_texture_data_counts_toward_the_size_budgets() {
    let mut e = Engine::new();
    let g = run(&mut e, json!({"cmd": "terrain_grid", "width": 30, "depth": 30, "spacing": 1}))["group"].as_u64().unwrap();
    let (est0, mem0) = (e.model.size_estimate(), e.model.approx_bytes());
    run(&mut e, json!({"cmd": "uv_unwrap", "entity": g, "method": "planar_islands"}));
    run(&mut e, json!({"cmd": "uv_lightmap", "entity": g}));
    run(&mut e, json!({"cmd": "vertex_paint", "entity": g, "color": [0.123456789, 0.987654321, 0.5]}));
    let real = serde_json::to_vec(&e.model).unwrap().len();
    let est = e.model.size_estimate();
    assert!(est >= real, "estimate {est} is below the real size {real}");
    assert!(est <= real * 5 / 2 + 2048, "estimate {est} is far above the real size {real}");
    assert!(est > est0 + 1800 * 3 * 40, "the coordinates and colours are counted ({est0} -> {est})");
    assert!(e.model.approx_bytes() > mem0 + 1800 * 3 * 20, "and in the undo memory ({mem0} -> {})", e.model.approx_bytes());
    // a painted-through placement too
    let f: Id = *e.model.faces.keys().next().unwrap();
    let mut m = e.model.clone();
    let before = m.size_estimate();
    m.faces.get_mut(&f).unwrap().tex = Some(serde_json::from_value(json!({"material": "brick", "u": [1, 0, 0], "v": [0, 1, 0], "origin": [0.123456789, 1e5, -3.5]})).unwrap());
    assert!(m.size_estimate() >= before + serde_json::to_string(&m.faces[&f].tex).unwrap().len());
}

// ---- texture_pixels ----------------------------------------------------------------------------------------------

#[test]
fn texture_pixels_checks_its_block_without_wrapping() {
    let (mut e, _, m) = painted(64);
    let err = refused(&mut e, json!({"cmd": "texture_pixels", "material": m, "w": 65536, "h": 65536}));
    assert!(err.contains("4096"), "{err}");
    let err = refused(&mut e, json!({"cmd": "texture_pixels", "material": m, "x": 999_999_999u64, "w": 64, "h": 64}));
    assert!(err.contains("outside"), "{err}");
    let r = run(&mut e, json!({"cmd": "texture_pixels", "material": m, "x": 60, "y": 60, "w": 4, "h": 4}));
    assert_eq!(r["pixels"].as_array().unwrap().len(), 4);
}

// ---- exported texture names --------------------------------------------------------------------------------------

#[test]
fn materials_whose_names_tidy_to_the_same_file_keep_their_own_pictures() {
    let mut e = Engine::new();
    let names = ["a b", "a_b", "A_B"];
    for (k, name) in names.iter().enumerate() {
        let f = rect(&mut e, 3.0 * k as f64, 0.0);
        let data = Canvas::filled(4, 4, [40 * k as u8, 100, 200]).to_data_url().unwrap();
        run(&mut e, json!({"cmd": "texture", "name": name, "data": data, "width": 1, "height": 1}));
        run(&mut e, json!({"cmd": "paint", "ids": [f], "material": name}));
    }
    let d = scratch("names");
    for (cmd, file) in [("export_obj", "m.obj"), ("export_dae", "m.dae"), ("export_fbx", "m.fbx")] {
        let sub = d.join(cmd);
        std::fs::create_dir_all(&sub).unwrap();
        run(&mut e, json!({"cmd": cmd, "path": sub.join(file)}));
        let dir = sub.join("m_textures");
        let mut files: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|x| x.unwrap().file_name().to_string_lossy().into_owned()).collect();
        files.sort();
        assert_eq!(files.len(), 3, "{cmd}: {files:?}");
        let mut contents: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(dir.join(f)).unwrap()).collect();
        contents.dedup();
        assert_eq!(contents.len(), 3, "{cmd}: each picture is its own file");
    }
    std::fs::remove_dir_all(&d).ok();
}

// ---- taking turns for batches ------------------------------------------------------------------------------------

#[test]
fn an_agent_lets_a_waiting_client_in_between_its_batches() {
    let agent = Caller::Mcp(Workspace::anywhere());
    let mut e = Engine::new();
    let go = |e: &mut Engine, c: &Caller, v: Value| e.exec_json_as(c, &v.to_string());
    // with nobody waiting, an agent begins batch after batch
    for _ in 0..3 {
        assert_eq!(go(&mut e, &agent, json!({"cmd": "begin_batch"}))["ok"], true);
        assert_eq!(go(&mut e, &agent, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}))["ok"], true);
        assert_eq!(go(&mut e, &agent, json!({"cmd": "end_batch"}))["ok"], true);
    }
    // a plugin told to wait gets the next turn
    assert_eq!(go(&mut e, &agent, json!({"cmd": "begin_batch"}))["ok"], true);
    let r = go(&mut e, &Caller::Plugin, json!({"cmd": "rect", "from": [3, 0, 0], "to": [4, 1, 0]}));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(go(&mut e, &agent, json!({"cmd": "end_batch"}))["ok"], true);
    let r = go(&mut e, &agent, json!({"cmd": "begin_batch"}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["error"].as_str().unwrap().contains("waiting for its turn"), "{r}");
    // reading doesn't use the turn; changing something does
    assert_eq!(go(&mut e, &Caller::Plugin, json!({"cmd": "info"}))["ok"], true);
    assert_eq!(go(&mut e, &agent, json!({"cmd": "begin_batch"}))["ok"], false);
    assert_eq!(go(&mut e, &Caller::Plugin, json!({"cmd": "rect", "from": [3, 0, 0], "to": [4, 1, 0]}))["ok"], true);
    assert_eq!(go(&mut e, &agent, json!({"cmd": "begin_batch"}))["ok"], true);
}
