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
//!             (three basis rows, origin, scale), name
//!
//! Each component definition stores its entities as [u32 definition index + 1]
//! [u16 0][u32 count][objects]; the model's own entities are a bare
//! [u32 count][objects] near the end of the file. Materials sit in a table
//! near the start: name, type, RGBA.

use crate::geom;
use crate::model::{Annotation, Definition, Edge, Face, Id, Instance, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DVec3, DVec4};
use std::collections::{BTreeMap, HashMap, HashSet};

const INCH: f64 = 0.0254;
/// The most dimensions one list may give (a damaged file can't flood the model with annotations).
const MAX_DIMS: usize = 10_000;

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
    /// the dictionaries an entity may carry (named attribute sets)
    AttrContainer,
    AttrNamed,
    /// a textured face's mapping, also kept in the container
    AttrTexCoords,
    /// a linear dimension (CDimensionLinear) and the font it names (CSkFont)
    DimLinear,
    /// a text label (CText): read so the list goes on, not placed
    Text,
    Font,
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
        // an Image is a placement of a definition that holds the picture's face, laid out like a component
        "CComponentInstance" | "CGroup" | "CImage" => Kind::Instance,
        "CConstructionPoint" => Kind::ConstructionPoint,
        "CAttributeContainer" => Kind::AttrContainer,
        "CAttributeNamed" => Kind::AttrNamed,
        "CFaceTextureCoords" => Kind::AttrTexCoords,
        "CDimensionLinear" => Kind::DimLinear,
        "CText" => Kind::Text,
        "CSkFont" => Kind::Font,
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
    Font,
    /// a text label: found, not imported
    Text,
    /// a linear dimension: where each end is, the text typed over the measured one (usually empty), and the
    /// direction and distance it is drawn offset from the measured points (inches)
    Dim { ends: [DimEnd; 2], text: String, dir: DVec3, dist: f64 },
}

/// One end of a dimension: a vertex of the model (by object index), or a free point.
#[derive(Clone, Copy, Debug)]
enum DimEnd {
    Vertex(u32),
    Free(DVec3),
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
    /// SketchUp 2017 and later: every entity carries an id after its attribute slot
    newf: bool,
    /// While parse_items reads an item: what the item added to `classes` and
    /// `objects` (the key and what it replaced), so a failed item is rewound
    /// instead of every item paying for a copy of both maps
    log: bool,
    jc: Vec<(u32, Option<Kind>)>,
    jo: Vec<(u32, Option<Obj>)>,
}

type R<T> = std::result::Result<T, Fail>;

impl<'a> Reader<'a> {
    fn put_class(&mut self, k: u32, kind: Kind) {
        let old = self.classes.insert(k, kind);
        if self.log {
            self.jc.push((k, old));
        }
    }
    /// Undoes everything logged since the last `mark`.
    fn rewind(&mut self) {
        for (k, old) in self.jc.drain(..).rev() {
            match old {
                Some(v) => self.classes.insert(k, v),
                None => self.classes.remove(&k),
            };
        }
        for (k, old) in self.jo.drain(..).rev() {
            match old {
                Some(v) => self.objects.insert(k, v),
                None => self.objects.remove(&k),
            };
        }
    }
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

    /// An entity's start: its attribute container slot (usually null; when a
    /// dictionary hangs off the entity it is stored here, in line), then, from
    /// SketchUp 2017 on, an id: a mask byte saying which of the id's four
    /// bytes are not zero (0 = id 0, 3 = the low two), then just those bytes.
    fn head(&mut self) -> R<()> {
        self.attrs()?;
        self.id_field()
    }

    fn id_field(&mut self) -> R<()> {
        if self.newf {
            let t = self.u8()?;
            if t > 0x0f {
                return Err(Fail);
            }
            self.skip(t.count_ones() as usize)?;
        }
        Ok(())
    }

