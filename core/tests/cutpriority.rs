//! Cut intersection priority (#88, design §3.4): where cut regions of different tags overlap, the tag with the higher
//! `cut.priority` keeps the overlap and the lower is trimmed back with no seam; one tag, or equal priority, merges.
//! Priority 0 on every tag leaves the cut exactly as it was.

#[path = "pdftext/mod.rs"]
mod pdftext;

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::draft::{self, ir::*};
use sketchcraft::layout::{PageSpec, ViewportSpec};
use sketchcraft::policy::{permits, Caller, Workspace};
use sketchcraft::section::{Cut, Region};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn fails(e: &mut Engine, v: Value) -> String {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} -> {r}");
    r["error"].as_str().unwrap_or_default().to_string()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-cutprio-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn block(e: &mut Engine, lo: [f64; 2], hi: [f64; 2], h: f64) {
    let f = ok(e, json!({"cmd": "rect", "from": [lo[0], lo[1], 0], "to": [hi[0], hi[1], 0]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": if up { h } else { -h }}));
}

/// A tagged group holding one box.
fn tagged(e: &mut Engine, name: &str, tag: &str, lo: [f64; 2], hi: [f64; 2]) -> u64 {
    let g = ok(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    block(e, lo, hi, 2.5);
    ok(e, json!({"cmd": "end_group"}));
    ok(e, json!({"cmd": "tag", "ids": [g], "tag": tag}));
    g
}

fn prio(e: &mut Engine, tag: &str, p: i64) {
    ok(e, json!({"cmd": "tag_style", "tag": tag, "cut": {"priority": p}}));
}

fn plan(e: &Engine) -> Cut {
    e.model.section(DVec3::new(0.0, 0.0, 1.2), DVec3::new(0.0, 0.0, -1.0)).unwrap()
}

fn ring_area(r: &[DVec3]) -> f64 {
    (0..r.len()).map(|i| (r[i].x * r[(i + 1) % r.len()].y - r[(i + 1) % r.len()].x * r[i].y) / 2.0).sum::<f64>().abs()
}

fn area(r: &Region) -> f64 {
    ring_area(&r.outer) - r.holes.iter().map(|h| ring_area(h)).sum::<f64>()
}

fn tag_area(c: &Cut, tag: &str) -> f64 {
    c.regions.iter().filter(|r| r.tag.as_deref() == Some(tag)).map(area).sum()
}

fn covers(c: &Cut, tag: &str, x: f64, y: f64) -> bool {
    c.regions.iter().filter(|r| r.tag.as_deref() == Some(tag)).any(|r| {
        let inside = |ring: &[DVec3]| {
            let mut c = false;
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a.y > y) != (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x {
                    c = !c;
                }
            }
            c
        };
        inside(&r.outer) && !r.holes.iter().any(|h| inside(h))
    })
}

/// A wall (x 0..2, y 0..4) running into a slab (x 1..5, y 0..4): they overlap on x 1..2.
fn wall_and_slab() -> Engine {
    let mut e = Engine::new();
    tagged(&mut e, "Wall", "Walls", [0.0, 0.0], [2.0, 4.0]);
    tagged(&mut e, "Slab", "Slab", [1.0, 0.0], [5.0, 4.0]);
    e
}

// ---- the engine -----------------------------------------------------------------------------------------

#[test]
fn the_higher_priority_wins_the_overlap_and_the_lower_is_trimmed() {
    let mut e = wall_and_slab();
    // default: both overlap, drawn as they were (the slab's whole 16 m2 and the wall's whole 8 m2)
    let c = plan(&e);
    assert!((tag_area(&c, "Walls") - 8.0).abs() < 1e-6 && (tag_area(&c, "Slab") - 16.0).abs() < 1e-6);
    prio(&mut e, "Walls", 10);
    let c = plan(&e);
    assert!((tag_area(&c, "Walls") - 8.0).abs() < 1e-6, "the winner is whole");
    assert!((tag_area(&c, "Slab") - 12.0).abs() < 1e-6, "the slab loses the 1x4 overlap");
    assert!(covers(&c, "Walls", 1.5, 2.0) && !covers(&c, "Slab", 1.5, 2.0));
    assert!(covers(&c, "Slab", 3.0, 2.0) && covers(&c, "Walls", 0.5, 2.0));
    // flip it: the slab wins
    prio(&mut e, "Slab", 20);
    let c = plan(&e);
    assert!((tag_area(&c, "Slab") - 16.0).abs() < 1e-6);
    assert!((tag_area(&c, "Walls") - 4.0).abs() < 1e-6, "the wall is trimmed back to the slab's edge");
    assert!(covers(&c, "Slab", 1.5, 2.0) && !covers(&c, "Walls", 1.5, 2.0));
    // each region keeps its tag and a real owner
    assert!(c.regions.iter().all(|r| r.tag.is_some() && e.model.groups.contains_key(&r.owner)));
}

#[test]
fn trimmed_regions_leave_no_seam_in_the_cut_lines() {
    let mut e = wall_and_slab();
    prio(&mut e, "Walls", 10);
    let c = plan(&e);
    // the slab's cut lines inside the wall are gone: nothing of the slab's edges between x 1 and 2 except the wall's own
    let inside_wall = |a: DVec3, b: DVec3| {
        let m = (a + b) / 2.0;
        m.x > 1.0 + 1e-6 && m.x < 2.0 - 1e-6 && m.y > 1e-6 && m.y < 4.0 - 1e-6
    };
    let slab_edge_in_wall = c.segments.iter().any(|(a, b)| inside_wall(*a, *b) && (a.x - 1.0).abs() < 1e-6 && (b.x - 1.0).abs() < 1e-6);
    assert!(!slab_edge_in_wall, "the slab's x=1 edge is under the wall");
    let loose = c.segments.iter().filter(|(a, b)| inside_wall(*a, *b)).count();
    assert_eq!(loose, 0, "no cut line is left inside the winner's body");
}

#[test]
fn the_same_tag_merges_with_no_internal_outline() {
    let mut e = Engine::new();
    tagged(&mut e, "Wall A", "Walls", [0.0, 0.0], [2.0, 4.0]);
    tagged(&mut e, "Wall B", "Walls", [1.0, 0.0], [5.0, 2.0]);
    // before priorities: two regions (an L of two overlapping boxes)
    assert_eq!(plan(&e).regions.len(), 2);
    prio(&mut e, "Other", 1); // any priority in use turns resolving on
    let c = plan(&e);
    assert_eq!(c.regions.len(), 1, "one L-shaped region");
    assert!((tag_area(&c, "Walls") - (8.0 + 8.0 - 2.0)).abs() < 1e-6, "the union, the 2 m2 overlap counted once");
    assert!(c.regions[0].holes.is_empty());
    assert_eq!(c.regions[0].outer.len(), 6, "an L: six corners");
}

#[test]
fn equal_priority_merges_and_the_overlap_goes_to_the_first_by_name() {
    let mut e = wall_and_slab();
    prio(&mut e, "Walls", 5);
    prio(&mut e, "Slab", 5);
    let c = plan(&e);
    // exactly one fill over the overlap, never two on top of each other
    let n = [covers(&c, "Walls", 1.5, 2.0), covers(&c, "Slab", 1.5, 2.0)];
    assert_eq!(n.iter().filter(|b| **b).count(), 1, "{n:?}");
    assert!(covers(&c, "Slab", 1.5, 2.0), "Slab sorts before Walls");
    assert!((tag_area(&c, "Walls") + tag_area(&c, "Slab") - (8.0 + 16.0 - 4.0)).abs() < 1e-6, "no area counted twice");
}

#[test]
fn holes_in_the_winner_show_the_lower_tag_and_holes_in_the_loser_are_kept() {
    let mut e = Engine::new();
    // a slab with a 1x1 hole, under a wall that covers the hole and more
    let g = ok(&mut e, json!({"cmd": "begin_group", "name": "Slab"}))["group"].as_u64().unwrap();
    block(&mut e, [0.0, 0.0], [6.0, 6.0], 2.5);
    ok(&mut e, json!({"cmd": "end_group"}));
    ok(&mut e, json!({"cmd": "tag", "ids": [g], "tag": "Slab"}));
    // the hole: a 2x2 shaft pushed down through the slab
    ok(&mut e, json!({"cmd": "edit", "id": g}));
    let top = ok(&mut e, json!({"cmd": "rect", "from": [2, 2, 2.5], "to": [4, 4, 2.5]}))["face"].as_u64().unwrap();
    let up = e.model.normal(top as _).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": top, "distance": if up { -2.5 } else { 2.5 }}));
    ok(&mut e, json!({"cmd": "close"}));
    let before = plan(&e);
    let holes: usize = before.regions.iter().filter(|r| r.tag.as_deref() == Some("Slab")).map(|r| r.holes.len()).sum();
    assert_eq!(holes, 1, "the slab's cut has the shaft as a hole");
    // a wall over the shaft's right half and beyond the slab's edge: x 3..8, y 1..5
    tagged(&mut e, "Wall", "Walls", [3.0, 1.0], [8.0, 5.0]);
    prio(&mut e, "Walls", 9);
    let c = plan(&e);
    assert!(covers(&c, "Walls", 3.5, 3.0), "the wall fills the shaft's right half: the winner is whole");
    assert!(!covers(&c, "Slab", 4.5, 3.0), "the slab is trimmed under the wall");
    assert!(covers(&c, "Slab", 1.0, 1.0) && !covers(&c, "Slab", 2.5, 3.0), "its own shaft is still open");
    assert!(!covers(&c, "Slab", 7.0, 3.0), "nothing is invented outside the slab");
    // the slab: 36 - the 3x4 the wall covers - the shaft's left 1x2 (the right half is already inside the wall's part)
    assert!((tag_area(&c, "Slab") - (36.0 - 12.0 - 2.0)).abs() < 1e-6, "{}", tag_area(&c, "Slab"));
    assert!((tag_area(&c, "Walls") - 20.0).abs() < 1e-6);
    // and the other way round, the wall under the slab: the slab's hole shows the wall through it
    prio(&mut e, "Slab", 50);
    let c = plan(&e);
    assert!(covers(&c, "Walls", 3.5, 3.0), "the wall shows through the slab's shaft");
    assert!(!covers(&c, "Walls", 5.0, 3.0), "and is trimmed where the slab is");
    assert!(covers(&c, "Walls", 7.0, 3.0));
}

#[test]
fn touching_edges_and_slivers_make_no_regions() {
    let mut e = Engine::new();
    // two boxes sharing an edge (x = 2), and a third a hair inside the first
    tagged(&mut e, "A", "A", [0.0, 0.0], [2.0, 2.0]);
    tagged(&mut e, "B", "B", [2.0, 0.0], [4.0, 2.0]);
    tagged(&mut e, "C", "C", [1.9999999, 0.0], [3.0, 2.0]);
    prio(&mut e, "A", 3);
    prio(&mut e, "B", 2);
    prio(&mut e, "C", 1);
    let c = plan(&e);
    assert!((tag_area(&c, "A") - 4.0).abs() < 1e-6 && (tag_area(&c, "B") - 4.0).abs() < 1e-6);
    // C lies on B (higher) and a 0.1 micron strip of A: nothing of it is left
    assert!(tag_area(&c, "C") < 1e-6, "{}", tag_area(&c, "C"));
    assert!(c.regions.iter().all(|r| area(r) > 1e-8), "no dust regions");
}

#[test]
fn with_every_priority_zero_the_cut_is_exactly_as_before() {
    let mut e = wall_and_slab();
    let plain = plan(&e);
    // a style with other fields, and one set back to 0, change nothing
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"outline_mm": 0.5}}));
    prio(&mut e, "Walls", 4);
    prio(&mut e, "Walls", 0);
    let c = plan(&e);
    assert_eq!(c.regions.len(), plain.regions.len());
    for (a, b) in c.regions.iter().zip(&plain.regions) {
        assert_eq!((a.owner, &a.tag, &a.outer, &a.holes), (b.owner, &b.tag, &b.outer, &b.holes));
    }
    assert_eq!(c.segments, plain.segments);
}

