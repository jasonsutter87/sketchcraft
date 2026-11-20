//! The UV editor's engine side: seams, `uv_unwrap` (islands, LSCM, packing, pins), `uv_pack`, `uv_set`, `uv_transform`,
//! the lightmap channel (`uv_lightmap`) and its exports, undo, save and load, and the format guard. Every test-set model
//! is unwrapped.
use serde_json::{json, Value};
use sketchcraft::model::{Id, Model};
use sketchcraft::Engine;

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

fn with(key: &str) -> Engine {
    let a = sketchcraft::assets::find(key).unwrap_or_else(|| panic!("no asset {key}"));
    let mut e = Engine::new();
    e.model = sketchcraft::assets::build(a).unwrap();
    e
}

fn all_faces(e: &Engine) -> Vec<Id> {
    e.model.faces.keys().copied().collect()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-uv-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Info {
    islands: usize,
    /// per face: (island, corners)
    faces: std::collections::BTreeMap<Id, (usize, Vec<[f64; 2]>)>,
}

fn info(e: &mut Engine, faces: &[Id], set: u8) -> Info {
    let r = run(e, json!({"cmd":"uv_info","faces":faces,"set":set}));
    let mut out = Info { islands: r["islands"].as_u64().unwrap() as usize, faces: Default::default() };
    for f in r["faces"].as_array().unwrap() {
        let uv = f["uv"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect();
        out.faces.insert(f["id"].as_u64().unwrap() as Id, (f["island"].as_u64().unwrap() as usize, uv));
    }
    out
}

/// The bounding box of each island: (x0, y0, x1, y1).
fn boxes(i: &Info) -> Vec<[f64; 4]> {
    let mut b = vec![[f64::MAX, f64::MAX, f64::MIN, f64::MIN]; i.islands];
    for (isl, c) in i.faces.values() {
        for p in c {
            b[*isl] = [b[*isl][0].min(p[0]), b[*isl][1].min(p[1]), b[*isl][2].max(p[0]), b[*isl][3].max(p[1])];
        }
    }
    b
}

fn check_layout(i: &Info, margin: f64, what: &str) {
    let b = boxes(i);
    for (k, r) in b.iter().enumerate() {
        assert!(r[0] >= -1e-6 && r[1] >= -1e-6 && r[2] <= 1.0 + 1e-6 && r[3] <= 1.0 + 1e-6, "{what}: island {k} {r:?} leaves 0..1");
        for (j, q) in b.iter().enumerate().skip(k + 1) {
            let apart = r[2] + margin - 1e-6 <= q[0] || q[2] + margin - 1e-6 <= r[0] || r[3] + margin - 1e-6 <= q[1] || q[3] + margin - 1e-6 <= r[1];
            assert!(apart, "{what}: islands {k} and {j} overlap or touch: {r:?} {q:?}");
        }
    }
}

/// Signed picture area of a triangle fan over each face's corners, over the model area: one island's mean density.
fn density(e: &Engine, i: &Info) -> Vec<f64> {
    let mut sum2 = vec![0.0; i.islands];
    let mut sum3 = vec![0.0; i.islands];
    for (id, (isl, c)) in &i.faces {
        let f = &e.model.faces[id];
        let pts = e.model.pts(&f.outer);
        for k in 1..f.outer.len() - 1 {
            let a3 = (pts[k] - pts[0]).cross(pts[k + 1] - pts[0]).length() * 0.5;
            let a2 = ((c[k][0] - c[0][0]) * (c[k + 1][1] - c[0][1]) - (c[k][1] - c[0][1]) * (c[k + 1][0] - c[0][0])).abs() * 0.5;
            sum2[*isl] += a2;
            sum3[*isl] += a3;
        }
    }
    sum2.iter().zip(&sum3).map(|(a, b)| a / b.max(1e-18)).collect()
}

// ---- every model of the test set -------------------------------------------------------------------------------------

#[test]
fn every_test_set_model_unwraps_to_islands_that_fit_the_sheet() {
    for key in ["dice", "can", "globe", "crate", "soccer_ball", "beaver"] {
        for method in ["conformal", "angle", "planar_islands"] {
            let mut e = with(key);
            let faces = all_faces(&e);
            let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"method":method}));
            assert_eq!(r["faces"].as_u64().unwrap() as usize, faces.len(), "{key} {method}");
            let i = info(&mut e, &faces, 1);
            assert_eq!(i.faces.len(), faces.len(), "{key} {method}: every face has UVs");
            assert!(i.islands >= 1);
            let margin = 0.01;
            eprintln!("{key:12} {method:15} faces {:4} islands {:4} (reported {:4}) split {} stretch {:.2} scale {:.3}", faces.len(), i.islands, r["islands"], r["split"], r["stretch"].as_f64().unwrap(), r["scale"].as_f64().unwrap());
            check_layout(&i, margin, &format!("{key} {method}"));
            // area distortion within a bound (1 = even density)
            let stretch = r["stretch"].as_f64().unwrap();
            assert!((1.0..4.0).contains(&stretch), "{key} {method}: stretch {stretch}");
            // one density for every island: each island's picture area over its model area is the same, within the
            // stretch the flattening allows
            let d = density(&e, &i);
            let mean = d.iter().sum::<f64>() / d.len() as f64;
            for (k, x) in d.iter().enumerate() {
                assert!(*x > mean / 3.0 && *x < mean * 3.0, "{key} {method}: island {k} density {x} vs {mean}");
            }
        }
    }
}

