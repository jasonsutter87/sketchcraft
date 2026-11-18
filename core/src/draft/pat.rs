//! Hatch patterns: the AutoCAD `.pat` format, and SketchCraft's own built-in set (design §3.3).
//!
//! A `.pat` file is plain text:
//!
//! ```text
//! ; a comment
//! *NAME, a description
//! angle, x-origin, y-origin, delta-x, delta-y [, dash-1, dash-2, ...]
//! ```
//!
//! Each line under a header is one family of parallel lines: `angle` in degrees, a point the family passes
//! through, the step from one line to the next (`delta-x` along the line, `delta-y` across it, both in the line's
//! own frame) and an optional dash list (positive: pen down, negative: pen up, 0: a dot).
//!
//! Files carry no unit. AutoCAD's own set is in inches and its ISO set in millimetres; a `;%UNITS=INCH` or
//! `;%UNITS=MM` comment (as Revit writes) says which, and otherwise the caller's default applies. Everything is
//! converted to millimetres here.
//!
//! The text may come from anywhere (an agent, a downloaded file), so the parser never panics and caps what it
//! keeps: the size of the text, the number of patterns, families per pattern and dashes per family, and every
//! number must be finite and not huge. A line it can't read is skipped with a warning; a pattern with nothing
//! usable left is dropped.
//!
//! **Built-ins.** `BUILTIN` below is SketchCraft's own set, written from scratch for this project and dedicated
//! to the public domain (CC0 1.0, https://creativecommons.org/publicdomain/zero/1.0/). It is not derived from
//! Autodesk's `acad.pat`/`acadiso.pat` or any other licensed set (design "Hatch licensing").

use std::sync::OnceLock;

/// Most bytes of `.pat` text read at once.
pub const MAX_TEXT: usize = 256 * 1024;
/// Most patterns kept from one text.
pub const MAX_PATTERNS: usize = 256;
/// Most line families in one pattern.
pub const MAX_FAMILIES: usize = 64;
/// Most dashes in one family.
pub const MAX_DASHES: usize = 16;
/// Longest pattern name.
pub const MAX_NAME: usize = 63;
/// Largest magnitude a number may have (mm or degrees).
const MAX_ABS: f64 = 1.0e5;
/// Most warnings listed (then "and N more").
const MAX_WARNINGS: usize = 20;

/// One family of parallel lines, in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct Family {
    /// degrees, counter-clockwise from +x
    pub angle: f64,
    /// a point the first line passes through
    pub origin: [f64; 2],
    /// the step to the next line: along the line, then across it (the line's own frame)
    pub delta: [f64; 2],
    /// on (positive), off (negative), dot (0); empty: a continuous line
    pub dash: Vec<f64>,
}

/// A named pattern, in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub name: String,
    pub desc: String,
    pub families: Vec<Family>,
}

/// What a text held: the patterns that could be read, and what was skipped and why.
#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub patterns: Vec<Pattern>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Mm,
    Inch,
}

impl Units {
    fn mm(self) -> f64 {
        match self {
            Units::Mm => 1.0,
            Units::Inch => 25.4,
        }
    }
}

/// A name a pattern may have: 1 to 63 characters, letters, digits, space, `_`, `-` and `.`.
pub fn valid_name(n: &str) -> bool {
    let n = n.trim();
    !n.is_empty() && n.chars().count() <= MAX_NAME && n.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.'))
}

struct Warn(Vec<String>, usize);

impl Warn {
    fn push(&mut self, s: String) {
        if self.0.len() < MAX_WARNINGS {
            self.0.push(s);
        } else {
            self.1 += 1;
        }
    }
    fn done(mut self) -> Vec<String> {
        if self.1 > 0 {
            self.0.push(format!("and {} more", self.1));
        }
        self.0
    }
}

fn num(s: &str) -> Option<f64> {
    let v: f64 = s.trim().parse().ok()?;
    (v.is_finite() && v.abs() <= MAX_ABS).then_some(v)
}