// ---- the command, undo, file, policy --------------------------------------------------------------------

#[test]
fn tag_style_priority_changes_only_what_is_given_and_checks_it() {
    let mut e = wall_and_slab();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"fill": "solid", "color": "#ff0000", "outline_mm": 0.5}}));
    prio(&mut e, "Walls", 7);
    let s = e.model.drafting.tag_style("Walls").unwrap().clone();
    assert_eq!((s.cut.priority, s.cut.outline_mm, s.cut.color.as_str()), (7, 0.5, "#ff0000"));
    // another cut field leaves the priority alone
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"outline_mm": 0.3}}));
    assert_eq!(e.model.drafting.tag_style("Walls").unwrap().cut.priority, 7);
    for bad in [json!(-1), json!(1000), json!(2.5), json!("high")] {
        let before = e.model.drafting.clone();
        fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"priority": bad}}));
        assert_eq!(e.model.drafting, before, "a bad value changes nothing");
    }
    prio(&mut e, "Walls", 999);
    prio(&mut e, "Walls", 0);
    // priority 0 alone on a tag leaves no style behind
    prio(&mut e, "Slab", 3);
    prio(&mut e, "Slab", 0);
    assert!(e.model.drafting.tag_style("Slab").is_none());
}

#[test]
fn priority_undoes_redoes_saves_and_opens() {
    let dir = scratch("file");
    let mut e = wall_and_slab();
    prio(&mut e, "Walls", 12);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(e.model.drafting.tag_style("Walls").is_none());
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(e.model.drafting.tag_style("Walls").unwrap().cut.priority, 12);
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    assert_eq!(f["format"], 2);
    assert_eq!(f["drafting"]["tag_styles"]["Walls"]["cut"]["priority"], 12);
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": a}));
    assert_eq!(g.model.drafting, e.model.drafting);
    assert!((tag_area(&plan(&g), "Slab") - 12.0).abs() < 1e-6, "the opened file resolves the same");
    // 0 is not written, so a file with no priorities reads exactly as before
    let mut h = wall_and_slab();
    ok(&mut h, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"outline_mm": 0.5}}));
    let b = dir.join("b.sketchcraft");
    ok(&mut h, json!({"cmd": "save", "path": b}));
    let f: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert!(f["drafting"]["tag_styles"]["Walls"]["cut"].get("priority").is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn agents_and_plugins_may_set_priority() {
    for who in [Caller::Plugin, Caller::Mcp(Workspace::anywhere())] {
        assert!(permits(&who, "tag_style"));
    }
    assert!(include_str!("../src/mcp.rs").contains("priority? (whole number 0-999"), "the MCP help says what priority does");
}

// ---- sheets and the section command ---------------------------------------------------------------------

fn plan_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "plan".into(), cut: Some(1.2), floor: Some(0.0), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Plan".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

fn section_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "section".into(), section: Some([0.0, 2.0, 1.0, 0.0, 1.0, 0.0]), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Section".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

/// Filled cut regions on the page: (fill colour, number of corners in the outer ring).
fn fills(d: &Doc) -> Vec<(Rgb, usize)> {
    let mut out = vec![];
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(PathPrim { fill: Some(f), segs, .. }) = p {
            if f.even_odd && segs.iter().any(|s| matches!(s, Seg::Z)) {
                out.push((f.rgb, segs.len()));
            }
        }
    });
    out
}