#[test]
fn the_unwrap_reports_what_it_did() {
    let mut e = with("crate");
    let faces = all_faces(&e);
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces}));
    for k in ["faces", "islands", "split", "pinned", "ignored_pins", "stretch", "scale"] {
        assert!(r.get(k).is_some(), "{k} in {r}");
    }
    // a box has no developable disc without seams: it is split where it must be
    let mut e = with("dice");
    let faces = all_faces(&e);
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces}));
    assert_eq!(r["split"], 1);
    assert_eq!(info(&mut e, &faces, 1).islands, 6);
}

#[test]
fn a_cube_with_a_marked_cross_unfolds_to_one_island_of_squares() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    // the dice's 12 edges: mark the 7 that leave a cross net (any spanning tree of the 6 faces' dual graph is a net)
    let m = &e.model;
    let edges: Vec<(Id, Vec<Id>)> = m
        .edges
        .iter()
        .map(|(&id, ed)| (id, faces.iter().copied().filter(|f| m.faces[f].uses_edge(ed.a, ed.b)).collect::<Vec<_>>()))
        .collect();
    // dual spanning tree from the first face
    let mut joined: std::collections::HashSet<Id> = Default::default();
    let mut seen: std::collections::HashSet<Id> = [faces[0]].into();
    let mut grew = true;
    while grew {
        grew = false;
        for (id, fs) in &edges {
            if fs.len() == 2 && seen.contains(&fs[0]) != seen.contains(&fs[1]) {
                seen.insert(fs[0]);
                seen.insert(fs[1]);
                joined.insert(*id);
                grew = true;
            }
        }
    }
    let seams: Vec<Id> = edges.iter().filter(|(id, fs)| fs.len() == 2 && !joined.contains(id)).map(|(id, _)| *id).collect();
    assert_eq!(seams.len(), 12 - 5);
    let r = run(&mut e, json!({"cmd":"uv_seam","edges":seams}));
    assert_eq!(r["changed"], 7);
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"seams":"marked"}));
    assert_eq!(r["islands"], 1, "{r}");
    assert_eq!(r["split"], 0, "the net is a disc: no further cutting");
    // a net is developable: LSCM keeps lengths, so the stretch is 1
    assert!(r["stretch"].as_f64().unwrap() < 1.001, "{r}");
    let i = info(&mut e, &faces, 1);
    assert_eq!(i.islands, 1);
    check_layout(&i, 0.01, "net");
}

