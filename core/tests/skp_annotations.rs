//! Dimensions from SketchUp files (#59): a classic .skp built in test code (no SketchUp file of ours has one that we may
//! commit), then read, then mangled every way a hostile file might be. The record layout was worked out from a
//! SketchUp 7 file that has four linear dimensions on a stair (36, 18, 96 and 72 inches, each drawn offset at a right
//! angle to the line it measures).
//!   cargo test -p sketchcraft-core --test skp_annotations

mod skp_fixture;
use glam::DVec3;
use skp_fixture::*;
use sketchcraft::model::{Annotation, Model};

/// Where an end of a dimension is.
enum End {
    /// on the nth vertex the edge before it made (0 or 1)
    Vertex(usize),
    Free([f64; 3]),
}

struct Dim {
    ends: [End; 2],
    text: &'static str,
    dir: [f64; 3],
    dist: f64,
}

const BASE: u32 = 1000;

/// A SketchUp 7 file whose own entities are one edge (0,0,0)-(100,0,0) inches and the given dimensions.
fn skp_with(dims: &[Dim]) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.0.extend([0xFF, 0xFE, 0xFF, 0x0E]);
    for u in "SketchUp Model".encode_utf16() {
        w.u16(u);
    }
    w.ustr("{7.1.6087}");
    w.0.extend([0u8; 400]);
    // the model's own list: [count][objects]; its first class takes BASE
    w.u32(2 + dims.len() as u32);
    w.class("CEdge").header(); // class BASE, object BASE + 1
    w.class("CVertex").vertex(0.0, 0.0, 0.0); // class +2, object +3
    w.u16(0x8000 | (BASE + 2) as u16).vertex(100.0, 0.0, 0.0); // object +4
    w.u16(0); // no curve
    // a second edge on the same two vertices, by reference (it pins down where the class numbers start)
    w.u16(0x8000 | BASE as u16).header().u16((BASE + 3) as u16).u16((BASE + 4) as u16).u16(0);
    let vertex = [BASE + 3, BASE + 4];
    let mut counter = BASE + 6;
    let (mut dim_class, mut font) = (None, None);
    for d in dims {
        match dim_class {
            None => {
                w.class("CDimensionLinear");
                dim_class = Some(counter);
                counter += 2;
            }
            Some(c) => {
                w.u16(0x8000 | c as u16);
                counter += 1;
            }
        }
        w.header().ustr(d.text);
        match font {
            None => {
                w.class("CSkFont").u16(0).ustr("Tahoma").u32(12).u8(0).f64(10.0).0.extend([0u8; 2]);
                font = Some(counter + 1);
                counter += 2;
            }
            Some(f) => {
                w.u16(f as u16);
            }
        }
        w.u8(0).u32(3);
        for e in &d.ends {
            w.u32(2).u32(4);
            match e {
                End::Vertex(i) => w.f64(0.0).f64(0.0).f64(0.0).u32(vertex[*i]),
                End::Free(p) => w.f64(p[0]).f64(p[1]).f64(p[2]).u32(0),
            };
            w.0.extend([0u8; 8]);
        }
        // 6 doubles: the offset's direction, then the plane's
        w.f64(d.dir[0]).f64(d.dir[1]).f64(d.dir[2]).f64(-1.0).f64(0.0).f64(0.0);
        w.u32(2).f64(d.dist);
        w.0.extend([0u8; 8]);
        w.u32(3);
    }
    w.0.extend([0u8; 64]);
    w.0
}

fn one() -> Vec<u8> {
    skp_with(&[Dim { ends: [End::Vertex(0), End::Vertex(1)], text: "", dir: [0.0, 1.0, 0.0], dist: 12.0 }])
}

fn import(d: &[u8], at: DVec3) -> Option<(Model, sketchcraft::skp::ImportSummary)> {
    let mut m = Model::default();
    let s = sketchcraft::skp::import(&mut m, d, at, "t").ok()?;
    Some((m, s))
}

fn dims(m: &Model) -> Vec<(DVec3, DVec3, DVec3)> {
    m.annotations.iter().filter_map(|a| if let Annotation::Dimension { a, b, offset, .. } = a { Some((*a, *b, *offset)) } else { None }).collect()
}

