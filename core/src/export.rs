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