#[test]
fn the_can_with_a_side_seam_unrolls_its_side_into_a_strip() {
    let mut e = with("can");
    let faces = all_faces(&e);
    // auto seams at 60 degrees cut the two rims; one vertical edge on the side is marked by hand
    let r = run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":60}));
    assert_eq!(r["changed"].as_u64().unwrap(), 64, "the two rims are 32 edges each: {r}");
    let side: Vec<Id> = faces.iter().copied().filter(|f| e.model.faces[f].outer.len() == 4).collect();
    assert_eq!(side.len(), 32);
    let vertical = {
        let m = &e.model;
        let f = &m.faces[&side[0]];
        let pts = m.pts(&f.outer);
        (0..4).find_map(|k| {
            let (a, b) = (f.outer[k], f.outer[(k + 1) % 4]);
            ((pts[k].z - pts[(k + 1) % 4].z).abs() > 0.1).then(|| m.find_edge(a, b).unwrap())
        })
        .unwrap()
    };
    run(&mut e, json!({"cmd":"uv_seam","edges":[vertical]}));
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"seams":"marked"}));
    assert_eq!(r["islands"], 3, "side strip and two caps: {r}");
    assert_eq!(r["split"], 0);
    let i = info(&mut e, &faces, 1);
    let b = boxes(&i);
    // the side strip is the island that holds the 32 quads
    let strip = i.faces[&side[0]].0;
    // its width over its height is the side's perimeter over the can's height (the strip is a rectangle, but it may be
    // turned: compare the long and the short sides)
    let m = &e.model;
    let f = &m.faces[&side[0]];
    let p = m.pts(&f.outer);
    let seg = (0..4).map(|k| p[k].distance(p[(k + 1) % 4])).fold(f64::MAX, f64::min);
    let perimeter = seg * 32.0;
    let h = 0.122;
    let (w2, h2) = (b[strip][2] - b[strip][0], b[strip][3] - b[strip][1]);
    let (long, short) = (w2.max(h2), w2.min(h2));
    let want = perimeter.max(h) / perimeter.min(h);
    assert!((long / short / want - 1.0).abs() < 0.03, "strip is {long} x {short}, wanted a ratio of {want}");
    check_layout(&i, 0.01, "can");
}

// ---- seams -----------------------------------------------------------------------------------------------------------

#[test]
fn seams_are_stored_on_edges_and_cleared() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    let some: Vec<Id> = e.model.edges.keys().copied().take(3).collect();
    let r = run(&mut e, json!({"cmd":"uv_seam","edges":some}));
    assert_eq!((r["changed"].as_u64(), r["on"].as_bool()), (Some(3), Some(true)));
    assert_eq!(e.model.edges.values().filter(|x| x.seam).count(), 3);
    // marking again changes nothing
    assert_eq!(run(&mut e, json!({"cmd":"uv_seam","edges":some}))["changed"], 0);
    let info = run(&mut e, json!({"cmd":"uv_info","faces":faces}));
    assert_eq!(info["seams"].as_array().unwrap().len(), 3);
    run(&mut e, json!({"cmd":"uv_seam","edges":some,"on":false}));
    assert_eq!(e.model.edges.values().filter(|x| x.seam).count(), 0);
    // auto: the dice's 12 edges are all hard (90 degrees)
    let r = run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":45}));
    assert_eq!(r["changed"], 12);
    run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":45,"on":false}));
    assert_eq!(e.model.edges.values().filter(|x| x.seam).count(), 0);
    // a gentle angle marks nothing on a cube's... nothing flatter than 90
    assert_eq!(run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":95}))["changed"], 0);
    // refused calls change nothing
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"uv_seam"})).contains("needs"));
    assert!(fails(&mut e, json!({"cmd":"uv_seam","edges":[999999]})).contains("not an edge"));
    assert!(fails(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":0})).contains("angle"));
    assert_eq!(snapshot(&e), before);
}

#[test]
fn a_seam_is_one_undo_step_and_survives_save_and_load_with_a_format_guard() {
    let dir = scratch("seam");
    let mut e = with("dice");
    let faces = all_faces(&e);
    let path = dir.join("a.sketchcraft").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"save","path":path}));
    let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(v.get("needs").is_none() && v["format"] == 1, "no seams: the file stays format 1");
    let d0 = e.undo_depth();
    run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":45}));
    assert_eq!(e.undo_depth(), d0 + 1);
    run(&mut e, json!({"cmd":"save","path":path}));
    let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["format"], 2);
    assert!(v["needs"].as_array().unwrap().iter().any(|n| n == "edge_seam"), "{v}");
    let mut back = Engine::new();
    run(&mut back, json!({"cmd":"open","path":path}));
    assert_eq!(back.model.edges.values().filter(|x| x.seam).count(), 12);
    assert!(sketchcraft::format::check(&std::fs::read(&path).unwrap()).is_none(), "this build reads it");
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(e.model.edges.values().filter(|x| x.seam).count(), 0);
    run(&mut e, json!({"cmd":"redo"}));
    assert_eq!(e.model.edges.values().filter(|x| x.seam).count(), 12);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_build_without_the_needs_would_open_read_only() {
    // what an older build sees: the needs it doesn't know
    let m = br#"{"format":2,"needs":["edge_seam","face_uv2","from_the_future"],"next_id":1}"#;
    let ro = sketchcraft::format::check(m).expect("unknown need");
    assert!(ro.reasons.iter().any(|r| r.contains("from_the_future")));
    assert!(!ro.reasons.iter().any(|r| r.contains("edge_seam") || r.contains("face_uv2")));
}

