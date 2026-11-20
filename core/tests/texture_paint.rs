//! Texture painting's engine side: `texture_paint_new` (unwrap, base colour, baking the present material), strokes in 3D
//! (across seams) and on the picture, erase, undo, save and load, glTF export, and the refusals. Tests use the texturing
//! test set (docs/dev/texturing-test-set.md).
use serde_json::{json, Value};
use sketchcraft::model::Id;
use sketchcraft::texpaint::Canvas;
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

fn with(key: &str) -> Engine {
    let a = sketchcraft::assets::find(key).unwrap_or_else(|| panic!("no asset {key}"));
    let mut e = Engine::new();
    e.model = sketchcraft::assets::build(a).unwrap();
    e
}

fn all_faces(e: &Engine) -> Vec<Id> {
    e.model.faces.keys().copied().collect()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sketchcraft-tp-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn canvas(e: &Engine, mat: &str) -> Canvas {
    e.model.texture_canvas(mat).unwrap()
}

fn data(e: &Engine, mat: &str) -> String {
    e.model.materials[mat].texture.as_ref().unwrap().data.clone()
}

/// A face's middle in 3D and on the picture (pixels of a `size` square picture).
fn middle(e: &Engine, face: Id, size: u32) -> ([f64; 3], (u32, u32)) {
    let f = &e.model.faces[&face];
    let pts = e.model.pts(&f.outer);
    let c = pts.iter().fold(glam::DVec3::ZERO, |a, p| a + *p) / pts.len() as f64;
    let uv = f.uv.as_ref().expect("face has uv");
    let n = uv.outer.len() as f32;
    let (u, v) = uv.outer.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0], a.1 + p[1]));
    let (u, v) = (u / n, v / n);
    ([c.x, c.y, c.z], ((u * size as f32) as u32, ((1.0 - v) * size as f32) as u32))
}

fn new_paint(e: &mut Engine, size: u32) -> String {
    let faces = all_faces(e);
    let r = run(e, json!({"cmd":"texture_paint_new","faces":faces,"size":size,"unwrap":true,"base":[1.0,1.0,1.0]}));
    r["material"].as_str().unwrap().to_string()
}

#[test]
fn faces_without_uvs_are_refused_until_unwrapped() {
    let mut e = with("crate");
    let faces = all_faces(&e);
    let s = run(&mut e, json!({"cmd":"texture_paint_status","faces":faces}));
    assert_eq!(s["paintable"], false);
    assert!(s["without"].as_array().unwrap().len() > 100, "{s}");
    let msg = fails(&mut e, json!({"cmd":"texture_paint_new","faces":faces}));
    assert!(msg.contains("have no UVs") && msg.contains("unwrap"), "{msg}");
    assert!(e.model.materials.keys().all(|k| !k.starts_with("Paint")), "a refused call changes nothing");
    let r = run(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"unwrap":true,"size":512}));
    assert_eq!(r["unwrapped"], true);
    assert_eq!(r["material"], "Paint 1");
    let s = run(&mut e, json!({"cmd":"texture_paint_status","faces":faces}));
    assert_eq!(s["paintable"], true, "{s}");
    assert!(e.model.faces.values().all(|f| f.material.as_deref() == Some("Paint 1")));
    let c = canvas(&e, "Paint 1");
    assert_eq!((c.w, c.h), (512, 512));
}

#[test]
fn a_stroke_changes_only_the_expected_pixels_and_erase_restores_them() {
    let mut e = with("beaver");
    let mat = new_paint(&mut e, 512);
    let before = canvas(&e, &mat);
    let faces = all_faces(&e);
    // the face with the biggest picture area, painted at its middle with a small hard brush
    let target = faces[0];
    let (p, px) = middle(&e, target, 512);
    let r = run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[p],"radius":0.02,"color":[1.0,0.0,0.0],"hardness":1.0}));
    assert!(r["texels"].as_u64().unwrap() > 0);
    let after = canvas(&e, &mat);
    assert_ne!(before, after);
    // the texel under the brush is red, and everything that changed is red (a hard full-opacity brush)
    let c = after.get(px.0, px.1);
    assert_eq!(c, [255, 0, 0, 255], "the middle of the face");
    let changed: Vec<(u32, u32)> = (0..512).flat_map(|y| (0..512).map(move |x| (x, y))).filter(|&(x, y)| before.get(x, y) != after.get(x, y)).collect();
    assert!(changed.iter().all(|&(x, y)| after.get(x, y) == [255, 0, 0, 255]));
    assert!(changed.len() < 512 * 512 / 4, "a 2 cm brush on a 1.5 m model is a small patch ({})", changed.len());
    // far from the dab nothing moved
    let far = faces.iter().copied().max_by(|a, b| {
        let d = |f: &Id| {
            let (q, _) = middle(&e, *f, 512);
            ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()
        };
        d(a).total_cmp(&d(b))
    });
    let (_, fpx) = middle(&e, far.unwrap(), 512);
    assert_eq!(after.get(fpx.0, fpx.1), before.get(fpx.0, fpx.1));
    // Alt: erase the same stroke and the picture is exactly as it was
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[p],"radius":0.02,"hardness":1.0,"erase":true}));
    assert_eq!(canvas(&e, &mat), before);
}

