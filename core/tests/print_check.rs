//! Print check and 3MF export (docs/dev/print-check.md). Fixtures are built from raw faces so each can be as broken as it
//! needs to be; sizes are in millimetres written as meters.

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::model::{Face, Group};
use sketchcraft::Engine;
use std::collections::HashMap;
use std::io::Read;

const MM: f64 = 0.001;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn raw_face(e: &mut Engine, pts: &[[f64; 3]], group: Option<u32>) -> u32 {
    let lp: Vec<u32> = pts.iter().map(|p| e.model.vertex_in(DVec3::from_array(*p), group)).collect();
    for i in 0..lp.len() {
        e.model.edge(lp[i], lp[(i + 1) % lp.len()]);
    }
    e.model.next_id += 1;
    let id = e.model.next_id;
    e.model.faces.insert(id, Face { outer: lp, group, ..Default::default() });
    id
}

/// An axis-aligned box, faces outward (or all turned around: a cavity), `skip` leaves one face out (+z top).
fn boxy(e: &mut Engine, lo: [f64; 3], hi: [f64; 3], inward: bool, open_top: bool, group: Option<u32>) -> Vec<u32> {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    // bottom, top, front, right, back, left (outward when listed like this)
    let quads: [[usize; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [1, 3, 7, 5], [3, 2, 6, 7], [2, 0, 4, 6]];
    let mut ids = vec![];
    for (k, q) in quads.iter().enumerate() {
        if open_top && k == 1 {
            continue;
        }
        let mut pts: Vec<[f64; 3]> = q.iter().map(|&i| c(i)).collect();
        if inward {
            pts.reverse();
        }
        ids.push(raw_face(e, &pts, group));
    }
    ids
}

/// A prism: the polygon (x, z; counter-clockwise) extruded along y from 0 to `h`.
fn prism(e: &mut Engine, poly: &[[f64; 2]], h: f64) {
    let at = |p: [f64; 2], y: f64| [p[0], y, p[1]];
    raw_face(e, &poly.iter().map(|&p| at(p, 0.0)).collect::<Vec<_>>(), None);
    raw_face(e, &poly.iter().rev().map(|&p| at(p, h)).collect::<Vec<_>>(), None);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        raw_face(e, &[at(a, 0.0), at(a, h), at(b, h), at(b, 0.0)], None);
    }
}

fn cube20(e: &mut Engine) {
    boxy(e, [0.0; 3], [20.0 * MM; 3], false, false, None);
}

fn t_shape(e: &mut Engine) {
    let p = |x: f64, z: f64| [x * MM, z * MM];
    prism(e, &[p(10., 0.), p(20., 0.), p(20., 20.), p(30., 20.), p(30., 30.), p(0., 30.), p(0., 20.), p(10., 20.)], 10.0 * MM);
}

fn check(e: &mut Engine, extra: Value) -> Value {
    let mut v = json!({"cmd": "print_check"});
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    run(e, v)
}

#[test]
fn a_solid_cube_passes() {
    let mut e = Engine::new();
    cube20(&mut e);
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], true);
    assert_eq!(r["thin_walls"]["count"], 0);
    assert_eq!(r["overhangs"]["count"], 0, "the bottom is on the bed");
    assert_eq!(r["size_mm"], json!([20.0, 20.0, 20.0]));
    assert!(r["units_hint"].is_null());
    assert!(r["fit"].is_null());
    assert_eq!(r["ready"], true);
}

#[test]
fn an_open_box_is_not_solid_and_says_where() {
    let mut e = Engine::new();
    boxy(&mut e, [0.0; 3], [20.0 * MM; 3], false, true, None);
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], false);
    assert_eq!(r["ready"], false);
    assert_eq!(r["solid_check"]["open_edges"].as_array().unwrap().len(), 4);
    // fixing it makes it pass
    run(&mut e, json!({"cmd": "fix_model"}));
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], true);
    assert_eq!(r["ready"], true);
}

