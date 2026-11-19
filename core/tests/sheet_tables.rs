//! Tables and text blocks on sheets (#7, design step 14): a door schedule and a drawing index made, edited, pasted
//! from a spreadsheet, undone one step per edit, saved and opened (and older files still read), drawn in the PDF
//! (one tagged table per table), SVG and DXF; text blocks wrapped and aligned; large ones laid out fast; policy.

use serde_json::{json, Value};
use sketchcraft::draft;
use sketchcraft::drafting::{ItemKind, SheetItem, MAX_TABLE_CELLS};
use sketchcraft::policy::{permits, vet, Caller, Workspace, MAX_TABLE_CELLS_UNTRUSTED};
use sketchcraft::Engine;
use std::path::PathBuf;
use std::time::Instant;

#[path = "pdftext/mod.rs"]
mod pdftext;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sc-sheet-tables-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ok(e: &mut Engine, v: Value) -> Value {
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], true, "{v} -> {r}");
    r
}

fn sheet_items(e: &Engine) -> Value {
    serde_json::to_value(&e.model.sheet_items).unwrap()
}

fn refused(e: &mut Engine, v: Value) -> String {
    let before = (e.undo_depth(), sheet_items(e));
    let r = e.exec_json(&v.to_string());
    assert_eq!(r["ok"], false, "{v} should be refused -> {r}");
    assert_eq!((e.undo_depth(), sheet_items(e)), before, "a refused command changes nothing and takes no undo step");
    r["error"].as_str().unwrap().to_string()
}

fn item(e: &Engine, id: u64) -> Value {
    serde_json::to_value(&*e.model.sheet_items[&(id as u32)]).unwrap()
}

fn rows(e: &Engine, id: u64) -> Value {
    item(e, id)["rows"].clone()
}

fn sheet() -> (Engine, u64) {
    let mut e = Engine::new();
    let s = ok(&mut e, json!({"cmd": "sheet_add", "name": "Schedules", "number": "A-601", "paper": "a3"}))["sheet"].as_u64().unwrap();
    (e, s)
}

fn export(e: &mut Engine, s: u64, dir: &std::path::Path, ext: &str) -> Vec<u8> {
    let p = dir.join(format!("s.{ext}"));
    ok(e, json!({"cmd": "export_sheets", "path": p, "pages": [s]}));
    std::fs::read(p).unwrap()
}

/// Every text run in the app's display list for the sheet: (text, x, y, bold).
fn ir_texts(e: &mut Engine, s: u64) -> Vec<(String, f64, f64, bool)> {
    let ir = ok(e, json!({"cmd": "sheet_ir", "sheet": s}));
    let mut out = vec![];
    fn walk(items: &[Value], out: &mut Vec<(String, f64, f64, bool)>) {
        for i in items {
            if i["k"] == "t" {
                out.push((i["text"].as_str().unwrap_or("").to_string(), i["x"].as_f64().unwrap(), i["y"].as_f64().unwrap(), i["bold"] == true));
            }
            if let Some(c) = i["items"].as_array() {
                walk(c, out);
            }
        }
    }
    walk(ir["items"].as_array().unwrap(), &mut out);
    out
}

fn info_item(e: &mut Engine, s: u64, id: u64) -> Value {
    let info = ok(e, json!({"cmd": "sheets_info", "sheet": s}));
    info["sheets"][0]["items"].as_array().unwrap().iter().find(|i| i["id"] == id).unwrap().clone()
}

const DOORS: &str = "Mark,Size,Type,Fire rating\nD1,900 x 2100,Single,\nD2,1800 x 2100,Double,\"FD30, self-closing\"\n";

