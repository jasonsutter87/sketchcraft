//! Line weight and style per tag and per edge (#3, design step 6): the data and commands, the pen lookup and its
//! precedence, and the same pens in the PDF, SVG and DXF of a sheet at any scale.

#[path = "pdftext/mod.rs"]
mod pdftext;

use serde_json::{json, Value};
use sketchcraft::draft::{self, ir::*};
use sketchcraft::drafting::{Drafting, EdgeDraft};
use sketchcraft::layout::{PageSpec, ViewportSpec};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
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

/// A box group called Walls on the "Walls" tag, and one called Doors on the "Doors" tag beside it.
fn model() -> Engine {
    let mut e = Engine::new();
    for (name, x) in [("Walls", 0.0), ("Doors", 3.0)] {
        let g = ok(&mut e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
        let f = ok(&mut e, json!({"cmd": "rect", "from": [x, 0, 0], "to": [x + 2.0, 1.0, 0]}))["face"].as_u64().unwrap();
        let up = e.model.normal(f as _).z > 0.0;
        ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.0 } else { -2.0 }}));
        ok(&mut e, json!({"cmd": "end_group"}));
        ok(&mut e, json!({"cmd": "tag", "ids": [g], "tag": name}));
    }
    e
}

fn sheet(e: &Engine, scale: &str) -> Doc {
    let vp = ViewportSpec { view: "front".into(), scale: Some(scale.into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Elevation".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A-1", "Test", vec![page], false, &[], "2026-11-17").unwrap()
}

fn svg(d: &Doc) -> String {
    draft::svg::page(&d.pages[0])
}

/// Stroke widths of the paths a model edge made (those with provenance), paper mm.
fn model_widths(d: &Doc) -> Vec<f64> {
    fn walk(items: &[Prim], out: &mut Vec<f64>) {
        // (packed lines opened into their paths)
        for p in draft::ir::expanded(items).iter() {
            match p {
                Prim::Path(PathPrim { pen: Some(pen), prov: Some(_), .. }) => {
                    if !out.iter().any(|w| (w - pen.w).abs() < 1e-9) {
                        out.push(pen.w);
                    }
                }
                Prim::Group(g) => walk(&g.children, out),
                _ => {}
            }
        }
    }
    let mut out = vec![];
    walk(&d.pages[0].items, &mut out);
    out.sort_by(|a, b| a.total_cmp(b));
    out
}

// ---- precedence ---------------------------------------------------------------------------------

#[test]
fn edge_beats_tag_beats_default() {
    let pens = Pens::new(false);
    let base = pens.ink(pens.medium);
    let mut d = Drafting::default();
    // no style: the default pen, untouched
    assert_eq!(pens.for_tag(&d, Some("Walls"), None, base.clone(), false), base);

    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.5, "line": "dashed", "color": "#ff0000"}));
    d = e.model.drafting.clone();

    // the tag over the default
    let p = pens.for_tag(&d, Some("Walls"), None, base.clone(), false);
    assert_eq!((p.w, p.rgb, p.dash.clone()), (0.5, [255, 0, 0], vec![3.0, 1.5]));
    // an untagged line, and a tag with no style, keep the default
    assert_eq!(pens.for_tag(&d, None, None, base.clone(), false), base);
    assert_eq!(pens.for_tag(&d, Some("Other"), None, base.clone(), false), base);

    // the edge over the tag, field by field: weight only (dash and colour fall through to the tag)
    let w = EdgeDraft { weight_mm: Some(0.9), ..Default::default() };
    let p = pens.for_tag(&d, Some("Walls"), Some(&w), base.clone(), false);
    assert_eq!((p.w, p.rgb, p.dash.clone()), (0.9, [255, 0, 0], vec![3.0, 1.5]));
    // line only: solid beats the tag's dashes, weight falls through
    let l = EdgeDraft { line: Some("solid".into()), ..Default::default() };
    let p = pens.for_tag(&d, Some("Walls"), Some(&l), base.clone(), false);
    assert_eq!((p.w, p.dash.is_empty()), (0.5, true));
    // an edge override works with no tag style at all
    let p = pens.for_tag(&Drafting::default(), None, Some(&w), base.clone(), false);
    assert_eq!(p.w, 0.9);
    // the width is paper mm: the pen has no scale in it
    assert_eq!(pens.for_tag(&d, Some("Walls"), None, base, false).w, 0.5);
}

