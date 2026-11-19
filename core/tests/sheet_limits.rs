//! Limits on sheets for every way in (WildLeopard): page tables from `sheets` are held to the table rules, sheet
//! pictures have a size cap, agents and plugins have a viewport budget, the document's own text is marked as data in
//! what agents read, files are brought within the limits when they open, an agent's quiet batch is closed, and the
//! drawing exports keep text as text.

use serde_json::{json, Value};
use sketchcraft::draft::raster;
use sketchcraft::drafting::{ItemKind, SheetItem, MAX_CELL_CHARS, MAX_TABLE_CELLS, MAX_TABLE_COLS, MAX_TEXT_CHARS, TEXT_HEIGHT_RANGE};
use sketchcraft::live::Live;
use sketchcraft::policy::{untrusted_text, vet, Caller, Workspace, MAX_PASTE_BYTES_UNTRUSTED, MAX_SHEET_TEXT_BYTES_UNTRUSTED, MAX_TABLE_CELLS_UNTRUSTED, MAX_VIEWPORTS_PER_SHEET_UNTRUSTED, MAX_VIEWPORTS_UNTRUSTED};
use sketchcraft::Engine;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-sheet-limits-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn agent() -> Caller {
    Caller::Mcp(Workspace::anywhere())
}

fn run(e: &mut Engine, c: &Caller, v: Value) -> Value {
    e.exec_json_as(c, &v.to_string())
}

