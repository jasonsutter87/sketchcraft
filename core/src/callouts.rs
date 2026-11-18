//! Drawing callouts worked out from the model: door and window tags (and
//! their schedule), automatic exterior dimension strings, and the shapes
//! blueprints mark things with (tags, elevation markers, north arrow, level
//! datums). The shapes draw into the display list (`draft::ir::Cv`): sizes and positions are in
//! PDF points, y up, as they always were, and `Cv` stores paper millimeters.

use crate::model::{Annotation, Model};
use crate::draft::ir::{Cv, Fill, Pens, PT};
use glam::DVec3;
use std::collections::BTreeMap;

/// A door or window placed in the model.
#[derive(Clone, Debug)]
pub struct Opening {
    /// 'D' or 'W'
    pub kind: char,
    /// D1, W2...: one tag per size of door or window
    pub tag: String,
    pub name: String,
    pub lo: DVec3,
    pub hi: DVec3,
    /// across the opening (along its wall) and how tall
    pub width: f64,
    pub height: f64,
    /// its wall runs along x (else along y)
    pub along_x: bool,
}

impl Opening {
    pub fn center(&self) -> DVec3 {
        (self.lo + self.hi) / 2.0
    }
}

/// Doors and windows: component instances named or classed as such. Each
/// size gets its own tag, doors first (D1, D2...), then windows (W1...).
pub fn openings(m: &Model) -> Vec<Opening> {
    let mut boxes: BTreeMap<u32, (DVec3, DVec3)> = BTreeMap::new();
    for f in m.world_faces() {
        if !m.instances.contains_key(&f.owner) {
            continue;
        }
        let b = boxes.entry(f.owner).or_insert((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)));
        for p in &f.outer {
            b.0 = b.0.min(*p);
            b.1 = b.1.max(*p);
        }
    }
    let mut out = Vec::new();
    for (id, (lo, hi)) in boxes {
        let inst = &m.instances[&id];
        let def = m.definitions.get(&inst.def).map(|d| d.name.as_str()).unwrap_or("");
        let class = m.classes.get(&id).map(|s| s.as_str()).unwrap_or("");
        let name = if inst.name.is_empty() { def } else { inst.name.as_str() };
        let low = format!("{} {}", name, def).to_ascii_lowercase();
        let kind = if class == "IfcDoor" || low.contains("door") {
            'D'
        } else if class == "IfcWindow" || low.contains("window") {
            'W'
        } else {
            continue;
        };
        let size = hi - lo;
        let along_x = size.x >= size.y;
        out.push(Opening { kind, tag: String::new(), name: def.to_string(), lo, hi, width: if along_x { size.x } else { size.y }, height: size.z, along_x });
    }
    // tags by type: same kind and size (to the centimeter) share a tag
    let key = |o: &Opening| (o.kind, (o.width * 100.0).round() as i64, (o.height * 100.0).round() as i64);
    let mut types: Vec<(char, i64, i64)> = out.iter().map(key).collect();
    types.sort();
    types.dedup();
    let mut n: BTreeMap<char, usize> = BTreeMap::new();
    let mut tag_of = BTreeMap::new();
    for t in types {
        let k = n.entry(t.0).or_insert(0);
        *k += 1;
        tag_of.insert(t, format!("{}{}", t.0, k));
    }
    for o in &mut out {
        o.tag = tag_of[&key(o)].clone();
    }
    out
}

/// The door & window schedule: header row, then one row per tag.
pub fn schedule(ops: &[Opening], units: &str, floors: &[f64]) -> Vec<Vec<String>> {
    let mut rows = vec![vec!["TAG".to_string(), "TYPE".into(), "WIDTH".into(), "HEIGHT".into(), "COUNT".into(), "FLOORS".into()]];
    let mut by: BTreeMap<String, Vec<&Opening>> = BTreeMap::new();
    for o in ops {
        by.entry(o.tag.clone()).or_default().push(o);
    }
    // D before W, then by number
    let mut tags: Vec<_> = by.keys().cloned().collect();
    tags.sort_by_key(|t| (t.chars().next(), t[1..].parse::<u32>().unwrap_or(0)));
    for t in tags {
        let list = &by[&t];
        let o = list[0];
        let mut fl: Vec<usize> = list.iter().map(|o| floor_of(floors, o.lo.z)).collect();
        fl.sort();
        fl.dedup();
        let kind = if o.name.is_empty() { if o.kind == 'D' { "Door" } else { "Window" }.to_string() } else { o.name.clone() };
        rows.push(vec![
            t.clone(),
            kind,
            crate::units::format(o.width, units),
            crate::units::format(o.height, units),
            list.len().to_string(),
            fl.iter().map(|i| floor_name(*i)).collect::<Vec<_>>().join(", "),
        ]);
    }
    rows
}

