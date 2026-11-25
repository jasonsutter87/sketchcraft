//! OBJ export of textured models (#222): the .mtl and the images beside the .obj, relative paths, and a texture
//! coordinate for every corner of every textured triangle, including painted-through and projected pictures.
use serde_json::{json, Value};
use sketchcraft::texmap::{FaceMap, Mapping};
use sketchcraft::Engine;
use std::path::{Path, PathBuf};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-objtex-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn picture(seed: u8) -> String {
    use base64::Engine as _;
    let img = image::RgbImage::from_fn(8, 8, |x, y| image::Rgb([(x * 30) as u8, (y * 30) as u8, seed]));
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner()))
}

struct Tri {
    mtl: String,
    pos: [[f64; 3]; 3],
    uv: [[f64; 2]; 3],
}

/// every triangle of the obj: its material, corner positions and corner texture coordinates
fn tris(obj: &str) -> Vec<Tri> {
    let (mut v, mut vt, mut out, mut mtl) = (vec![], vec![], vec![], String::new());
    for l in obj.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        let n = |i: usize| w[i].parse::<f64>().unwrap();
        match w.first().copied() {
            Some("v") => v.push([n(1), n(2), n(3)]),
            Some("vt") => vt.push([n(1), n(2)]),
            Some("usemtl") => mtl = w[1].to_string(),
            Some("f") => {
                assert_eq!(w.len(), 4, "triangles only: {l}");
                let mut pos = [[0.0; 3]; 3];
                let mut uv = [[0.0; 2]; 3];
                for k in 0..3 {
                    let parts: Vec<&str> = w[k + 1].split('/').collect();
                    pos[k] = v[parts[0].parse::<usize>().unwrap() - 1];
                    assert!(!parts[1].is_empty(), "a textured triangle's corner has a vt: {l}");
                    uv[k] = vt[parts[1].parse::<usize>().unwrap() - 1];
                }
                out.push(Tri { mtl: mtl.clone(), pos, uv });
            }
            _ => {}
        }
    }
    out
}

/// mtl text -> (material, map_Kd path)
fn maps(mtl: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut cur = String::new();
    for l in mtl.lines() {
        if let Some(n) = l.strip_prefix("newmtl ") {
            cur = n.to_string();
        } else if let Some(p) = l.strip_prefix("map_Kd ") {
            out.push((cur.clone(), p.to_string()));
        }
    }
    out
}

/// the obj, its mtl, and every image the mtl names: all present beside it, by relative paths
fn check_files(dir: &Path, stem: &str) -> (String, Vec<(String, String)>) {
    let obj = std::fs::read_to_string(dir.join(format!("{stem}.obj"))).unwrap();
    let mtl_name = obj.lines().find_map(|l| l.strip_prefix("mtllib ")).expect("mtllib line");
    let mtl = std::fs::read_to_string(dir.join(mtl_name)).expect("the .mtl is beside the .obj");
    let m = maps(&mtl);
    assert!(!m.is_empty(), "the mtl names a texture");
    for (_, p) in &m {
        assert!(!p.starts_with('/') && !p.contains("..") && !p.contains('\\') && !p.contains(':'), "relative path: {p}");
        let bytes = std::fs::read(dir.join(p)).unwrap_or_else(|_| panic!("image {p} is beside the obj"));
        assert!(image::load_from_memory(&bytes).is_ok(), "{p} is a readable image");
    }
    (obj, m)
}

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
}

fn same(a: [f64; 3], q: glam::DVec3) -> bool {
    (q.x - a[0]).abs() < 1e-5 && (q.y - a[1]).abs() < 1e-5 && (q.z - a[2]).abs() < 1e-5
}

