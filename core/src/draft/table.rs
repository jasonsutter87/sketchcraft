//! Tables and text blocks on paper (#7): wrapping text to a width, laying a table's rows and columns out, drawing
//! both into the display list, and reading a grid pasted from a spreadsheet (tab- or comma-separated text).
//!
//! Everything is paper millimetres from the page's top left, y down. Letter heights are capital heights (as paper
//! text's are, `sheet::CAP_OF_SIZE`). The same layout feeds the drawing and `sheets_info` (where the editor finds
//! cells), so what is drawn and what is clicked agree.

use super::ir::*;
use super::sheet::{CAP_OF_SIZE, TEXT_LEADING};
use crate::drafting::{Align, Border};

/// The width of `s` in mm with capitals `h` mm tall.
pub fn text_w(s: &str, h: f64, bold: bool) -> f64 {
    super::font::width(s, h / CAP_OF_SIZE, bold)
}

/// One paragraph wrapped to `width` mm (capitals `h` mm tall): breaks between words, and inside a word only when it
/// is wider than the whole width. An empty paragraph is one empty line (a blank line between paragraphs).
pub fn wrap(par: &str, width: f64, h: f64, bold: bool) -> Vec<String> {
    let size = h / CAP_OF_SIZE;
    let adv = |c: char| {
        let (f, g) = super::font::glyph(c, bold);
        super::font::advance_1000(f, g) * size / 1000.0
    };
    let space = adv(' ');
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut lw = 0.0;
    for word in par.split(' ') {
        let ww: f64 = word.chars().map(adv).sum();
        let gap = if line.is_empty() { 0.0 } else { space };
        if lw + gap + ww <= width + 1e-9 || (line.is_empty() && ww <= width + 1e-9) {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
            lw += gap + ww;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            lw = 0.0;
        }
        if ww <= width + 1e-9 {
            line.push_str(word);
            lw = ww;
            continue;
        }
        // a word wider than the line: break it where it fills a line
        for c in word.chars() {
            let a = adv(c);
            if lw + a > width + 1e-9 && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                lw = 0.0;
            }
            line.push(c);
            lw += a;
        }
    }
    lines.push(line);
    lines
}

/// The distance between two lines' baselines, for capitals `h` mm tall.
pub fn pitch(h: f64) -> f64 {
    h / CAP_OF_SIZE * TEXT_LEADING
}

// ---- text blocks ------------------------------------------------------------------------------------------------

/// A text, ready to lay out: plain text (no `width`: one line per paragraph) or a text block (wrapped to `width`).
#[derive(Clone, Debug, Default)]
pub struct TextDraw {
    /// the first line's baseline start (paper mm from the top left)
    pub at_mm: [f64; 2],
    pub lines: Vec<String>,
    pub height_mm: f64,
    /// a text block's width: its paragraphs wrap to it
    pub width_mm: Option<f64>,
    pub align: Align,
}

/// A text laid out: each line's baseline start, and the box around it all (x, y, w, h).
#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    pub lines: Vec<(String, [f64; 2])>,
    pub rect: [f64; 4],
}

pub fn layout_text(t: &TextDraw) -> TextLayout {
    let h = t.height_mm;
    let p = pitch(h);
    let mut lines: Vec<String> = Vec::new();
    for par in &t.lines {
        match t.width_mm {
            Some(w) => lines.extend(wrap(par, w, h, false)),
            None => lines.push(par.clone()),
        }
    }
    let widths: Vec<f64> = lines.iter().map(|l| text_w(l, h, false)).collect();
    let widest = widths.iter().copied().fold(0.0, f64::max);
    let box_w = t.width_mm.unwrap_or(widest);
    // the box's left: a plain text's alignment is about `at_mm`
    let left = match (t.width_mm, t.align) {
        (None, Align::Center) => t.at_mm[0] - widest / 2.0,
        (None, Align::Right) => t.at_mm[0] - widest,
        _ => t.at_mm[0],
    };
    let out: Vec<(String, [f64; 2])> = lines
        .into_iter()
        .zip(widths)
        .enumerate()
        .map(|(i, (l, w))| {
            let x = match t.align {
                Align::Left => left,
                Align::Center => left + (box_w - w) / 2.0,
                Align::Right => left + box_w - w,
            };
            (l, [x, t.at_mm[1] + i as f64 * p])
        })
        .collect();
    let n = out.len().max(1) as f64;
    // from a capital's top on the first line to the descenders on the last
    let top = t.at_mm[1] - h;
    let bottom = t.at_mm[1] + (n - 1.0) * p + h * 0.3;
    TextLayout { lines: out, rect: [left, top, box_w, bottom - top] }
}

