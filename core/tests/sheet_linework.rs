//! Paper line work on saved sheets, and trim, extend, offset and chain dimensions (#19, design step 10): the commands,
//! undo, save and open, policy, and the work showing in the PDF, SVG and DXF.

#[path = "pdftext/mod.rs"]
mod pdftext;

use serde_json::{json, Value};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;
use std::collections::BTreeSet;
use std::path::PathBuf;

const COLOR: &str = "#12ab34";
const WEIGHT: f64 = 0.7;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-linework-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(e: &mut Engine, v: Value) -> String {
    let before = (e.undo_depth(), sheet_items(e));
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    assert_eq!((e.undo_depth(), sheet_items(e)), before, "a refused command changes nothing and takes no undo step");
    r["error"].as_str().unwrap().to_string()
}

fn sheet_items(e: &Engine) -> Value {
    serde_json::to_value(&e.model.sheet_items).unwrap()
}

fn item(e: &Engine, id: u64) -> Value {
    serde_json::to_value(&*e.model.sheet_items[&(id as u32)]).unwrap()
}

fn pts(v: &Value) -> Vec<[f64; 2]> {
    v["pts_mm"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

/// A wall (x .. x + 0.2, y 0 to 3, 2.5 high) as a group, and an A3 sheet with a 1:50 plan viewport. The plan is pinned
/// to its anchor, the middle of the wall at x = 0 (a saved viewport is drawn at its anchor), so the wall's left face is at paper x = 203 and its right at 207, running from y = 145 up to
/// y = 85 (20 paper mm per model metre). The viewport's frame is [15, 15, 380, 200].
fn site(wall_x: f64) -> (Engine, u64) {
    let mut e = Engine::new();
    let f = ok(&mut e, json!({"cmd": "rect", "from": [wall_x, 0, 0], "to": [wall_x + 0.2, 3.0, 0]}))["face"].clone();
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let faces: Vec<u32> = e.model.faces.keys().copied().collect();
    ok(&mut e, json!({"cmd": "group", "ids": faces}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plan", "paper": "a3"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "anchor": [0.1, 1.5, 1.2], "rect_mm": [15, 15, 380, 200]}));
    (e, s)
}

fn add_line(e: &mut Engine, s: u64, a: [f64; 2], b: [f64; 2]) -> u64 {
    ok(e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [a, b], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap()
}

/// The sheet exported, as SVG text.
fn svg(e: &mut Engine, s: u64, dir: &std::path::Path) -> String {
    let p = dir.join("s.svg");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    std::fs::read_to_string(p).unwrap()
}

/// The point lists of the SVG paths drawn in our test colour: each path's numbers, as pairs.
fn svg_paths(svg: &str) -> Vec<Vec<[f64; 2]>> {
    svg.lines()
        .filter(|l| l.contains(&format!("stroke=\"{COLOR}\"")) && l.contains(&format!("stroke-width=\"{WEIGHT}\"")))
        .map(|l| {
            let d = l.split("d=\"").nth(1).unwrap().split('"').next().unwrap();
            let nums: Vec<f64> = d.replace(['M', 'L', 'C', 'Z'], " ").split_whitespace().map(|n| n.parse().unwrap()).collect();
            nums.chunks(2).map(|c| [c[0], c[1]]).collect()
        })
        .collect()
}

/// The DXF's entities in our colour: (kind, its points, y up from the page's bottom).
fn dxf_ents(e: &mut Engine, s: u64, dir: &std::path::Path) -> Vec<(String, Vec<[f64; 2]>)> {
    let p = dir.join("s.dxf");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    let text = std::fs::read_to_string(p).unwrap();
    let l: Vec<&str> = text.lines().collect();
    let g: Vec<(i32, String)> = l.chunks(2).filter(|c| c.len() == 2).filter_map(|c| Some((c[0].trim().parse().ok()?, c[1].trim().to_string()))).collect();
    let want = u32::from_str_radix(&COLOR[1..], 16).unwrap().to_string();
    let mut out = vec![];
    let mut i = 0;
    while i < g.len() {
        if g[i].0 == 0 && (g[i].1 == "LINE" || g[i].1 == "LWPOLYLINE") {
            let kind = g[i].1.clone();
            let (mut col, mut lw) = (String::new(), 0);
            let (mut xs, mut ys) = (vec![], vec![]);
            i += 1;
            while i < g.len() && g[i].0 != 0 {
                match g[i].0 {
                    420 => col = g[i].1.clone(),
                    370 => lw = g[i].1.parse().unwrap(),
                    10 | 11 => xs.push(g[i].1.parse::<f64>().unwrap()),
                    20 | 21 => ys.push(g[i].1.parse::<f64>().unwrap()),
                    _ => {}
                }
                i += 1;
            }
            if col == want {
                assert_eq!(lw, 70, "the pen's weight is the entity's lineweight");
                out.push((kind, xs.into_iter().zip(ys).map(|(x, y)| [x, y]).collect()));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn pdf_widths(e: &mut Engine, s: u64, dir: &std::path::Path) -> Vec<f64> {
    let p = dir.join("s.pdf");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    let pdf = pdftext::parse(&std::fs::read(p).unwrap());
    pdf.line_widths_mm(pdf.pages()[0])
}

// ---- drawing the line work --------------------------------------------------------------------

#[test]
fn every_kind_is_saved_and_drawn_in_pdf_svg_and_dxf() {
    let dir = scratch("kinds");
    let (mut e, s) = site(0.0);
    let line = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[20, 230], [120, 230]], "w_mm": WEIGHT, "color": COLOR}));
    assert!(line["item"].is_u64());
    let poly = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "polyline", "pts_mm": [[20, 240], [60, 240], [60, 260], [100, 260]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    let arc = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[200, 250], [225, 225], [250, 250]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    let rect = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[300, 220], [360, 260]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    // what is kept
    assert_eq!(item(&e, line["item"].as_u64().unwrap())["kind"], "line");
    assert_eq!(item(&e, poly)["kind"], "polyline");
    assert_eq!(item(&e, arc)["kind"], "arc");
    assert_eq!(item(&e, rect)["kind"], "rect");
    assert_eq!(item(&e, rect)["pen"]["color"], COLOR);
    // SVG: four paths in our pen; the rect is closed; the arc is curves
    let s_svg = svg(&mut e, s, &dir);
    let paths = svg_paths(&s_svg);
    assert_eq!(paths.len(), 4, "{s_svg}");
    assert!(paths.iter().any(|p| p == &vec![[20.0, 230.0], [120.0, 230.0]]));
    assert!(paths.iter().any(|p| p.len() == 4 && near(p[3], [100.0, 260.0])));
    let rect_line = s_svg.lines().find(|l| l.contains("M300 220L360 220L360 260L300 260Z")).expect("a closed rectangle");
    assert!(rect_line.contains(&format!("stroke=\"{COLOR}\"")));
    let arc_line = s_svg.lines().find(|l| l.contains("M200 250C")).expect("an arc as curves");
    assert!(arc_line.contains("250 250\""), "ends where it should: {arc_line}");
    // PDF: the pen's width is in the page
    let w = pdf_widths(&mut e, s, &dir);
    assert!(w.iter().any(|x| (x - WEIGHT).abs() < 0.01), "{w:?}");
    // DXF: lines and polylines in the pen, y up from the page's bottom (A3 landscape is 297 high)
    let ents = dxf_ents(&mut e, s, &dir);
    assert_eq!(ents.len(), 4, "{ents:?}");
    assert!(ents.iter().any(|(k, p)| k == "LINE" && near(p[0], [20.0, 297.0 - 230.0]) && near(p[1], [120.0, 297.0 - 230.0])));
    assert!(ents.iter().any(|(k, p)| k == "LWPOLYLINE" && p.len() == 4));
    // the arc is a polyline of many points; the rectangle a closed one
    assert!(ents.iter().any(|(k, p)| k == "LWPOLYLINE" && p.len() > 8 && near(p[0], [200.0, 297.0 - 250.0])));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn line_work_is_in_the_file_and_undoes_as_one_step() {
    let dir = scratch("saved");
    let (mut e, s) = site(0.0);
    let depth = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "lines": [
        {"pts_mm": [[20, 230], [120, 230]], "w_mm": 0.5},
        {"kind": "rect", "pts_mm": [[20, 235], [60, 250]], "dash_mm": [3, 1]},
        {"kind": "arc", "pts_mm": [[200, 250], [225, 225], [250, 250]], "z": 4}]}));
    assert_eq!(r["items"].as_array().unwrap().len(), 3);
    assert_eq!(e.undo_depth(), depth + 1, "one batch, one step");
    let path = dir.join("a.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": path}));
    assert_eq!(sheet_items(&g), sheet_items(&e), "paper line work survives save and open");
    let before = sheet_items(&e);
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), 1, "only the viewport is left");
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(sheet_items(&e), before);
    // the saved pen
    let id = r["items"][1].as_u64().unwrap();
    assert_eq!(item(&e, id)["pen"]["dash_mm"], json!([3.0, 1.0]));
    // delete goes by the existing command
    ok(&mut e, json!({"cmd": "sheet_item_delete", "ids": [id]}));
    assert!(!e.model.sheet_items.contains_key(&(id as u32)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn line_work_is_edited_by_id_and_a_bad_one_changes_nothing() {
    let (mut e, s) = site(0.0);
    let id = add_line(&mut e, s, [20.0, 230.0], [120.0, 230.0]);
    ok(&mut e, json!({"cmd": "sheet_line", "id": id, "pts_mm": [[20, 235], [140, 235]], "w_mm": 0.35}));
    let it = item(&e, id);
    assert_eq!(pts(&it), vec![[20.0, 235.0], [140.0, 235.0]]);
    assert_eq!(it["pen"]["w_mm"], 0.35);
    assert_eq!(it["pen"]["color"], COLOR, "what was not given stays");
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(pts(&item(&e, id)), vec![[20.0, 230.0], [120.0, 230.0]]);
    // refusals
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "id": id, "kind": "rect"})).contains("stays"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "id": 999999, "w_mm": 0.3})).contains("no sheet item"));
    let vp = *e.model.sheet_items.iter().find(|(_, i)| matches!(&***i, sketchcraft::drafting::SheetItem::Known(sketchcraft::drafting::ItemKind::Viewport(_)))).unwrap().0;
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "id": vp, "w_mm": 0.3})).contains("not paper line work"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "pts_mm": [[0, 0], [1, 1]]})).contains("sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": 99999, "pts_mm": [[0, 0], [1, 1]]})).contains("no sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [0, 0]]})).contains("no length"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0]]})).contains("points"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[0, 0], [5, 5], [10, 10]]})).contains("circle"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[0, 0], [5, 5]]})).contains("3 points"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[0, 0], [5, 0]]})).contains("height"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "line", "pts_mm": [[0, 0], [5, 5], [9, 9]]})).contains("2 points"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "squiggle", "pts_mm": [[0, 0], [5, 5]]})).contains("kind"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [1e9, 1]]})).contains("paper position"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [5, 5]], "w_mm": 9})).contains("pen width"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [5, 5]], "color": "red"})).contains("#rrggbb"));
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[0, 0], [5, 5]], "dash_mm": [0, 2]})).contains("dash_mm"));
    // a batch with one bad line adds none of them
    assert!(refused(&mut e, json!({"cmd": "sheet_line", "sheet": s, "lines": [{"sheet": s, "pts_mm": [[0, 0], [5, 5]]}, {"sheet": s, "pts_mm": [[0, 0], [0, 0]]}]})).contains("no length"));
}

