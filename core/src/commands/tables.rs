//! Tables on sheets (#7): `sheet_table` (make or change one: cells, rows, columns, header, borders, widths) and
//! `sheet_table_paste` (a grid copied from a spreadsheet, tab- or comma-separated). Arms of `Engine::run_sheets`;
//! each is one ordinary model change, so one undo step. Text blocks are `sheet_text` with a `width_mm`.

use super::command::{CellArg, SpanArg};
use super::*;
use crate::draft::linework;
use crate::draft::table::{self as dt, TableDraw};
use crate::drafting::*;
use std::sync::Arc;

/// What a linked table may be filled from.
pub const SOURCES: [&str; 1] = ["sheets"];

/// The drawing index: a header, then each sheet's number and name, in the set's order.
pub fn index_rows(m: &Model) -> Vec<Vec<String>> {
    let mut order: Vec<(&Id, &Sheet)> = m.sheets.iter().collect();
    order.sort_by_key(|(id, s)| (s.order, **id));
    std::iter::once(vec!["Sheet".to_string(), "Title".to_string()]).chain(order.iter().map(|(_, s)| vec![s.number.clone(), s.name.clone()])).collect()
}

/// A table's rows as drawn: its own, or (linked) the live ones.
pub fn live_rows<'a>(m: &Model, t: &'a Table) -> std::borrow::Cow<'a, [Vec<String>]> {
    match t.source.as_deref() {
        Some("sheets") => std::borrow::Cow::Owned(index_rows(m)),
        _ => std::borrow::Cow::Borrowed(&t.rows[..]),
    }
}

/// A saved table, ready to draw.
pub fn table_draw(m: &Model, t: &Table) -> TableDraw {
    TableDraw {
        at_mm: t.at_mm,
        cols_mm: t.cols_mm.clone(),
        rows: live_rows(m, t).into_owned(),
        header_rows: t.header_rows.unwrap_or(1) as usize,
        header_cols: t.header_cols as usize,
        title: t.title.clone(),
        text_mm: t.text_mm.unwrap_or(TABLE_TEXT),
        border: t.border,
        align: t.align.clone(),
    }
}

/// A saved text (plain or a block), ready to draw.
pub fn text_draw(t: &Text) -> crate::layout::TextDraw {
    crate::layout::TextDraw { at_mm: t.at_mm, lines: t.paragraphs.clone(), height_mm: t.height_mm.unwrap_or(TEXT_HEIGHT), width_mm: t.box_mm.map(|b| b[0]).filter(|w| *w > 0.0), align: t.align }
}

/// Where a table's rows, columns and title are on the paper, and (linked) its live rows, for `sheets_info`.
pub fn table_info(m: &Model, t: &Table) -> Value {
    let d = table_draw(m, t);
    let l = dt::layout_table(&d);
    let r2 = |v: &[f64]| v.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>();
    json!({"xs": r2(&l.xs), "ys": r2(&l.ys), "title_at": l.title_at, "rows": d.rows, "text_mm": d.text_mm, "header_rows": d.header_rows, "pad_mm": dt::pad(d.text_mm).0})
}

/// Where a text's lines and box are on the paper, for `sheets_info`.
pub fn text_info(t: &Text) -> Value {
    let l = dt::layout_text(&text_draw(t));
    let r = l.rect.map(|x| (x * 100.0).round() / 100.0);
    json!({"rect": r, "lines": l.lines.len()})
}

/// Cell text as stored: tabs become spaces, carriage returns go; other control characters are refused.
pub(super) fn clean_cell(s: &str) -> Result<String> {
    if s.chars().count() > MAX_CELL_CHARS {
        bail!("a table cell holds at most {MAX_CELL_CHARS} characters");
    }
    if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r')) {
        bail!("a cell's text has control characters in it");
    }
    Ok(s.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    "))
}

fn ncols(rows: &[Vec<String>]) -> usize {
    rows.iter().map(Vec::len).max().unwrap_or(0)
}

/// Every row as long as the longest.
fn square(rows: &mut [Vec<String>]) {
    let n = ncols(rows);
    for r in rows.iter_mut() {
        r.resize(n, String::new());
    }
}

