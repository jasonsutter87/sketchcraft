//! Drafting (#62): the display list, vector drawings, hidden lines, PDF/SVG/DXF output.
//!
//! Golden pages live in tests/golden/*.svg. After a change you meant to make, rewrite them with
//! `SC_UPDATE_GOLDEN=1 cargo test -p sketchcraft-core --test drafting`, and look at the diff.
//! `SC_DRAFT_OUT=<dir>` also writes every fixture page there (SVG and one PDF) to look at.

#[path = "pdftext/mod.rs"]
mod pdftext;

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::draft::{self, ir::*};
use sketchcraft::layout::{PageSpec, ViewportSpec};
use sketchcraft::{Engine, Model};

fn run(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn box_group(e: &mut Engine, from: [f64; 3], to: [f64; 3], name: &str) -> u64 {
    let g = run(e, json!({"cmd":"begin_group","name":name}))["group"].as_u64().unwrap();
    let f = run(e, json!({"cmd":"rect","from":[from[0],from[1],from[2]],"to":[to[0],to[1],from[2]]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    let h = to[2] - from[2];
    run(e, json!({"cmd":"pushpull","face":f,"distance": if up { h } else { -h }}));
    run(e, json!({"cmd":"end_group"}));
    g
}

/// Two storeys: walls on the ground, a slab at 3 m, walls on it, a door and a window.
fn house() -> Engine {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"wall","points":[[0,0,0],[6,0,0],[6,8,0],[0,8,0]],"closed":true,"height":2.8}));
    box_group(&mut e, [0.0, 0.0, 2.8], [6.0, 8.0, 3.0], "Floor");
    run(&mut e, json!({"cmd":"wall","points":[[0,0,3],[6,0,3],[6,8,3],[0,8,3]],"closed":true,"height":2.6}));
    run(&mut e, json!({"cmd":"insert","asset":"door","at":[1.0,-0.1,0.0]}));
    run(&mut e, json!({"cmd":"insert","asset":"window","at":[3.5,-0.1,0.9]}));
    e
}

/// One room with its dimensions, a label and a door swing.
fn dimensioned() -> Engine {
    let mut e = Engine::new();
    run(&mut e, json!({"cmd":"wall","points":[[0,0,0],[5,0,0],[5,4,0],[0,4,0]],"closed":true,"height":2.5}));
    run(&mut e, json!({"cmd":"dimension","from":[0,0,0],"to":[5,0,0],"offset":[0,-1,0]}));
    run(&mut e, json!({"cmd":"dimension","from":[5,0,0],"to":[5,4,0],"offset":[1,0,0]}));
    run(&mut e, json!({"cmd":"dimension","from":[0,0,0],"to":[0,4,0],"offset":[-1,0,0]}));
    run(&mut e, json!({"cmd":"text","at":[2.5,2,0],"text":"Studio\n20 m2"}));
    run(&mut e, json!({"cmd":"text","at":[4,3,0],"text":"Desk","leader":[4.6,3.5,0]}));
    run(&mut e, json!({"cmd":"door_swing","hinge":[1,0,0.02],"strike":[2,0,0.02],"toward":[1.5,3,0]}));
    e
}

fn vp(view: &str, scale: &str) -> ViewportSpec {
    ViewportSpec { view: view.into(), scale: Some(scale.into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() }
}

fn page(title: &str, vps: Vec<ViewportSpec>) -> PageSpec {
    PageSpec { title: title.into(), viewports: vps, table: vec![], ..Default::default() }
}

fn doc(m: &Model, sheet: &str, pages: Vec<PageSpec>, blueprint: bool) -> Doc {
    draft::sheet::build(m, sheet, "Fixture", pages, blueprint, &[], "2026-11-17").unwrap()
}

fn plan_vp(scale: &str) -> ViewportSpec {
    ViewportSpec { cut: Some(1.22), floor: Some(0.0), callouts: true, ..vp("top", scale) }
}

fn elevation_vp(scale: &str) -> ViewportSpec {
    ViewportSpec { callouts: true, levels: vec![("GROUND".into(), 0.0), ("FLOOR 2".into(), 3.0), ("TOP".into(), 5.6)], ..vp("front", scale) }
}

fn section_vp(scale: &str) -> ViewportSpec {
    ViewportSpec { callouts: true, section: Some([3.0, 4.0, 0.0, 1.0, 0.0, 0.0]), ..vp("section", scale) }
}

// ---- golden pages -------------------------------------------------------------------------

/// Numbers within `tol`, everything else exactly: the same drawing from another platform's maths.
fn same_svg(a: &str, b: &str, tol: f64) -> Result<(), String> {
    fn tokens(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = i;
            let num = b[i].is_ascii_digit() || (b[i] == b'-' && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()));
            if num {
                i += 1;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
            } else {
                while i < b.len() && !(b[i].is_ascii_digit() || (b[i] == b'-' && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()))) {
                    i += 1;
                }
            }
            out.push((num, s[start..i].to_string()));
        }
        out
    }
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.len() != tb.len() {
        return Err(format!("{} tokens against {}", ta.len(), tb.len()));
    }
    for (i, (x, y)) in ta.iter().zip(&tb).enumerate() {
        let ok = if x.0 && y.0 { (x.1.parse::<f64>().unwrap() - y.1.parse::<f64>().unwrap()).abs() <= tol } else { x == y };
        if !ok {
            let ctx: String = ta[i.saturating_sub(4)..(i + 2).min(ta.len())].iter().map(|t| t.1.as_str()).collect();
            return Err(format!("token {i}: {:?} against {:?}, near {ctx:?}", x.1, y.1));
        }
    }
    Ok(())
}

fn golden(name: &str, svg: &str) {
    let path = format!("{}/tests/golden/{name}.svg", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("SC_UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, svg).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is missing: write it with SC_UPDATE_GOLDEN=1"));
    if let Err(why) = same_svg(&want, svg, 0.06) {
        let new = format!("{}.new", path);
        std::fs::write(&new, svg).ok();
        panic!("{name} no longer matches its golden page ({why}); the new page is in {new}. If the change is meant: SC_UPDATE_GOLDEN=1");
    }
    std::fs::remove_file(format!("{path}.new")).ok();
}