// ---- trim -------------------------------------------------------------------------------------

#[test]
fn trim_cuts_the_span_between_two_lines_in_one_undo_step() {
    let dir = scratch("trim");
    let (mut e, s) = site(0.0);
    let a = add_line(&mut e, s, [20.0, 230.0], [200.0, 230.0]);
    add_line(&mut e, s, [60.0, 220.0], [60.0, 245.0]);
    add_line(&mut e, s, [140.0, 220.0], [140.0, 245.0]);
    let before = sheet_items(&e);
    let depth = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 230]}));
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(r["created"].as_array().unwrap().len(), 1);
    assert!(near([r["cut"]["from"][0].as_f64().unwrap(), 230.0], [60.0, 230.0]) && near([r["cut"]["to"][0].as_f64().unwrap(), 230.0], [140.0, 230.0]));
    let second = r["created"][0].as_u64().unwrap();
    assert_eq!(pts(&item(&e, a)), vec![[20.0, 230.0], [60.0, 230.0]]);
    assert_eq!(pts(&item(&e, second)), vec![[140.0, 230.0], [200.0, 230.0]]);
    assert_eq!(item(&e, second)["pen"], item(&e, a)["pen"], "the other piece keeps the pen");
    assert_eq!(item(&e, second)["kind"], "line");
    // the drawings carry the trimmed result, not the original
    let paths = svg_paths(&svg(&mut e, s, &dir));
    assert!(paths.contains(&vec![[20.0, 230.0], [60.0, 230.0]]) && paths.contains(&vec![[140.0, 230.0], [200.0, 230.0]]));
    assert!(!paths.iter().any(|p| p.len() == 2 && (p[0][0] - p[1][0]).abs() > 150.0), "no full-length line left: {paths:?}");
    let dxf = dxf_ents(&mut e, s, &dir);
    assert!(dxf.iter().any(|(_, p)| near(p[0], [140.0, 297.0 - 230.0]) && near(p[1], [200.0, 297.0 - 230.0])), "{dxf:?}");
    assert!(!dxf.iter().any(|(_, p)| p.len() == 2 && (p[0][0] - p[1][0]).abs() > 150.0));
    // save and open
    let path = dir.join("t.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": path}));
    assert_eq!(sheet_items(&g), sheet_items(&e));
    // undo and redo
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(sheet_items(&e), before);
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(pts(&item(&e, a)), vec![[20.0, 230.0], [60.0, 230.0]]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn trim_an_end_span_and_trim_to_the_end() {
    let (mut e, s) = site(0.0);
    let a = add_line(&mut e, s, [20.0, 230.0], [200.0, 230.0]);
    add_line(&mut e, s, [60.0, 220.0], [60.0, 245.0]);
    add_line(&mut e, s, [140.0, 220.0], [140.0, 245.0]);
    // the span at the left end goes: one piece stays, and the item keeps its id
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [30, 230]}));
    assert_eq!(r["created"], json!([]));
    assert_eq!(r["kept"], json!([a]));
    assert_eq!(pts(&item(&e, a)), vec![[60.0, 230.0], [200.0, 230.0]]);
    // now only the line at x = 140 is left to cross it past the pick: trim the right part away to the end
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [180, 230]}));
    assert!(near([r["cut"]["to"][0].as_f64().unwrap(), r["cut"]["to"][1].as_f64().unwrap()], [200.0, 230.0]));
    assert_eq!(pts(&item(&e, a)), vec![[60.0, 230.0], [140.0, 230.0]]);
}

