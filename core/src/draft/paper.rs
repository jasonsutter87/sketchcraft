//! Paper sizes and drawing scales.

use anyhow::{bail, Context, Result};

/// Every size the sheets know, landscape, in millimeters: (names, width, height).
const SIZES: &[(&[&str], f64, f64)] = &[
    (&["a4"], 297.0, 210.0),
    (&["a3"], 420.0, 297.0),
    (&["a2"], 594.0, 420.0),
    (&["a1"], 841.0, 594.0),
    (&["a0"], 1189.0, 841.0),
    (&["letter"], 279.4, 215.9),
    (&["tabloid", "11x17", "ansib"], 431.8, 279.4),
    (&["archa", "9x12"], 304.8, 228.6),
    (&["archb", "12x18"], 457.2, 304.8),
    (&["archc", "18x24"], 609.6, 457.2),
    (&["archd", "24x36"], 914.4, 609.6),
    (&["arche", "36x48"], 1219.2, 914.4),
    (&["ansic", "17x22"], 558.8, 431.8),
    (&["ansid", "22x34"], 863.6, 558.8),
    (&["ansie", "34x44"], 1117.6, 863.6),
];

pub const SIZE_NAMES: &str = "a4, a3, a2, a1, a0, letter, tabloid, arch a to e, ansi c to e";

/// Sheet sizes, landscape, in millimeters. "ARCH D", "arch-d" and "archd" all work.
pub fn sheet_mm(name: &str) -> Result<(f64, f64)> {
    let key: String = name.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    match SIZES.iter().find(|(names, ..)| names.contains(&key.as_str())) {
        Some((_, w, h)) => Ok((*w, *h)),
        None => bail!("sheet is one of: {SIZE_NAMES}"),
    }
}

/// A length like `1'-6"`, `3/16"`, `1 1/2"`, `20'`, `2ft` or `5in`, in inches.
fn inches(s: &str) -> Result<f64> {
    let s = s.trim().replace("feet", "'").replace("foot", "'").replace("ft", "'").replace("inches", "\"").replace("inch", "\"").replace("in", "\"");
    let mut total = 0.0;
    let mut cur = String::new();
    let mut any = false;
    for ch in s.chars() {
        match ch {
            '\'' | '"' => {
                let n = number(cur.trim_matches(|c: char| c == '-' || c.is_whitespace()))?;
                total += n * if ch == '\'' { 12.0 } else { 1.0 };
                cur.clear();
                any = true;
            }
            c => cur.push(c),
        }
    }
    if !cur.trim_matches(|c: char| c == '-' || c.is_whitespace()).is_empty() {
        bail!("a length like 1/4\" or 1'-0\" needs its unit marks");
    }
    if !any {
        bail!("a length like 1/4\" or 1'-0\"");
    }
    Ok(total)
}

/// `6`, `0.25`, `3/16` or `1 1/2`.
fn number(s: &str) -> Result<f64> {
    let one = |t: &str| -> Result<f64> {
        match t.split_once('/') {
            Some((a, b)) => Ok(a.trim().parse::<f64>()? / b.trim().parse::<f64>()?),
            None => Ok(t.trim().parse::<f64>()?),
        }
    };
    let mut parts = s.split_whitespace();
    let first = parts.next().context("a number")?;
    let v = one(first)? + parts.next().map(one).transpose()?.unwrap_or(0.0);
    if !v.is_finite() {
        bail!("a number");
    }
    Ok(v)
}

/// A drawing scale as its denominator: "1:100" is 100, `1/4"=1'-0"` is 48 (a model length
/// is that many times its length on the paper).
pub fn scale_of(s: &str) -> Result<f64> {
    let t = s.trim();
    let d = if let Some((l, r)) = t.split_once('=') {
        let (a, b) = (inches(l)?, inches(r)?);
        if a <= 0.0 {
            bail!("a scale like 1/4\"=1'-0\" has a length on each side");
        }
        b / a
    } else {
        // "1:100", or a bare denominator ("100")
        let (l, r) = t.split_once(':').unwrap_or(("1", t));
        let (l, r) = (l.trim().parse::<f64>().context("scale looks like 1:100 or 1/4\"=1'-0\"")?, r.trim().parse::<f64>().context("scale looks like 1:100 or 1/4\"=1'-0\"")?);
        if l <= 0.0 {
            bail!("scale looks like 1:100");
        }
        r / l
    };
    if !d.is_finite() || d <= 0.0 {
        bail!("scale must be positive");
    }
    Ok(d)
}

/// How a scale reads on the drawing: "1:100" as written, or the imperial form as written.
pub fn scale_label(s: &str, denom: f64) -> String {
    if s.contains('=') {
        s.trim().to_string()
    } else {
        format!("1:{denom}")
    }
}
