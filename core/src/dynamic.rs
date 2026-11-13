//! Dynamic Components: a component placement can carry attributes, plain
//! values or formulas ("=lenx/2+0.1"), that drive how it's drawn:
//!
//! - `lenx`, `leny`, `lenz`: its size in meters along its own axes
//!   (stretched from the component's own size)
//! - `copies`: extra copies along its red axis, `spacing` apart (default:
//!   its own length), like fence pickets or stair treads
//! - `material`: a color ("#a05a2c" or a material name) for its unpainted faces
//! - `hidden`: 1 to hide it
//!
//! Formulas can use + - * / ^, parentheses, comparisons (< <= > >= = <>),
//! min, max, round, floor, ceil, abs, sqrt, if(cond, a, b), other attributes
//! of the same placement by name, and `parent!name` for the placement it
//! sits inside. A component's definition can hold defaults for its
//! placements.

use crate::model::{Definition, Instance, Model};
use anyhow::{anyhow, bail, Result};
use glam::{DMat4, DVec3};
use std::collections::BTreeMap;

// ----- formulas -----------------------------------------------------------------

struct P<'a> {
    s: &'a [u8],
    i: usize,
    look: &'a dyn Fn(&str) -> Result<f64>,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, t: &str) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(t.as_bytes()) {
            self.i += t.len();
            true
        } else {
            false
        }
    }
    fn cmp(&mut self) -> Result<f64> {
        let a = self.sum()?;
        for (op, f) in [("<=", 0), (">=", 1), ("<>", 2), ("<", 3), (">", 4), ("=", 5)] {
            if self.eat(op) {
                let b = self.sum()?;
                let r = match f {
                    0 => a <= b,
                    1 => a >= b,
                    2 => (a - b).abs() > 1e-12,
                    3 => a < b,
                    4 => a > b,
                    _ => (a - b).abs() <= 1e-12,
                };
                return Ok(if r { 1.0 } else { 0.0 });
            }
        }
        Ok(a)
    }
    fn sum(&mut self) -> Result<f64> {
        let mut a = self.prod()?;
        loop {
            if self.eat("+") {
                a += self.prod()?;
            } else if self.eat("-") {
                a -= self.prod()?;
            } else {
                return Ok(a);
            }
        }
    }
    fn prod(&mut self) -> Result<f64> {
        let mut a = self.pow()?;
        loop {
            if self.eat("*") {
                a *= self.pow()?;
            } else if self.eat("/") {
                let b = self.pow()?;
                if b == 0.0 {
                    bail!("division by zero");
                }
                a /= b;
            } else {
                return Ok(a);
            }
        }
    }
    fn pow(&mut self) -> Result<f64> {
        let a = self.unary()?;
        if self.eat("^") {
            return Ok(a.powf(self.pow()?));
        }
        Ok(a)
    }
    fn unary(&mut self) -> Result<f64> {
        if self.eat("-") {
            return Ok(-self.unary()?);
        }
        if self.eat("+") {
            return self.unary();
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<f64> {
        self.ws();
        if self.eat("(") {
            let v = self.cmp()?;
            if !self.eat(")") {
                bail!("missing )");
            }
            return Ok(v);
        }
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if self.i > start {
            return std::str::from_utf8(&self.s[start..self.i])?.parse::<f64>().map_err(|_| anyhow!("bad number"));
        }
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || matches!(self.s[self.i], b'_' | b'!')) {
            self.i += 1;
        }
        if self.i == start {
            bail!("expected a number or a name at {}", start + 1);
        }
        let name = std::str::from_utf8(&self.s[start..self.i])?.to_ascii_lowercase();
        if self.eat("(") {
            let mut args = Vec::new();
            if !self.eat(")") {
                loop {
                    args.push(self.cmp()?);
                    if self.eat(")") {
                        break;
                    }
                    if !self.eat(",") {
                        bail!("expected , or ) in {name}()");
                    }
                }
            }
            let need = |n: usize| if args.len() == n { Ok(()) } else { Err(anyhow!("{name}() takes {n} value(s)")) };
            return Ok(match name.as_str() {
                "min" if !args.is_empty() => args.iter().copied().fold(f64::MAX, f64::min),
                "max" if !args.is_empty() => args.iter().copied().fold(f64::MIN, f64::max),
                "round" => {
                    need(1)?;
                    args[0].round()
                }
                "floor" => {
                    need(1)?;
                    args[0].floor()
                }
                "ceil" => {
                    need(1)?;
                    args[0].ceil()
                }
                "abs" => {
                    need(1)?;
                    args[0].abs()
                }
                "sqrt" => {
                    need(1)?;
                    args[0].max(0.0).sqrt()
                }
                "if" => {
                    need(3)?;
                    if args[0] != 0.0 {
                        args[1]
                    } else {
                        args[2]
                    }
                }
                _ => bail!("unknown function {name}()"),
            });
        }
        match name.as_str() {
            "pi" => Ok(std::f64::consts::PI),
            "true" => Ok(1.0),
            "false" => Ok(0.0),
            _ => (self.look)(&name),
        }
    }
}

