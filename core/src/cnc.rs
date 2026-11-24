//! CNC files: one DXF per part (docs/dev/woodworker.md, "CNC and joinery").
//!
//! Each part is drawn flat in its own frame, the one `plugins::measure` and `parts_layout` use: X along the part's
//! length, Y across its width, seen from the side that faces up when the part lies on the table (the face `parts_layout`
//! puts down is the underside). The part's lower left corner is the origin, millimeters (`$INSUNITS` 4).
//!
//! Layers: `OUTLINE` (the part's edge), `INSIDE` (holes that go all the way through), `POCKET_<depth>mm` (floors cut
//! from the top, such as dados and rabbets: the loop is the floor's edge) and `POCKET_BOTTOM_<depth>mm` (cut from the
//! underside). With `dogbones` every inside corner gets a relief arc of the tool's radius centered on the corner, so a
//! round cutter can reach it: the corners of pockets and holes, and the notches in an outline.
//!
//! The drafting `Seg` type has lines and beziers only, so these files are written here, with `ARC` entities.

use crate::model::{Id, Model};
use crate::parts_layout::sort_parts;
use crate::plugins::{measure, Measured};
use anyhow::{bail, Result};
use glam::DVec3;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Distances closer than this (meters) are the same: 0.01 mm.
const TOL: f64 = 1e-5;
/// Relief is cut only where the cutter's wedge is sharper than this (a right angle is 90 degrees).
const MAX_CORNER: f64 = 2.4;
pub const MAX_TOOL: f64 = 0.1;

pub struct CncFile {
    pub number: usize,
    pub id: Id,
    pub name: String,
    pub file: String,
    pub size: [f64; 3],
    pub holes: usize,
    pub pockets: usize,
    pub reliefs: usize,
    pub skipped_corners: usize,
    pub bytes: Vec<u8>,
}

impl CncFile {
    pub fn info(&self) -> Value {
        json!({"number": self.number, "id": self.id, "name": self.name, "file": self.file, "size": self.size, "holes": self.holes, "pockets": self.pockets,
               "reliefs": self.reliefs, "skipped_corners": self.skipped_corners})
    }
}

type P2 = [f64; 2];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Outline,
    Through,
    Pocket,
}

struct Loop {
    kind: Kind,
    layer: String,
    pts: Vec<P2>,
}

fn area(p: &[P2]) -> f64 {
    (0..p.len()).map(|i| p[i][0] * p[(i + 1) % p.len()][1] - p[(i + 1) % p.len()][0] * p[i][1]).sum::<f64>() / 2.0
}

fn centroid(p: &[P2]) -> P2 {
    let n = p.len() as f64;
    [p.iter().map(|q| q[0]).sum::<f64>() / n, p.iter().map(|q| q[1]).sum::<f64>() / n]
}

/// What names a loop for matching: its area and middle, to 0.01 mm.
fn key(p: &[P2]) -> (i64, i64, i64) {
    let c = centroid(p);
    ((area(p).abs() * 1e8).round() as i64, (c[0] * 1e5).round() as i64, (c[1] * 1e5).round() as i64)
}