fn ok(e: &mut Engine, c: &Caller, v: Value) -> Value {
    let r = run(e, c, v.clone());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn refused(e: &mut Engine, c: &Caller, v: Value) -> String {
    let before = (e.undo_depth(), serde_json::to_value(&e.model.sheet_items).unwrap(), e.model.sheets.len());
    let r = run(e, c, v.clone());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    assert_eq!((e.undo_depth(), serde_json::to_value(&e.model.sheet_items).unwrap(), e.model.sheets.len()), before, "a refused command changes nothing");
    r["error"].as_str().unwrap().to_string()
}

fn tables(e: &Engine) -> Vec<Vec<Vec<String>>> {
    e.model.sheet_items.values().filter_map(|i| if let SheetItem::Known(ItemKind::Table(t)) = &**i { Some(t.rows.clone()) } else { None }).collect()
}

fn viewports(e: &Engine) -> usize {
    e.model.sheet_items.values().filter(|i| matches!(&***i, SheetItem::Known(ItemKind::Viewport(_)))).count()
}

// ---- page tables from `sheets` -------------------------------------------------------------------------------

#[test]
fn page_tables_from_sheets_follow_the_table_rules() {
    let mut e = Engine::new();
    let ui = Caller::Ui;
    // a wide or long table is refused for everyone, like sheet_table
    let wide = vec![vec!["a"; MAX_TABLE_COLS + 1]];
    let err = refused(&mut e, &ui, json!({"cmd": "sheets", "pages": [{"title": "Index", "table": wide}]}));
    assert!(err.contains("columns"), "{err}");
    let long: Vec<Vec<&str>> = vec![vec!["a", "b"]; MAX_TABLE_CELLS / 2 + 1];
    let err = refused(&mut e, &ui, json!({"cmd": "sheets", "pages": [{"title": "Index", "table": long}]}));
    assert!(err.contains("cells"), "{err}");
    // the cell rules: no control characters, at most MAX_CELL_CHARS, tabs become spaces
    let err = refused(&mut e, &ui, json!({"cmd": "sheets", "pages": [{"title": "Index", "table": [["a\u{1b}[2J"]]}]}));
    assert!(err.contains("control characters"), "{err}");
    let err = refused(&mut e, &ui, json!({"cmd": "sheets", "pages": [{"title": "Index", "table": [["x".repeat(MAX_CELL_CHARS + 1)]]}]}));
    assert!(err.contains("characters"), "{err}");
    ok(&mut e, &ui, json!({"cmd": "sheets", "pages": [{"title": "Doors", "table": [["Mark", "Size"], ["D1", "36\tx 80"]]}]}));
    assert_eq!(tables(&e), vec![vec![vec!["Mark".to_string(), "Size".into()], vec!["D1".into(), "36    x 80".into()]]]);
}

#[test]
fn page_tables_count_against_an_agents_cell_budget() {
    let a = agent();
    // one page of 1 x 20,001 (the original case) and pages that add up past the budget
    let one = json!({"cmd": "sheets", "pages": [{"title": "x", "table": [vec!["a"; 20_001]]}]});
    assert!(vet(&a, &mut one.clone()).unwrap_err().contains("too many table cells"));
    let half: Vec<Vec<&str>> = vec![vec!["a"; 50]; MAX_TABLE_CELLS_UNTRUSTED / 100 + 1];
    let two = json!({"cmd": "sheets", "pages": [{"title": "x", "table": half}, {"title": "y", "table": half}]});
    assert!(vet(&a, &mut two.clone()).unwrap_err().contains("too many table cells"));
    assert!(vet(&Caller::Plugin, &mut two.clone()).is_err());
    // within it, and the person, are fine
    let fits: Vec<Vec<&str>> = vec![vec!["a"; 50]; MAX_TABLE_CELLS_UNTRUSTED / 100];
    assert!(vet(&a, &mut json!({"cmd": "sheets", "pages": [{"title": "x", "table": fits}, {"title": "y", "table": fits}]})).is_ok());
    assert!(vet(&Caller::Ui, &mut two.clone()).is_ok());
    let mut e = Engine::new();
    let err = refused(&mut e, &a, one);
    assert!(err.contains("table cells"), "{err}");
}

// ---- sheet pictures ------------------------------------------------------------------------------------------

#[test]
fn sheet_pictures_have_a_size_cap() {
    // ordinary paper: as before (the width asked, the paper's proportions)
    assert_eq!(raster::size(420.0, 297.0, 1600), (1600, 1131, 1600.0 / 420.0));
    assert_eq!(raster::size(420.0, 297.0, 99_999).0, raster::MAX_PX);
    // a tall, narrow page at the widest: drawn narrower, never past the caps
    for (w, h) in [(20.0, 5000.0), (5000.0, 5000.0), (300.0, 5000.0), (5000.0, 20.0), (1e-3, 1e9)] {
        let (pw, ph, _) = raster::size(w, h, raster::MAX_PX);
        assert!(ph <= raster::MAX_SIDE && pw <= raster::MAX_PX, "{w} x {h}: {pw} x {ph}");
        assert!(u64::from(pw) * u64::from(ph) <= raster::MAX_PIXELS + u64::from(raster::MAX_SIDE), "{w} x {h}: {pw} x {ph}");
    }
    // the picture itself, for any page a caller draws
    let img = raster::page(&sketchcraft::draft::ir::Page::new(300.0, 5000.0), 4000);
    assert!(img.height() <= raster::MAX_SIDE && u64::from(img.width()) * u64::from(img.height()) <= raster::MAX_PIXELS + u64::from(raster::MAX_SIDE));
    assert!(img.width() < 4000 && (f64::from(img.height()) / f64::from(img.width()) - 5000.0 / 300.0).abs() < 0.2, "the proportions stay");
    // through the command, for the person and for an agent alike: a standard sheet as wide as asked
    let d = scratch("render");
    let mut e = Engine::new();
    for c in [Caller::Ui, agent()] {
        let s = ok(&mut e, &c, json!({"cmd": "sheet_add", "paper": "a1"}))["sheet"].as_u64().unwrap();
        let r = ok(&mut e, &c, json!({"cmd": "sheet_render", "sheet": s, "path": d.join(format!("s{s}.png")), "px": 4000}));
        assert_eq!((r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap()), (4000, (4000.0f64 * 594.0 / 841.0).round() as u64), "{r}");
    }
}

// ---- viewports ---------------------------------------------------------------------------------------------

#[test]
fn agents_and_plugins_have_a_viewport_budget() {
    let a = agent();
    let mut e = Engine::new();
    let s = ok(&mut e, &a, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    let one = |s: u64| json!({"sheet": s, "rect_mm": [10, 10, 20, 20]});
    // one command past the per-sheet budget adds nothing
    let many: Vec<Value> = (0..=MAX_VIEWPORTS_PER_SHEET_UNTRUSTED).map(|_| one(s)).collect();
    let err = refused(&mut e, &a, json!({"cmd": "viewport_add", "viewports": many}));
    assert!(err.contains("at most 200 on one sheet"), "{err}");
    let err = refused(&mut e, &Caller::Plugin, json!({"cmd": "viewport_add", "viewports": many}));
    assert!(err.contains("on one sheet"), "{err}");
    // up to it is fine; one more is not, from a later command either
    let fits: Vec<Value> = (0..MAX_VIEWPORTS_PER_SHEET_UNTRUSTED).map(|_| one(s)).collect();
    ok(&mut e, &a, json!({"cmd": "viewport_add", "viewports": fits}));
    let err = refused(&mut e, &a, json!({"cmd": "viewport_add", "sheet": s, "rect_mm": [1, 1, 5, 5]}));
    assert!(err.contains("on one sheet"), "{err}");
    // nor by moving one there, or copying the sheet... (a copy of 200 fits a new sheet)
    let s2 = ok(&mut e, &a, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    let v = ok(&mut e, &a, json!({"cmd": "viewport_add", "sheet": s2, "rect_mm": [1, 1, 5, 5]}))["viewport"].as_u64().unwrap();
    let err = refused(&mut e, &a, json!({"cmd": "viewport_edit", "id": v, "sheet": s}));
    assert!(err.contains("on one sheet"), "{err}");
    ok(&mut e, &a, json!({"cmd": "sheet_duplicate", "sheet": s}));
    // ...nor through `sheets` pages
    let page: Vec<Value> = (0..=MAX_VIEWPORTS_PER_SHEET_UNTRUSTED).map(|_| json!({"view": "iso"})).collect();
    let err = refused(&mut e, &a, json!({"cmd": "sheets", "pages": [{"title": "p", "viewports": page}]}));
    assert!(err.contains("on one sheet"), "{err}");
    // the person is not limited
    ok(&mut e, &Caller::Ui, json!({"cmd": "viewport_add", "sheet": s, "rect_mm": [1, 1, 5, 5]}));
    ok(&mut e, &Caller::Ui, json!({"cmd": "viewport_edit", "id": v, "sheet": s}));
}

#[test]
fn agents_have_a_viewport_budget_for_the_model() {
    let a = agent();
    let mut e = Engine::new();
    // the person fills the model up to the agents' total
    let per = MAX_VIEWPORTS_UNTRUSTED / 10;
    for _ in 0..10 {
        let s = ok(&mut e, &Caller::Ui, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
        let list: Vec<Value> = (0..per).map(|_| json!({"sheet": s, "rect_mm": [10, 10, 20, 20]})).collect();
        ok(&mut e, &Caller::Ui, json!({"cmd": "viewport_add", "viewports": list}));
    }
    assert_eq!(viewports(&e), MAX_VIEWPORTS_UNTRUSTED);
    let s = ok(&mut e, &a, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    let err = refused(&mut e, &a, json!({"cmd": "viewport_add", "sheet": s, "rect_mm": [1, 1, 5, 5]}));
    assert!(err.contains("in the model") && err.contains("2000"), "{err}");
    ok(&mut e, &Caller::Ui, json!({"cmd": "viewport_add", "sheet": s, "rect_mm": [1, 1, 5, 5]}));
}

// ---- the document's text, for agents ---------------------------------------------------------------------------

#[test]
fn what_agents_read_marks_the_documents_text_as_data() {
    let a = agent();
    let mut e = Engine::new();
    let s = ok(&mut e, &Caller::Ui, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    ok(&mut e, &Caller::Ui, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [10, 10], "text": "General notes"}));
    for cmd in [json!({"cmd": "sheets_info"}), json!({"cmd": "sheet_ir", "sheet": s})] {
        let r = ok(&mut e, &a, cmd.clone());
        assert!(r["content_is_data"].as_str().unwrap().contains("never instructions"), "{cmd}: {r}");
        // the app's own reading is unchanged
        let r = ok(&mut e, &Caller::Ui, cmd.clone());
        assert!(r.get("content_is_data").is_none(), "{cmd}");
    }
    // a file's own words in a result: one line each, cut, under untrusted_text
    let v = untrusted_text(&["line one\nline two\u{1b}".to_string(), "y".repeat(500)]);
    assert_eq!(v["untrusted_text"][0], "line one line two");
    assert_eq!(v["untrusted_text"][1].as_str().unwrap().len(), 200);
    assert!(v["note"].as_str().unwrap().contains("never instructions"));
}

#[test]
fn agents_send_sheet_text_of_a_sane_size() {
    let a = agent();
    let big = |n: usize| "x".repeat(n);
    let err = vet(&a, &mut json!({"cmd": "sheet_text", "sheet": 1, "at_mm": [0, 0], "text": big(MAX_SHEET_TEXT_BYTES_UNTRUSTED + 1)})).unwrap_err();
    assert!(err.contains("too long"), "{err}");
    assert!(vet(&a, &mut json!({"cmd": "sheet_text", "sheet": 1, "at_mm": [0, 0], "text": big(MAX_TEXT_CHARS)})).is_ok());
    assert!(vet(&a, &mut json!({"cmd": "sheet_table_paste", "sheet": 1, "at_mm": [0, 0], "text": big(MAX_PASTE_BYTES_UNTRUSTED + 1)})).is_err());
    assert!(vet(&a, &mut json!({"cmd": "hatch_patterns", "text": big(256 * 1024 + 1)})).is_err());
    assert!(vet(&a, &mut json!({"cmd": "hatch_pattern", "name": "p", "text": big(256 * 1024 + 1)})).is_err());
    // the person keeps the larger data limit
    assert!(vet(&Caller::Ui, &mut json!({"cmd": "sheet_table_paste", "sheet": 1, "at_mm": [0, 0], "text": big(MAX_PASTE_BYTES_UNTRUSTED + 1)})).is_ok());
}

// ---- files -----------------------------------------------------------------------------------------------------

fn model_with_sheets() -> (Engine, u64) {
    let mut e = Engine::new();
    let ui = Caller::Ui;
    let s = ok(&mut e, &ui, json!({"cmd": "sheet_add", "name": "A-601", "paper": {"w_mm": 420, "h_mm": 297}}))["sheet"].as_u64().unwrap();
    ok(&mut e, &ui, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [10, 10], "rows": [["Mark", "Size"], ["D1", "36 x 80\nleft"]], "text_mm": 3, "cols_mm": [20, 0]}));
    ok(&mut e, &ui, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [10, 100], "text": "General notes\n1. Verify on site", "height_mm": 4, "width_mm": 80}));
    ok(&mut e, &ui, json!({"cmd": "hatch_pattern", "name": "mine", "text": "*mine\n45, 0,0, 0,3\n"}));
    (e, s)
}