#[test]
fn a_door_schedule_is_made_edited_and_undone_one_step_per_edit() {
    let dir = scratch("doors");
    let (mut e, s) = sheet();
    let d0 = e.undo_depth();
    let r = ok(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [20, 30], "size": [3, 4], "title": "DOOR SCHEDULE"}));
    let t = r["item"].as_u64().unwrap();
    assert_eq!((r["rows"].as_u64(), r["cols"].as_u64()), (Some(3), Some(4)));
    assert_eq!(e.undo_depth(), d0 + 1);
    // typed cell by cell: one undo step each
    for (i, (row, col, text)) in [(0, 0, "Mark"), (0, 1, "Size"), (0, 2, "Type"), (1, 0, "D1"), (1, 1, "900 x 2100"), (1, 2, "Single")].into_iter().enumerate() {
        ok(&mut e, json!({"cmd": "sheet_table", "id": t, "cells": [{"row": row, "col": col, "text": text}]}));
        assert_eq!(e.undo_depth(), d0 + 2 + i, "one step per cell");
    }
    assert_eq!(rows(&e, t)[1], json!(["D1", "900 x 2100", "Single", ""]));
    // a row inserted under the header, a column taken out, a cell past the end grows the table
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "insert_rows": {"at": 1}}));
    assert_eq!(rows(&e, t)[1], json!(["", "", "", ""]));
    assert_eq!(rows(&e, t)[2][0], "D1");
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "delete_rows": {"at": 1}}));
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "delete_cols": {"at": 3}}));
    assert_eq!(rows(&e, t)[0], json!(["Mark", "Size", "Type"]));
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "cells": [{"row": 3, "col": 0, "text": "D2"}, {"row": 3, "col": 2, "text": "Double\nleaf"}]}));
    assert_eq!(rows(&e, t).as_array().unwrap().len(), 4);
    assert_eq!(rows(&e, t)[3], json!(["D2", "", "Double\nleaf"]));
    // style: header, widths, borders, alignment, text height; only what is given changes
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "cols_mm": [15, 0, 30], "border": "rules", "align": ["center", "left", "right"], "text_mm": 3, "header_cols": 1}));
    let it = item(&e, t);
    assert_eq!(it["cols_mm"], json!([15.0, 0.0, 30.0]));
    assert_eq!((it["border"].as_str(), it["text_mm"].as_f64(), it["header_cols"].as_u64()), (Some("rules"), Some(3.0), Some(1)));
    assert_eq!(it["align"], json!(["center", "left", "right"]));
    assert!(it.get("header_rows").is_none(), "the default header isn't written");
    assert_eq!(it["title"], "DOOR SCHEDULE");
    // where it is on the paper: column 0 is 15 mm, column 2 is 30 mm, column 1 fits its text
    let info = info_item(&mut e, s, t);
    let xs: Vec<f64> = info["layout"]["xs"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert_eq!(xs.len(), 4);
    assert!((xs[0] - 20.0).abs() < 1e-9 && (xs[1] - 35.0).abs() < 0.01 && (xs[3] - xs[2] - 30.0).abs() < 0.01, "{xs:?}");
    let ys: Vec<f64> = info["layout"]["ys"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert_eq!(ys.len(), 5);
    assert!(ys[4] - ys[3] > ys[2] - ys[1] + 2.0, "the row with a two-line cell is taller: {ys:?}");
    // drawn: header bold, the first column bold (it heads its row), the second line of the cell below the first
    let runs = ir_texts(&mut e, s);
    let find = |w: &str| runs.iter().find(|r| r.0 == w).unwrap_or_else(|| panic!("{w} in {runs:?}")).clone();
    assert!(find("Mark").3 && find("D1").3 && !find("Single").3, "header row and column bold");
    assert!(find("leaf").2 > find("Double").2 + 3.0, "a newline in a cell is a second line");
    assert!(find("DOOR SCHEDULE").2 < 30.0, "the title sits above the table");
    // right-aligned: the right column's text ends at its right edge less the padding
    let single = find("Single");
    let w = draft::table::text_w("Single", 3.0, false);
    assert!((single.1 + w - (xs[3] - draft::table::pad(3.0).0)).abs() < 0.05, "right aligned: {single:?}");
    // saved and opened
    let path = dir.join("doors.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": path}));
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": path}));
    assert_eq!(sheet_items(&g), sheet_items(&e), "tables survive save and open");
    // undone back to nothing, one step at a time, and redone
    let after = sheet_items(&e);
    let steps = e.undo_depth() - d0;
    for _ in 0..steps {
        ok(&mut e, json!({"cmd": "undo"}));
    }
    assert!(e.model.sheet_items.is_empty());
    for _ in 0..steps {
        ok(&mut e, json!({"cmd": "redo"}));
    }
    assert_eq!(sheet_items(&e), after);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_spreadsheet_paste_makes_or_fills_a_table() {
    let (mut e, s) = sheet();
    // CSV with a quoted cell holding a comma, and an empty cell
    let r = ok(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [20, 30], "text": DOORS}));
    assert_eq!(r["format"], "csv");
    assert_eq!(r["pasted"], json!([3, 4]));
    let t = r["item"].as_u64().unwrap();
    assert_eq!(rows(&e, t)[2], json!(["D2", "1800 x 2100", "Double", "FD30, self-closing"]));
    assert_eq!(rows(&e, t)[1][3], "");
    // tab-separated (what a spreadsheet copies), CRLF, a quoted cell with a newline, into the table from a cell
    let d = e.undo_depth();
    let tsv = "D3\t800 x 2100\tSingle\r\nD4\t\"900 x 2100\"\t\"Single\nglazed\"\r\n";
    let r = ok(&mut e, json!({"cmd": "sheet_table_paste", "id": t, "at": [3, 0], "text": tsv}));
    assert_eq!(r["format"], "tsv");
    assert_eq!(e.undo_depth(), d + 1, "a paste is one undo step");
    assert_eq!(rows(&e, t).as_array().unwrap().len(), 5, "grown to hold it");
    assert_eq!(rows(&e, t)[4], json!(["D4", "900 x 2100", "Single\nglazed", ""]));
    // past the right edge: the table grows a column
    ok(&mut e, json!({"cmd": "sheet_table_paste", "id": t, "at": [0, 4], "text": "Hardware\tFinish"}));
    assert_eq!(rows(&e, t)[0], json!(["Mark", "Size", "Type", "Fire rating", "Hardware", "Finish"]));
    assert_eq!(rows(&e, t)[1].as_array().unwrap().len(), 6, "every row as long");
    // semicolons, and a format given
    let r = ok(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [200, 30], "text": "a;b\n1;2", "header_rows": 0}));
    assert_eq!(r["format"], "ssv");
    assert_eq!(item(&e, r["item"].as_u64().unwrap())["header_rows"], 0);
    let r = ok(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [200, 80], "text": "a;b,c", "format": "csv"}));
    assert_eq!(rows(&e, r["item"].as_u64().unwrap()), json!([["a;b", "c"]]));
    // refusals
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [1, 1], "text": "\n\n"})).contains("nothing to paste"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [1, 1], "text": "a", "format": "xlsx"})).contains("format"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "text": "a\tb"})).contains("at_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [1, 1], "at": [0, 0], "text": "a\tb"})).contains("`at`"));
    let big = vec!["x\t".repeat(100); 300].join("\n");
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [1, 1], "text": big})).contains("at most"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [1, 1], "text": "a\u{7}\tb"})).contains("control"));
}

