//! Import legacy SketchUp files (.skp from SketchUp 6-8 era, the "SketchUp
//! Model" MFC CArchive container). Clean-room: the layout below was worked
//! out from sample files, for interoperability. Nothing here comes from
//! SketchUp's code. Files from SketchUp 2013 on use a different layout and
//! are read by `skp_vff`; `parse` hands them over.
//!
//! The stream is MFC CArchive object serialization:
//!   new class : FFFF, schema u16, name length u16, ASCII name (takes a map index;
//!               its first object takes the next)
//!   new object of a known class : (0x8000 | class index) u16
//!   reference to an earlier object : its index as u16 (0 = null)
//!   big indices : 7FFF, then a u32 (high bit = class index)
//!
//! Records this importer understands (sizes in bytes; doubles are inches):
//!   CVertex   2, then x y z
//!   CEdge     12-byte entity header (byte 8 = soft), start vertex, end vertex, curve
//!   CCurve    3, then a u32 edge count (the edges point at the curve)
//!   CArcCurve 3, u32 segment count, then 14 doubles (center, normal, axes, angles)
//!   CFace     12-byte header (bytes 2..4 = material ref), plane (4 doubles),
//!             u32 loop count, loops, then a u16 back-material ref
//!   CLoop     4, then edge uses until a null
//!   CEdgeUse  u16, edge, u8 reversed, u16 loop ref
//!   CComponentInstance / CGroup  12-byte header, definition ref, 13 doubles
//!             (x axis, y axis, z axis, origin, scale), name
//!
//! Each component definition stores its entities as [u32 definition index + 1]
//! [u16 0][u32 count][objects]; the model's own entities are a bare
//! [u32 count][objects] near the end of the file. Materials sit in a table
//! near the start: name, type, RGBA.

use crate::geom;
use crate::model::{Definition, Edge, Face, Id, Instance, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DVec3, DVec4};
use std::collections::{BTreeMap, HashMap, HashSet};

const INCH: f64 = 0.0254;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Vertex,
    Edge,
    Face,
    Loop,
    EdgeUse,
    Curve,
    ArcCurve,
    Instance,
    ConstructionPoint,
    /// a class we don't decode (dimension, text...): the list parser skips it
    Unknown,
}

fn kind_of(name: &str) -> Option<Kind> {
    Some(match name {
        "CVertex" => Kind::Vertex,
        "CEdge" => Kind::Edge,
        "CFace" => Kind::Face,
        "CLoop" => Kind::Loop,
        "CEdgeUse" => Kind::EdgeUse,
        "CCurve" => Kind::Curve,
        "CArcCurve" => Kind::ArcCurve,
        "CComponentInstance" | "CGroup" => Kind::Instance,
        "CConstructionPoint" => Kind::ConstructionPoint,
        _ => Kind::Unknown,
    })
}

#[derive(Clone, Copy, Debug)]
enum Slot {
    Null,
    Ref(u32),
    New(u32),
}

impl Slot {
    fn index(self) -> Option<u32> {
        match self {
            Slot::Ref(i) | Slot::New(i) => Some(i),
            Slot::Null => None,
        }
    }
}

#[derive(Clone, Debug)]
enum Obj {
    Vertex(DVec3),
    Edge { a: Slot, b: Slot, soft: bool },
    Face { loops: Vec<Slot>, material: u32, normal: DVec3 },
    Loop { uses: Vec<Slot> },
    EdgeUse { edge: Slot, reversed: bool, loop_ref: u32 },
    Curve,
    Instance { def: Slot, m: [f64; 13] },
}

#[derive(Debug)]
struct Fail;

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    counter: u32,
    classes: HashMap<u32, Kind>,
    names: HashMap<u32, String>,
    objects: HashMap<u32, Obj>,
    depth: usize,
}

type R<T> = std::result::Result<T, Fail>;

