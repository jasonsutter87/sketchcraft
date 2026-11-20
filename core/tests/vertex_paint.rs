//! Vertex painting (`vertex_paint`): strokes, fills, clearing, undo, save and load, the viewport scene, and the glTF and
//! FBX exports, on the texturing test set's globe and soccer ball.
use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Id, Model};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

struct Set {
    e: Engine,
    inst: Id,
    def: Id,
}

fn with(asset: &str) -> Set {
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":asset,"at":[0.0, 0.0, 0.0]}));
    Set { inst: r["instance"].as_u64().unwrap() as Id, def: r["component"].as_u64().unwrap() as Id, e }
}

const RED: [f64; 3] = [1.0, 0.0, 0.0];

impl Set {
    fn dm(&self) -> &Model {
        &self.e.model.definitions[&self.def].model
    }
    fn faces(&self) -> Vec<Id> {
        self.dm().faces.keys().copied().collect()
    }
    /// every corner of every face: (face, corner index, world point)
    fn corners(&self) -> Vec<(Id, usize, DVec3)> {
        let t = self.e.model.instances[&self.inst].transform;
        let mut v = vec![];
        for (&id, f) in &self.dm().faces {
            for (i, p) in self.dm().pts(&f.outer).into_iter().enumerate() {
                v.push((id, i, t.transform_point3(p)));
            }
        }
        v
    }
    fn colors(&mut self, ids: &[Id]) -> Value {
        run(&mut self.e, json!({"cmd":"vertex_colors","faces":ids}))["colors"].clone()
    }
    fn painted(&mut self) -> Vec<(Id, usize)> {
        let ids = self.faces();
        let c = self.colors(&ids);
        let mut out = vec![];
        for id in ids {
            if let Some(a) = c[id.to_string()].as_array() {
                for (i, rgba) in a.iter().enumerate() {
                    if rgba != &json!([1.0, 1.0, 1.0, 1.0]) {
                        out.push((id, i));
                    }
                }
            }
        }
        out
    }
}

fn pt(p: DVec3) -> Value {
    json!([p.x, p.y, p.z])
}

#[test]
fn a_stroke_changes_only_the_corners_within_its_radius() {
    let mut s = with("globe");
    assert!(s.painted().is_empty());
    let corners = s.corners();
    let centre = corners[corners.len() / 2].2;
    let radius = 0.3;
    let inst = s.inst;
    let r = run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[pt(centre)],"radius":radius,"falloff":"hard"}));
    assert!(r["corners"].as_u64().unwrap() > 0);
    let painted = s.painted();
    assert!(!painted.is_empty());
    let near: std::collections::HashSet<(Id, usize)> = corners.iter().filter(|c| c.2.distance(centre) <= radius).map(|c| (c.0, c.1)).collect();
    // exactly the corners inside the brush (a hard brush at full strength paints them solid red)
    let got: std::collections::HashSet<(Id, usize)> = painted.iter().copied().collect();
    assert_eq!(got, near);
    let ids = s.faces();
    let c = s.colors(&ids);
    for (f, i) in painted {
        assert_eq!(c[f.to_string()][i], json!([1.0, 0.0, 0.0, 1.0]));
    }
    assert!(near.len() < corners.len(), "the stroke is local");
}

#[test]
fn strength_and_smooth_falloff_blend_toward_the_colour() {
    let mut s = with("globe");
    let corners = s.corners();
    let centre = corners[0].2;
    let (f0, i0) = (corners[0].0, corners[0].1);
    let inst = s.inst;
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[pt(centre)],"radius":0.5,"strength":0.5}));
    let c = s.colors(&[f0]);
    // at the centre the weight is 1 x strength: halfway from white to red
    assert_eq!(c[f0.to_string()][i0], json!([1.0, 0.5, 0.5, 1.0]));
    // a second dab of the same strength goes three quarters of the way
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[pt(centre)],"radius":0.5,"strength":0.5}));
    let c = s.colors(&[f0]);
    assert_eq!(c[f0.to_string()][i0], json!([1.0, 0.25, 0.25, 1.0]));
}

#[test]
fn without_points_the_whole_selection_is_filled_and_clear_takes_it_off() {
    let mut s = with("soccer_ball");
    let inst = s.inst;
    let n = s.faces().len();
    let r = run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":[0.0, 0.0, 1.0]}));
    assert_eq!(r["faces"].as_u64().unwrap() as usize, n);
    let total: usize = s.dm().faces.values().map(|f| f.outer.len()).sum();
    assert_eq!(s.painted().len(), total);
    let r = run(&mut s.e, json!({"cmd":"vertex_paint_clear","entity":inst}));
    assert_eq!(r["cleared"].as_u64().unwrap() as usize, n);
    assert!(s.painted().is_empty());
    assert!(s.dm().faces.values().all(|f| f.vcol.is_none()));
    // erase paints back toward white, and a face that is white again keeps nothing
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED}));
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"erase":true}));
    assert!(s.dm().faces.values().all(|f| f.vcol.is_none()));
}

