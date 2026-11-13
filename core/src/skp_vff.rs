//! Modern SketchUp files (2013 and later, checked against SketchUp 26.2).
//!
//! Clean-room reader, worked out from sample files for interoperability.
//! After the usual UTF-16 "SketchUp Model" + version header comes a short
//! "VFF" marker and a ZIP archive holding:
//!
//! - `model.dat`: a tree of records, each `[tag u16][length u32][body]`. A
//!   body is either raw bytes or more records. Integers (ids, counts) are
//!   little-endian of whatever length the record gives.
//! - `materials/<name>/material.xml`: color, and the image file of textured
//!   materials.
//!
//! Inside `model.dat` (tag paths from the root record 0x1f4):
//!
//! ```text
//! 0x1f7 .. 0x32c8          material: 0x5dc/0x5de id, 0x32cc name
//! 0x1f9/0x1770/0x1771/0x157c   component definition
//!     0x157e               its GUID (text)
//!     0x1388               its entities:
//!         0x1389/0x9c4     vertex: 0x5dc/0x5de id, 0x9c5 x,y,z (f64 inches)
//!         0x138a/0xbb8     edge: 0x7d0 header (0x5dc/0x5de id, 0x7d3 flags),
//!                          0xbb9 start vertex id, 0xbba end vertex id
//!         0x138b/0xdac     face: 0x7d0 header (id; 0x7d1 material id),
//!                          0xdad plane, 0xdae/0x1194 loops (first is outer),
//!                          each 0x1195/0xfa0 edge use: 0xfa1 edge id,
//!                          0xfa2 reversed
//!         0x138d/0x1d4c/0x1964  instance: 0x1965 definition GUID,
//!                          0x1966 13 f64 (x axis, y axis, z axis, origin, scale)
//! 0x1f6/0x1388             the model's own entities (same layout)
//! ```
//!
//! Face records of textured faces carry 0x5dd/../0x2715 texture matrices;
//! their scale gives the tile size. Edge flag bit 0x10 marks soft edges.

use crate::skp::{Entities, SkpFace, SkpFile};
use anyhow::{bail, Context, Result};
use glam::{DMat4, DVec3, DVec4};
use std::collections::HashMap;
use std::io::{Cursor, Read};

#[derive(Clone, Copy)]
struct Node<'a> {
    d: &'a [u8],
    tag: u16,
}

impl<'a> Node<'a> {
    /// The records inside this one, or none if its body isn't records.
    fn kids(&self) -> Vec<Node<'a>> {
        let (d, mut p, mut out) = (self.d, 0usize, Vec::new());
        while p < d.len() {
            if p + 6 > d.len() {
                return Vec::new();
            }
            let tag = u16::from_le_bytes([d[p], d[p + 1]]);
            let n = u32::from_le_bytes([d[p + 2], d[p + 3], d[p + 4], d[p + 5]]) as usize;
            if n > d.len() - p - 6 {
                return Vec::new();
            }
            out.push(Node { d: &d[p + 6..p + 6 + n], tag });
            p += 6 + n;
        }
        out
    }
    fn get(&self, tag: u16) -> Option<Node<'a>> {
        self.kids().into_iter().find(|k| k.tag == tag)
    }
    fn all(&self, tag: u16) -> Vec<Node<'a>> {
        self.kids().into_iter().filter(|k| k.tag == tag).collect()
    }
    fn at(&self, path: &[u16]) -> Option<Node<'a>> {
        path.iter().try_fold(*self, |n, t| n.get(*t))
    }
    fn num(&self) -> u64 {
        self.d.iter().take(8).rev().fold(0, |a, b| (a << 8) | *b as u64)
    }
    fn f64s(&self) -> Vec<f64> {
        self.d.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(self.d).trim_end_matches('\0').to_string()
    }
    /// Every record with `tag` anywhere below this one.
    fn find_all(&self, tag: u16, out: &mut Vec<Node<'a>>) {
        for k in self.kids() {
            if k.tag == tag {
                out.push(k);
            } else if k.d.len() >= 6 {
                k.find_all(tag, out);
            }
        }
    }
}

fn id_of(n: &Node) -> Option<u64> {
    n.at(&[0x5dc, 0x5de]).map(|x| x.num())
}