#[test]
fn trim_refuses_what_it_cannot_do_and_says_why() {
    let (mut e, s) = site(0.0);
    let a = add_line(&mut e, s, [20.0, 230.0], [200.0, 230.0]);
    // nothing crosses it
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 230]})).contains("nothing crosses"));
    // parallel and collinear lines, and one that only touches an end, are not crossings
    add_line(&mut e, s, [20.0, 240.0], [200.0, 240.0]);
    add_line(&mut e, s, [50.0, 230.0], [90.0, 230.0]);
    add_line(&mut e, s, [200.0, 220.0], [200.0, 245.0]);
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 230]})).contains("nothing crosses"));
    // a pick away from the line
    add_line(&mut e, s, [60.0, 220.0], [60.0, 245.0]);
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 236]})).contains("pick a point on it"));
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 232], "pick_tol_mm": 500})).contains("pick_tol_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [f64::MAX, 230]})).contains("at_mm"));
    // not line work, not there at all
    let vp = *e.model.sheet_items.iter().find(|(_, i)| i.kind_name() == "viewport").unwrap().0;
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": vp, "at_mm": [100, 100]})).contains("never is"));
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": 987654, "at_mm": [1, 1]})).contains("no sheet item"));
    // the one that does work, with the tolerance
    ok(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [100, 231.5]}));
}