fn pages_svg(d: &Doc) -> Vec<String> {
    d.pages.iter().map(draft::svg::page).collect()
}

#[test]
fn golden_plan() {
    let e = house();
    golden("plan", &pages_svg(&doc(&e.model, "a3", vec![page("Ground floor plan", vec![plan_vp("1:75")])], false))[0]);
}

#[test]
fn golden_elevation() {
    let e = house();
    golden("elevation", &pages_svg(&doc(&e.model, "a3", vec![page("Front elevation", vec![elevation_vp("1:75")])], false))[0]);
}

#[test]
fn golden_section() {
    let e = house();
    golden("section", &pages_svg(&doc(&e.model, "a3", vec![page("Section A", vec![section_vp("1:75")])], false))[0]);
}

#[test]
fn golden_parallel_3d() {
    let e = house();
    golden("view3d", &pages_svg(&doc(&e.model, "a3", vec![page("Isometric", vec![ViewportSpec { beyond: true, ..vp("iso", "1:100") }])], false))[0]);
}

#[test]
fn golden_blueprint_page() {
    let e = house();
    let (pages, notes) = sketchcraft::layout::blueprint_specs(&e.model, "a3", &[]).unwrap();
    let d = draft::sheet::build(&e.model, "a3", "Fixture", pages, true, &notes, "2026-11-17").unwrap();
    assert!(d.pages.len() >= 3, "plans, elevations, a section");
    assert_eq!(d.pages[0].bg, Some(BP_PAPER), "white on blueprint blue");
    golden("blueprint", &pages_svg(&d)[0]);
}

#[test]
fn golden_dimensions() {
    let e = dimensioned();
    golden("dimensions", &pages_svg(&doc(&e.model, "a3", vec![page("Studio", vec![plan_vp("1:50")])], false))[0]);
}

#[test]
fn look_at_fixtures() {
    let Ok(dir) = std::env::var("SC_DRAFT_OUT") else { return };
    let e = house();
    let d = doc(
        &e.model,
        "a3",
        vec![
            page("Plan", vec![plan_vp("1:75")]),
            page("Elevation", vec![elevation_vp("1:75")]),
            page("Section", vec![section_vp("1:75")]),
            page("Iso", vec![ViewportSpec { beyond: true, ..vp("iso", "1:100") }]),
        ],
        false,
    );
    for (i, p) in d.pages.iter().enumerate() {
        std::fs::write(format!("{dir}/p{i}.svg"), draft::svg::page(p)).unwrap();
    }
    std::fs::write(format!("{dir}/set.pdf"), draft::pdf::write(&d).unwrap()).unwrap();
    let (pages, notes) = sketchcraft::layout::blueprint_specs(&e.model, "a3", &[]).unwrap();
    let bp = draft::sheet::build(&e.model, "a3", "Fixture", pages, true, &notes, "2026-11-17").unwrap();
    std::fs::write(format!("{dir}/blueprint.pdf"), draft::pdf::write(&bp).unwrap()).unwrap();
    // what these drawings were before: the picture the old sheets put on the page (6 px per mm)
    let (lo, hi) = e.model.world_bounds().unwrap();
    let center = (lo + hi) / 2.0;
    let (w, h) = (260.0, 170.0);
    let px = 6.0;
    let base = sketchcraft::render::RenderOpts { paper: true, grid: false, annotations: false, parallel: true, ortho_height: Some(h * 75.0 / 1000.0), width: (w * px) as u32, height: (h * px) as u32, ..Default::default() };
    let eye = center + DVec3::new(0.0, -1.0, 0.0) * ((hi - lo).length() + 10.0);
    let elev = sketchcraft::render::RenderOpts { camera: Some(sketchcraft::render::CameraSpec { eye: eye.to_array(), target: center.to_array(), fov: 40.0 }), ..base.clone() };
    sketchcraft::render::render(&e.model, &elev).save(format!("{dir}/before_elevation.png")).unwrap();
    let look = DVec3::X;
    let eye = center - look * ((hi - lo).length() + 10.0);
    let sec = sketchcraft::render::RenderOpts { camera: Some(sketchcraft::render::CameraSpec { eye: eye.to_array(), target: center.to_array(), fov: 40.0 }), clip: Some([3.0, 4.0, 0.0, 1.0, 0.0, 0.0]), ..base };
    sketchcraft::render::render(&e.model, &sec).save(format!("{dir}/before_section.png")).unwrap();
    let dm = dimensioned();
    let d2 = doc(&dm.model, "a3", vec![page("Studio", vec![plan_vp("1:50")])], false);
    std::fs::write(format!("{dir}/dims.svg"), draft::svg::page(&d2.pages[0])).unwrap();
    std::fs::write(format!("{dir}/dims.dxf"), draft::dxf::page(&d2.pages[0])).unwrap();
    std::fs::write(format!("{dir}/plan.dxf"), draft::dxf::page(&d.pages[0])).unwrap();
}

// ---- hidden lines -----------------------------------------------------------------------------

/// A box in front of a box: A at x 0..2, y -3..-1; B behind it at x 1..4, y 1..3; both 2 m high.
fn box_behind_box() -> Engine {
    let mut e = Engine::new();
    box_group(&mut e, [0.0, -3.0, 0.0], [2.0, -1.0, 2.0], "A");
    box_group(&mut e, [1.0, 1.0, 0.0], [4.0, 3.0, 2.0], "B");
    e
}

struct Span {
    a: [f64; 2],
    b: [f64; 2],
    dashed: bool,
}

/// The spans of the model edge between two vertices (found by where they are), as drawn.
fn spans_of(m: &Model, d: &Doc, from: DVec3, to: DVec3) -> Vec<Span> {
    let mut out = Vec::new();
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(pp) = p {
            let Some(pv) = &pp.prov else { return };
            let ids = pv.edge.v;
            let (a, b) = (m.p(ids[0]), m.p(ids[1]));
            let hit = |p: DVec3, q: DVec3| p.distance(q) < 1e-9;
            if (hit(a, from) && hit(b, to)) || (hit(a, to) && hit(b, from)) {
                if let [Seg::M(p), Seg::L(q)] = pp.segs[..] {
                    out.push(Span { a: p, b: q, dashed: !pp.pen.as_ref().unwrap().dash.is_empty() });
                }
            }
        }
    });
    out
}