/// Grow the grid to at least `r` rows and `c` columns, within the limits.
fn grow(t: &mut Table, r: usize, c: usize) -> Result<()> {
    if r > MAX_TABLE_ROWS || c > MAX_TABLE_COLS || r.saturating_mul(c.max(ncols(&t.rows))) > MAX_TABLE_CELLS {
        bail!("a table has at most {MAX_TABLE_ROWS} rows, {MAX_TABLE_COLS} columns and {MAX_TABLE_CELLS} cells");
    }
    let c = c.max(ncols(&t.rows));
    if t.rows.len() < r {
        t.rows.resize(r, vec![String::new(); c]);
    }
    for row in t.rows.iter_mut() {
        if row.len() < c {
            row.resize(c, String::new());
        }
    }
    Ok(())
}

/// The whole table is within the limits and sane.
pub(super) fn check(t: &Table) -> Result<()> {
    let (r, c) = (t.rows.len(), ncols(&t.rows));
    if r == 0 || c == 0 {
        bail!("a table needs at least one row and one column (delete it with sheet_item_delete instead)");
    }
    if r > MAX_TABLE_ROWS || c > MAX_TABLE_COLS || r * c > MAX_TABLE_CELLS {
        bail!("a table has at most {MAX_TABLE_ROWS} rows, {MAX_TABLE_COLS} columns and {MAX_TABLE_CELLS} cells (this is {r} by {c})");
    }
    if t.cols_mm.len() > MAX_TABLE_COLS || t.align.len() > MAX_TABLE_COLS {
        bail!("a table has at most {MAX_TABLE_COLS} columns");
    }
    Ok(())
}

fn span(what: &str, s: &SpanArg, len: usize) -> Result<std::ops::Range<usize>> {
    if s.n == 0 || s.at > len || s.n > MAX_TABLE_ROWS {
        bail!("{what}: `at` is 0 to {len} and `n` at least 1");
    }
    Ok(s.at..s.at + s.n)
}

impl Engine {
    /// The table `id`, to change.
    fn table_of(&self, id: Id, sheet: Option<Id>) -> Result<Table> {
        let Some(item) = self.model.sheet_items.get(&id) else { bail!("no sheet item {id}") };
        let SheetItem::Known(ItemKind::Table(t)) = &**item else { bail!("sheet item {id} is a {}, not a table", item.kind_name()) };
        if sheet.is_some_and(|s| s != t.sheet) {
            bail!("a table stays on its sheet (leave `sheet` out when changing one)");
        }
        Ok(t.clone())
    }

    /// A new table's sheet and corner, checked.
    fn new_table(&self, sheet: Option<Id>, at_mm: Option<[f64; 2]>) -> Result<Table> {
        let Some(s) = sheet else { bail!("say which sheet the table goes on (`sheet`), or which table to change (`id`)") };
        if !self.model.sheets.contains_key(&s) {
            bail!("no sheet {s}");
        }
        let Some(at) = at_mm else { bail!("a new table needs `at_mm` (its top left corner)") };
        corner(at)?;
        Ok(Table { sheet: s, at_mm: at, made_by: self.made_by(), ..Default::default() })
    }