#[test]
fn a_file_within_the_limits_opens_unchanged() {
    let d = scratch("unchanged");
    let (mut e, _) = model_with_sheets();
    let p = d.join("m.sketchcraft");
    ok(&mut e, &Caller::Ui, json!({"cmd": "save", "path": p}));
    let mut f = Engine::new();
    let r = ok(&mut f, &Caller::Ui, json!({"cmd": "open", "path": p}));
    assert!(r.get("sheets_clamped").is_none(), "{r}");
    assert_eq!(serde_json::to_value(&f.model.sheets).unwrap(), serde_json::to_value(&e.model.sheets).unwrap());
    assert_eq!(serde_json::to_value(&f.model.sheet_items).unwrap(), serde_json::to_value(&e.model.sheet_items).unwrap());
    assert_eq!(serde_json::to_value(&f.model.drafting).unwrap(), serde_json::to_value(&e.model.drafting).unwrap());
    // and saved again, byte for byte
    let q = d.join("again.sketchcraft");
    ok(&mut f, &Caller::Ui, json!({"cmd": "save", "path": q}));
    assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(&q).unwrap());
}

#[test]
fn a_file_past_the_limits_opens_within_them() {
    let d = scratch("clamped");
    let (mut e, _) = model_with_sheets();
    let p = d.join("m.sketchcraft");
    ok(&mut e, &Caller::Ui, json!({"cmd": "save", "path": p}));
    // a file written by another program, past every limit
    let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    for (_, s) in v["sheets"].as_object_mut().unwrap() {
        s["paper"] = json!({"w_mm": 1e-3, "h_mm": 1e8, "name": ""});
    }
    for (_, i) in v["sheet_items"].as_object_mut().unwrap() {
        match i["kind"].as_str().unwrap() {
            "table" => {
                i["rows"] = json!(vec![vec!["c\u{7}ell"; 3]; 30_000]);
                i["text_mm"] = json!(1e-9);
                i["cols_mm"] = json!(vec![1e6; 300]);
                i["title"] = json!("x\ny".repeat(300));
            }
            "text" => {
                i["paragraphs"] = json!(vec!["p\u{1b}".repeat(1000); 100]);
                i["height_mm"] = json!(1e-9);
            }
            _ => {}
        }
    }
    let pats = v["drafting"]["patterns"].as_object_mut().unwrap();
    for n in 0..300 {
        pats.insert(format!("p{n:03}"), json!("*p\n0, 0,0, 0,1\n"));
    }
    pats.insert("bad\nname".into(), json!("*b\n0, 0,0, 0,1\n"));
    std::fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();

    let mut f = Engine::new();
    let r = ok(&mut f, &Caller::Ui, json!({"cmd": "open", "path": p}));
    let said = r["sheets_clamped"].to_string();
    assert!(said.contains("paper size") && said.contains("table") && said.contains("text") && said.contains("hatch pattern"), "{r}");
    let m = &f.model;
    for s in m.sheets.values() {
        assert!((20.0..=5000.0).contains(&s.paper.w_mm) && (20.0..=5000.0).contains(&s.paper.h_mm));
    }
    for i in m.sheet_items.values() {
        match &**i {
            SheetItem::Known(ItemKind::Table(t)) => {
                let cols = t.rows.iter().map(Vec::len).max().unwrap();
                assert!(t.rows.len() * cols <= MAX_TABLE_CELLS && t.cols_mm.len() <= MAX_TABLE_COLS);
                assert!(t.rows.iter().flatten().all(|c| c == "cell"));
                assert!(t.cols_mm.iter().all(|w| *w <= 2000.0));
                assert_eq!(t.text_mm, Some(TEXT_HEIGHT_RANGE.0));
                assert!(t.title.chars().count() <= 200 && !t.title.contains('\n'));
            }
            SheetItem::Known(ItemKind::Text(t)) => {
                assert!(t.paragraphs.iter().map(|p| p.chars().count()).sum::<usize>() <= MAX_TEXT_CHARS);
                assert!(t.paragraphs.iter().all(|p| !p.chars().any(char::is_control)));
                assert_eq!(t.height_mm, Some(TEXT_HEIGHT_RANGE.0));
            }
            _ => {}
        }
    }
    assert!(m.drafting.patterns.len() <= 200 && !m.drafting.patterns.contains_key("bad\nname"));
    // its tables and texts lay out as usual
    let info = ok(&mut f, &Caller::Ui, json!({"cmd": "sheets_info"}));
    assert!(info["sheets"][0]["items"].as_array().unwrap().iter().all(|i| i.get("layout").is_some()), "{}", info.to_string().len());
}