fn styled() -> Engine {
    let mut e = wall_and_slab();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"fill": "solid", "color": "#ff0000", "priority": 10}}));
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "solid", "color": "#0000ff"}}));
    e
}

#[test]
fn sheets_draw_the_resolved_regions_in_every_format() {
    let plain = {
        let mut e = wall_and_slab();
        ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"fill": "solid", "color": "#ff0000"}}));
        ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "solid", "color": "#0000ff"}}));
        e
    };
    let e = styled();
    // the slab (blue) is an L-less rectangle with a notch? it is a 3x4 box: 5 path segments (M L L L Z) - one fewer corner
    // than... the same four corners, but a different shape: compare the SVG of the two
    let svg_plain = draft::svg::page(&plan_sheet(&plain).pages[0]);
    let svg = draft::svg::page(&plan_sheet(&e).pages[0]);
    assert_ne!(svg, svg_plain, "priority changes the drawing");
    for d in [plan_sheet(&e), section_sheet(&e)] {
        let f = fills(&d);
        assert!(f.iter().any(|(c, _)| *c == [255, 0, 0]) && f.iter().any(|(c, _)| *c == [0, 0, 255]), "{f:?}");
    }
    // PDF: both fills are there and the PDF differs from the unresolved one
    let pdf = pdftext::parse(&draft::pdf::write(&plan_sheet(&e)).unwrap());
    let c = pdf.content(pdf.pages()[0]);
    assert!(c.contains("1 0 0 rg") && c.contains("0 0 1 rg"));
    let pdf_plain = pdftext::parse(&draft::pdf::write(&plan_sheet(&plain)).unwrap());
    assert_ne!(c, pdf_plain.content(pdf_plain.pages()[0]));
    // DXF: two solid hatches in the two colours
    let dxf = draft::dxf::page(&plan_sheet(&e).pages[0]);
    assert!(dxf.contains("16711680") && dxf.contains("255"));
    assert_ne!(dxf, draft::dxf::page(&plan_sheet(&plain).pages[0]));
}