fn near(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn a_dimension_between_two_vertices_is_imported_in_meters() {
    let (m, s) = import(&one(), DVec3::ZERO).expect("imports");
    assert_eq!((s.dimensions, s.labels, s.skipped_annotations), (1, 0, 0));
    let d = dims(&m);
    assert_eq!(d.len(), 1);
    assert!(near(d[0].0, DVec3::ZERO), "{:?}", d[0]);
    assert!(near(d[0].1, DVec3::new(2.54, 0.0, 0.0)), "100 inches");
    assert!(near(d[0].2, DVec3::new(0.0, 0.3048, 0.0)), "12 inches along y");
}

#[test]
fn a_free_end_and_the_placement_point_are_honoured() {
    let d = skp_with(&[Dim { ends: [End::Vertex(1), End::Free([100.0, 0.0, 36.0])], text: "", dir: [-1.0, 0.0, 0.0], dist: -5.0 }]);
    let at = DVec3::new(10.0, 20.0, 30.0);
    let (m, _) = import(&d, at).expect("imports");
    let d = dims(&m);
    assert!(near(d[0].0, at + DVec3::new(2.54, 0.0, 0.0)));
    assert!(near(d[0].1, at + DVec3::new(2.54, 0.0, 0.9144)));
    assert!(near(d[0].2, DVec3::new(0.127, 0.0, 0.0)), "a negative distance flips the offset: {:?}", d[0].2);
}

#[test]
fn typed_text_becomes_a_label_and_the_font_is_shared() {
    let d = skp_with(&[
        Dim { ends: [End::Vertex(0), End::Vertex(1)], text: "Rise 7\"", dir: [0.0, 0.0, 1.0], dist: 4.0 },
        Dim { ends: [End::Vertex(1), End::Vertex(0)], text: "", dir: [0.0, 0.0, 1.0], dist: 4.0 },
    ]);
    let (m, s) = import(&d, DVec3::ZERO).expect("imports");
    assert_eq!((s.dimensions, s.labels), (2, 1));
    let t: Vec<_> = m.annotations.iter().filter_map(|a| if let Annotation::Text { text, at, .. } = a { Some((text.clone(), *at)) } else { None }).collect();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].0, "Rise 7\"");
    assert!(near(t[0].1, DVec3::new(1.27, 0.0, 0.1016)), "{:?}", t[0].1);
}

#[test]
fn the_geometry_still_imports_beside_its_dimensions() {
    let (m, _) = import(&one(), DVec3::ZERO).unwrap();
    assert!(!m.edges.is_empty() || m.definitions.values().any(|d| !d.model.edges.is_empty()), "the edge came in");
}

#[test]
fn nonsense_coordinates_are_refused_not_imported() {
    for bad in [f64::NAN, f64::INFINITY, 1e300] {
        let d = skp_with(&[Dim { ends: [End::Free([bad, 0.0, 0.0]), End::Vertex(1)], text: "", dir: [0.0, 1.0, 0.0], dist: 1.0 }]);
        if let Some((m, _)) = import(&d, DVec3::ZERO) {
            assert!(dims(&m).is_empty(), "{bad}");
        }
        let d = skp_with(&[Dim { ends: [End::Vertex(0), End::Vertex(1)], text: "", dir: [0.0, 1.0, 0.0], dist: bad }]);
        if let Some((m, _)) = import(&d, DVec3::ZERO) {
            assert!(dims(&m).iter().all(|(a, b, o)| a.is_finite() && b.is_finite() && o.is_finite()), "{bad}");
        }
    }
}

#[test]
fn a_dimension_on_a_vertex_that_is_not_there_is_counted_not_guessed() {
    let mut d = skp_with(&[Dim { ends: [End::Vertex(0), End::Vertex(1)], text: "", dir: [0.0, 1.0, 0.0], dist: 1.0 }]);
    // point the first end at an object index nothing has
    let needle = (BASE + 3).to_le_bytes();
    let at = d.windows(4).rposition(|w| w == needle).unwrap();
    d[at..at + 4].copy_from_slice(&(BASE + 900).to_le_bytes());
    if let Some((m, s)) = import(&d, DVec3::ZERO) {
        assert!(dims(&m).is_empty());
        assert_eq!(s.skipped_annotations, 1);
    }
}

/// Past 32767 objects a reference is 0x7FFF and a u32 (6 bytes, not 4), and the 10 bytes after it are the same.
#[test]
fn a_dimension_end_on_a_vertex_past_32767_objects_is_read() {
    let mut d = one();
    let needle = (BASE + 3).to_le_bytes();
    let at = d.windows(4).rposition(|w| w == needle).unwrap();
    // [u32 ref][8 zeros] becomes [0x7FFF][u32 ref][10 zeros]
    let mut big = vec![0xFF, 0x7F];
    big.extend(needle);
    big.extend([0u8; 10]);
    d.splice(at..at + 12, big);
    let (m, s) = import(&d, DVec3::ZERO).expect("imports");
    assert_eq!((s.dimensions, s.skipped_annotations), (1, 0));
    assert!(near(dims(&m)[0].0, DVec3::ZERO));
}

