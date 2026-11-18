//! A SketchUp 2016 file that would not open (#70, the "SpeedyBee" sample from #1): the classic reader stopped at a
//! Bezier curve's control points (an attribute holding an array of points) and could not read a picture (CImage) or a
//! text label (CText) in the model's own list. Built here in test code, because the real model is someone else's and is
//! not ours to commit. The record layouts were worked out from that file.
//!   cargo test -p sketchcraft-core --test skp_classic_curves
//! An optional test reads the real file when SPEEDYBEE_SKP names it:
//!   SPEEDYBEE_SKP=/path/SpeedyBee.skp cargo test --release -p sketchcraft-core --test skp_classic_curves -- --ignored

mod skp_fixture;
use glam::DVec3;
use skp_fixture::*;
use sketchcraft::model::Model;

const BASE: u32 = 100;

/// Where the model's own list puts the pieces: edge A on the curve, the picture, the label, edge C.
struct Opts {
    /// the control points of the Bezier curve's attribute (each a 3-double point)
    points: u32,
    /// the picture's rotation rows (the file stores three, then the origin, then the scale)
    picture: [f64; 13],
}

fn plain_edge(w: &mut W, x: f64) {
    w.u16(0x8000 | BASE as u16).header();
    w.u16(0x8000 | (BASE + 2) as u16).vertex(x, 0.0, 0.0);
    w.u16(0x8000 | (BASE + 2) as u16).vertex(x, 10.0, 0.0);
    w.u16(0);
}

/// A 2016 file: a triangle definition (object 9), then the model's own list of five: an edge on a Bezier curve
/// (the curve carries a dictionary with the control points), a plain edge, a picture placing the triangle's
/// definition, a text label, and another plain edge.
fn skp_2016(o: &Opts) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.0.extend([0xFF, 0xFE, 0xFF, 0x0E]);
    for u in "SketchUp Model".encode_utf16() {
        w.u16(u);
    }
    w.ustr("{16.1.1451}");
    w.0.extend([0u8; 400]);
    // definition 9: a triangle (class 11 on)
    let base = 11u32;
    w.u32(base - 1).u16(0).u32(1);
    w.class("CFace").header().f64(0.0).f64(0.0).f64(1.0).f64(0.0).u32(1);
    w.class("CLoop").u32(0x0101_0000);
    w.class("CEdgeUse").u16(0);
    w.class("CEdge").header();
    w.class("CVertex").vertex(0.0, 0.0, 0.0);
    w.u16(0x8000 | (base + 8) as u16).vertex(100.0, 0.0, 0.0);
    w.u16(0);
    w.u8(0).u16((base + 3) as u16);
    w.u16(0x8000 | (base + 4) as u16).u16(0);
    w.u16(0x8000 | (base + 6) as u16).header();
    w.u16((base + 10) as u16);
    w.u16(0x8000 | (base + 8) as u16).vertex(0.0, 100.0, 0.0);
    w.u16(0);
    w.u8(0).u16((base + 3) as u16);
    w.u16(0x8000 | (base + 4) as u16).u16(0);
    w.u16(0x8000 | (base + 6) as u16).header();
    w.u16((base + 13) as u16).u16((base + 9) as u16).u16(0);
    w.u8(0).u16((base + 3) as u16);
    w.u16(0);
    w.u16(0);
    w.0.extend([0u8; 32]);

    // the model's own list
    w.u32(5);
    // edge A: classes BASE (edge) and BASE + 2 (vertex), ending on a Bezier curve
    w.class("CEdge").header();
    w.class("CVertex").vertex(0.0, 0.0, 0.0);
    w.u16(0x8000 | (BASE + 2) as u16).vertex(0.0, 0.0, 10.0);
    // the curve: its dictionary holds the control points and the type
    w.class("CCurve");
    w.class("CAttributeContainer").u16(0);
    w.class("CAttributeNamed").u16(0).u32(0);
    w.ustr("skp");
    w.ustr("crvpts").u8(11).u32(o.points);
    for i in 0..o.points {
        w.u8(17).f64(i as f64).f64(0.0).f64(1.0);
    }
    w.ustr("crvtype").u8(10).ustr("Bezier");
    w.ustr("").u32(0);
    w.u16(0); // no more dictionaries
    w.u8(0).u32(1); // then the curve's own record: 3 bytes and the edge count
    // edge B
    plain_edge(&mut w, 20.0);
    // the picture: a placement, laid out like a component's
    w.class("CImage").header();
    w.u16(9);
    for v in o.picture {
        w.f64(v);
    }
    w.ustr("");
    w.0.extend([0x5au8; 16]);
    // the text label: header, its font, 16 bytes, an end (a vertex reference past 32767 objects takes 6 bytes),
    // a point and a direction, 15 bytes of counts and flags, the text, five zero bytes
    w.class("CText").header();
    w.class("CSkFont").u16(0).ustr("Tahoma").u32(12).u8(0).f64(10.0).0.extend([0u8; 2]);
    w.0.extend([0u8; 16]);
    w.u32(2).u32(4);
    w.0.extend([0u8; 24]);
    w.u16(0x7FFF).u32(0x8000 | 0x1234);
    w.0.extend([0u8; 10]);
    for v in [-5.9, 2.9, 3.77, 0.0, 0.0, 1.0] {
        w.f64(v);
    }
    w.u32(2).u32(1).u16(1).u32(3).u8(1);
    w.ustr("36x3");
    w.0.extend([0u8; 5]);
    // edge C
    plain_edge(&mut w, 30.0);
    w.0.extend([0u8; 64]);
    w.0
}

