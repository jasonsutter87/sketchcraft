//! Cut fill per tag (#5, design step 8, §3.4): section regions are grouped by the innermost tagged owner (groups
//! nested at the root, components and components inside components), each tag's cut is filled solid, hatched or
//! left blank with its own outline weight, on sheets (the display list, SVG, PDF, DXF) and on screen (the `section`
//! command), set by `tag_style {cut}`, undone, saved and opened.

#[path = "pdftext/mod.rs"]
mod pdftext;

use glam::DVec3;
use serde_json::{json, Value};
use sketchcraft::draft::{self, ir::*};
use sketchcraft::drafting::CutFill;
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

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-cutfill-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A solid box from `lo` (x, y) to `hi`, `h` tall, in whatever is open (a group being drawn, a component).
fn block(e: &mut Engine, lo: [f64; 2], hi: [f64; 2], h: f64) {
    let f = ok(e, json!({"cmd": "rect", "from": [lo[0], lo[1], 0], "to": [hi[0], hi[1], 0]}))["face"].as_u64().unwrap();
    // the face is in whatever is open: the model, or the component being edited
    let m = match e.ctx {
        Some(d) => &e.model.definitions[&d].model,
        None => &e.model,
    };
    let up = m.normal(f as _).z > 0.0;
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": if up { h } else { -h }}));
}

/// A group holding one box.
fn group(e: &mut Engine, name: &str, lo: [f64; 2], hi: [f64; 2]) -> u64 {
    let g = ok(e, json!({"cmd": "begin_group", "name": name}))["group"].as_u64().unwrap();
    block(e, lo, hi, 2.5);
    ok(e, json!({"cmd": "end_group"}));
    g
}

fn tag(e: &mut Engine, id: u64, t: &str) {
    ok(e, json!({"cmd": "tag", "ids": [id], "tag": t}));
}

fn plan_cut(e: &Engine) -> sketchcraft::section::Cut {
    e.model.section(DVec3::new(0.0, 0.0, 1.2), DVec3::new(0.0, 0.0, -1.0)).unwrap()
}

/// (owner, tag) of every region whose outline's centre is near `x` (each test box sits at its own x).
fn region_at(c: &sketchcraft::section::Cut, x: f64) -> Vec<(u64, Option<String>)> {
    c.regions
        .iter()
        .filter(|r| {
            let cx = r.outer.iter().map(|p| p.x).sum::<f64>() / r.outer.len() as f64;
            (cx - x).abs() < 0.6
        })
        .map(|r| (r.owner as u64, r.tag.clone()))
        .collect()
}

// ---- grouping by the innermost tagged owner -------------------------------------------------------------

#[test]
fn root_regions_group_by_the_innermost_tagged_owner() {
    let mut e = Engine::new();
    // a tagged group with an untagged group inside: the inner box's cut belongs to the tagged one
    let walls = group(&mut e, "Walls", [0.0, 0.0], [1.0, 1.0]);
    let piece = group(&mut e, "Wall piece", [2.0, 0.0], [3.0, 1.0]);
    ok(&mut e, json!({"cmd": "set_parent", "ids": [piece], "parent": walls}));
    tag(&mut e, walls, "Walls");
    // a tagged group inside a tagged group: the inner tag wins
    let slab = group(&mut e, "Slab", [4.0, 0.0], [5.0, 1.0]);
    let finish = group(&mut e, "Finish", [6.0, 0.0], [7.0, 1.0]);
    ok(&mut e, json!({"cmd": "set_parent", "ids": [finish], "parent": slab}));
    tag(&mut e, slab, "Slab");
    tag(&mut e, finish, "Finish");
    // nothing tagged: each face's own group, as before tags styled cuts
    let outer = group(&mut e, "Outer", [8.0, 0.0], [9.0, 1.0]);
    let inner = group(&mut e, "Inner", [10.0, 0.0], [11.0, 1.0]);
    ok(&mut e, json!({"cmd": "set_parent", "ids": [inner], "parent": outer}));
    // a loose box (no group at all)
    block(&mut e, [12.0, 0.0], [13.0, 1.0], 2.5);

    let c = plan_cut(&e);
    let w = Some("Walls".to_string());
    assert_eq!(region_at(&c, 0.5), vec![(walls, w.clone())]);
    assert_eq!(region_at(&c, 2.5), vec![(walls, w)], "an untagged group takes the tagged group around it");
    assert_eq!(region_at(&c, 4.5), vec![(slab, Some("Slab".into()))]);
    assert_eq!(region_at(&c, 6.5), vec![(finish, Some("Finish".into()))], "the innermost tag wins");
    assert_eq!(region_at(&c, 8.5), vec![(outer, None)]);
    assert_eq!(region_at(&c, 10.5), vec![(inner, None)], "untagged nesting keeps the face's own group");
    // loose faces are each their own owner, so a loose box's cut never closes into a region (as before)
    assert!(region_at(&c, 12.5).is_empty());
    assert!(c.segments.len() > 6 * 4, "but its cut lines are there");
    assert_eq!(c.regions.len(), 6);
}