/// Draw a text (plain or a block).
pub fn draw_text(cv: &mut Cv, pens: &Pens, t: &TextDraw) {
    let size = t.height_mm / CAP_OF_SIZE;
    for (line, at) in layout_text(t).lines {
        if line.is_empty() {
            continue;
        }
        cv.push(Prim::Text(TextPrim { at, size, bold: false, rgb: pens.ink, angle: 0.0, text: line, role: Role::Paragraph }));
    }
}

// ---- tables -----------------------------------------------------------------------------------------------------

/// A table, ready to lay out (a saved `Table` item, with its live rows when it is linked).
#[derive(Clone, Debug, Default)]
pub struct TableDraw {
    pub at_mm: [f64; 2],
    pub cols_mm: Vec<f64>,
    pub rows: Vec<Vec<String>>,
    pub header_rows: usize,
    pub header_cols: usize,
    pub title: String,
    pub text_mm: f64,
    pub border: Border,
    pub align: Vec<Align>,
}

/// A table laid out: column and row edges (absolute paper mm; `xs` has one more than the columns, `ys` one more
/// than the rows), each cell's wrapped lines, and the title's baseline when it has one.
#[derive(Clone, Debug, Default)]
pub struct TableLayout {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub cells: Vec<Vec<Vec<String>>>,
    pub title_at: Option<[f64; 2]>,
}

/// Room between a cell's rules and its text, across and down, for capitals `h` mm tall.
pub fn pad(h: f64) -> (f64, f64) {
    (1.5f64.max(h * 0.6), 1.0f64.max(h * 0.5))
}

/// The narrowest and widest a column is made when it fits its text.
pub const AUTO_COL: (f64, f64) = (8.0, 80.0);

/// How wide a column of these texts is made to fit them (one line per explicit line, no wrapping), within `AUTO_COL`.
pub fn fit_width<'a>(cells: impl Iterator<Item = (&'a str, bool)>, h: f64) -> f64 {
    let widest = cells.flat_map(|(s, bold)| s.split('\n').map(move |l| text_w(l, h, bold))).fold(0.0, f64::max);
    (widest + 2.0 * pad(h).0 + 0.5).clamp(AUTO_COL.0, AUTO_COL.1)
}

/// How many columns a table has (its longest row, or its widths when wider).
pub fn ncols(t: &TableDraw) -> usize {
    t.rows.iter().map(Vec::len).max().unwrap_or(0).max(t.cols_mm.len())
}

fn is_header(t: &TableDraw, r: usize, c: usize) -> bool {
    r < t.header_rows || c < t.header_cols
}

