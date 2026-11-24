//! A classic SketchUp 8 file with several placements (#201), built in test code (the user file that showed the
//! problem is of unknown license, so it is not committed; set SKP_RUSTIC_BED_8 to its path to check the real one).
//!   cargo test -p sketchcraft-core --test skp_su8_nested

mod skp_fixture;
use glam::DVec3;
use skp_fixture::*;
use sketchcraft::model::Model;

/// A classic file of the given version: one definition (a single edge, 100 inches along x), and a model list that
/// places it twice, 200 inches apart.
/// A placement ends right after its name here: the 16-byte GUID is only in SketchUp 2013 and later.
fn skp(version: &str) -> Vec<u8> {
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
    w.u32(2);
    w.class("CComponentInstance"); // class 16, object 17
    place(&mut w, 0.0);
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
fn sketchup_8_placements_have_no_guid_so_every_one_is_read() {
    // before: the reader skipped 16 bytes after each SketchUp 8 placement's name and lost every placement after the first
    let (s, m) = read(&skp("{8.0.14345}"));
    assert_eq!(s.version, "{8.0.14345}");
    assert_eq!((s.components, s.placed), (1, 2));
    assert_eq!(m.instances.len(), 2);
}

fn real(var: &str) -> Option<Vec<u8>> {
    std::fs::read(std::env::var_os(var)?).ok()
}

/// The reporter's Rustic Pine Bed.skp (SketchUp 8.0.14345): 33 components, each of the 4 nested assemblies placed in full.
#[test]
fn the_real_rustic_pine_bed_opens_in_full_when_the_file_is_there() {
    let Some(d) = real("SKP_RUSTIC_BED_8") else { return };
    let (s, m) = read(&d);
    assert_eq!((s.faces, s.components), (310, 33));
    let nested: usize = m.definitions.values().map(|d| d.model.instances.len()).sum();
    assert_eq!(nested, 76, "placements inside the definitions (the file's bed is built from them)");
}
