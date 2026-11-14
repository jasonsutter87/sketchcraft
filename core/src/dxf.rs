//! DXF import (AutoCAD's interchange format; DWG files come through it via a
//! converter). Reads lines, polylines (with arc bulges), circles, arcs, 3D
//! faces, polyface meshes and block references, in their layers and units.
//! Each layer becomes a group tagged with the layer's name; closed outlines
//! become faces, ready to push/pull, the way SketchUp imports CAD.

use std::collections::HashMap;

use anyhow::{bail, Result};
use glam::{DMat4, DVec3};

use crate::model::{Id, Model};

/// What one import made.
pub struct DxfResult {
    pub group: Id,
    pub layers: usize,
    pub faces: usize,
    pub edges: usize,
    /// units the drawing said it was in (or the ones assumed)
    pub units: String,
    pub skipped: usize,
    /// text labels and dimensions brought in as annotations
    pub labels: usize,
    pub dimensions: usize,
    /// where most of the drawing is (the 2nd to 98th percentile of its points),
    /// so the view can land on the plans and not a scaled-up sheet border
    pub focus: [DVec3; 2],
}

#[derive(Clone, Debug)]
enum Shape {
    /// an open run of points
    Path(Vec<DVec3>),
    /// a closed outline (a face if flat)
    Loop(Vec<DVec3>),
    /// a 3D face
    Face(Vec<DVec3>),
    /// a text label
    Text(DVec3, String),
    /// a linear dimension: measured points and the offset to its line
    Dim(DVec3, DVec3, DVec3),
}

/// Plain text from DXF TEXT/MTEXT: formatting codes out, specials in.
fn plain_text(raw: &str) -> String {
    let mut out = String::new();
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('P') | Some('p') | Some('~') | Some('X') => out.push(' '),
                Some(c @ ('\\' | '{' | '}')) => out.push(c),
                Some('L' | 'l' | 'O' | 'o' | 'K' | 'k') => {}
                Some('U') if it.peek() == Some(&'+') => {
                    it.next();
                    let hex: String = (0..4).filter_map(|_| it.next()).collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                }
                Some('S') => {
                    // stacked fraction "1^2;" or "1#2;"
                    for c in it.by_ref() {
                        if c == ';' {
                            break;
                        }
                        out.push(if c == '^' || c == '#' { '/' } else { c });
                    }
                }
                // \A1; \C3; \fArial|b0; \H2.5; ... : codes with an argument up to ';'
                Some(_) => {
                    for c in it.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                None => {}
            },
            '{' | '}' => {}
            '%' if it.peek() == Some(&'%') => {
                it.next();
                match it.next().map(|c| c.to_ascii_lowercase()) {
                    Some('c') => out.push('\u{2300}'),
                    Some('d') => out.push('\u{b0}'),
                    Some('p') => out.push('\u{b1}'),
                    Some('%') => out.push('%'),
                    _ => {}
                }
            }
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Debug)]
struct Ent {
    kind: String,
    codes: Vec<(i32, String)>,
    /// POLYLINE's vertices (each a VERTEX entity)
    verts: Vec<Vec<(i32, String)>>,
}

impl Ent {
    fn f(&self, code: i32) -> Option<f64> {
        self.codes.iter().find(|(c, _)| *c == code).and_then(|(_, v)| v.trim().parse().ok())
    }
    fn s(&self, code: i32) -> Option<&str> {
        self.codes.iter().find(|(c, _)| *c == code).map(|(_, v)| v.trim())
    }
    fn i(&self, code: i32) -> i64 {
        self.f(code).unwrap_or(0.0) as i64
    }
    fn p(&self, x: i32) -> DVec3 {
        DVec3::new(self.f(x).unwrap_or(0.0), self.f(x + 10).unwrap_or(0.0), self.f(x + 20).unwrap_or(0.0))
    }
    fn layer(&self) -> String {
        self.s(8).filter(|s| !s.is_empty()).unwrap_or("0").to_string()
    }
}

/// Group-code / value pairs.
fn pairs(text: &str) -> Result<Vec<(i32, String)>> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(code) = lines.next() {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        let Ok(c) = code.parse::<i32>() else { bail!("this isn't a text DXF (binary DXF isn't supported; save as ASCII DXF)") };
        let v = lines.next().unwrap_or("").to_string();
        out.push((c, v));
    }
    Ok(out)
}

