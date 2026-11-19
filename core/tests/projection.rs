//! Projection mapping (`texture_project`): each projection applied to the texturing test set asset it is meant for,
//! undo, save and load, and the glTF export.
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

/// An engine holding one test-set asset as a placed component, with a picture material ready (`tile` metres square).
fn with(asset: &str, tile: f64) -> Set {
    use base64::Engine as _;
    let mut e = Engine::new();
    let r = run(&mut e, json!({"cmd":"insert","asset":asset,"at":[0.0, 0.0, 0.0]}));
    let (inst, def) = (r["instance"].as_u64().unwrap() as Id, r["component"].as_u64().unwrap() as Id);
    let img = image::RgbImage::from_fn(8, 8, |x, y| image::Rgb([(x * 30) as u8, (y * 30) as u8, 90]));
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner()));
    run(&mut e, json!({"cmd":"texture","name":"Label","data":data,"width":tile,"height":tile}));
    Set { e, inst, def }
}

impl Set {
    fn dm(&self) -> &Model {
        &self.e.model.definitions[&self.def].model
    }
    /// open the component for editing, as a person does to pick faces inside it
    fn open(&mut self) {
        run(&mut self.e, json!({"cmd":"edit","id":self.inst}));
    }
    fn group(&self, name: &str) -> Id {
        *self.dm().groups.iter().find(|(_, g)| g.name == name).unwrap_or_else(|| panic!("no group {name}")).0
    }
    /// every face of a group (and its child groups)
    fn faces(&self, g: Id) -> Vec<Id> {
        self.dm().faces_of(&[g]).unwrap()
    }
    fn all_faces(&self) -> Vec<Id> {
        self.dm().faces.keys().copied().collect()
    }
    fn uv(&mut self, face: Id) -> Vec<[f64; 2]> {
        let r = run(&mut self.e, json!({"cmd":"texture_uv","faces":[face]}));
        r["uv"][face.to_string()].as_array().unwrap_or_else(|| panic!("face {face} has no uv")).iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect()
    }
    fn project(&mut self, target: Value, extra: Value) -> Value {
        let mut c = json!({"cmd":"texture_project","material":"Label"});
        for (k, v) in target.as_object().unwrap().iter().chain(extra.as_object().unwrap()) {
            c[k] = v.clone();
        }
        run(&mut self.e, c)
    }
}

fn span(p: &[[f64; 2]], k: usize) -> (f64, f64) {
    p.iter().fold((9.0f64, -9.0f64), |a, q| (a.0.min(q[k]), a.1.max(q[k])))
}

#[test]
fn box_on_the_dice_gives_every_face_one_whole_picture() {
    let mut s = with("dice", 0.05);
    let inst = s.inst;
    // the placed component, projected from outside, without opening it
    let r = s.project(json!({"entity": inst}), json!({"mode":"box"}));
    assert_eq!(r["faces"], 6);
    for f in s.all_faces() {
        let p = s.uv(f);
        assert_eq!(p.len(), 4);
        let (u0, u1) = span(&p, 0);
        let (v0, v1) = span(&p, 1);
        // a 5 cm face on a 5 cm tile is exactly one tile, centred on the dice's middle: 0..1
        assert!((u1 - u0 - 1.0).abs() < 1e-5 && (v1 - v0 - 1.0).abs() < 1e-5, "face {f}: {p:?}");
        assert!(u0.abs() < 1e-5 && v0.abs() < 1e-5, "{p:?}");
    }
    assert_eq!(s.dm().faces.values().filter(|f| f.material.as_deref() == Some("Label")).count(), 6, "painted too");
}

#[test]
fn box_on_the_crate_covers_every_box() {
    let mut s = with("crate", 0.3);
    s.open();
    let g = s.group("Crate");
    let all = s.faces(g);
    assert!(all.len() > 100);
    let r = s.project(json!({"entity": g}), json!({"mode":"box"}));
    assert_eq!(r["faces"].as_u64().unwrap() as usize, all.len());
    for f in all {
        assert_eq!(s.uv(f).len(), s.dm().faces[&f].outer.len());
    }
}