/// Evaluate a formula; names are looked up with `look` (lowercase).
pub fn eval(expr: &str, look: &dyn Fn(&str) -> Result<f64>) -> Result<f64> {
    let e = expr.trim().trim_start_matches('=');
    let mut p = P { s: e.as_bytes(), i: 0, look };
    let v = p.cmp()?;
    p.ws();
    if p.i != p.s.len() {
        bail!("unexpected {:?} in {expr:?}", e[p.i..].chars().next().unwrap_or(' '));
    }
    if !v.is_finite() {
        bail!("{expr:?} isn't a number");
    }
    Ok(v)
}

// ----- attributes -----------------------------------------------------------------

/// A placement's attributes: its own over its component's defaults.
pub fn attrs_of(inst: &Instance, def: Option<&Definition>) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = def.map(|d| d.attrs.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect()).unwrap_or_default();
    for (k, v) in &inst.attrs {
        out.insert(k.to_ascii_lowercase(), v.clone());
    }
    out
}

/// Every attribute as a number where it is one (formulas worked out), in
/// the scope of `parent` (the enclosing placement's values).
pub fn resolve(attrs: &BTreeMap<String, String>, parent: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    fn get(name: &str, attrs: &BTreeMap<String, String>, parent: &BTreeMap<String, f64>, depth: usize) -> Result<f64> {
        if depth > 32 {
            bail!("formulas refer to each other in a loop");
        }
        if let Some(p) = name.strip_prefix("parent!") {
            return parent.get(p).copied().ok_or_else(|| anyhow!("the parent has no {p}"));
        }
        let raw = attrs.get(name).ok_or_else(|| anyhow!("no attribute {name}"))?;
        eval(raw, &|n| get(n, attrs, parent, depth + 1))
    }
    attrs.keys().filter_map(|k| get(k, attrs, parent, 0).ok().map(|v| (k.clone(), v))).collect()
}

/// How a placement draws: its transforms (one per copy) and a fill color.
pub struct Draw {
    pub placements: Vec<DMat4>,
    pub color: Option<[f32; 3]>,
    pub hidden: bool,
    /// numbers for nested placements' `parent!` lookups
    pub scope: BTreeMap<String, f64>,
}

fn hex(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0])
}

/// Work out a placement's drawing: `size` is its component's own size.
pub fn draw(root: &Model, inst: &Instance, def: Option<&Definition>, size: DVec3, parent: &BTreeMap<String, f64>) -> Draw {
    let attrs = attrs_of(inst, def);
    if attrs.is_empty() {
        return Draw { placements: vec![inst.transform], color: None, hidden: false, scope: BTreeMap::new() };
    }
    let mut v = resolve(&attrs, parent);
    // lengths: stretch from the component's own size
    let stretch = |want: Option<&f64>, have: f64| match want {
        Some(w) if *w > 0.0 && have > 1e-9 => w / have,
        _ => 1.0,
    };
    let s = DVec3::new(stretch(v.get("lenx"), size.x), stretch(v.get("leny"), size.y), stretch(v.get("lenz"), size.z));
    // the sizes it actually ends up, for formulas below it
    v.entry("lenx".into()).or_insert(size.x * s.x);
    v.entry("leny".into()).or_insert(size.y * s.y);
    v.entry("lenz".into()).or_insert(size.z * s.z);
    let base = inst.transform * DMat4::from_scale(s);
    let copies = v.get("copies").map(|c| c.round().clamp(0.0, 1000.0) as usize).unwrap_or(0);
    // spacing is measured in the parent's space, along the placement's red axis
    let step_len = v.get("spacing").copied().unwrap_or(size.x * s.x);
    let axis = inst.transform.transform_vector3(DVec3::X).normalize_or_zero();
    let placements = (0..=copies).map(|k| DMat4::from_translation(axis * step_len * k as f64) * base).collect();
    let color = attrs.get("material").and_then(|m| hex(m).or_else(|| root.find_material(m).map(|x| x.color)));
    let hidden = v.get("hidden").map_or(false, |h| *h != 0.0);
    Draw { placements, color, hidden, scope: v }
}