#[test]
fn hidden_lines_exact_to_a_twentieth_of_a_millimeter() {
    let e = box_behind_box();
    // front view, 1:100 (10 mm per m), pinned at (2, 0, 1): the model point (x, ., z) lands at
    // paper (120 + (x - 2) * 10, 70 - (z - 1) * 10)
    let v = ViewportSpec { anchor: Some([2.0, 0.0, 1.0]), beyond: true, callouts: true, x: 20.0, y: 20.0, w: 200.0, h: 100.0, ..vp("front", "1:100") };
    let d = doc(&e.model, "a3", vec![page("Hidden", vec![v])], false);
    let paper = |x: f64, z: f64| [120.0 + (x - 2.0) * 10.0, 70.0 - (z - 1.0) * 10.0];
    let tol = 0.05;
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).abs() <= tol && (p[1] - q[1]).abs() <= tol;
    // B's front-bottom and front-top edges (y = 1) run x 1..4: behind A until x = 2
    for z in [0.0, 2.0] {
        let spans = spans_of(&e.model, &d, DVec3::new(1.0, 1.0, z), DVec3::new(4.0, 1.0, z));
        let (hidden, shown): (Vec<_>, Vec<_>) = spans.iter().partition(|s| s.dashed);
        assert_eq!((hidden.len(), shown.len()), (1, 1), "one hidden span and one visible span at z={z}");
        let (h, s) = (hidden[0], shown[0]);
        let (h0, h1) = if h.a[0] < h.b[0] { (h.a, h.b) } else { (h.b, h.a) };
        let (s0, s1) = if s.a[0] < s.b[0] { (s.a, s.b) } else { (s.b, s.a) };
        assert!(near(h0, paper(1.0, z)), "hidden span starts at the corner: {h0:?}");
        assert!(near(h1, paper(2.0, z)), "hidden span ends where A's side ends (x=2): {h1:?} want {:?}", paper(2.0, z));
        assert!(near(s0, paper(2.0, z)), "visible span starts there: {s0:?}");
        assert!(near(s1, paper(4.0, z)), "and runs to the far corner: {s1:?}");
    }
    // B's left vertical edge (x = 1) is wholly behind A, its right one (x = 4) wholly in the clear
    let left = spans_of(&e.model, &d, DVec3::new(1.0, 1.0, 0.0), DVec3::new(1.0, 1.0, 2.0));
    assert!(!left.is_empty() && left.iter().all(|s| s.dashed), "x=1 is hidden");
    let right = spans_of(&e.model, &d, DVec3::new(4.0, 1.0, 0.0), DVec3::new(4.0, 1.0, 2.0));
    assert!(!right.is_empty() && right.iter().all(|s| !s.dashed), "x=4 is visible");
    // A, in front, is never hidden
    for (a, b) in [(DVec3::new(0.0, -3.0, 0.0), DVec3::new(2.0, -3.0, 0.0)), (DVec3::new(0.0, -3.0, 2.0), DVec3::new(2.0, -3.0, 2.0))] {
        let s = spans_of(&e.model, &d, a, b);
        assert!(!s.is_empty() && s.iter().all(|s| !s.dashed));
    }
    // without "beyond", the hidden spans simply aren't drawn
    let v2 = ViewportSpec { beyond: false, ..ViewportSpec { anchor: Some([2.0, 0.0, 1.0]), callouts: true, x: 20.0, y: 20.0, w: 200.0, h: 100.0, ..vp("front", "1:100") } };
    let d2 = doc(&e.model, "a3", vec![page("Hidden", vec![v2])], false);
    assert!(spans_of(&e.model, &d2, DVec3::new(1.0, 1.0, 1.0 - 1.0), DVec3::new(4.0, 1.0, 0.0)).iter().all(|s| !s.dashed));
    assert!(spans_of(&e.model, &d2, DVec3::new(1.0, 1.0, 0.0), DVec3::new(1.0, 1.0, 2.0)).is_empty(), "x=1 isn't drawn at all");
}

#[test]
fn a_post_narrower_than_the_old_sample_spacing_still_hides_the_line_behind_it() {
    // a wall 10 m long, and a 0.15 m post in front of it: 1.5 mm on the paper at 1:100
    let mut e = Engine::new();
    box_group(&mut e, [0.0, 2.0, 0.0], [10.0, 3.0, 3.0], "Wall");
    box_group(&mut e, [5.0, -2.0, 0.0], [5.15, -1.85, 3.0], "Post");
    let v = ViewportSpec { anchor: Some([5.0, 0.0, 1.5]), beyond: true, callouts: true, x: 20.0, y: 20.0, w: 200.0, h: 100.0, ..vp("front", "1:100") };
    let d = doc(&e.model, "a3", vec![page("Post", vec![v])], false);
    for z in [0.0, 3.0] {
        let spans = spans_of(&e.model, &d, DVec3::new(0.0, 2.0, z), DVec3::new(10.0, 2.0, z));
        let hidden: Vec<&Span> = spans.iter().filter(|s| s.dashed).collect();
        assert_eq!(hidden.len(), 1, "z={z}: the post hides one stretch of the wall's edge");
        let w = (hidden[0].a[0] - hidden[0].b[0]).abs();
        assert!((w - 1.5).abs() < 0.1, "z={z}: hidden stretch is {w} mm, want 1.5");
        let mid = (hidden[0].a[0] + hidden[0].b[0]) / 2.0;
        assert!((mid - (120.0 + 0.075 * 10.0)).abs() < 0.1, "z={z}: centred on the post: {mid}");
    }
}

