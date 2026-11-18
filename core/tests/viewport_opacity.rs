//! Opacity per viewport on sheets (#6, design step 9): the saved field and its command, and what the PDF, SVG
//! and DXF do with it.

#[path = "pdftext/mod.rs"]
mod pdftext;

use serde_json::{json, Value};
use sketchcraft::policy::{permits, vet, Caller, Workspace};
use sketchcraft::Engine;

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sc-vpop-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A box, and one sheet with two stacked viewports of it: the first (z 0) at full strength, the second (z 1) on
/// top at 40%. Returns the engine, the sheet, and the two viewport ids.
fn stacked() -> (Engine, u64, u64, u64) {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "rect", "from": [0, 0, 0], "to": [4, 3, 0]}));
    let f = *e.model.faces.keys().next().unwrap();
    let up = e.model.normal(f).z > 0.0;
    ok(&mut e, json!({"cmd": "pushpull", "face": f, "distance": if up { 2.0 } else { -2.0 }}));
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Overlay", "paper": "a3"}))["sheet"].as_u64().unwrap();
    let rect = json!([20, 20, 260, 170]);
    let a = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "front", "scale": 50, "rect_mm": rect, "z": 0, "label": "Base"}))["viewport"].as_u64().unwrap();
    let b = ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "top", "scale": 50, "rect_mm": rect, "z": 1, "label": "Overlay", "opacity": 0.4}))["viewport"].as_u64().unwrap();
    (e, s, a, b)
}

fn export(e: &mut Engine, dir: &std::path::Path, file: &str) -> (Value, String) {
    let path = dir.join(file).to_string_lossy().to_string();
    let r = ok(e, json!({"cmd": "export_sheets", "path": path}));
    (r, path)
}