#[test]
fn a_stroke_is_one_undo_step_and_undo_restores_the_exact_picture() {
    let mut e = with("beaver");
    let mat = new_paint(&mut e, 256);
    let faces = all_faces(&e);
    let (p, _) = middle(&e, faces[0], 256);
    let d0 = data(&e, &mat);
    let depth = e.undo_depth();
    // a long stroke: many points, still one step
    let pts: Vec<[f64; 3]> = (0..40).map(|i| [p[0] + i as f64 * 0.002, p[1], p[2]]).collect();
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":pts,"radius":0.03,"color":[0.0,0.0,1.0]}));
    assert_eq!(e.undo_depth(), depth + 1);
    let d1 = data(&e, &mat);
    assert_ne!(d0, d1);
    run(&mut e, json!({"cmd":"undo"}));
    assert_eq!(data(&e, &mat), d0, "byte-identical after undo");
    run(&mut e, json!({"cmd":"redo"}));
    assert_eq!(data(&e, &mat), d1);
}

#[test]
fn a_stroke_depends_on_its_points_not_on_how_they_were_batched() {
    let mut a = with("beaver");
    let mut b = with("beaver");
    let (ma, mb) = (new_paint(&mut a, 256), new_paint(&mut b, 256));
    let faces = all_faces(&a);
    let (p, _) = middle(&a, faces[0], 256);
    let pts: Vec<[f64; 3]> = (0..30).map(|i| [p[0] + i as f64 * 0.003, p[1], p[2]]).collect();
    run(&mut a, json!({"cmd":"texture_paint_stroke","material":ma,"points":pts,"radius":0.03,"color":[0.2,0.6,0.1],"opacity":0.7}));
    // the same points in a different order, with repeats
    let mut rev = pts.clone();
    rev.reverse();
    rev.extend(pts.iter().take(5).cloned());
    run(&mut b, json!({"cmd":"texture_paint_stroke","material":mb,"points":rev,"radius":0.03,"color":[0.2,0.6,0.1],"opacity":0.7}));
    assert_eq!(data(&a, &ma), data(&b, &mb));
}

#[test]
fn a_brush_across_a_uv_seam_paints_both_sides() {
    let mut e = with("dice");
    let mat = new_paint(&mut e, 512);
    let faces = all_faces(&e);
    let info = run(&mut e, json!({"cmd":"uv_info","faces":faces}));
    let island: std::collections::HashMap<u64, u64> = info["faces"].as_array().unwrap().iter().map(|f| (f["id"].as_u64().unwrap(), f["island"].as_u64().unwrap())).collect();
    // two faces that share an edge but sit on different islands (a seam)
    let m = &e.model;
    let mut pair = None;
    'outer: for (&ia, fa) in &m.faces {
        for (&ib, fb) in &m.faces {
            if ia >= ib || island[&(ia as u64)] == island[&(ib as u64)] {
                continue;
            }
            let shared: Vec<Id> = fa.outer.iter().copied().filter(|v| fb.outer.contains(v)).collect();
            if shared.len() == 2 {
                pair = Some((ia, ib, shared));
                break 'outer;
            }
        }
    }
    let (fa, fb, shared) = pair.expect("a cube unwrap has seams");
    let (p0, p1) = (m.p(shared[0]), m.p(shared[1]));
    let mid = (p0 + p1) / 2.0;
    let before = canvas(&e, &mat);
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[[mid.x, mid.y, mid.z]],"radius":0.01,"color":[1.0,0.0,0.0],"hardness":1.0}));
    let after = canvas(&e, &mat);
    let bbox = |f: Id| {
        let uv = e.model.faces[&f].uv.as_ref().unwrap();
        let xs: Vec<f32> = uv.outer.iter().map(|p| p[0] * 512.0).collect();
        let ys: Vec<f32> = uv.outer.iter().map(|p| (1.0 - p[1]) * 512.0).collect();
        (xs.iter().cloned().fold(f32::MAX, f32::min) as i32 - 2, xs.iter().cloned().fold(f32::MIN, f32::max) as i32 + 2, ys.iter().cloned().fold(f32::MAX, f32::min) as i32 - 2, ys.iter().cloned().fold(f32::MIN, f32::max) as i32 + 2)
    };
    let hits = |f: Id| {
        let (x0, x1, y0, y1) = bbox(f);
        (y0.max(0)..y1.min(512)).flat_map(|y| (x0.max(0)..x1.min(512)).map(move |x| (x, y))).filter(|&(x, y)| before.get(x as u32, y as u32) != after.get(x as u32, y as u32)).count()
    };
    assert!(hits(fa) > 0, "painted on the first face's island");
    assert!(hits(fb) > 0, "and across the seam on the second face's island");
}

