//! Export to Wavefront OBJ + MTL, which Blender, SketchUp-likes and most
//! renderers open. Faces are triangulated (OBJ can't express holes).

use crate::geom;
use crate::model::Model;
use anyhow::Result;
use std::fmt::Write as _;
use std::path::Path;

pub fn write_obj(m: &Model, path: &str) -> Result<usize> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let mtl_path = p.with_extension("mtl");
    let mtl_name = mtl_path.file_name().unwrap().to_string_lossy().to_string();

    let mut mtl = String::new();
    writeln!(mtl, "newmtl default\nKd 0.95 0.94 0.91\n")?;
    for mat in m.materials.values() {
        writeln!(mtl, "newmtl {}\nKd {} {} {}\n", color_id(mat.color), mat.color[0], mat.color[1], mat.color[2])?;
    }

    let mut obj = format!("# Riseline export\nmtllib {mtl_name}\n");
    let mut count = 0usize;
    let mut vn = 0usize;
    // one object per group / component placement, so Blender gets one object per house
    let mut objects: std::collections::BTreeMap<String, Vec<crate::components::WFace>> = Default::default();
    for f in m.world_faces() {
        let name = if let Some(i) = m.instances.get(&f.owner) {
            format!("{}_{}", i.name, f.owner)
        } else {
            f.group.and_then(|g| m.groups.get(&g)).map(|g| g.name.clone()).unwrap_or_else(|| "Ungrouped".into())
        };
        objects.entry(name).or_default().push(f);
    }
    // component materials live in their definitions
    let mut colors: std::collections::BTreeMap<String, [f32; 3]> = Default::default();
    for (_, faces) in &objects {
        for f in faces {
            if let Some(c) = f.color {
                colors.insert(color_id(c), c);
            }
        }
    }
    for (name, c) in &colors {
        if !m.materials.keys().any(|k| mat_id(k) == *name) {
            writeln!(mtl, "newmtl {}\nKd {} {} {}\n", name, c[0], c[1], c[2])?;
        }
    }
    for (name, faces) in objects {
        writeln!(obj, "o {}", name.replace(' ', "_"))?;
        for f in faces {
            let n = f.normal;
            writeln!(obj, "usemtl {}", f.color.map(color_id).unwrap_or_else(|| "default".into()))?;
            writeln!(obj, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z)?;
            vn += 1;
            for t in geom::triangulate(&f.outer, &f.holes, n) {
                for p in t {
                    writeln!(obj, "v {:.6} {:.6} {:.6}", p.x, p.y, p.z)?;
                }
                count += 1;
                let b = count * 3;
                writeln!(obj, "f {}//{vn} {}//{vn} {}//{vn}", b - 2, b - 1, b)?;
            }
        }
    }
    std::fs::write(p, obj)?;
    std::fs::write(&mtl_path, mtl)?;
    Ok(count)
}

fn mat_id(name: &str) -> String {
    name.replace('#', "c_")
}

/// Materials are named by color, so the same color from a component and
/// the main model share one material.
pub fn color_id(c: [f32; 3]) -> String {
    format!("c_{:02x}{:02x}{:02x}", (c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8)
}

/// Binary glTF (.glb): one mesh, one primitive per color. Z-up model is
/// turned to glTF's Y-up. Opens in Blender, three.js, Unity, Unreal, macOS Quick Look.
pub fn write_glb(m: &Model, path: &str) -> Result<usize> {
    use serde_json::json;
    let mut by_color: std::collections::BTreeMap<String, ([f32; 3], Vec<[f32; 3]>, Vec<[f32; 3]>)> = Default::default();
    let mut tris = 0;
    for f in m.world_faces() {
        let c = f.color.unwrap_or([0.95, 0.94, 0.91]);
        let e = by_color.entry(color_id(c)).or_insert((c, vec![], vec![]));
        let n = f.normal;
        let ny = [n.x as f32, n.z as f32, -n.y as f32];
        for t in geom::triangulate(&f.outer, &f.holes, n) {
            for p in t {
                e.1.push([p.x as f32, p.z as f32, -p.y as f32]);
                e.2.push(ny);
            }
            tris += 1;
        }
    }
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut prims, mut mats) = (vec![], vec![], vec![], vec![]);
    for (i, (_, (c, pos, nor))) in by_color.iter().enumerate() {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        for (arr, is_pos) in [(pos, true), (nor, false)] {
            let off = bin.len();
            for v in arr {
                for x in v {
                    bin.extend_from_slice(&x.to_le_bytes());
                }
            }
            views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": arr.len() * 12, "target": 34962}));
            let mut acc = json!({"bufferView": views.len() - 1, "componentType": 5126, "count": arr.len(), "type": "VEC3"});
            if is_pos {
                acc["min"] = json!(lo);
                acc["max"] = json!(hi);
            }
            accessors.push(acc);
        }
        mats.push(json!({"pbrMetallicRoughness": {"baseColorFactor": [c[0], c[1], c[2], 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.85}, "doubleSided": false}));
        prims.push(json!({"attributes": {"POSITION": accessors.len() - 2, "NORMAL": accessors.len() - 1}, "material": i}));
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "Riseline"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "Riseline model"}],
        "meshes": [{"primitives": prims}],
        "materials": mats,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    let mut js = serde_json::to_vec(&doc)?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, out)?;
    Ok(tris)
}

/// Binary STL for 3D printing (meters; most slicers want mm, so pass scale 1000).
pub fn write_stl(m: &Model, path: &str, scale: f64) -> Result<usize> {
    let mut tris: Vec<([f32; 3], [[f32; 3]; 3])> = Vec::new();
    for f in m.world_faces() {
        let n = f.normal;
        for t in geom::triangulate(&f.outer, &f.holes, n) {
            let v = t.map(|p| [(p.x * scale) as f32, (p.y * scale) as f32, (p.z * scale) as f32]);
            tris.push(([n.x as f32, n.y as f32, n.z as f32], v));
        }
    }
    let mut out = vec![0u8; 80];
    out[..21].copy_from_slice(b"Riseline binary STL  ");
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for (n, v) in &tris {
        for x in n.iter().chain(v.iter().flatten()) {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, out)?;
    Ok(tris.len())
}