pub fn layout_table(t: &TableDraw) -> TableLayout {
    let h = t.text_mm;
    let (px, py) = pad(h);
    let p = pitch(h);
    let n = ncols(t);
    let widths: Vec<f64> = (0..n)
        .map(|c| match t.cols_mm.get(c) {
            Some(w) if *w > 0.0 => *w,
            _ => fit_width(t.rows.iter().enumerate().filter_map(|(r, row)| row.get(c).map(|s| (s.as_str(), is_header(t, r, c)))), h),
        })
        .collect();
    let mut xs = Vec::with_capacity(n + 1);
    let mut x = t.at_mm[0];
    xs.push(x);
    for w in &widths {
        x += w;
        xs.push(x);
    }
    let mut ys = Vec::with_capacity(t.rows.len() + 1);
    let mut y = t.at_mm[1];
    ys.push(y);
    let mut cells = Vec::with_capacity(t.rows.len());
    for (r, row) in t.rows.iter().enumerate() {
        let mut lines_in_row = 1usize;
        let mut wrapped: Vec<Vec<String>> = Vec::with_capacity(n);
        for (c, w) in widths.iter().enumerate() {
            let s = row.get(c).map_or("", String::as_str);
            let inner = (w - 2.0 * px).max(1.0);
            let bold = is_header(t, r, c);
            let lines: Vec<String> = if s.is_empty() { vec![] } else { s.split('\n').flat_map(|l| wrap(l, inner, h, bold)).collect() };
            lines_in_row = lines_in_row.max(lines.len());
            wrapped.push(lines);
        }
        y += 2.0 * py + h + (lines_in_row - 1) as f64 * p;
        ys.push(y);
        cells.push(wrapped);
    }
    let title_at = (!t.title.is_empty()).then(|| [t.at_mm[0], t.at_mm[1] - py - h * 0.3]);
    TableLayout { xs, ys, cells, title_at }
}

/// Draw a table. `index` tells this page's tables apart in the PDF's structure (1 and up; 0 is the old schedule).
pub fn draw_table(cv: &mut Cv, pens: &Pens, t: &TableDraw, index: u16) {
    let l = layout_table(t);
    let h = t.text_mm;
    let size = h / CAP_OF_SIZE;
    let (px, py) = pad(h);
    let p = pitch(h);
    if let Some(at) = l.title_at {
        cv.push(Prim::Text(TextPrim { at, size: size * 1.3, bold: true, rgb: pens.ink, angle: 0.0, text: t.title.clone(), role: Role::Caption }));
    }
    let (x0, x1) = (l.xs[0], *l.xs.last().unwrap_or(&l.xs[0]));
    let (y0, y1) = (l.ys[0], *l.ys.last().unwrap_or(&l.ys[0]));
    let line = |cv: &mut Cv, a: [f64; 2], b: [f64; 2], w: f64| cv.push(Prim::Path(PathPrim { segs: vec![Seg::M(a), Seg::L(b)], pen: Some(pens.ink(w)), fill: None, prov: None }));
    let hr = t.header_rows.min(t.rows.len());
    if x1 > x0 && y1 > y0 && t.border != Border::None {
        {
            // rules between rows (the header's heavy), then columns, then the outline over them
            for (i, y) in l.ys.iter().enumerate().skip(1).take(l.ys.len().saturating_sub(2)) {
                if i == hr {
                    line(cv, [x0, *y], [x1, *y], pens.heavy);
                } else if matches!(t.border, Border::Grid | Border::Rules) {
                    line(cv, [x0, *y], [x1, *y], pens.hair);
                }
            }
            if t.border == Border::Grid {
                for x in l.xs.iter().skip(1).take(l.xs.len().saturating_sub(2)) {
                    line(cv, [*x, y0], [*x, y1], pens.hair);
                }
            }
            let segs = vec![Seg::M([x0, y0]), Seg::L([x1, y0]), Seg::L([x1, y1]), Seg::L([x0, y1]), Seg::Z];
            cv.push(Prim::Path(PathPrim { segs, pen: Some(pens.ink(pens.medium)), fill: None, prov: None }));
        }
    }
    for (r, row) in l.cells.iter().enumerate() {
        for (c, lines) in row.iter().enumerate() {
            let header = is_header(t, r, c);
            let inner = l.xs[c + 1] - l.xs[c] - 2.0 * px;
            for (k, s) in lines.iter().enumerate() {
                if s.is_empty() {
                    continue;
                }
                let w = text_w(s, h, header);
                let x = l.xs[c]
                    + px
                    + match t.align.get(c).copied().unwrap_or_default() {
                        Align::Left => 0.0,
                        Align::Center => (inner - w) / 2.0,
                        Align::Right => inner - w,
                    };
                let at = [x, l.ys[r] + py + h + k as f64 * p];
                let role = Role::Cell { table: index, row: r.min(u16::MAX as usize) as u16, col: c as u16, header };
                cv.push(Prim::Text(TextPrim { at, size, bold: header, rgb: pens.ink, angle: 0.0, text: s.clone(), role }));
            }
        }
    }
}