#[test]
fn inside_components_the_innermost_tag_wins_and_the_placement_names_it() {
    let mut e = Engine::new();
    // a desk: a component with a box of its own and a tagged group (its glass top) inside
    let f = ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [1, 1, 0]}))["face"].as_u64().unwrap();
    let up = e.model.normal(f as _).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.5 } else { -2.5 }}));
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    let made = ok(&mut e, json!({"cmd": "make_component", "ids": ids, "name": "Desk"}));
    let desk = made["instance"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "edit", "id": desk}));
    let glass = group(&mut e, "Glass top", [2.0, 0.0], [3.0, 1.0]);
    ok(&mut e, json!({"cmd": "tag", "ids": [glass], "tag": "Glass"}));
    ok(&mut e, json!({"cmd": "close"}));

    // untagged placement, untagged root: the component's own box falls back to the placement, untagged
    let c = plan_cut(&e);
    assert_eq!(region_at(&c, 0.5), vec![(desk, None)]);
    assert_eq!(region_at(&c, 2.5), vec![(desk, Some("Glass".into()))], "the tag inside the component styles its cut");

    // the placement tagged: its box takes that tag, the glass keeps its own (innermost)
    tag(&mut e, desk, "Furniture");
    let c = plan_cut(&e);
    assert_eq!(region_at(&c, 0.5), vec![(desk, Some("Furniture".into()))]);
    assert_eq!(region_at(&c, 2.5), vec![(desk, Some("Glass".into()))]);

    // an untagged placement in a tagged root group: the group is the innermost tagged owner
    ok(&mut e, json!({"cmd": "tag", "ids": [desk], "tag": null}));
    let room = ok(&mut e, json!({"cmd": "group", "ids": [desk], "name": "Room"}))["group"].as_u64().unwrap();
    tag(&mut e, room, "Interior");
    let c = plan_cut(&e);
    assert_eq!(region_at(&c, 0.5), vec![(room, Some("Interior".into()))]);
    assert_eq!(region_at(&c, 2.5), vec![(desk, Some("Glass".into()))]);
}

#[test]
fn components_inside_components_take_the_nearest_tag_outward() {
    let mut e = Engine::new();
    block(&mut e, [0.0, 0.0], [1.0, 1.0], 2.5);
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    let part = ok(&mut e, json!({"cmd": "make_component", "ids": ids, "name": "Leg"}))["instance"].as_u64().unwrap();
    tag(&mut e, part, "Legs");
    // the table: a box of its own (untagged), made a component; then the leg's placement moved inside its
    // definition (the commands place components only at the root; SketchUp files nest them like this)
    block(&mut e, [3.0, 0.0], [4.0, 1.0], 2.5);
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    let table = ok(&mut e, json!({"cmd": "make_component", "ids": ids, "name": "Table"}))["instance"].as_u64().unwrap();
    let mut leg = e.model.instances.remove(&(part as _)).unwrap();
    let tdef = e.model.instances[&(table as _)].def;
    // the table's definition sits at its own origin: the leg moves back by the table's placement
    leg.transform = e.model.instances[&(table as _)].transform.inverse() * leg.transform;
    let leg_id = 900_000;
    std::sync::Arc::make_mut(&mut e.model.definitions.get_mut(&tdef).unwrap().model).instances.insert(leg_id, leg);
    let c = plan_cut(&e);
    let all = || c.regions.iter().map(|r| (r.owner, r.tag.clone())).collect::<Vec<_>>();
    // the leg's placement is inside the table's definition: named by the top-level placement, tagged Legs
    assert_eq!(region_at(&c, 0.5), vec![(table, Some("Legs".into()))], "{:?}", all());
    assert_eq!(region_at(&c, 3.5), vec![(table, None)]);
    // the table tagged: its own box takes it, the leg keeps the nearer tag
    tag(&mut e, table, "Furniture");
    let c = plan_cut(&e);
    assert_eq!(region_at(&c, 0.5), vec![(table, Some("Legs".into()))]);
    assert_eq!(region_at(&c, 3.5), vec![(table, Some("Furniture".into()))]);
    // the leg untagged: it takes the table's tag
    std::sync::Arc::make_mut(&mut e.model.definitions.get_mut(&tdef).unwrap().model).instances.get_mut(&leg_id).unwrap().tag = None;
    let c = plan_cut(&e);
    assert_eq!(region_at(&c, 0.5), vec![(table, Some("Furniture".into()))]);
}

