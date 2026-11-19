//! FBX export tests: structure of the file, and a round trip through our own importer.
use super::*;
use crate::fbx::{parse, Node, Prop};
use crate::Engine;
use serde_json::{json, Value};

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-fbxout-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn box_group(e: &mut Engine, from: [f64; 3], to: [f64; 3], name: &str) -> u64 {
    let g = run(e, json!({"cmd":"begin_group","name":name}))["group"].as_u64().unwrap();
    let f = run(e, json!({"cmd":"rect","from":from,"to":[to[0], to[1], from[2]]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    let h = to[2] - from[2];
    run(e, json!({"cmd":"pushpull","face":f,"distance": if up { h } else { -h }}));
    run(e, json!({"cmd":"end_group"}));
    g
}

/// Model nodes: (id, name, class)
fn models(root: &Node) -> Vec<(i64, String, String)> {
    root.get("Objects")
        .unwrap()
        .all("Model")
        .map(|m| {
            let nm = m.props[1].str().split('\0').next().unwrap().to_string();
            (m.props[0].int().unwrap(), nm, m.props[2].str().to_string())
        })
        .collect()
}

/// the parent a Model is connected to ("OO", child first)
fn parent_of(root: &Node, child: i64) -> Option<i64> {
    root.get("Connections").unwrap().all("C").find(|c| c.props[0].str() == "OO" && c.props[1].int() == Some(child)).and_then(|c| c.props[2].int())
}

fn floats(n: &Node, key: &str) -> usize {
    match n.get(key).and_then(|k| k.props.first()) {
        Some(Prop::Floats(v)) => v.len(),
        _ => 0,
    }
}

#[test]
fn a_textured_box_carries_uvs_a_material_and_its_picture() {
    let dir = scratch("tex");
    let mut e = Engine::new();
    let g = box_group(&mut e, [0.0, 0.0, 0.0], [2.0, 1.0, 1.0], "Crate");
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":PNG,"width":0.5,"height":0.5,"color":[0.6,0.3,0.2]}));
    run(&mut e, json!({"cmd":"paint","ids":[g],"material":"Brick","scope":"object"}));
    let path = dir.join("box.fbx");
    let r = run(&mut e, json!({"cmd":"export_fbx","path":path}));
    assert_eq!(r["meshes"], 1);
    assert_eq!(r["triangles"], 12);
    let bytes = std::fs::read(&path).unwrap();
    // header, version and footer
    assert_eq!(&bytes[..21], b"Kaydara FBX Binary  \0");
    assert_eq!(u32::from_le_bytes(bytes[23..27].try_into().unwrap()), 7400);
    assert!(bytes.ends_with(&[0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b]));
    let root = parse(&bytes).unwrap();
    let gs = root.get("GlobalSettings").unwrap().p70();
    assert_eq!(gs["UpAxis"][0].int(), Some(1));
    assert_eq!(gs["UnitScaleFactor"][0].num(), Some(100.0));
    let objects = root.get("Objects").unwrap();
    let geo = objects.get("Geometry").unwrap();
    assert_eq!(floats(geo, "Vertices"), 8 * 3, "a box shares its 8 corners");
    assert_eq!(floats(geo.get("LayerElementUV").unwrap(), "UV"), 12 * 3 * 2, "one UV per corner of every triangle");
    assert_eq!(floats(geo.get("LayerElementNormal").unwrap(), "Normals"), 12 * 3 * 3);
    assert!(objects.get("Material").unwrap().props[1].str().starts_with("Brick\0"));
    // the picture sits beside the file, referenced by a relative path
    let rel = objects.get("Texture").unwrap().get("RelativeFilename").unwrap().props[0].str().to_string();
    assert_eq!(rel, "box_textures/Brick.png");
    assert!(dir.join(&rel).metadata().unwrap().len() > 0);
    let here = dir.to_string_lossy().to_string();
    assert!(!bytes.windows(here.len()).any(|w| w == here.as_bytes()), "no absolute path in the file");
    // our own importer reads it back: size
    let mut back = crate::model::Model::default();
    let imp = crate::fbx::import(&mut back, &bytes, "Back", DVec3::ZERO).unwrap();
    assert_eq!(imp.meshes, 1);
    assert!(imp.faces >= 6 && imp.faces <= 12, "{}", imp.faces);
    let (lo, hi) = back.world_bounds().unwrap();
    let sz = hi - lo;
    assert!((sz.x - 2.0).abs() < 1e-4 && (sz.y - 1.0).abs() < 1e-4 && (sz.z - 1.0).abs() < 1e-4, "{sz:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn groups_and_components_keep_their_names_and_nesting() {
    let dir = scratch("tree");
    let mut e = Engine::new();
    let wall = box_group(&mut e, [0.0, 0.0, 0.0], [4.0, 0.2, 3.0], "Wall") as u32;
    let floor = box_group(&mut e, [0.0, 0.0, -0.2], [4.0, 4.0, 0.0], "Floor") as u32;
    let door = run(&mut e, json!({"cmd":"insert","asset":"door","at":[1.0, 0.0, 0.0]}))["instance"].as_u64().unwrap() as u32;
    let room = run(&mut e, json!({"cmd":"group","ids":[wall, floor, door],"name":"Room"}))["group"].as_u64().unwrap() as u32;
    run(&mut e, json!({"cmd":"group","ids":[room],"name":"House"}));
    let path = dir.join("tree.fbx");
    let r = run(&mut e, json!({"cmd":"export_fbx","path":path}));
    let root = parse(&std::fs::read(&path).unwrap()).unwrap();
    let ms = models(&root);
    let by = |name: &str| ms.iter().find(|m| m.1 == name).unwrap_or_else(|| panic!("no model {name} in {ms:?}")).clone();
    let (house, room, wall, floor) = (by("House"), by("Room"), by("Wall"), by("Floor"));
    assert_eq!(house.2, "Null", "a group with nothing of its own is an empty node");
    assert_eq!(wall.2, "Mesh");
    assert_eq!(parent_of(&root, house.0), Some(0));
    assert_eq!(parent_of(&root, room.0), Some(house.0));
    assert_eq!(parent_of(&root, wall.0), Some(room.0));
    assert_eq!(parent_of(&root, floor.0), Some(room.0));
    let door_name = e.model.instances.values().next().unwrap().name.clone();
    assert_eq!(parent_of(&root, by(&door_name).0), Some(room.0), "the placement sits in its group");
    let meshes = r["meshes"].as_u64().unwrap() as usize;
    assert_eq!(meshes, ms.iter().filter(|m| m.2 == "Mesh").count());
    assert_eq!(root.get("Objects").unwrap().all("Geometry").count(), meshes);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_texturing_set_round_trips_through_our_importer() {
    let dir = scratch("set");
    let mut e = Engine::new();
    for (i, key) in ["dice", "can", "globe", "crate", "soccer_ball", "beaver"].iter().enumerate() {
        run(&mut e, json!({"cmd":"insert","asset":key,"at":[i as f64, 0.0, 0.0]}));
    }
    let path = dir.join("set.fbx");
    let r = run(&mut e, json!({"cmd":"export_fbx","path":path}));
    let (meshes, tris) = (r["meshes"].as_u64().unwrap() as usize, r["triangles"].as_u64().unwrap() as usize);
    assert!(meshes >= 6 && tris > 500, "{meshes} {tris}");
    let bytes = std::fs::read(&path).unwrap();
    let root = parse(&bytes).unwrap();
    assert_eq!(models(&root).iter().filter(|m| m.2 == "Mesh").count(), meshes);
    let mut back = crate::model::Model::default();
    let imp = crate::fbx::import(&mut back, &bytes, "Set", DVec3::ZERO).unwrap();
    assert_eq!(imp.meshes, meshes);
    assert!(imp.faces > 0 && imp.faces <= tris);
    let (a, b) = (e.model.world_bounds().unwrap(), back.world_bounds().unwrap());
    assert!((a.0 - b.0).length() < 1e-3 && (a.1 - b.1).length() < 1e-3, "{a:?} vs {b:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn ascii_is_well_formed() {
    let dir = scratch("ascii");
    let mut e = Engine::new();
    box_group(&mut e, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], "Cube");
    let path = dir.join("a.fbx");
    let r = run(&mut e, json!({"cmd":"export_fbx","path":path,"binary":false}));
    assert_eq!(r["binary"], false);
    let t = std::fs::read_to_string(&path).unwrap();
    assert!(t.starts_with("; FBX 7.4.0 project file"));
    assert_eq!(t.matches('{').count(), t.matches('}').count(), "braces balance");
    assert!(t.contains("\"Model::Cube\", \"Mesh\""));
    // every array says how long it is, and holds that many values
    let lines: Vec<&str> = t.lines().collect();
    let mut arrays = 0;
    for (i, l) in lines.iter().enumerate() {
        if let Some((_, rest)) = l.rsplit_once(" *") {
            let count: usize = rest.trim_end_matches(" {").parse().unwrap();
            let vals = lines[i + 1].trim().strip_prefix("a: ").unwrap();
            assert_eq!(vals.split(',').count(), count, "{l}");
            arrays += 1;
        }
    }
    assert!(arrays >= 3);
    // our importer says plainly that it reads binary only
    let mut m = crate::model::Model::default();
    assert!(crate::fbx::import(&mut m, t.as_bytes(), "x", DVec3::ZERO).is_err());
    std::fs::remove_dir_all(dir).ok();
}