#[test]
fn a_stroke_on_the_picture_paints_a_disc_where_the_uv_says() {
    let mut e = with("beaver");
    let mat = new_paint(&mut e, 256);
    let before = canvas(&e, &mat);
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"uv_points":[[0.5,0.5]],"uv_radius":0.05,"color":[0.0,1.0,0.0],"hardness":1.0}));
    let after = canvas(&e, &mat);
    assert_eq!(after.get(128, 128), [0, 255, 0, 255]);
    assert_eq!(after.get(128 + 12, 128), [0, 255, 0, 255]);
    assert_eq!(after.get(128 + 14, 128), before.get(128 + 14, 128));
    assert_eq!(after.get(0, 0), before.get(0, 0));
    // uv (0.25, 0.75) is a quarter across and a quarter down from the top
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"uv_points":[[0.25,0.75]],"uv_radius":0.02,"color":[0.0,0.0,1.0],"hardness":1.0}));
    assert_eq!(canvas(&e, &mat).get(64, 64), [0, 0, 255, 255]);
}

#[test]
fn a_paint_texture_starts_from_the_present_material() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    let colour = e.model.materials[e.model.faces[&faces[0]].material.as_ref().unwrap()].color;
    let r = run(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"unwrap":true,"size":256,"base":"material","name":"Dice paint"}));
    assert_eq!(r["material"], "Dice paint");
    let c = canvas(&e, "Dice paint");
    let (_, px) = middle(&e, faces[0], 256);
    let want = [0, 1, 2].map(|k| (colour[k] * 255.0).round() as u8);
    assert_eq!(&c.get(px.0, px.1)[..3], &want[..]);
    // a name in use is refused
    let msg = fails(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"name":"Dice paint","unwrap":true}));
    assert!(msg.contains("already"), "{msg}");
}

#[test]
fn a_paint_texture_bakes_a_picture_through_the_new_uvs() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    // a 2 x 1 picture: left half red, right half blue
    let mut c = Canvas::filled(64, 32, [255, 0, 0]);
    for y in 0..32 {
        for x in 32..64 {
            c.px[((y * 64 + x) * 4) as usize..((y * 64 + x) * 4 + 3) as usize].copy_from_slice(&[0, 0, 255]);
        }
    }
    run(&mut e, json!({"cmd":"texture","name":"Flag","data":c.to_data_url().unwrap(),"width":0.05,"height":0.05}));
    run(&mut e, json!({"cmd":"texture_project","faces":faces,"mode":"planar","material":"Flag","axis":[0.0,0.0,-1.0]}));
    run(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"unwrap":true,"size":128,"base":"material"}));
    let out = canvas(&e, "Paint 1");
    // the baked picture holds both colours of the flag
    let red = (0..128 * 128).filter(|i| out.px[i * 4..i * 4 + 4] == [255, 0, 0, 255]).count();
    let blue = (0..128 * 128).filter(|i| out.px[i * 4..i * 4 + 4] == [0, 0, 255, 255]).count();
    assert!(red > 100 && blue > 100, "red {red}, blue {blue}");
}