#[test]
fn trim_a_rect_arc_and_polyline() {
    let (mut e, s) = site(0.0);
    // a rect cut by a line through it opens into a polyline
    let r = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[20, 225], [80, 255]]}))["item"].as_u64().unwrap();
    add_line(&mut e, s, [50.0, 215.0], [50.0, 265.0]);
    ok(&mut e, json!({"cmd": "sheet_trim", "item": r, "at_mm": [30, 225]}));
    let it = item(&e, r);
    assert_eq!(it["kind"], "polyline");
    let p = pts(&it);
    assert!(near(p[0], [50.0, 225.0]) && near(*p.last().unwrap(), [50.0, 255.0]) && p.len() == 4, "{p:?}: the open outline runs from one crossing round the far side to the other");
    // an arc trimmed at a line through its top
    let arc = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[200, 250], [225, 225], [250, 250]]}))["item"].as_u64().unwrap();
    add_line(&mut e, s, [225.0, 200.0], [225.0, 260.0]);
    ok(&mut e, json!({"cmd": "sheet_trim", "item": arc, "at_mm": [203, 240]}));
    let it = item(&e, arc);
    assert_eq!(it["kind"], "arc");
    let p = pts(&it);
    assert!(near(p[0], [225.0, 225.0]) && near(p[2], [250.0, 250.0]), "{p:?}");
    // a polyline cut at a crossing in its middle segment
    let pl = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "polyline", "pts_mm": [[300, 220], [340, 220], [340, 260]]}))["item"].as_u64().unwrap();
    add_line(&mut e, s, [330.0, 210.0], [330.0, 230.0]);
    add_line(&mut e, s, [320.0, 250.0], [360.0, 250.0]);
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": pl, "at_mm": [340, 230]}));
    assert_eq!(r["created"].as_array().unwrap().len(), 1);
    assert_eq!(pts(&item(&e, pl)), vec![[300.0, 220.0], [330.0, 220.0]]);
    assert_eq!(pts(&item(&e, r["created"][0].as_u64().unwrap())), vec![[340.0, 250.0], [340.0, 260.0]]);
}