#[test]
fn every_truncation_is_an_error_or_a_smaller_model_never_a_panic() {
    let d = skp_with(&[Dim { ends: [End::Vertex(0), End::Free([1.0, 2.0, 3.0])], text: "label", dir: [0.0, 1.0, 0.0], dist: 3.0 }]);
    for n in 0..d.len() {
        if let Some((m, _)) = import(&d[..n], DVec3::ZERO) {
            assert!(m.annotations.len() <= 2);
        }
    }
}

#[test]
fn garbled_dimension_records_never_panic_or_flood_the_model() {
    let d = skp_with(&(0..40).map(|i| Dim { ends: [End::Vertex(i % 2), End::Vertex(1 - i % 2)], text: "x", dir: [0.0, 1.0, 0.0], dist: i as f64 }).collect::<Vec<_>>());
    let start = d.len() - 40 * 150;
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let t = std::time::Instant::now();
    for _ in 0..600 {
        let mut x = d.clone();
        for _ in 0..1 + next() % 6 {
            let i = start + (next() as usize % (d.len() - start));
            x[i] = match next() % 3 {
                0 => 0xFF,
                1 => 0,
                _ => next() as u8,
            };
        }
        if let Some((m, _)) = import(&x, DVec3::ZERO) {
            assert!(m.annotations.len() <= 100);
        }
    }
    assert!(t.elapsed() < std::time::Duration::from_secs(60));
}

#[test]
fn a_long_label_comes_through_whole() {
    let long: &'static str = Box::leak("a".repeat(200).into_boxed_str());
    let (m, _) = import(&skp_with(&[Dim { ends: [End::Vertex(0), End::Vertex(1)], text: long, dir: [0.0, 1.0, 0.0], dist: 1.0 }]), DVec3::ZERO).unwrap();
    assert!(m.annotations.iter().any(|a| matches!(a, Annotation::Text { text, .. } if text.len() == 200)));
}

// ---- attached dimensions (#59): imported dimensions hold anchors and follow the geometry ----------------------------

use serde_json::{json, Value};
use sketchcraft::drafting::{Anchor, AnchorAt};
use sketchcraft::Engine;

fn anchors_of(m: &Model) -> Vec<[Option<Anchor>; 2]> {
    m.annotations.iter().filter_map(|a| if let Annotation::Dimension { anchors, .. } = a { Some((**anchors).clone()) } else { None }).collect()
}

#[test]
fn imported_dimensions_attach_to_the_vertices_they_measure() {
    let (m, s) = import(&one(), DVec3::ZERO).expect("imports");
    assert_eq!((s.dimensions, s.dimensions_attached, s.dimensions_unattached), (1, 1, 0));
    let an = &anchors_of(&m)[0];
    for (k, want) in [(0, DVec3::ZERO), (1, DVec3::new(2.54, 0.0, 0.0))] {
        let a = an[k].as_ref().expect("attached");
        assert!(matches!(a.at, AnchorAt::Vertex(_)), "{:?}", a.at);
        assert!(near(m.resolve_anchor(a).unwrap(), want));
        assert!(near(DVec3::from(a.last), want), "an anchor carries its world point");
    }
}

#[test]
fn a_free_end_on_an_edge_attaches_along_it_and_one_on_nothing_stays_free() {
    // the first end is free but lies on the edge (halfway); the second is in the air
    let d = skp_with(&[Dim { ends: [End::Free([50.0, 0.0, 0.0]), End::Free([100.0, 0.0, 36.0])], text: "", dir: [-1.0, 0.0, 0.0], dist: 5.0 }]);
    let (m, s) = import(&d, DVec3::ZERO).expect("imports");
    assert_eq!((s.dimensions, s.dimensions_attached, s.dimensions_unattached), (1, 0, 1));
    let an = &anchors_of(&m)[0];
    match &an[0].as_ref().expect("on the edge").at {
        AnchorAt::Between { t, .. } => assert!((t - 0.5).abs() < 1e-9, "{t}"),
        other => panic!("{other:?}"),
    }
    assert!(near(m.resolve_anchor(an[0].as_ref().unwrap()).unwrap(), DVec3::new(1.27, 0.0, 0.0)));
    assert!(an[1].is_none(), "nothing within a thousandth of an inch of the second end");
}