#[test]
fn tables_refuse_what_they_cannot_hold() {
    let (mut e, s) = sheet();
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "at_mm": [1, 1], "size": [2, 2]})).contains("which sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": 999, "at_mm": [1, 1], "size": [2, 2]})).contains("no sheet"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "size": [2, 2]})).contains("at_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1]})).contains("rows"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "size": [0, 2]})).contains("at least 1"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "size": [MAX_TABLE_CELLS, 2]})).contains("at most"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "rows": [["a"]], "cols_mm": [3]})).contains("wide"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "rows": [["a"]], "text_mm": 0.2})).contains("text_mm"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "rows": [["a"]], "source": "doors"})).contains("filled from"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "rows": [["x".repeat(2001)]]})).contains("at most"));
    let t = ok(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [1, 1], "rows": [["a", "b"], ["c", "d"]]}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": t, "delete_rows": {"at": 0, "n": 2}})).contains("at least one row"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": t, "delete_rows": {"at": 1, "n": 5}})).contains("past the last row"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": t, "insert_cols": {"at": 9}})).contains("`at`"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": t, "sheet": s + 1})).contains("stays on its sheet"));
    let txt = ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [1, 1], "text": "x"}))["item"].as_u64().unwrap();
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": txt, "title": "x"})).contains("not a table"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "id": txt, "text": "a\tb"})).contains("not a table"));
}

