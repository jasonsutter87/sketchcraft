//! Hardening for 0.3.11: push/pull through a part at its edge or around a hole stays solid, colours from a file are
//! always numbers, scene lists count toward the limits, links in a workspace aren't written through, and a few
//! smaller limits.

use serde_json::{json, Value};
use sketchcraft::policy::{Caller, Workspace};
use sketchcraft::Engine;
use std::io::{Read, Write};

/// A fresh folder under the system temp folder, removed when dropped.
struct Tmp(std::path::PathBuf);
impl Tmp {
    fn new(tag: &str) -> Tmp {
        let d = std::env::temp_dir().join(format!("sc-hardening-0311-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        Tmp(d)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

/// Every edge of every face is used by exactly two faces.
fn closed(e: &Engine) -> bool {
    let mut uses = std::collections::HashMap::<(u32, u32), usize>::new();
    for f in e.model.faces.values() {
        for l in f.loops() {
            for i in 0..l.len() {
                let (a, b) = (l[i], l[(i + 1) % l.len()]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
    }
    uses.values().all(|&n| n == 2)
}

fn slab(e: &mut Engine, t: f64) {
    let f = ok(e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [10, 10, 0]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": t}));
}

fn solid(e: &mut Engine) -> bool {
    ok(e, json!({"cmd": "check_solid"}))["solid"] == true
}

#[test]
fn a_notch_at_the_edge_stays_solid_part_way_and_all_the_way_through() {
    for (from, to, d) in [([3., 0., 2.], [6., 2., 2.], -1.0), ([3., 0., 2.], [6., 2., 2.], -2.0), ([0., 0., 2.], [3., 2.5, 2.], -9.0)] {
        let mut e = Engine::new();
        slab(&mut e, 2.0);
        let n = ok(&mut e, json!({"cmd": "rect", "from": from, "to": to}))["face"].clone();
        let r = ok(&mut e, json!({"cmd": "pushpull", "face": n, "distance": d}));
        assert!(closed(&e) && solid(&mut e), "{from:?} {d}: still a closed solid");
        // nothing left over the notch on the outside wall
        let over = glam::DVec3::new(from[0] + 0.5, 0.0, 1.9);
        assert!(!e.model.faces.keys().any(|&id| e.model.contains(id, over)), "{from:?} {d}: the notch is open at the side");
        for id in r["created"].as_array().unwrap().iter().chain([&r["face"]]) {
            assert!(e.model.faces.contains_key(&(id.as_u64().unwrap() as u32)), "the reply names faces that exist");
        }
    }
}

#[test]
fn an_opening_around_an_earlier_hole_leaves_one_hole() {
    let mut e = Engine::new();
    slab(&mut e, 1.0);
    let a = ok(&mut e, json!({"cmd": "rect", "from": [4, 4, 1], "to": [6, 6, 1]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": a, "distance": -5}));
    let b = ok(&mut e, json!({"cmd": "rect", "from": [2, 2, 1], "to": [8, 8, 1]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": b, "distance": -5}));
    assert!(closed(&e) && solid(&mut e));
    let top: f64 = e.model.faces.keys().filter(|id| e.model.normal(**id).z > 0.9).map(|id| e.model.face_area(*id)).sum();
    assert!((top - 64.0).abs() < 1e-9, "top area {top}");
    assert!(e.model.faces.values().all(|f| f.holes.len() <= 1), "no face has overlapping holes");
}

#[test]
fn drawing_around_a_hole_takes_the_hole_with_it() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [10, 10, 0]}))["face"].as_u64().unwrap() as u32;
    ok(&mut e, json!({"cmd": "rect", "from": [4, 4, 0], "to": [6, 6, 0]}));
    let inner = ok(&mut e, json!({"cmd": "rect", "from": [2, 2, 0], "to": [8, 8, 0]}))["face"].as_u64().unwrap() as u32;
    assert_eq!(e.model.faces[&f].holes.len(), 1, "the big face has only the new hole");
    assert_eq!(e.model.faces[&inner].holes.len(), 1, "the new face has the old one");
}

/// The corpus file with one material's red channel set to `red`.
fn skp_with_red(red: &str) -> Vec<u8> {
    let src = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/skp_corpus/v22-soft-block.skp")).unwrap();
    // a SketchUp 2017+ file is a header, then a zip: keep the header
    let at = src.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
    let head = src[..at].to_vec();
    let mut zin = zip::ZipArchive::new(std::io::Cursor::new(src[at..].to_vec())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut changed = false;
    for i in 0..zin.len() {
        let mut f = zin.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name.ends_with("material.xml") && !changed {
            let s = String::from_utf8_lossy(&data).to_string();
            let i = s.find(" colorRed=\"").unwrap() + 11;
            let j = i + s[i..].find('"').unwrap();
            data = format!("{}{red}{}", &s[..i], &s[j..]).into_bytes();
            changed = true;
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    assert!(changed);
    [head, out.finish().unwrap().into_inner()].concat()
}

#[test]
fn colours_from_a_sketchup_file_are_numbers_and_the_model_saves_and_opens() {
    use base64::Engine as _;
    let dir = Tmp::new("skp");
    for red in ["NaN", "inf", "-inf", "1e38", "-5000", "junk"] {
        let mut e = Engine::new();
        let data = base64::engine::general_purpose::STANDARD.encode(skp_with_red(red));
        ok(&mut e, json!({"cmd": "import_skp", "data": data}));
        assert!(e.model.materials.values().chain(e.model.definitions.values().flat_map(|d| d.model.materials.values())).all(|m| m.color.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c))), "{red}: every colour is 0..1");
        let path = dir.path().join(format!("m{}.sketchcraft", red.len()));
        ok(&mut e, json!({"cmd": "save", "path": path}));
        let mut back = Engine::new();
        ok(&mut back, json!({"cmd": "open", "path": path}));
    }
}

#[test]
fn a_colour_that_isnt_a_number_is_refused_before_it_is_saved() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let mut m = e.model.clone();
    let name = m.color_material([0.5, 0.5, 0.5]);
    m.materials.get_mut(&name).unwrap().color[0] = f32::NAN;
    assert!(m.check_sane_with(false).is_err());
}

fn agent(ws: &std::path::Path) -> Caller {
    Caller::Mcp(Workspace::at(ws))
}

#[cfg(unix)]
#[test]
fn an_agent_doesnt_write_through_a_link_that_points_nowhere_yet() {
    let base = Tmp::new("link");
    let ws = base.path().join("ws");
    let outside = base.path().join("outside");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(outside.join("d.obj"), ws.join("d.obj")).unwrap();
    std::os::unix::fs::symlink(outside.join("pwn.mtl"), ws.join("s1.mtl")).unwrap();
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let caller = agent(&ws);
    for name in ["d.obj", "s1.obj"] {
        let r = e.exec_json_as(&caller, &json!({"cmd": "export_obj", "path": ws.join(name)}).to_string());
        assert_eq!(r["ok"], false, "{name}: refused -> {r}");
    }
    assert!(!outside.join("d.obj").exists() && !outside.join("pwn.mtl").exists(), "nothing was written outside");
    // a plain new name in the workspace still works
    let r = e.exec_json_as(&caller, &json!({"cmd": "export_obj", "path": ws.join("fine.obj")}).to_string());
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn scene_lists_count_and_agents_keep_to_limits() {
    let mut e = Engine::new();
    let ids: Vec<u32> = (0..30).map(|i| ok(&mut e, json!({"cmd": "line", "points": [[i, 0, 0], [i, 1, 0]]}))["edges"][0].as_u64().unwrap_or(0) as u32).collect();
    let ids: Vec<u32> = if ids.iter().all(|&i| i != 0) { ids } else { e.model.edges.keys().copied().collect() };
    let before = e.model.size_estimate();
    ok(&mut e, json!({"cmd": "save_view", "name": "only", "eye": [5, 5, 5], "target": [0, 0, 0], "only": ids}));
    assert!(e.model.size_estimate() >= before + 12 * ids.len(), "the list counts toward the model's size");
    let caller = Caller::Mcp(Workspace::anywhere());
    let big: Vec<u32> = (0..20_001).map(|i| ids[i % ids.len()]).collect();
    let r = e.exec_json_as(&caller, &json!({"cmd": "save_view", "name": "big", "eye": [5, 5, 5], "target": [0, 0, 0], "only": big}).to_string());
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn a_scene_from_a_file_that_shows_nothing_left_doesnt_hide_everything() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    for only in [json!([]), json!([999_999])] {
        let mut m = e.model.clone();
        let mut v: sketchcraft::model::View = serde_json::from_value(json!({"name": "s", "eye": [5, 5, 5], "target": [0, 0, 0], "fov": 45.0, "only": only})).unwrap();
        v.hidden = Some(vec![]);
        m.views = vec![v];
        let mut e2 = Engine::new();
        e2.model = m;
        ok(&mut e2, json!({"cmd": "show_view", "name": "s"}));
        assert!(e2.model.faces.values().all(|f| !f.hidden), "{only}: the model still shows");
    }
}

#[test]
fn texture_files_never_get_a_windows_device_name_and_many_alike_are_quick() {
    assert_eq!(sketchcraft::texmap::file_name("CON", "png"), "CON_.png");
    assert_eq!(sketchcraft::texmap::file_name("com1", "png"), "com1_.png");
    assert_eq!(sketchcraft::texmap::file_name("Brick", "png"), "Brick.png");
    let mut names = sketchcraft::texmap::FileNames::default();
    let t = std::time::Instant::now();
    let all: std::collections::HashSet<String> = (0..5000).map(|i| names.name(&format!("a b{}", "!".repeat(i % 3)), "png")).collect();
    assert_eq!(all.len(), 5000, "every picture its own file");
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:?}", t.elapsed());
}

#[cfg(unix)]
#[test]
fn an_agent_with_any_path_doesnt_write_through_a_link_either() {
    let base = Tmp::new("anylink");
    let target = base.path().join("elsewhere.obj");
    std::os::unix::fs::symlink(&target, base.path().join("dl.obj")).unwrap();
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let r = e.exec_json_as(&Caller::Mcp(Workspace::anywhere()), &json!({"cmd": "export_obj", "path": base.path().join("dl.obj")}).to_string());
    assert_eq!(r["ok"], false, "{r}");
    assert!(!target.exists(), "nothing was written where the link points");
}

#[test]
fn colours_in_a_saved_model_are_brought_into_range_when_it_opens() {
    let dir = Tmp::new("open");
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}));
    let path = dir.path().join("m.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["materials"] = json!({"odd": {"color": [7.0, -3.0, 1e30], "opacity": 9.0}});
    std::fs::write(&path, v.to_string()).unwrap();
    let mut back = Engine::new();
    ok(&mut back, json!({"cmd": "open", "path": path}));
    let m = &back.model.materials["odd"];
    assert_eq!(m.color, [1.0, 0.0, 1.0]);
    assert_eq!(m.opacity, 1.0);
}

#[cfg(unix)]
#[test]
fn an_agents_writes_dont_follow_a_link_put_in_place_late_but_the_persons_do() {
    let base = Tmp::new("late");
    let target = base.path().join("target.obj");
    let link = base.path().join("late.obj");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    sketchcraft::safety::set_no_follow(true);
    assert!(sketchcraft::safety::write_output(&link, b"x").is_err(), "an agent's export isn't written through a link");
    assert!(sketchcraft::safety::write_atomic(&link, b"x").is_err(), "nor is an atomic write");
    assert!(!target.exists());
    sketchcraft::safety::set_no_follow(false);
    sketchcraft::safety::write_output(&link, b"y").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"y", "the person's own export goes where the link points, as before");
}

#[test]
fn colours_read_from_any_file_are_in_range_tag_colours_too() {
    let v = json!({"materials": {"m": {"color": [3.0, -1.0, 1e39], "opacity": 7.0, "roughness": -2.0, "emission": 1e9}}, "tag_colors": {"t": [2.0, 0.5, -1.0]}});
    let base: Value = serde_json::to_value(sketchcraft::model::Model::default()).unwrap();
    let mut whole = base.clone();
    for (k, x) in v.as_object().unwrap() {
        whole[k] = x.clone();
    }
    let m: sketchcraft::model::Model = serde_json::from_value(whole).unwrap();
    let mat = &m.materials["m"];
    assert_eq!(mat.color, [1.0, 0.0, 1.0]);
    assert_eq!(mat.opacity, 1.0);
    assert_eq!(mat.roughness, Some(0.0));
    assert_eq!(mat.emission, Some(100.0));
    assert_eq!(m.tag_colors["t"], [1.0, 0.5, 0.0]);
}