// ---- unwrap: undo, save/load, errors ---------------------------------------------------------------------------------

#[test]
fn an_unwrap_is_one_undo_step_that_restores_everything() {
    let mut e = with("globe");
    let faces = all_faces(&e);
    let before = snapshot(&e);
    let d0 = e.undo_depth();
    run(&mut e, json!({"cmd":"uv_unwrap","faces":faces}));
    assert_eq!(e.undo_depth(), d0 + 1);
    assert_ne!(snapshot(&e), before);
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(snapshot(&e), before);
    run(&mut e, json!({"cmd":"redo"}));
    assert!(e.model.faces.values().all(|f| f.uv.is_some()));
}

#[test]
fn unwrapped_uvs_survive_save_and_load_and_need_face_uv() {
    let dir = scratch("save");
    let mut e = with("crate");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"uv_unwrap","faces":faces}));
    let path = dir.join("c.sketchcraft").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"save","path":path}));
    let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(v["needs"].as_array().unwrap().iter().any(|n| n == "face_uv"));
    let mut back = Engine::new();
    run(&mut back, json!({"cmd":"open","path":path}));
    for (id, f) in &e.model.faces {
        assert_eq!(back.model.faces[id].uv, f.uv);
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn refused_unwraps_change_nothing() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap"})).contains("needs faces"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"method":"nope"})).contains("unknown unwrap method"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"seams":"some"})).contains("seams"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"margin":0.9})).contains("margin"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"angle":0})).contains("angle"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":[424242]})).contains("not a face"));
    assert!(fails(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"pins":[[424242, 0, 0.1, 0.1]]})).contains("pin"));
    assert_eq!(snapshot(&e), before);
}

#[test]
fn a_placed_component_is_unwrapped_in_its_definition() {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":"soccer_ball","at":[0.0, 0.0, 0.0]}));
    let inst = r["instance"].as_u64().unwrap();
    let def = r["component"].as_u64().unwrap() as Id;
    let r = run(&mut e, json!({"cmd":"uv_unwrap","entity":inst}));
    assert_eq!(r["faces"], 32);
    assert!(e.model.definitions[&def].model.faces.values().all(|f| f.uv.is_some()));
    let i = run(&mut e, json!({"cmd":"uv_info","entity":inst}));
    assert_eq!(i["faces"].as_array().unwrap().len(), 32);
}