/// Split a run of pairs into entities (each starts at a 0 code).
fn entities(ps: &[(i32, String)]) -> Vec<Ent> {
    let mut out: Vec<Ent> = Vec::new();
    for (c, v) in ps {
        if *c == 0 {
            let kind = v.trim().to_string();
            if kind == "VERTEX" {
                if let Some(last) = out.last_mut().filter(|e| e.kind == "POLYLINE") {
                    last.verts.push(Vec::new());
                    continue;
                }
            }
            if kind == "SEQEND" {
                continue;
            }
            out.push(Ent { kind, codes: Vec::new(), verts: Vec::new() });
        } else if let Some(e) = out.last_mut() {
            if e.kind == "POLYLINE" && !e.verts.is_empty() {
                e.verts.last_mut().unwrap().push((*c, v.clone()));
            } else {
                e.codes.push((*c, v.clone()));
            }
        }
    }
    out
}

/// AutoCAD's arbitrary axis algorithm: the entity's own (OCS) axes for an extrusion direction.
fn ocs(n: DVec3) -> DMat4 {
    let n = n.normalize_or(DVec3::Z);
    if (n - DVec3::Z).length() < 1e-9 {
        return DMat4::IDENTITY;
    }
    let ax = if n.x.abs() < 1.0 / 64.0 && n.y.abs() < 1.0 / 64.0 { DVec3::Y.cross(n) } else { DVec3::Z.cross(n) }.normalize();
    let ay = n.cross(ax).normalize();
    DMat4::from_cols(ax.extend(0.0), ay.extend(0.0), n.extend(0.0), glam::DVec4::W)
}

/// How far a curve's segments may stray from the true curve, in meters (in
/// the model). Small arcs get fewer segments: a revision cloud's thousands of
/// tiny scallops become one segment each, not fifteen.
const CURVE_TOLERANCE: f64 = 0.0005;

/// Segments for an arc of `sweep` radians and radius `r`: 48 to a full turn
/// at most, fewer when fewer stay within `tol` (same units as `r`).
fn arc_steps(sweep: f64, r: f64, tol: f64) -> usize {
    let by_angle = ((sweep.abs() / std::f64::consts::TAU * 48.0).ceil() as usize).max(2);
    if !(tol > 0.0) || !r.is_finite() {
        return by_angle;
    }
    if r <= tol {
        return 1;
    }
    let per = 2.0 * (1.0 - tol / r).acos();
    by_angle.min(((sweep.abs() / per).ceil() as usize).max(1))
}

/// Points along an arc (center, radius, start and end angle in degrees,
/// counter-clockwise); `tol` is the allowed error in the arc's own units.
fn arc(c: DVec3, r: f64, a0: f64, a1: f64, tol: f64) -> Vec<DVec3> {
    let mut sweep = (a1 - a0).rem_euclid(360.0);
    if sweep < 1e-9 {
        sweep = 360.0;
    }
    let full = sweep >= 360.0;
    let n = arc_steps(sweep.to_radians(), r, tol).max(if full { 8 } else { 1 });
    (0..=n)
        .map(|k| {
            let a = (a0 + sweep * k as f64 / n as f64).to_radians();
            c + DVec3::new(a.cos(), a.sin(), 0.0) * r
        })
        .collect()
}

/// A polyline's points, with arc bulges between them turned into segments.
fn bulged(pts: &[(DVec3, f64)], closed: bool, tol: f64) -> Vec<DVec3> {
    let mut out = Vec::new();
    let n = pts.len();
    for i in 0..n {
        let (p, bulge) = pts[i];
        out.push(p);
        if !closed && i == n - 1 {
            break;
        }
        let q = pts[(i + 1) % n].0;
        if bulge.abs() > 1e-9 {
            // bulge = tan(sweep / 4); the arc runs counter-clockwise when positive
            let sweep = 4.0 * bulge.atan();
            let chord = q - p;
            let l = chord.truncate().length();
            if l < 1e-12 {
                continue;
            }
            let r = (l / (2.0 * (sweep / 2.0).sin())).abs();
            let mid = (p + q) / 2.0;
            // left of the chord; the sign of tan puts it on the right side when it should be
            let left = DVec3::new(-chord.y, chord.x, 0.0).normalize();
            let c = mid + left * ((l / 2.0) / (sweep / 2.0).tan());
            let a0 = (p.y - c.y).atan2(p.x - c.x);
            let steps = arc_steps(sweep, r, tol);
            for k in 1..steps {
                let a = a0 + sweep * k as f64 / steps as f64;
                out.push(DVec3::new(c.x + r * a.cos(), c.y + r * a.sin(), p.z + (q.z - p.z) * k as f64 / steps as f64));
            }
        }
    }
    out
}