#[test]
fn schedules_are_tables_and_figures_say_what_is_in_them() {
    let e = house();
    let mut spec = page("Door schedule", vec![plan_vp("1:75")]);
    spec.table = vec![vec!["Mark".into(), "Width".into()], vec!["D1".into(), "0.9 m".into()], vec!["W1".into(), "1.2 m".into()]];
    spec.viewports[0].y = 80.0;
    spec.viewports[0].h = 100.0;
    let mut d = doc(&e.model, "a3", vec![spec], false);
    d.lang = "de".into();
    let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
    let count = |s: &str| pdf.objs.values().filter(|o| o.dict.contains(s)).count();
    assert_eq!(count("/S /Table"), 1);
    assert_eq!(count("/S /TR"), 3, "a row each");
    assert_eq!(count("/S /TH"), 4, "the header row and the first column head their rows and columns");
    assert_eq!(count("/S /TD"), 2);
    assert!(pdf.objs.values().any(|o| o.dict.contains("/Lang (de)")), "the language the document was written in");
    let fig = pdf.objs.values().find(|o| o.dict.contains("/S /Figure")).unwrap();
    assert!(fig.dict.contains("Marked on it") && fig.dict.contains("D1"), "the figure's description lists what's marked on the plan: {}", fig.dict);
}

#[test]
fn line_work_remembers_where_it_came_from() {
    let e = box_behind_box();
    let v = ViewportSpec { anchor: Some([2.0, 0.0, 1.0]), callouts: true, x: 20.0, y: 20.0, w: 200.0, h: 100.0, ..vp("front", "1:100") };
    let d = doc(&e.model, "a3", vec![page("Prov", vec![v])], false);
    let mut with = 0;
    let mut without = 0;
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(pp) = p {
            match &pp.prov {
                Some(pv) => {
                    with += 1;
                    assert!(pv.edge.path.is_empty(), "these are root-model edges");
                    assert!(pv.t[0] >= 0.0 && pv.t[1] <= 1.0 && pv.t[0] < pv.t[1]);
                    assert!(e.model.vertices.contains_key(&pv.edge.v[0]) && e.model.vertices.contains_key(&pv.edge.v[1]));
                }
                None => without += 1,
            }
        }
    });
    assert!(with >= 7, "the boxes' visible edges carry their vertex ids: {with}");
    assert!(without > 0, "the frame and title block don't");
}

// ---- drawings stay put ------------------------------------------------------------------------

#[test]
fn moving_something_far_away_leaves_the_page_unchanged() {
    // pinned drawings: a plan, an elevation and a section, each with its own anchor
    let pinned = |m: &Model| -> Vec<String> {
        let a = Some([3.0, 4.0, 1.5]);
        let pages = vec![
            // (a plan's automatic dimensions and tags are worked out from the whole building, so a pinned plan
            // that must not move asks for none)
            page("Plan", vec![ViewportSpec { anchor: a, callouts: false, ..plan_vp("1:75") }]),
            page("Elevation", vec![ViewportSpec { anchor: a, ..elevation_vp("1:75") }]),
            page("Section", vec![ViewportSpec { anchor: a, ..section_vp("1:75") }]),
            page("Iso", vec![ViewportSpec { anchor: a, ..vp("iso", "1:100") }]),
        ];
        pages_svg(&doc(m, "a3", pages, false))
    };
    let mut e = house();
    let far = box_group(&mut e, [900.0, 700.0, 0.0], [902.0, 702.0, 6.0], "Far");
    let before = pinned(&e.model);
    run(&mut e, json!({"cmd":"move","ids":[far],"by":[-3000.0, 1500.0, 0.0]}));
    let after = pinned(&e.model);
    for (i, (a, b)) in before.iter().zip(&after).enumerate() {
        if a != b {
            let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
            std::fs::write("/private/tmp/claude-501/draft-out/far_a.svg", a).ok();
            std::fs::write("/private/tmp/claude-501/draft-out/far_b.svg", b).ok();
            let k = la.iter().zip(&lb).position(|(x, y)| x != y).unwrap_or(la.len().min(lb.len()));
            panic!("page {i} changed when a far-away object moved: line {k}: {:?} against {:?}", la.get(k), lb.get(k));
        }
    }
    // and without it at all, the same again
    run(&mut e, json!({"cmd":"delete","ids":[far]}));
    assert_eq!(before, pinned(&e.model));
    // what is near does change the drawing
    box_group(&mut e, [2.0, 2.0, 0.0], [3.0, 3.0, 1.0], "Near");
    assert_ne!(before, pinned(&e.model));
}