#[test]
fn trim_uses_the_models_lines_and_viewport_frames_as_boundaries_but_never_trims_them() {
    let (mut e, s) = site(0.0);
    let model_before = serde_json::to_value(&e.model.vertices).unwrap();
    // across the wall (paper x 203 to 207): the span between its two faces goes
    let a = add_line(&mut e, s, [180.0, 115.0], [230.0, 115.0]);
    let r = ok(&mut e, json!({"cmd": "sheet_trim", "item": a, "at_mm": [205, 115]}));
    assert!(r["boundaries"]["model_lines"].as_u64().unwrap() > 0, "{r}");
    assert!(r["boundaries"]["viewport_frames"].as_u64().unwrap() >= 4);
    let first = pts(&item(&e, a));
    assert!(near(first[0], [180.0, 115.0]) && (first[1][0] - 203.0).abs() < 0.05, "{first:?}");
    let second = pts(&item(&e, r["created"][0].as_u64().unwrap()));
    assert!((second[0][0] - 207.0).abs() < 0.05 && near(second[1], [230.0, 115.0]), "{second:?}");
    // with the model's lines left out, the same line has nothing to trim to
    let b = add_line(&mut e, s, [180.0, 125.0], [230.0, 125.0]);
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": b, "at_mm": [205, 125], "use_model": false})).contains("nothing crosses"));
    // a line leaving the viewport is cut at its frame (x = 15)
    let c = add_line(&mut e, s, [5.0, 60.0], [100.0, 60.0]);
    ok(&mut e, json!({"cmd": "sheet_trim", "item": c, "at_mm": [10, 60]}));
    assert!(near(pts(&item(&e, c))[0], [15.0, 60.0]), "{:?}", pts(&item(&e, c)));
    // the model was not touched (model lines are boundaries, never trimmed)
    assert_eq!(serde_json::to_value(&e.model.vertices).unwrap(), model_before);
    // and a viewport itself is not line work
    let vp = *e.model.sheet_items.iter().find(|(_, i)| i.kind_name() == "viewport").unwrap().0;
    assert!(refused(&mut e, json!({"cmd": "sheet_trim", "item": vp, "at_mm": [100, 15]})).contains("not paper line work"));
}

// ---- extend -----------------------------------------------------------------------------------

#[test]
fn extend_runs_the_nearest_end_to_a_line_an_arc_a_model_line_or_a_frame() {
    let (mut e, s) = site(0.0);
    // to a line
    let a = add_line(&mut e, s, [20.0, 230.0], [60.0, 230.0]);
    add_line(&mut e, s, [100.0, 220.0], [100.0, 245.0]);
    add_line(&mut e, s, [90.0, 220.0], [90.0, 245.0]);
    let depth = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_extend", "item": a, "at_mm": [58, 231]}));
    assert_eq!(e.undo_depth(), depth + 1);
    assert_eq!(r["end"], "end");
    assert_eq!(pts(&item(&e, a)), vec![[20.0, 230.0], [90.0, 230.0]], "the nearest boundary ahead");
    // the other end, to a frame edge? (the viewport frame's bottom is at y = 215): a vertical line below it
    let v = add_line(&mut e, s, [300.0, 270.0], [300.0, 250.0]);
    ok(&mut e, json!({"cmd": "sheet_extend", "item": v, "at_mm": [300, 251]}));
    assert!(near(pts(&item(&e, v))[1], [300.0, 215.0]), "{:?}", pts(&item(&e, v)));
    // to an arc: a line from inside a circle runs out to it
    let arc = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[200, 250], [225, 225], [250, 250]]}))["item"].as_u64().unwrap();
    let l = add_line(&mut e, s, [225.0, 250.0], [225.0, 240.0]);
    ok(&mut e, json!({"cmd": "sheet_extend", "item": l, "at_mm": [225, 241]}));
    assert!(near(pts(&item(&e, l))[1], [225.0, 225.0]), "{:?}", pts(&item(&e, l)));
    // an arc runs on round its circle to a line
    let rest = add_line(&mut e, s, [190.0, 262.0], [260.0, 262.0]);
    let _ = rest;
    ok(&mut e, json!({"cmd": "sheet_extend", "item": arc, "at_mm": [250, 250]}));
    let ap = pts(&item(&e, arc));
    assert!(near(ap[0], [200.0, 250.0]) && (ap[2][1] - 262.0).abs() < 1e-6, "{ap:?}");
    // a model line: a short line left of the wall runs to its left face (x = 203)
    let m = add_line(&mut e, s, [160.0, 115.0], [190.0, 115.0]);
    let r = ok(&mut e, json!({"cmd": "sheet_extend", "item": m, "at_mm": [189, 115]}));
    assert!((pts(&item(&e, m))[1][0] - 203.0).abs() < 0.05, "{:?}", pts(&item(&e, m)));
    assert!(r["boundaries"]["model_lines"].as_u64().unwrap() > 0);
    // undo puts it back
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(pts(&item(&e, m)), vec![[160.0, 115.0], [190.0, 115.0]]);
}