// ---- pins ------------------------------------------------------------------------------------------------------------

/// A flat 3 x 1 strip of quads on the floor (separate faces sharing edges) and a separate square elsewhere.
fn strip_and_square() -> (Engine, Vec<Id>, Vec<Id>) {
    let mut e = Engine::new();
    let mut strip = vec![];
    for k in 0..3 {
        let x = k as f64;
        strip.push(run(&mut e, json!({"cmd":"rect","from":[x, 0.0, 0.0],"to":[x + 1.0, 1.0, 0.0]}))["face"].as_u64().unwrap() as Id);
    }
    let sq = run(&mut e, json!({"cmd":"rect","from":[10.0, 0.0, 0.0],"to":[11.0, 1.0, 0.0]}))["face"].as_u64().unwrap() as Id;
    (e, strip, vec![sq])
}

#[test]
fn pinned_corners_stay_where_they_are_put_and_the_rest_packs_around_them() {
    let (mut e, strip, sq) = strip_and_square();
    let mut all = strip.clone();
    all.extend(&sq);
    // pin two corners of the strip's first and last face to chosen places: the strip is then 0.6 wide
    let pins = json!([[strip[0], 0, 0.1, 0.1], [strip[2], 2, 0.7, 0.3]]);
    let first: Vec<_> = e.model.pts(&e.model.faces[&strip[0]].outer);
    let last: Vec<_> = e.model.pts(&e.model.faces[&strip[2]].outer);
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":all,"pins":pins}));
    assert_eq!(r["pinned"], 2, "{r}");
    assert_eq!(r["ignored_pins"], 0);
    let i = info(&mut e, &all, 1);
    let c0 = i.faces[&strip[0]].1[0];
    let c2 = i.faces[&strip[2]].1[2];
    assert!((c0[0] - 0.1).abs() < 1e-5 && (c0[1] - 0.1).abs() < 1e-5, "{c0:?}");
    assert!((c2[0] - 0.7).abs() < 1e-5 && (c2[1] - 0.3).abs() < 1e-5, "{c2:?}");
    // the strip is one conformal island: lengths scale together
    let _ = (first, last);
    assert_eq!(i.islands, 2);
    // the other island sits clear of the pinned one
    let b = boxes(&i);
    let (s, q) = (i.faces[&strip[0]].0, i.faces[&sq[0]].0);
    let apart = b[s][2] <= b[q][0] || b[q][2] <= b[s][0] || b[s][3] <= b[q][1] || b[q][3] <= b[s][1];
    assert!(apart, "{b:?}");
    let sb = b[q];
    assert!(sb[0] >= 0.0 && sb[1] >= 0.0 && sb[2] <= 1.0 && sb[3] <= 1.0);
}

#[test]
fn pinning_every_corner_of_one_island_keeps_it_while_the_others_repack() {
    let mut e = with("crate");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"uv_seam","faces":faces,"auto":60}));
    run(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"seams":"marked"}));
    let before = info(&mut e, &faces, 1);
    let face = *before.faces.keys().nth(1).unwrap();
    let pins: Vec<Value> = before.faces[&face].1.iter().enumerate().map(|(k, p)| json!([face, k, p[0], p[1]])).collect();
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":faces,"seams":"marked","pins":pins}));
    assert_eq!((r["pinned"].as_u64(), r["ignored_pins"].as_u64()), (Some(4), Some(0)), "{r}");
    let after = info(&mut e, &faces, 1);
    for (a, b) in before.faces[&face].1.iter().zip(&after.faces[&face].1) {
        assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5, "{a:?} vs {b:?}");
    }
    check_layout(&after, 0.01, "pinned crate");
}

#[test]
fn a_single_pin_is_ignored_and_said_so() {
    let (mut e, strip, _) = strip_and_square();
    let r = run(&mut e, json!({"cmd":"uv_unwrap","faces":strip,"pins":[[strip[0], 0, 0.5, 0.5]]}));
    assert_eq!((r["pinned"].as_u64(), r["ignored_pins"].as_u64()), (Some(0), Some(1)));
}