#[test]
fn two_stacked_viewports_one_at_40_percent_print_as_a_transparency_group_in_z_order() {
    let dir = scratch("pdf");
    let (mut e, _, _, _) = stacked();
    let (_, path) = export(&mut e, &dir, "o.pdf");
    let bytes = std::fs::read(&path).unwrap();
    let pdf = pdftext::parse(&bytes);
    // one ExtGState, with the stroke and fill alpha both 0.4
    let gs: Vec<&str> = pdf.objs.values().map(|o| o.dict.as_str()).filter(|d| d.contains("/CA ")).collect();
    assert_eq!(gs.len(), 1, "one faded viewport, one graphics state: {gs:?}");
    assert!(gs[0].contains("/CA 0.4") && gs[0].contains("/ca 0.4"), "{}", gs[0]);
    // the viewport is a Form XObject that is an isolated transparency group
    let forms: Vec<&pdftext::Obj> = pdf.objs.values().filter(|o| o.dict.contains("/Subtype /Form")).collect();
    assert_eq!(forms.len(), 1, "only the faded viewport is a form");
    assert!(forms[0].dict.contains("/S /Transparency") && forms[0].dict.contains("/I true"), "{}", forms[0].dict);
    assert!(forms[0].dict.contains("/BBox"), "{}", forms[0].dict);
    // lines inside it are plain strokes, not faded one by one
    let inner = String::from_utf8_lossy(forms[0].stream.as_ref().unwrap()).to_string();
    assert!(inner.contains("\nS\n"), "the drawing is in the form");
    assert!(!inner.contains(" gs"), "no per-line transparency inside the group");
    // z order on the page: the base viewport's lines come before the faded form is drawn, and what is drawn
    // after it (the second viewport's caption, scale bar, the title block) comes later
    let page = pdf.pages()[0];
    let c = pdf.content(page);
    let gs_at = c.find(" gs").expect("the page sets the graphics state");
    let do_at = c.find(" Do").expect("the page draws the form");
    assert!(gs_at < do_at, "the alpha is set before the form is drawn");
    assert!(c.matches("/Figure").count() == 2, "both viewports are figures");
    let fig1 = c.find("/Figure").unwrap();
    let fig2 = fig1 + 1 + c[fig1 + 1..].find("/Figure").unwrap();
    assert!(c[fig1..fig2].contains("\nS\n"), "the base viewport (z 0) is drawn first, as plain strokes");
    assert!(fig2 < gs_at && gs_at < do_at, "the faded viewport (z 1) comes second, as a form");
    assert!(c[do_at..].contains("EMC"), "the form sits inside its figure");
    // the page resources name the form and the state
    let page_dict = &pdf.objs[&page].dict;
    assert!(page_dict.contains("/ExtGState") && page_dict.contains("/Gs1") && page_dict.contains("/Fm1"), "{page_dict}");
    // and a set with nothing faded has neither
    let (mut plain, ..) = stacked();
    let v = *plain.model.sheet_items.keys().max().unwrap();
    ok(&mut plain, json!({"cmd": "viewport_edit", "id": v, "opacity": 1.0}));
    let (_, p2) = export(&mut plain, &dir, "p.pdf");
    let pdf2 = pdftext::parse(&std::fs::read(p2).unwrap());
    assert!(!pdf2.objs.values().any(|o| o.dict.contains("/CA ") || o.dict.contains("/Subtype /Form")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_faded_viewport_with_text_and_a_picture_keeps_its_resources() {
    // a perspective view is a picture; its labels are text. Both are drawn inside the form.
    let dir = scratch("res");
    let (mut e, s, _, _) = stacked();
    ok(&mut e, json!({"cmd": "viewport_add", "sheet": s, "from_view": "iso", "rect_mm": [20, 20, 100, 80], "opacity": 0.5, "z": 2}));
    let (_, path) = export(&mut e, &dir, "r.pdf");
    let pdf = pdftext::parse(&std::fs::read(path).unwrap());
    let forms: Vec<&pdftext::Obj> = pdf.objs.values().filter(|o| o.dict.contains("/Subtype /Form")).collect();
    assert_eq!(forms.len(), 2);
    assert!(forms.iter().any(|f| f.dict.contains("/XObject") && f.dict.contains("/Im1")), "the picture is in its form's resources");
    let alphas = pdf.objs.values().filter(|o| o.dict.contains("/CA 0.5")).count();
    assert_eq!(alphas, 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn svg_wraps_a_faded_viewport_in_a_group_with_opacity() {
    let dir = scratch("svg");
    let (mut e, ..) = stacked();
    let (_, path) = export(&mut e, &dir, "o.svg");
    let svg = std::fs::read_to_string(path).unwrap();
    assert_eq!(svg.matches("opacity=\"0.4\"").count(), 1, "{}", svg.lines().filter(|l| l.contains("<g")).collect::<Vec<_>>().join("\n"));
    assert!(svg.contains("<g clip-path=\"url(#") && svg.contains(" opacity=\"0.4\">"));
    // the full-strength viewport has no opacity attribute
    assert_eq!(svg.matches("opacity=").count(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn dxf_drops_opacity_and_says_so() {
    let dir = scratch("dxf");
    let (mut e, ..) = stacked();
    let (r, path) = export(&mut e, &dir, "o.dxf");
    let warnings = r["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{r}");
    assert!(warnings[0].as_str().unwrap().contains("opacity"), "{warnings:?}");
    assert!(std::fs::metadata(path).unwrap().len() > 1000, "the drawing is still written");
    // PDF and SVG have nothing to warn about, and neither does a DXF of plain viewports
    assert_eq!(export(&mut e, &dir, "o.pdf").0["warnings"], json!([]));
    assert_eq!(export(&mut e, &dir, "o.svg").0["warnings"], json!([]));
    let (mut plain, ..) = stacked();
    let v = *plain.model.sheet_items.keys().max().unwrap();
    ok(&mut plain, json!({"cmd": "viewport_edit", "id": v, "opacity": 1}));
    assert_eq!(export(&mut plain, &dir, "p.dxf").0["warnings"], json!([]));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn opacity_is_set_by_command_through_the_policy_saved_and_undone() {
    let dir = scratch("cmd");
    let (mut e, _, a, b) = stacked();
    let op = |e: &Engine, id: u64| serde_json::to_value(&e.model.sheet_items[&(id as u32)]).unwrap()["opacity"].clone();
    assert_eq!(op(&e, b), json!(0.4));
    assert!(op(&e, a).is_null(), "full strength is not stored");
    // set, and it is one undo step
    ok(&mut e, json!({"cmd": "viewport_edit", "id": a, "opacity": 0.25}));
    assert_eq!(op(&e, a), json!(0.25));
    // out of range is refused and changes nothing
    for bad in [1.5, -0.1] {
        let r = e.exec_json(&json!({"cmd": "viewport_edit", "id": a, "opacity": bad}).to_string());
        assert_eq!(r["ok"], false, "{r}");
    }
    assert_eq!(op(&e, a), json!(0.25));
    // saved as format 2 with the field, and read back
    let p = dir.join("o.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p}));
    let f: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(f["format"], 2);
    assert_eq!(f["needs"], json!(["sheets"]));
    assert_eq!(f["sheet_items"][a.to_string()]["opacity"], 0.25);
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": p}));
    assert_eq!(op(&g, a), json!(0.25));
    assert_eq!(op(&g, b), json!(0.4));
    // undo and redo
    ok(&mut e, json!({"cmd": "undo"}));
    assert!(op(&e, a).is_null());
    assert_eq!(op(&e, b), json!(0.4));
    ok(&mut e, json!({"cmd": "redo"}));
    assert_eq!(op(&e, a), json!(0.25));
    // the policy: agents and plugins may set it, within the sheet limits
    for who in [Caller::Plugin, Caller::Mcp(Workspace::anywhere())] {
        assert!(permits(&who, "viewport_edit") && permits(&who, "viewport_add"));
    }
    let mut c = json!({"cmd": "viewport_edit", "id": a, "opacity": 0.5});
    assert!(vet(&Caller::Mcp(Workspace::anywhere()), &mut c).is_ok());
    std::fs::remove_dir_all(dir).ok();
}