// ---- the model and sheets -----------------------------------------------------------------------------

/// Walls, a slab and furniture side by side, each on its own tag.
fn model() -> Engine {
    let mut e = Engine::new();
    let w = group(&mut e, "Walls", [0.0, 0.0], [1.0, 3.0]);
    tag(&mut e, w, "Walls");
    let s = group(&mut e, "Slab", [2.0, 0.0], [4.0, 3.0]);
    tag(&mut e, s, "Slab");
    let f = group(&mut e, "Desk", [5.0, 0.0], [6.0, 1.0]);
    tag(&mut e, f, "Furniture");
    e
}

fn three_fills(e: &mut Engine) {
    ok(e, json!({"cmd": "tag_style", "tag": "Walls", "hatch": {"pattern": "brick"}, "cut": {"outline_mm": 0.5}}));
    ok(e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "solid", "color": "#FF0000", "outline_mm": 0.7}}));
    ok(e, json!({"cmd": "tag_style", "tag": "Furniture", "cut": {"fill": "none", "outline_mm": 0.18}}));
}

fn plan_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "plan".into(), cut: Some(1.2), floor: Some(0.0), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Plan".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

fn section_sheet(e: &Engine) -> Doc {
    let vp = ViewportSpec { view: "section".into(), section: Some([3.0, 0.5, 1.0, 0.0, 1.0, 0.0]), scale: Some("1:50".into()), x: 20.0, y: 20.0, w: 260.0, h: 170.0, ..Default::default() };
    let page = PageSpec { title: "Section".into(), viewports: vec![vp], table: vec![], ..Default::default() };
    draft::sheet::build(&e.model, "A3", "Test", vec![page], false, &[], "2026-11-18").unwrap()
}

/// Every closed, filled cut region on the page: (fill colour, outline pen).
fn cut_paths(d: &Doc) -> Vec<(Rgb, Option<Pen>)> {
    let mut out = vec![];
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(PathPrim { fill: Some(f), pen, segs, .. }) = p {
            if f.even_odd && segs.iter().any(|s| matches!(s, Seg::Z)) {
                out.push((f.rgb, pen.clone()));
            }
        }
    });
    out
}

fn hatch_count(d: &Doc) -> usize {
    let mut n = 0;
    d.pages[0].walk(&mut |p| n += usize::from(matches!(p, Prim::Hatch(_))));
    n
}

fn outlines(d: &Doc) -> Vec<f64> {
    let mut out = vec![];
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(PathPrim { pen: Some(pen), segs, .. }) = p {
            if segs.iter().any(|s| matches!(s, Seg::Z)) {
                out.push(pen.w);
            }
        }
    });
    out
}

#[test]
fn with_no_cut_style_the_cut_looks_as_it_always_did() {
    let mut e = model();
    // a tag with only a line weight: its cut is still the dark poché with the heavy outline
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "weight_mm": 0.13}));
    let pens = Pens::new(false);
    for d in [plan_sheet(&e), section_sheet(&e)] {
        let cuts = cut_paths(&d);
        assert!(!cuts.is_empty());
        for (rgb, pen) in &cuts {
            assert_eq!(*rgb, pens.cut_fill);
            assert_eq!(pen.as_ref().map(|p| p.w), Some(pens.heavy));
            assert!(pen.as_ref().unwrap().dash.is_empty());
        }
        assert_eq!(hatch_count(&d), 0);
    }
    // "solid" with no colour of its own is the same poché
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Walls", "cut": {"fill": "solid"}}));
    assert!(cut_paths(&plan_sheet(&e)).iter().all(|(rgb, _)| *rgb == pens.cut_fill));
    assert_eq!(e.model.drafting.cut_fill(Some("Nothing")), CutFill::Auto);
    assert_eq!(e.model.drafting.cut_fill(None), CutFill::Auto);
}