/// A report row value: numbers rounded for reading.
pub fn fmt(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == r.trunc() { format!("{r:.0}") } else { format!("{r}") }
}

// ----- Generate Report ------------------------------------------------------------

/// One row per group and component placement: what it is, how big, how much.
pub fn report(m: &Model) -> Vec<BTreeMap<&'static str, String>> {
    let faces = m.world_faces();
    let mut scratch = m.clone(); // the solid check can add paint entries; keep them out of the model
    let mut rows = Vec::new();
    let mut add = |kind: &'static str, id: crate::model::Id, name: String, component: String, tag: Option<String>, attrs: String| {
        let mine: Vec<_> = faces.iter().filter(|f| if kind == "Group" { f.group == Some(id) && f.face.is_some() } else { f.owner == id }).collect();
        let pts: Vec<DVec3> = mine.iter().flat_map(|f| f.outer.iter().copied()).collect();
        let size = if pts.is_empty() { DVec3::ZERO } else { pts.iter().fold((pts[0], pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p))).1 - pts.iter().fold(pts[0], |lo, p| lo.min(*p)) };
        let area: f64 = mine.iter().map(|f| crate::geom::area(&f.outer) - f.holes.iter().map(|h| crate::geom::area(h)).sum::<f64>()).sum();
        let volume = crate::csg::solid_info(&mut scratch, id).map(fmt).unwrap_or_default();
        let mut r = BTreeMap::new();
        r.insert("type", kind.to_string());
        r.insert("id", id.to_string());
        r.insert("name", name);
        r.insert("component", component);
        r.insert("tag", tag.unwrap_or_default());
        r.insert("size_x_m", fmt(size.x));
        r.insert("size_y_m", fmt(size.y));
        r.insert("size_z_m", fmt(size.z));
        r.insert("faces", mine.len().to_string());
        r.insert("area_m2", fmt(area));
        r.insert("volume_m3", volume);
        r.insert("attributes", attrs);
        rows.push(r);
    };
    for (&id, g) in &m.groups {
        add("Group", id, g.name.clone(), String::new(), g.tag.clone(), String::new());
    }
    for (&id, i) in &m.instances {
        let def = m.definitions.get(&i.def);
        let a = attrs_of(i, def);
        let vals = resolve(&a, &BTreeMap::new());
        let shown: Vec<String> = a.iter().map(|(k, v)| match vals.get(k) { Some(n) => format!("{k}={}", fmt(*n)), None => format!("{k}={v}") }).collect();
        add("Component", id, i.name.clone(), def.map(|d| d.name.clone()).unwrap_or_default(), i.tag.clone(), shown.join("; "));
    }
    rows
}

pub const REPORT_COLUMNS: [&str; 12] = ["type", "id", "name", "component", "tag", "size_x_m", "size_y_m", "size_z_m", "faces", "area_m2", "volume_m3", "attributes"];

pub fn report_csv(rows: &[BTreeMap<&'static str, String>]) -> String {
    let q = |s: &str| if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
    let mut out = REPORT_COLUMNS.join(",") + "\n";
    for r in rows {
        out += &REPORT_COLUMNS.iter().map(|c| q(r.get(c).map(|s| s.as_str()).unwrap_or(""))).collect::<Vec<_>>().join(",");
        out += "\n";
    }
    out
}