// ---- uv_set, uv_transform, uv_pack -----------------------------------------------------------------------------------

#[test]
fn uv_set_writes_corners_and_refuses_a_wrong_count() {
    let (mut e, strip, _) = strip_and_square();
    let f = strip[0];
    let d0 = e.undo_depth();
    run(&mut e, json!({"cmd":"uv_set","uv":{ f.to_string(): [[0.0,0.0],[0.5,0.0],[0.5,0.5],[0.0,0.5]] }}));
    assert_eq!(e.undo_depth(), d0 + 1);
    let i = info(&mut e, &[f], 1);
    assert_eq!(i.faces[&f].1[2], [0.5, 0.5]);
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"uv_set","uv":{ f.to_string(): [[0.0,0.0]] }})).contains("4 corners"));
    assert!(fails(&mut e, json!({"cmd":"uv_set","uv":{ "x": null }})).contains("not a face id"));
    assert!(fails(&mut e, json!({"cmd":"uv_set","uv":{}, "set": 3})).contains("set must be"));
    assert_eq!(snapshot(&e), before);
    // null drops them
    run(&mut e, json!({"cmd":"uv_set","uv":{ f.to_string(): null }}));
    assert!(e.model.faces[&f].uv.is_none());
    run(&mut e, json!({"cmd":"undo"}));
    assert!(e.model.faces[&f].uv.is_some());
}

#[test]
fn uv_transform_moves_an_island_or_chosen_corners_in_one_step() {
    let (mut e, strip, _) = strip_and_square();
    run(&mut e, json!({"cmd":"uv_unwrap","faces":strip}));
    let before = info(&mut e, &strip, 1);
    let d0 = e.undo_depth();
    run(&mut e, json!({"cmd":"uv_transform","faces":strip,"matrix":[0.5, 0.0, 0.0, 0.5, 0.1, 0.2]}));
    assert_eq!(e.undo_depth(), d0 + 1);
    let after = info(&mut e, &strip, 1);
    for f in &strip {
        for (a, b) in before.faces[f].1.iter().zip(&after.faces[f].1) {
            assert!((b[0] - (a[0] * 0.5 + 0.1)).abs() < 1e-6 && (b[1] - (a[1] * 0.5 + 0.2)).abs() < 1e-6);
        }
    }
    assert_eq!(after.islands, 1, "an island stays whole when it moves");
    // only the corners at one vertex: the island tears there
    let v = e.model.faces[&strip[0]].outer[0];
    run(&mut e, json!({"cmd":"uv_transform","faces":strip,"vertices":[v],"matrix":[1.0, 0.0, 0.0, 1.0, 0.05, 0.0]}));
    let moved = info(&mut e, &strip, 1);
    let a = after.faces[&strip[0]].1[0];
    let b = moved.faces[&strip[0]].1[0];
    assert!((b[0] - a[0] - 0.05).abs() < 1e-6);
    assert_eq!(moved.faces[&strip[0]].1[1], after.faces[&strip[0]].1[1]);
    // refused
    let snap = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"uv_transform","faces":strip,"matrix":[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]})).contains("flat"));
    let (_, _, sq) = (0, 0, strip_and_square().2);
    let _ = sq;
    assert_eq!(snapshot(&e), snap);
}

#[test]
fn uv_transform_on_a_face_without_uvs_is_refused() {
    let (mut e, strip, _) = strip_and_square();
    let before = snapshot(&e);
    assert!(fails(&mut e, json!({"cmd":"uv_transform","faces":strip,"matrix":[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]})).contains("no UVs"));
    assert_eq!(snapshot(&e), before);
}