#[test]
fn it_never_changes_the_model() {
    let mut e = Engine::new();
    boxy(&mut e, [0.0; 3], [20.0 * MM; 3], false, true, None);
    let before = serde_json::to_string(&e.model).unwrap();
    check(&mut e, json!({}));
    assert_eq!(before, serde_json::to_string(&e.model).unwrap());
}

#[test]
fn a_thin_walled_box_flags_thin_walls() {
    let mut e = Engine::new();
    // a 20 mm box with a cavity leaving 0.5 mm walls on every side
    boxy(&mut e, [0.0; 3], [20.0 * MM; 3], false, false, None);
    boxy(&mut e, [0.5 * MM; 3], [19.5 * MM; 3], true, false, None);
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], true, "{r}");
    let t = &r["thin_walls"];
    assert_eq!(t["count"], 12, "all 6 outer and 6 cavity faces: {r}");
    assert!((t["thinnest_mm"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert_eq!(r["ready"], false);
    // with a 0.4 mm limit it passes
    let r = check(&mut e, json!({"min_wall": 0.4}));
    assert_eq!(r["thin_walls"]["count"], 0);
    assert_eq!(r["ready"], true);
}

#[test]
fn a_t_shape_flags_its_overhangs() {
    let mut e = Engine::new();
    t_shape(&mut e);
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], true, "{r}");
    let o = &r["overhangs"];
    assert_eq!(o["count"], 2, "the two undersides of the bar: {r}");
    // each underside is 10 x 10 mm
    assert!((o["area_mm2"].as_f64().unwrap() - 200.0).abs() < 1e-6);
    assert_eq!(o["items"][0]["lean_deg"], 90.0);
    // a ceiling needs support at any angle under 90
    let r = check(&mut e, json!({"overhang_angle": 90}));
    assert_eq!(r["overhangs"]["count"], 0);
    assert_eq!(r["ready"], true, "overhangs alone do not block");
}

#[test]
fn a_sloped_face_is_an_overhang_only_past_the_angle() {
    let mut e = Engine::new();
    let p = |x: f64, z: f64| [x * MM, z * MM];
    // its left side slopes out over the bed: the face's normal is (-20, -30), 56 degrees out of vertical
    prism(&mut e, &[p(0., 0.), p(20., 0.), p(20., 20.), p(-30., 20.)], 10.0 * MM);
    let r = check(&mut e, json!({"overhang_angle": 45}));
    assert_eq!(r["solid"], true, "{r}");
    assert_eq!(r["overhangs"]["count"], 1, "{r}");
    assert!((r["overhangs"]["items"][0]["lean_deg"].as_f64().unwrap() - 56.3).abs() < 0.1);
    assert_eq!(check(&mut e, json!({"overhang_angle": 70}))["overhangs"]["count"], 0);
}

#[test]
fn half_a_millimetre_gets_the_units_hint() {
    let mut e = Engine::new();
    boxy(&mut e, [0.0; 3], [0.5 * MM; 3], false, false, None);
    let r = check(&mut e, json!({}));
    assert_eq!(r["units_hint"]["kind"], "too_small");
    assert_eq!(r["ready"], false);
    // and a building-sized model the other way
    let mut e = Engine::new();
    boxy(&mut e, [0.0; 3], [10.0; 3], false, false, None);
    let r = check(&mut e, json!({}));
    assert_eq!(r["units_hint"]["kind"], "too_large");
}

#[test]
fn it_says_whether_it_fits_the_build_volume() {
    let mut e = Engine::new();
    t_shape(&mut e); // 30 x 10 x 30
    let r = check(&mut e, json!({"build": [220, 220, 250]}));
    assert_eq!(r["fit"]["fits"], true);
    let r = check(&mut e, json!({"build": [25, 100, 100]}));
    assert_eq!(r["fit"]["fits"], false);
    assert_eq!(r["fit"]["fits_if_turned"], true);
    assert_eq!(r["fit"]["over_mm"], json!([5.0, 0.0, 0.0]));
    assert_eq!(r["ready"], false);
    let r = check(&mut e, json!({"build": [20, 20, 20]}));
    assert_eq!(r["fit"]["fits_if_turned"], false);
}