#[test]
fn three_tags_get_three_fills_and_outlines_on_plans_and_sections() {
    let mut e = model();
    three_fills(&mut e);
    let pens = Pens::new(false);
    for (what, d) in [("plan", plan_sheet(&e)), ("section", section_sheet(&e))] {
        let cuts = cut_paths(&d);
        // the slab: solid red with a 0.7 outline
        assert!(cuts.iter().any(|(rgb, p)| *rgb == [255, 0, 0] && p.as_ref().is_some_and(|p| (p.w - 0.7).abs() < 1e-9)), "{what}: {cuts:?}");
        // the furniture: blank (paper) with a 0.18 outline
        assert!(cuts.iter().any(|(rgb, p)| *rgb == pens.paper && p.as_ref().is_some_and(|p| (p.w - 0.18).abs() < 1e-9)), "{what}: {cuts:?}");
        // the walls: hatched over paper, outlined at 0.5
        assert!(hatch_count(&d) >= 1, "{what}");
        assert!(outlines(&d).iter().any(|w| (w - 0.5).abs() < 1e-9), "{what}");
        // nothing is the default poché any more
        assert!(cuts.iter().all(|(rgb, _)| *rgb != pens.cut_fill), "{what}: {cuts:?}");
    }
    // a cut outline is never dashed, even when the tag's lines are
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "line": "dashed", "color": "#0000ff"}));
    let d = plan_sheet(&e);
    let mut slab_pen = None;
    d.pages[0].walk(&mut |p| {
        if let Prim::Path(PathPrim { fill: Some(f), pen: Some(pen), .. }) = p {
            if f.rgb == [255, 0, 0] {
                slab_pen = Some(pen.clone());
            }
        }
    });
    let slab_pen = slab_pen.unwrap();
    assert!(slab_pen.dash.is_empty());
    assert_eq!(slab_pen.rgb, [0, 0, 255], "the outline is in the tag's line colour");
}

#[test]
fn svg_pdf_and_dxf_draw_each_fill() {
    let mut e = model();
    three_fills(&mut e);
    let d = plan_sheet(&e);
    let page = &d.pages[0];
    let svg = draft::svg::page(page);
    assert!(svg.contains("fill=\"#ff0000\""), "solid slab");
    assert!(svg.contains("data-hatch=\"brick\""), "hatched walls");
    assert!(svg.contains("stroke-width=\"0.7\"") && svg.contains("stroke-width=\"0.18\"") && svg.contains("stroke-width=\"0.5\""));
    let pdf = pdftext::parse(&draft::pdf::write(&d).unwrap());
    let pg = pdf.pages()[0];
    let c = pdf.content(pg);
    assert!(c.contains("1 0 0 rg"), "the red fill");
    let widths = pdf.line_widths_mm(pg);
    for w in [0.7, 0.18, 0.5] {
        assert!(widths.iter().any(|x| (x - w).abs() < 0.01), "{w} in {widths:?}");
    }
    // DXF: a solid HATCH in red for the slab, a pattern HATCH for the walls, outlines with their lineweights
    let dxf = draft::dxf::page(page);
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let pairs: Vec<(&str, &str)> = lines.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect();
    assert!(pairs.contains(&("2", "BRICK")));
    assert!(pairs.contains(&("420", "16711680")), "a red entity");
    assert!(pairs.contains(&("2", "SOLID")), "solid fills are solid hatches");
    for lw in ["70", "18", "50"] {
        assert!(pairs.contains(&("370", lw)), "lineweight {lw}");
    }
}

#[test]
fn saved_sheets_export_with_each_cut_fill() {
    let dir = scratch("saved");
    let mut e = model();
    three_fills(&mut e);
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "P", "paper": "a3"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "rect_mm": [15, 15, 250, 180]}));
    let path = dir.join("p.svg").to_string_lossy().to_string();
    ok(&mut e, json!({"cmd": "export_sheets", "path": path}));
    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(svg.contains("fill=\"#ff0000\"") && svg.contains("data-hatch=\"brick\""));
    std::fs::remove_dir_all(&dir).ok();
}

// ---- the command, undo, file, policy --------------------------------------------------------------------

