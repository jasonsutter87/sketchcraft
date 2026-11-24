//! 3MF export (the 3D Manufacturing Format core spec): a zip with `[Content_Types].xml`, `_rels/.rels` and
//! `3D/3dmodel.model`. See docs/dev/print-check.md.
//!
//! One `<object>` per group (a nested group is its own object) and one per placed component (each placement is
//! written as its own mesh with its transform applied, so a component placed three times is three objects), each listed
//! once in `<build>`. Vertices are shared inside an object (welded to 0.1 micron) so a solid's every edge is used by
//! exactly two triangles; triangles are wound counter-clockwise seen from outside, as the spec wants. Colours and
//! textures are not written (slicers colour by filament).

use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;

/// The unit the file's coordinates are written in, and how many of them make a meter.
pub fn unit_scale(units: &str, model_units: &str) -> Result<(&'static str, f64)> {
    let u = if units.eq_ignore_ascii_case("model") { model_units } else { units };
    Ok(match u.to_ascii_lowercase().as_str() {
        "mm" | "millimeter" | "millimetre" => ("millimeter", 1000.0),
        "cm" | "centimeter" | "centimetre" => ("centimeter", 100.0),
        "m" | "meter" | "metre" => ("meter", 1.0),
        "in" | "inch" => ("inch", 1.0 / 0.0254),
        "ft" | "foot" | "feet" => ("foot", 1.0 / 0.3048),
        "micron" | "um" => ("micron", 1e6),
        other => bail!("units is millimeter, centimeter, meter, inch, foot, micron or model (got {other})"),
    })
}

struct Mesh {
    name: String,
    verts: Vec<[f64; 3]>,
    tris: Vec<[usize; 3]>,
    index: HashMap<[i64; 3], usize>,
}

impl Mesh {
    fn vertex(&mut self, p: glam::DVec3, scale: f64) -> usize {
        // weld in meters at 0.1 micron, so corners shared by neighbouring faces become one vertex
        let key = [(p.x * 1e7).round() as i64, (p.y * 1e7).round() as i64, (p.z * 1e7).round() as i64];
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        self.verts.push([key[0] as f64 / 1e7 * scale, key[1] as f64 / 1e7 * scale, key[2] as f64 / 1e7 * scale]);
        self.index.insert(key, self.verts.len() - 1);
        self.verts.len() - 1
    }
}

fn num(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

/// A character XML text may hold: not a control, and not a Unicode non-character (U+FFFE, U+FFFF, U+FDD0 to U+FDEF, and
/// the last two of every plane), which XML refuses.
fn xml_char(c: char) -> bool {
    let n = c as u32;
    !c.is_control() && !(0xFDD0..=0xFDEF).contains(&n) && (n & 0xFFFE) != 0xFFFE
}

fn esc(s: &str) -> String {
    s.chars().filter(|c| xml_char(*c)).collect::<String>().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// What was written.
pub struct Written {
    pub objects: usize,
    pub triangles: usize,
    pub vertices: usize,
    pub unit: &'static str,
}

/// The model as the bytes of a 3MF file.
pub fn to_3mf(m: &Model, units: &str) -> Result<(Vec<u8>, Written)> {
    let (unit, scale) = unit_scale(units, &m.units)?;
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Group(Option<Id>),
        Placed(Id),
    }
    let mut meshes: BTreeMap<Key, Mesh> = BTreeMap::new();
    for f in m.world_faces() {
        let key = if f.face.is_some() { Key::Group(f.group) } else { Key::Placed(f.owner) };
        let mesh = meshes.entry(key).or_insert_with(|| {
            let name = match (f.face, f.group) {
                (Some(_), Some(g)) => m.groups.get(&g).map(|g| g.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Group {g}")),
                (Some(_), None) => "Model".to_string(),
                (None, _) => m.instances.get(&f.owner).map(|i| i.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Component {}", f.owner)),
            };
            Mesh { name, verts: vec![], tris: vec![], index: HashMap::new() }
        });
        let n = f.normal;
        for t in geom::triangulate(&f.outer, &f.holes, n) {
            let i = t.map(|p| mesh.vertex(p, scale));
            if i[0] != i[1] && i[1] != i[2] && i[0] != i[2] {
                mesh.tris.push(i);
            }
        }
    }
    meshes.retain(|_, v| !v.tris.is_empty());
    if meshes.is_empty() {
        bail!("there is nothing to export: the model has no faces");
    }
    let mut xml = String::new();
    write!(xml, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"{unit}\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">")?;
    write!(xml, "<metadata name=\"Application\">SketchCraft</metadata><resources>")?;
    let (mut tris, mut verts) = (0, 0);
    for (i, mesh) in meshes.values().enumerate() {
        write!(xml, "<object id=\"{}\" type=\"model\" name=\"{}\"><mesh><vertices>", i + 1, esc(&mesh.name))?;
        for v in &mesh.verts {
            write!(xml, "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", num(v[0]), num(v[1]), num(v[2]))?;
        }
        xml.push_str("</vertices><triangles>");
        for t in &mesh.tris {
            write!(xml, "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2])?;
        }
        xml.push_str("</triangles></mesh></object>");
        tris += mesh.tris.len();
        verts += mesh.verts.len();
    }
    xml.push_str("</resources><build>");
    for i in 0..meshes.len() {
        write!(xml, "<item objectid=\"{}\"/>", i + 1)?;
    }
    xml.push_str("</build></model>");

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [("[Content_Types].xml", CONTENT_TYPES), ("_rels/.rels", RELS), ("3D/3dmodel.model", xml.as_str())] {
            z.start_file(name, opts)?;
            z.write_all(body.as_bytes())?;
        }
        z.finish()?;
    }
    Ok((buf.into_inner(), Written { objects: meshes.len(), triangles: tris, vertices: verts, unit }))
}

/// Write a 3MF file.
pub fn write_3mf(m: &Model, path: &str, units: &str) -> Result<Written> {
    let (bytes, w) = to_3mf(m, units)?;
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    crate::safety::write_atomic(path, &bytes)?;
    Ok(w)
}