    fn put_table(&mut self, id: Option<Id>, t: Table) -> Value {
        let (r, c) = (t.rows.len(), ncols(&t.rows));
        let id = id.unwrap_or_else(|| self.model.new_id());
        self.model.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Table(t))));
        json!({"item": id, "table": id, "rows": r, "cols": c})
    }

    pub(super) fn run_tables(&mut self, cmd: Command) -> Result<Value> {
        Ok(match cmd {
            Command::SheetTable { id, sheet, at_mm, rows, size, cells, insert_rows, delete_rows, insert_cols, delete_cols, cols_mm, header_rows, header_cols, title, text_mm, border, align, source, z } => {
                let mut t = match id {
                    Some(id) => self.table_of(id, sheet)?,
                    None => {
                        if rows.is_none() && size.is_none() && source.as_deref() != Some("sheets") {
                            bail!("a new table needs `rows` (its cells' text), `size: [rows, cols]` or `source: \"sheets\"`");
                        }
                        self.new_table(sheet, at_mm)?
                    }
                };
                if let Some(p) = at_mm {
                    corner(p)?;
                    t.at_mm = p;
                }
                if let Some(src) = &source {
                    match src.as_str() {
                        "" => {
                            // unlinked: it keeps what it showed last
                            t.rows = live_rows(&self.model, &t).into_owned();
                            t.source = None;
                        }
                        s if SOURCES.contains(&s) => {
                            t.source = Some(s.to_string());
                            t.rows = live_rows(&self.model, &t).into_owned();
                        }
                        s => bail!("a table can be filled from {SOURCES:?} (got {s:?}; \"\" unlinks it)"),
                    }
                }
                let edits_cells = rows.is_some() || size.is_some() || !cells.is_empty() || insert_rows.is_some() || delete_rows.is_some() || insert_cols.is_some() || delete_cols.is_some();
                if edits_cells && t.source.is_some() {
                    bail!("this table is filled from the document's {}: unlink it (source: \"\") to type in it", t.source.as_deref().unwrap_or("?"));
                }
                if let Some(s) = &insert_rows {
                    let r = span("insert_rows", s, t.rows.len())?;
                    let want = t.rows.len() + s.n;
                    grow(&mut t, want, 0)?;
                    let blank = vec![String::new(); ncols(&t.rows)];
                    t.rows.truncate(t.rows.len() - s.n);
                    t.rows.splice(r.start..r.start, std::iter::repeat_n(blank, s.n));
                }
                if let Some(s) = &delete_rows {
                    let r = span("delete_rows", s, t.rows.len())?;
                    if r.end > t.rows.len() {
                        bail!("delete_rows runs past the last row ({} rows)", t.rows.len());
                    }
                    t.rows.drain(r);
                }
                if let Some(s) = &insert_cols {
                    let n = ncols(&t.rows);
                    let r = span("insert_cols", s, n)?;
                    grow(&mut t, 0, n + s.n)?;
                    for row in t.rows.iter_mut() {
                        row.truncate(n);
                        row.splice(r.start..r.start, std::iter::repeat_n(String::new(), s.n));
                    }
                    if r.start < t.cols_mm.len() {
                        t.cols_mm.splice(r.start..r.start, std::iter::repeat_n(0.0, s.n));
                    }
                    if r.start < t.align.len() {
                        t.align.splice(r.start..r.start, std::iter::repeat_n(Align::Left, s.n));
                    }
                }
                if let Some(s) = &delete_cols {
                    let n = ncols(&t.rows);
                    let r = span("delete_cols", s, n)?;
                    if r.end > n {
                        bail!("delete_cols runs past the last column ({n} columns)");
                    }
                    for row in t.rows.iter_mut() {
                        row.drain(r.start..r.end.min(row.len()));
                    }
                    if r.start < t.cols_mm.len() {
                        let e = r.end.min(t.cols_mm.len());
                        t.cols_mm.drain(r.start..e);
                    }
                    if r.start < t.align.len() {
                        let e = r.end.min(t.align.len());
                        t.align.drain(r.start..e);
                    }
                }
                if let Some(rs) = &rows {
                    if rs.len() > MAX_TABLE_ROWS || ncols(rs) > MAX_TABLE_COLS || rs.len().saturating_mul(ncols(rs)) > MAX_TABLE_CELLS {
                        bail!("a table has at most {MAX_TABLE_ROWS} rows, {MAX_TABLE_COLS} columns and {MAX_TABLE_CELLS} cells");
                    }
                    t.rows = rs.iter().map(|r| r.iter().map(|c| clean_cell(c)).collect::<Result<Vec<_>>>()).collect::<Result<_>>()?;
                }
                if let Some([r, c]) = size {
                    if r == 0 || c == 0 {
                        bail!("size is [rows, columns], each at least 1");
                    }
                    grow(&mut t, r, c)?;
                    t.rows.truncate(r);
                    for row in t.rows.iter_mut() {
                        row.truncate(c);
                    }
                }
                for CellArg { row, col, text } in &cells {
                    grow(&mut t, row + 1, col + 1)?;
                    t.rows[*row][*col] = clean_cell(text)?;
                }
                square(&mut t.rows);
                if let Some(ws) = cols_mm {
                    if let Some(bad) = ws.iter().find(|w| !(w.is_finite() && (**w == 0.0 || (BLOCK_WIDTH_RANGE.0..=BLOCK_WIDTH_RANGE.1).contains(*w)))) {
                        bail!("a column is 0 (fit its text) or {} to {} mm wide (got {bad})", BLOCK_WIDTH_RANGE.0, BLOCK_WIDTH_RANGE.1);
                    }
                    t.cols_mm = ws;
                    while t.cols_mm.last() == Some(&0.0) {
                        t.cols_mm.pop();
                    }
                }
                if let Some(h) = header_rows {
                    if h as usize > MAX_TABLE_ROWS {
                        bail!("header_rows is at most {MAX_TABLE_ROWS}");
                    }
                    t.header_rows = (h != 1).then_some(h);
                }
                if let Some(h) = header_cols {
                    if h as usize > MAX_TABLE_COLS {
                        bail!("header_cols is at most {MAX_TABLE_COLS}");
                    }
                    t.header_cols = h;
                }
                if let Some(s) = title {
                    if s.chars().count() > 200 || s.chars().any(char::is_control) {
                        bail!("a table's title is one line of at most 200 characters");
                    }
                    t.title = s;
                }
                if let Some(h) = text_mm {
                    if !(h.is_finite() && (TEXT_HEIGHT_RANGE.0..=TEXT_HEIGHT_RANGE.1).contains(&h)) {
                        bail!("text_mm is between {} and {} mm", TEXT_HEIGHT_RANGE.0, TEXT_HEIGHT_RANGE.1);
                    }
                    t.text_mm = (h != TABLE_TEXT).then_some(h);
                }
                if let Some(b) = border {
                    t.border = b;
                }
                if let Some(a) = align {
                    t.align = a;
                    while t.align.last() == Some(&Align::Left) {
                        t.align.pop();
                    }
                }
                if let Some(z) = z {
                    t.z = z;
                }
                check(&t)?;
                self.put_table(id, t)
            }
            Command::SheetTablePaste { id, sheet, at_mm, text, format, at, header_rows } => {
                let sep = match format.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    None | Some("auto") | Some("") => dt::sniff(&text),
                    Some("tsv") => dt::Sep::Tab,
                    Some("csv") => dt::Sep::Comma,
                    Some("ssv") => dt::Sep::Semicolon,
                    Some(f) => bail!("format is auto, tsv, csv or ssv (got {f:?})"),
                };
                let grid = dt::parse_grid(&text, sep);
                if grid.is_empty() || grid.iter().flatten().all(|c| c.trim().is_empty()) {
                    bail!("there is nothing to paste (no cells in the text)");
                }
                let (gr, gc) = (grid.len(), ncols(&grid));
                if gr > MAX_TABLE_ROWS || gc > MAX_TABLE_COLS || gr * gc > MAX_TABLE_CELLS {
                    bail!("a table has at most {MAX_TABLE_ROWS} rows, {MAX_TABLE_COLS} columns and {MAX_TABLE_CELLS} cells (the paste is {gr} by {gc})");
                }
                let grid: Vec<Vec<String>> = grid.iter().map(|r| r.iter().map(|c| clean_cell(c)).collect::<Result<Vec<_>>>()).collect::<Result<_>>()?;
                let t = match id {
                    Some(tid) => {
                        let mut t = self.table_of(tid, sheet)?;
                        if t.source.is_some() {
                            bail!("this table is filled from the document's {}: unlink it (source: \"\") to paste into it", t.source.as_deref().unwrap_or("?"));
                        }
                        let [r0, c0] = at.unwrap_or([0, 0]);
                        grow(&mut t, r0.saturating_add(gr), c0.saturating_add(gc))?;
                        for (i, row) in grid.into_iter().enumerate() {
                            for (j, c) in row.into_iter().enumerate() {
                                t.rows[r0 + i][c0 + j] = c;
                            }
                        }
                        if let Some(h) = header_rows {
                            t.header_rows = (h != 1).then_some(h.min(MAX_TABLE_ROWS as u32));
                        }
                        t
                    }
                    None => {
                        if at.is_some() {
                            bail!("`at` (a cell) is for pasting into a table (`id`); a new table takes `sheet` and `at_mm`");
                        }
                        let mut t = self.new_table(sheet, at_mm)?;
                        t.rows = grid;
                        t.header_rows = header_rows.filter(|h| *h != 1).map(|h| h.min(MAX_TABLE_ROWS as u32));
                        t
                    }
                };
                check(&t)?;
                let mut v = self.put_table(id, t);
                v["pasted"] = json!([gr, gc]);
                v["format"] = json!(match sep {
                    dt::Sep::Tab => "tsv",
                    dt::Sep::Comma => "csv",
                    dt::Sep::Semicolon => "ssv",
                });
                v
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}

fn corner(p: [f64; 2]) -> Result<()> {
    if !p.iter().all(|x| x.is_finite() && x.abs() <= linework::MAX_COORD) {
        bail!("at_mm is a point on the sheet, in millimetres");
    }
    Ok(())
}
