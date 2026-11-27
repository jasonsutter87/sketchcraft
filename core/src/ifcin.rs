//! IFC import (and so Revit, which exports IFC), through IfcOpenShell: the
//! open IFC toolkit turns every element's geometry into triangles (with
//! openings cut), and SketchCraft rebuilds each element as a group of
//! whole faces, typed for the Classifier and grouped by storey.
//!
//! IfcOpenShell is a Python package the user installs once (see `HELP`); it
//! runs as a separate program, with the file path as an argument.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use glam::DVec3;
use serde::Deserialize;

use crate::model::{Id, Model};

const SCRIPT: &str = include_str!("ifc_mesh.py");

/// Where SketchCraft keeps its own Python for IFC (made by the setup steps).
fn own_python() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("SketchCraft\\ifc\\Scripts\\python.exe"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/SketchCraft/ifc/bin/python3"))
}

#[cfg(not(windows))]
pub const HELP: &str = "Opening IFC needs IfcOpenShell (free, open source). Once, in Terminal:\n  python3 -m venv ~/Library/Application\\ Support/SketchCraft/ifc\n  ~/Library/Application\\ Support/SketchCraft/ifc/bin/pip install ifcopenshell\nthen try again. (Or set SKETCHCRAFT_PYTHON to a Python that has it.)";
#[cfg(windows)]
pub const HELP: &str = "Opening IFC needs IfcOpenShell (free, open source). Once, in a Command Prompt (with Python from python.org):\n  py -m venv %LOCALAPPDATA%\\SketchCraft\\ifc\n  %LOCALAPPDATA%\\SketchCraft\\ifc\\Scripts\\pip install ifcopenshell\nthen try again. (Or set SKETCHCRAFT_PYTHON to a python.exe that has it.)";

/// A Python that can `import ifcopenshell`, if there is one.
pub fn python() -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = vec![];
    if let Some(p) = std::env::var_os("SKETCHCRAFT_PYTHON") {
        cands.push(PathBuf::from(p));
    }
    cands.extend(own_python());
    let path = std::env::var("PATH").unwrap_or_default();
    let names: &[&str] = if cfg!(windows) { &["python.exe", "python3.exe", "py.exe"] } else { &["python3"] };
    cands.extend(std::env::split_paths(&path).flat_map(|d| names.iter().map(move |n| d.join(n)).collect::<Vec<_>>()));
    cands.extend(["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"].map(PathBuf::from));
    cands.into_iter().filter(|p| p.is_file()).find(|p| {
        {
            let mut c = Command::new(p);
            c.args(["-I", "-c", "import ifcopenshell.geom"]);
            crate::helper::run(c, crate::helper::PROBE)
        }
        .map(|o| o.status.success()).unwrap_or(false)
    })
}

#[derive(Deserialize)]
struct Out {
    #[serde(default)]
    schema: String,
    storeys: Vec<Storey>,
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Storey {
    id: i64,
    name: String,
    #[serde(default)]
    elevation: f64,
}

#[derive(Deserialize)]
struct Element {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    storey: Option<i64>,
    verts: Vec<f64>,
    faces: Vec<usize>,
    #[serde(default)]
    mats: Vec<[f64; 4]>,
    #[serde(default)]
    mat: Vec<i64>,
}

pub struct IfcResult {
    pub group: Id,
    pub elements: usize,
    pub faces: usize,
    pub storeys: usize,
    pub schema: String,
}

/// Run IfcOpenShell on `path` and build the model.
pub fn import(m: &mut Model, path: &Path) -> Result<IfcResult> {
    if !path.is_file() {
        bail!("no file at {}", path.display());
    }
    let py = python().with_context(|| HELP.to_string())?;
    let scratch = crate::safety::TempDir::new("sketchcraft-ifc")?;
    let tmp = scratch.path().to_path_buf();
    let script = tmp.join("ifc_mesh.py");
    let out = tmp.join("out.json");
    std::fs::write(&script, SCRIPT)?;
    let mut c = Command::new(&py);
    c.arg("-I").arg(&script).arg(crate::safety::path_arg(path)).arg(&out);
    let run = crate::helper::run(c, crate::helper::IFC);
    let result = (|| {
        let o = run.map_err(|e| anyhow::anyhow!("Python ({}): {}", py.display(), crate::helper::why(&e))).context("couldn't run IfcOpenShell")?;
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            bail!("IfcOpenShell couldn't read that file: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message"));
        }
        let text = std::fs::read_to_string(&out).context("IfcOpenShell wrote nothing")?;
        let data: Out = serde_json::from_str(&text).context("IfcOpenShell's output didn't parse")?;
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "IFC model".into());
        build(m, data, &name)
    })();
    result
}