/// The shapes one entity draws (in its own coordinates; INSERTs are expanded).
fn shapes(e: &Ent, blocks: &HashMap<String, (DVec3, Vec<Ent>)>, t: DMat4, layer: &str, depth: usize, out: &mut Vec<(String, Shape)>, skipped: &mut usize) {
    // entities on layer 0 inside a block take the block reference's layer
    let lay = match e.layer().as_str() {
        "0" if depth > 0 => layer.to_string(),
        l => l.to_string(),
    };
    let xf = |pts: Vec<DVec3>, o: DMat4| -> Vec<DVec3> { pts.into_iter().map(|p| t.transform_point3(o.transform_point3(p))).collect() };
    let o = ocs(DVec3::new(e.f(210).unwrap_or(0.0), e.f(220).unwrap_or(0.0), e.f(230).unwrap_or(1.0)));
    // the curve tolerance in this entity's own units (blocks may be scaled)
    let k = t.x_axis.truncate().length().max(t.y_axis.truncate().length()).max(t.z_axis.truncate().length());
    let tol = if k > 1e-12 { CURVE_TOLERANCE / k } else { 0.0 };
    match e.kind.as_str() {
        "LINE" => out.push((lay, Shape::Path(xf(vec![e.p(10), e.p(11)], DMat4::IDENTITY)))),
        "3DFACE" => {
            let mut c = vec![e.p(10), e.p(11), e.p(12), e.p(13)];
            if c[3].distance(c[2]) < 1e-9 {
                c.pop();
            }
            out.push((lay, Shape::Face(xf(c, DMat4::IDENTITY))));
        }
        "CIRCLE" => {
            let mut pts = arc(e.p(10), e.f(40).unwrap_or(0.0), 0.0, 360.0, tol);
            pts.pop();
            out.push((lay, Shape::Loop(xf(pts, o))));
        }
        "ARC" => out.push((lay, Shape::Path(xf(arc(e.p(10), e.f(40).unwrap_or(0.0), e.f(50).unwrap_or(0.0), e.f(51).unwrap_or(360.0), tol), o)))),
        "LWPOLYLINE" => {
            let z = e.f(38).unwrap_or(0.0);
            let mut pts: Vec<(DVec3, f64)> = vec![];
            let mut x = None;
            for (c, v) in &e.codes {
                let v = v.trim().parse::<f64>().unwrap_or(0.0);
                match c {
                    10 => x = Some(v),
                    20 => {
                        if let Some(xx) = x.take() {
                            pts.push((DVec3::new(xx, v, z), 0.0));
                        }
                    }
                    42 => {
                        if let Some(last) = pts.last_mut() {
                            last.1 = v;
                        }
                    }
                    _ => {}
                }
            }
            let closed = e.i(70) & 1 == 1;
            let line = xf(bulged(&pts, closed, tol), o);
            out.push((lay, if closed { Shape::Loop(line) } else { Shape::Path(line) }));
        }
        "POLYLINE" => {
            let flags = e.i(70);
            let vs: Vec<Ent> = e.verts.iter().map(|codes| Ent { kind: "VERTEX".into(), codes: codes.clone(), verts: vec![] }).collect();
            if flags & 64 != 0 {
                // polyface mesh: point vertices, then faces that index them (1-based, negative = hidden edge)
                let points: Vec<DVec3> = vs.iter().filter(|v| v.i(70) & 64 != 0 || v.i(70) & 192 == 192).map(|v| v.p(10)).collect();
                for v in vs.iter().filter(|v| v.i(70) & 128 != 0 && v.i(70) & 64 == 0) {
                    let idx: Vec<usize> = [71, 72, 73, 74].iter().map(|c| v.i(*c).unsigned_abs() as usize).filter(|i| *i > 0).collect();
                    let c: Vec<DVec3> = idx.iter().filter_map(|i| points.get(i - 1).copied()).collect();
                    if c.len() >= 3 {
                        out.push((lay.clone(), Shape::Face(xf(c, DMat4::IDENTITY))));
                    }
                }
            } else if flags & 16 != 0 {
                *skipped += 1; // polygon meshes (M×N grids) aren't read yet
            } else {
                let three_d = flags & 8 != 0;
                let pts: Vec<(DVec3, f64)> = vs.iter().map(|v| (v.p(10), v.f(42).unwrap_or(0.0))).collect();
                let closed = flags & 1 == 1;
                let line = xf(bulged(&pts, closed, tol), if three_d { DMat4::IDENTITY } else { o });
                out.push((lay, if closed { Shape::Loop(line) } else { Shape::Path(line) }));
            }
        }
        "INSERT" => {
            let name = e.s(2).unwrap_or("").to_string();
            let Some((base, ents)) = blocks.get(&name) else {
                *skipped += 1;
                return;
            };
            if depth > 8 {
                *skipped += 1;
                return;
            }
            let at = e.p(10);
            let s = DVec3::new(e.f(41).unwrap_or(1.0), e.f(42).unwrap_or(1.0), e.f(43).unwrap_or(1.0));
            let rot = e.f(50).unwrap_or(0.0).to_radians();
            // rows and columns (MINSERT)
            let (cols, rows) = (e.i(70).max(1) as usize, e.i(71).max(1) as usize);
            let (dx, dy) = (e.f(44).unwrap_or(0.0), e.f(45).unwrap_or(0.0));
            for r in 0..rows.min(100) {
                for c in 0..cols.min(100) {
                    let place = o * DMat4::from_translation(at) * DMat4::from_rotation_z(rot) * DMat4::from_translation(DVec3::new(c as f64 * dx, r as f64 * dy, 0.0)) * DMat4::from_scale(s) * DMat4::from_translation(-*base);
                    for be in ents {
                        shapes(be, blocks, t * place, &lay, depth + 1, out, skipped);
                    }
                }
            }
        }
        "TEXT" | "MTEXT" => {
            let raw = if e.kind == "MTEXT" {
                // long MTEXT comes in 250-character pieces (code 3), then the rest (code 1)
                let mut r: String = e.codes.iter().filter(|(c, _)| *c == 3).map(|(_, v)| v.as_str()).collect();
                r += e.s(1).unwrap_or("");
                r
            } else {
                e.s(1).unwrap_or("").to_string()
            };
            let text = plain_text(&raw);
            if text.is_empty() {
                return;
            }
            // TEXT: aligned text is placed by its second point; MTEXT is in world coordinates
            let aligned = e.kind == "TEXT" && (e.i(72) != 0 || e.i(73) != 0) && e.f(11).is_some();
            let at = if aligned { e.p(11) } else { e.p(10) };
            let at = xf(vec![at], if e.kind == "MTEXT" { DMat4::IDENTITY } else { o })[0];
            out.push((lay, Shape::Text(at, text.chars().take(200).collect())));
        }
        "DIMENSION" => {
            // linear dimensions (rotated/horizontal/vertical, and aligned), from their definition points
            let kind = e.i(70) & 7;
            let (a, b, d) = (e.p(13), e.p(14), e.p(10));
            let dir = if kind == 1 {
                (b - a).normalize_or_zero()
            } else if kind == 0 {
                let r = e.f(50).unwrap_or(0.0).to_radians();
                DVec3::new(r.cos(), r.sin(), 0.0)
            } else {
                DVec3::ZERO
            };
            if dir == DVec3::ZERO || a.distance(b) < 1e-12 {
                *skipped += 1;
                return;
            }
            let b2 = a + dir * (b - a).dot(dir);
            let off = (d - a) - dir * (d - a).dot(dir);
            let w = xf(vec![a, b2, a + off], DMat4::IDENTITY);
            out.push((lay, Shape::Dim(w[0], w[1], w[2] - w[0])));
        }
        "POINT" | "HATCH" | "ATTRIB" | "ATTDEF" | "VIEWPORT" | "SOLID" | "SPLINE" | "ELLIPSE" | "LEADER" | "MLEADER" | "IMAGE" | "WIPEOUT" => *skipped += 1,
        _ => *skipped += 1,
    }
}