#[test]
fn extend_a_polyline_and_refuse_what_cannot_be_extended() {
    let (mut e, s) = site(0.0);
    let p = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "polyline", "pts_mm": [[20, 260], [60, 260], [60, 240]]}))["item"].as_u64().unwrap();
    add_line(&mut e, s, [10.0, 225.0], [100.0, 225.0]);
    ok(&mut e, json!({"cmd": "sheet_extend", "item": p, "at_mm": [60, 241]}));
    assert_eq!(pts(&item(&e, p)), vec![[20.0, 260.0], [60.0, 260.0], [60.0, 225.0]]);
    // nothing ahead: a horizontal line out in the margin, parallel to everything
    let lone = add_line(&mut e, s, [20.0, 285.0], [40.0, 285.0]);
    assert!(refused(&mut e, json!({"cmd": "sheet_extend", "item": lone, "at_mm": [40, 285]})).contains("nothing to extend to"));
    // closed shapes have no end; other items are not line work
    let r = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[300, 220], [360, 260]]}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_extend", "item": r, "at_mm": [300, 220]})).contains("closed"));
    let vp = *e.model.sheet_items.iter().find(|(_, i)| i.kind_name() == "viewport").unwrap().0;
    assert!(refused(&mut e, json!({"cmd": "sheet_extend", "item": vp, "at_mm": [15, 15]})).contains("not paper line work"));
    assert!(refused(&mut e, json!({"cmd": "sheet_extend", "item": lone, "at_mm": [200, 285]})).contains("pick a point"));
}

// ---- offset -----------------------------------------------------------------------------------

#[test]
fn offset_makes_a_copy_of_each_kind_with_the_same_pen() {
    let dir = scratch("offset");
    let (mut e, s) = site(0.0);
    let depth = e.undo_depth();
    // a line, to the side of a point
    let l = add_line(&mut e, s, [20.0, 230.0], [120.0, 230.0]);
    let depth_l = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 6, "at_mm": [60, 260]}));
    assert_eq!(e.undo_depth(), depth_l + 1);
    let c = r["item"].as_u64().unwrap();
    assert_ne!(c, l);
    assert_eq!(pts(&item(&e, c)), vec![[20.0, 236.0], [120.0, 236.0]]);
    assert_eq!(item(&e, c)["pen"], item(&e, l)["pen"]);
    assert_eq!(pts(&item(&e, l)), vec![[20.0, 230.0], [120.0, 230.0]], "the original stays");
    let by_side = ok(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 6, "side": "left"}))["item"].as_u64().unwrap();
    assert_eq!(pts(&item(&e, by_side)), vec![[20.0, 224.0], [120.0, 224.0]]);
    // a polyline: mitred corners
    let pl = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "polyline", "pts_mm": [[200, 220], [250, 220], [250, 270]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    let c = ok(&mut e, json!({"cmd": "sheet_offset", "item": pl, "distance_mm": 5, "side": "right"}))["item"].as_u64().unwrap();
    assert_eq!(pts(&item(&e, c)), vec![[200.0, 225.0], [245.0, 225.0], [245.0, 270.0]]);
    assert_eq!(item(&e, c)["kind"], "polyline");
    // an arc: concentric
    let arc = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[300, 250], [325, 225], [350, 250]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    let inner = ok(&mut e, json!({"cmd": "sheet_offset", "item": arc, "distance_mm": 3, "at_mm": [325, 245]}))["item"].as_u64().unwrap();
    let ap = pts(&item(&e, inner));
    assert_eq!(item(&e, inner)["kind"], "arc");
    // same centre as the original, radius 3 smaller
    let (c0, r0) = circle(&pts(&item(&e, arc)));
    let (c1, r1) = circle(&ap);
    assert!(near(c0, c1) && (r0 - r1 - 3.0).abs() < 1e-6, "{c0:?} {r0} {c1:?} {r1}");
    // a rect: in and out
    let rect = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[20, 240], [80, 270]], "w_mm": WEIGHT, "color": COLOR}))["item"].as_u64().unwrap();
    let ri = ok(&mut e, json!({"cmd": "sheet_offset", "item": rect, "distance_mm": 4, "side": "inside"}))["item"].as_u64().unwrap();
    assert_eq!(item(&e, ri)["kind"], "rect");
    assert_eq!(pts(&item(&e, ri)), vec![[24.0, 244.0], [76.0, 266.0]]);
    let ro = ok(&mut e, json!({"cmd": "sheet_offset", "item": rect, "distance_mm": 4, "side": "outside"}))["item"].as_u64().unwrap();
    assert_eq!(pts(&item(&e, ro)), vec![[16.0, 236.0], [84.0, 274.0]]);
    // the copies are in the drawings
    let paths = svg_paths(&svg(&mut e, s, &dir));
    assert!(paths.contains(&vec![[20.0, 236.0], [120.0, 236.0]]));
    assert!(dxf_ents(&mut e, s, &dir).len() >= 9);
    // one step each, and undo takes a copy away
    assert!(e.undo_depth() > depth + 5);
    let n = e.model.sheet_items.len();
    ok(&mut e, json!({"cmd": "undo"}));
    assert_eq!(e.model.sheet_items.len(), n - 1);
    std::fs::remove_dir_all(&dir).ok();
}