#[test]
fn a_drawing_index_follows_the_sheets() {
    let (mut e, s) = sheet();
    ok(&mut e, json!({"cmd": "sheet_add", "name": "Ground floor plan", "number": "A-101"}));
    let t = ok(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [20, 30], "source": "sheets", "title": "DRAWING INDEX", "header_cols": 1}))["item"].as_u64().unwrap();
    let live = |e: &mut Engine| info_item(e, s, t)["layout"]["rows"].clone();
    assert_eq!(live(&mut e), json!([["Sheet", "Title"], ["A-601", "Schedules"], ["A-101", "Ground floor plan"]]));
    // a new sheet, a renamed one: the index shows them without being touched
    let n = ok(&mut e, json!({"cmd": "sheet_add", "name": "Elevations", "number": "A-201"}))["sheet"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "sheet_edit", "sheet": n, "name": "North and south elevations"}));
    assert_eq!(live(&mut e)[3], json!(["A-201", "North and south elevations"]));
    assert!(ir_texts(&mut e, s).iter().any(|r| r.0 == "North and south elevations"), "drawn as it is now");
    // typing into it is refused until it is unlinked; unlinked, it keeps what it showed
    assert!(refused(&mut e, json!({"cmd": "sheet_table", "id": t, "cells": [{"row": 0, "col": 0, "text": "No."}]})).contains("unlink"));
    assert!(refused(&mut e, json!({"cmd": "sheet_table_paste", "id": t, "text": "a\tb"})).contains("unlink"));
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "source": ""}));
    assert!(item(&e, t).get("source").is_none());
    assert_eq!(rows(&e, t)[3], json!(["A-201", "North and south elevations"]));
    ok(&mut e, json!({"cmd": "sheet_table", "id": t, "cells": [{"row": 0, "col": 0, "text": "No."}]}));
    ok(&mut e, json!({"cmd": "sheet_add", "name": "Sections"}));
    assert_eq!(rows(&e, t).as_array().unwrap().len(), 4, "unlinked: a new sheet doesn't change it");
}

