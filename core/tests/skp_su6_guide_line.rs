//! A classic SketchUp 6 file with a guide line (#200), built in test code (the user files that showed the problems are of
//! unknown license, so they are not committed; set SKP_SHED_6 to its path to check the real one).
//!   cargo test -p sketchcraft-core --test skp_su6_guide_line

mod skp_fixture;
use glam::DVec3;
use skp_fixture::*;
use sketchcraft::model::Model;

/// A classic file of the given version: one definition (a single edge, 100 inches along x), and a model list that
/// places it twice, 200 inches apart, with a guide line (CConstructionLine) between the two when `guide` is set.
/// A placement ends right after its name here: the 16-byte GUID is only in SketchUp 2013 and later.
fn skp(version: &str, guide: bool) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.0.extend([0xFF, 0xFE, 0xFF, 0x0E]);
    for u in "SketchUp Model".encode_utf16() {
        w.u16(u);
    }
    w.ustr(version);
    w.0.extend([0u8; 400]);
    // the definition's list: [x][0][count]; its first class takes x + 1 (object x - 1 is the definition itself)
    w.u32(10).u16(0).u32(1);
    w.class("CEdge").header(); // class 11, object 12
    w.class("CVertex").vertex(0.0, 0.0, 0.0); // class 13, object 14
    w.u16(0x8000 | 13).vertex(100.0, 0.0, 0.0); // object 15
    w.u16(0); // no curve
    w.0.extend([0u8; 64]);
    let place = |w: &mut W, x: f64| {
        w.u16(0).u16(0).u8(0).u8(1).u8(1).0.extend([0u8; 5]);
        w.u16(9); // the definition's object
        for v in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, x, 0.0, 0.0, 1.0] {
            w.f64(v);
        }
        w.ustr("");
    };
    // the model's own list
    w.u32(if guide { 3 } else { 2 });
    w.class("CComponentInstance"); // class 16, object 17
    place(&mut w, 0.0);
    if guide {
        // a point, a direction, the two ends it runs to (endless: about +-1e30) and 4 bytes
        w.class("CConstructionLine").u16(0).u16(0).u8(0).u8(1).u8(1).0.extend([0u8; 5]);
        for v in [10.0, 20.0, 30.0, 0.0, 0.8, 0.6, -1.0e30, 1.0e30] {
            w.f64(v);
        }
        w.0.extend([0u8; 4]);
    }
    w.u16(0x8000 | 16);
    place(&mut w, 200.0);
    w.0.extend([0u8; 64]);
    w.0
}

fn read(d: &[u8]) -> (sketchcraft::skp::ImportSummary, Model) {
    let mut m = Model::default();
    let s = sketchcraft::skp::import(&mut m, d, DVec3::ZERO, "t").expect("reads");
    (s, m)
}

#[test]
fn a_sketchup_6_guide_line_is_read_past_so_the_models_list_reads_in_full() {
    // before: "CConstructionLine" was an unknown class, the model's list could not be read and the file "looked damaged"
    let (s, _) = read(&skp("{6.0.1145}", true));
    assert_eq!((s.components, s.placed), (1, 2), "both placements, either side of the guide line");
    let (s7, _) = read(&skp("{7.1.6087}", true));
    assert_eq!(s7.placed, 2);
}

#[test]
fn sketchup_6_without_a_guide_line_still_reads() {
    let (s, _) = read(&skp("{6.0.1145}", false));
    assert_eq!((s.components, s.placed), (1, 2));
}

fn real(var: &str) -> Option<Vec<u8>> {
    std::fs::read(std::env::var_os(var)?).ok()
}

/// The reporter's Shed.skp (SketchUp 6.0.1145, 2007): 205 faces, 7 components, 5 placed, 7 dimensions.
#[test]
fn the_real_shed_opens_when_the_file_is_there() {
    let Some(d) = real("SKP_SHED_6") else { return };
    let (s, _) = read(&d);
    assert_eq!((s.faces, s.components, s.placed, s.dimensions), (205, 7, 5, 7));
}