#[test]
fn pack_repacks_the_islands_as_they_are_inside_the_sheet() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"uv_unwrap","faces":faces}));
    // shrink every face into a corner, then pack: the islands come back to full size, not overlapping
    let squash = [0.1, 0.0, 0.0, 0.1, 0.0, 0.0];
    run(&mut e, json!({"cmd":"uv_transform","faces":faces,"matrix":squash}));
    let small = boxes(&info(&mut e, &faces, 1));
    assert!(small.iter().all(|b| b[2] < 0.15));
    let r = run(&mut e, json!({"cmd":"uv_pack","faces":faces}));
    assert_eq!(r["islands"], 6);
    let i = info(&mut e, &faces, 1);
    check_layout(&i, 0.01, "packed");
    let used: f64 = boxes(&i).iter().map(|b| (b[2] - b[0]) * (b[3] - b[1])).sum();
    assert!(used > 0.5, "the sheet is used: {used}");
    // the shape of each island is kept (a face is still a square)
    for (_, c) in i.faces.values() {
        let (w, h) = ((c[1][0] - c[0][0]).hypot(c[1][1] - c[0][1]), (c[2][0] - c[1][0]).hypot(c[2][1] - c[1][1]));
        assert!((w - h).abs() < 1e-5);
    }
    // nothing to pack without UVs
    let mut f = with("dice");
    let ids = all_faces(&f);
    assert!(fails(&mut f, json!({"cmd":"uv_pack","faces":ids})).contains("no UVs"));
}

// ---- the lightmap channel --------------------------------------------------------------------------------------------

#[test]
fn the_lightmap_is_a_second_unwrap_that_never_changes_how_the_model_shows() {
    let mut e = with("globe");
    let faces = all_faces(&e);
    let before = sketchcraft::scene::build(&e.model, None);
    let uv_before: Vec<_> = e.model.faces.values().map(|f| f.uv.clone()).collect();
    let r = run(&mut e, json!({"cmd":"uv_lightmap","faces":faces}));
    assert_eq!(r["faces"].as_u64().unwrap() as usize, faces.len());
    assert!(e.model.faces.values().all(|f| f.uv2.is_some() && f.uv2.as_ref().unwrap().fits(f)));
    assert_eq!(e.model.faces.values().map(|f| f.uv.clone()).collect::<Vec<_>>(), uv_before, "the picture's UVs are untouched");
    assert_eq!(sketchcraft::scene::build(&e.model, None), before, "the viewport scene is the same");
    let i = info(&mut e, &faces, 2);
    assert_eq!(i.faces.len(), faces.len());
    // no overlaps, all inside, a margin of 0.02
    check_layout(&i, 0.02, "lightmap");
    // set 1 reads as before
    assert_eq!(info(&mut e, &faces, 1).faces.len(), 0);
    // undo is one step
    let d = e.undo_depth();
    run(&mut e, json!({"cmd":"uv_lightmap","faces":faces,"margin":0.05}));
    assert_eq!(e.undo_depth(), d + 1);
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(boxes(&info(&mut e, &faces, 2)).len(), i.islands);
    // clearing
    run(&mut e, json!({"cmd":"uv_lightmap","faces":faces,"clear":true}));
    assert!(e.model.faces.values().all(|f| f.uv2.is_none()));
}

#[test]
fn a_lightmap_needs_face_uv2_in_the_file() {
    let dir = scratch("uv2");
    let mut e = with("dice");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"uv_lightmap","faces":faces}));
    let path = dir.join("d.sketchcraft").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"save","path":path}));
    let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(v["needs"].as_array().unwrap().iter().any(|n| n == "face_uv2"), "{v}");
    let mut back = Engine::new();
    run(&mut back, json!({"cmd":"open","path":path}));
    assert!(back.model.faces.values().all(|f| f.uv2.is_some()));
    std::fs::remove_dir_all(dir).ok();
}

fn glb(path: &str) -> (Value, Vec<u8>) {
    let b = std::fs::read(path).unwrap();
    let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    (serde_json::from_slice(&b[20..20 + jl]).unwrap(), b[20 + jl + 8..].to_vec())
}