#[test]
fn commands_set_check_and_clear_styles() {
    let mut e = model();
    let r = ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7}));
    assert_eq!(r["style"]["weight_mm"], 0.7);
    // only what is given changes
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "line": "center"}));
    let s = e.model.drafting.tag_style("Walls").unwrap().clone();
    assert_eq!((s.weight_mm, s.line.as_str()), (0.7, "center"));
    // bad values are refused, with a reason
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 9.0})).contains("between"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": -1.0})).contains("between"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "line": "wavy"})).contains("no line style"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "color": "red"})).contains("#rrggbb"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "  "})).contains("name"));
    // 0 and "" clear one field; clearing them all removes the style
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0, "line": ""}));
    assert!(e.model.drafting.tag_styles.is_empty());
    // renaming a tag takes its style along
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.35}));
    ok(&mut e, json!({"cmd": "rename_tag", "tag": "Walls", "to": "Shell"}));
    assert!(e.model.drafting.tag_style("Walls").is_none());
    assert_eq!(e.model.drafting.tag_style("Shell").unwrap().weight_mm, 0.35);
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Shell", "clear": true}));
    assert!(e.model.drafting.tag_styles.is_empty());

    // edges: by edge id, or every edge of a face
    let edge = *e.model.edges.keys().next().unwrap();
    assert_eq!(ok(&mut e, json!({"cmd": "edge_style", "ids": [edge], "weight_mm": 1.0, "line": "hidden"}))["edges"], 1);
    let d = e.model.edges[&edge].draft.clone().unwrap();
    assert_eq!((d.weight_mm, d.line.as_deref()), (Some(1.0), Some("hidden")));
    let face = *e.model.faces.keys().next().unwrap();
    assert!(ok(&mut e, json!({"cmd": "edge_style", "ids": [face], "weight_mm": 0.5}))["edges"].as_u64().unwrap() >= 4);
    assert!(fails(&mut e, json!({"cmd": "edge_style", "ids": [987654], "weight_mm": 0.5})).contains("not an edge or face"));
    ok(&mut e, json!({"cmd": "edge_style", "ids": [edge], "clear": true}));
    assert!(e.model.edges[&edge].draft.is_none());
}

#[test]
fn a_tag_style_comes_from_the_group_or_placement_an_edge_sits_in() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7}));
    let ws = e.model.world_edges();
    assert!(ws.iter().any(|w| w.tag.as_deref() == Some("Walls")));
    assert!(ws.iter().any(|w| w.tag.as_deref() == Some("Doors")));
    // a component's edges take the tag of the placement
    let g = *e.model.groups.iter().find(|(_, g)| g.name == "Doors").unwrap().0;
    ok(&mut e, json!({"cmd": "make_component", "ids": [g], "name": "Door"}));
    let inst = *e.model.instances.keys().next().unwrap();
    ok(&mut e, json!({"cmd": "tag", "ids": [inst], "tag": "Walls"}));
    assert!(e.model.world_edges().iter().filter(|w| !w.path.is_empty()).all(|w| w.tag.as_deref() == Some("Walls")));
}

// ---- file and undo ------------------------------------------------------------------------------