/// Where a modern file's archive starts, if this is one.
pub fn zip_start(d: &[u8]) -> Option<usize> {
    let head = &d[..d.len().min(256)];
    let v = head.windows(3).position(|w| w == b"VFF")?;
    head[v..].windows(4).position(|w| w == b"PK\x03\x04").map(|p| v + p)
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub data: String,
    /// tile width in meters
    pub tile: f64,
    /// image height / width
    pub aspect: f64,
}

pub fn parse(d: &[u8], version: String) -> Result<SkpFile> {
    let start = zip_start(d).context("not a SketchUp 2013+ archive")?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&d[start..])).context("reading the SketchUp archive")?;
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut f = zip.by_name(name).ok()?;
        let mut v = Vec::new();
        f.read_to_end(&mut v).ok()?;
        Some(v)
    };
    let model = read("model.dat").context("the archive has no model.dat")?;
    if model.len() < 6 {
        bail!("empty model.dat");
    }
    let root = Node { d: &model[..], tag: 0 };
    let top = root.get(0x1f4).context("model.dat has an unexpected layout")?;

    // materials: id -> name; color and image from the archive's material.xml
    let mut file = SkpFile { version, ..Default::default() };
    let mut textures: HashMap<u32, Texture> = HashMap::new();
    let mut recs = Vec::new();
    top.find_all(0x32c8, &mut recs);
    let mut mat_names: HashMap<u64, String> = HashMap::new();
    for r in recs {
        let (Some(id), Some(name)) = (id_of(&r), r.get(0x32cc)) else { continue };
        mat_names.insert(id, name.text());
    }

    // definitions and the model's entities
    let defs = top.at(&[0x1f9, 0x1770, 0x1771]).map(|n| n.all(0x157c)).unwrap_or_default();
    let mut keys: HashMap<String, u32> = HashMap::new();
    for (i, def) in defs.iter().enumerate() {
        if let Some(g) = def.get(0x157e) {
            keys.insert(g.text(), i as u32 + 1);
        }
    }
    let mut tiles: HashMap<u64, Vec<f64>> = HashMap::new();
    for (i, def) in defs.iter().enumerate() {
        let ents = def.get(0x1388).map(|e| entities(&e, &keys, &mut tiles)).unwrap_or_default();
        file.definitions.insert(i as u32 + 1, (String::new(), ents));
    }
    if let Some(e) = top.at(&[0x1f6, 0x1388]) {
        file.roots.push(entities(&e, &keys, &mut tiles));
    }

    for (&id, name) in &mat_names {
        let Some(xml) = read(&format!("materials/{name}/material.xml")) else { continue };
        let xml = String::from_utf8_lossy(&xml).to_string();
        let attr = |k: &str| -> Option<String> {
            let i = xml.find(&format!(" {k}=\""))? + k.len() + 3;
            Some(xml[i..i + xml[i..].find('"')?].to_string())
        };
        let ch = |k: &str| attr(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(200.0) / 255.0;
        let color = [ch("colorRed"), ch("colorGreen"), ch("colorBlue")];
        file.materials.insert(id as u32, (name.trim_start_matches("_LC_").to_string(), color));
        if attr("hasTexture").as_deref() == Some("1") {
            // the base image: the first <mat:image path="./...">
            let img = xml.find("<mat:image ").and_then(|i| {
                let j = xml[i..].find("path=\"./")? + i + 8;
                Some(xml[j..j + xml[j..].find('"')?].to_string())
            });
            let bytes = img.and_then(|p| read(&format!("materials/{name}/{p}")));
            if let Some((data, aspect)) = bytes.and_then(|b| shrink(&b)) {
                let mut t = tiles.remove(&id).unwrap_or_default();
                t.sort_by(|a, b| a.total_cmp(b));
                let inches = t.get(t.len() / 2).copied().filter(|v| *v > 0.5 && *v < 2000.0).unwrap_or(39.37);
                textures.insert(id as u32, Texture { data, tile: inches * 0.0254, aspect });
            }
        }
    }
    if file.definitions.values().all(|(_, e)| e.faces.is_empty()) && file.roots.iter().all(|e| e.faces.is_empty()) {
        bail!("no geometry found in this SketchUp file");
    }
    file.textures = textures;
    Ok(file)
}