#[test]
fn adding_a_tree_does_not_move_a_drawing() {
    // not pinned: the drawing fits itself to the building when made, and the environment isn't the building
    let draw = |m: &Model| -> Vec<String> {
        let pages = vec![page("Plan", vec![plan_vp("1:75")]), page("Elevation", vec![elevation_vp("1:75")]), page("Section", vec![section_vp("1:75")])];
        pages_svg(&doc(m, "a3", pages, false))
    };
    let mut e = house();
    let before = draw(&e.model);
    let blueprint_before = format!("{:?}", sketchcraft::layout::blueprint_specs(&e.model, "a3", &[]).unwrap());
    // a tree, a long way off, in the environment group
    let g = run(&mut e, json!({"cmd":"begin_group","name":"Environment: trees"}))["group"].as_u64().unwrap();
    let _ = g;
    let f = run(&mut e, json!({"cmd":"rect","from":[400,400,0],"to":[401,401,0]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    run(&mut e, json!({"cmd":"pushpull","face":f,"distance": if up { 9.0 } else { -9.0 }}));
    run(&mut e, json!({"cmd":"end_group"}));
    assert_eq!(before, draw(&e.model), "a tree that far off changes nothing on the pages");
    // nor in a blueprint set: its scales and the section's place come from the building
    assert_eq!(blueprint_before, format!("{:?}", sketchcraft::layout::blueprint_specs(&e.model, "a3", &[]).unwrap()));
}

// ---- paper sizes and scales -----------------------------------------------------------------

#[test]
fn paper_sizes() {
    use sketchcraft::draft::paper::sheet_mm;
    let close = |(a, b): (f64, f64), (c, d): (f64, f64)| (a - c).abs() < 0.05 && (b - d).abs() < 0.05;
    assert!(close(sheet_mm("a1").unwrap(), (841.0, 594.0)));
    assert!(close(sheet_mm("A0").unwrap(), (1189.0, 841.0)));
    assert!(close(sheet_mm("ARCH C").unwrap(), (609.6, 457.2)), "18 x 24 in");
    assert!(close(sheet_mm("arch-d").unwrap(), (914.4, 609.6)), "24 x 36 in");
    assert!(close(sheet_mm("archE").unwrap(), (1219.2, 914.4)), "36 x 48 in");
    assert!(close(sheet_mm("a4").unwrap(), (297.0, 210.0)) && close(sheet_mm("tabloid").unwrap(), (431.8, 279.4)));
    assert!(sheet_mm("a5").is_err());
    // and the page is that big, in the PDF too (points)
    let e = house();
    for (name, w, h) in [("a1", 841.0, 594.0), ("a0", 1189.0, 841.0), ("arch c", 609.6, 457.2), ("arch d", 914.4, 609.6), ("arch e", 1219.2, 914.4)] {
        let d = doc(&e.model, name, vec![page("x", vec![ViewportSpec { w: 100.0, h: 80.0, x: 20.0, y: 20.0, ..vp("front", "1:100") }])], false);
        assert_eq!((d.pages[0].w, d.pages[0].h), (w, h));
        let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
        let mb = pdf.media_box(pdf.pages()[0]);
        assert!((mb[2] - w * PT).abs() < 0.02 && (mb[3] - h * PT).abs() < 0.02, "{name}: {mb:?}");
    }
}

#[test]
fn imperial_and_metric_scales() {
    use sketchcraft::draft::paper::{scale_label, scale_of};
    assert_eq!(scale_of("1:100").unwrap(), 100.0);
    assert_eq!(scale_of("1:48").unwrap(), 48.0);
    assert_eq!(scale_of("1/4\"=1'-0\"").unwrap(), 48.0, "quarter inch scale");
    assert_eq!(scale_of("1/8\" = 1'-0\"").unwrap(), 96.0);
    assert_eq!(scale_of("3/16\"=1'").unwrap(), 64.0);
    assert_eq!(scale_of("1/2\"=1'-0\"").unwrap(), 24.0);
    assert_eq!(scale_of("1\"=1'-0\"").unwrap(), 12.0);
    assert_eq!(scale_of("3\" = 1'-0\"").unwrap(), 4.0);
    assert_eq!(scale_of("1\"=20'").unwrap(), 240.0, "civil");
    assert_eq!(scale_of("1 in = 10 ft").unwrap(), 120.0);
    assert_eq!(scale_of("1/4in=1ft").unwrap(), 48.0);
    assert!((scale_of("1 1/2\"=1'-0\"").unwrap() - 8.0).abs() < 1e-9);
    assert_eq!(scale_of("2:1").unwrap(), 0.5);
    assert_eq!(scale_of("100").unwrap(), 100.0, "a bare denominator");
    assert_eq!(scale_of(" 50 ").unwrap(), 50.0);
    for bad in ["one to fifty", "1:0", "0:1", "=", "1/4\"=", "1:-3", "1/0\"=1'"] {
        assert!(scale_of(bad).is_err(), "{bad:?}");
    }
    assert_eq!(scale_label("1:100", 100.0), "1:100");
    assert_eq!(scale_label("1/4\"=1'-0\"", 48.0), "1/4\"=1'-0\"");
    // a drawing at 1/4" = 1'-0" is the 1:48 drawing: the front is 6.2 m wide, 129.17 mm on paper
    let e = house();
    let width = |scale: &str| -> f64 {
        let d = doc(&e.model, "arch d", vec![page("x", vec![ViewportSpec { w: 300.0, h: 200.0, anchor: Some([3.0, 4.0, 2.8]), ..vp("front", scale) }])], false);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        d.pages[0].walk(&mut |p| {
            if let Prim::Path(pp) = p {
                if pp.prov.is_some() {
                    for s in &pp.segs {
                        if let Seg::M(a) | Seg::L(a) = s {
                            lo = lo.min(a[0]);
                            hi = hi.max(a[0]);
                        }
                    }
                }
            }
        });
        hi - lo
    };
    let (imperial, metric) = (width("1/4\"=1'-0\""), width("1:48"));
    assert!((imperial - metric).abs() < 1e-9);
    assert!((metric - 6.2 * 1000.0 / 48.0).abs() < 0.1, "{metric}");
    // a very fine scale still gets a short, readable scale bar label
    for scale in ["3\"=1'-0\"", "1:5", "1:2"] {
        let d = doc(&e.model, "arch d", vec![page("x", vec![vp("front", scale)])], false);
        for t in d.pages[0].texts() {
            assert!(!t.chars().collect::<Vec<_>>().windows(5).any(|w| w.iter().all(|c| c.is_ascii_digit() || *c == '.')), "{scale}: {t:?}");
        }
    }
    // the caption says it the way it was asked for
    let d = doc(&e.model, "arch d", vec![page("x", vec![vp("front", "1/4\"=1'-0\"")])], false);
    assert!(d.pages[0].texts().iter().any(|t| t.contains("1/4\"=1'-0\"")), "{:?}", d.pages[0].texts());
}

// ---- the PDF -------------------------------------------------------------------------------

#[test]
fn pdf_structure() {
    let e = house();
    let mut spec = elevation_vp("1:75");
    spec.label = Some("Façade sud — Größe ½ ± 5° ł Ž".into());
    let mut d = doc(&e.model, "a3", vec![page("Elevation", vec![spec]), page("Plan", vec![plan_vp("1:75")])], false);
    // a table with accents and a character neither bundled subset has
    d.pages[1].items.push(Prim::Text(TextPrim { at: [30.0, 200.0], size: 4.0, bold: false, rgb: BLACK, angle: 0.0, text: "Tür: 0.9 × 2.1 m, 寸法".into(), role: Role::Paragraph }));
    let bytes = draft::pdf::write(&d).unwrap();
    assert!(bytes.starts_with(b"%PDF-1."));
    let pdf = pdftext::parse(&bytes);
    // pages
    let pages = pdf.pages();
    assert_eq!(pages.len(), 2);
    for p in &pages {
        let mb = pdf.media_box(*p);
        assert!((mb[2] - 420.0 * PT).abs() < 0.02 && (mb[3] - 297.0 * PT).abs() < 0.02, "an A3 page");
    }
    // embedded, subset fonts (and no unembedded standard ones)
    let fonts = pdf.fonts();
    assert!(!fonts.is_empty() && fonts.iter().all(|(_, embedded)| *embedded), "{fonts:?}");
    assert!(fonts.iter().all(|(n, _)| n.contains('+') && n.contains("Inter")), "subset tags: {fonts:?}");
    assert!(bytes.len() < 150_000, "subsetting keeps it small: {} bytes", bytes.len());
    // text comes back as text, accents and all
    let t0 = pdf.text(pages[0]);
    // (a run is split where the character comes from the other Latin subset, so read them joined)
    assert!(t0.concat().contains("Façade sud — Größe ½ ± 5° ł Ž   1:75"), "{t0:?}");
    assert!(t0.iter().any(|t| t == "Fixture") && t0.iter().any(|t| t.contains("Sheet 1 of 2")));
    let t1 = pdf.text(pages[1]);
    assert!(t1.concat().contains("Tür: 0.9 × 2.1 m, "), "{t1:?}");
    // line widths are pens in paper millimeters (ISO 128): 0.13, 0.18, 0.25, 0.35, 0.5
    let mut w = pdf.line_widths_mm(pages[0]);
    w.extend(pdf.line_widths_mm(pages[1]));
    for want in [0.13, 0.18, 0.25, 0.35, 0.5] {
        assert!(w.iter().any(|x| (x - want).abs() < 0.002), "pen {want} mm in {w:?}");
    }
    assert!(w.iter().all(|x| *x < 0.51), "no hairline is drawn heavier than the border pen: {w:?}");
    // vector, not a picture
    assert!(!pdf.objs.values().any(|o| o.dict.contains("/Subtype /Image")), "an elevation draws no image");
    // a picture view still is one
    let pic = doc(&e.model, "a4", vec![page("Perspective", vec![ViewportSpec { view: "iso".into(), x: 20.0, y: 20.0, w: 100.0, h: 80.0, ..Default::default() }])], false);
    let pdf2 = pdftext::parse(&draft::pdf::write(&pic).unwrap());
    assert!(pdf2.objs.values().any(|o| o.dict.contains("/Subtype /Image")), "a perspective view is raster");
}

#[test]
fn pen_widths_do_not_scale_with_the_drawing() {
    let e = house();
    let widths = |scale: &str| {
        let d = doc(&e.model, "a3", vec![page("x", vec![vp("front", scale)])], false);
        let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
        pdf.line_widths_mm(pdf.pages()[0])
    };
    let (a, b) = (widths("1:50"), widths("1:500"));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() < 1e-6, "{a:?} vs {b:?}");
    }
}

// ---- the SVG and DXF --------------------------------------------------------------------------

#[test]
fn svg_is_a_page_in_millimeters() {
    let e = house();
    let d = doc(&e.model, "a3", vec![page("Plan", vec![plan_vp("1:75")])], false);
    let svg = draft::svg::page(&d.pages[0]);
    assert!(svg.starts_with("<svg ") && svg.contains("viewBox=\"0 0 420 297\"") && svg.contains("width=\"420mm\""));
    assert!(svg.contains("stroke-width=\"0.5\""), "the border pen");
    assert!(svg.contains(">Sheet") || svg.contains("Sheet 1 of 1"));
    assert!(svg.contains("<clipPath"), "viewports clip");
}

#[test]
fn dxf_round_trips_through_our_importer() {
    let e = dimensioned();
    let d = doc(&e.model, "a3", vec![page("Studio", vec![plan_vp("1:50")])], false);
    let text = draft::dxf::page(&d.pages[0]);
    assert!(text.contains("AC1015"));
    // lineweights (370) are the pens, snapped to the DXF set; colors are true colors (420)
    let codes: Vec<(&str, &str)> = text.lines().collect::<Vec<_>>().chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].trim(), c[1].trim())).collect();
    let weights: std::collections::BTreeSet<i32> = codes.iter().filter(|(c, _)| *c == "370").map(|(_, v)| v.parse().unwrap()).collect();
    for w in [13, 18, 25, 35, 50] {
        assert!(weights.contains(&w), "lineweight {w} in {weights:?}");
    }
    assert!(codes.iter().any(|(c, _)| *c == "420"));
    assert!(codes.iter().any(|(c, v)| *c == "0" && *v == "LWPOLYLINE") && codes.iter().any(|(c, v)| *c == "0" && *v == "HATCH") && codes.iter().any(|(c, v)| *c == "0" && *v == "TEXT"));
    // and it reads back: millimeters, the sheet's border where it was, the labels
    let mut m = Model::default();
    let r = sketchcraft::dxf::import(&mut m, &text, "Sheet", Some("mm"), DVec3::ZERO).expect("our own DXF imports");
    assert_eq!(r.units, "mm");
    assert!(r.edges > 50, "line work came back: {} edges", r.edges);
    assert!(r.labels >= 3, "text came back: {}", r.labels);
    let (lo, hi) = m.world_bounds().unwrap();
    // the border is inset 10 mm all round on 420 x 297 (y flips: DXF is y up)
    assert!((lo.x - 0.010).abs() < 1e-3 && (lo.y - 0.010).abs() < 1e-3, "{lo:?}");
    assert!((hi.x - 0.410).abs() < 1e-3 && (hi.y - 0.287).abs() < 1e-3, "{hi:?}");
    let labels: Vec<String> = m.annotations.iter().filter_map(|a| if let sketchcraft::model::Annotation::Text { text, .. } = a { Some(text.clone()) } else { None }).collect();
    assert!(labels.iter().any(|t| t == "Fixture") && labels.iter().any(|t| t.contains("Sheet 1 of 1")), "{labels:?}");
}

