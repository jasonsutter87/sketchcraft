//! OBJ import: each `o`/`g` becomes a group. Most OBJ files are Y-up
//! (Blender's default export); `up: "y"` turns them to SketchCraft's Z-up.

use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;

pub struct ImportResult {
    pub groups: Vec<Id>,
    pub faces: usize,
    pub skipped: usize,
}

pub fn import_obj(m: &mut Model, text: &str, y_up: bool, scale: f64, at: DVec3) -> Result<ImportResult> {
    let mut verts: Vec<DVec3> = Vec::new();
    let mut res = ImportResult { groups: vec![], faces: 0, skipped: 0 };
    let mut current: Option<Id> = None;
    let start_group = |m: &mut Model, name: &str, res: &mut ImportResult| -> Id {
        let g = m.begin_group(name.to_string());
        res.groups.push(g);
        g
    };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f64> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() < 3 {
                    bail!("bad vertex line: {line}");
                }
                let p = if y_up { DVec3::new(c[0], -c[2], c[1]) } else { DVec3::new(c[0], c[1], c[2]) };
                verts.push(p * scale + at);
            }
            Some("o") | Some("g") => {
                let name = it.collect::<Vec<_>>().join(" ");
                current = Some(start_group(m, if name.is_empty() { "Imported" } else { &name }, &mut res));
            }
            Some("f") => {
                if current.is_none() {
                    current = Some(start_group(m, "Imported", &mut res));
                }
                let idx: Vec<usize> = it
                    .filter_map(|t| t.split('/').next()?.parse::<i64>().ok())
                    .map(|i| if i < 0 { (verts.len() as i64 + i) as usize } else { (i - 1) as usize })
                    .collect();
                if idx.len() < 3 || idx.iter().any(|&i| i >= verts.len()) {
                    res.skipped += 1;
                    continue;
                }
                let pts: Vec<DVec3> = idx.iter().map(|&i| verts[i]).collect();
                if m.add_polygon(&pts).is_ok() {
                    res.faces += 1;
                } else {
                    // not flat: fall back to a fan of triangles
                    for k in 1..pts.len() - 1 {
                        if m.add_polygon(&[pts[0], pts[k], pts[k + 1]]).is_ok() {
                            res.faces += 1;
                        } else {
                            res.skipped += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    m.active_group = None;
    Ok(res)
}