#[test]
fn styles_survive_save_and_open_and_undo_one_step_each() {
    let dir = std::env::temp_dir().join(format!("sc-linestyles-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let mut e = model();
    // no styles: still a plain format 1 file an old build opens
    let plain = dir.join("plain.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": plain}));
    let f: Value = serde_json::from_slice(&std::fs::read(&plain).unwrap()).unwrap();
    assert!(f["format"] == 1 || f["format"].is_null(), "{}", f["format"]);

    let edge = *e.model.edges.keys().next().unwrap();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7, "line": "dashed"}));
    ok(&mut e, json!({"cmd": "edge_style", "ids": [edge], "weight_mm": 1.2}));
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    assert_eq!(f["format"], 2, "styles need the newer format, so an older build opens it read-only");
    assert_eq!(f["drafting"]["tag_styles"]["Walls"]["weight_mm"], 0.7);
    assert_eq!(f["edges"][edge.to_string()]["draft"]["weight_mm"], 1.2);

    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": a}));
    assert_eq!(g.model.drafting.tag_style("Walls").unwrap().line, "dashed");
    assert_eq!(g.model.edges[&edge].draft.as_ref().unwrap().weight_mm, Some(1.2));
    // an edge style alone (no tag style) also makes the file format 2
    ok(&mut g, json!({"cmd": "tag_style", "tag": "Walls", "clear": true}));
    ok(&mut g, json!({"cmd": "save", "path": dir.join("b.sketchcraft")}));
    let f: Value = serde_json::from_slice(&std::fs::read(dir.join("b.sketchcraft")).unwrap()).unwrap();
    assert_eq!(f["format"], 2);

    // undo and redo, one step per command
    ok(&mut g, json!({"cmd": "undo"}));
    assert!(g.model.drafting.tag_style("Walls").is_some());
    ok(&mut g, json!({"cmd": "edge_style", "ids": [edge], "clear": true}));
    assert!(g.model.edges[&edge].draft.is_none());
    ok(&mut g, json!({"cmd": "undo"}));
    assert_eq!(g.model.edges[&edge].draft.as_ref().unwrap().weight_mm, Some(1.2));
    ok(&mut g, json!({"cmd": "redo"}));
    assert!(g.model.edges[&edge].draft.is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn agents_and_plugins_may_set_styles_inside_their_limits() {
    for who in [Caller::Plugin, Caller::Mcp(Workspace::anywhere())] {
        assert!(permits(&who, "tag_style") && permits(&who, "edge_style"));
    }
    let mut c = json!({"cmd": "edge_style", "ids": [1, 2], "weight_mm": 0.5});
    assert!(vet(&Caller::Mcp(Workspace::anywhere()), &mut c).is_ok());
}

// ---- the drawings -------------------------------------------------------------------------------

#[test]
fn a_tag_style_prints_at_true_weight_at_any_scale_in_svg_and_pdf() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7, "line": "dashed"}));
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Doors", "weight_mm": 0.13}));
    for scale in ["1:20", "1:50", "1:200"] {
        let d = sheet(&e, scale);
        let w = model_widths(&d);
        assert!(w.contains(&0.7) && w.contains(&0.13), "{scale}: {w:?}");
        assert!(!w.contains(&0.25), "{scale}: every edge is tagged here: {w:?}");
        let s = svg(&d);
        assert!(s.contains("stroke-width=\"0.7\""), "{scale}");
        assert!(s.contains("stroke-dasharray=\"3 1.5\""), "{scale}");
        // the PDF carries the same widths, in points
        let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
        let mm = pdf.line_widths_mm(pdf.pages()[0]);
        for want in [0.7, 0.13] {
            assert!(mm.iter().any(|x| (x - want).abs() < 0.01), "{scale}: {want} not in {mm:?}");
        }
        let c = pdf.content(pdf.pages()[0]);
        assert!(c.contains("] 0 d") || c.contains(" d\n"), "{scale}: a dash pattern is set");
    }
}