#[test]
fn planar_on_a_crate_side_is_one_flat_picture_across_the_planks() {
    let mut s = with("crate", 1.0);
    s.open();
    let g = s.group("Crate");
    let front: Vec<Id> = s.faces(g).into_iter().filter(|&f| s.dm().normal(f).y < -0.99).collect();
    assert!(front.len() >= 5, "a side of planks: {}", front.len());
    // project along +y (onto the faces that look toward -y), the whole side fitted once
    s.project(json!({"faces": front}), json!({"mode":"planar","axis":[0,1,0],"fit":true}));
    let (mut all, mut seen) = (vec![], std::collections::HashMap::<(i64, i64), [f64; 2]>::new());
    for &f in &front {
        let pts = s.dm().pts(&s.dm().faces[&f].outer);
        let uv = s.uv(f);
        for (p, q) in pts.iter().zip(&uv) {
            // planks side by side share one continuous picture: a shared corner has one coordinate
            let key = ((p.x * 1e5).round() as i64, (p.z * 1e5).round() as i64);
            if let Some(prev) = seen.insert(key, *q) {
                assert!((prev[0] - q[0]).abs() < 1e-5 && (prev[1] - q[1]).abs() < 1e-5, "{key:?}: {prev:?} vs {q:?}");
            }
        }
        all.extend(uv);
    }
    let ((u0, u1), (v0, v1)) = (span(&all, 0), span(&all, 1));
    assert!(u0.abs() < 1e-5 && (u1 - 1.0).abs() < 1e-5 && v0.abs() < 1e-5 && (v1 - 1.0).abs() < 1e-5, "{u0} {u1} {v0} {v1}");
}

#[test]
fn cylinder_on_the_can_wraps_once_and_keeps_the_caps_flat() {
    let mut s = with("can", 1.0);
    s.open();
    let g = s.group("Can");
    let all = s.faces(g);
    let r = s.project(json!({"entity": g}), json!({"mode":"cylinder"}));
    assert_eq!(r["faces"].as_u64().unwrap() as usize, all.len());
    let (sides, caps): (Vec<Id>, Vec<Id>) = all.iter().partition(|&&f| s.dm().normal(f).z.abs() < 0.5);
    assert_eq!(sides.len(), 32);
    assert_eq!(caps.len(), 2);
    let mut by_corner: std::collections::HashMap<(i64, i64, i64), Vec<f64>> = Default::default();
    let mut seams = 0;
    for &f in &sides {
        let pts = s.dm().pts(&s.dm().faces[&f].outer);
        let p = s.uv(f);
        assert_eq!(p.len(), pts.len());
        for (q, c) in pts.iter().zip(&p) {
            // v spans the can's height: 0 at the bottom, 1 at the top
            assert!((c[1] - if q.z > 0.06 { 1.0 } else { 0.0 }).abs() < 1e-5, "{c:?} at z {}", q.z);
            by_corner.entry(((q.x * 1e6).round() as i64, (q.y * 1e6).round() as i64, (q.z * 1e6).round() as i64)).or_default().push(c[0]);
        }
        let (lo, hi) = span(&p, 0);
        assert!(hi - lo < 0.1, "a face covers about 1/32 of the way round: {lo}..{hi}");
        if hi > 1.0 || lo < 0.0 {
            seams += 1;
        }
    }
    // continuity: a corner shared by two side faces has the same u (mod 1) in both; the only jump is a whole picture
    for (k, us) in &by_corner {
        assert_eq!(us.len(), 2, "{k:?}");
        let d = us[0] - us[1];
        assert!((d - d.round()).abs() < 1e-5, "{k:?}: {us:?}");
    }
    assert!(seams <= 1, "at most one face crosses the seam: {seams}");
    // the caps are planar discs: radius 0.5 around the middle of the picture
    for &f in &caps {
        for c in s.uv(f) {
            let d = ((c[0] - 0.5).powi(2) + (c[1] - 0.5).powi(2)).sqrt();
            assert!((d - 0.5).abs() < 1e-4, "{d}");
        }
    }
}