#[test]
fn tables_print_as_tagged_tables_in_pdf_and_as_text_and_rules_in_svg_and_dxf() {
    let dir = scratch("print");
    let (mut e, s) = sheet();
    let doors = ok(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [20, 30], "text": DOORS}))["item"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "sheet_table", "id": doors, "title": "DOOR SCHEDULE", "cells": [{"row": 1, "col": 3, "text": "FD60\nsmoke seals"}]}));
    ok(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [250, 30], "rows": [["Sheet", "Title"], ["A-601", "Schedules"]], "border": "outer"}));
    // PDF: two tables, a row element each, one cell element per cell (a two-line cell is one cell), headers as TH
    let pdf = pdftext::parse(&export(&mut e, s, &dir, "pdf"));
    let count = |k: &str| pdf.objs.values().filter(|o| o.dict.contains(k)).count();
    assert_eq!(count("/S /Table"), 2, "one table each");
    assert_eq!(count("/S /TR"), 3 + 2);
    assert_eq!(count("/S /TH"), 4 + 2, "the header rows");
    // filled cells only: doors 3 rows by 4 (D1's fire rating now filled), the index's one body row
    assert_eq!(count("/S /TD"), 2 * 4 + 2);
    let text = pdf.text(pdf.pages()[0]).join(" ");
    for w in ["DOOR SCHEDULE", "FD30, self-closing", "FD60", "smoke seals", "A-601"] {
        assert!(text.contains(w), "{w} in the PDF's text");
    }
    // SVG: the words, and rules
    let svg = String::from_utf8(export(&mut e, s, &dir, "svg")).unwrap();
    for w in ["DOOR SCHEDULE", "FD30, self-closing", "1800 x 2100"] {
        assert!(svg.contains(w), "{w} in the SVG");
    }
    // DXF: TEXT entities and the rules as lines
    let dxf = String::from_utf8(export(&mut e, s, &dir, "dxf")).unwrap();
    assert!(dxf.contains("DOOR SCHEDULE") && dxf.contains("1800 x 2100"), "the DXF has the cells' text");
    assert!(dxf.matches("\nTEXT\n").count() >= 15, "a TEXT per cell line");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn text_blocks_wrap_to_their_width_and_align() {
    let (mut e, s) = sheet();
    let words = "All dimensions are in millimetres unless noted otherwise. Do not scale from this drawing.\n\nCheck all dimensions on site before work starts.";
    let r = ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [30, 50], "text": words, "width_mm": 60, "height_mm": 2.5}));
    let id = r["item"].as_u64().unwrap();
    let it = item(&e, id);
    assert_eq!(it["box_mm"], json!([60.0, 0.0]));
    assert_eq!(it["paragraphs"].as_array().unwrap().len(), 3, "a paragraph per line, the blank one kept");
    // the block's lines (the title block's text is on the page too, bottom right)
    let block = |e: &mut Engine| ir_texts(e, s).into_iter().filter(|r| r.1 < 150.0 && r.2 < 200.0).collect::<Vec<_>>();
    let runs = block(&mut e);
    assert!(runs.len() > 4, "wrapped onto more lines than paragraphs: {runs:?}");
    for r in &runs {
        assert!((r.1 - 30.0).abs() < 1e-6, "left aligned");
        assert!(draft::table::text_w(&r.0, 2.5, false) <= 60.0 + 1e-6, "{r:?} fits the width");
    }
    let joined: Vec<&str> = runs.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(joined.join(" "), words.replace("\n\n", " "), "every word, in order");
    // the blank paragraph is a blank line: the third paragraph starts two pitches below the second's last line
    let pitch = 2.5 / 0.73 * 1.25;
    let check = runs.iter().find(|r| r.0.starts_with("Check")).unwrap();
    let before = runs.iter().filter(|r| r.2 < check.2).map(|r| r.2).fold(0.0, f64::max);
    assert!((check.2 - before - 2.0 * pitch).abs() < 0.01);
    // the box sheets_info gives the editor
    let info = info_item(&mut e, s, id);
    let rect = info["layout"]["rect"].as_array().unwrap();
    assert_eq!(rect[0].as_f64(), Some(30.0));
    assert_eq!(rect[2].as_f64(), Some(60.0));
    assert_eq!(info["layout"]["lines"].as_u64(), Some(runs.len() as u64 + 1));
    // centred and right-aligned across the block
    ok(&mut e, json!({"cmd": "sheet_text", "id": id, "align": "right"}));
    for r in block(&mut e) {
        assert!((r.1 + draft::table::text_w(&r.0, 2.5, false) - 90.0).abs() < 0.01, "right edge at 90: {r:?}");
    }
    ok(&mut e, json!({"cmd": "sheet_text", "id": id, "align": "center"}));
    for r in block(&mut e) {
        assert!((r.1 + draft::table::text_w(&r.0, 2.5, false) / 2.0 - 60.0).abs() < 0.01, "centred on 60: {r:?}");
    }
    // plain text again: one line per paragraph
    ok(&mut e, json!({"cmd": "sheet_text", "id": id, "width_mm": 0, "align": "left"}));
    assert!(item(&e, id).get("box_mm").is_none());
    assert_eq!(block(&mut e).len(), 2);
    assert!(refused(&mut e, json!({"cmd": "sheet_text", "id": id, "width_mm": 2})).contains("width_mm"));
    assert!(!refused(&mut e, json!({"cmd": "sheet_text", "id": id, "align": "justify"})).is_empty());
}

