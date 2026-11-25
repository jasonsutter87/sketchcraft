// The import report (#36): after a SketchUp import, what came across (counts) and what did not, each with why. The
// engine's `import_skp` reply carries it as `report`; the app shows it (app/src/importreport.ts).
//   cargo test --release -p sketchcraft-core --test skp_report
mod skp_fixture;
use glam::DVec3;
use serde_json::{json, Value};
use skp_fixture::*;
use sketchcraft::model::Model;
use sketchcraft::Engine;
use std::path::PathBuf;

fn corpus(file: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/skp_corpus").join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn import(d: &[u8]) -> sketchcraft::skp::ImportSummary {
    // pictures noted by an earlier import on this thread are not this file's
    sketchcraft::safety::take_skipped();
    sketchcraft::skp::import(&mut Model::default(), d, DVec3::ZERO, "t").expect("reads")
}

fn kinds(s: &sketchcraft::skp::ImportSummary) -> Vec<&str> {
    s.report.skipped.iter().map(|k| k.what.as_str()).collect()
}

#[test]
fn a_file_that_came_across_whole_has_nothing_listed_as_left_out() {
    for f in ["v06-extruded-box.skp", "v13-extruded-circle.skp", "v15-materials.skp", "v16-textured.skp", "v20-model.skp", "v21-components.skp", "v22-soft-block.skp"] {
        let s = import(&corpus(f));
        assert!(s.report.skipped.is_empty(), "{f}: {:?}", s.report.skipped);
    }
}

#[test]
fn the_counts_are_what_the_file_holds() {
    let s = import(&corpus("v15-materials.skp"));
    assert_eq!((s.faces, s.report.materials, s.report.textured_materials, s.report.textures), (4, 4, 3, 3));
    let s = import(&corpus("v16-textured.skp"));
    assert_eq!((s.faces, s.report.materials, s.report.textured_materials, s.report.textures), (5, 1, 1, 1));
    // a 2021+ file with a picture
    let s = import(&modern_skp_with(Some(&tiny_png())));
    assert_eq!((s.report.textured_materials, s.report.textures), (1, 1));
    assert!(s.report.skipped.is_empty(), "{:?}", s.report.skipped);
}

#[test]
fn a_picture_that_cannot_be_read_is_listed_with_its_material_and_why() {
    // 2021+: a picture that isn't one
    let s = import(&modern_skp_with(Some(b"this is not a picture")));
    assert_eq!((s.report.textured_materials, s.report.textures), (1, 0));
    let k = &s.report.skipped[0];
    assert_eq!((k.what.as_str(), k.count), ("textures", Some(1)));
    assert!(k.why.contains("keeps its color"), "{}", k.why);
    assert!(k.items.iter().any(|i| i.starts_with("Red")), "{:?}", k.items);
    // 2013-2016: the same, from a picture whose bytes are damaged
    let mut d = corpus("v16-textured.skp");
    let name: Vec<u8> = "UVTile".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let from = d.windows(name.len()).position(|w| w == &name[..]).unwrap();
    let at = from + d[from..].windows(4).position(|w| w == [0x89, b'P', b'N', b'G']).unwrap();
    for b in &mut d[at + 8..at + 32] {
        *b ^= 0xff;
    }
    let s = import(&d);
    assert_eq!((s.report.textured_materials, s.report.textures), (1, 0));
    assert_eq!(kinds(&s), ["textures"]);
    assert!(s.report.skipped[0].items.iter().any(|i| i.starts_with("UVTile")), "{:?}", s.report.skipped[0].items);
}

#[test]
fn a_screen_text_is_listed_as_left_out() {
    let s = import(&corpus("v06-screen-text.skp"));
    assert_eq!(kinds(&s), ["screen texts"]);
    assert_eq!(s.report.skipped[0].count, Some(1));
    assert!(s.report.skipped[0].why.contains("window"));
}

/// The reply of `import_skp` as the app gets it.
#[test]
fn the_reply_carries_the_report_for_the_app() {
    let dir = std::env::temp_dir().join(format!("sc-skpreport-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("in.skp");
    std::fs::write(&path, corpus("v06-screen-text.skp")).unwrap();
    let mut e = Engine::new();
    let r: Value = e.exec_json(&json!({"cmd": "import_skp", "path": path}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    let rep = &r["report"];
    assert_eq!(rep["version"], "{6.0.1}");
    assert_eq!(rep["imported"]["faces"], 0);
    assert_eq!(rep["skipped"][0]["what"], "screen texts");
    assert_eq!(rep["skipped"][0]["count"], 1);
    // words from the file are marked as the file's, never ours
    assert!(rep["skipped"][0]["items"]["untrusted_text"].is_array());

    std::fs::write(&path, corpus("v16-textured.skp")).unwrap();
    let r = e.exec_json(&json!({"cmd": "import_skp", "path": path}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    let imported = &r["report"]["imported"];
    assert_eq!((imported["faces"].as_u64(), imported["textures"].as_u64(), imported["textured_materials"].as_u64(), imported["materials"].as_u64()), (Some(5), Some(1), Some(1), Some(1)));
    assert_eq!(r["report"]["skipped"].as_array().unwrap().len(), 0);
    std::fs::remove_dir_all(&dir).ok();
}