#[test]
fn it_can_look_at_one_group() {
    let mut e = Engine::new();
    cube20(&mut e);
    e.model.groups.insert(500, Group { name: "Lid".into(), ..Default::default() });
    // an open box in a group far away
    boxy(&mut e, [100.0 * MM, 0.0, 0.0], [120.0 * MM, 20.0 * MM, 20.0 * MM], false, true, Some(500));
    assert_eq!(check(&mut e, json!({}))["solid"], false);
    assert_eq!(check(&mut e, json!({"ids": [500]}))["solid"], false);
    let r = check(&mut e, json!({"ids": [500]}));
    assert_eq!(r["size_mm"], json!([20.0, 20.0, 20.0]), "only the group's faces are measured");
}

#[test]
fn an_empty_model_is_not_ready() {
    let mut e = Engine::new();
    let r = check(&mut e, json!({}));
    assert_eq!(r["solid"], false);
    assert_eq!(r["ready"], false);
    assert!(r["size_mm"].is_null());
}

#[test]
fn it_is_allowed_for_agents() {
    use sketchcraft::policy::{vet, Caller};
    let mut c = json!({"cmd": "print_check"});
    assert!(vet(&Caller::Plugin, &mut c).is_ok());
}

// ---------- 3MF ----------

fn export(e: &mut Engine, units: Option<&str>) -> (Value, String, Vec<String>) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("sc-3mf-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.3mf");
    let mut cmd = json!({"cmd": "export_3mf", "path": path.to_str().unwrap()});
    if let Some(u) = units {
        cmd["units"] = json!(u);
    }
    let r = run(e, cmd);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().unwrap().to_string()).collect();
    let mut xml = String::new();
    z.by_name("3D/3dmodel.model").unwrap().read_to_string(&mut xml).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    (r, xml, names)
}

fn attr(tag: &str, name: &str) -> String {
    let k = format!(" {name}=\"");
    let s = tag.find(&k).unwrap_or_else(|| panic!("{name} in {tag}")) + k.len();
    tag[s..s + tag[s..].find('"').unwrap()].to_string()
}

fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name} ");
    xml.match_indices(&open).map(|(i, _)| &xml[i..i + xml[i..].find('>').unwrap()]).collect()
}

/// Every undirected edge is used by exactly two triangles, and each direction once (consistent winding).
fn assert_closed(xml: &str) {
    for obj in xml.split("<object ").skip(1) {
        let mut directed: HashMap<(usize, usize), usize> = HashMap::new();
        for t in tags(obj, "triangle") {
            let v: Vec<usize> = ["v1", "v2", "v3"].iter().map(|k| attr(t, k).parse().unwrap()).collect();
            for i in 0..3 {
                *directed.entry((v[i], v[(i + 1) % 3])).or_default() += 1;
            }
        }
        assert!(!directed.is_empty());
        for (&(a, b), &n) in &directed {
            assert_eq!(n, 1, "edge {a}->{b} runs {n} times in one direction");
            assert_eq!(directed.get(&(b, a)).copied(), Some(1), "edge {a}-{b} is not shared by exactly two triangles");
        }
    }
}