#[test]
fn older_files_still_read_and_newer_table_fields_survive() {
    // a table as the first saved-sheets build wrote it (no header, title, border...) reads with the defaults and
    // writes back as it came
    let old = json!({"kind": "table", "sheet": 3, "at_mm": [20.0, 24.0], "cols_mm": [20.0, 30.0], "rows": [["A-101", "Plan"]]});
    let it: SheetItem = serde_json::from_value(old.clone()).unwrap();
    let SheetItem::Known(ItemKind::Table(t)) = &it else { panic!("a known table: {it:?}") };
    assert_eq!((t.header_rows, t.header_cols, t.title.as_str(), t.text_mm), (None, 0, "", None));
    assert_eq!(serde_json::to_value(&it).unwrap(), old, "nothing new is written for an old table");
    let txt = json!({"kind": "text", "sheet": 3, "at_mm": [1.0, 2.0], "paragraphs": ["x"]});
    assert_eq!(serde_json::to_value(serde_json::from_value::<SheetItem>(txt.clone()).unwrap()).unwrap(), txt);
    // a value from a newer build this one doesn't know: kept as it came
    let newer = json!({"kind": "table", "sheet": 3, "at_mm": [0.0, 0.0], "cols_mm": [], "rows": [["a"]], "border": "double"});
    let it: SheetItem = serde_json::from_value(newer.clone()).unwrap();
    assert!(it.is_raw());
    assert_eq!(serde_json::to_value(&it).unwrap(), newer);
    // and an old table in a saved file opens, draws, and saves back unchanged
    let dir = scratch("old");
    let (mut e, s) = sheet();
    let t = ok(&mut e, json!({"cmd": "sheet_table", "sheet": s, "at_mm": [20, 24], "rows": [["x"]]}))["item"].as_u64().unwrap();
    let p = dir.join("old.sketchcraft");
    ok(&mut e, json!({"cmd": "save", "path": p}));
    let mut file: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    let mut v = old.clone();
    v["sheet"] = json!(s);
    file["sheet_items"][t.to_string()] = v.clone();
    std::fs::write(&p, serde_json::to_vec(&file).unwrap()).unwrap();
    let mut g = Engine::new();
    ok(&mut g, json!({"cmd": "open", "path": p}));
    let got = ir_texts(&mut g, s);
    assert!(got.iter().any(|r| r.0 == "A-101" && r.3), "the old table header row is bold: {got:?} {}", item(&g, t));
    assert_eq!(item(&g, t), v);
    let p2 = dir.join("again.sketchcraft");
    ok(&mut g, json!({"cmd": "save", "path": p2}));
    let again: Value = serde_json::from_slice(&std::fs::read(&p2).unwrap()).unwrap();
    assert_eq!(again["sheet_items"][t.to_string()], v, "written back as it came");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_permit_sets_schedule_and_index_are_tables_you_can_edit() {
    let mut e = Engine::new();
    ok(&mut e, json!({"cmd": "wall", "points": [[0, 0, 0], [8, 0, 0], [8, 6, 0], [0, 6, 0]], "closed": true, "height": 2.7}));
    ok(&mut e, json!({"cmd": "insert", "asset": "door", "at": [1.0, -0.1, 0.0]}));
    ok(&mut e, json!({"cmd": "layout_preset", "preset": "permit", "paper": "a3"}));
    let info = ok(&mut e, json!({"cmd": "sheets_info"}));
    let cover = &info["sheets"][0];
    let t = cover["items"].as_array().unwrap().iter().find(|i| i["kind"] == "table").unwrap().clone();
    assert_eq!(t["header_cols"], 1, "the index's first column heads its rows");
    assert!(!t["title"].as_str().unwrap().is_empty());
    let id = t["id"].as_u64().unwrap();
    ok(&mut e, json!({"cmd": "sheet_table", "id": id, "cells": [{"row": 1, "col": 1, "text": "Cover sheet"}]}));
    assert!(ir_texts(&mut e, cover["id"].as_u64().unwrap()).iter().any(|r| r.0 == "Cover sheet"));
}

#[test]
fn a_long_text_block_and_a_big_table_lay_out_and_draw_fast() {
    let (mut e, s) = sheet();
    // 200 rows of a door schedule, pasted
    let mut csv = String::from("Mark,Width,Height,Type,Fire rating,Hardware\n");
    for i in 0..200 {
        csv.push_str(&format!("D{i},{},2100,Single leaf with vision panel,FD30,Lever set and closer {}\n", 800 + i, i % 7));
    }
    let t0 = Instant::now();
    let t = ok(&mut e, json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [15, 15], "text": csv}))["item"].as_u64().unwrap();
    let paste = t0.elapsed();
    // a long text block: 20,000 characters of notes
    let para = "The contractor shall verify all dimensions on site before any work starts and report discrepancies. ";
    let mut notes = String::new();
    while notes.len() + para.len() < 19_900 {
        notes.push_str(para);
        if notes.len().is_multiple_of(7) {
            notes.push('\n');
        }
    }
    ok(&mut e, json!({"cmd": "sheet_text", "sheet": s, "at_mm": [250, 20], "text": notes, "width_mm": 150, "height_mm": 1.8}));
    // laying out the table alone, many times
    let it = e.model.sheet_items[&(t as u32)].clone();
    let SheetItem::Known(ItemKind::Table(tb)) = &*it else { panic!() };
    let draw = draft::table::TableDraw { at_mm: tb.at_mm, cols_mm: tb.cols_mm.clone(), rows: tb.rows.clone(), header_rows: 1, header_cols: 0, title: String::new(), text_mm: 2.5, border: Default::default(), align: vec![] };
    let t1 = Instant::now();
    for _ in 0..10 {
        assert_eq!(draft::table::layout_table(&draw).ys.len(), 202);
    }
    let layout = t1.elapsed() / 10;
    // the whole page: the app's display list, and the PDF
    let t2 = Instant::now();
    let ir = ok(&mut e, json!({"cmd": "sheet_ir", "sheet": s}));
    let page = t2.elapsed();
    assert!(ir["counts"]["texts"].as_u64().unwrap() > 1200, "{}", ir["counts"]);
    let dir = scratch("perf");
    let t3 = Instant::now();
    export(&mut e, s, &dir, "pdf");
    let pdf = t3.elapsed();
    std::fs::remove_dir_all(&dir).ok();
    println!("tables perf: paste {paste:?}, 200-row layout {layout:?}, page display list {page:?}, PDF {pdf:?}");
    // budgets with room for a slow CI machine (an M1 does each in a few milliseconds to tens of milliseconds)
    assert!(layout.as_millis() < 100, "a 200-row table lays out in {layout:?}");
    assert!(paste.as_millis() < 1000, "pasting 200 rows took {paste:?}");
    assert!(page.as_millis() < 2000, "the page's display list took {page:?}");
    assert!(pdf.as_millis() < 4000, "the PDF took {pdf:?}");
}