/// Drop repeated points and points in a straight run.
fn clean(p: Vec<P2>) -> Vec<P2> {
    let mut v: Vec<P2> = Vec::new();
    for q in p {
        if v.last().is_none_or(|l| (l[0] - q[0]).hypot(l[1] - q[1]) > TOL) {
            v.push(q);
        }
    }
    while v.len() > 1 && (v[0][0] - v[v.len() - 1][0]).hypot(v[0][1] - v[v.len() - 1][1]) <= TOL {
        v.pop();
    }
    loop {
        let n = v.len();
        if n < 4 {
            return v;
        }
        let k = (0..n).find(|&i| {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let (u, w) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
            (u[0] * w[1] - u[1] * w[0]).abs() <= TOL * 0.1 * u[0].hypot(u[1]).max(w[0].hypot(w[1])) && u[0] * w[0] + u[1] * w[1] > 0.0
        });
        match k {
            Some(i) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

/// `number` is 1-based, padded so the files sort in order.
fn file_name(number: usize, count: usize, name: &str) -> String {
    let digits = count.to_string().len().max(2);
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(60).collect();
    format!("{number:0digits$}-{}.dxf", if slug.is_empty() { "part" } else { &slug })
}

fn mm(v: f64) -> String {
    let s = format!("{:.4}", v * 1000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

fn depth_layer(prefix: &str, d: f64) -> String {
    format!("{prefix}{:.1}mm", d * 1000.0)
}

/// The world faces of each part, found by sorting the faces into their groups once (the work grows with the model, not
/// with the parts times the model). Each list is in the order the model lists the faces.
fn faces_by_part(m: &Model, world: &[crate::components::WFace], parts: &[Measured]) -> Vec<Vec<usize>> {
    use std::collections::HashMap;
    let mut kids: HashMap<Id, Vec<Id>> = HashMap::new();
    for (&id, g) in &m.groups {
        if let Some(p) = g.parent {
            kids.entry(p).or_default().push(id);
        }
    }
    let mut by_group: HashMap<Id, Vec<usize>> = HashMap::new();
    let mut by_owner: HashMap<Id, Vec<usize>> = HashMap::new();
    for (i, f) in world.iter().enumerate() {
        by_owner.entry(f.owner).or_default().push(i);
        if let Some(g) = f.face.and_then(|fid| m.faces.get(&fid)).and_then(|x| x.group) {
            by_group.entry(g).or_default().push(i);
        }
    }
    parts
        .iter()
        .map(|p| {
            if p.kind == "group" {
                let mut seen: std::collections::HashSet<Id> = std::collections::HashSet::from([p.id]);
                let mut stack = vec![p.id];
                let mut idx: Vec<usize> = Vec::new();
                while let Some(x) = stack.pop() {
                    idx.extend(by_group.get(&x).into_iter().flatten().copied());
                    for &k in kids.get(&x).into_iter().flatten() {
                        if seen.insert(k) {
                            stack.push(k);
                        }
                    }
                }
                idx.sort_unstable();
                idx
            } else {
                by_owner.get(&p.id).cloned().unwrap_or_default()
            }
        })
        .collect()
}

/// The loops of one part, in its flat frame. `mine` are the indexes of the part's faces in `world`.
fn loops_of(world: &[crate::components::WFace], mine: &[usize], p: &Measured) -> Vec<Loop> {
    let t = p.extent.z;
    // the faces square to the board's thickness, in the part's own frame: x along u, y along w, z along n
    let local = |q: DVec3| -> DVec3 {
        let d = q - p.origin;
        DVec3::new(d.dot(p.u), d.dot(p.w), d.dot(p.n))
    };
    let faces: Vec<_> = mine.iter().map(|&i| &world[i]).filter(|f| f.normal.normalize_or_zero().dot(p.n).abs() >= 1.0 - 1e-6).collect();
    // seen from the up side (the side opposite `n`, the way the part lies in the lay-out). When every floor was cut from
    // the other side, turn the part over so they are cut from the top: one setup on the router.
    let floors = |down: bool| faces.iter().any(|f| f.outer.first().is_some_and(|q| { let z = local(*q).z; z > TOL && z < t - TOL }) && (f.normal.normalize_or_zero().dot(p.n) > 0.0) == down);
    let flip = floors(true) && !floors(false);
    let flat = |q: DVec3| -> (P2, f64) {
        let d = local(q);
        if flip { ([p.extent.x - d.x, p.extent.y - d.y], t - d.z) } else { ([d.x, p.extent.y - d.y], d.z) }
    };
    type Flat = (Vec<P2>, Vec<Vec<P2>>, f64, f64);
    let mut flats: Vec<Flat> = Vec::new(); // outer, holes, z, normal along n
    for f in faces {
        let along = f.normal.normalize_or_zero().dot(p.n) * if flip { -1.0 } else { 1.0 };
        let z = f.outer.first().map(|q| flat(*q).1).unwrap_or(0.0);
        flats.push((f.outer.iter().map(|q| flat(*q).0).collect(), f.holes.iter().map(|h| h.iter().map(|q| flat(*q).0).collect()).collect(), z, along));
    }
    let mut out = Vec::new();
    // the silhouette: the biggest flat loop on either side
    let outline = flats.iter().filter(|f| f.2 < TOL || f.2 > t - TOL).map(|f| &f.0).max_by(|a, b| area(a).abs().total_cmp(&area(b).abs()));
    if let Some(o) = outline {
        out.push(Loop { kind: Kind::Outline, layer: "OUTLINE".into(), pts: clean(o.clone()) });
    }
    // pockets: the floors in between. A floor facing the up side was cut from it.
    let mut seen_floors = Vec::new();
    for (o, _, z, along) in &flats {
        if *z > TOL && *z < t - TOL {
            let layer = if *along < 0.0 { depth_layer("POCKET_", *z) } else { depth_layer("POCKET_BOTTOM_", t - *z) };
            let pts = clean(o.clone());
            let k = (key(&pts), layer.clone());
            if pts.len() >= 3 && !seen_floors.contains(&k) {
                seen_floors.push(k);
                out.push(Loop { kind: Kind::Pocket, layer, pts });
            }
        }
    }
    let opening: Vec<_> = out.iter().filter(|l| l.kind == Kind::Pocket).map(|l| key(&l.pts)).collect();
    // holes in the top and bottom faces that no pocket floor matches go all the way through
    let mut seen = Vec::new();
    for (_, holes, ..) in flats.iter().filter(|f| f.2 < TOL || f.2 > t - TOL) {
        for h in holes {
            let pts = clean(h.clone());
            let k = key(&pts);
            if pts.len() >= 3 && !opening.contains(&k) && !seen.contains(&k) {
                seen.push(k);
                out.push(Loop { kind: Kind::Through, layer: "INSIDE".into(), pts });
            }
        }
    }
    out
}

/// A relief at one corner of a loop.
struct Relief {
    at: usize,
    /// ARC start and end angle, degrees (counter-clockwise)
    arc: (f64, f64),
}

/// Is `p` on the edge of `poly`?
fn on_edge(poly: &[P2], p: P2) -> bool {
    (0..poly.len()).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (d, e) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
        let len2 = d[0] * d[0] + d[1] * d[1];
        let t = if len2 > 0.0 { ((e[0] * d[0] + e[1] * d[1]) / len2).clamp(0.0, 1.0) } else { 0.0 };
        (e[0] - d[0] * t).hypot(e[1] - d[1] * t) <= TOL
    })
}

/// `outline` is the part's edge: a pocket corner on it opens to the outside, where the cutter can run off.
fn reliefs_for(l: &Loop, outline: &[P2], r: f64, skipped: &mut usize) -> Vec<Relief> {
    let p = &l.pts;
    let n = p.len();
    let sign = if area(p) >= 0.0 { 1.0 } else { -1.0 };
    let mut cand: Vec<(usize, (f64, f64))> = Vec::new();
    for i in 0..n {
        let (prev, c, next) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let a = [prev[0] - c[0], prev[1] - c[1]];
        let b = [next[0] - c[0], next[1] - c[1]];
        let (la, lb) = (a[0].hypot(a[1]), b[0].hypot(b[1]));
        if la < TOL || lb < TOL {
            continue;
        }
        // counter-clockwise loop: a left turn at the corner is convex
        let convex = sign * ((c[0] - prev[0]) * (next[1] - c[1]) - (c[1] - prev[1]) * (next[0] - c[0])) > 0.0;
        // the cutter works inside a hole or pocket, outside an outline
        let inside_corner = if l.kind == Kind::Outline { !convex } else { convex };
        if !inside_corner || (l.kind == Kind::Pocket && on_edge(outline, c)) {
            continue;
        }
        let theta = ((a[0] * b[0] + a[1] * b[1]) / (la * lb)).clamp(-1.0, 1.0).acos();
        if theta > MAX_CORNER {
            continue;
        }
        // a relief needs room on both sides
        if la < r * 1.0001 || lb < r * 1.0001 {
            *skipped += 1;
            continue;
        }
        let (aa, ab) = (a[1].atan2(a[0]).to_degrees(), b[1].atan2(b[0]).to_degrees());
        // the arc goes the long way round, through the material
        let arc = if a[0] * b[1] - a[1] * b[0] > 0.0 { (ab, aa) } else { (aa, ab) };
        cand.push((i, arc));
    }
    // two reliefs along one edge need the edge to be at least a tool wide
    let marked: Vec<usize> = cand.iter().map(|c| c.0).collect();
    let mut out = Vec::new();
    for (i, arc) in cand {
        let (j, k) = ((i + 1) % n, (i + n - 1) % n);
        let short = |a: usize, b: usize| (p[a][0] - p[b][0]).hypot(p[a][1] - p[b][1]) < 2.0 * r * 0.9999;
        if (marked.contains(&j) && short(i, j)) || (marked.contains(&k) && short(i, k)) {
            *skipped += 1;
            continue;
        }
        out.push(Relief { at: i, arc });
    }
    out
}

struct Dxf {
    body: String,
    handle: u32,
    layers: BTreeMap<String, u8>,
    lo: P2,
    hi: P2,
}

impl Dxf {
    fn g(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.body, "{code}\n{v}\n");
    }
    fn start(&mut self, kind: &str, layer: &str, sub: &str) {
        self.handle += 1;
        self.g(0, kind);
        let h = self.handle;
        self.g(5, format!("{h:X}"));
        self.g(100, "AcDbEntity");
        self.g(8, layer);
        self.g(100, sub);
        let color = match layer {
            "OUTLINE" => 7,
            "INSIDE" => 1,
            _ if layer.starts_with("POCKET_BOTTOM") => 5,
            _ => 3,
        };
        self.layers.insert(layer.into(), color);
    }
    fn touch(&mut self, p: P2) {
        self.lo = [self.lo[0].min(p[0]), self.lo[1].min(p[1])];
        self.hi = [self.hi[0].max(p[0]), self.hi[1].max(p[1])];
    }
    fn line(&mut self, layer: &str, a: P2, b: P2) {
        self.start("LINE", layer, "AcDbLine");
        self.g(10, mm(a[0]));
        self.g(20, mm(a[1]));
        self.g(30, 0);
        self.g(11, mm(b[0]));
        self.g(21, mm(b[1]));
        self.g(31, 0);
    }
    fn arc(&mut self, layer: &str, c: P2, r: f64, a0: f64, a1: f64) {
        self.start("ARC", layer, "AcDbCircle");
        self.g(10, mm(c[0]));
        self.g(20, mm(c[1]));
        self.g(30, 0);
        self.g(40, mm(r));
        self.g(100, "AcDbArc");
        self.g(50, format!("{:.4}", a0.rem_euclid(360.0)));
        self.g(51, format!("{:.4}", a1.rem_euclid(360.0)));
    }
    fn poly(&mut self, layer: &str, pts: &[P2]) {
        self.start("LWPOLYLINE", layer, "AcDbPolyline");
        self.g(90, pts.len());
        self.g(70, 1);
        for p in pts {
            self.g(10, mm(p[0]));
            self.g(20, mm(p[1]));
        }
    }
}

fn dxf_text(part: &Measured, number: usize, loops: &[Loop], tool: f64, dogbones: bool, counts: &mut (usize, usize)) -> String {
    let mut d = Dxf { body: String::new(), handle: 0x100, layers: BTreeMap::new(), lo: [f64::MAX; 2], hi: [f64::MIN; 2] };
    let r = tool / 2.0;
    let outline: Vec<P2> = loops.iter().find(|l| l.kind == Kind::Outline).map(|l| l.pts.clone()).unwrap_or_default();
    for l in loops {
        for p in &l.pts {
            d.touch(*p);
        }
        let rel = if dogbones && r > 0.0 { reliefs_for(l, &outline, r, &mut counts.1) } else { Vec::new() };
        if rel.is_empty() {
            d.poly(&l.layer, &l.pts);
            continue;
        }
        counts.0 += rel.len();
        let n = l.pts.len();
        let at = |i: usize| rel.iter().any(|x| x.at == i);
        for i in 0..n {
            let (a, b) = (l.pts[i], l.pts[(i + 1) % n]);
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            let u = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
            let s = if at(i) { [a[0] + u[0] * r, a[1] + u[1] * r] } else { a };
            let e = if at((i + 1) % n) { [b[0] - u[0] * r, b[1] - u[1] * r] } else { b };
            if (e[0] - s[0]).hypot(e[1] - s[1]) > TOL {
                d.line(&l.layer, s, e);
            }
        }
        for x in &rel {
            d.arc(&l.layer, l.pts[x.at], r, x.arc.0, x.arc.1);
        }
    }
    let mut t = String::new();
    let mut g = |c: i32, v: &dyn std::fmt::Display| {
        let _ = write!(t, "{c}\n{v}\n");
    };
    let sz = part.size;
    let name: String = part.name.chars().filter(|c| !c.is_control()).collect();
    g(999, &format!("SketchCraft CNC part {number}: {name} ({} x {} x {} mm)", mm(sz[0]), mm(sz[1]), mm(sz[2])));
    g(0, &"SECTION");
    g(2, &"HEADER");
    g(9, &"$ACADVER");
    g(1, &"AC1015");
    g(9, &"$INSUNITS");
    g(70, &4);
    g(9, &"$EXTMIN");
    g(10, &mm(d.lo[0]));
    g(20, &mm(d.lo[1]));
    g(30, &0);
    g(9, &"$EXTMAX");
    g(10, &mm(d.hi[0]));
    g(20, &mm(d.hi[1]));
    g(30, &0);
    g(0, &"ENDSEC");
    g(0, &"SECTION");
    g(2, &"TABLES");
    g(0, &"TABLE");
    g(2, &"LTYPE");
    g(70, &1);
    g(0, &"LTYPE");
    g(5, &"10");
    g(100, &"AcDbSymbolTableRecord");
    g(100, &"AcDbLinetypeTableRecord");
    g(2, &"CONTINUOUS");
    g(70, &0);
    g(3, &"Solid line");
    g(72, &65);
    g(73, &0);
    g(40, &0);
    g(0, &"ENDTAB");
    g(0, &"TABLE");
    g(2, &"LAYER");
    g(70, &d.layers.len());
    for (i, (name, color)) in d.layers.iter().enumerate() {
        g(0, &"LAYER");
        g(5, &format!("{:X}", 0x20 + i));
        g(100, &"AcDbSymbolTableRecord");
        g(100, &"AcDbLayerTableRecord");
        g(2, name);
        g(70, &0);
        g(62, color);
        g(6, &"CONTINUOUS");
    }
    g(0, &"ENDTAB");
    g(0, &"ENDSEC");
    g(0, &"SECTION");
    g(2, &"ENTITIES");
    t.push_str(&d.body);
    t.push_str("0\nENDSEC\n0\nEOF\n");
    t
}

/// One DXF per part, numbered in the cut plan's order. `tool` is the cutter's diameter in meters.
pub fn files(m: &Model, ids: &[Id], tool: f64, dogbones: bool) -> Result<Vec<CncFile>> {
    if !tool.is_finite() || !(0.0..=MAX_TOOL).contains(&tool) {
        bail!("the tool diameter must be between 0 and {} mm", MAX_TOOL * 1000.0);
    }
    // far more than a cut plan holds: refuse before measuring (a few of them may be flat and not count)
    if crate::plugins::wanted_parts(m, ids).len() > crate::cutplan::MAX_PARTS * 4 {
        bail!("that is more parts than CNC files take ({}); pick some of them", crate::cutplan::MAX_PARTS);
    }
    let mut parts = measure(m, ids);
    if parts.len() > crate::cutplan::MAX_PARTS {
        bail!("{} parts is more than CNC files take ({}); pick some of them", parts.len(), crate::cutplan::MAX_PARTS);
    }
    if parts.is_empty() {
        bail!("no parts to cut: pick groups or components that are solid boards, or draw some");
    }
    sort_parts(&mut parts);
    let world = m.world_faces();
    let count = parts.len();
    let mine = faces_by_part(m, &world, &parts);
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let loops = loops_of(&world, &mine[i], p);
        if !loops.iter().any(|l| l.kind == Kind::Outline) {
            bail!("{} has no flat face to draw an outline from", p.name);
        }
        let mut counts = (0, 0);
        let text = dxf_text(p, i + 1, &loops, tool, dogbones, &mut counts);
        out.push(CncFile {
            number: i + 1,
            id: p.id,
            name: p.name.clone(),
            file: file_name(i + 1, count, &p.name),
            size: p.size,
            holes: loops.iter().filter(|l| l.kind == Kind::Through).count(),
            pockets: loops.iter().filter(|l| l.kind == Kind::Pocket).count(),
            reliefs: counts.0,
            skipped_corners: counts.1,
            bytes: text.into_bytes(),
        });
    }
    Ok(out)
}

/// The files in one .zip, in order, with the same time on each so the same parts always give the same bytes.
pub fn zip(files: &[CncFile]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in files {
            z.start_file(&f.file, opts)?;
            z.write_all(&f.bytes)?;
        }
        z.finish()?;
    }
    Ok(buf.into_inner())
}