#[test]
fn cylinder_rotation_moves_the_seam() {
    let mut s = with("can", 1.0);
    s.open();
    let g = s.group("Can");
    let side = s.faces(g).into_iter().find(|&f| s.dm().normal(f).z.abs() < 0.5).unwrap();
    s.project(json!({"entity": g}), json!({"mode":"cylinder"}));
    let a = s.uv(side)[0][0];
    s.project(json!({"entity": g}), json!({"mode":"cylinder","rotation":90.0}));
    let b = s.uv(side)[0][0];
    assert!(((b - a).rem_euclid(1.0) - 0.25).abs() < 1e-5, "{a} {b}");
}

#[test]
fn sphere_on_the_globe_and_the_soccer_ball_stays_in_range() {
    for (asset, name) in [("globe", Some("Sphere")), ("soccer_ball", None)] {
        let mut s = with(asset, 1.0);
        s.open();
        let all: Vec<Id> = match name {
            Some(n) => s.faces(s.group(n)),
            None => s.all_faces(),
        };
        assert!(all.len() >= 32, "{asset}");
        s.project(json!({"faces": all}), json!({"mode":"sphere"}));
        let mut crossing = 0;
        for &f in &all {
            let p = s.uv(f);
            assert_eq!(p.len(), s.dm().faces[&f].outer.len(), "{asset}");
            for c in &p {
                assert!(c[1] >= -1e-5 && c[1] <= 1.0 + 1e-5, "{asset}: v out of range {c:?}");
                // u is in 0..1 except on a face that straddles the seam, which runs a little past it
                assert!(s.dm().normal(f).z.abs() > 0.99 || (c[0] >= -0.2 && c[0] <= 1.2), "{asset}: {c:?}");
            }
            let (lo, hi) = span(&p, 0);
            // (a face with the pole inside it, like the ball's top and bottom hexagons, has to go all the way round)
            let on_the_pole = s.dm().normal(f).z.abs() > 0.99;
            assert!(on_the_pole || hi - lo < 0.5, "{asset}: a face never smears across the picture: {p:?}");
            if hi > 1.0 || lo < 0.0 {
                crossing += 1;
            }
        }
        assert!(crossing < all.len() / 4, "{asset}: only the faces on the seam cross it");
    }
}

fn snapshot(e: &Engine) -> Value {
    serde_json::to_value(&e.model).unwrap()
}

#[test]
fn it_is_one_undo_step_and_clear_goes_back() {
    let mut s = with("can", 1.0);
    let inst = s.inst;
    let before = snapshot(&s.e);
    s.project(json!({"entity": inst}), json!({"mode":"cylinder"}));
    assert!(s.dm().faces.values().any(|f| f.uv.is_some()));
    run(&mut s.e, json!({"cmd":"undo"}));
    assert_eq!(snapshot(&s.e), before, "one undo undoes the paint and the projection together");
    run(&mut s.e, json!({"cmd":"redo"}));
    assert!(s.dm().faces.values().any(|f| f.uv.is_some()));
    let r = run(&mut s.e, json!({"cmd":"texture_project","entity":inst,"mode":"clear"}));
    assert_eq!(r["cleared"], 34);
    assert!(s.dm().faces.values().all(|f| f.uv.is_none()));
}

#[test]
fn a_refusal_leaves_the_model_alone() {
    let mut s = with("dice", 0.05);
    let inst = s.inst;
    let before = snapshot(&s.e);
    assert!(fails(&mut s.e, json!({"cmd":"texture_project","entity":inst,"mode":"cone"})).contains("unknown projection"));
    assert!(fails(&mut s.e, json!({"cmd":"texture_project","entity":inst,"mode":"box","material":"Nope"})).contains("no material"));
    assert!(fails(&mut s.e, json!({"cmd":"texture_project","entity":inst,"mode":"box","scale":0.0})).contains("scale"));
    assert!(fails(&mut s.e, json!({"cmd":"texture_project","entity":inst,"material":"Label","mode":"cylinder","axis":[0,0,0]})).contains("axis"));
    fails(&mut s.e, json!({"cmd":"texture_project","mode":"box"}));
    assert_eq!(snapshot(&s.e), before);
}