#[test]
fn each_stroke_is_one_undo_step() {
    let mut s = with("globe");
    let corners = s.corners();
    let inst = s.inst;
    let pts: Vec<Value> = corners.iter().take(40).map(|c| pt(c.2)).collect();
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":pts,"radius":0.2}));
    assert!(!s.painted().is_empty());
    run(&mut s.e, json!({"cmd":"undo"}));
    assert!(s.painted().is_empty(), "one undo takes the whole stroke back");
    run(&mut s.e, json!({"cmd":"redo"}));
    assert!(!s.painted().is_empty());
}

#[test]
fn refused_calls_change_nothing() {
    let mut s = with("globe");
    let inst = s.inst;
    let before = s.dm().faces.clone();
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","color":RED})).contains("faces or an entity"));
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","entity":inst})).contains("color"));
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":[1.0, 2.0]})).contains("color"));
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"strength":3.0})).contains("strength"));
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[[0,0,0]],"radius":0.0})).contains("radius"));
    assert!(fails(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"falloff":"spiky"})).contains("falloff"));
    assert_eq!(&before, &s.dm().faces);
}

#[test]
fn colours_survive_save_and_open_and_mark_the_file() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-vcol-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("globe.sketchcraft").to_string_lossy().to_string();
    let mut s = with("globe");
    let inst = s.inst;
    let plain: Value = serde_json::from_slice(&sketchcraft::format::to_vec(&s.e.model).unwrap()).unwrap();
    assert_eq!(plain["format"], 1);
    assert!(plain.get("needs").is_none());
    let c3 = pt(s.corners()[3].2);
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[c3.clone()],"radius":0.4}));
    run(&mut s.e, json!({"cmd":"save","path":path}));
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["format"], 2);
    assert_eq!(file["needs"], json!(["face_vcol"]));
    let mut e2 = Engine::new();
    run(&mut e2, json!({"cmd":"open","path":path}));
    assert_eq!(e2.model.definitions[&s.def].model.faces, s.dm().faces);
    assert!(s.dm().faces.values().any(|f| f.vcol.is_some()));
    // and the engine that read it is not read-only
    run(&mut e2, json!({"cmd":"vertex_paint_clear","entity":inst}));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn colours_that_no_longer_fit_the_face_are_ignored() {
    let mut s = with("globe");
    let inst = s.inst;
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED}));
    let def = s.def;
    let dm = std::sync::Arc::make_mut(&mut s.e.model.definitions.get_mut(&def).unwrap().model);
    let id = *dm.faces.keys().next().unwrap();
    dm.faces.get_mut(&id).unwrap().vcol.as_mut().unwrap().outer.pop();
    assert!(s.colors(&[id])[id.to_string()].is_null());
    // painting again starts that face from white
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED}));
    assert!(!s.colors(&[id])[id.to_string()].is_null());
}

#[test]
fn the_scene_carries_a_tint_per_vertex_only_when_something_is_painted() {
    let mut s = with("globe");
    let sc = sketchcraft::scene::build(&s.e.model, None);
    assert!(sc["vcolors"].as_array().unwrap().is_empty());
    let inst = s.inst;
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED}));
    let sc = sketchcraft::scene::build(&s.e.model, None);
    let v = sc["vcolors"].as_array().unwrap();
    assert_eq!(v.len(), sc["positions"].as_array().unwrap().len(), "one rgb tint per vertex");
    assert!(v.chunks(3).all(|c| c[0].as_f64() == Some(1.0) && c[1].as_f64() == Some(0.0)));
}