#[test]
fn the_section_command_and_the_cut_file_give_the_resolved_regions() {
    let mut e = styled();
    let r = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    // the slab's blue triangles stay out of the wall's footprint (x 0..2): they are inside x 2..5
    let solid = r["solid"].as_array().unwrap();
    let blue = solid.iter().find(|s| s["tag"] == "Slab").unwrap();
    let tri: Vec<f64> = blue["fill"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(!tri.is_empty());
    for p in tri.chunks(3) {
        assert!(p[0] > 2.0 - 1e-6, "{p:?}");
    }
    // the exported cut file has the same trimmed regions (one outline per region)
    let dir = scratch("cutfile");
    let path = dir.join("cut.svg").to_string_lossy().to_string();
    ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1], "path": path}));
    let svg = std::fs::read_to_string(&path).unwrap();
    assert_eq!(svg.matches("<path").count(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_component_being_edited_still_resolves_by_the_whole_models_styles() {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.5 } else { -2.5 }}));
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    let desk = ok(&mut e, json!({"cmd": "make_component", "ids": ids, "name": "Desk"}))["instance"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "edit", "id": desk}));
    tagged_open(&mut e, "Top", "Top", [0.5, 0.0], [3.0, 1.0]);
    tagged_open(&mut e, "Rail", "Frame", [0.0, 0.0], [1.0, 1.0]);
    prio(&mut e, "Frame", 5);
    // the open component's own model has no styles: the whole model's decide
    let def = &e.model.definitions[&e.ctx.unwrap()].model;
    let (o, n) = (DVec3::new(0.0, 0.0, 1.2), DVec3::new(0.0, 0.0, -1.0));
    let styled = def.section_styled(o, n, &e.model.drafting).unwrap();
    let unstyled = def.section(o, n).unwrap();
    assert!((tag_area(&styled, "Top") - 2.0).abs() < 1e-6, "{}", tag_area(&styled, "Top"));
    assert!((tag_area(&unstyled, "Top") - 2.5).abs() < 1e-6, "no styles: nothing resolved");
    let r = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    assert!(r["regions"].as_u64().unwrap() >= 2, "{r}");
}

fn tagged_open(e: &mut Engine, name: &str, tag: &str, lo: [f64; 2], hi: [f64; 2]) -> u64 {
    let g = ok(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    let f = ok(e, json!({"cmd": "rect", "from": [lo[0], lo[1], 0], "to": [hi[0], hi[1], 0]}))["face"].as_u64().unwrap();
    let m = &e.model.definitions[&e.ctx.unwrap()].model;
    let up = m.normal(f as _).z > 0.0;
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.5 } else { -2.5 }}));
    ok(e, json!({"cmd": "end_group"}));
    ok(e, json!({"cmd": "tag", "ids": [g], "tag": tag}));
    g
}