/// Which floor a height is on (the highest floor at or below it).
pub fn floor_of(floors: &[f64], z: f64) -> usize {
    floors.iter().rposition(|f| z >= f - 0.2).unwrap_or(0)
}

pub fn floor_name(i: usize) -> String {
    match i {
        0 => "Main".into(),
        1 => "Upper".into(),
        k => format!("Floor {}", k + 1),
    }
}

/// Openings standing on this floor (doors at it, windows above it).
pub fn on_floor<'a>(ops: &'a [Opening], floor: f64) -> impl Iterator<Item = &'a Opening> {
    ops.iter().filter(move |o| o.lo.z >= floor - 0.2 && o.lo.z < floor + 2.0)
}

/// A side of the building on a plan.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    South,
    North,
    West,
    East,
}

impl Side {
    pub fn out(self) -> DVec3 {
        match self {
            Side::South => DVec3::NEG_Y,
            Side::North => DVec3::Y,
            Side::West => DVec3::NEG_X,
            Side::East => DVec3::X,
        }
    }
}

/// What one automatic dimension string measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainKind {
    /// across the face, through both sides of every door and window in it
    Openings,
    /// the overall width (south) or depth (east)
    Overall,
}

/// One automatic dimension string: its stops (world points on the face, in order along it), which side of the
/// building it is on, and how far out it goes (`offset`, a world vector out from the face).
#[derive(Clone, Debug)]
pub struct AutoChain {
    pub side: Side,
    pub kind: ChainKind,
    pub stops: Vec<DVec3>,
    pub offset: DVec3,
}

/// Automatic dimension strings around a floor: a chain through every
/// opening on each side, and (if the floor has no hand-placed dimensions)
/// the overall width and depth. `outline` is the floor's outside box
/// (x0, y0, x1, y1). Returns (a, b, offset) like dimension annotations,
/// placed beyond any hand-placed ones.
pub fn auto_dims(m: &Model, ops: &[Opening], floor: f64, outline: (f64, f64, f64, f64)) -> Vec<(DVec3, DVec3, DVec3)> {
    let mut out = Vec::new();
    let (chains, any_manual) = auto_chains(m, ops, floor, outline);
    for c in chains {
        match c.kind {
            ChainKind::Openings => out.extend(c.stops.windows(2).map(|w| (w[0], w[1], c.offset))),
            // the overall ones only when nothing on the floor is dimensioned by hand
            ChainKind::Overall if !any_manual => out.push((c.stops[0], c.stops[c.stops.len() - 1], c.offset)),
            ChainKind::Overall => {}
        }
    }
    out
}

