// Textures in SketchUp 2013-2020 files (#17): their materials and the pictures on them come across with the right
// size. Real files from SketchUp's own test suite (tests/skp_corpus/, MIT); the 2017-2020 layout is also covered by
// a table built byte for byte like SketchUp 2020 writes one (the unit tests in core/src/skp.rs), and, when you have it,
// by a real 2020 file: SKP_TEXTURED_2020=/path/to/Hearst+Tower+(New+York)-su2020.skp (testup-2,
// tests/SketchUp Ruby API/TC_Sketchup_Texture/, 5 MB, so not kept here).
//   cargo test --release -p sketchcraft-core --test skp_textures
use glam::DVec3;
use sketchcraft::model::{Material, Model};
use std::path::PathBuf;

fn read(file: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/skp_corpus").join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn import(d: &[u8]) -> Model {
    let mut m = Model::default();
    sketchcraft::skp::import(&mut m, d, DVec3::ZERO, "t").expect("reads");
    m
}

/// Every face of the model and of its components: its material's name.
fn face_materials(m: &Model) -> Vec<Option<String>> {
    m.faces.values().map(|f| f.material.clone()).chain(m.definitions.values().flat_map(|d| d.model.faces.values().map(|f| f.material.clone()))).collect()
}

/// A material by name, from the model or from its components (the faces' own copies).
fn material<'a>(m: &'a Model, name: &str) -> Option<&'a Material> {
    m.materials.get(&format!("skp:{name}")).or_else(|| m.definitions.values().find_map(|d| d.model.materials.get(&format!("skp:{name}"))))
}

#[test]
fn a_2016_file_keeps_the_picture_on_its_faces_at_the_size_the_file_gives_it() {
    let m = import(&read("v16-textured.skp"));
    let mat = material(&m, "UVTile").expect("the picture material comes across");
    let tex = mat.texture.as_ref().expect("with its picture");
    assert!(tex.data.starts_with("data:image/jpeg;base64,") || tex.data.starts_with("data:image/png;base64,"), "{}", &tex.data[..40]);
    // 0.5 m square (a 512 px picture)
    assert!((tex.width - 0.5).abs() < 1e-6 && (tex.height - 0.5).abs() < 1e-6, "{} x {}", tex.width, tex.height);
    // all five faces are painted with it
    let on: Vec<_> = face_materials(&m).into_iter().filter(|f| f.as_deref() == Some("skp:UVTile")).collect();
    assert_eq!(on.len(), 5);
}

#[test]
fn materials_that_share_a_picture_each_get_it_with_their_own_size() {
    let m = import(&read("v15-materials.skp"));
    let (a, b, c) = (material(&m, "Textured"), material(&m, "TexturedShifted"), material(&m, "TexturedTinted"));
    for (name, mat) in [("Textured", a), ("TexturedShifted", b), ("TexturedTinted", c)] {
        let t = mat.unwrap_or_else(|| panic!("{name} comes across")).texture.as_ref().unwrap_or_else(|| panic!("{name} has its picture"));
        // 12 x 6 inches
        assert!((t.width - 0.3048).abs() < 1e-6 && (t.height - 0.1524).abs() < 1e-6, "{name}: {} x {}", t.width, t.height);
    }
    assert_eq!(a.unwrap().texture.as_ref().unwrap().data, b.unwrap().texture.as_ref().unwrap().data);
    // the plain one stays a color, and each of the four faces has its own material
    let solid = material(&m, "Solid").expect("the plain material");
    assert!(solid.texture.is_none());
    assert!((solid.color[0] - 0.749).abs() < 0.01);
    let mut used: Vec<_> = face_materials(&m).into_iter().flatten().collect();
    used.sort();
    assert_eq!(used, ["skp:Solid", "skp:Textured", "skp:TexturedShifted", "skp:TexturedTinted"]);
}

/// Up to 2020, a 2017+ file's materials were not found at all (their id field sits where the older layout has none), so
/// no color came across either: now the names and colors do, and every face finds its material.
#[test]
fn a_2020_files_materials_are_found_and_the_faces_use_them() {
    let m = import(&read("v20-model.skp"));
    let used: std::collections::BTreeSet<_> = face_materials(&m).into_iter().flatten().collect();
    assert!(!used.is_empty(), "the faces name their materials");
    for name in &used {
        assert!(m.materials.contains_key(name) || m.definitions.values().any(|d| d.model.materials.contains_key(name)), "{name} is not in the model");
    }
}

/// A real 2017-2020 file with pictures, when it is at hand (see the top of this file).
#[test]
fn a_real_2020_file_with_pictures_comes_across_textured() {
    let Some(path) = std::env::var_os("SKP_TEXTURED_2020") else { return };
    let m = import(&std::fs::read(path).expect("SKP_TEXTURED_2020 names a file"));
    let textured: Vec<_> = m.materials.iter().chain(m.definitions.values().flat_map(|d| d.model.materials.iter())).filter(|(_, mat)| mat.texture.is_some()).collect();
    assert!(textured.len() >= 20, "{} textured materials", textured.len());
    // faces painted with pictures: the file's 2,000 or so faces named one of its 39 materials, and every one was found
    let painted = face_materials(&m).into_iter().flatten().filter(|n| textured.iter().any(|(k, _)| *k == n)).count();
    assert!(painted > 1500, "{painted} faces carry a picture");
    for (_, mat) in &textured {
        let t = mat.texture.as_ref().unwrap();
        assert!(t.width > 0.0 && t.height > 0.0 && t.data.starts_with("data:image/"));
    }
}

/// A picture that cannot be read leaves the material as a color, and the import says which one.
#[test]
fn a_broken_picture_keeps_the_color_and_is_named() {
    let mut d = read("v16-textured.skp");
    // the picture after the material's name (the first one in the file is the model's thumbnail)
    let name: Vec<u8> = "UVTile".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let from = d.windows(name.len()).position(|w| w == &name[..]).expect("the material's name");
    let at = from + d[from..].windows(4).position(|w| w == [0x89, b'P', b'N', b'G']).expect("a picture in the file");
    // flip the picture's header: its bytes no longer decode
    for b in &mut d[at + 8..at + 32] {
        *b ^= 0xff;
    }
    sketchcraft::safety::take_skipped();
    let m = import(&d);
    let mat = material(&m, "UVTile").expect("the material is still there");
    assert!(mat.texture.is_none());
    let said = sketchcraft::safety::take_skipped();
    assert!(said.iter().any(|s| s.starts_with("UVTile:")), "{said:?}");
}
