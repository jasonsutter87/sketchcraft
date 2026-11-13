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
    for (name, mat) in &m.materials {
        writeln!(mtl, "newmtl {}\nKd {} {} {}\n", mat_id(name), mat.color[0], mat.color[1], mat.color[2])?;
    }

    let mut obj = format!("# Riseline export\nmtllib {mtl_name}\n");
    let mut count = 0usize;
    let mut vn = 0usize;
    // group faces by object (group) so Blender gets one object per house
    let mut by_group: std::collections::BTreeMap<Option<u32>, Vec<u32>> = Default::default();
    for (&id, f) in &m.faces {
        by_group.entry(f.group).or_default().push(id);
    }
    for (g, faces) in by_group {
        let name = g.and_then(|g| m.groups.get(&g)).map(|g| g.name.clone()).unwrap_or_else(|| "Ungrouped".into());
        writeln!(obj, "o {}", name.replace(' ', "_"))?;
        for fid in faces {
            let f = &m.faces[&fid];
            let outer = m.pts(&f.outer);
            let holes: Vec<_> = f.holes.iter().map(|h| m.pts(h)).collect();
            let n = geom::newell(&outer);
            writeln!(obj, "usemtl {}", f.material.as_deref().map(mat_id).unwrap_or_else(|| "default".into()))?;
            writeln!(obj, "vn {:.6} {:.6} {:.6}", n.x, n.y, n.z)?;
            vn += 1;
            for t in geom::triangulate(&outer, &holes, n) {
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