/// The dimension strings `auto_dims` draws, as whole chains (the openings chain of each side that has openings, the
/// overall width on the south and depth on the east), and whether the floor has hand-placed dimensions (then
/// `auto_dims` leaves the overall ones out). `dimension_auto` makes saved, attached dimensions from these.
pub fn auto_chains(m: &Model, ops: &[Opening], floor: f64, outline: (f64, f64, f64, f64)) -> (Vec<AutoChain>, bool) {
    let (x0, y0, x1, y1) = outline;
    let z = floor + 0.05;
    // how far out the hand-placed dimensions already reach, per side
    let mut manual = [0.0f64; 4];
    let mut any_manual = false;
    for a in &m.annotations {
        if let Annotation::Dimension { a, b, offset, .. } = a {
            if a.z < floor - 0.05 || a.z > floor + 1.0 {
                continue;
            }
            any_manual = true;
            let horizontal = (a.y - b.y).abs() < 1e-3;
            let side = if horizontal { if offset.y > 0.0 { 1 } else { 0 } } else if offset.x > 0.0 { 3 } else { 2 };
            let reach = if horizontal { if side == 1 { a.y + offset.y - y1 } else { y0 - (a.y + offset.y) } } else if side == 3 { a.x + offset.x - x1 } else { x0 - (a.x + offset.x) };
            manual[side] = manual[side].max(reach);
        }
    }
    let mut out = Vec::new();
    for (si, side) in [Side::South, Side::North, Side::West, Side::East].into_iter().enumerate() {
        let horizontal = matches!(side, Side::South | Side::North);
        let face = match side {
            Side::South => y0,
            Side::North => y1,
            Side::West => x0,
            Side::East => x1,
        };
        // the openings in this face
        let mut stops = vec![if horizontal { x0 } else { y0 }, if horizontal { x1 } else { y1 }];
        let mut found = false;
        for o in on_floor(ops, floor) {
            let c = o.center();
            if o.along_x != horizontal || (if horizontal { c.y } else { c.x } - face).abs() > 0.6 {
                continue;
            }
            found = true;
            let mid = if horizontal { c.x } else { c.y };
            stops.push(mid - o.width / 2.0);
            stops.push(mid + o.width / 2.0);
        }
        stops.sort_by(|a, b| a.total_cmp(b));
        stops.dedup_by(|a, b| (*a - *b).abs() < 0.02);
        // beyond the tags (0.5 out) and any hand-placed dimensions
        let base = if manual[si] > 0.0 { manual[si] + 0.6 } else { 1.2 };
        let n = side.out();
        let at = |s: f64| if horizontal { DVec3::new(s, face, z) } else { DVec3::new(face, s, z) };
        if found {
            out.push(AutoChain { side, kind: ChainKind::Openings, stops: stops.iter().map(|s| at(*s)).collect(), offset: n * base });
        }
        // overall width (south) and depth (east)
        if matches!(side, Side::South | Side::East) {
            let last = stops.len() - 1;
            out.push(AutoChain { side, kind: ChainKind::Overall, stops: vec![at(stops[0]), at(stops[last])], offset: n * (base + if found { 0.6 } else { 0.0 }) });
        }
    }
    (out, any_manual)
}