/// Meters per drawing unit from $INSUNITS (or the units asked for).
fn unit_scale(code: i64) -> Option<(f64, &'static str)> {
    Some(match code {
        1 => (0.0254, "in"),
        2 => (0.3048, "ft"),
        4 => (0.001, "mm"),
        5 => (0.01, "cm"),
        6 => (1.0, "m"),
        _ => return None,
    })
}

pub fn named_units(u: &str) -> Option<(f64, &'static str)> {
    Some(match u.to_ascii_lowercase().as_str() {
        "in" | "inch" | "inches" => (0.0254, "in"),
        "ft" | "feet" | "foot" => (0.3048, "ft"),
        "mm" => (0.001, "mm"),
        "cm" => (0.01, "cm"),
        "m" | "meters" => (1.0, "m"),
        _ => return None,
    })
}

/// Import DXF text into its own group (a group per layer inside it).
pub fn import(m: &mut Model, text: &str, name: &str, units: Option<&str>, at: DVec3) -> Result<DxfResult> {
    let ps = pairs(text)?;
    // sections
    let mut header: Vec<(i32, String)> = vec![];
    let mut blocks_raw: Vec<(i32, String)> = vec![];
    let mut ents_raw: Vec<(i32, String)> = vec![];
    let mut i = 0;
    while i < ps.len() {
        if ps[i].0 == 0 && ps[i].1.trim() == "SECTION" && i + 1 < ps.len() && ps[i + 1].0 == 2 {
            let which = ps[i + 1].1.trim().to_string();
            let start = i + 2;
            let mut j = start;
            while j < ps.len() && !(ps[j].0 == 0 && ps[j].1.trim() == "ENDSEC") {
                j += 1;
            }
            let body = ps[start..j].to_vec();
            match which.as_str() {
                "HEADER" => header = body,
                "BLOCKS" => blocks_raw = body,
                "ENTITIES" => ents_raw = body,
                _ => {}
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    if ents_raw.is_empty() {
        bail!("no drawing in that DXF (its ENTITIES section is empty or missing)");
    }
    // units: asked for, else the header's, else millimeters
    let ins = header.iter().position(|(c, v)| *c == 9 && v.trim() == "$INSUNITS").and_then(|k| header.get(k + 1)).and_then(|(_, v)| v.trim().parse::<i64>().ok());
    let (scale, unit) = match units.and_then(named_units) {
        Some(u) => u,
        None => ins.and_then(unit_scale).unwrap_or((0.001, "mm")),
    };
    // blocks: name → (base point, entities)
    let mut blocks: HashMap<String, (DVec3, Vec<Ent>)> = HashMap::new();
    let mut cur: Option<(String, DVec3, Vec<(i32, String)>)> = None;
    for (k, (c, v)) in blocks_raw.iter().enumerate() {
        if *c == 0 && v.trim() == "BLOCK" {
            // the block's header codes run until its first entity
            let mut name = String::new();
            let mut base = DVec3::ZERO;
            for (cc, vv) in blocks_raw[k + 1..].iter().take_while(|(cc, _)| *cc != 0) {
                match cc {
                    2 => name = vv.trim().to_string(),
                    10 => base.x = vv.trim().parse().unwrap_or(0.0),
                    20 => base.y = vv.trim().parse().unwrap_or(0.0),
                    30 => base.z = vv.trim().parse().unwrap_or(0.0),
                    _ => {}
                }
            }
            cur = Some((name, base, vec![]));
        } else if *c == 0 && v.trim() == "ENDBLK" {
            if let Some((name, base, body)) = cur.take() {
                // drop the BLOCK header pairs (before the first entity)
                let first = body.iter().position(|(cc, _)| *cc == 0).unwrap_or(body.len());
                blocks.insert(name, (base, entities(&body[first..])));
            }
        } else if let Some((_, _, body)) = cur.as_mut() {
            body.push((*c, v.clone()));
        }
    }
    let t = DMat4::from_translation(at) * DMat4::from_scale(DVec3::splat(scale));
    let mut shapes_out = vec![];
    let mut skipped = 0;
    for e in entities(&ents_raw) {
        shapes(&e, &blocks, t, "0", 0, &mut shapes_out, &mut skipped);
    }
    if shapes_out.is_empty() {
        bail!("nothing SketchCraft can draw in that DXF ({skipped} text, hatch or other items skipped)");
    }
    // where the drawing mostly is: percentiles of its points, so a sheet
    // border drawn hundreds of times larger doesn't decide the view
    let focus = {
        let (mut xs, mut ys, mut zs) = (vec![], vec![], vec![]);
        for (_, sh) in &shapes_out {
            let pts: &[DVec3] = match sh {
                Shape::Path(p) | Shape::Loop(p) | Shape::Face(p) => p,
                _ => &[],
            };
            for q in pts {
                xs.push(q.x);
                ys.push(q.y);
                zs.push(q.z);
            }
        }
        let pick = |v: &mut Vec<f64>| -> (f64, f64) {
            if v.is_empty() {
                return (0.0, 0.0);
            }
            v.sort_by(|a, b| a.total_cmp(b));
            let k = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
            (k(0.02), k(0.98))
        };
        let ((x0, x1), (y0, y1), (z0, z1)) = (pick(&mut xs), pick(&mut ys), pick(&mut zs));
        [DVec3::new(x0, y0, z0), DVec3::new(x1, y1, z1)]
    };
    let mut by_layer: std::collections::BTreeMap<String, Vec<Shape>> = Default::default();
    let (mut labels, mut dimensions) = (0, 0);
    for (l, s) in shapes_out {
        match s {
            Shape::Text(at, text) => {
                let id = m.new_id();
                m.annotations.push(crate::model::Annotation::Text { id, at, text, leader: None });
                labels += 1;
            }
            Shape::Dim(a, b, offset) => {
                let id = m.new_id();
                m.annotations.push(crate::model::Annotation::Dimension { id, a, b, offset });
                dimensions += 1;
            }
            s => by_layer.entry(l).or_default().push(s),
        }
    }
    let outer = m.begin_group(name.to_string());
    let (mut faces, mut edges) = (0, 0);
    let layers = by_layer.len();
    let mut bulks: Vec<Bulk> = vec![];
    for (layer, shapes) in by_layer {
        let g = m.begin_group(layer.clone());
        let mut b = Bulk::new(g);
        for s in shapes {
            match s {
                // 3D faces go in as they are
                Shape::Face(p) => b.face(m, &p),
                // outlines and lines: edges; faces are found among them below
                Shape::Loop(mut p) => {
                    if let Some(first) = p.first().copied() {
                        p.push(first);
                    }
                    b.path(m, &p);
                }
                Shape::Path(p) => b.path(m, &p),
                Shape::Text(..) | Shape::Dim(..) => {}
            }
        }
        faces += b.faces + b.find_faces(m);
        edges += b.edges.len();
        m.active_group = Some(outer);
        if layer != "0" {
            m.set_tag(&[g], Some(layer)).ok();
        }
        bulks.push(b);
    }
    edges += cut_across_layers(m, &mut bulks);
    m.active_group = m.groups.get(&outer).and_then(|g| g.parent);
    Ok(DxfResult { group: outer, layers, faces, edges, units: unit.into(), skipped, labels, dimensions, focus })
}

/// Layers are separate groups, so a sheet border on one layer would fill as a
/// solid face under the shapes drawn on others, and the two would fight for
/// the same plane. Like SketchUp, cut them out: a flat face from one layer
/// that lies inside a flat face of another, on the same level, becomes a hole
/// in it (outermost ones only; a shape inside that shape stays its business).
/// Returns how many edges the holes added.
fn cut_across_layers(m: &mut Model, bulks: &mut [Bulk]) -> usize {
    struct Flat {
        id: Id,
        layer: usize,
        z: f64,
        ring: Vec<glam::DVec2>,
        lo: glam::DVec2,
        hi: glam::DVec2,
        area: f64,
    }
    let mut flats: Vec<Flat> = vec![];
    for (li, b) in bulks.iter().enumerate() {
        for (id, f) in m.faces.iter() {
            if f.group != Some(b.group) || f.outer.len() < 3 {
                continue;
            }
            let pts: Vec<DVec3> = f.outer.iter().map(|v| m.p(*v)).collect();
            let z = pts[0].z;
            if pts.iter().any(|p| (p.z - z).abs() > 1e-6) {
                continue;
            }
            let ring: Vec<glam::DVec2> = pts.iter().map(|p| p.truncate()).collect();
            let lo = ring.iter().fold(glam::DVec2::splat(f64::MAX), |a, p| a.min(*p));
            let hi = ring.iter().fold(glam::DVec2::splat(f64::MIN), |a, p| a.max(*p));
            let area = ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum::<f64>().abs() / 2.0;
            flats.push(Flat { id: *id, layer: li, z, ring, lo, hi, area });
        }
    }
    if flats.len() < 2 || flats.len() > 20_000 {
        return 0;
    }
    let inside = |p: glam::DVec2, ring: &[glam::DVec2]| -> bool {
        let mut c = false;
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                c = !c;
            }
        }
        c
    };
    // `inner` lies within `outer` (every corner inside or on it, and smaller)
    let within = |inner: &Flat, outer: &Flat| -> bool {
        (inner.z - outer.z).abs() < 1e-6
            && inner.area < outer.area
            && inner.lo.cmpge(outer.lo - 1e-9).all()
            && inner.hi.cmple(outer.hi + 1e-9).all()
            && {
                // test a point just inside each corner's neighborhood: the centroid of each ear
                let n = inner.ring.len();
                (0..n).all(|i| {
                    let c = (inner.ring[i] * 0.98) + (inner.ring[(i + 1) % n] * 0.01) + (inner.ring[(i + n - 1) % n] * 0.01);
                    inside(c, &outer.ring)
                })
            }
    };
    let mut added = 0;
    for oi in 0..flats.len() {
        let outer = &flats[oi];
        // faces from other layers inside this one
        let cands: Vec<usize> = (0..flats.len()).filter(|&gi| flats[gi].layer != outer.layer && within(&flats[gi], outer)).collect();
        // only the outermost of them, and not ones already inside a hole of this face
        let holes_now: Vec<Vec<glam::DVec2>> = m.faces[&outer.id].holes.iter().map(|h| h.iter().map(|v| m.p(*v).truncate()).collect()).collect();
        let top: Vec<usize> = cands
            .iter()
            .copied()
            .filter(|&gi| !cands.iter().any(|&hi| hi != gi && within(&flats[gi], &flats[hi])))
            .filter(|&gi| !holes_now.iter().any(|h| inside(flats[gi].ring[0], h)))
            .collect();
        if top.is_empty() {
            continue;
        }
        let normal = crate::geom::newell(&m.faces[&outer.id].outer.iter().map(|v| m.p(*v)).collect::<Vec<_>>());
        let b = &mut bulks[outer.layer];
        let z = outer.z;
        let mut new_holes = vec![];
        for gi in top {
            let mut pts: Vec<DVec3> = flats[gi].ring.iter().map(|p| p.extend(z)).collect();
            // holes run clockwise about the face normal
            if crate::geom::newell(&pts).dot(normal) > 0.0 {
                pts.reverse();
            }
            let ids: Vec<Id> = pts.iter().map(|p| b.vertex(m, *p)).collect();
            for i in 0..ids.len() {
                if b.edge(m, ids[i], ids[(i + 1) % ids.len()]) {
                    added += 1;
                }
            }
            new_holes.push(ids);
        }
        if let Some(f) = m.faces.get_mut(&outer.id) {
            f.holes.extend(new_holes);
        }
    }
    added
}

/// Building a big drawing fast: points welded through a hash (not a search
/// of the whole model), edges made once, and faces found in one pass over
/// each flat level instead of line by line.
struct Bulk {
    group: Id,
    at: HashMap<(i64, i64, i64), Id>,
    edges: std::collections::HashSet<(Id, Id)>,
    /// edges from outlines and lines (faces are looked for among these)
    loose: Vec<(Id, Id)>,
    faces: usize,
}

const Q: f64 = 1e-6;

impl Bulk {
    fn new(group: Id) -> Bulk {
        Bulk { group, at: HashMap::new(), edges: Default::default(), loose: vec![], faces: 0 }
    }

    fn vertex(&mut self, m: &mut Model, p: DVec3) -> Id {
        let k = ((p.x / Q).round() as i64, (p.y / Q).round() as i64, (p.z / Q).round() as i64);
        if let Some(&v) = self.at.get(&k) {
            return v;
        }
        let id = m.new_id();
        m.vertices.insert(id, crate::model::Vertex { p, group: Some(self.group) });
        self.at.insert(k, id);
        id
    }

    fn edge(&mut self, m: &mut Model, a: Id, b: Id) -> bool {
        if a == b || !self.edges.insert((a.min(b), a.max(b))) {
            return false;
        }
        let id = m.new_id();
        m.edges.insert(id, crate::model::Edge { a, b, group: Some(self.group), ..Default::default() });
        true
    }

    fn path(&mut self, m: &mut Model, p: &[DVec3]) {
        let ids: Vec<Id> = p.iter().map(|q| self.vertex(m, *q)).collect();
        for w in ids.windows(2) {
            if self.edge(m, w[0], w[1]) {
                self.loose.push((w[0], w[1]));
            }
        }
    }

    fn face(&mut self, m: &mut Model, p: &[DVec3]) {
        let mut ids: Vec<Id> = p.iter().map(|q| self.vertex(m, *q)).collect();
        ids.dedup();
        if ids.len() > 1 && ids.first() == ids.last() {
            ids.pop();
        }
        let pts: Vec<DVec3> = ids.iter().map(|v| m.p(*v)).collect();
        let n = crate::geom::newell(&pts);
        let flat = n != DVec3::ZERO && pts.iter().all(|q| (*q - pts[0]).dot(n).abs() < 1e-6);
        for i in 0..ids.len() {
            self.edge(m, ids[i], ids[(i + 1) % ids.len()]);
        }
        if ids.len() >= 3 && flat {
            let id = m.new_id();
            m.faces.insert(id, crate::model::Face { outer: ids, group: Some(self.group), ..Default::default() });
            self.faces += 1;
        }
    }

    /// Faces among the loose edges, one flat level at a time: walk the plan
    /// graph keeping the face on the left; islands become holes.
    fn find_faces(&mut self, m: &mut Model) -> usize {
        let mut levels: HashMap<i64, Vec<(Id, Id)>> = HashMap::new();
        for &(a, b) in &self.loose {
            let (pa, pb) = (m.p(a), m.p(b));
            if (pa.z - pb.z).abs() < 1e-6 {
                levels.entry((pa.z / 1e-5).round() as i64).or_default().push((a, b));
            }
        }
        let mut made = 0;
        for (_, es) in levels {
            made += faces_in_plan(m, &es, self.group);
        }
        made
    }
}

fn faces_in_plan(m: &mut Model, es: &[(Id, Id)], group: Id) -> usize {
    // neighbours, without dangling ends (they can't bound a face)
    let mut nb: HashMap<Id, Vec<Id>> = HashMap::new();
    for &(a, b) in es {
        nb.entry(a).or_default().push(b);
        nb.entry(b).or_default().push(a);
    }
    loop {
        let ends: Vec<Id> = nb.iter().filter(|(_, v)| v.len() < 2).map(|(k, _)| *k).collect();
        if ends.is_empty() {
            break;
        }
        for v in ends {
            if let Some(ns) = nb.remove(&v) {
                for n in ns {
                    if let Some(l) = nb.get_mut(&n) {
                        l.retain(|x| *x != v);
                    }
                }
            }
        }
    }
    if nb.is_empty() {
        return 0;
    }
    let xy = |v: Id| m.p(v).truncate();
    // neighbours in counter-clockwise order
    for (v, ns) in nb.iter_mut() {
        let c = xy(*v);
        ns.sort_by(|a, b| {
            let (da, db) = (xy(*a) - c, xy(*b) - c);
            da.y.atan2(da.x).partial_cmp(&db.y.atan2(db.x)).unwrap()
        });
    }
    let mut used: std::collections::HashSet<(Id, Id)> = Default::default();
    let mut pos: Vec<(Vec<Id>, f64)> = vec![];
    let mut neg: Vec<Vec<Id>> = vec![];
    let starts: Vec<(Id, Id)> = nb.iter().flat_map(|(v, ns)| ns.iter().map(move |n| (*v, *n))).collect();
    for (a0, b0) in starts {
        if used.contains(&(a0, b0)) {
            continue;
        }
        let mut cyc = vec![a0];
        let (mut u, mut v) = (a0, b0);
        let mut ok = true;
        loop {
            used.insert((u, v));
            if v == a0 {
                break;
            }
            cyc.push(v);
            if cyc.len() > 100_000 {
                ok = false;
                break;
            }
            // the next edge clockwise from the way back: the face stays on the left
            let ns = &nb[&v];
            let i = ns.iter().position(|x| *x == u).unwrap();
            let w = ns[(i + ns.len() - 1) % ns.len()];
            u = v;
            v = w;
        }
        if !ok || cyc.len() < 3 {
            continue;
        }
        let pts: Vec<glam::DVec2> = cyc.iter().map(|v| xy(*v)).collect();
        let area = (0..pts.len()).map(|i| pts[i].perp_dot(pts[(i + 1) % pts.len()])).sum::<f64>() / 2.0;
        if area > 1e-12 {
            pos.push((cyc, area));
        } else if area < -1e-12 {
            neg.push(cyc);
        }
    }
    // each island's outline (a clockwise cycle) is a hole in the smallest face around it
    let mut holes: HashMap<usize, Vec<Vec<Id>>> = HashMap::new();
    for h in neg {
        let p = xy(h[0]);
        // a point just inside the island's outline isn't needed: any outline point is
        // outside every face of its own island, so only faces around it contain it
        let around = pos
            .iter()
            .enumerate()
            .filter(|(_, (c, _))| !c.contains(&h[0]) && crate::geom::inside(p, &c.iter().map(|v| xy(*v)).collect::<Vec<_>>()))
            .min_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap());
        if let Some((i, _)) = around {
            holes.entry(i).or_default().push(h);
        }
    }
    let n = pos.len();
    for (i, (outer, _)) in pos.into_iter().enumerate() {
        let id = m.new_id();
        m.faces.insert(id, crate::model::Face { outer, holes: holes.remove(&i).unwrap_or_default(), group: Some(group), ..Default::default() });
    }
    n
}