/// Rows (0,-1,0), (1,0,0), (0,0,1): a quarter turn about z that sends local +x to +y, 50 inches along x.
const QUARTER_TURN: [f64; 13] = [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 50.0, 0.0, 0.0, 1.0];

const IDENTITY: [f64; 13] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 50.0, 0.0, 0.0, 1.0];

fn import(d: &[u8]) -> Option<(Model, sketchcraft::skp::ImportSummary)> {
    let mut m = Model::default();
    let s = sketchcraft::skp::import(&mut m, d, DVec3::ZERO, "t").ok()?;
    Some((m, s))
}

#[test]
fn a_bezier_curve_attribute_does_not_stop_the_import() {
    let d = skp_2016(&Opts { points: 4, picture: IDENTITY });
    let (m, s) = import(&d).expect("a file whose curve holds an array of control points imports");
    // all three edges of the model's own list came in (A, B and C)
    assert_eq!(m.edges.len(), 3, "the edges on both sides of the curve, the picture and the label");
    // the triangle's definition is there, and the picture placed it
    assert_eq!(s.placed, 1, "the picture is a placement");
    assert_eq!(m.instances.len(), 1);
    // the label is found, not imported
    assert_eq!(s.skipped_annotations, 1);
}

#[test]
fn the_picture_is_placed_where_the_file_says() {
    let d = skp_2016(&Opts { points: 4, picture: IDENTITY });
    let (m, _) = import(&d).unwrap();
    let t = m.instances.values().next().unwrap().transform;
    assert!(t.transform_point3(DVec3::ZERO).distance(DVec3::new(50.0 * 0.0254, 0.0, 0.0)) < 1e-9);
}

/// The classic reader stores a placement's rotation as three rows, like the modern one (#71): read as columns, a part
/// turned a quarter turn came out turned the other way, and the parts of a model ended up scattered.
#[test]
fn a_placement_is_turned_the_way_the_file_says() {
    let d = skp_2016(&Opts { points: 4, picture: QUARTER_TURN });
    let (m, _) = import(&d).unwrap();
    let t = m.instances.values().next().unwrap().transform;
    // the triangle's corner at (100, 0, 0) inches goes to (50, 100, 0): up the y axis from the origin of the placement
    let got = t.transform_point3(DVec3::new(100.0 * 0.0254, 0.0, 0.0));
    assert!(got.distance(DVec3::new(50.0 * 0.0254, 100.0 * 0.0254, 0.0)) < 1e-9, "{got:?}");
}

#[test]
fn an_empty_and_a_long_array_both_read() {
    for points in [0, 1, 200] {
        let (m, _) = import(&skp_2016(&Opts { points, picture: IDENTITY })).unwrap_or_else(|| panic!("{points} points"));
        assert_eq!(m.edges.len(), 3, "{points} points");
    }
}

#[test]
fn an_array_that_claims_more_points_than_the_file_holds_is_refused_not_allocated() {
    let mut d = skp_2016(&Opts { points: 4, picture: IDENTITY });
    // find the array's count (4, right after the key and the array type byte) and claim four billion points
    let key: Vec<u8> = "crvpts".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let at = d.windows(key.len()).position(|w| w == key).unwrap() + key.len();
    assert_eq!(d[at], 11);
    d[at + 1..at + 5].copy_from_slice(&u32::MAX.to_le_bytes());
    let t = std::time::Instant::now();
    let _ = import(&d);
    assert!(t.elapsed().as_secs() < 5);
}

#[test]
fn every_truncation_and_a_burst_of_damage_is_an_error_or_a_smaller_model_never_a_panic() {
    let d = skp_2016(&Opts { points: 4, picture: IDENTITY });
    for n in 0..d.len() {
        let _ = import(&d[..n]);
    }
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let start = d.len() - 330;
    let t = std::time::Instant::now();
    for _ in 0..400 {
        let mut x = d.clone();
        for _ in 0..1 + next() % 6 {
            let at = start + (next() as usize) % 330;
            x[at] = next() as u8;
        }
        let _ = import(&x);
    }
    assert!(t.elapsed().as_secs() < 60);
}

/// The real SpeedyBee sample, when SPEEDYBEE_SKP names it: it must import, with the faces, edges and placements of the
/// SketchUp 2026 re-save (4,987 faces, 25,128 edges and 374 placements before the pictures were turned into faces).
#[test]
#[ignore]
fn the_real_speedybee_file_imports() {
    let Some(path) = std::env::var_os("SPEEDYBEE_SKP") else { return };
    let d = std::fs::read(path).unwrap();
    let (m, s) = import(&d).expect("SpeedyBee imports");
    let (faces, edges, placed) = flat_counts(&m);
    eprintln!("{} components, {} placed at the top, flattened: {faces} faces, {edges} edges, {placed} placements, {} annotations skipped", s.components, s.placed, s.skipped_annotations);
    assert!((4900..=5100).contains(&faces), "faces {faces}");
    assert!((24_500..=25_700).contains(&edges), "edges {edges}");
    assert!((360..=390).contains(&placed), "placements {placed}");
}

/// (faces, edges, placements) with every placement expanded through its definition (placements inside a definition
/// find their definition in the document's table).
fn flat_counts(doc: &Model) -> (usize, usize, usize) {
    fn walk(doc: &Model, m: &Model, depth: usize) -> (usize, usize, usize) {
        let (mut f, mut e, mut p) = (m.faces.len(), m.edges.len(), 0);
        if depth > 32 {
            return (f, e, p);
        }
        for i in m.instances.values() {
            p += 1;
            if let Some(d) = doc.definitions.get(&i.def) {
                let (a, b, c) = walk(doc, &d.model, depth + 1);
                f += a;
                e += b;
                p += c;
            }
        }
        (f, e, p)
    }
    walk(doc, doc, 0)
}