/// An engine that imported `d` with `import_skp` (as the app does), its scratch folder, and the command's answer.
fn engine_with(d: &[u8], name: &str) -> (Engine, std::path::PathBuf, Value) {
    let dir = std::env::temp_dir().join(format!("sc-skpdims-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("in.skp");
    std::fs::write(&p, d).unwrap();
    let mut e = Engine::new();
    let r = ok(&mut e, json!({"cmd": "import_skp", "path": p}));
    (e, dir, r)
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn first_dim(m: &Model) -> (DVec3, DVec3) {
    dims(m).first().map(|d| (d.0, d.1)).expect("a dimension")
}

fn vertex_at(m: &Model, p: DVec3) -> u32 {
    *m.vertices.iter().find(|(_, v)| v.p.distance(p) < 1e-9).expect("a vertex there").0
}

#[test]
fn moving_the_geometry_moves_the_dimension_and_undo_puts_it_back() {
    let (mut e, dir, r) = engine_with(&one(), "move");
    assert_eq!((r["dimensions_attached"].as_u64(), r["dimensions_unattached"].as_u64()), (Some(1), Some(0)), "{r}");
    let end = vertex_at(&e.model, DVec3::new(2.54, 0.0, 0.0));
    let depth = e.undo_depth();
    ok(&mut e, json!({"cmd": "move", "ids": [end], "by": [1.0, 0.5, 0.0]}));
    let (a, b) = first_dim(&e.model);
    assert!(near(a, DVec3::ZERO), "the other end stayed: {a}");
    assert!(near(b, DVec3::new(3.54, 0.5, 0.0)), "the moved end followed: {b}");
    assert_eq!(e.undo_depth(), depth + 1, "the dimension moved in the same undo step as the vertex");
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(near(first_dim(&e.model).1, DVec3::new(2.54, 0.0, 0.0)), "undo puts the dimension back");
    ok(&mut e, json!({"cmd": "redo"}));
    assert!(near(first_dim(&e.model).1, DVec3::new(3.54, 0.5, 0.0)), "redo moves it again");
    // moving the whole imported group moves both ends; the offset is kept
    ok(&mut e, json!({"cmd": "move", "ids": [r["group"]], "by": [0.0, 0.0, 2.0]}));
    let (a, b) = first_dim(&e.model);
    assert!(near(a, DVec3::new(0.0, 0.0, 2.0)) && near(b, DVec3::new(3.54, 0.5, 2.0)), "{a} {b}");
    assert!(near(dims(&e.model)[0].2, DVec3::new(0.0, 0.3048, 0.0)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_point_along_an_edge_stays_on_it_when_the_edge_is_split_and_moved() {
    // the second end is free, three quarters along the edge (1.905 m)
    let d = skp_with(&[Dim { ends: [End::Vertex(0), End::Free([75.0, 0.0, 0.0])], text: "", dir: [0.0, 1.0, 0.0], dist: 5.0 }]);
    let (mut e, dir, r) = engine_with(&d, "split");
    assert_eq!(r["dimensions_attached"], 1, "{r}");
    // a line across the edge at x = 1 m splits it there
    ok(&mut e, json!({"cmd": "line", "points": [[1.0, -1.0, 0.0], [1.0, 1.0, 0.0]]}));
    assert!(near(first_dim(&e.model).1, DVec3::new(1.905, 0.0, 0.0)), "the split moved nothing");
    // stretch the far piece: the point stays the same fraction along what it is on
    let far = vertex_at(&e.model, DVec3::new(2.54, 0.0, 0.0));
    ok(&mut e, json!({"cmd": "move", "ids": [far], "by": [1.54, 0.0, 0.0]}));
    let (_, b) = first_dim(&e.model);
    assert!(b.y.abs() < 1e-9 && b.x > 1.905 + 1e-6 && b.x < 4.08, "it stayed on the stretched edge and moved along: {b}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn deleting_what_a_dimension_measures_leaves_it_where_it_was() {
    let (mut e, dir, _) = engine_with(&one(), "delete");
    let edges: Vec<u32> = e.model.edges.keys().copied().collect();
    let before = first_dim(&e.model);
    ok(&mut e, json!({"cmd": "delete", "ids": edges}));
    let after = first_dim(&e.model);
    assert!(near(before.0, after.0) && near(before.1, after.1), "a broken anchor keeps its last point");
    // and a later edit doesn't trip over it
    ok(&mut e, json!({"cmd": "line", "points": [[0, 0, 0], [1, 1, 1]]}));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_attached_dimension_saves_opens_and_still_follows() {
    let (mut e, dir, _) = engine_with(&one(), "save");
    let f = dir.join("m.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": f}));
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(v["format"], 2, "a 0.3.7 build would drop the anchors, so it must open this read-only");
    let ann = v["annotations"].as_array().unwrap().iter().find(|a| a["kind"] == "dimension").unwrap();
    assert!(ann["anchors"].as_array().is_some_and(|a| a.len() == 2), "{ann}");
    let mut e2 = Engine::new();
    ok(&mut e2, json!({"cmd": "open", "path": f}));
    assert!(e2.read_only.is_none());
    assert!(anchors_of(&e2.model)[0].iter().all(Option::is_some));
    let end = vertex_at(&e2.model, DVec3::new(2.54, 0.0, 0.0));
    ok(&mut e2, json!({"cmd": "move", "ids": [end], "by": [0.0, 0.0, 1.0]}));
    assert!(near(first_dim(&e2.model).1, DVec3::new(2.54, 0.0, 1.0)));
    std::fs::remove_dir_all(&dir).ok();
}

/// A saved model file (an empty model) with these annotations, written as format number `format`.
fn file_with(format: u64, annotations: &str) -> String {
    let mut v = serde_json::to_value(Model::default()).unwrap();
    v["format"] = json!(format);
    if format > 1 {
        v["needs"] = json!(["sheets"]);
    }
    v["next_id"] = json!(10);
    v["annotations"] = serde_json::from_str(annotations).unwrap();
    v.to_string()
}

#[test]
fn an_old_file_with_fixed_dimensions_opens_and_saves_unchanged() {
    // a 0.3.7 file: a dimension and a label, no anchors
    let old = file_with(1, r#"[{"kind":"dimension","id":3,"a":[0.0,0.0,0.0],"b":[1.5,0.0,0.0],"offset":[0.0,0.25,0.0]},{"kind":"text","id":4,"at":[1.0,1.0,0.0],"text":"hi","leader":null}]"#);
    let dir = std::env::temp_dir().join(format!("sc-skpdims-old-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("old.sketchcraft");
    std::fs::write(&f, old).unwrap();
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "open", "path": f}));
    assert!(e.read_only.is_none(), "an old file opens for editing");
    assert_eq!(anchors_of(&e.model), vec![[None, None]]);
    // an edit at the dimension's end leaves a fixed dimension alone
    ok(&mut e, json!({"cmd": "line", "points": [[0, 0, 0], [1.5, 0, 0]]}));
    let end = vertex_at(&e.model, DVec3::new(1.5, 0.0, 0.0));
    ok(&mut e, json!({"cmd": "move", "ids": [end], "by": [1, 0, 0]}));
    assert!(near(first_dim(&e.model).1, DVec3::new(1.5, 0.0, 0.0)), "a dimension without anchors keeps its points");
    let out = dir.join("again.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": out}));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(saved["format"], 1, "no anchors, no format change");
    assert_eq!(saved["annotations"][0], json!({"kind": "dimension", "id": 3, "a": [0.0, 0.0, 0.0], "b": [1.5, 0.0, 0.0], "offset": [0.0, 0.25, 0.0]}));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hand_edited_anchors_that_point_nowhere_open_and_stay_put() {
    let bad = file_with(2, r#"[{"kind":"dimension","id":3,"a":[0.0,0.0,0.0],"b":[1.5,0.0,0.0],"offset":[0.0,0.25,0.0],"anchors":[{"at":{"vertex":777},"last":[0.0,0.0,0.0]},{"path":[42,43],"at":{"between":{"a":1,"b":2,"t":0.5}},"last":[1.5,0.0,0.0]}]}]"#);
    let dir = std::env::temp_dir().join(format!("sc-skpdims-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("bad.sketchcraft");
    std::fs::write(&f, bad).unwrap();
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "open", "path": f}));
    ok(&mut e, json!({"cmd": "line", "points": [[0, 0, 0], [1, 0, 0]]}));
    assert!(near(first_dim(&e.model).1, DVec3::new(1.5, 0.0, 0.0)), "broken anchors keep their points");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn every_truncation_of_an_attached_import_settles_without_a_panic() {
    let d = skp_with(&[
        Dim { ends: [End::Vertex(0), End::Free([50.0, 0.0, 0.0])], text: "a", dir: [0.0, 1.0, 0.0], dist: 3.0 },
        Dim { ends: [End::Vertex(1), End::Free([9.0, 9.0, 9.0])], text: "", dir: [0.0, 1.0, 0.0], dist: 3.0 },
    ]);
    for n in 0..d.len() {
        if let Some((mut m, s)) = import(&d[..n], DVec3::ZERO) {
            assert_eq!(s.dimensions, s.dimensions_attached + s.dimensions_unattached);
            m.settle_anchors();
        }
    }
}