#[test]
fn dxf_is_a_complete_r2000_file() {
    // a blueprint plan with fills, text and dashes, and a big page, so the handle seed matters
    let big = {
        let e = town();
        doc(&e.model, "a1", vec![page("Tilted", vec![ViewportSpec { w: 780.0, h: 400.0, x: 30.0, y: 30.0, ..vp("front", "1:200") }])], false)
    };
    let e = house();
    let small = doc(&e.model, "a3", vec![page("Plan", vec![plan_vp("1:75")])], true);
    for (name, d) in [("big", &big), ("small", &small)] {
        let text = draft::dxf::page(&d.pages[0]);
        let lines: Vec<&str> = text.lines().collect();
        let pairs: Vec<(&str, &str)> = lines.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].trim(), c[1].trim())).collect();
        assert_eq!(pairs.last(), Some(&("0", "EOF")));
        // handles are unique, and the header's seed is past all of them
        let seed_at = pairs.iter().position(|p| *p == ("9", "$HANDSEED")).unwrap();
        let handles: Vec<u32> = pairs.iter().enumerate().filter(|(i, (c, _))| (*c == "5" || *c == "105") && *i != seed_at + 1).map(|(_, (_, v))| u32::from_str_radix(v, 16).unwrap()).collect();
        let hs: std::collections::BTreeSet<u32> = handles.iter().copied().collect();
        assert_eq!(hs.len(), handles.len(), "{name}: handles are unique");
        let seed = u32::from_str_radix(pairs[seed_at + 1].1, 16).unwrap();
        assert!(seed > *hs.iter().max().unwrap(), "{name}: $HANDSEED {seed:X} is past the highest handle {:X}", hs.iter().max().unwrap());
        // every owner (330) is a handle that exists (or 0)
        for (c, v) in &pairs {
            if *c == "330" {
                let h = u32::from_str_radix(v, 16).unwrap();
                assert!(h == 0 || hs.contains(&h), "{name}: owner {v} isn't a handle in the file");
            }
        }
        // all the symbol tables, the two spaces' blocks and the root dictionary's group and layout entries
        for t in ["VPORT", "LTYPE", "LAYER", "STYLE", "VIEW", "UCS", "APPID", "DIMSTYLE", "BLOCK_RECORD"] {
            assert!(pairs.contains(&("2", t)), "{name}: a {t} table");
        }
        for want in ["*Model_Space", "*Paper_Space", "ACAD_GROUP", "ACAD_LAYOUT", "Model", "Layout1"] {
            assert!(pairs.contains(&("2", want)) || pairs.contains(&("3", want)), "{name}: {want}");
        }
        assert!(pairs.contains(&("0", "LAYOUT")));
        // every entity is owned by the model space block record, and its color (420) sits in the AcDbEntity part
        let start = pairs.iter().position(|p| *p == ("2", "ENTITIES")).unwrap();
        let end = start + pairs[start..].iter().position(|p| *p == ("0", "ENDSEC")).unwrap();
        let mut entities = 0;
        let mut i = start + 1;
        while i < end {
            assert_eq!(pairs[i].0, "0");
            let next = (i + 1..end).find(|k| pairs[*k].0 == "0").unwrap_or(end);
            let rec = &pairs[i..next];
            assert!(rec.contains(&("330", "17")), "{name}: {} has no owner: {rec:?}", rec[0].1);
            let first_sub = rec.iter().position(|(c, _)| *c == "100").unwrap();
            let second_sub = rec[first_sub + 1..].iter().position(|(c, _)| *c == "100").map(|k| k + first_sub + 1).unwrap();
            if let Some(color) = rec.iter().position(|(c, _)| *c == "420") {
                assert!(color < second_sub, "{name}: {} puts its color after its subclass marker", rec[0].1);
            }
            entities += 1;
            i = next;
        }
        assert!(entities > 20, "{name}: {entities}");
    }
}