/// The glTF's COLOR_0 values per primitive, with the primitive's position count.
fn glb_colors(path: &str) -> Vec<(usize, Vec<[f32; 4]>)> {
    let b = std::fs::read(path).unwrap();
    let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let doc: Value = serde_json::from_slice(&b[20..20 + jl]).unwrap();
    let bin = &b[20 + jl + 8..];
    let mut out = vec![];
    for m in doc["meshes"].as_array().unwrap() {
        for pr in m["primitives"].as_array().unwrap() {
            let positions = doc["accessors"][pr["attributes"]["POSITION"].as_u64().unwrap() as usize]["count"].as_u64().unwrap() as usize;
            let Some(acc) = pr["attributes"]["COLOR_0"].as_u64() else {
                out.push((positions, vec![]));
                continue;
            };
            let a = &doc["accessors"][acc as usize];
            assert_eq!(a["type"], "VEC4");
            assert_eq!(a["componentType"], 5126);
            let v = &doc["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
            let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
            let mut cols = vec![];
            for i in 0..a["count"].as_u64().unwrap() as usize {
                let at = off + i * 16;
                let f = |k: usize| f32::from_le_bytes(bin[at + k * 4..at + k * 4 + 4].try_into().unwrap());
                cols.push([f(0), f(1), f(2), f(3)]);
            }
            out.push((positions, cols));
        }
    }
    out
}

#[test]
fn glb_carries_color_0_with_one_colour_per_vertex() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-vcol-glb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("globe.glb").to_string_lossy().to_string();
    let mut s = with("globe");
    run(&mut s.e, json!({"cmd":"export_glb","path":path}));
    assert!(glb_colors(&path).iter().all(|(_, c)| c.is_empty()), "no vertex colours, no COLOR_0");
    let inst = s.inst;
    let c3 = pt(s.corners()[3].2);
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[c3.clone()],"radius":0.1,"falloff":"hard"}));
    run(&mut s.e, json!({"cmd":"export_glb","path":path}));
    let prims = glb_colors(&path);
    let with: Vec<_> = prims.iter().filter(|(_, c)| !c.is_empty()).collect();
    assert!(!with.is_empty());
    let (mut red, mut white) = (0, 0);
    for (n, c) in &with {
        assert_eq!(c.len(), *n, "one colour per position");
        red += c.iter().filter(|c| **c == [1.0, 0.0, 0.0, 1.0]).count();
        white += c.iter().filter(|c| **c == [1.0; 4]).count();
    }
    // painted corners are red; the rest of the model has none or white
    let all: usize = prims.iter().map(|(n, _)| n).sum();
    assert!(red > 0 && red + white <= all && red < all, "{red} red, {white} white of {all}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn fbx_carries_a_colour_layer() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-vcol-fbx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (bin, asc) = (dir.join("g.fbx"), dir.join("g-ascii.fbx"));
    let mut s = with("globe");
    run(&mut s.e, json!({"cmd":"export_fbx","path":bin}));
    let has = |b: &[u8]| b.windows(17).any(|w| w == b"LayerElementColor");
    assert!(!has(&std::fs::read(&bin).unwrap()));
    let inst = s.inst;
    let c3 = pt(s.corners()[3].2);
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED,"points":[c3.clone()],"radius":0.1,"falloff":"hard"}));
    let r = run(&mut s.e, json!({"cmd":"export_fbx","path":bin}));
    assert!(has(&std::fs::read(&bin).unwrap()));
    // the ascii file spells the colours out: four numbers per triangle corner
    run(&mut s.e, json!({"cmd":"export_fbx","path":asc,"binary":false}));
    let text = std::fs::read_to_string(&asc).unwrap();
    let at = text.find("Colors: *").expect("a Colors array");
    let n: usize = text[at + 9..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
    assert_eq!(n, r["triangles"].as_u64().unwrap() as usize * 3 * 4);
    assert!(text.contains("ColorIndex: *"));
    // our importer still reads the file
    let mut back = Model::default();
    let imp = sketchcraft::fbx::import(&mut back, &std::fs::read(&bin).unwrap(), "Back", DVec3::ZERO).unwrap();
    assert_eq!(imp.meshes, r["meshes"].as_u64().unwrap() as usize);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_headless_render_shows_vertex_colours() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-vcol-png-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    let mut s = with("globe");
    run(&mut s.e, json!({"cmd":"render","out":a,"width":160,"height":120}));
    let inst = s.inst;
    run(&mut s.e, json!({"cmd":"vertex_paint","entity":inst,"color":RED}));
    run(&mut s.e, json!({"cmd":"render","out":b,"width":160,"height":120}));
    let (ia, ib) = (image::open(&a).unwrap().to_rgb8(), image::open(&b).unwrap().to_rgb8());
    // a red-filled globe loses green and blue wherever it is drawn
    let sum = |i: &image::RgbImage, k: usize| i.pixels().map(|p| p.0[k] as u64).sum::<u64>();
    assert!(sum(&ib, 1) < sum(&ia, 1) && sum(&ib, 2) < sum(&ia, 2));
    assert!(sum(&ib, 0) >= sum(&ia, 0) * 9 / 10);
    std::fs::remove_dir_all(dir).ok();
}