    /// Reads a tag in a slot that holds attribute objects. Some(kind) for a new
    /// object (declaring its class first when the tag says so; the class and the
    /// object each take a map index), None for null and back references.
    fn new_attr_object(&mut self, allowed: &[Kind]) -> R<Option<Kind>> {
        let tag = self.u16()?;
        let (class, declared) = match tag {
            0 => return Ok(None),
            0x7FFF => {
                let big = self.u32()?;
                if big & 0x8000_0000 == 0 {
                    return Ok(None);
                }
                (big & 0x7FFF_FFFF, None)
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
                let Some(k) = kind_of(&name).filter(|k| allowed.contains(k)) else { return Err(Fail) };
                let ci = self.counter;
                self.counter += 1;
                (ci, Some(k))
            }
            t if t & 0x8000 != 0 => ((t & 0x7FFF) as u32, None),
            _ => return Ok(None),
        };
        // a class declared outside the list: the slot can only hold these kinds,
        // and the usual one is the first
        let kind = declared.or_else(|| self.classes.get(&class).copied()).unwrap_or(allowed[allowed.len() - 1]);
        if !allowed.contains(&kind) {
            return Err(Fail);
        }
        self.put_class(class, kind);
        self.counter += 1;
        Ok(Some(kind))
    }

    /// One value of a named attribute set, its type byte already read: 4 a 32-bit integer, 6 a double,
    /// 7 a flag byte, 10 a string, 11 an array (a u32 count, then that many values, each with its own type
    /// byte; a Bezier curve's control points are stored this way) and 17 a point or vector (3 doubles).
    fn attr_value(&mut self, t: u8, depth: usize) -> R<()> {
        match t {
            4 => self.skip(4),
            6 => self.skip(8),
            7 => self.skip(1),
            10 => self.cstring().map(|_| ()),
            17 => self.skip(24),
            11 if depth < 4 => {
                let n = self.u32()? as usize;
                // every element is at least its type byte and a byte of value
                if n > (self.d.len() - self.p) / 2 {
                    return Err(Fail);
                }
                for _ in 0..n {
                    let et = self.u8()?;
                    self.attr_value(et, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(Fail),
        }
    }

    fn attrs(&mut self) -> R<()> {
        if self.new_attr_object(&[Kind::AttrContainer])?.is_none() {
            return Ok(());
        }
        // the container is an entity itself: a null slot and an id
        self.skip(2)?;
        self.id_field()?;
        for _ in 0..100_000 {
            let Some(kind) = self.new_attr_object(&[Kind::AttrTexCoords, Kind::AttrNamed])? else { return Ok(()) };
            self.skip(2)?;
            self.id_field()?;
            self.skip(4)?;
            if kind == Kind::AttrTexCoords {
                // 24 doubles of mapping, two lists of 4-double points (a count each), 8 bytes
                self.skip(24 * 8)?;
                for _ in 0..2 {
                    let n = self.u32()? as usize;
                    if n > 4096 {
                        return Err(Fail);
                    }
                    self.skip(n * 32)?;
                }
                self.skip(8)?;
                continue;
            }
            // a named set: its name, [key, type, value]... up to an empty key, a u32
            self.cstring()?;
            for _ in 0..1_000_000 {
                if self.cstring()?.is_empty() {
                    break;
                }
                let t = self.u8()?;
                self.attr_value(t, 0)?;
            }
            self.skip(4)?;
        }
        Err(Fail)
    }

    fn obj(&mut self) -> R<Slot> {
        if self.depth > 400 || (self.counter & 1023 == 1023 && out_of_time()) {
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
                self.put_class(ci, k);
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
        let body = body?;
        if self.log {
            self.jo.push((oi, self.objects.get(&oi).cloned()));
        }
        self.objects.insert(oi, body);
        Ok(Slot::New(oi))
    }

    fn body(&mut self, kind: Kind) -> R<Obj> {
        Ok(match kind {
            Kind::Vertex => {
                self.head()?;
                let p = DVec3::new(self.f64()?, self.f64()?, self.f64()?);
                let ok = |v: f64| v.is_finite() && v.abs() < 1e7 && (v == 0.0 || v.abs() > 1e-300);
                if !(ok(p.x) && ok(p.y) && ok(p.z)) {
                    return Err(Fail);
                }
                Obj::Vertex(p)
            }
            Kind::Edge => {
                self.head()?;
                self.need(10)?;
                let soft = self.d[self.p + 6] != 0;
                self.skip(10)?;
                let a = self.obj()?;
                let b = self.obj()?;
                let _curve = self.obj()?;
                Obj::Edge { a, b, soft }
            }
            Kind::Curve => {
                self.head()?;
                self.skip(1)?;
                self.u32()?;
                Obj::Curve
            }
            Kind::ArcCurve => {
                self.head()?;
                self.skip(5 + 14 * 8)?;
                Obj::Curve
            }
            Kind::Face => {
                self.head()?;
                self.need(10)?;
                let material = u16::from_le_bytes([self.d[self.p], self.d[self.p + 1]]) as u32;
                self.skip(10)?;
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
                self.head()?;
                self.skip(2)?;
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
                self.head()?;
                let edge = self.obj()?;
                let reversed = self.u8()? != 0;
                // a plain reference: a u16, or 0x7FFF and a u32 once there are 32767+ objects
                let loop_ref = self.plain_ref()?;
                Obj::EdgeUse { edge, reversed, loop_ref }
            }
            Kind::ConstructionPoint => {
                self.head()?;
                self.skip(10 + 48 + 1)?;
                Obj::Curve
            }
            Kind::Font => {
                self.head()?;
                self.cstring()?;
                // size and style: 15 bytes
                self.skip(15)?;
                Obj::Font
            }
            Kind::Text => self.text_label()?,
            Kind::DimLinear => {
                let start = self.p;
                let r = self.dim_linear();
                if r.is_err() && std::env::var_os("SKP_DEBUG").is_some() {
                    eprintln!("  dimension at {start:#x} failed at {:#x}", self.p);
                }
                r?
            }
            Kind::Unknown | Kind::AttrContainer | Kind::AttrNamed | Kind::AttrTexCoords => return Err(Fail),
            Kind::Instance => {
                self.head()?;
                self.skip(10)?;
                let def = self.obj()?;
                let mut m = [0.0; 13];
                for v in &mut m {
                    *v = self.f64()?;
                    if !v.is_finite() {
                        return Err(Fail);
                    }
                }
                self.cstring()?;
                self.skip(16)?; // its GUID
                Obj::Instance { def, m }
            }
        })
    }

    /// CText (worked out from one SketchUp 2016 file): entity header, 10 bytes, the font (an object or a
    /// reference), 16 bytes, an end like a dimension's (u32 2, u32 4, a point, a vertex reference,
    /// 10 bytes), a point and a direction (6 doubles), a short fixed stretch of counts and flags, the text (a
    /// string) and a few zero bytes. Only read to the end so the entity list carries on.
    fn text_label(&mut self) -> R<Obj> {
        self.head()?;
        self.skip(10)?;
        match self.obj()? {
            Slot::New(i) | Slot::Ref(i) if i == 0 || matches!(self.objects.get(&i), Some(Obj::Font) | None) => {}
            Slot::Null => {}
            _ => return Err(Fail),
        }
        self.skip(16)?;
        // the first count is a kind (1, 2 and 5 are seen); the second is 4
        if self.u32()? > 16 || self.u32()? != 4 {
            return Err(Fail);
        }
        self.skip(24)?;
        self.plain_ref()?;
        self.skip(10)?;
        for _ in 0..6 {
            self.coord()?;
        }
        // the counts and flags before the text vary; the text starts at the first string marker
        let end = (self.p + 40).min(self.d.len());
        let at = (self.p..end).find(|&i| self.d.get(i..i + 3) == Some(b"\xff\xfe\xff")).ok_or(Fail)?;
        self.p = at;
        self.cstring()?;
        // the closing zero bytes
        for _ in 0..5 {
            if self.d.get(self.p) != Some(&0) {
                break;
            }
            self.p += 1;
        }
        Ok(Obj::Text)
    }

    /// A plain reference: a u16, or 0x7FFF and a u32 once there are 32767+ objects.
    fn plain_ref(&mut self) -> R<u32> {
        Ok(match self.u16()? {
            0x7FFF => self.u32()? & 0x7FFF_FFFF,
            r => r as u32,
        })
    }

    /// A finite coordinate of a sane size (inches).
    fn coord(&mut self) -> R<f64> {
        let v = self.f64()?;
        if v.is_finite() && v.abs() < 1e7 { Ok(v) } else { Err(Fail) }
    }

    /// CDimensionLinear (worked out from SketchUp 7 files):
    ///   entity header, 10 bytes, the typed text (a string; empty = show the measured length), the font (an object
    ///   or a reference), a zero byte, u32 3,
    ///   two ends of 44 bytes each [u32 2, u32 4, a point (3 doubles, zero when attached), a vertex reference (u16, or 0x7FFF and a u32 in big files), 10 bytes],
    ///   six doubles (the offset direction, then the plane's), u32 2, the offset distance (a double),
    ///   a few zero bytes, u32 3.
    fn dim_linear(&mut self) -> R<Obj> {
        self.head()?;
        self.skip(10)?;
        let mut text = self.cstring()?;
        if text.len() > 2000 {
            text = text.chars().take(2000).collect();
        }
        match self.obj()? {
            Slot::New(i) | Slot::Ref(i) if i == 0 || matches!(self.objects.get(&i), Some(Obj::Font) | None) => {}
            Slot::Null => {}
            _ => return Err(Fail),
        }
        self.skip(1)?;
        if self.u32()? != 3 {
            return Err(Fail);
        }
        let mut ends = [DimEnd::Free(DVec3::ZERO); 2];
        for e in &mut ends {
            if self.u32()? != 2 {
                return Err(Fail);
            }
            self.u32()?;
            let p = DVec3::new(self.coord()?, self.coord()?, self.coord()?);
            let v = self.plain_ref()?;
            self.skip(10)?;
            *e = if v == 0 { DimEnd::Free(p) } else { DimEnd::Vertex(v) };
        }
        let mut f = [0.0; 6];
        for v in &mut f {
            *v = self.coord()?;
        }
        self.u32()?; // 1 or 2
        let dist = self.coord()?;
        // zero padding, then the closing 3
        for _ in 0..16 {
            if self.d.get(self.p) == Some(&3) {
                break;
            }
            if self.u8()? != 0 {
                return Err(Fail);
            }
        }
        if self.u32()? != 3 {
            return Err(Fail);
        }
        Ok(Obj::Dim { ends, text, dir: DVec3::new(f[0], f[1], f[2]), dist })
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
                Obj::Instance { .. } | Obj::Dim { .. } => items += 1,
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
    /// dimensions (inches)
    pub dims: Vec<SkpDim>,
    /// dimension records the file had that couldn't be placed (an end on a vertex that isn't in this list)
    pub dims_skipped: usize,
}

/// A linear dimension as the file has it: the two points it measures, and where its line is drawn.
#[derive(Debug, Clone)]
pub struct SkpDim {
    pub a: DVec3,
    pub b: DVec3,
    /// the drawn line's offset from the points
    pub offset: DVec3,
    /// text typed over the measured length (empty: none)
    pub text: String,
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
            Some(Obj::Text) => out.dims_skipped += 1,
            Some(Obj::Dim { ends, text, dir, dist }) => {
                let at = |e: &DimEnd| match e {
                    DimEnd::Free(p) => Some(*p),
                    DimEnd::Vertex(v) => match r.objects.get(v) {
                        Some(Obj::Vertex(p)) => Some(*p),
                        _ => None,
                    },
                };
                match (at(&ends[0]), at(&ends[1])) {
                    (Some(a), Some(b)) if out.dims.len() < MAX_DIMS => {
                        let d = if dir.length() > 1e-9 { dir.normalize() } else { DVec3::ZERO };
                        out.dims.push(SkpDim { a, b, offset: d * *dist, text: text.clone() });
                    }
                    _ => out.dims_skipped += 1,
                }
            }
            Some(Obj::Instance { def, m }) => {
                if let Some(d) = def.index() {
                    // The file stores the basis as three rows (checked on a 2016 file: read as columns, a
                    // rotated part's corners touch its neighbours' 57 times in 4,538, as rows 1,066 times);
                    // glam takes columns, the same fix #71 made in the modern reader.
                    let t = DMat4::from_cols(
                        DVec4::new(m[0], m[3], m[6], 0.0),
                        DVec4::new(m[1], m[4], m[7], 0.0),
                        DVec4::new(m[2], m[5], m[8], 0.0),
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
fn learn(d: &[u8], newf: bool) -> HashMap<u32, Kind> {
    // bytes the 2017+ id field adds after an entity's attribute slot
    let idlen = |at: usize| -> usize {
        if !newf {
            return 0;
        }
        match d.get(at) {
            Some(0) | None => 1,
            Some(t) => 1 + t.count_ones() as usize,
        }
    };
    let mut known = HashMap::new();
    let headers = class_headers(d);
    if let Some(&(off, _)) = headers.iter().find(|(_, n)| n == "CVertex") {
        let after = off + 6 + 7 + 2 + idlen(off + 6 + 7 + 2) + 24;
        if after + 2 <= d.len() {
            let tag = u16::from_le_bytes([d[after], d[after + 1]]);
            if tag & 0x8000 != 0 && tag != 0xFFFF {
                let v = (tag & 0x7FFF) as u32;
                known.insert(v, Kind::Vertex);
                if v >= 2 {
                    known.insert(v - 2, Kind::Edge);
                }
            }
        }
    }
    if let Some(&(off, _)) = headers.iter().find(|(_, n)| n == "CEdgeUse") {
        let body = off + 6 + 8 + idlen(off + 6 + 8 + 2);
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

/// Where a 2017+ definition header's id sits when something other than the
/// plain 2 bytes separates it from the entity count at `i`: the count is
/// preceded by zero padding, an empty text (ff fe ff 00) and a flag byte,
/// and before that comes the u32 id.
fn def_header_id(d: &[u8], i: usize) -> Option<u32> {
    let mut s = i;
    while s > 0 && i - s < 64 && d[s - 1] == 0 {
        s -= 1;
    }
    // the zeros include the text's length byte; the text starts 3 bytes before
    if s < 9 || &d[s - 3..s] != b"\xff\xfe\xff" {
        return None;
    }
    let at = s - 3 - 1 - 4;
    let x = u32::from_le_bytes(d[at..at + 4].try_into().ok()?);
    (x > 0 && x < 0x7FFF_FFFF).then_some(x)
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
        let mut r = Reader { d, p, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0, newf: false, log: false, jc: Vec::new(), jo: Vec::new() };
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
    if gave_up() {
        return (0, None);
    }
    let mut k = 0;
    while k < n {
        if r.obj().is_err() || (k % 256 == 255 && out_of_time()) {
            break;
        }
        k += 1;
    }
    PROBED.with(|w| w.set(w.get() + k + 1 + 3 * r.objects.len()));
    (k, r.validate().map(|(good, _)| good))
}

thread_local! {
    /// items read by the trial parses that look for where a list starts (all lists together)
    static PROBED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PROBE_LIMIT: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
}

/// Real files need up to about a tenth of an item per byte of trial parsing
/// and under a hundred look-aheads; a damaged one sends the search through
/// every byte and every counter, so it is cut off at a generous multiple of
/// that (a few tens of milliseconds) instead of at the deadline.
fn gave_up() -> bool {
    PROBED.with(|w| w.get()) > PROBE_LIMIT.with(|l| l.get()) || WORK.with(|w| w.get()) > MAX_LOOK_AHEAD
}

// How long one file may take: a damaged file sends the look-ahead searching
// everywhere, so the whole parse has a deadline (real files take well under a second).
thread_local! {
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// The look-ahead attempts one file may make in all (real files use under a
/// hundred; a damaged file would otherwise try millions and look hung).
const MAX_LOOK_AHEAD: usize = 4_000;

thread_local! {
    /// look-ahead attempts made so far in this parse (all lists together)
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn out_of_time() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|t| std::time::Instant::now() > t))
}

/// Parse `n` list items. When an item can't be decoded (a dimension, a text
/// label...), look ahead for a plausible next item where the rest of the list
/// parses cleanly, allowing for the object indices the skipped item used.
fn parse_items<'a>(mut r: Reader<'a>, n: usize, budget: usize, tries: &mut usize) -> Option<(Reader<'a>, Vec<Slot>)> {
    let mut items = Vec::with_capacity(n.min(1 << 20));
    for k in 0..n {
        if out_of_time() || (budget < 4 && gave_up()) {
            return None;
        }
        if budget < 4 {
            // a trial parse of the rest of a list after a look-ahead
            PROBED.with(|w| w.set(w.get() + 1));
        }
        let start = r.p;
        let counter = r.counter;
        r.log = true;
        r.jc.clear();
        r.jo.clear();
        match r.obj() {
            Ok(s) => items.push(s),
            Err(_) => {
                r.rewind();
                let (classes, objects) = (&r.classes, &r.objects);
                if std::env::var_os("SKP_DEBUG").is_some() && budget == 4 {
                    eprintln!("  item {k}/{n} at {start:#x} undecodable (counter {counter:#x}); looking ahead");
                }
                if budget == 0 {
                    return None;
                }
                let rest = n - k - 1;
                let d = r.d;
                let newf = r.newf;
                let limit = (start + 32 * 1024).min(d.len().saturating_sub(2));
                // a plausible item start: a new object of a class we decode, or a
                // reference to an edge, face or instance we already have
                let plausible = |q: usize| -> bool {
                    let t = u16::from_le_bytes([d[q], d[q + 1]]);
                    if t == 0xFFFF {
                        let nl = d.get(q + 4..q + 6).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0);
                        return d.get(q + 6..q + 6 + nl).map_or(false, |nm| matches!(nm, b"CEdge" | b"CFace" | b"CComponentInstance" | b"CGroup" | b"CImage" | b"CConstructionPoint"));
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
                            let work = WORK.with(|w| {
                                w.set(w.get() + 1);
                                w.get()
                            });
                            if *tries > 60_000 || work > MAX_LOOK_AHEAD || (*tries % 64 == 0 && out_of_time()) {
                                return None;
                            }
                            let want = rest - skipped;
                            let fresh = || Reader { d, p: q, counter: counter + extra, classes: classes.clone(), names: HashMap::new(), objects: objects.clone(), depth: 0, newf, log: false, jc: Vec::new(), jo: Vec::new() };
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
    let mut r = Reader { d, p: 0x20, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0, newf: false, log: false, jc: Vec::new(), jo: Vec::new() };
    file.version = r.cstring().unwrap_or_default();
    let major: u32 = file.version.trim_start_matches('{').split('.').next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let newf = major >= 17;
    // How long the whole read may take, either kind of file: 4 s, plus 4 s per megabyte, never more than 30 s.
    let allowed = std::time::Duration::from_millis((4000 + (d.len() as u64 * 4000 >> 20)).min(30_000));
    let deadline = std::time::Instant::now() + allowed;
    DEADLINE.with(|c| c.set(Some(deadline)));
    WORK.with(|w| w.set(0));
    PROBED.with(|w| w.set(0));
    PROBE_LIMIT.with(|l| l.set(300_000 + 4 * d.len()));
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            DEADLINE.with(|c| c.set(None));
        }
    }
    let _clear = Clear;
    // 2021 and later wrap the model in a ZIP; up to 2020 it is the classic
    // binary layout this reader handles (checked on 2013-2016 files)
    if crate::skp_vff::zip_start(d).is_some() {
        return crate::skp_vff::parse(d, file.version);
    }

    let mut known = learn(d, newf);
    file.materials = materials(d);
    let geometry_tag = |known: &HashMap<u32, Kind>, t: u16, i: usize| -> bool {
        if t == 0xFFFF {
            let n = d.get(i + 4..i + 6).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0);
            return d.get(i + 6..i + 6 + n).map_or(false, |name| matches!(name, b"CEdge" | b"CFace" | b"CComponentInstance" | b"CGroup" | b"CImage"));
        }
        t & 0x8000 != 0 && matches!(known.get(&((t & 0x7FFF) as u32)), Some(Kind::Edge | Kind::Face | Kind::Instance))
    };

    let new_reader = |known: &HashMap<u32, Kind>, at: usize, base: u32| Reader { d, p: at, counter: base, classes: known.clone(), names: HashMap::new(), objects: HashMap::new(), depth: 0, newf, log: false, jc: Vec::new(), jo: Vec::new() };
    // Pick the starting index from the cleanly-parsing prefix, then parse fully
    // (skipping undecodable items) with that one.
    let try_bases = |known: &HashMap<u32, Kind>, at: usize, n: u32, bases: &mut dyn Iterator<Item = u32>| -> Option<(Reader, Vec<Slot>, usize)> {
        let mut best: Option<(usize, usize, u32)> = None; // (items parsed, good refs, base)
        for base in bases {
            if gave_up() {
                return None;
            }
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
    // per definition: the object count when the previous list ended (its own object comes after)
    let mut floors: HashMap<u32, u32> = HashMap::new();
    while i + 12 < d.len() {
        if gave_up() {
            break;
        }
        if out_of_time() {
            bail!("this SketchUp file looks damaged: SketchCraft stopped reading it after {} s", allowed.as_secs());
        }
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
                let floor = last_counter;
                last_counter = last_counter.max(r.counter);
                if std::env::var_os("SKP_DEBUG").is_some() {
                    eprintln!("def list @{i:#x} def={x:#x} n={n} end={:#x} counter={:#x}", r.p, r.counter);
                }
                let ents = resolve(&r, &items);
                let name = (r.p..(r.p + 48).min(d.len().saturating_sub(4)))
                    .find(|&k| &d[k..k + 3] == b"\xff\xfe\xff")
                    .and_then(|k| {
                        let mut nr = Reader { d, p: k, counter: 0, classes: HashMap::new(), names: HashMap::new(), objects: HashMap::new(), depth: 0, newf, log: false, jc: Vec::new(), jo: Vec::new() };
                        nr.cstring().ok()
                    })
                    .unwrap_or_default();
                // X is one past the definition's own object index
                file.definitions.insert(x - 1, (name, ents));
                floors.insert(x - 1, floor);
                i = r.p;
                continue;
            }
        }
        // 2017+: a definition's header may carry more between its id and its
        // count (a flag, a text, padding): [u32 x][u8][text][zeros][u32 n][object]
        if newf && (1..=2_000_000).contains(&x) {
            if let Some(xd) = def_header_id(d, i) {
                let t = u16::from_le_bytes([d[i + 4], d[i + 5]]);
                if geometry_tag(&known, t, i + 4) {
                    if let Some((r, items, _)) = try_bases(&known, i + 4, x, &mut [xd + 1, xd + 2, xd + 3].into_iter()) {
                        for (k, v) in &r.classes {
                            known.insert(*k, *v);
                        }
                        let floor = last_counter;
                        last_counter = last_counter.max(r.counter);
                        let ents = resolve(&r, &items);
                        file.definitions.insert(xd - 1, (String::new(), ents));
                        floors.insert(xd - 1, floor);
                        i = r.p;
                        continue;
                    }
                }
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
    if gave_up() {
        bail!("this SketchUp file looks damaged: SketchCraft couldn't make sense of its contents");
    }
    if file.definitions.is_empty() && file.roots.is_empty() {
        bail!("no geometry found in this SketchUp file");
    }
    // A placement names its definition's object index. The list header only says
    // where the definition's entities start, which is further on when the
    // definition carries attribute dictionaries (their objects take indices
    // too). The definition's own object comes after the previous list ended, so
    // a reference means the first definition whose list starts at or after it,
    // as long as the reference isn't before that point.
    let keys: Vec<u32> = file.definitions.keys().copied().collect();
    if std::env::var_os("SKP_DEBUG").is_some() {
        eprintln!("definition keys {keys:x?}");
        for (k, (_, e)) in &file.definitions {
            eprintln!("  def {k:#x} (floor {:#x}): {} faces, places {:x?}", floors.get(k).copied().unwrap_or(0), e.faces.len(), e.instances.iter().map(|i| i.0).collect::<Vec<_>>());
        }
        for e in &file.roots {
            eprintln!("  root: {} faces, places {:x?}", e.faces.len(), e.instances.iter().map(|i| i.0).collect::<Vec<_>>());
        }
    }
    let fix = |d: u32| -> u32 {
        match keys.iter().copied().find(|k| *k >= d) {
            Some(k) if floors.get(&k).map_or(true, |f| d >= *f) => k,
            _ => d,
        }
    };
    for (_, ents) in file.definitions.values_mut() {
        for (d, _) in &mut ents.instances {
            *d = fix(*d);
        }
    }
    for ents in &mut file.roots {
        for (d, _) in &mut ents.instances {
            *d = fix(*d);
        }
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
        m.definitions.insert(id, Definition { name: label, model: Default::default(), attrs: Default::default(), source: None, cuts: false, face_me: false });
    }
    for (&key, (_, ents)) in &file.definitions {
        let mut dm = Model::default();
        summary.faces += fill(&mut dm, ents, &file);
        for (child, t) in &ents.instances {
            if let Some(&cid) = def_ids.get(child) {
                let iid = dm.new_id();
                dm.instances.insert(iid, Instance { parent: None, def: cid, name: "part".into(), transform: s * *t * s_inv, tag: None, hidden: false, attrs: Default::default() });
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
    let mut root_faces = 0;
    for root in &file.roots {
        let mut tmp = Model::default();
        let n = fill(&mut tmp, root, &file);
        root_faces += n;
        summary.faces += n;
        merge_into(m, &tmp, gid, base);
        let to_world = |p: DVec3| at + p * INCH;
        summary.skipped_annotations += root.dims_skipped;
        for dim in &root.dims {
            let (a, b, off) = (to_world(dim.a), to_world(dim.b), dim.offset * INCH);
            if !(a.is_finite() && b.is_finite() && off.is_finite()) {
                continue;
            }
            let id = m.new_id();
            m.annotations.push(Annotation::Dimension { id, a, b, offset: off });
            summary.dimensions += 1;
            if !dim.text.is_empty() {
                let id = m.new_id();
                m.annotations.push(Annotation::Text { id, at: (a + b) * 0.5 + off, text: dim.text.clone(), leader: None });
                summary.labels += 1;
            }
        }
        for (child, t) in &root.instances {
            if let Some(&cid) = def_ids.get(child) {
                let iid = m.new_id();
                m.instances.insert(iid, Instance { parent: None, def: cid, name: format!("{name} part"), transform: base * s * *t * s_inv, tag: None, hidden: false, attrs: Default::default() });
                placed += 1;
            }
        }
    }
    m.active_group = None;
    if placed == 0 && root_faces == 0 {
        // nothing in the model's own list draws (no list found, or only loose
        // lines): show every definition that nothing else places
        let used: HashSet<u32> = file.definitions.values().flat_map(|(_, e)| e.instances.iter().map(|(d, _)| *d)).collect();
        for key in file.definitions.keys().filter(|k| !used.contains(k)) {
            let iid = m.new_id();
            m.instances.insert(iid, Instance { parent: None, def: def_ids[key], name: name.to_string(), transform: base, tag: None, hidden: false, attrs: Default::default() });
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
    /// dimensions and text labels made from the file's annotations
    pub dimensions: usize,
    pub labels: usize,
    /// annotation records found but not imported (not attached to anything we read)
    pub skipped_annotations: usize,
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
            m.materials.entry(key.clone()).or_insert(crate::model::Material { color: *rgb, texture, opacity: 1.0, ..Default::default() });
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
