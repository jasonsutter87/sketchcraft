//! The command layer. Everything that changes or inspects a model goes
//! through `Engine::exec`, whether it comes from a mouse in the app, a script,
//! the CLI or an AI agent over MCP. Commands are JSON:
//!
//!   {"cmd":"rect","from":[0,0,0],"to":[8,12,0]}
//!   {"cmd":"pushpull","face":7,"distance":6}
//!
//! Each reply is JSON too: {"ok":true, ...} or {"ok":false,"error":"..."}.

use crate::export;
use crate::model::{Id, Model};
use crate::render::{self, RenderOpts};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Start an empty model.
    New,
    /// Axis-aligned rectangle between two opposite corners (one coordinate must match).
    Rect { from: [f64; 3], to: [f64; 3] },
    /// Planar polygon through the points (a building footprint, a gable end...).
    Polygon { points: Vec<[f64; 3]> },
    /// Connected lines; if the last point closes the loop, a face is made.
    Line { points: Vec<[f64; 3]> },
    /// Pull a face out (positive) or push it in (negative) along its normal.
    #[serde(alias = "push_pull")]
    Pushpull { face: Id, distance: f64 },
    /// Move faces, edges, vertices or whole groups.
    Move { ids: Vec<Id>, by: [f64; 3] },
    Delete { ids: Vec<Id> },
    /// Flip faces (or every face in a group).
    Reverse { ids: Vec<Id> },
    /// Color faces (or every face in a group), 0..1 RGB.
    Paint { ids: Vec<Id>, color: [f32; 3] },
    /// New geometry goes into a fresh group until end_group.
    BeginGroup { #[serde(default)] name: Option<String> },
    EndGroup,
    /// Wrap existing faces into a group.
    Group { ids: Vec<Id>, #[serde(default)] name: Option<String> },
    /// List faces, optionally only one group's, or only those facing a direction.
    Faces {
        #[serde(default)]
        group: Option<Id>,
        #[serde(default)]
        facing: Option<[f64; 3]>,
    },
    /// Counts, bounds, groups, materials.
    Info,
    Undo,
    Redo,
    /// Render a PNG. Without `out`, just reports the camera it would use.
    Render {
        #[serde(default)]
        out: Option<String>,
        #[serde(flatten)]
        opts: RenderOpts,
    },
    Save { path: String },
    Open { path: String },
    /// Wavefront OBJ (+ .mtl) for Blender and friends.
    ExportObj { path: String },
}

impl Command {
    fn mutates(&self) -> bool {
        !matches!(
            self,
            Command::Faces { .. }
                | Command::Info
                | Command::Undo
                | Command::Redo
                | Command::Render { .. }
                | Command::Save { .. }
                | Command::ExportObj { .. }
        )
    }
}

#[derive(Default)]
pub struct Engine {
    pub model: Model,
    undo: Vec<Model>,
    redo: Vec<Model>,
}

const UNDO_LIMIT: usize = 200;

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::from(a)
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run one JSON command line and return a JSON reply (never panics on bad input).
    pub fn exec_json(&mut self, line: &str) -> Value {
        match serde_json::from_str::<Command>(line) {
            Ok(cmd) => self.exec(cmd),
            Err(e) => json!({"ok": false, "error": format!("bad command: {e}")}),
        }
    }

    pub fn exec(&mut self, cmd: Command) -> Value {
        let mutates = cmd.mutates();
        let before = if mutates { Some(self.model.clone()) } else { None };
        match self.run(cmd) {
            Ok(mut v) => {
                if let Some(b) = before {
                    self.undo.push(b);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                v["ok"] = json!(true);
                v
            }
            Err(e) => {
                if let Some(b) = before {
                    self.model = b; // a failed command leaves no half-done edits
                }
                json!({"ok": false, "error": e.to_string()})
            }
        }
    }

    fn run(&mut self, cmd: Command) -> Result<Value> {
        let m = &mut self.model;
        Ok(match cmd {
            Command::New => {
                *m = Model::default();
                json!({})
            }
            Command::Rect { from, to } => {
                let (a, b) = (v3(from), v3(to));
                let d = b - a;
                let k = (0..3).find(|&k| d[k].abs() < 1e-9);
                let Some(k) = k else { bail!("rect corners must share one coordinate (use polygon for tilted shapes)") };
                let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                if d[i].abs() < 1e-9 || d[j].abs() < 1e-9 {
                    bail!("rect has zero width");
                }
                let mut p1 = a;
                p1[i] = b[i];
                let mut p3 = a;
                p3[j] = b[j];
                let f = m.add_polygon(&[a, p1, b, p3])?;
                json!({"face": f})
            }
            Command::Polygon { points } => {
                let pts: Vec<DVec3> = points.into_iter().map(v3).collect();
                json!({"face": m.add_polygon(&pts)?})
            }
            Command::Line { points } => {
                if points.len() < 2 {
                    bail!("a line needs 2 points");
                }
                let ids: Vec<Id> = points.iter().map(|p| m.vertex(v3(*p))).collect();
                let edges: Vec<Id> = ids.windows(2).filter(|w| w[0] != w[1]).map(|w| m.edge(w[0], w[1])).collect();
                let closed = ids.len() >= 4 && ids.first() == ids.last();
                let face = if closed { m.add_face(ids[..ids.len() - 1].to_vec()).ok() } else { None };
                json!({"edges": edges, "face": face})
            }
            Command::Pushpull { face, distance } => {
                let ids = m.pushpull(face, distance)?;
                json!({"face": ids[0], "created": &ids[1..]})
            }
            Command::Move { ids, by } => json!({"moved_vertices": m.translate(&ids, v3(by))?}),
            Command::Delete { ids } => {
                m.delete(&ids)?;
                json!({})
            }
            Command::Reverse { ids } => {
                let faces = m.faces_of(&ids)?;
                for f in &faces {
                    m.faces.get_mut(f).unwrap().reverse();
                }
                json!({"faces": faces})
            }
            Command::Paint { ids, color } => json!({"material": m.paint(&ids, color)?}),
            Command::BeginGroup { name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.begin_group(n)})
            }
            Command::EndGroup => {
                m.active_group = None;
                json!({})
            }
            Command::Group { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.group(&ids, n)?})
            }
            Command::Faces { group, facing } => {
                let dir = facing.map(|f| v3(f).normalize_or_zero());
                let list: Vec<Value> = m
                    .faces
                    .iter()
                    .filter(|(_, f)| group.is_none() || f.group == group)
                    .map(|(&id, _)| id)
                    .filter(|&id| dir.map_or(true, |d| m.normal(id).dot(d) > 0.94))
                    .map(|id| {
                        let f = &m.faces[&id];
                        let r = |v: DVec3| [round(v.x), round(v.y), round(v.z)];
                        json!({
                            "id": id,
                            "normal": r(m.normal(id)),
                            "center": r(m.center(id)),
                            "area": round(m.face_area(id)),
                            "points": f.outer.len(),
                            "holes": f.holes.len(),
                            "group": f.group,
                            "material": f.material,
                        })
                    })
                    .collect();
                json!({"faces": list})
            }
            Command::Info => {
                let b = m.bounds().map(|(lo, hi)| json!({"min": lo.to_array(), "max": hi.to_array()}));
                json!({
                    "vertices": m.vertices.len(),
                    "edges": m.edges.len(),
                    "faces": m.faces.len(),
                    "groups": m.groups.iter().map(|(id, g)| json!({"id": id, "name": g.name})).collect::<Vec<_>>(),
                    "materials": m.materials.keys().collect::<Vec<_>>(),
                    "active_group": m.active_group,
                    "bounds": b,
                })
            }
            Command::Undo => {
                let Some(prev) = self.undo.pop() else { bail!("nothing to undo") };
                self.redo.push(std::mem::replace(&mut self.model, prev));
                json!({})
            }
            Command::Redo => {
                let Some(next) = self.redo.pop() else { bail!("nothing to redo") };
                self.undo.push(std::mem::replace(&mut self.model, next));
                json!({})
            }
            Command::Render { out, opts } => {
                let cam = render::camera_for(m, &opts);
                if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    render::render(m, &opts).save(path)?;
                }
                json!({"out": out, "camera": {"eye": cam.eye, "target": cam.target, "fov": cam.fov}})
            }
            Command::Save { path } => {
                std::fs::write(&path, serde_json::to_string(&*m)?)?;
                json!({"path": path})
            }
            Command::Open { path } => {
                *m = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                json!({"path": path, "faces": m.faces.len()})
            }
            Command::ExportObj { path } => {
                let tris = export::write_obj(m, &path)?;
                json!({"path": path, "triangles": tris})
            }
        })
    }
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