/// The outside box of the main building on a floor plan cut.
pub fn outline(m: &Model, cut_z: f64) -> Option<(f64, f64, f64, f64)> {
    let c = m.section(DVec3::new(0.0, 0.0, cut_z), DVec3::new(0.0, 0.0, -1.0)).ok()?;
    // every wall is its own piece of the cut: the building is all of them
    if c.regions.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in c.regions.iter().flat_map(|r| r.outer.iter()) {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Some((x0, y0, x1, y1))
}

// ---- shapes (points, y up; drawn into the display list) ------------------------------

/// Width of `s` in points at `size` (the drawing typeface's real metrics).
pub fn text_w(s: &str, size: f64) -> f64 {
    crate::draft::font::width(s, size, false)
}

/// A door tag (circle) or window tag (hexagon) with its label.
pub fn tag(cv: &mut Cv, pens: &Pens, x: f64, y: f64, label: &str, kind: char) {
    let r = 6.2;
    let pen = pens.ink(pens.medium);
    if kind == 'D' {
        cv.circle(x, y, r, Some(&pen), Some(pens.paper));
    } else {
        let pts: Vec<(f64, f64)> = (0..6)
            .map(|i| {
                let a = std::f64::consts::PI / 3.0 * i as f64;
                (x + r * 1.1 * a.cos(), y + r * 1.1 * a.sin())
            })
            .collect();
        cv.path(&pts, true, Some(&pen), Some(Fill { rgb: pens.paper, even_odd: false }));
    }
    cv.text_c(x, y, label, 5.5, true, pens.ink);
}

/// An elevation marker: a circle split in two (drawing number over sheet
/// number) with a solid pointer toward what it looks at (`dir`, unit, in
/// paper space with y up).
pub fn elevation_marker(cv: &mut Cv, pens: &Pens, x: f64, y: f64, dir: (f64, f64), num: &str, sheet: &str) {
    let r = 9.0;
    let (dx, dy) = dir;
    let (px, py) = (-dy, dx);
    // the pointer: a triangle from the circle's sides to a tip beyond it
    let tip = (x + dx * r * 1.75, y + dy * r * 1.75);
    let fill = Fill { rgb: pens.ink, even_odd: false };
    cv.path(&[(x + px * r * 0.95, y + py * r * 0.95), tip, (x - px * r * 0.95, y - py * r * 0.95)], true, None, Some(fill));
    cv.circle(x, y, r, Some(&pens.ink(pens.medium)), Some(pens.paper));
    cv.line((x - r, y), (x + r, y), &pens.ink(pens.thin));
    cv.text_c(x, y + r * 0.42, num, 6.5, true, pens.ink);
    cv.text_c(x, y - r * 0.45, sheet, 5.5, false, pens.ink);
}

/// A north arrow (plan +Y is north).
pub fn north_arrow(cv: &mut Cv, pens: &Pens, x: f64, y: f64) {
    let r = 10.0;
    cv.circle(x, y, r, Some(&pens.ink(pens.medium)), None);
    // half-filled arrow
    let fill = Fill { rgb: pens.ink, even_odd: false };
    cv.path(&[(x, y + r * 0.85), (x + r * 0.42, y - r * 0.6), (x, y - r * 0.25)], true, None, Some(fill));
    cv.path(&[(x, y + r * 0.85), (x - r * 0.42, y - r * 0.6), (x, y - r * 0.25)], true, Some(&pens.ink(pens.medium)), None);
    cv.text_c(x, y + r + 6.0, "N", 7.0, true, pens.ink);
}

/// A level datum on an elevation: a dashed line across, and at its right
/// end a quartered circle with the level's name and height.
pub fn level(cv: &mut Cv, pens: &Pens, x0: f64, x1: f64, y: f64, name: &str, height: &str) {
    cv.line((x0, y), (x1, y), &pens.ink(pens.thin).dashed(&[3.0 / PT, 2.0 / PT]));
    let (cx, r) = (x1 + 5.0, 3.6);
    cv.circle(cx, y, r, Some(&pens.ink(pens.thin)), None);
    // two opposite quarters filled
    let fill = Fill { rgb: pens.ink, even_odd: false };
    cv.path(&[(cx, y), (cx + r, y), (cx, y + r)], true, None, Some(fill));
    cv.path(&[(cx, y), (cx - r, y), (cx, y - r)], true, None, Some(fill));
    cv.text(cx + r + 3.0, y + 1.6, name, 5.5, true, pens.ink, 0.0);
    cv.text(cx + r + 3.0, y - 5.2, height, 5.5, false, pens.ink, 0.0);
}

/// The elevation key: a circle with four pointers, each the way one
/// elevation looks (front looks north at the south face, right side looks
/// west, rear south, left side east), numbered with its drawing; the sheet
/// in the middle. `labels` are "drawing/sheet" for front, right, rear, left.
pub fn elevation_key(cv: &mut Cv, pens: &Pens, x: f64, y: f64, labels: &[String]) {
    let r = 11.0;
    let dirs = [(0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)];
    let fill = Fill { rgb: pens.ink, even_odd: false };
    for (dx, dy) in dirs {
        let (px, py) = (-dy, dx);
        let tip = (x + dx * r * 1.6, y + dy * r * 1.6);
        let (bx, by) = (x + dx * r * 0.6, y + dy * r * 0.6);
        cv.path(&[(bx + px * r * 0.55, by + py * r * 0.55), tip, (bx - px * r * 0.55, by - py * r * 0.55)], true, None, Some(fill));
    }
    cv.circle(x, y, r, Some(&pens.ink(pens.medium)), Some(pens.paper));
    for ((dx, dy), l) in dirs.iter().zip(labels) {
        let num = l.split('/').next().unwrap_or(l);
        cv.text_c(x + dx * r * 0.58, y + dy * r * 0.58, num, 6.0, true, pens.ink);
    }
    let sheet = labels.first().and_then(|l| l.split('/').nth(1)).unwrap_or("");
    cv.circle(x, y, 3.6, Some(&pens.ink(pens.thin)), None);
    cv.text_c(x, y, sheet, 4.5, false, pens.ink);
    cv.text_c(x, y - r * 1.6 - 9.0, "ELEVATIONS", 5.0, true, pens.ink);
    cv.text_c(x, y - r * 1.6 - 15.0, &format!("on sheet {sheet}"), 4.5, false, pens.ink);
}