#[test]
fn a_stopped_hidden_line_worker_fails_the_drawing() {
    use sketchcraft::draft::hidden::{Proj, View};
    let e = town();
    let pens = Pens::new(false);
    let mk = |inject_panic: bool| View {
        m: &e.model,
        proj: Proj::new(DVec3::new(75.0, 60.0, 1.0), DVec3::Y, 5.0, [400.0, 250.0]),
        rect: [30.0, 30.0, 780.0, 400.0],
        clip: None,
        beyond: false,
        pens: &pens,
        inject_panic,
    };
    assert!(mk(false).lines().unwrap().len() > 100);
    let err = mk(true).lines().err().expect("a worker that stopped must not leave a silent hole");
    assert!(err.to_string().contains("incomplete"), "{err}");
}

#[test]
fn missing_glyphs_and_accessibility_marks() {
    let e = house();
    let mut d = doc(&e.model, "a3", vec![page("Plan", vec![plan_vp("1:75")])], false);
    d.pages[0].items.push(Prim::Text(TextPrim { at: [30.0, 200.0], size: 4.0, bold: false, rgb: BLACK, angle: 0.0, text: "寸法 ok".into(), role: Role::Paragraph }));
    let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
    let t = pdf.text(pdf.pages()[0]).concat();
    assert!(t.contains("\u{fffd}\u{fffd} ok"), "every missing character reads as the replacement character, not the first one seen: {t:?}");
    assert!(pdf.objs.values().any(|o| o.dict.contains("/Lang (en)")), "document language");
    assert!(pdf.objs.values().any(|o| o.dict.contains("/Title")), "document title");
    assert!(pdf.objs.values().any(|o| o.dict.contains("/DisplayDocTitle true")), "the viewer shows the title");
    assert!(pdf.objs.values().any(|o| o.dict.contains("/Marked true")) && pdf.objs.values().any(|o| o.dict.contains("/Type /StructTreeRoot")), "a tagged PDF");
    let paragraphs = pdf.objs.values().filter(|o| o.dict.contains("/S /P")).count();
    assert!(paragraphs >= 1, "text outside a figure is a tagged paragraph: {paragraphs}");
    // tagged content never nests: a sequence opens only when none is open
    let content = pdf.content(pdf.pages()[0]);
    let mut depth = 0;
    for l in content.lines() {
        if l.ends_with("BDC") || l.ends_with("BMC") {
            depth += 1;
            assert!(depth <= 1, "nested marked content at {l:?}");
        } else if l.trim() == "EMC" {
            depth -= 1;
        }
    }
    assert_eq!(depth, 0);
    assert!(pdf.content(pdf.pages()[0]).contains("/Artifact BMC"), "the sheet's rules are artifacts");
    let figures: Vec<&str> = pdf.objs.values().filter(|o| o.dict.contains("/S /Figure")).map(|o| o.dict.as_str()).collect();
    assert_eq!(figures.len(), 1, "the viewport is a figure");
    assert!(figures[0].contains("/Alt (") && figures[0].contains("1:75"), "described by its caption: {}", figures[0]);
    assert!(pdf.objs.values().any(|o| o.dict.contains("/S /H1")) && pdf.objs.values().any(|o| o.dict.contains("/S /H2")) && pdf.objs.values().any(|o| o.dict.contains("/S /Caption")), "headings and a caption");
    assert!(pdf.content(pdf.pages()[0]).contains("/ActualText"), "text with characters the fonts lack keeps its real text");
    let svg = draft::svg::page(&d.pages[0]);
    assert!(svg.contains("aria-labelledby=\"sc-") && svg.contains("-title\">Fixture: Plan</title>") && !svg.contains("role=\"img\""), "{}", &svg[..300]);
}