#[test]
fn an_edge_override_beats_its_tag_in_every_output() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.5}));
    // the front bottom edge of the Walls box
    let (&edge, _) = e.model.edges.iter().find(|(_, ed)| {
        let (a, b) = (e.model.p(ed.a), e.model.p(ed.b));
        a.y.abs() < 1e-9 && b.y.abs() < 1e-9 && a.z.abs() < 1e-9 && b.z.abs() < 1e-9 && a.x.min(b.x) < 1e-9
    }).expect("a front bottom edge");
    ok(&mut e, json!({"cmd": "edge_style", "ids": [edge], "weight_mm": 1.2, "line": "center"}));
    let d = sheet(&e, "1:50");
    let w = model_widths(&d);
    assert!(w.contains(&1.2) && w.contains(&0.5), "{w:?}");
    // and the others stayed on the tag
    let s = svg(&d);
    assert!(s.contains("stroke-width=\"1.2\"") && s.contains("stroke-dasharray=\"8 1.5 1.5 1.5\""));
    let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
    assert!(pdf.line_widths_mm(pdf.pages()[0]).iter().any(|x| (x - 1.2).abs() < 0.01));
}

/// A DXF as (group code, value) pairs.
fn groups(dxf: &str) -> Vec<(i32, String)> {
    let l: Vec<&str> = dxf.lines().collect();
    l.chunks(2).filter(|c| c.len() == 2).filter_map(|c| Some((c[0].trim().parse().ok()?, c[1].trim().to_string()))).collect()
}

#[test]
fn dxf_round_trips_lineweight_and_linetype() {
    let mut e = model();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.7, "line": "hidden"}));
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Doors", "weight_mm": 0.35, "line": "center", "color": "#0000ff"}));
    let d = sheet(&e, "1:50");
    let dxf = draft::dxf::page(&d.pages[0]);
    let g = groups(&dxf);
    // entities: (linetype name, lineweight, true colour)
    let mut ents: Vec<(Option<String>, i32, Option<u32>)> = vec![];
    let mut i = 0;
    while i < g.len() {
        if g[i] == (0, "LINE".into()) || g[i] == (0, "LWPOLYLINE".into()) {
            let (mut lt, mut lw, mut col) = (None, -99, None);
            i += 1;
            while i < g.len() && g[i].0 != 0 {
                match g[i].0 {
                    6 => lt = Some(g[i].1.clone()),
                    370 => lw = g[i].1.parse().unwrap(),
                    420 => col = g[i].1.parse().ok(),
                    _ => {}
                }
                i += 1;
            }
            ents.push((lt, lw, col));
        } else {
            i += 1;
        }
    }
    // the linetypes are in the table, with their dash lengths
    let hidden = "SC_DASH_2.00_1.00";
    let center = "SC_DASH_8.00_1.50_1.50_1.50";
    assert!(ents.iter().any(|(lt, lw, _)| lt.as_deref() == Some(hidden) && *lw == 70), "{ents:?}");
    assert!(ents.iter().any(|(lt, lw, c)| lt.as_deref() == Some(center) && *lw == 35 && *c == Some(0x0000ff)), "{ents:?}");
    for (name, pattern) in [(hidden, [2.0, -1.0].as_slice()), (center, [8.0, -1.5, 1.5, -1.5].as_slice())] {
        let at = g.iter().position(|x| x == &(2, name.to_string())).unwrap_or_else(|| panic!("{name} is not in the LTYPE table"));
        let dashes: Vec<f64> = g[at..].iter().skip_while(|x| x.0 != 49).take_while(|x| x.0 == 49 || x.0 == 74 || x.0 == 75 || x.0 == 46 || x.0 == 50 || x.0 == 44 || x.0 == 45 || x.0 == 9).filter(|x| x.0 == 49).map(|x| x.1.parse().unwrap()).collect();
        assert_eq!(dashes.iter().map(|d| d.abs()).collect::<Vec<_>>(), pattern.iter().map(|d: &f64| d.abs()).collect::<Vec<_>>(), "{name}");
    }
}