#[test]
fn a_textured_box_exports_obj_mtl_images_and_uvs_that_match_the_viewport() {
    let dir = scratch("box");
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick wall","data":picture(10),"width":2.0,"height":1.0}));
    run(&mut e, json!({"cmd":"texture","name":"Roof/tile","data":picture(200),"width":1.0,"height":1.0}));
    let f = run(&mut e, json!({"cmd":"rect","from":[-2,-2,0],"to":[2,2,0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance":3.0}));
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    run(&mut e, json!({"cmd":"paint","ids":ids,"material":"Brick wall"}));
    let top = e.model.world_faces().into_iter().find(|f| f.normal.z > 0.9).unwrap().face.unwrap();
    run(&mut e, json!({"cmd":"paint","ids":[top],"material":"Roof/tile"}));
    // a name with a space and a dot, as a person types them
    run(&mut e, json!({"cmd":"export_obj","path":dir.join("my house.v2.obj").to_str().unwrap()}));

    let (obj, m) = check_files(&dir, "my house.v2");
    assert_eq!(m.len(), 2, "one map_Kd per picture: {m:?}");
    let ts = tris(&obj);
    assert_eq!(ts.len(), 12);
    let wf = e.model.world_faces();
    for t in &ts {
        assert!(m.iter().any(|(n, _)| *n == t.mtl), "{} has a map_Kd", t.mtl);
        let face = wf.iter().find(|f| t.pos.iter().all(|p| f.outer.iter().any(|q| same(*p, *q)))).unwrap();
        let tex = sketchcraft::texmap::of_face(&e.model, face).unwrap().1;
        let map = FaceMap::of(tex, face);
        for k in 0..3 {
            assert!(close(t.uv[k], map.uv(glam::DVec3::from(t.pos[k]))), "uv of {:?}: {:?}", t.pos[k], t.uv[k]);
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn painted_through_pictures_export_their_continuous_uvs() {
    let dir = scratch("through");
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":picture(10),"width":2.0,"height":1.0}));
    let a = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[8,4,0]}))["face"].as_u64().unwrap();
    let b = run(&mut e, json!({"cmd":"rect","from":[8,0,0],"to":[8,4,6]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick","scope":"connected"}));
    run(&mut e, json!({"cmd":"export_obj","path":dir.join("strip.obj").to_str().unwrap()}));
    let (obj, _) = check_files(&dir, "strip");
    let ts = tris(&obj);
    let at = |p: [f64; 3]| -> Vec<[f64; 2]> { ts.iter().flat_map(|t| (0..3).filter(move |k| t.pos[*k] == p).map(move |k| t.uv[k])).collect() };
    for p in [[8.0, 0.0, 0.0], [8.0, 4.0, 0.0]] {
        let uvs = at(p);
        assert!(uvs.len() >= 2 && uvs.iter().all(|u| close(*u, uvs[0])), "continuous at {p:?}: {uvs:?}");
    }
    // not the wall's own restarted default
    let tex = e.model.materials["Brick"].texture.clone().unwrap();
    let wall = e.model.world_faces().into_iter().find(|f| f.face == Some(b as u32)).unwrap();
    let own = Mapping::of(&tex, wall.normal).uv(glam::DVec3::new(8.0, 0.0, 6.0));
    let exported = at([8.0, 0.0, 6.0]);
    assert!(!exported.is_empty() && !close(exported[0], own), "exported the painted-through placement, not the default");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn projection_mapped_pictures_export_their_corner_uvs() {
    let dir = scratch("proj");
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":"dice","at":[1.0, 2.0, 0.0]}));
    let inst = r["instance"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"texture","name":"Label","data":picture(90),"width":0.05,"height":0.05}));
    run(&mut e, json!({"cmd":"texture_project","entity":inst,"material":"Label","mode":"box"}));
    run(&mut e, json!({"cmd":"export_obj","path":dir.join("dice.obj").to_str().unwrap()}));
    let (obj, m) = check_files(&dir, "dice");
    assert_eq!(m.len(), 1);
    let ts = tris(&obj);
    assert_eq!(ts.len(), 12, "six faces, two triangles each");
    let wf = e.model.world_faces();
    for t in &ts {
        let face = wf.iter().find(|f| f.uv.is_some() && t.pos.iter().all(|p| f.outer.iter().any(|q| same(*p, *q)))).expect("a projected face");
        let (o, _) = face.uv.as_ref().unwrap();
        for k in 0..3 {
            let i = face.outer.iter().position(|q| same(t.pos[k], *q)).unwrap();
            assert!(close(t.uv[k], [o[i][0] as f64, o[i][1] as f64]), "projected uv: {:?} vs {:?}", t.uv[k], o[i]);
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_reply_names_the_files_and_a_picture_that_cant_be_written_fails_the_export() {
    let dir = scratch("reply");
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Brick","data":picture(10),"width":1.0,"height":1.0}));
    let a = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[2,2,0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Brick"}));
    let r = run(&mut e, json!({"cmd":"export_obj","path":dir.join("room.obj").to_str().unwrap()}));
    assert_eq!(r["mtl"], "room.mtl");
    assert_eq!(r["textures_dir"], "room_textures");
    assert_eq!(r["images"], 1);
    assert_eq!(r["skipped"], json!([]));
    // a plain model has no pictures folder to keep with it
    let mut plain = Engine::new();
    run(&mut plain, json!({"cmd":"rect","from":[0,0,0],"to":[2,2,0]}));
    let r = run(&mut plain, json!({"cmd":"export_obj","path":dir.join("plain.obj").to_str().unwrap()}));
    assert!(r["textures_dir"].is_null() && r["images"] == 0);
    assert!(!dir.join("plain_textures").exists());
    // the folder's name taken by a file: the export fails (it used to write an .obj whose pictures were missing)
    std::fs::write(dir.join("blocked_textures"), "in the way").unwrap();
    let out = e.exec_json(&json!({"cmd":"export_obj","path":dir.join("blocked.obj").to_str().unwrap()}).to_string());
    assert_eq!(out["ok"], false, "{out}");
    assert!(out["error"].as_str().unwrap_or_default().contains("textures folder"), "{out}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn two_materials_with_one_file_safe_name_keep_two_images() {
    let dir = scratch("names");
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"texture","name":"Wood a","data":picture(10),"width":1.0,"height":1.0}));
    run(&mut e, json!({"cmd":"texture","name":"Wood_a","data":picture(220),"width":1.0,"height":1.0}));
    let a = run(&mut e, json!({"cmd":"rect","from":[0,0,0],"to":[2,2,0]}))["face"].as_u64().unwrap();
    let b = run(&mut e, json!({"cmd":"rect","from":[5,0,0],"to":[7,2,0]}))["face"].as_u64().unwrap();
    run(&mut e, json!({"cmd":"paint","ids":[a],"material":"Wood a"}));
    run(&mut e, json!({"cmd":"paint","ids":[b],"material":"Wood_a"}));
    run(&mut e, json!({"cmd":"export_obj","path":dir.join("w.obj").to_str().unwrap()}));
    let (_, m) = check_files(&dir, "w");
    assert_eq!(m.len(), 2);
    assert_ne!(m[0].1, m[1].1);
    assert_ne!(std::fs::read(dir.join(&m[0].1)).unwrap(), std::fs::read(dir.join(&m[1].1)).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}
