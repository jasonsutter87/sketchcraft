//! More ways out: Collada (.dae, what SketchUp itself writes for other 3D
//! apps), 3D DXF (3DFACEs, millimeters, one layer per object) and PDF (a
//! rendered view on a landscape A4 sheet with a title line).

use crate::components::WFace;
use crate::geom;
use crate::model::Model;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fmt::Write as _;

const DEFAULT: [f32; 3] = [0.95, 0.94, 0.91];

/// Faces by object (group name, component placement, or "Ungrouped").
fn objects(m: &Model) -> BTreeMap<String, Vec<WFace>> {
    let mut out: BTreeMap<String, Vec<WFace>> = BTreeMap::new();
    for f in m.world_faces() {
        let name = if let Some(i) = m.instances.get(&f.owner) {
            format!("{} {}", i.name, f.owner)
        } else {
            f.group.and_then(|g| m.groups.get(&g)).map(|g| format!("{} {}", g.name, f.group.unwrap())).unwrap_or_else(|| "Ungrouped".into())
        };
        out.entry(name).or_default().push(f);
    }
    out
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn write_to(path: &str, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Collada 1.4.1, Z up, meters: one geometry per object, one material per color.
pub fn write_dae(m: &Model, path: &str) -> Result<usize> {
    let objs = objects(m);
    let mut colors: Vec<[f32; 3]> = vec![DEFAULT];
    let color_of = |f: &WFace| f.color.unwrap_or(DEFAULT);
    for fs in objs.values() {
        for f in fs {
            let c = color_of(f);
            if !colors.contains(&c) {
                colors.push(c);
            }
        }
    }
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<COLLADA xmlns=\"http://www.collada.org/2005/11/COLLADASchema\" version=\"1.4.1\">\n");
    s += "<asset><contributor><authoring_tool>SketchCraft</authoring_tool></contributor><unit name=\"meter\" meter=\"1\"/><up_axis>Z_UP</up_axis></asset>\n<library_effects>\n";
    for (i, c) in colors.iter().enumerate() {
        writeln!(s, "<effect id=\"e{i}\"><profile_COMMON><technique sid=\"common\"><lambert><diffuse><color>{} {} {} 1</color></diffuse></lambert></technique></profile_COMMON></effect>", c[0], c[1], c[2])?;
    }
    s += "</library_effects>\n<library_materials>\n";
    for i in 0..colors.len() {
        writeln!(s, "<material id=\"m{i}\" name=\"{}\"><instance_effect url=\"#e{i}\"/></material>", crate::export::color_id(colors[i]))?;
    }
    s += "</library_materials>\n<library_geometries>\n";
    let mut tris = 0;
    let mut nodes = String::new();
    for (gi, (name, faces)) in objs.iter().enumerate() {
        let mut pos: Vec<f64> = Vec::new();
        let mut by_mat: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for f in faces {
            let mi = colors.iter().position(|c| *c == color_of(f)).unwrap_or(0);
            for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
                let base = pos.len() / 3;
                for p in t {
                    pos.extend([p.x, p.y, p.z]);
                }
                by_mat.entry(mi).or_default().extend([base, base + 1, base + 2]);
                tris += 1;
            }
        }
        let n = pos.len();
        writeln!(s, "<geometry id=\"g{gi}\" name=\"{}\"><mesh>", xml(name))?;
        writeln!(
            s,
            "<source id=\"g{gi}-p\"><float_array id=\"g{gi}-pa\" count=\"{n}\">{}</float_array><technique_common><accessor source=\"#g{gi}-pa\" count=\"{}\" stride=\"3\"><param name=\"X\" type=\"float\"/><param name=\"Y\" type=\"float\"/><param name=\"Z\" type=\"float\"/></accessor></technique_common></source>",
            pos.iter().map(|v| format!("{v:.6}")).collect::<Vec<_>>().join(" "),
            n / 3
        )?;
        writeln!(s, "<vertices id=\"g{gi}-v\"><input semantic=\"POSITION\" source=\"#g{gi}-p\"/></vertices>")?;
        for (mi, idx) in &by_mat {
            writeln!(
                s,
                "<triangles material=\"m{mi}\" count=\"{}\"><input semantic=\"VERTEX\" source=\"#g{gi}-v\" offset=\"0\"/><p>{}</p></triangles>",
                idx.len() / 3,
                idx.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(" ")
            )?;
        }
        s += "</mesh></geometry>\n";
        let binds: String = by_mat.keys().map(|mi| format!("<instance_material symbol=\"m{mi}\" target=\"#m{mi}\"/>")).collect();
        writeln!(nodes, "<node id=\"n{gi}\" name=\"{}\"><instance_geometry url=\"#g{gi}\"><bind_material><technique_common>{binds}</technique_common></bind_material></instance_geometry></node>", xml(name))?;
    }
    s += "</library_geometries>\n<library_visual_scenes><visual_scene id=\"scene\" name=\"SketchCraft\">\n";
    s += &nodes;
    s += "</visual_scene></library_visual_scenes>\n<scene><instance_visual_scene url=\"#scene\"/></scene>\n</COLLADA>\n";
    write_to(path, s.as_bytes())?;
    Ok(tris)
}

/// 3D DXF (R12): every face as 3DFACE triangles, in millimeters, one layer
/// per object.
pub fn write_dxf(m: &Model, path: &str) -> Result<usize> {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    let mut tris = 0;
    for (name, faces) in objects(m) {
        let layer: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        for f in faces {
            for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
                s += "0\n3DFACE\n8\n";
                s += &layer;
                s += "\n";
                for (k, p) in [t[0], t[1], t[2], t[2]].iter().enumerate() {
                    write!(s, "1{k}\n{:.3}\n2{k}\n{:.3}\n3{k}\n{:.3}\n", p.x * 1000.0, p.y * 1000.0, p.z * 1000.0)?;
                }
                tris += 1;
            }
        }
    }
    s += "0\nENDSEC\n0\nEOF\n";
    write_to(path, s.as_bytes())?;
    Ok(tris)
}

/// A one-page PDF: the image fitted on landscape A4, a title line below.
pub fn write_pdf(img: &image::RgbImage, title: &str, path: &str) -> Result<()> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;
    let (w, h) = (img.width(), img.height());
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(img.as_raw())?;
    let data = enc.finish()?;
    // landscape A4 in points, 36 pt margins, 24 pt for the title
    let (pw, ph, margin, foot) = (842.0f64, 595.0f64, 36.0f64, 24.0f64);
    let (aw, ah) = (pw - 2.0 * margin, ph - 2.0 * margin - foot);
    let k = (aw / w as f64).min(ah / h as f64);
    let (iw, ih) = (w as f64 * k, h as f64 * k);
    let (ix, iy) = (margin + (aw - iw) / 2.0, margin + foot + (ah - ih) / 2.0);
    let safe: String = title.chars().filter(|c| c.is_ascii() && !matches!(c, '(' | ')' | '\\')).collect();
    let content = format!("q {iw:.2} 0 0 {ih:.2} {ix:.2} {iy:.2} cm /Im1 Do Q\nBT /F1 10 Tf {margin} {:.2} Td ({safe}) Tj ET\n", margin + 6.0);

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(&mut out, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im1 4 0 R >> /Font << /F1 6 0 R >> >> /Contents 5 0 R >>").as_bytes());
    let mut im = format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", data.len()).into_bytes();
    im.extend(&data);
    im.extend(b"\nendstream");
    obj(&mut out, &im);
    let mut c = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    c.extend(content.as_bytes());
    c.extend(b"endstream");
    obj(&mut out, &c);
    obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    write_to(path, &out)
}