#[test]
fn coordinates_survive_save_and_open_and_mark_the_file() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-proj-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("can.sketchcraft").to_string_lossy().to_string();
    let mut s = with("can", 1.0);
    let inst = s.inst;
    // a model with no projection is still format 1: older builds open it as before
    let plain: Value = serde_json::from_slice(&sketchcraft::format::to_vec(&s.e.model).unwrap()).unwrap();
    assert_eq!(plain["format"], 1);
    assert!(plain.get("needs").is_none());
    s.project(json!({"entity": inst}), json!({"mode":"cylinder"}));
    run(&mut s.e, json!({"cmd":"save","path":path}));
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["format"], 2);
    assert_eq!(file["needs"], json!(["face_uv"]));
    let mut e2 = Engine::new();
    run(&mut e2, json!({"cmd":"open","path":path}));
    let (a, b) = (&e2.model.definitions[&s.def].model.faces, &s.dm().faces);
    assert_eq!(a, b);
    assert!(a.values().any(|f| f.uv.is_some()));
    // and the engine that read it is not read-only
    run(&mut e2, json!({"cmd":"texture_project","entity":inst,"mode":"clear"}));
    std::fs::remove_dir_all(dir).ok();
}

/// The glTF's TEXCOORD_0 values: (u, v) pairs, v down the picture.
fn glb_uvs(path: &str) -> Vec<[f32; 2]> {
    let b = std::fs::read(path).unwrap();
    let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let doc: Value = serde_json::from_slice(&b[20..20 + jl]).unwrap();
    let bin = &b[20 + jl + 8..];
    let mut out = vec![];
    for m in doc["meshes"].as_array().unwrap() {
        for pr in m["primitives"].as_array().unwrap() {
            let Some(acc) = pr["attributes"]["TEXCOORD_0"].as_u64() else { continue };
            let a = &doc["accessors"][acc as usize];
            let v = &doc["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
            let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
            for i in 0..a["count"].as_u64().unwrap() as usize {
                let at = off + i * 8;
                out.push([f32::from_le_bytes(bin[at..at + 4].try_into().unwrap()), f32::from_le_bytes(bin[at + 4..at + 8].try_into().unwrap())]);
            }
        }
    }
    out
}

#[test]
fn the_glb_carries_the_projected_coordinates() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-projglb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("can.glb").to_string_lossy().to_string();
    let mut s = with("can", 1.0);
    let inst = s.inst;
    s.project(json!({"entity": inst}), json!({"mode":"cylinder"}));
    run(&mut s.e, json!({"cmd":"export_glb","path":path}));
    let tex = glb_uvs(&path);
    assert!(!tex.is_empty());
    // every corner the model holds, as (u, 1 - v), appears in the file, and every file coordinate is one of them
    let mut want: Vec<[f32; 2]> = vec![];
    for f in s.all_faces() {
        for c in s.uv(f) {
            want.push([c[0] as f32, 1.0 - c[1] as f32]);
        }
    }
    let has = |hay: &[[f32; 2]], p: &[f32; 2]| hay.iter().any(|q| (q[0] - p[0]).abs() < 1e-4 && (q[1] - p[1]).abs() < 1e-4);
    assert!(want.iter().all(|p| has(&tex, p)));
    assert!(tex.iter().all(|p| has(&want, p)), "no planar coordinates left over");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_changed_outline_falls_back_to_the_materials_placement() {
    let mut s = with("dice", 0.05);
    let inst = s.inst;
    s.project(json!({"entity": inst}), json!({"mode":"box"}));
    let f = s.all_faces()[0];
    // pretend the loop changed size: stale coordinates are ignored rather than misread
    let def = s.def;
    let face = std::sync::Arc::make_mut(&mut s.e.model.definitions.get_mut(&def).unwrap().model).faces.get_mut(&f).unwrap();
    face.uv.as_mut().unwrap().outer.pop();
    assert!(!face.uv.as_ref().unwrap().fits(face));
    let scene = sketchcraft::scene::build(&s.e.model, None);
    assert!(scene["uvs"].as_array().is_some_and(|u| !u.is_empty()));
}