#[test]
fn agents_and_plugins_may_make_tables_within_a_cell_budget() {
    let agent = Caller::Mcp(Workspace::anywhere());
    for c in ["sheet_table", "sheet_table_paste"] {
        assert!(permits(&agent, c) && permits(&Caller::Plugin, c) && permits(&Caller::Ui, c), "{c}");
    }
    assert!(vet(&agent, &mut json!({"cmd": "sheet_table", "sheet": 1, "at_mm": [1, 1], "rows": [["a", "b"]]})).is_ok());
    let n = MAX_TABLE_CELLS_UNTRUSTED;
    assert!(vet(&agent, &mut json!({"cmd": "sheet_table", "sheet": 1, "at_mm": [1, 1], "size": [n / 10 + 1, 10]})).is_err());
    assert!(vet(&Caller::Ui, &mut json!({"cmd": "sheet_table", "sheet": 1, "at_mm": [1, 1], "size": [n / 10 + 1, 10]})).is_ok());
    let big = "a\t".repeat(n + 1);
    assert!(vet(&agent, &mut json!({"cmd": "sheet_table_paste", "sheet": 1, "at_mm": [1, 1], "text": big})).is_err());
    assert!(vet(&agent, &mut json!({"cmd": "sheet_table_paste", "sheet": 1, "at_mm": [1, 1], "text": "a\tb\n1\t2"})).is_ok());
    let rows: Vec<Value> = (0..n / 5 + 1).map(|_| json!(["a", "b", "c", "d", "e"])).collect();
    assert!(vet(&agent, &mut json!({"cmd": "sheet_table", "sheet": 1, "at_mm": [1, 1], "rows": rows})).is_err());
    // an agent's table is marked as its own, and one undo step
    let (mut e, s) = sheet();
    let d = e.undo_depth();
    let r = e.exec_json_as(&agent, &json!({"cmd": "sheet_table_paste", "sheet": s, "at_mm": [10, 10], "text": DOORS}).to_string());
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(item(&e, r["item"].as_u64().unwrap())["made_by"], "agent");
    assert_eq!(e.undo_depth(), d + 1);
    let help = include_str!("../src/mcp.rs");
    assert!(help.contains("sheet_table {") && help.contains("sheet_table_paste {") && help.contains("width_mm"), "the MCP help says how");
}
