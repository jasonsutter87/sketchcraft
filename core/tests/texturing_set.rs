//! The texturing test set (dice, can, globe, crate, soccer ball, plus the beaver): every texturing feature is built and
//! tested on these. They are ordinary built-in assets made by tools/texturing_set/texturing_set.py.
use serde_json::{json, Value};
use sketchcraft::model::{Id, Model};
use sketchcraft::Engine;

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn build(key: &str) -> Model {
    let a = sketchcraft::assets::find(key).unwrap_or_else(|| panic!("no asset {key}"));
    assert!(!a.face_me, "{key}");
    sketchcraft::assets::build(a).unwrap_or_else(|e| panic!("{key}: {e}"))
}

fn group_named(m: &Model, name: &str) -> Id {
    *m.groups.iter().find(|(_, g)| g.name == name).unwrap_or_else(|| panic!("no group {name}")).0
}

fn faces_in(m: &Model, g: Id) -> usize {
    m.faces.values().filter(|f| f.group == Some(g)).count()
}

/// Closed and consistent: Some(volume) for a solid.
fn volume(m: &Model, g: Id) -> Option<f64> {
    sketchcraft::csg::solid_info(&mut m.clone(), g)
}

fn size(m: &Model) -> (f64, f64, f64, f64) {
    let (lo, hi) = m.bounds().unwrap();
    (hi.x - lo.x, hi.y - lo.y, hi.z - lo.z, lo.z)
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn the_set_is_listed_and_places_like_any_asset() {
    let mut e = Engine::new();
    let list = run(&mut e, json!({"cmd":"assets"}))["assets"].as_array().unwrap().clone();
    for key in ["dice", "can", "globe", "crate", "soccer_ball", "beaver"] {
        let a = list.iter().find(|a| a["key"] == key).unwrap_or_else(|| panic!("{key} listed"));
        assert!(a["about"].as_str().unwrap().contains(" m"), "{key} says its size");
        run(&mut e, json!({"cmd":"insert","asset":key,"at":[1.0, 2.0, 0.0]}));
    }
    assert_eq!(sketchcraft::assets::find("Soccer ball").unwrap().key, "soccer_ball");
}

#[test]
fn dice_is_a_closed_5_cm_cube_of_six_faces() {
    let m = build("dice");
    assert_eq!(m.faces.len(), 6);
    let (w, d, h, z0) = size(&m);
    assert!(near(w, 0.05, 1e-6) && near(d, 0.05, 1e-6) && near(h, 0.05, 1e-6) && near(z0, 0.0, 1e-9), "{w} {d} {h} {z0}");
    let v = volume(&m, group_named(&m, "Dice")).expect("a closed solid");
    assert!(near(v, 0.05f64.powi(3), 1e-9), "{v}");
    assert!(m.faces.values().all(|f| f.material.as_deref() == Some("Ivory plastic")));
}

#[test]
fn can_is_a_closed_32_sided_cylinder_with_two_caps() {
    let m = build("can");
    assert_eq!(m.faces.len(), 34, "32 sides and two caps");
    let (w, d, h, z0) = size(&m);
    assert!(near(w, 0.066, 1e-3) && near(d, 0.066, 1e-3) && near(h, 0.122, 1e-6) && near(z0, 0.0, 1e-9), "{w} {d} {h}");
    let g = group_named(&m, "Can");
    let v = volume(&m, g).expect("a closed solid");
    let ideal = std::f64::consts::PI * 0.033f64.powi(2) * 0.122;
    assert!(v < ideal && v > ideal * 0.99, "{v} vs {ideal}");
    let caps = m.faces.values().filter(|f| f.outer.len() == 32).count();
    assert_eq!(caps, 2);
    // the sides are soft, so they shade as one curved surface
    assert!(m.edges.values().filter(|e| e.soft).count() >= 32, "soft side edges");
}

#[test]
fn globe_is_a_faceted_sphere_on_a_stand() {
    let m = build("globe");
    let sphere = group_named(&m, "Sphere");
    // 16 segments x 10 rings, with triangles in the two polar rows
    assert_eq!(faces_in(&m, sphere), 16 * 10);
    let tris = m.faces.values().filter(|f| f.group == Some(sphere) && f.outer.len() == 3).count();
    assert_eq!(tris, 32);
    let v = volume(&m, sphere).expect("a closed sphere");
    let ideal = 4.0 / 3.0 * std::f64::consts::PI * 0.15f64.powi(3);
    assert!(v < ideal && v > ideal * 0.9, "{v} vs {ideal}");
    assert!(volume(&m, group_named(&m, "Base")).is_some());
    assert!(volume(&m, group_named(&m, "Stem")).is_some());
    let (w, d, h, z0) = size(&m);
    assert!(near(w, 0.3, 1e-3) && near(d, 0.3, 1e-3) && near(h, 0.35, 1e-3) && near(z0, 0.0, 1e-9), "{w} {d} {h}");
}

#[test]
fn crate_is_separate_closed_planks_and_posts() {
    let m = build("crate");
    let tops: Vec<_> = m.groups.values().filter(|g| g.parent.is_none()).collect();
    assert_eq!(tops.len(), 1);
    assert_eq!(tops[0].name, "Crate");
    let parts: Vec<Id> = m.groups.iter().filter(|(_, g)| g.parent.is_some()).map(|(&i, _)| i).collect();
    // 4 posts, 4 sides x 5 planks, floor and lid x 5 boards
    assert_eq!(parts.len(), 4 + 20 + 10);
    for &p in &parts {
        assert_eq!(faces_in(&m, p), 6, "{} is a box", m.groups[&p].name);
        assert!(volume(&m, p).is_some(), "{} is closed", m.groups[&p].name);
    }
    let (w, d, h, z0) = size(&m);
    assert!(near(w, 0.6, 1e-6) && near(d, 0.6, 1e-6) && near(h, 0.6, 1e-6) && near(z0, 0.0, 1e-9), "{w} {d} {h}");
    let mats: std::collections::BTreeSet<_> = m.faces.values().filter_map(|f| f.material.clone()).collect();
    assert_eq!(mats.into_iter().collect::<Vec<_>>(), vec!["Pine".to_string(), "Pine dark".to_string()]);
}

#[test]
fn soccer_ball_is_a_truncated_icosahedron() {
    let m = build("soccer_ball");
    assert_eq!(m.faces.len(), 32, "12 pentagons and 20 hexagons");
    assert_eq!(m.faces.values().filter(|f| f.outer.len() == 5).count(), 12);
    assert_eq!(m.faces.values().filter(|f| f.outer.len() == 6).count(), 20);
    assert_eq!(m.vertices.len(), 60);
    assert_eq!(m.edges.len(), 90);
    // pentagons are black, hexagons white
    assert!(m.faces.values().all(|f| f.material.as_deref() == Some(if f.outer.len() == 5 { "Ball black" } else { "Ball white" })));
    let g = group_named(&m, "Soccer ball");
    assert_eq!(faces_in(&m, g), 32);
    let v = volume(&m, g).expect("a closed solid");
    let ideal = 4.0 / 3.0 * std::f64::consts::PI * 0.11f64.powi(3);
    assert!(v < ideal && v > ideal * 0.85, "{v} vs {ideal}");
    let (w, d, h, z0) = size(&m);
    assert!(near(z0, 0.0, 1e-6), "rests on the floor: {z0}");
    assert!(h > 0.2 && h < 0.221 && w < 0.221 && d < 0.221, "about 22 cm across: {w} {d} {h}");
    // every face points out of the ball
    let (lo, hi) = m.bounds().unwrap();
    let c = (lo + hi) / 2.0;
    for (id, f) in &m.faces {
        let pts: Vec<_> = f.outer.iter().map(|v| m.vertices[v].p).collect();
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        let mid = pts.iter().fold(glam::DVec3::ZERO, |a, p| a + *p) / pts.len() as f64;
        assert!(n.dot(mid - c) > 0.0, "face {id} points inward (n={n}, mid-c={})", mid - c);
    }
}

#[test]
fn every_convex_piece_faces_outward() {
    // each face of a convex group points away from the middle of the group's box (so shading, paint-through and export agree)
    for (key, only) in [("dice", "Dice"), ("can", "Can"), ("soccer_ball", "Soccer ball"), ("globe", "Sphere"), ("globe", "Base"), ("globe", "Stem")] {
        let m = build(key);
        let g = group_named(&m, only);
        let faces: Vec<_> = m.faces.iter().filter(|(_, f)| f.group == Some(g)).collect();
        let pts = |f: &sketchcraft::model::Face| f.outer.iter().map(|v| m.vertices[v].p).collect::<Vec<_>>();
        let all: Vec<_> = faces.iter().flat_map(|(_, f)| pts(f)).collect();
        let (lo, hi) = all.iter().fold((all[0], all[0]), |(l, h), p| (l.min(*p), h.max(*p)));
        let c = (lo + hi) / 2.0;
        for (id, f) in faces {
            let p = pts(f);
            let mut n = glam::DVec3::ZERO;
            for i in 0..p.len() {
                n += p[i].cross(p[(i + 1) % p.len()]); // Newell's method: right for any polygon
            }
            let mid = p.iter().fold(glam::DVec3::ZERO, |a, q| a + *q) / p.len() as f64;
            assert!(n.dot(mid - c) > 0.0, "{key}/{only}: face {id} points inward");
        }
    }
    let m = build("crate");
    for (&g, grp) in m.groups.iter().filter(|(_, g)| g.parent.is_some()) {
        let pts: Vec<_> = m.faces.values().filter(|f| f.group == Some(g)).flat_map(|f| f.outer.iter().map(|v| m.vertices[v].p)).collect();
        let (lo, hi) = pts.iter().fold((pts[0], pts[0]), |(l, h), p| (l.min(*p), h.max(*p)));
        let c = (lo + hi) / 2.0;
        for (id, f) in m.faces.iter().filter(|(_, f)| f.group == Some(g)) {
            let p: Vec<_> = f.outer.iter().map(|v| m.vertices[v].p).collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let mid = p.iter().fold(glam::DVec3::ZERO, |a, q| a + *q) / p.len() as f64;
            assert!(n.dot(mid - c) > 0.0, "crate/{}: face {id} points inward", grp.name);
        }
    }
}

#[test]
fn all_six_are_valid_for_every_export() {
    let dir = std::env::temp_dir().join(format!("sketchcraft-texset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut e = Engine::new();
    for (i, key) in ["dice", "can", "globe", "crate", "soccer_ball", "beaver"].iter().enumerate() {
        run(&mut e, json!({"cmd":"insert","asset":key,"at":[i as f64, 0.0, 0.0]}));
    }
    let p = dir.join("set.glb").to_string_lossy().to_string();
    let r = run(&mut e, json!({"cmd":"export_glb","path":p}));
    assert!(r["triangles"].as_u64().unwrap() > 500);
    std::fs::remove_dir_all(dir).ok();
}