impl<'a> Reader<'a> {
    fn need(&self, n: usize) -> R<()> {
        if self.p + n > self.d.len() { Err(Fail) } else { Ok(()) }
    }
    fn u8(&mut self) -> R<u8> {
        self.need(1)?;
        self.p += 1;
        Ok(self.d[self.p - 1])
    }
    fn u16(&mut self) -> R<u16> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.d[self.p], self.d[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> R<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.d[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        Ok(v)
    }
    fn f64(&mut self) -> R<f64> {
        self.need(8)?;
        let v = f64::from_le_bytes(self.d[self.p..self.p + 8].try_into().unwrap());
        self.p += 8;
        Ok(v)
    }
    fn skip(&mut self, n: usize) -> R<()> {
        self.need(n)?;
        self.p += n;
        Ok(())
    }
    fn cstring(&mut self) -> R<String> {
        self.need(3)?;
        if &self.d[self.p..self.p + 3] != b"\xff\xfe\xff" {
            return Err(Fail);
        }
        self.p += 3;
        let mut n = self.u8()? as usize;
        if n == 0xFF {
            n = self.u16()? as usize;
        }
        self.need(n * 2)?;
        let units: Vec<u16> = self.d[self.p..self.p + n * 2].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        self.p += n * 2;
        Ok(String::from_utf16_lossy(&units))
    }

    fn obj(&mut self) -> R<Slot> {
        if self.depth > 400 {
            return Err(Fail);
        }
        let tag = self.u16()?;
        let class = match tag {
            0 => return Ok(Slot::Null),
            0x7FFF => {
                let big = self.u32()?;
                if big & 0x8000_0000 == 0 {
                    return Ok(Slot::Ref(big));
                }
                big & 0x7FFF_FFFF
            }
            0xFFFF => {
                let _schema = self.u16()?;
                let n = self.u16()? as usize;
                if n == 0 || n > 64 {
                    return Err(Fail);
                }
                self.need(n)?;
                let name = String::from_utf8_lossy(&self.d[self.p..self.p + n]).to_string();
                self.p += n;
                let ci = self.counter;
                self.counter += 1;
                let Some(k) = kind_of(&name) else { return Err(Fail) };
                if !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'#') {
                    return Err(Fail);
                }
                self.classes.insert(ci, k);
                self.names.insert(ci, name);
                ci
            }
            t if t & 0x8000 != 0 => (t & 0x7FFF) as u32,
            t => return Ok(Slot::Ref(t as u32)),
        };
        let Some(&kind) = self.classes.get(&class) else { return Err(Fail) };
        let oi = self.counter;
        self.counter += 1;
        self.depth += 1;
        let body = self.body(kind);
        self.depth -= 1;
        self.objects.insert(oi, body?);
        Ok(Slot::New(oi))
    }

    fn body(&mut self, kind: Kind) -> R<Obj> {
        Ok(match kind {
            Kind::Vertex => {
                self.skip(2)?;
                let p = DVec3::new(self.f64()?, self.f64()?, self.f64()?);
                let ok = |v: f64| v.is_finite() && v.abs() < 1e7 && (v == 0.0 || v.abs() > 1e-300);
                if !(ok(p.x) && ok(p.y) && ok(p.z)) {
                    return Err(Fail);
                }
                Obj::Vertex(p)
            }
            Kind::Edge => {
                self.need(12)?;
                let soft = self.d[self.p + 8] != 0;
                self.skip(12)?;
                let a = self.obj()?;
                let b = self.obj()?;
                let _curve = self.obj()?;
                Obj::Edge { a, b, soft }
            }
            Kind::Curve => {
                self.skip(3)?;
                self.u32()?;
                Obj::Curve
            }
            Kind::ArcCurve => {
                self.skip(7 + 14 * 8)?;
                Obj::Curve
            }
            Kind::Face => {
                self.need(12)?;
                let material = u16::from_le_bytes([self.d[self.p + 2], self.d[self.p + 3]]) as u32;
                self.skip(12)?;
                let normal = DVec3::new(self.f64()?, self.f64()?, self.f64()?);
                let _d = self.f64()?;
                if !(normal.is_finite() && (normal.length() - 1.0).abs() < 1e-3) {
                    return Err(Fail);
                }
                let n = self.u32()?;
                if n == 0 || n > 10_000 {
                    return Err(Fail);
                }
                let mut loops = Vec::new();
                for _ in 0..n {
                    loops.push(self.obj()?);
                }
                self.u16()?; // back material
                Obj::Face { loops, material, normal }
            }
            Kind::Loop => {
                self.skip(4)?;
                let mut uses = Vec::new();
                loop {
                    match self.obj()? {
                        Slot::Null => break,
                        s => uses.push(s),
                    }
                    if uses.len() > 100_000 {
                        return Err(Fail);
                    }
                }
                Obj::Loop { uses }
            }
            Kind::EdgeUse => {
                self.u16()?;
                let edge = self.obj()?;
                let reversed = self.u8()? != 0;
                let loop_ref = self.u16()? as u32;
                Obj::EdgeUse { edge, reversed, loop_ref }
            }
            Kind::ConstructionPoint => {
                self.skip(12 + 48 + 1)?;
                Obj::Curve
            }
            Kind::Unknown => return Err(Fail),
            Kind::Instance => {
                self.skip(12)?;
                let def = self.obj()?;
                let mut m = [0.0; 13];
                for v in &mut m {
                    *v = self.f64()?;
                    if !v.is_finite() {
                        return Err(Fail);
                    }
                }
                self.cstring()?;
                Obj::Instance { def, m }
            }
        })
    }