// ---- pasting a grid ---------------------------------------------------------------------------------------------

/// How pasted text is split into cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sep {
    Tab,
    Comma,
    Semicolon,
}

/// Which separator text uses: a tab on its first line means tab-separated (what spreadsheets copy), else commas, or
/// semicolons when the first line has those and no commas (a European spreadsheet's CSV).
pub fn sniff(text: &str) -> Sep {
    let first = text.lines().next().unwrap_or("");
    if first.contains('\t') {
        Sep::Tab
    } else if !first.contains(',') && first.contains(';') {
        Sep::Semicolon
    } else {
        Sep::Comma
    }
}

/// Cells from tab- or comma-separated text, as spreadsheets copy it and RFC 4180 writes it: a cell in double quotes
/// may hold the separator, newlines and doubled quotes (`""` is one `"`). Line ends may be `\r\n`. A last empty line
/// is not a row. Rows are made as long as the longest (missing cells empty).
pub fn parse_grid(text: &str, sep: Sep) -> Vec<Vec<String>> {
    let d = match sep {
        Sep::Tab => '\t',
        Sep::Comma => ',',
        Sep::Semicolon => ';',
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    // whether anything was read on this row (an empty line still ends a row only when it had something)
    let mut started = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else if c == '\r' && chars.peek() == Some(&'\n') {
                // a spreadsheet's newline inside a cell
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => {
                quoted = true;
                started = true;
            }
            c if c == d => {
                row.push(std::mem::take(&mut cell));
                started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                started = false;
            }
            c => {
                cell.push(c);
                started = true;
            }
        }
    }
    if started || !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    let n = rows.iter().map(Vec::len).max().unwrap_or(0);
    for r in &mut rows {
        r.resize(n, String::new());
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_between_words_and_inside_long_ones() {
        let lines = wrap("the quick brown fox jumps over the lazy dog", 20.0, 2.5, false);
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(text_w(l, 2.5, false) <= 20.0 + 1e-6, "{l}");
        }
        assert_eq!(lines.join(" "), "the quick brown fox jumps over the lazy dog");
        let long = wrap(&"x".repeat(100), 10.0, 2.5, false);
        assert!(long.len() > 3 && long.concat() == "x".repeat(100));
        assert_eq!(wrap("", 10.0, 2.5, false), vec![String::new()]);
    }

    #[test]
    fn parses_quoted_csv_and_tsv() {
        let g = parse_grid("Mark,Size,Note\nD1,\"900 x 2100\",\"say \"\"hi\"\", then\nnext line\"\r\nD2,800\n", Sep::Comma);
        assert_eq!(g, vec![vec!["Mark", "Size", "Note"], vec!["D1", "900 x 2100", "say \"hi\", then\nnext line"], vec!["D2", "800", ""]]);
        let t = parse_grid("a\tb\r\n1\t2", sniff("a\tb\r\n1\t2"));
        assert_eq!(t, vec![vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(sniff("a;b\n1;2"), Sep::Semicolon);
        assert_eq!(sniff("a,b;c"), Sep::Comma);
        assert!(parse_grid("", Sep::Comma).is_empty());
        assert_eq!(parse_grid("a,,\n", Sep::Comma), vec![vec!["a", "", ""]]);
    }
}