fn circle(p: &[[f64; 2]]) -> ([f64; 2], f64) {
    let a = sketchcraft::draft::linework::ArcC::from_three(p[0], p[1], p[2]).unwrap();
    (a.c, a.r)
}

#[test]
fn offset_refuses_a_missing_side_and_a_distance_the_shape_cannot_take() {
    let (mut e, s) = site(0.0);
    let l = add_line(&mut e, s, [20.0, 230.0], [120.0, 230.0]);
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 5})).contains("which side"));
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 5, "side": "left", "at_mm": [1, 2]})).contains("not both"));
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 5, "side": "up"})).contains("left, right"));
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 5, "at_mm": [60, 230]})).contains("on the line work itself"));
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": 5, "side": "inside"})).contains("closed"));
    for d in [0.0, -3.0] {
        assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": l, "distance_mm": d, "side": "left"})).contains("distance_mm"));
    }
    let arc = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "arc", "pts_mm": [[300, 250], [325, 225], [350, 250]]}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": arc, "distance_mm": 40, "at_mm": [325, 245]})).contains("collapse"));
    let rect = ok(&mut e, json!({"cmd": "sheet_line", "sheet": s, "kind": "rect", "pts_mm": [[20, 240], [80, 270]]}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": rect, "distance_mm": 15, "side": "inside"})).contains("too much for this shape"));
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": 424242, "distance_mm": 5, "side": "left"})).contains("no sheet item"));
    let vp = *e.model.sheet_items.iter().find(|(_, i)| i.kind_name() == "viewport").unwrap().0;
    assert!(refused(&mut e, json!({"cmd": "sheet_offset", "item": vp, "distance_mm": 5, "side": "left"})).contains("not paper line work"));
}

// ---- policy -----------------------------------------------------------------------------------

#[test]
fn agents_and_plugins_may_draw_trim_extend_and_offset_inside_their_limits() {
    let agent = Caller::Mcp(Workspace::anywhere());
    for c in ["sheet_line", "sheet_trim", "sheet_extend", "sheet_offset"] {
        assert!(permits(&agent, c) && permits(&Caller::Plugin, c) && permits(&Caller::Ui, c), "{c}");
    }
    let lines = |n: usize| json!({"cmd": "sheet_line", "sheet": 1, "lines": vec![json!({"pts_mm": [[0, 0], [1, 1]]}); n]});
    assert!(vet(&agent, &mut lines(5000)).is_ok());
    assert!(vet(&agent, &mut lines(5001)).is_err());
    assert!(vet(&Caller::Ui, &mut lines(5001)).is_ok());
    // an agent's line says so
    let (mut e, s) = site(0.0);
    let r = e.exec_json_as(&agent, &json!({"cmd": "sheet_line", "sheet": s, "pts_mm": [[20, 230], [120, 230]]}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(item(&e, r["item"].as_u64().unwrap())["made_by"], "agent");
    let id = r["item"].as_u64().unwrap();
    add_line(&mut e, s, [60.0, 220.0], [60.0, 245.0]);
    let r = e.exec_json_as(&agent, &json!({"cmd": "sheet_trim", "item": id, "at_mm": [30, 230]}).to_string());
    assert_eq!(r["ok"], true, "{r}");
}

// ---- chains of dimensions ---------------------------------------------------------------------

fn wall(e: &mut Engine, x0: f64) -> u32 {
    let before: BTreeSet<u32> = e.model.faces.keys().copied().collect();
    let f = ok(e, json!({"cmd": "rect", "from": [x0, 0, 0], "to": [x0 + 0.2, 3.0, 0]}))["face"].clone();
    ok(e, json!({"cmd": "pushpull", "face": f, "distance": 2.5}));
    let new: Vec<u32> = e.model.faces.keys().copied().filter(|k| !before.contains(k)).collect();
    ok(e, json!({"cmd": "group", "ids": new}))["group"].as_u64().unwrap() as u32
}

fn chain_site() -> (Engine, u64) {
    let mut e = Engine::new();
    for x in [0.0, 3.0, 5.0, 9.0] {
        wall(&mut e, x);
    }
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Plan", "paper": "a3"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "view": {"type": "plan", "cut_z": 1.2, "floor_z": 0.0}, "scale": "1:50", "anchor": [4.6, 1.5, 1.2], "rect_mm": [15, 15, 380, 200]}));
    (e, s)
}

fn texts(e: &mut Engine, s: u64, name: &str) -> String {
    let dir = scratch(name);
    let p = dir.join("s.pdf");
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    let pdf = pdftext::parse(&std::fs::read(&p).unwrap());
    let t = pdf.pages().iter().flat_map(|pg| pdf.text(*pg)).collect::<Vec<_>>().join("\n");
    std::fs::remove_dir_all(&dir).ok();
    t
}

fn spans(e: &mut Engine, s: u64, id: u64) -> Vec<String> {
    let info = ok(e, json!({"cmd": "sheets_info", "sheet": s}));
    let it = info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone();
    it["measured"]["spans"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_chain_of_anchors_is_drawn_in_one_command_with_every_span_measured() {
    let dir = scratch("chain");
    let (mut e, s) = chain_site();
    let depth = e.undo_depth();
    // the left face of each of the four walls (and the first wall's near face): five anchors, four spans
    let pts_m = [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [5.0, 0.0, 0.0], [9.0, 0.0, 0.0], [9.2, 0.0, 0.0]];
    let anchors: Vec<Value> = pts_m.iter().map(|p| json!({"point": p})).collect();
    let r = ok(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": anchors, "mode": "horizontal"}));
    assert_eq!(e.undo_depth(), depth + 1, "a whole chain is one undo step");
    let id = r["dimension"].as_u64().unwrap();
    let values: Vec<f64> = r["values_m"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(values.len(), 4);
    for (v, want) in values.iter().zip([3.0, 2.0, 4.0, 0.2]) {
        assert!((v - want).abs() < 1e-9, "{values:?}");
    }
    assert_eq!(item(&e, id)["anchors"].as_array().unwrap().len(), 5);
    assert_eq!(spans(&mut e, s, id), ["3.00 m", "2.00 m", "4.00 m", "0.20 m"]);
    // every span's text is on the page, in each format
    let t = texts(&mut e, s, "chain");
    for want in ["3.00 m", "2.00 m", "4.00 m", "0.20 m"] {
        assert!(t.contains(want), "{want} missing from {t}");
    }
    let p = dir.join("s.svg");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    let svg = std::fs::read_to_string(p).unwrap();
    for want in ["3.00 m", "2.00 m", "4.00 m", "0.20 m"] {
        assert!(svg.contains(want), "{want} missing from the SVG");
    }
    let p = dir.join("s.dxf");
    ok(&mut e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    let dxf = std::fs::read_to_string(p).unwrap();
    assert!(dxf.contains("4.00 m") && dxf.contains("0.20 m"));
    // it saves, opens and undoes as one item
    let path = dir.join("c.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": path}));
    assert_eq!(spans(&mut g, s, id), ["3.00 m", "2.00 m", "4.00 m", "0.20 m"]);
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(!e.model.sheet_items.contains_key(&(id as u32)));
    // the chain follows the model: move the third wall
    ok(&mut e, json!({"cmd": "redo"}));
    let walls: Vec<u32> = e.model.groups.keys().copied().collect();
    ok(&mut e, json!({"cmd": "move", "ids": [walls[2]], "by": [1.0, 0.0, 0.0]}));
    assert_eq!(spans(&mut e, s, id), ["3.00 m", "3.00 m", "3.00 m", "0.20 m"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn baseline_dimensions_measure_every_span_from_the_first_anchor() {
    let (mut e, s) = chain_site();
    let anchors: Vec<Value> = [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [5.0, 0.0, 0.0], [9.0, 0.0, 0.0]].iter().map(|p| json!({"point": p})).collect();
    let id = ok(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": anchors, "mode": "baseline"}))["dimension"].as_u64().unwrap();
    assert_eq!(spans(&mut e, s, id), ["3.00 m", "5.00 m", "9.00 m"]);
    let t = texts(&mut e, s, "baseline");
    assert!(t.contains("5.00 m") && t.contains("9.00 m"), "{t}");
    // a chain can be as long as 200 anchors, and not longer
    let many = |n: usize| -> Vec<Value> { (0..n).map(|i| json!({"point": if i % 2 == 0 { [0.0, 0.0, 0.0] } else { [3.0, 0.0, 0.0] }})).collect() };
    assert!(e.exec_json(&json!({"cmd": "sheet_dim", "sheet": s, "anchors": many(200)}).to_string())["ok"] == true);
    assert!(refused(&mut e, json!({"cmd": "sheet_dim", "sheet": s, "anchors": many(201)})).contains("2 to 200"));
}
