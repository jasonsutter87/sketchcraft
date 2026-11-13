//! OBJ import. The file becomes one component (named after the file) placed
//! once; each `o`/`g` object is a group inside it. Built in bulk (see
//! `mesh`), so 200,000-triangle downloads load in seconds and later drawing
//! never has to look through them. Colors come from the `.mtl` file when it
//! sits next to the OBJ. Most OBJ files are Y-up (Blender's default export);
//! `up: "y"` turns them to SketchCraft's Z-up.

use crate::mesh::Mesh;
use crate::model::{Definition, Id, Instance, Material, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DVec3};
use std::collections::HashMap;
use std::path::Path;

const DEFAULT: &str = "obj:Imported";

pub struct ImportResult {
    pub instance: Id,
    pub component: Id,
    pub groups: usize,
    pub faces: usize,
    pub skipped: usize,
}

/// `newmtl` name -> diffuse color, from an .mtl file's text.
pub fn parse_mtl(text: &str) -> HashMap<String, [f32; 3]> {
    let mut out = HashMap::new();
    let mut cur: Option<String> = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => cur = Some(it.collect::<Vec<_>>().join(" ")),
            Some("Kd") => {
                let c: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if let (Some(n), 3) = (&cur, c.len()) {
                    out.insert(n.clone(), [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0)]);
                }
            }
            _ => {}
        }
    }
    out
}

pub fn import_obj(m: &mut Model, text: &str, dir: Option<&Path>, name: &str, y_up: bool, scale: f64, at: DVec3) -> Result<ImportResult> {
    let mut verts: Vec<DVec3> = Vec::new();
    let mut colors: HashMap<String, [f32; 3]> = HashMap::new();
    // objects in file order: (name, faces as (points, material))
    let mut objects: Vec<(String, Vec<(Vec<u32>, Option<String>)>)> = Vec::new();
    let mut material: Option<String> = None;
    let mut skipped = 0;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f64> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                if c.len() < 3 {
                    bail!("bad vertex line: {line}");
                }
                let p = if y_up { DVec3::new(c[0], -c[2], c[1]) } else { DVec3::new(c[0], c[1], c[2]) };
                verts.push(p * scale);
            }
            Some("o") | Some("g") => {
                let n = it.collect::<Vec<_>>().join(" ");
                // `g` right after `o` (or repeated names) adds nothing
                if objects.last().map_or(false, |(_, f)| f.is_empty()) {
                    objects.pop();
                }
                objects.push((if n.is_empty() { "Part".into() } else { n }, Vec::new()));
            }
            Some("mtllib") => {
                let file = it.collect::<Vec<_>>().join(" ");
                if let Some(t) = dir.and_then(|d| std::fs::read_to_string(d.join(&file)).ok()) {
                    colors.extend(parse_mtl(&t));
                }
            }
            Some("usemtl") => material = Some(it.collect::<Vec<_>>().join(" ")),
            Some("f") => {
                let n = verts.len() as i64;
                let idx: Vec<u32> = it
                    .filter_map(|t| t.split('/').next()?.parse::<i64>().ok())
                    .map(|i| if i < 0 { n + i } else { i - 1 })
                    .filter(|&i| i >= 0 && i < n)
                    .map(|i| i as u32)
                    .collect();
                if idx.len() < 3 {
                    skipped += 1;
                    continue;
                }
                if objects.is_empty() {
                    objects.push(("Part".into(), Vec::new()));
                }
                objects.last_mut().unwrap().1.push((idx, material.clone()));
            }
            _ => {}
        }
    }
    objects.retain(|(_, f)| !f.is_empty());
    if objects.is_empty() {
        bail!("no faces in this OBJ file");
    }

    let mut def = Model::default();
    let mut faces = 0;
    for (oname, list) in &objects {
        let g = def.begin_group(oname.clone());
        let mut mesh = Mesh::new(&mut def, Some(g));
        for (idx, mat) in list {
            let pts: Vec<DVec3> = idx.iter().map(|&i| verts[i as usize]).collect();
            // unpainted faces get a neutral paint: imported meshes often have
            // their backs facing out, which would show as blue
            let key = Some(mat.as_ref().filter(|k| colors.contains_key(*k)).map(|k| format!("obj:{k}")).unwrap_or_else(|| DEFAULT.to_string()));
            mesh.face(&pts, &[], key);
        }
        let (f, s) = mesh.finish();
        faces += f;
        skipped += s;
        def.active_group = None;
    }
    colors.insert(DEFAULT[4..].to_string(), [0.9, 0.9, 0.88]);
    for (k, c) in &colors {
        let key = format!("obj:{k}");
        let mat = Material { color: *c, texture: None };
        def.materials.insert(key.clone(), mat.clone());
        m.materials.entry(key).or_insert(mat);
    }
    let component = m.new_id();
    m.definitions.insert(component, Definition { name: name.to_string(), model: std::sync::Arc::new(def), attrs: Default::default(), source: None });
    let instance = m.new_id();
    m.instances.insert(instance, Instance { def: component, name: name.to_string(), transform: DMat4::from_translation(at), tag: None, hidden: false, attrs: Default::default() });
    Ok(ImportResult { instance, component, groups: objects.len(), faces, skipped })
}