fn read_vec2(doc: &Value, bin: &[u8], acc: u64) -> Vec<[f32; 2]> {
    let a = &doc["accessors"][acc as usize];
    assert_eq!((a["type"].as_str(), a["componentType"].as_u64()), (Some("VEC2"), Some(5126)));
    let v = &doc["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
    (0..a["count"].as_u64().unwrap() as usize)
        .map(|i| {
            let f = |k: usize| f32::from_le_bytes(bin[off + i * 8 + k * 4..off + i * 8 + k * 4 + 4].try_into().unwrap());
            [f(0), f(1)]
        })
        .collect()
}

#[test]
fn glb_carries_texcoord_1_for_the_lightmap() {
    let dir = scratch("glb");
    let path = dir.join("g.glb").to_string_lossy().to_string();
    let mut e = with("soccer_ball");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"export_glb","path":path}));
    let (doc, _) = glb(&path);
    assert!(doc["meshes"][0]["primitives"].as_array().unwrap().iter().all(|p| p["attributes"].get("TEXCOORD_1").is_none()), "no lightmap, no TEXCOORD_1");
    run(&mut e, json!({"cmd":"uv_lightmap","faces":faces}));
    run(&mut e, json!({"cmd":"export_glb","path":path}));
    let (doc, bin) = glb(&path);
    let mut seen = 0;
    for p in doc["meshes"][0]["primitives"].as_array().unwrap() {
        let acc = p["attributes"]["TEXCOORD_1"].as_u64().expect("TEXCOORD_1 on every primitive of a lightmapped model");
        let pos = doc["accessors"][p["attributes"]["POSITION"].as_u64().unwrap() as usize]["count"].as_u64().unwrap();
        let uv = read_vec2(&doc, &bin, acc);
        assert_eq!(uv.len() as u64, pos, "one lightmap coordinate per position");
        // glTF's v runs down the sheet: all inside 0..1
        assert!(uv.iter().all(|p| (-1e-6..=1.0 + 1e-6).contains(&p[0]) && (-1e-6..=1.0 + 1e-6).contains(&p[1])));
        // and they are not all the same point
        assert!(uv.iter().any(|p| *p != uv[0]));
        seen += 1;
    }
    assert!(seen > 0);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn fbx_carries_a_second_uv_layer_for_the_lightmap() {
    let dir = scratch("fbx");
    let (bin, asc) = (dir.join("g.fbx"), dir.join("g-ascii.fbx"));
    let mut e = with("dice");
    let faces = all_faces(&e);
    run(&mut e, json!({"cmd":"export_fbx","path":bin}));
    assert!(!std::fs::read(&bin).unwrap().windows(10).any(|w| w == b"LightmapUV"));
    run(&mut e, json!({"cmd":"uv_lightmap","faces":faces}));
    let r = run(&mut e, json!({"cmd":"export_fbx","path":bin}));
    assert!(std::fs::read(&bin).unwrap().windows(10).any(|w| w == b"LightmapUV"));
    run(&mut e, json!({"cmd":"export_fbx","path":asc,"binary":false}));
    let text = std::fs::read_to_string(&asc).unwrap();
    assert!(text.contains("Name: \"LightmapUV\""));
    // two LayerElementUV nodes (index 0 only exists when a picture's UVs do; the lightmap is index 1), in their own layer
    assert!(text.contains("LayerElementUV: 1 {"));
    let at = text.find("LightmapUV").unwrap();
    let uv = &text[at..];
    let at = uv.find("UV: *").expect("a UV array");
    let n: usize = uv[at + 5..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
    assert_eq!(n, r["triangles"].as_u64().unwrap() as usize * 3 * 2, "a (u, v) per triangle corner");
    assert!(text.contains("TypedIndex: 1"));
    // our importer still reads the file
    let mut back = Model::default();
    let imp = sketchcraft::fbx::import(&mut back, &std::fs::read(&bin).unwrap(), "Back", glam::DVec3::ZERO).unwrap();
    assert_eq!(imp.meshes, r["meshes"].as_u64().unwrap() as usize);
    std::fs::remove_dir_all(dir).ok();
}