    fn kind(&self, s: Slot) -> Option<&Obj> {
        s.index().and_then(|i| self.objects.get(&i))
    }

    /// None if any reference points at the wrong kind of object; otherwise
    /// (resolved references, faces + instances).
    fn validate(&self) -> Option<(usize, usize)> {
        let mut good = 0;
        let mut items = 0;
        let mut check = |s: Slot, want: fn(&Obj) -> bool| -> bool {
            match s {
                Slot::Ref(i) => match self.objects.get(&i) {
                    Some(o) if want(o) => {
                        good += 1;
                        true
                    }
                    _ => false,
                },
                _ => true,
            }
        };
        for o in self.objects.values() {
            match o {
                Obj::Edge { a, b, .. } => {
                    if !check(*a, |o| matches!(o, Obj::Vertex(_))) || !check(*b, |o| matches!(o, Obj::Vertex(_))) {
                        return None;
                    }
                }
                Obj::EdgeUse { edge, loop_ref, .. } => {
                    if !check(*edge, |o| matches!(o, Obj::Edge { .. })) || !check(Slot::Ref(*loop_ref), |o| matches!(o, Obj::Loop { .. })) {
                        return None;
                    }
                }
                Obj::Face { loops, .. } => {
                    items += 1;
                    for l in loops {
                        if !check(*l, |o| matches!(o, Obj::Loop { .. })) {
                            return None;
                        }
                    }
                }
                Obj::Instance { .. } => items += 1,
                _ => {}
            }
        }
        Some((good, items))
    }
}

/// A component definition's (or the model's) entities, resolved to points.
#[derive(Default, Debug, Clone)]
pub struct Entities {
    pub faces: Vec<SkpFace>,
    pub lines: Vec<(DVec3, DVec3, bool)>,
    pub instances: Vec<(u32, DMat4)>,
}

#[derive(Debug, Clone)]
pub struct SkpFace {
    pub outer: Vec<DVec3>,
    pub holes: Vec<Vec<DVec3>>,
    pub material: u32,
}

#[derive(Debug, Default)]
pub struct SkpFile {
    pub version: String,
    pub definitions: BTreeMap<u32, (String, Entities)>,
    pub roots: Vec<Entities>,
    pub materials: HashMap<u32, (String, [f32; 3])>,
    /// images of textured materials (modern files)
    pub textures: HashMap<u32, crate::skp_vff::Texture>,
}

