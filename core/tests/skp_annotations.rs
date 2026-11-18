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