#[test]
fn a_restored_backup_is_brought_within_the_limits_too() {
    let (mut e, s) = model_with_sheets();
    let mut m = e.model.clone();
    m.sheets.get_mut(&(s as u32)).unwrap().paper.w_mm = 1e7;
    e.replace_model(m).unwrap();
    assert_eq!(e.model.sheets[&(s as u32)].paper.w_mm, 5000.0);
}

#[test]
fn the_app_shows_a_notice_when_a_file_was_trimmed() {
    let d = scratch("notice");
    let doc = d.join("doc.sketchcraft");
    let (mut e, _) = model_with_sheets();
    ok(&mut e, &Caller::Ui, json!({"cmd": "save", "path": doc}));
    let mut v: Value = serde_json::from_slice(&std::fs::read(&doc).unwrap()).unwrap();
    for (_, s) in v["sheets"].as_object_mut().unwrap() {
        s["paper"]["w_mm"] = json!(1e7);
    }
    std::fs::write(&doc, serde_json::to_vec(&v).unwrap()).unwrap();
    let live = Live::open(doc);
    assert!(live.notice.as_deref().is_some_and(|n| n.contains("trimmed")), "{:?}", live.notice);
}

// ---- batches ---------------------------------------------------------------------------------------------------

#[test]
fn an_agents_quiet_batch_is_closed_and_plugins_hear_when_to_retry() {
    let d = scratch("batch");
    let mut live = Live::open(d.join("doc.sketchcraft"));
    let a = agent();
    let rect = |x: i64| json!({"cmd": "rect", "from": [x, 0, 0], "to": [x + 2, 2, 0]});
    let id = live.exec_as(&a, &json!({"cmd": "begin_batch"}))["batch"].as_u64().unwrap();
    assert_eq!(live.exec_as(&a, &rect(0))["ok"], true);
    // a plugin waits, and is told for how long at most
    let r = live.exec_as(&Caller::Plugin, &rect(10));
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("at the latest in"), "{r}");
    // reading keeps the agent's batch open; 60 s with nothing from the agent closes it
    live.engine.quiet_batch_for_test(50);
    assert_eq!(live.exec_as(&a, &json!({"cmd": "sheets_info"}))["ok"], true);
    assert_eq!(live.exec_as(&Caller::Plugin, &rect(10))["ok"], false, "still the agent's");
    live.engine.quiet_batch_for_test(61);
    assert_eq!(live.exec_as(&Caller::Plugin, &rect(10))["ok"], true, "the quiet batch was closed");
    // what the agent did is one step; its late end hears only that its batch is over
    let end = live.exec_as(&a, &json!({"cmd": "end_batch", "id": id}));
    assert_eq!(end["ok"], true, "{end}");
    assert_eq!(end["already_ended"], true, "{end}");
}