fn resolve(r: &Reader, items: &[Slot]) -> Entities {
    let mut out = Entities::default();
    let point = |s: Slot| -> Option<DVec3> {
        match r.kind(s)? {
            Obj::Vertex(p) => Some(*p),
            _ => None,
        }
    };
    let mut face_edges: HashSet<u32> = HashSet::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for it in items {
        let Some(idx) = it.index() else { continue };
        if !seen.insert(idx) {
            continue;
        }
        match r.objects.get(&idx) {
            Some(Obj::Face { loops, material, normal }) => {
                let mut rings = Vec::new();
                for l in loops {
                    let Some(Obj::Loop { uses }) = r.kind(*l) else { continue };
                    let mut ring = Vec::new();
                    for u in uses {
                        let Some(Obj::EdgeUse { edge, reversed, .. }) = r.kind(*u) else { continue };
                        let Some(Obj::Edge { a, b, .. }) = r.kind(*edge) else { continue };
                        if let Some(e) = edge.index() {
                            face_edges.insert(e);
                        }
                        let (Some(pa), Some(pb)) = (point(*a), point(*b)) else { continue };
                        ring.push(if *reversed { pb } else { pa });
                    }
                    if ring.len() >= 3 {
                        rings.push(ring);
                    }
                }
                if !rings.is_empty() {
                    let mut outer = rings.remove(0);
                    if geom::newell(&outer).dot(*normal) < 0.0 {
                        outer.reverse();
                    }
                    out.faces.push(SkpFace { outer, holes: rings, material: *material });
                }
            }
            Some(Obj::Instance { def, m }) => {
                if let Some(d) = def.index() {
                    let t = DMat4::from_cols(
                        DVec4::new(m[0], m[1], m[2], 0.0),
                        DVec4::new(m[3], m[4], m[5], 0.0),
                        DVec4::new(m[6], m[7], m[8], 0.0),
                        DVec4::new(m[9], m[10], m[11], if m[12].abs() > 1e-12 { m[12] } else { 1.0 }),
                    );
                    out.instances.push((d, t));
                }
            }
            _ => {}
        }
    }
    // loose lines: edges listed directly that no face uses
    for it in items {
        if let Some(idx) = it.index() {
            if face_edges.contains(&idx) {
                continue;
            }
            if let Some(Obj::Edge { a, b, soft }) = r.objects.get(&idx) {
                if let (Some(pa), Some(pb)) = (point(*a), point(*b)) {
                    out.lines.push((pa, pb, *soft));
                }
            }
        }
    }
    out
}

/// Learn class indices that were declared outside any entity list, from how
/// the stream uses them (the first edge's second vertex; the first edge use's
/// loop reference).
fn learn(d: &[u8]) -> HashMap<u32, Kind> {
    let mut known = HashMap::new();
    let headers = class_headers(d);
    if let Some(&(off, _)) = headers.iter().find(|(_, n)| n == "CVertex") {
        let after = off + 6 + 7 + 2 + 24;
        if after + 2 <= d.len() {
            let tag = u16::from_le_bytes([d[after], d[after + 1]]);
            if tag & 0x8000 != 0 && tag != 0xFFFF {
                let v = (tag & 0x7FFF) as u32;
                known.insert(v, Kind::Vertex);
                known.insert(v - 2, Kind::Edge);
            }
        }
    }
    if let Some(&(off, _)) = headers.iter().find(|(_, n)| n == "CEdgeUse") {
        let body = off + 6 + 8;
        if body + 7 <= d.len() {
            let e = u16::from_le_bytes([d[body + 2], d[body + 3]]);
            if e & 0x8000 == 0 && e != 0 {
                let l = u16::from_le_bytes([d[body + 5], d[body + 6]]) as u32;
                if l >= 3 {
                    let f = l - 3;
                    known.insert(f, Kind::Face);
                    known.insert(f + 2, Kind::Loop);
                    known.insert(f + 4, Kind::EdgeUse);
                }
            }
        }
    }
    known
}