/// Read `.pat` text. `units` is what a file that doesn't say means. Never fails: what can't be read is skipped and
/// listed in `warnings`.
pub fn parse(text: &str, units: Units) -> Parsed {
    let mut w = Warn(Vec::new(), 0);
    let text = if text.len() > MAX_TEXT {
        w.push(format!("the file is larger than {} KB: only the start was read", MAX_TEXT / 1024));
        let mut end = MAX_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut units = units;
    let mut out: Vec<Pattern> = Vec::new();
    // the pattern being read, and whether it went over a limit (then it is dropped)
    let mut cur: Option<(Pattern, bool)> = None;
    let mut full = false;
    let finish = |cur: &mut Option<(Pattern, bool)>, out: &mut Vec<Pattern>, w: &mut Warn| {
        if let Some((p, bad)) = cur.take() {
            if bad {
                return;
            }
            if p.families.is_empty() {
                w.push(format!("pattern {:?} has no lines it can draw: skipped", p.name));
            } else if out.iter().any(|q| q.name.eq_ignore_ascii_case(&p.name)) {
                w.push(format!("pattern {:?} appears twice: the first one is kept", p.name));
            } else {
                out.push(p);
            }
        }
    };
    for (ln, raw) in text.split('\n').enumerate() {
        let ln = ln + 1;
        let line = raw.trim_end_matches('\r').trim();
        if line.len() > 4096 {
            w.push(format!("line {ln} is too long: skipped"));
            continue;
        }
        if let Some(c) = line.strip_prefix(';') {
            let c = c.trim().to_ascii_uppercase().replace(' ', "");
            if let Some(u) = c.strip_prefix("%UNITS=") {
                match u {
                    "MM" => units = Units::Mm,
                    "INCH" | "INCHES" | "IN" => units = Units::Inch,
                    _ => w.push(format!("line {ln}: unknown units {u:?}")),
                }
            }
            continue;
        }
        // a comment after the data
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(h) = line.strip_prefix('*') {
            finish(&mut cur, &mut out, &mut w);
            if out.len() >= MAX_PATTERNS {
                if !full {
                    w.push(format!("more than {MAX_PATTERNS} patterns: the rest were skipped"));
                    full = true;
                }
                continue;
            }
            let (name, desc) = h.split_once(',').unwrap_or((h, ""));
            let name = name.trim();
            let desc: String = desc.trim().chars().filter(|c| !c.is_control()).take(200).collect();
            if !valid_name(name) {
                w.push(format!("line {ln}: {:?} isn't a pattern name (letters, digits, space, _ - .; up to {MAX_NAME}): skipped", name.chars().take(80).collect::<String>()));
                cur = Some((Pattern { name: String::new(), desc, families: Vec::new() }, true));
                continue;
            }
            cur = Some((Pattern { name: name.to_string(), desc, families: Vec::new() }, false));
            continue;
        }
        let Some((p, bad)) = cur.as_mut() else {
            if !full {
                w.push(format!("line {ln}: a line outside any pattern (no *NAME above it): skipped"));
            }
            continue;
        };
        if *bad {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 5 {
            w.push(format!("line {ln} in {:?}: a line needs angle, x, y, delta-x, delta-y: skipped", p.name));
            continue;
        }
        if parts.len() > 5 + MAX_DASHES {
            w.push(format!("line {ln} in {:?}: more than {MAX_DASHES} dashes: skipped", p.name));
            continue;
        }
        let vals: Option<Vec<f64>> = parts.iter().map(|s| num(s)).collect();
        let Some(vals) = vals else {
            w.push(format!("line {ln} in {:?}: a number that isn't one (or is out of range): skipped", p.name));
            continue;
        };
        let k = units.mm();
        let fam = Family { angle: vals[0].rem_euclid(360.0), origin: [vals[1] * k, vals[2] * k], delta: [vals[3] * k, vals[4] * k], dash: vals[5..].iter().map(|d| d * k).collect() };
        if fam.delta[1].abs() < 1e-6 {
            w.push(format!("line {ln} in {:?}: its lines have no spacing (delta-y is 0): skipped", p.name));
            continue;
        }
        if !fam.dash.is_empty() {
            let period: f64 = fam.dash.iter().map(|d| d.abs()).sum();
            if period < 1e-6 {
                w.push(format!("line {ln} in {:?}: its dashes have no length: skipped", p.name));
                continue;
            }
            if fam.dash.iter().all(|d| *d < 0.0) {
                w.push(format!("line {ln} in {:?}: only gaps, nothing to draw: skipped", p.name));
                continue;
            }
        }
        if p.families.len() >= MAX_FAMILIES {
            w.push(format!("pattern {:?} has more than {MAX_FAMILIES} lines: skipped", p.name));
            *bad = true;
            continue;
        }
        p.families.push(fam);
    }
    finish(&mut cur, &mut out, &mut w);
    Parsed { patterns: out, warnings: w.done() }
}

fn n(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" { "0".into() } else { s.to_string() }
}

/// A pattern as `.pat` text, in millimetres (it says so), under `name`.
pub fn to_text(p: &Pattern, name: &str) -> String {
    let mut s = format!(";%UNITS=MM\n*{}, {}\n", name, p.desc);
    for f in &p.families {
        let mut parts = vec![n(f.angle), n(f.origin[0]), n(f.origin[1]), n(f.delta[0]), n(f.delta[1])];
        parts.extend(f.dash.iter().map(|d| n(*d)));
        s += &parts.join(", ");
        s.push('\n');
    }
    s
}

/// SketchCraft's own hatch patterns, in paper millimetres at scale 1. CC0 1.0: written from scratch for this
/// project and dedicated to the public domain (see the module notes).
pub const BUILTIN: &str = "\
;%UNITS=MM
; SketchCraft built-in hatch patterns. CC0 1.0 (public domain dedication).
; Written from scratch for SketchCraft; not derived from any other pattern library.
*diagonal, Diagonal lines (general cut, masonry in plan)
45, 0,0, 0,2.5
*cross, Crossed diagonals
45, 0,0, 0,2.5
135, 0,0, 0,2.5
*steel, Paired diagonals (metal)
45, 0,0, 0,3
45, 0,0.85, 0,3
*grid, Square grid
0, 0,0, 0,3
90, 0,0, 0,3
*dots, Staggered dots
0, 0,0, 1.5,1.5, 0,-3
*brick, Running bond brick courses
0, 0,0, 0,3
90, 0,0, 3,3, 3,-3
*concrete, Concrete: scattered strokes and dots
50, 0,0, 4.1,4.3, 0.8,-7.3
140, 1.3,2.1, 3.7,5.1, 0.6,-6.2
20, 2.6,0.4, 2.9,3.9, 0,-5.3
95, 0.7,3.3, 5.3,4.7, 0,-6.9
*earth, Earth: blocks of short strokes
0, 0,0, 6,6, 3,-9
0, 0,0.9, 6,6, 3,-9
0, 0,1.8, 6,6, 3,-9
90, 4.2,0, 6,6, 3,-9
90, 5.1,0, 6,6, 3,-9
90, 6,0, 6,6, 3,-9
*insulation, Batt insulation zigzag between rules
0, 0,0, 0,12
0, 0,6, 0,12
60, 0,0, 3.4641,-6, 6.9282,-6.9282
120, 6.9282,0, 3.4641,6, 6.9282,-6.9282
*wood, Wood grain: long broken strokes
0, 0,0, 7,1.4, 9,-1.2,3,-1.8
3, 2.5,0.7, 9,1.4, 6,-6
*sand, Sand: fine scattered dots
0, 0,0, 1.3,1.9, 0,-2.9
0, 0.7,0.9, 2.1,2.3, 0,-3.7
0, 1.6,0.4, 0.9,2.9, 0,-2.2
";

/// The built-in patterns, parsed once.
pub fn builtins() -> &'static [Pattern] {
    static B: OnceLock<Vec<Pattern>> = OnceLock::new();
    B.get_or_init(|| parse(BUILTIN, Units::Mm).patterns)
}

/// Is `name` a built-in pattern's?
pub fn is_builtin(name: &str) -> bool {
    builtins().iter().any(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

/// The pattern a name means: the file's own (`Drafting::patterns`) first, then the built-ins (names compare
/// without case).
pub fn lookup(d: &crate::drafting::Drafting, name: &str) -> Option<Pattern> {
    let name = name.trim();
    let own = d.patterns.get_key_value(name).or_else(|| d.patterns.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)));
    if let Some((key, text)) = own {
        let mut p = parse(text, Units::Mm).patterns.into_iter().next()?;
        p.name = key.clone();
        return Some(p);
    }
    builtins().iter().find(|p| p.name.eq_ignore_ascii_case(name)).cloned()
}
