//! FBX import against a tiny hand-built binary file: one quad, Y up, in
//! centimeters, red, moved 100 cm up by its model.

use serde_json::json;
use sketchcraft::Engine;

fn s(v: &str) -> Vec<u8> {
    let mut o = vec![b'S'];
    o.extend((v.len() as u32).to_le_bytes());
    o.extend(v.as_bytes());
    o
}
fn l(v: i64) -> Vec<u8> {
    let mut o = vec![b'L'];
    o.extend(v.to_le_bytes());
    o
}
fn d(v: f64) -> Vec<u8> {
    let mut o = vec![b'D'];
    o.extend(v.to_le_bytes());
    o
}
fn darr(v: &[f64]) -> Vec<u8> {
    let mut o = vec![b'd'];
    o.extend((v.len() as u32).to_le_bytes());
    o.extend(0u32.to_le_bytes());
    o.extend(((v.len() * 8) as u32).to_le_bytes());
    for x in v {
        o.extend(x.to_le_bytes());
    }
    o
}
fn iarr(v: &[i32]) -> Vec<u8> {
    let mut o = vec![b'i'];
    o.extend((v.len() as u32).to_le_bytes());
    o.extend(0u32.to_le_bytes());
    o.extend(((v.len() * 4) as u32).to_le_bytes());
    for x in v {
        o.extend(x.to_le_bytes());
    }
    o
}

/// A node at absolute offset `at` (7.4 layout: u32 header fields).
fn node(at: usize, name: &str, props: &[Vec<u8>], kids: &[&dyn Fn(usize) -> Vec<u8>]) -> Vec<u8> {
    let plist: Vec<u8> = props.concat();
    let head = 13 + name.len() + plist.len();
    let mut body = Vec::new();
    for k in kids {
        body.extend(k(at + head + body.len()));
    }
    if !kids.is_empty() {
        body.extend([0u8; 13]);
    }
    let end = at + head + body.len();
    let mut o = Vec::new();
    o.extend((end as u32).to_le_bytes());
    o.extend((props.len() as u32).to_le_bytes());
    o.extend((plist.len() as u32).to_le_bytes());
    o.push(name.len() as u8);
    o.extend(name.as_bytes());
    o.extend(plist);
    o.extend(body);
    o
}

fn tiny_fbx() -> Vec<u8> {
    let mut f = b"Kaydara FBX Binary  \0\x1a\x00".to_vec();
    f.extend(7400u32.to_le_bytes());
    let p = |n: &'static str, kind: &'static str, vals: Vec<Vec<u8>>| {
        move |at: usize| {
            let mut props = vec![s(n), s(kind), s(""), s("A")];
            props.extend(vals.clone());
            node(at, "P", &props, &[])
        }
    };
    let gs = |at: usize| node(at, "GlobalSettings", &[], &[&|at| node(at, "Properties70", &[], &[&p("UpAxis", "int", vec![{ let mut o = vec![b'I']; o.extend(1i32.to_le_bytes()); o }]), &p("UnitScaleFactor", "double", vec![d(1.0)])])]);
    let geometry = |at: usize| {
        node(
            at,
            "Geometry",
            &[l(10), s("Quad\0\x01Geometry"), s("Mesh")],
            &[
                &|at| node(at, "Vertices", &[darr(&[0.0, 0.0, 0.0, 200.0, 0.0, 0.0, 200.0, 0.0, -100.0, 0.0, 0.0, -100.0])], &[]),
                &|at| node(at, "PolygonVertexIndex", &[iarr(&[0, 1, 2, -4])], &[]),
            ],
        )
    };
    let model = |at: usize| node(at, "Model", &[l(20), s("Floor\0\x01Model"), s("Mesh")], &[&|at| node(at, "Properties70", &[], &[&p("Lcl Translation", "Lcl Translation", vec![d(0.0), d(100.0), d(0.0)])])]);
    let material = |at: usize| node(at, "Material", &[l(30), s("Red\0\x01Material"), s("")], &[&|at| node(at, "Properties70", &[], &[&p("DiffuseColor", "Color", vec![d(1.0), d(0.0), d(0.0)])])]);
    let objects = |at: usize| node(at, "Objects", &[], &[&geometry, &model, &material]);
    let c = |a: i64, b: i64| move |at: usize| node(at, "C", &[s("OO"), l(a), l(b)], &[]);
    let conns = |at: usize| node(at, "Connections", &[], &[&c(10, 20), &c(20, 0), &c(30, 20)]);
    let at = f.len();
    f.extend(gs(at));
    let at = f.len();
    f.extend(objects(at));
    let at = f.len();
    f.extend(conns(at));
    f.extend([0u8; 13]);
    f
}

#[test]
fn imports_a_hand_built_binary_fbx() {
    use base64::Engine as _;
    let mut e = Engine::new();
    let data = base64::engine::general_purpose::STANDARD.encode(tiny_fbx());
    let r = e.exec_json(&json!({"cmd":"import_fbx","data":data,"name":"Tiny"}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["meshes"], 1);
    assert_eq!(r["faces"], 1);
    let faces = e.model.world_faces();
    assert_eq!(faces.len(), 1);
    // 2 m x 1 m in meters, lying flat (Y up -> Z up), lifted 1 m
    let area = sketchcraft::geom::area(&faces[0].outer);
    assert!((area - 2.0).abs() < 1e-9, "{area}");
    assert!(faces[0].outer.iter().all(|p| (p.z - 1.0).abs() < 1e-9), "{:?}", faces[0].outer);
    assert!(faces[0].outer.iter().any(|p| (p.y - 1.0).abs() < 1e-9), "z -100 cm -> y +1 m");
    assert_eq!(faces[0].color, Some([1.0, 0.0, 0.0]));
}

#[test]
fn rejects_text_fbx_with_a_hint() {
    use base64::Engine as _;
    let mut e = Engine::new();
    let data = base64::engine::general_purpose::STANDARD.encode(b"; FBX 7.4.0 project file\nFBXHeaderExtension: {\n}\n");
    let r = e.exec_json(&json!({"cmd":"import_fbx","data":data}).to_string());
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("binary"));
}

#[path = "fuzzlib.rs"]
mod fuzzlib;

#[test]
#[ignore]
fn fuzz_fbx() {
    let seed = tiny_fbx();
    let (p, ms) = fuzzlib::fuzz("fbx", &seed, false, 20000, |d| {
        let mut m = sketchcraft::model::Model::default();
        let _ = sketchcraft::fbx::import(&mut m, d, "f", glam::DVec3::ZERO);
    });
    println!("fbx: {p} panics, slowest {ms} ms");
}

#[test]
fn a_node_pointing_backwards_is_refused_not_looped() {
    // found by fuzzing: a 1 KB file whose node length points backwards made
    // the reader loop over the same bytes until the process ran out of memory
    let d = std::fs::read("tests/data/fbx-backward-node.bin").unwrap();
    let mut m = sketchcraft::model::Model::default();
    let t = std::time::Instant::now();
    assert!(sketchcraft::fbx::import(&mut m, &d, "f", glam::DVec3::ZERO).is_err());
    assert!(t.elapsed().as_secs() < 2);
}