fn class_headers(d: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 < d.len() {
        if d[i] == 0xFF && d[i + 1] == 0xFF {
            let n = u16::from_le_bytes([d[i + 4], d[i + 5]]) as usize;
            if (3..=40).contains(&n) && i + 6 + n <= d.len() && d[i + 6] == b'C' && d[i + 6..i + 6 + n].iter().all(|c| c.is_ascii_alphanumeric() || *c == b'#') {
                out.push((i, String::from_utf8_lossy(&d[i + 6..i + 6 + n]).to_string()));
                i += 6 + n;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Materials: each is [tag][u8][name][u16 type][RGBA]...; textured ones (type
/// != 0) get a neutral color. Indices count up from the CMaterial class index;
/// a texture's image object takes one more.
fn materials(d: &[u8]) -> HashMap<u32, (String, [f32; 3])> {
    let mut out = HashMap::new();
    let Some((hoff, _)) = class_headers(d).into_iter().find(|(_, n)| n == "CMaterial") else { return out };
    // the class index: the tag the second material uses
    let mut p = hoff + 6 + 9;
    let mut idx: Option<u32> = None;
    let mut first = true;
    for _ in 0..4096 {
        if !first {
            if p + 2 > d.len() {
                break;
            }
            let tag = u16::from_le_bytes([d[p], d[p + 1]]);
            if tag & 0x8000 == 0 || tag == 0xFFFF {
                break;
            }
            let cls = (tag & 0x7FFF) as u32;
            if idx.is_none() {
                idx = Some(cls + 2); // class, first material, then this one
                // backfill the first material's index
                if let Some(v) = out.remove(&u32::MAX) {
                    out.insert(cls + 1, v);
                }
            }
            p += 2;
        }
        let mut r = Reader { d, p, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0 };
        if r.skip(2).is_err() {
            break;
        }
        let Ok(name) = r.cstring() else { break };
        let Ok(kind) = r.u16() else { break };
        let rgb = if kind == 0 && r.need(4).is_ok() {
            [d[r.p] as f32 / 255.0, d[r.p + 1] as f32 / 255.0, d[r.p + 2] as f32 / 255.0]
        } else {
            [0.72, 0.62, 0.48] // textured: a neutral wood tone
        };
        let key = if first { u32::MAX } else { *idx.as_ref().unwrap() };
        out.insert(key, (name, rgb));
        if !first {
            *idx.as_mut().unwrap() += 1;
            if kind != 0 {
                *idx.as_mut().unwrap() += 1; // the texture image object
            }
        }
        first = false;
        // find the next material tag: skip forward to the next "<tag> 00 FF FE FF"
        let limit = (r.p + 400_000).min(d.len().saturating_sub(7));
        let Some(next) = (r.p..limit).find(|&i| d[i + 1] & 0x80 != 0 && d[i + 1] != 0xFF && d[i + 2] == 0 && d[i + 3] == 0 && &d[i + 4..i + 7] == b"\xff\xfe\xff") else { break };
        p = next;
    }
    out.remove(&u32::MAX);
    out
}

/// Parse up to `n` list items with no skipping; returns how many parsed and
/// whether the references so far are consistent. Cheap: wrong guesses of the
/// starting index fail within a few bytes.
fn parse_prefix(mut r: Reader, n: usize) -> (usize, Option<usize>) {
    let mut k = 0;
    while k < n {
        if r.obj().is_err() {
            break;
        }
        k += 1;
    }
    (k, r.validate().map(|(good, _)| good))
}

/// Parse `n` list items. When an item can't be decoded (a dimension, a text
/// label...), look ahead for a plausible next item where the rest of the list
/// parses cleanly, allowing for the object indices the skipped item used.
fn parse_items<'a>(mut r: Reader<'a>, n: usize, budget: usize, tries: &mut usize) -> Option<(Reader<'a>, Vec<Slot>)> {
    let mut items = Vec::with_capacity(n);
    for k in 0..n {
        let start = r.p;
        let counter = r.counter;
        let classes = r.classes.clone();
        let objects = r.objects.clone();
        match r.obj() {
            Ok(s) => items.push(s),
            Err(_) => {
                if std::env::var_os("SKP_DEBUG").is_some() && budget == 4 {
                    eprintln!("  item {k}/{n} at {start:#x} undecodable (counter {counter:#x}); looking ahead");
                }
                if budget == 0 {
                    return None;
                }
                let rest = n - k - 1;
                let d = r.d;
                let limit = (start + 32 * 1024).min(d.len().saturating_sub(2));
                // a plausible item start: a new object of a class we decode, or a
                // reference to an edge, face or instance we already have
                let plausible = |q: usize| -> bool {
                    let t = u16::from_le_bytes([d[q], d[q + 1]]);
                    if t == 0xFFFF {
                        let nl = d.get(q + 4..q + 6).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0);
                        return d.get(q + 6..q + 6 + nl).map_or(false, |nm| matches!(nm, b"CEdge" | b"CFace" | b"CComponentInstance" | b"CGroup" | b"CConstructionPoint"));
                    }
                    if t & 0x8000 != 0 {
                        return matches!(classes.get(&((t & 0x7FFF) as u32)), Some(Kind::Edge | Kind::Face | Kind::Instance | Kind::ConstructionPoint));
                    }
                    matches!(objects.get(&(t as u32)), Some(Obj::Edge { .. } | Obj::Face { .. } | Obj::Instance { .. }))
                };
                for q in start + 2..limit {
                    if !plausible(q) {
                        continue;
                    }
                    // the skipped stretch may hold several items (two dimensions in
                    // a row) and declare a few classes of its own
                    for skipped in 0..=rest.min(5) {
                        for extra in 1..=16u32 {
                            *tries += 1;
                            if *tries > 60_000 {
                                return None;
                            }
                            let want = rest - skipped;
                            let fresh = || Reader { d, p: q, counter: counter + extra, classes: classes.clone(), names: HashMap::new(), objects: objects.clone(), depth: 0 };
                            // cheap check first: the next few items must parse on their own
                            let (k2, ok) = parse_prefix(fresh(), want.min(4));
                            if ok.is_none() || k2 < want.min(4) {
                                continue;
                            }
                            if let Some((t2, more)) = parse_items(fresh(), want, budget - 1, tries) {
                                if t2.validate().is_some() {
                                    items.extend(std::iter::repeat(Slot::Null).take(skipped + 1));
                                    items.extend(more);
                                    return Some((t2, items));
                                }
                            }
                        }
                    }
                }
                return None;
            }
        }
    }
    Some((r, items))
}

pub fn parse(d: &[u8]) -> Result<SkpFile> {
    if d.len() < 40 || &d[0..4] != b"\xff\xfe\xff\x0e" {
        bail!("not a SketchUp file");
    }
    let head: Vec<u16> = d[4..4 + 28].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    if String::from_utf16_lossy(&head) != "SketchUp Model" {
        bail!("not a SketchUp file");
    }
    let mut file = SkpFile::default();
    let mut r = Reader { d, p: 0x20, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0 };
    file.version = r.cstring().unwrap_or_default();
    let major: u32 = file.version.trim_start_matches('{').split('.').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if major >= 13 || crate::skp_vff::zip_start(d).is_some() {
        return crate::skp_vff::parse(d, file.version);
    }

    let mut known = learn(d);
    file.materials = materials(d);
    let geometry_tag = |known: &HashMap<u32, Kind>, t: u16, i: usize| -> bool {
        if t == 0xFFFF {
            let n = d.get(i + 4..i + 6).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0);
            return d.get(i + 6..i + 6 + n).map_or(false, |name| matches!(name, b"CEdge" | b"CFace" | b"CComponentInstance" | b"CGroup"));
        }
        t & 0x8000 != 0 && matches!(known.get(&((t & 0x7FFF) as u32)), Some(Kind::Edge | Kind::Face | Kind::Instance))
    };

    let new_reader = |known: &HashMap<u32, Kind>, at: usize, base: u32| Reader { d, p: at, counter: base, classes: known.clone(), names: HashMap::new(), objects: HashMap::new(), depth: 0 };
    // Pick the starting index from the cleanly-parsing prefix, then parse fully
    // (skipping undecodable items) with that one.
    let try_bases = |known: &HashMap<u32, Kind>, at: usize, n: u32, bases: &mut dyn Iterator<Item = u32>| -> Option<(Reader, Vec<Slot>, usize)> {
        let mut best: Option<(usize, usize, u32)> = None; // (items parsed, good refs, base)
        for base in bases {
            let (k, good) = parse_prefix(new_reader(known, at, base), n as usize);
            let Some(good) = good else { continue };
            if k == 0 {
                continue;
            }
            let better = best.map_or(true, |(bk, bg, _)| (k, good) > (bk, bg));
            if better {
                best = Some((k, good, base));
                if k == n as usize && good > 0 {
                    break;
                }
            }
        }
        let (bk, bg, base) = best?;
        if std::env::var_os("SKP_DEBUG").is_some() && n > 50 {
            eprintln!("list @{at:#x} n={n}: best base {base:#x} parses {bk} items cleanly ({bg} refs)");
        }
        let mut tries = 0;
        let (r, items) = parse_items(new_reader(known, at, base), n as usize, 4, &mut tries)?;
        let (good, count) = r.validate()?;
        if count == 0 {
            return None;
        }
        Some((r, items, good))
    };

    let mut i = 0usize;
    let mut last_counter = 1u32;
    while i + 12 < d.len() {
        // a component definition's list: [u32 def][u16 0][u32 n][object]
        let x = u32::from_le_bytes(d[i..i + 4].try_into().unwrap());
        let z = u16::from_le_bytes([d[i + 4], d[i + 5]]);
        let n = u32::from_le_bytes(d[i + 6..i + 10].try_into().unwrap());
        let t = u16::from_le_bytes([d[i + 10], d[i + 11]]);
        if z == 0 && (1..=2_000_000).contains(&n) && x > 0 && x < 0x7FFF_FFFF && geometry_tag(&known, t, i + 10) {
            if let Some((r, items, _)) = try_bases(&known, i + 10, n, &mut [x + 1, x + 2, x + 3].into_iter()) {
                for (k, v) in &r.classes {
                    known.insert(*k, *v);
                }
                last_counter = last_counter.max(r.counter);
                if std::env::var_os("SKP_DEBUG").is_some() {
                    eprintln!("def list @{i:#x} def={x:#x} n={n} end={:#x} counter={:#x}", r.p, r.counter);
                }
                let ents = resolve(&r, &items);
                let name = (r.p..(r.p + 48).min(d.len().saturating_sub(4)))
                    .find(|&k| &d[k..k + 3] == b"\xff\xfe\xff")
                    .and_then(|k| {
                        let mut nr = Reader { d, p: k, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0 };
                        nr.cstring().ok()
                    })
                    .unwrap_or_default();
                // X is one past the definition's own object index
                file.definitions.insert(x - 1, (name, ents));
                i = r.p;
                continue;
            }
        }
        // the model's own list: [u32 n][object], counter unknown: search upward
        let n0 = x;
        let t0 = u16::from_le_bytes([d[i + 4], d[i + 5]]);
        if (1..=2_000_000).contains(&n0) && geometry_tag(&known, t0, i + 4) && i > 0x100 {
            if let Some((r, items, _)) = try_bases(&known, i + 4, n0, &mut (last_counter..last_counter.saturating_add(8192))) {
                for (k, v) in &r.classes {
                    known.insert(*k, *v);
                }
                last_counter = last_counter.max(r.counter);
                if std::env::var_os("SKP_DEBUG").is_some() {
                    eprintln!("root list @{i:#x} n={n0} end={:#x} base={:#x}", r.p, r.counter);
                }
                file.roots.push(resolve(&r, &items));
                i = r.p;
                continue;
            }
        }
        i += 1;
    }
    if file.definitions.is_empty() && file.roots.is_empty() {
        bail!("no geometry found in this SketchUp file");
    }
    Ok(file)
}

/// Build a SketchCraft model fragment: every definition becomes a component,
/// the model's own entities go into a group, instances keep their transforms.
pub fn import(m: &mut Model, d: &[u8], at: DVec3, name: &str) -> Result<ImportSummary> {
    let file = parse(d)?;
    let mut summary = ImportSummary { version: file.version.clone(), ..Default::default() };
    let s = DMat4::from_scale(DVec3::splat(INCH));
    let s_inv = DMat4::from_scale(DVec3::splat(1.0 / INCH));

    // definitions -> components (geometry in meters)
    let mut def_ids: HashMap<u32, Id> = HashMap::new();
    for (&key, (_, ents)) in &file.definitions {
        let id = m.new_id();
        def_ids.insert(key, id);
        let _ = ents;
        let label = file.definitions[&key].0.clone();
        let label = if label.is_empty() { format!("{name} part {}", def_ids.len()) } else { label };
        m.definitions.insert(id, Definition { name: label, model: Default::default() });
    }
    for (&key, (_, ents)) in &file.definitions {
        let mut dm = Model::default();
        summary.faces += fill(&mut dm, ents, &file);
        for (child, t) in &ents.instances {
            if let Some(&cid) = def_ids.get(child) {
                let iid = dm.new_id();
                dm.instances.insert(iid, Instance { def: cid, name: "part".into(), transform: s * *t * s_inv, tag: None, hidden: false });
            }
        }
        // materials live on the document too, so the app can show their images
        for (k, mat) in &dm.materials {
            m.materials.entry(k.clone()).or_insert(mat.clone());
        }
        let id = def_ids[&key];
        m.definitions.get_mut(&id).unwrap().model = std::sync::Arc::new(dm);
    }

    // the model's own entities
    let gid = m.begin_group(name.to_string());
    let base = DMat4::from_translation(at);
    let mut placed = 0;
    for root in &file.roots {
        let mut tmp = Model::default();
        summary.faces += fill(&mut tmp, root, &file);
        merge_into(m, &tmp, gid, base);
        for (child, t) in &root.instances {
            if let Some(&cid) = def_ids.get(child) {
                let iid = m.new_id();
                m.instances.insert(iid, Instance { def: cid, name: format!("{name} part"), transform: base * s * *t * s_inv, tag: None, hidden: false });
                placed += 1;
            }
        }
    }
    m.active_group = None;
    if file.roots.is_empty() {
        // no model list found: show every definition that nothing else places
        let used: HashSet<u32> = file.definitions.values().flat_map(|(_, e)| e.instances.iter().map(|(d, _)| *d)).collect();
        for key in file.definitions.keys().filter(|k| !used.contains(k)) {
            let iid = m.new_id();
            m.instances.insert(iid, Instance { def: def_ids[key], name: name.to_string(), transform: base, tag: None, hidden: false });
            placed += 1;
        }
    }
    // drop definitions nothing uses (SketchUp keeps unused ones in the file)
    summary.components = m.definitions.len();
    summary.placed = placed;
    summary.group = gid;
    Ok(summary)
}

#[derive(Debug, Default)]
pub struct ImportSummary {
    pub version: String,
    pub faces: usize,
    pub components: usize,
    pub placed: usize,
    pub group: Id,
}

/// Add faces and loose lines (inches -> meters) to a model, in bulk.
fn fill(m: &mut Model, e: &Entities, file: &SkpFile) -> usize {
    let mut keys: HashMap<u32, String> = HashMap::new();
    for f in &e.faces {
        if keys.contains_key(&f.material) {
            continue;
        }
        if let Some((name, rgb)) = file.materials.get(&f.material) {
            let key = format!("skp:{name}");
            let texture = file.textures.get(&f.material).map(|t| crate::model::Texture { data: t.data.clone(), width: t.tile, height: t.tile * t.aspect, project: None, origin: None });
            m.materials.entry(key.clone()).or_insert(crate::model::Material { color: *rgb, texture });
            keys.insert(f.material, key);
        }
    }
    let group = m.active_group;
    let mut mesh = crate::mesh::Mesh::new(m, group);
    for f in &e.faces {
        let outer: Vec<DVec3> = f.outer.iter().map(|p| *p * INCH).collect();
        let holes: Vec<Vec<DVec3>> = f.holes.iter().map(|h| h.iter().map(|p| *p * INCH).collect()).collect();
        mesh.face(&outer, &holes, keys.get(&f.material).cloned());
    }
    for (a, b, soft) in &e.lines {
        mesh.line(*a * INCH, *b * INCH, *soft);
    }
    mesh.finish().0
}

/// Copy a model's geometry into `m` under group `gid`, transformed.
fn merge_into(m: &mut Model, src: &Model, gid: Id, t: DMat4) {
    let mut map: HashMap<Id, Id> = HashMap::new();
    for (&k, v) in &src.vertices {
        map.insert(k, m.vertex_in(t.transform_point3(v.p), Some(gid)));
    }
    for e in src.edges.values() {
        let id = m.edge(map[&e.a], map[&e.b]);
        let ne: &mut Edge = m.edges.get_mut(&id).unwrap();
        ne.soft = e.soft;
    }
    for (k, mat) in &src.materials {
        m.materials.entry(k.clone()).or_insert(mat.clone());
    }
    for f in src.faces.values() {
        m.insert_face(Face {
            outer: f.outer.iter().map(|v| map[v]).collect(),
            holes: f.holes.iter().map(|h| h.iter().map(|v| map[v]).collect()).collect(),
            material: f.material.clone(),
            group: Some(gid),
            hidden: false,
        });
    }
}