#[test]
fn the_painted_picture_saves_loads_and_exports_losslessly() {
    let dir = scratch("save");
    let mut e = with("beaver");
    let mat = new_paint(&mut e, 256);
    let faces = all_faces(&e);
    let (p, _) = middle(&e, faces[0], 256);
    run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[p],"radius":0.05,"color":[0.9,0.1,0.4],"hardness":0.3,"opacity":0.8}));
    let want = data(&e, &mat);
    let path = dir.join("p.sketchcraft").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"save","path":path}));
    assert!(sketchcraft::format::check(&std::fs::read(&path).unwrap()).is_none(), "this build reads it");
    let mut back = Engine::new();
    run(&mut back, json!({"cmd":"open","path":path}));
    assert_eq!(data(&back, &mat), want, "byte-identical PNG after a save and load");
    // saving the loaded model again gives the same file
    let path2 = dir.join("q.sketchcraft").to_string_lossy().to_string();
    run(&mut back, json!({"cmd":"save","path":path2}));
    assert_eq!(std::fs::read(&path).unwrap(), std::fs::read(&path2).unwrap());
    // the GLB embeds the picture
    let glb = dir.join("p.glb").to_string_lossy().to_string();
    run(&mut e, json!({"cmd":"export_glb","path":glb}));
    let b = std::fs::read(&glb).unwrap();
    let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let doc: Value = serde_json::from_slice(&b[20..20 + jl]).unwrap();
    let bin = &b[20 + jl + 8..];
    let img = &doc["images"][0];
    assert_eq!(img["mimeType"], "image/png");
    let v = &doc["bufferViews"][img["bufferView"].as_u64().unwrap() as usize];
    let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
    let png = &bin[off..off + v["byteLength"].as_u64().unwrap() as usize];
    let decoded = image::load_from_memory(png).unwrap().to_rgba8();
    assert_eq!(decoded.into_raw(), canvas(&e, &mat).px, "the picture in the GLB is the painted one");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn bad_calls_are_refused_and_change_nothing() {
    let mut e = with("dice");
    let faces = all_faces(&e);
    assert!(fails(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"size":10,"unwrap":true})).contains("size"));
    assert!(fails(&mut e, json!({"cmd":"texture_paint_new","faces":faces,"size":99999,"unwrap":true})).contains("size"));
    assert!(fails(&mut e, json!({"cmd":"texture_paint_new","unwrap":true})).contains("faces"));
    let mat = new_paint(&mut e, 128);
    let before = data(&e, &mat);
    let ids = all_faces(&e);
    assert!(fails(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[[0,0,0]],"radius":0.1})).contains("color"));
    assert!(fails(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[[0,0,0]],"radius":0.1,"color":[1,0,0],"opacity":2})).contains("opacity"));
    assert!(fails(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"color":[1,0,0]})).contains("points"));
    assert!(fails(&mut e, json!({"cmd":"texture_paint_stroke","material":"Nope","points":[[0,0,0]],"radius":0.1,"color":[1,0,0]})).contains("Nope"));
    let plain = e.model.materials.keys().find(|k| e.model.materials[*k].texture.is_none()).cloned();
    if let Some(p) = plain {
        assert!(fails(&mut e, json!({"cmd":"texture_paint_stroke","material":p,"points":[[0,0,0]],"radius":0.1,"color":[1,0,0],"faces":ids})).contains("no picture"));
    }
    assert_eq!(data(&e, &mat), before);
    // reading pixels
    let r = run(&mut e, json!({"cmd":"texture_pixels","material":mat,"x":0,"y":0,"w":2,"h":2}));
    assert_eq!(r["width"], 128);
    assert_eq!(r["pixels"].as_array().unwrap().len(), 2);
    assert!(fails(&mut e, json!({"cmd":"texture_pixels","material":mat,"x":0,"y":0,"w":128,"h":128})).contains("4096"));
}

#[test]
fn a_stroke_on_faces_of_the_crate_reaches_their_texels() {
    let mut e = with("crate");
    let mat = new_paint(&mut e, 512);
    let faces = all_faces(&e);
    let mut missed = vec![];
    for f in faces.iter().step_by(7) {
        let (p, px) = middle(&e, *f, 512);
        let before = canvas(&e, &mat).get(px.0, px.1);
        let r = run(&mut e, json!({"cmd":"texture_paint_stroke","material":mat,"points":[p],"radius":0.02,"color":[1.0,0.0,0.0],"hardness":1.0,"opacity":1.0}));
        let after = canvas(&e, &mat).get(px.0, px.1);
        if r["texels"].as_u64().unwrap() == 0 || after != [255, 0, 0, 255] {
            missed.push((*f, r["texels"].clone(), before, after));
        }
    }
    assert!(missed.is_empty(), "{missed:?}");
}