#[test]
fn tag_style_cut_changes_only_what_is_given_and_checks_it() {
    let mut e = model();
    let r = ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "solid", "color": "#00AA00"}}));
    assert_eq!(r["style"]["cut"]["fill"], "solid");
    assert_eq!(r["style"]["cut"]["color"], "#00aa00");
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"outline_mm": 1.0}}));
    let st = e.model.drafting.tag_style("Slab").unwrap().clone();
    assert_eq!((st.cut.fill, st.cut.color.as_str(), st.cut.outline_mm), (CutFill::Solid, "#00aa00", 1.0), "the fill and colour stayed");
    // the line weight is apart from the cut outline
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "weight_mm": 0.25}));
    assert_eq!(e.model.drafting.tag_style("Slab").unwrap().cut.outline_mm, 1.0);
    // bad values change nothing
    let before = e.model.drafting.clone();
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "gradient"}})).contains("a cut fill is"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"color": "red"}})).contains("#rrggbb"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"outline_mm": 3}})).contains("between"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"outline_mm": -0.5}})).contains("between"));
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "hatch"}})).contains("no hatch"), "a hatch fill needs a hatch");
    assert!(fails(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "hatch"}, "hatch": {"pattern": ""}})).contains("no hatch"));
    assert_eq!(e.model.drafting, before);
    // a hatch and the fill in one command; picking solid keeps the hatch for later
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "hatch"}, "hatch": {"pattern": "concrete"}}));
    assert_eq!(e.model.drafting.cut_hatch(Some("Slab")).unwrap().pattern, "concrete");
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "solid"}}));
    assert!(e.model.drafting.cut_hatch(Some("Slab")).is_none());
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "hatch"}}));
    assert_eq!(e.model.drafting.cut_hatch(Some("Slab")).unwrap().pattern, "concrete", "the hatch came back");
    // removing the hatch of a hatch-filled tag goes back to the default fill
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "hatch": {"pattern": ""}}));
    assert_eq!(e.model.drafting.cut_fill(Some("Slab")), CutFill::Auto);
    // clearing every part leaves no style at all
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Slab", "weight_mm": 0, "cut": {"fill": "auto", "color": "", "outline_mm": 0}}));
    assert!(e.model.drafting.tag_style("Slab").is_none(), "{:?}", e.model.drafting.tag_styles);
    // a tag that has none yet gets one
    ok(&mut e, json!({"cmd": "tag_style", "tag": "New", "cut": {"fill": "none"}}));
    assert_eq!(e.model.drafting.cut_fill(Some("New")), CutFill::None);
}