/// An image as a data URL no bigger than 1024 px (JPEG unless it has alpha).
fn shrink(bytes: &[u8]) -> Option<(String, f64)> {
    use base64::Engine as _;
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width().max(img.height()) > 1024 { img.thumbnail(1024, 1024) } else { img };
    let mut out = Cursor::new(Vec::new());
    let mime = if img.color().has_alpha() {
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        "png"
    } else {
        image::DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut out, image::ImageFormat::Jpeg).ok()?;
        "jpeg"
    };
    let aspect = img.height() as f64 / img.width().max(1) as f64;
    Some((format!("data:image/{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(out.into_inner())), aspect))
}

fn entities(e: &Node, keys: &HashMap<String, u32>, tiles: &mut HashMap<u64, Vec<f64>>) -> Entities {
    let mut out = Entities::default();
    let mut verts: HashMap<u64, DVec3> = HashMap::new();
    for list in e.all(0x1389) {
        for v in list.all(0x9c4) {
            let (Some(id), Some(p)) = (id_of(&v), v.get(0x9c5)) else { continue };
            let p = p.f64s();
            if p.len() >= 3 {
                verts.insert(id, DVec3::new(p[0], p[1], p[2]));
            }
        }
    }
    let mut edges: HashMap<u64, (u64, u64, u64)> = HashMap::new();
    for list in e.all(0x138a) {
        for ed in list.all(0xbb8) {
            let Some(h) = ed.get(0x7d0) else { continue };
            let (Some(id), Some(a), Some(b)) = (id_of(&h), ed.get(0xbb9), ed.get(0xbba)) else { continue };
            let flags = h.get(0x7d3).map(|f| f.num()).unwrap_or(0);
            edges.insert(id, (a.num(), b.num(), flags));
        }
    }
    let mut used = std::collections::HashSet::new();
    for list in e.all(0x138b) {
        for f in list.all(0xdac) {
            let h = f.get(0x7d0);
            let material = h.and_then(|h| h.get(0x7d1)).map(|m| m.num()).unwrap_or(0);
            let mut rings: Vec<Vec<DVec3>> = Vec::new();
            for lp in f.get(0xdae).map(|l| l.all(0x1194)).unwrap_or_default() {
                let mut ring = Vec::new();
                for u in lp.get(0x1195).map(|x| x.all(0xfa0)).unwrap_or_default() {
                    let (Some(eid), rev) = (u.get(0xfa1).map(|x| x.num()), u.get(0xfa2).map(|x| x.num()).unwrap_or(0)) else { continue };
                    let Some(&(a, b, _)) = edges.get(&eid) else { continue };
                    used.insert(eid);
                    if let Some(p) = verts.get(if rev != 0 { &b } else { &a }) {
                        ring.push(*p);
                    }
                }
                if ring.len() >= 3 {
                    rings.push(ring);
                }
            }
            if rings.is_empty() {
                continue;
            }
            // texture matrix scale -> tile size in inches
            let mut ms = Vec::new();
            f.find_all(0x2715, &mut ms);
            if let Some(m) = ms.first().map(|m| m.f64s()) {
                if m.len() >= 6 {
                    let s = DVec3::new(m[0], m[1], m[2]).length().max(DVec3::new(m[3], m[4], m[5]).length());
                    tiles.entry(material).or_default().push(s);
                }
            }
            let outer = rings.remove(0);
            out.faces.push(SkpFace { outer, holes: rings, material: material as u32 });
        }
    }
    // soft flags for the faces' edges, and loose lines
    for (id, (a, b, flags)) in &edges {
        let (Some(pa), Some(pb)) = (verts.get(a), verts.get(b)) else { continue };
        let soft = flags & 0x10 != 0;
        if soft || !used.contains(id) {
            out.lines.push((*pa, *pb, soft));
        }
    }
    for list in e.all(0x138d) {
        for g in list.all(0x1d4c) {
            for i in g.all(0x1964) {
                let Some(key) = i.get(0x1965).and_then(|g| keys.get(&g.text())) else { continue };
                let m = i.get(0x1966).map(|m| m.f64s()).unwrap_or_default();
                let t = if m.len() >= 13 {
                    let s = if m[12].abs() > 1e-12 { 1.0 / m[12] } else { 1.0 };
                    DMat4::from_cols(
                        DVec4::new(m[0], m[1], m[2], 0.0),
                        DVec4::new(m[3], m[4], m[5], 0.0),
                        DVec4::new(m[6], m[7], m[8], 0.0),
                        DVec4::new(m[9] * s, m[10] * s, m[11] * s, 1.0),
                    )
                } else {
                    DMat4::IDENTITY
                };
                out.instances.push((*key, t));
            }
        }
    }
    out
}