/// Signed volume in the file's units: positive when the triangles face out.
fn volume(xml: &str) -> f64 {
    let v: Vec<[f64; 3]> = tags(xml, "vertex").iter().map(|t| ["x", "y", "z"].map(|k| attr(t, k).parse().unwrap())).collect();
    tags(xml, "triangle")
        .iter()
        .map(|t| {
            let i = ["v1", "v2", "v3"].map(|k| attr(t, k).parse::<usize>().unwrap());
            let (a, b, c) = (DVec3::from_array(v[i[0]]), DVec3::from_array(v[i[1]]), DVec3::from_array(v[i[2]]));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[test]
fn a_cube_exports_as_a_valid_3mf() {
    let mut e = Engine::new();
    cube20(&mut e);
    let (r, xml, names) = export(&mut e, None);
    assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]);
    assert_eq!(r["solid"], true);
    assert_eq!(r["objects"], 1);
    assert!(xml.starts_with("<?xml"));
    let model = tags(&xml, "model")[0].to_string();
    assert_eq!(attr(&model, "unit"), "millimeter");
    assert!(model.contains("http://schemas.microsoft.com/3dmanufacturing/core/2015/02"));
    assert_eq!(tags(&xml, "vertex").len(), 8);
    assert_eq!(tags(&xml, "triangle").len(), 12);
    assert_eq!(r["triangles"], 12);
    assert_eq!(tags(&xml, "item").len(), 1);
    assert_closed(&xml);
    assert!((volume(&xml) - 8000.0).abs() < 1e-6, "20 mm cube, outward: {}", volume(&xml));
    // real size: the corners are 20 (mm), not 0.02
    assert!(tags(&xml, "vertex").iter().any(|t| attr(t, "x") == "20"));
}

#[test]
fn a_non_convex_shape_is_closed_and_faces_out() {
    let mut e = Engine::new();
    t_shape(&mut e);
    let (_, xml, _) = export(&mut e, None);
    assert_closed(&xml);
    // stem 10x20x10 + bar 30x10x10
    assert!((volume(&xml) - 5000.0).abs() < 1e-6, "{}", volume(&xml));
}

#[test]
fn units_can_be_the_models_own() {
    let mut e = Engine::new();
    cube20(&mut e);
    let (_, xml, _) = export(&mut e, Some("inch"));
    assert_eq!(attr(tags(&xml, "model")[0], "unit"), "inch");
    assert!((volume(&xml) - 8000.0 / 25.4f64.powi(3)).abs() < 1e-4);
    run(&mut e, json!({"cmd": "units", "units": "cm"}));
    let (_, xml, _) = export(&mut e, Some("model"));
    assert_eq!(attr(tags(&xml, "model")[0], "unit"), "centimeter");
    let bad = e.exec_json(&json!({"cmd": "export_3mf", "path": "/tmp/sc-never.3mf", "units": "parsec"}).to_string());
    assert_eq!(bad["ok"], false);
}

#[test]
fn each_group_is_its_own_object() {
    let mut e = Engine::new();
    e.model.groups.insert(600, Group { name: "Base & <lid>".into(), ..Default::default() });
    boxy(&mut e, [0.0; 3], [20.0 * MM; 3], false, false, Some(600));
    boxy(&mut e, [50.0 * MM, 0.0, 0.0], [70.0 * MM, 20.0 * MM, 20.0 * MM], false, false, None);
    let (r, xml, _) = export(&mut e, None);
    assert_eq!(r["objects"], 2);
    assert_eq!(tags(&xml, "object").len(), 2);
    assert_eq!(tags(&xml, "item").len(), 2);
    assert!(xml.contains("Base &amp; &lt;lid&gt;"), "names are escaped");
    assert_closed(&xml);
}

#[test]
fn an_empty_model_does_not_export() {
    let mut e = Engine::new();
    let r = e.exec_json(&json!({"cmd": "export_3mf", "path": "/tmp/sc-never-empty.3mf"}).to_string());
    assert_eq!(r["ok"], false);
    assert!(!std::path::Path::new("/tmp/sc-never-empty.3mf").exists());
}

#[test]
fn a_model_that_is_not_solid_exports_but_says_so() {
    let mut e = Engine::new();
    boxy(&mut e, [0.0; 3], [20.0 * MM; 3], false, true, None);
    let (r, _, _) = export(&mut e, None);
    assert_eq!(r["solid"], false);
}