#[test]
fn sheets_command_writes_svg_and_dxf_pages() {
    let mut e = house();
    let dir = std::env::temp_dir().join(format!("sketchcraft-draft-{}", std::process::id()));
    let pages = json!([{"title":"Plan","viewports":[{"view":"top","cut":1.22,"scale":"1:75","x":20,"y":20,"w":260,"h":170}]},{"title":"Front","viewports":[{"view":"front","scale":"1/4\"=1'-0\"","x":20,"y":20,"w":260,"h":170}]}]);
    for (file, page) in [("a.pdf", None), ("a.svg", Some(2)), ("a.dxf", Some(1))] {
        let path = dir.join(file).to_string_lossy().to_string();
        let mut cmd = json!({"cmd":"export_sheets","path":path,"sheet":"arch d","pages":pages});
        if let Some(p) = page {
            cmd["page"] = json!(p);
        }
        let r = run(&mut e, cmd);
        assert_eq!(r["pages"], 2);
        assert!(std::fs::metadata(&path).unwrap().len() > 1000, "{file}");
    }
    let svg = std::fs::read_to_string(dir.join("a.svg")).unwrap();
    assert!(svg.contains("1/4&quot;=1'-0&quot;") && svg.contains("914.4"), "page 2, on an ARCH D sheet");
    let bad = e.exec_json(&json!({"cmd":"export_sheets","path":dir.join("b.svg").to_string_lossy(),"page":9}).to_string());
    assert_eq!(bad["ok"], false);
    std::fs::remove_dir_all(dir).ok();
}

// ---- how fast ------------------------------------------------------------------------------------

/// A town of 8,400 one-metre boxes (50,400 faces, 100,800 edges), placed as component instances.
fn town() -> Engine {
    let mut e = Engine::new();
    let g = box_group(&mut e, [0.0, 0.0, 0.0], [1.0, 1.0, 2.0], "Block");
    let inst = run(&mut e, json!({"cmd":"make_component","ids":[g],"name":"Block"}))["instance"].as_u64().unwrap();
    let row = run(&mut e, json!({"cmd":"array","ids":[inst],"by":[1.5, 0.0, 0.0],"count":99}));
    let mut ids: Vec<u64> = row["ids"].as_array().map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect()).unwrap_or_default();
    ids.push(inst);
    run(&mut e, json!({"cmd":"array","ids":ids,"by":[0.0, 1.5, 0.0],"count":83}));
    e
}

#[test]
fn a_fifty_thousand_face_elevation_takes_seconds() {
    let e = town();
    let faces = e.model.world_faces().len();
    assert!(faces >= 50_000, "{faces} faces");
    let big = |v: ViewportSpec| ViewportSpec { w: 780.0, h: 400.0, x: 30.0, y: 30.0, ..v };
    let paths = |d: &Doc| {
        let mut n = 0;
        d.pages[0].walk(&mut |p| n += matches!(p, Prim::Path(_)) as usize);
        n
    };
    // a true elevation: the front row hides the rest
    let t = std::time::Instant::now();
    let d = doc(&e.model, "a1", vec![page("Elevation", vec![big(ViewportSpec { callouts: true, ..vp("front", "1:200") })])], false);
    let elevation = t.elapsed();
    let t = std::time::Instant::now();
    let pdf = draft::pdf::write(&d).unwrap();
    let write = t.elapsed();
    eprintln!("PERF {faces} faces: elevation {elevation:?} ({} paths), PDF {write:?} ({} KB)", paths(&d), pdf.len() / 1024);
    assert!(elevation.as_secs_f64() < 10.0, "the elevation took {elevation:?}");
    assert!(paths(&d) < 3000, "the boxes behind the front row are hidden: {} paths", paths(&d));
    // the worst case: a view slightly above the horizon, where nearly every edge shows
    let t = std::time::Instant::now();
    let d = doc(&e.model, "a1", vec![page("Tilted", vec![big(vp("front", "1:200"))])], false);
    let tilted = t.elapsed();
    eprintln!("PERF {faces} faces: tilted view, most edges visible {tilted:?} ({} paths)", paths(&d));
    assert!(tilted.as_secs_f64() < 10.0, "took {tilted:?}");
    // a section through the middle, with the hidden lines drawn too
    let t = std::time::Instant::now();
    let d = doc(&e.model, "a1", vec![page("Section", vec![big(ViewportSpec { beyond: true, callouts: true, section: Some([60.0, 60.0, 0.0, 0.0, 1.0, 0.0]), ..vp("section", "1:200") })])], false);
    eprintln!("PERF {faces} faces: section with hidden lines drawn {:?} ({} paths)", t.elapsed(), paths(&d));
}