#[test]
fn an_agent_ending_an_old_batch_hears_nothing_about_another_clients() {
    let d = scratch("stale");
    let mut live = Live::open(d.join("doc.sketchcraft"));
    let a = agent();
    let old = live.exec_as(&a, &json!({"cmd": "begin_batch"}))["batch"].as_u64().unwrap();
    assert_eq!(live.exec_as(&a, &json!({"cmd": "end_batch", "id": old}))["ok"], true);
    // the app runs a plugin in a batch; the person's edit (or the timer) ends it
    assert_eq!(live.exec_as(&Caller::Ui, &json!({"cmd": "begin_batch"}))["ok"], true);
    assert_eq!(live.exec_as(&Caller::Plugin, &json!({"cmd": "rect", "from": [0, 0, 0], "to": [2, 2, 0]}))["ok"], true);
    live.engine.expire_batch_for_test();
    live.engine.close_expired_batch();
    // the agent's end with its old id says nothing about the plugin's run
    let r = live.exec_as(&a, &json!({"cmd": "end_batch", "id": old}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["already_ended"], true, "{r}");
    assert!(r.get("origin").is_none() && r.get("changed").is_none(), "{r}");
}

// ---- exports -------------------------------------------------------------------------------------------------------

#[test]
fn exports_keep_text_as_text() {
    let d = scratch("exports");
    let mut e = Engine::new();
    let ui = Caller::Ui;
    let s = ok(&mut e, &ui, json!({"cmd": "sheet_add"}))["sheet"].as_u64().unwrap();
    ok(&mut e, &ui, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [10, 10], "text": "C:\\U+0041 \u{FFFF}\u{FDD0}\u{FFFE} end"}));
    ok(&mut e, &ui, json!({"cmd": "export_sheets", "path": d.join("s.dxf"), "pages": [s]}));
    let dxf = String::from_utf8(std::fs::read(d.join("s.dxf")).unwrap()).unwrap();
    // a backslash is written as its own code, so the text can't make another character
    assert!(dxf.contains("C:\\U+005CU+0041"), "{}", dxf.lines().filter(|l| l.contains("C:")).collect::<Vec<_>>().join("|"));
    ok(&mut e, &ui, json!({"cmd": "export_sheets", "path": d.join("s.svg"), "pages": [s]}));
    let svg = String::from_utf8(std::fs::read(d.join("s.svg")).unwrap()).unwrap();
    assert!(svg.contains(" end") && !svg.contains('\u{FFFF}') && !svg.contains('\u{FFFE}') && !svg.contains('\u{FDD0}'));
}