/// Readable names for IFC types in the outliner ("IfcWallStandardCase" → "Wall").
fn plain(kind: &str) -> String {
    let k = kind.strip_prefix("Ifc").unwrap_or(kind);
    let k = k.strip_suffix("StandardCase").or_else(|| k.strip_suffix("ElementedCase")).unwrap_or(k);
    let mut out = String::new();
    for (i, c) in k.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn build(m: &mut Model, data: Out, name: &str) -> Result<IfcResult> {
    let outer = m.begin_group(name.to_string());
    let prev_active = m.active_group;
    // one group per storey (by elevation), tagged with its name
    let mut storeys: Vec<&Storey> = data.storeys.iter().collect();
    storeys.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    let mut storey_group = std::collections::HashMap::new();
    for s in &storeys {
        m.active_group = Some(outer);
        let g = m.begin_group(s.name.clone());
        m.set_tag(&[g], Some(s.name.clone())).ok();
        storey_group.insert(s.id, g);
    }
    m.active_group = Some(outer);
    // colors become materials, shared by value
    let mut materials: std::collections::HashMap<(u8, u8, u8, u8), String> = Default::default();
    let (mut elements, mut faces) = (0, 0);
    for el in &data.elements {
        if el.faces.len() < 3 || el.verts.len() < 9 {
            continue;
        }
        let v = |i: usize| -> Option<DVec3> { el.verts.get(i * 3..i * 3 + 3).map(|p| DVec3::new(p[0], p[1], p[2])) };
        let mut polys = vec![];
        for (t, tri) in el.faces.chunks_exact(3).enumerate() {
            let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
            if !(a.is_finite() && b.is_finite() && c.is_finite()) {
                continue;
            }
            let mat = el.mat.get(t).and_then(|&k| usize::try_from(k).ok()).and_then(|k| el.mats.get(k)).or(el.mats.first()).map(|c| {
                let q = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
                let key = (q(c[0]), q(c[1]), q(c[2]), q(c[3]));
                materials
                    .entry(key)
                    .or_insert_with(|| {
                        let n = format!("ifc:{:02x}{:02x}{:02x}{}", key.0, key.1, key.2, if key.3 < 255 { format!("-{:02x}", key.3) } else { String::new() });
                        m.materials.entry(n.clone()).or_insert(crate::model::Material { color: [unit(c[0], 0.8), unit(c[1], 0.8), unit(c[2], 0.8)], texture: None, opacity: unit(c[3], 1.0).clamp(0.05, 1.0), ..Default::default() });
                        n
                    })
                    .clone()
            });
            if let Some(p) = crate::csg::Poly::new(vec![a, b, c], mat) {
                polys.push(p);
            }
        }
        if polys.is_empty() {
            continue;
        }
        let label = if el.name.trim().is_empty() { plain(&el.kind) } else { format!("{} ({})", el.name.trim(), plain(&el.kind)) };
        let Ok(g) = crate::csg::to_group(m, &polys, &label) else { continue };
        let parent = el.storey.and_then(|s| storey_group.get(&s).copied()).unwrap_or(outer);
        if let Some(gr) = m.groups.get_mut(&g) {
            gr.parent = Some(parent);
        }
        m.classes.insert(g, el.kind.clone());
        faces += m.faces.values().filter(|f| f.group == Some(g)).count();
        elements += 1;
    }
    // empty storeys go
    for g in storey_group.values() {
        if !m.groups.values().any(|x| x.parent == Some(*g)) {
            m.groups.remove(g);
        }
    }
    m.active_group = prev_active.filter(|a| *a != outer).or(m.groups.get(&outer).and_then(|g| g.parent));
    if elements == 0 {
        m.groups.remove(&outer);
        bail!("no building elements with geometry in that IFC");
    }
    let storeys = storey_group.values().filter(|g| m.groups.contains_key(g)).count();
    Ok(IfcResult { group: outer, elements, faces, storeys, schema: data.schema })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names() {
        assert_eq!(plain("IfcWallStandardCase"), "Wall");
        assert_eq!(plain("IfcBuildingElementProxy"), "Building Element Proxy");
        assert_eq!(plain("IfcDoor"), "Door");
    }

    #[test]
    fn builds_typed_groups_by_storey_with_merged_faces() {
        // a 1 m cube as 12 triangles, on storey 7, red
        let verts = vec![0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0., 0., 0., 1., 1., 0., 1., 1., 1., 1., 0., 1., 1.];
        let faces = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
        let data = Out {
            schema: "IFC4".into(),
            storeys: vec![Storey { id: 7, name: "Ground Floor".into(), elevation: 0.0 }, Storey { id: 9, name: "Empty".into(), elevation: 3.0 }],
            elements: vec![Element { kind: "IfcWallStandardCase".into(), name: "Basic Wall".into(), storey: Some(7), verts, faces, mats: vec![[1.0, 0.0, 0.0, 1.0]], mat: vec![0; 12] }],
        };
        let mut m = Model::default();
        let r = build(&mut m, data, "House").unwrap();
        assert_eq!((r.elements, r.storeys), (1, 1));
        assert_eq!(r.faces, 6, "triangles merged into the cube's 6 faces");
        let wall = *m.classes.keys().next().unwrap();
        assert_eq!(m.classes[&wall], "IfcWallStandardCase");
        assert_eq!(m.groups[&wall].name, "Basic Wall (Wall)");
        let storey = m.groups[&wall].parent.unwrap();
        assert_eq!(m.groups[&storey].name, "Ground Floor");
        assert_eq!(m.groups[&storey].tag.as_deref(), Some("Ground Floor"));
        assert!(m.faces.values().all(|f| f.material.as_deref() == Some("ifc:ff0000")));
    }
}

/// A colour channel from the file as 0..1: out-of-range values to the nearest end, a value that isn't a number to `or`.
fn unit(c: f64, or: f32) -> f32 {
    if c.is_nan() {
        or
    } else {
        c.clamp(0.0, 1.0) as f32
    }
}