#[test]
fn cut_styles_undo_one_step_each_and_survive_save_and_open() {
    let dir = scratch("file");
    let mut e = model();
    three_fills(&mut e);
    let a = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": a}));
    let f: Value = serde_json::from_slice(&std::fs::read(&a).unwrap()).unwrap();
    assert_eq!(f["format"], 2, "cut styles need the newer format");
    assert_eq!(f["drafting"]["tag_styles"]["Slab"]["cut"]["fill"], "solid");
    assert_eq!(f["drafting"]["tag_styles"]["Slab"]["cut"]["color"], "#ff0000");
    assert_eq!(f["drafting"]["tag_styles"]["Slab"]["cut"]["outline_mm"], 0.7);
    assert_eq!(f["drafting"]["tag_styles"]["Furniture"]["cut"]["fill"], "none");
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": a}));
    assert_eq!(g.model.drafting, e.model.drafting);
    let svg = |e: &Engine| draft::svg::page(&plan_sheet(e).pages[0]);
    assert_eq!(svg(&g), svg(&e), "the opened file draws the same");
    // undo and redo: one step per command
    ok(&mut g, json!({"cmd": "tag_style", "tag": "Furniture", "cut": {"fill": "solid", "color": "#123456"}}));
    ok(&mut g, json!({"cmd": "undo"}));
    assert_eq!(g.model.drafting.cut_fill(Some("Furniture")), CutFill::None);
    ok(&mut g, json!({"cmd": "redo"}));
    assert_eq!(g.model.drafting.cut_fill(Some("Furniture")), CutFill::Solid);
    let mut e2 = model();
    ok(&mut e2, json!({"cmd": "tag_style", "tag": "Slab", "cut": {"fill": "none", "outline_mm": 0.5}}));
    ok(&mut e2, json!({"cmd": "undo"}));
    assert!(e2.model.drafting.tag_style("Slab").is_none());
    // an old file's style (a "fill" this build has a name for, unknown fields kept) still opens
    let mut f2 = f.clone();
    f2["drafting"]["tag_styles"]["Slab"]["cut"]["future"] = json!(1);
    let b = dir.join("b.sketchcraft");
    std::fs::write(&b, f2.to_string()).unwrap();
    let mut h = Engine::new();
    ok(&mut h, json!({"cmd": "open", "path": b}));
    assert_eq!(h.model.drafting.cut_fill(Some("Slab")), CutFill::Solid);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn agents_and_plugins_may_set_cut_fills() {
    for who in [Caller::Plugin, Caller::Mcp(Workspace::anywhere())] {
        assert!(permits(&who, "tag_style"));
    }
    let mut c = json!({"cmd": "tag_style", "tag": "Walls", "cut": {"fill": "solid", "color": "#ff0000"}});
    assert!(vet(&Caller::Mcp(Workspace::anywhere()), &mut c).is_ok());
}

// ---- on screen -----------------------------------------------------------------------------------------

fn nums(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

#[test]
fn the_section_command_gives_each_tag_its_fill_and_outline() {
    let mut e = model();
    let plain = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    let all = plain["fill"].as_array().unwrap().len();
    assert!(all > 0 && plain["solid"].as_array().unwrap().is_empty() && plain["open"].as_array().unwrap().is_empty() && plain["outlines"].as_array().unwrap().is_empty());
    three_fills(&mut e);
    let r = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    // the slab: its own red triangles, inside its footprint
    let solid = r["solid"].as_array().unwrap();
    assert_eq!(solid.len(), 1, "{r}");
    assert_eq!((solid[0]["tag"].as_str(), solid[0]["color"].as_str()), (Some("Slab"), Some("#ff0000")));
    let tri = nums(&solid[0]["fill"]);
    assert!(!tri.is_empty() && tri.len().is_multiple_of(9));
    for p in tri.chunks(3) {
        assert!(p[0] > 2.0 - 1e-6 && p[0] < 4.0 + 1e-6 && (p[2] - 1.2).abs() < 1e-6, "{p:?}");
    }
    // the furniture: no cap at all, just listed
    assert_eq!(r["open"], json!(["Furniture"]));
    // the walls: paper under the hatch, hatch lines
    assert!(!r["hatchFill"].as_array().unwrap().is_empty());
    assert_eq!(r["hatch"][0]["tag"], "Walls");
    // nothing is left in the default fill
    assert!(r["fill"].as_array().unwrap().is_empty(), "{}", r["fill"]);
    // outlines per tag with their weights, as closed rings on the plane
    let ol = r["outlines"].as_array().unwrap();
    let w: Vec<(&str, f64)> = ol.iter().map(|o| (o["tag"].as_str().unwrap(), o["weight_mm"].as_f64().unwrap())).collect();
    assert_eq!(w, vec![("Furniture", 0.18), ("Slab", 0.7), ("Walls", 0.5)]);
    for o in ol {
        let l = nums(&o["lines"]);
        assert!(l.len() >= 4 * 6 && l.len().is_multiple_of(6));
        assert!(l.chunks(3).all(|p| (p[2] - 1.2).abs() < 1e-6));
    }
    // the cut lines themselves are unchanged
    assert_eq!(r["lines"], plain["lines"]);
}

#[test]
fn a_component_being_edited_shows_its_tags_cut_fills() {
    let mut e = Engine::new();
    block(&mut e, [0.0, 0.0], [1.0, 1.0], 2.5);
    let ids: Vec<u64> = e.model.faces.keys().map(|k| *k as u64).collect();
    let inst = ok(&mut e, json!({"cmd": "make_component", "ids": ids, "name": "Desk"}))["instance"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "tag_style", "tag": "Glass", "cut": {"fill": "solid", "color": "#88ccff"}}));
    ok(&mut e, json!({"cmd": "edit", "id": inst}));
    let g = group(&mut e, "Glass", [2.0, 0.0], [3.0, 1.0]);
    ok(&mut e, json!({"cmd": "tag", "ids": [g], "tag": "Glass"}));
    let r = ok(&mut e, json!({"cmd": "section", "origin": [0, 0, 1.2], "normal": [0, 0, -1]}));
    assert_eq!(r["solid"][0]["tag"], "Glass", "{r}");
    assert_eq!(r["regions"], 1, "only the glass group closes into a region: the loose box is loose faces, as before");
    ok(&mut e, json!({"cmd": "close"}));
}
