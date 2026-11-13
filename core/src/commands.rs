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
use std::collections::HashMap;

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
    /// Circle (or regular polygon with few `segments`) as a face.
    Circle {
        center: [f64; 3],
        radius: f64,
        #[serde(default = "up")]
        normal: [f64; 3],
        #[serde(default = "circle_segments")]
        segments: usize,
    },
    /// Arc as edges, from `start` to `end`, bowing out by `bulge` meters.
    Arc {
        start: [f64; 3],
        end: [f64; 3],
        bulge: f64,
        #[serde(default = "up")]
        normal: [f64; 3],
        #[serde(default = "arc_segments")]
        segments: usize,
    },
    /// Offset a face's outline: + inward (new face inside), - outward (ring).
    Offset { face: Id, distance: f64 },
    /// Sweep a profile face along a path: `points`, `edges`, or `around` a face's outline.
    FollowMe {
        face: Id,
        #[serde(default)]
        points: Option<Vec<[f64; 3]>>,
        #[serde(default)]
        edges: Option<Vec<Id>>,
        #[serde(default)]
        around: Option<Id>,
    },
    /// Pull a face out (positive) or push it in (negative) along its normal.
    #[serde(alias = "push_pull")]
    Pushpull { face: Id, distance: f64 },
    /// Move faces, edges, vertices or whole groups.
    Move { ids: Vec<Id>, by: [f64; 3] },
    /// Rotate around `center` about `axis` (default: vertical), in degrees.
    Rotate {
        ids: Vec<Id>,
        center: [f64; 3],
        #[serde(default = "up")]
        axis: [f64; 3],
        angle: f64,
    },
    /// Scale from `center`: one number for uniform, or [x, y, z].
    Scale { ids: Vec<Id>, center: [f64; 3], factor: Factor },
    /// Copy faces, groups or components, offset by `by`.
    Duplicate { ids: Vec<Id>, by: [f64; 3] },
    /// `count` more copies spaced `by` apart (a row of houses, a fence).
    Array { ids: Vec<Id>, by: [f64; 3], count: usize },
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
    /// The face under a point (on its plane, inside its outline), or the
    /// nearest face center if none contains it. Optional facing filter.
    Pick {
        at: [f64; 3],
        #[serde(default)]
        facing: Option<[f64; 3]>,
    },
    /// Duplicate a group (a whole house), offset by `by`.
    Copy { group: Id, by: [f64; 3], #[serde(default)] name: Option<String> },
    /// Counts, bounds, groups, materials.
    Info,
    /// Make faces or a group into a reusable component (placed where it was).
    MakeComponent { ids: Vec<Id>, #[serde(default)] name: Option<String> },
    /// Place a component (by id or name) at a point, optionally turned about Z.
    Place {
        component: Value,
        at: [f64; 3],
        #[serde(default)]
        rotate: f64,
        #[serde(default)]
        name: Option<String>,
    },
    /// Open a group, or a component's definition through one of its placements; `close` to finish.
    Edit { id: Id },
    Close,
    /// Turn a component placement back into plain geometry.
    Explode { id: Id },
    /// List the component library.
    Components,
    Hide { ids: Vec<Id> },
    /// Unhide `ids`, or everything if none given.
    Unhide { #[serde(default)] ids: Vec<Id> },
    /// Put groups/components on a tag (layer); null tag removes it.
    Tag { ids: Vec<Id>, tag: Option<String> },
    TagVisible { tag: String, visible: bool },
    /// Soft edges aren't drawn. `ids` edges or faces; `coplanar:true` softens every flat seam.
    Soften {
        #[serde(default)]
        ids: Vec<Id>,
        #[serde(default = "yes")]
        soft: bool,
        #[serde(default)]
        coplanar: bool,
    },
    Rename { id: Id, name: String },
    /// Construction guide: a line through `point` along `direction`, or a point.
    Guide { point: [f64; 3], #[serde(default)] direction: Option<[f64; 3]> },
    ClearGuides,
    /// A dimension from `from` to `to`, drawn `offset` away (e.g. [0,-1,0] = 1 m toward the street).
    Dimension { from: [f64; 3], to: [f64; 3], #[serde(default)] offset: [f64; 3] },
    /// A text label at `at`, with an optional leader line to `leader`.
    Text { at: [f64; 3], text: String, #[serde(default)] leader: Option<[f64; 3]> },
    /// Save a camera as a named view.
    SaveView { name: String, eye: [f64; 3], target: [f64; 3], #[serde(default = "fov")] fov: f64 },
    DeleteView { name: String },
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
    /// Binary glTF (.glb): Blender, three.js, game engines, Quick Look.
    ExportGlb { path: String },
    /// Binary STL for 3D printing; `scale` 1000 gives millimeters.
    ExportStl { path: String, #[serde(default = "one")] scale: f64 },
    /// Import an OBJ file; each object becomes a group. `up` "y" (default) or "z".
    ImportObj {
        #[serde(default)]
        path: Option<String>,
        /// the file's contents, instead of a path (the app sends this)
        #[serde(default)]
        text: Option<String>,
        #[serde(default = "y_up")]
        up: String,
        #[serde(default = "one")]
        scale: f64,
        #[serde(default)]
        at: [f64; 3],
    },
    /// Place a built-in asset (door, window, garage_door, tree, palm, car, streetlight, bench, hydrant).
    Insert {
        asset: String,
        at: [f64; 3],
        #[serde(default)]
        rotate: f64,
    },
    /// List the built-in assets.
    Assets,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Factor {
    Uniform(f64),
    Axes([f64; 3]),
}

fn circle_segments() -> usize {
    24
}

fn arc_segments() -> usize {
    12
}

fn one() -> f64 {
    1.0
}

fn y_up() -> String {
    "y".into()
}

fn yes() -> bool {
    true
}

fn fov() -> f64 {
    45.0
}

fn up() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

impl Command {
    fn mutates(&self) -> bool {
        !matches!(
            self,
            Command::Faces { .. }
                | Command::Info
                | Command::Components
                | Command::Pick { .. }
                | Command::Undo
                | Command::Redo
                | Command::Render { .. }
                | Command::Save { .. }
                | Command::ExportObj { .. }
                | Command::ExportGlb { .. }
                | Command::ExportStl { .. }
                | Command::Assets
        )
    }
}

#[derive(Default)]
pub struct Engine {
    pub model: Model,
    undo: Vec<Model>,
    redo: Vec<Model>,
    /// Script names: `"as":"front"` on a command stores its result id,
    /// and any later `"@front"` value is replaced by it.
    pub names: HashMap<String, Id>,
    /// Component definition being edited (`edit`), if any.
    pub ctx: Option<Id>,
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
        let mut v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return json!({"ok": false, "error": format!("bad JSON: {e}")}),
        };
        let name = v.as_object_mut().and_then(|o| o.remove("as")).and_then(|n| n.as_str().map(String::from));
        if let Err(e) = self.resolve(&mut v) {
            return json!({"ok": false, "error": e.to_string()});
        }
        let r = match serde_json::from_value::<Command>(v) {
            Ok(cmd) => self.exec(cmd),
            Err(e) => return json!({"ok": false, "error": format!("bad command: {e}")}),
        };
        if let (Some(n), true) = (name, r["ok"] == true) {
            if let Some(id) = ["face", "group", "instance", "guide", "dimension", "text"]
                .iter()
                .find_map(|k| r[*k].as_u64())
                .or_else(|| r["ids"][0].as_u64())
                .or_else(|| r["edges"][0].as_u64())
            {
                self.names.insert(n, id as Id);
            }
        }
        r
    }

    fn resolve(&self, v: &mut Value) -> Result<()> {
        match v {
            Value::String(s) if s.starts_with('@') => {
                let Some(id) = self.names.get(&s[1..]) else { bail!("unknown name {s}") };
                *v = json!(id);
            }
            Value::Array(a) => a.iter_mut().try_for_each(|x| self.resolve(x))?,
            Value::Object(o) => o.values_mut().try_for_each(|x| self.resolve(x))?,
            _ => {}
        }
        Ok(())
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
        // inside `edit`, geometry commands work on the component's definition
        if let Some(d) = self.ctx {
            if !self.model.definitions.contains_key(&d) {
                self.ctx = None;
            }
        }
        let ctx = self.ctx;
        let m: &mut Model = match ctx {
            Some(d) => &mut self.model.definitions.get_mut(&d).unwrap().model,
            None => &mut self.model,
        };
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
                let (mut edges, mut faces) = (vec![], vec![]);
                for w in points.windows(2) {
                    let r = m.draw_line(v3(w[0]), v3(w[1]))?;
                    edges.extend(r.edges);
                    faces.extend(r.faces);
                }
                json!({"edges": edges, "faces": faces, "face": faces.last()})
            }
            Command::Circle { center, radius, normal, segments } => json!({"face": m.circle(v3(center), v3(normal), radius, segments)?}),
            Command::Arc { start, end, bulge, normal, segments } => {
                let pts = Model::arc_points(v3(start), v3(end), bulge, v3(normal), segments)?;
                let mut edges = vec![];
                let mut faces = vec![];
                for w in pts.windows(2) {
                    let r = m.draw_line(w[0], w[1])?;
                    edges.extend(r.edges);
                    faces.extend(r.faces);
                }
                json!({"edges": edges, "faces": faces, "face": faces.last()})
            }
            Command::Offset { face, distance } => json!({"face": m.offset(face, distance)?}),
            Command::FollowMe { face, points, edges, around } => {
                let (path, closed) = if let Some(p) = points {
                    let pts: Vec<DVec3> = p.into_iter().map(v3).collect();
                    let closed = pts.len() > 2 && pts[0].distance(*pts.last().unwrap()) < 1e-9;
                    (pts, closed)
                } else if let Some(es) = edges {
                    m.edge_path(&es)?
                } else if let Some(f) = around {
                    (m.pts(&m.face(f)?.outer.clone()), true)
                } else {
                    bail!("follow_me needs points, edges or around");
                };
                let faces = m.follow_me(face, &path, closed)?;
                json!({"faces": faces})
            }
            Command::Pushpull { face, distance } => {
                let ids = m.pushpull(face, distance)?;
                json!({"face": ids[0], "created": &ids[1..]})
            }
            Command::Move { ids, by } => json!({"moved": m.translate(&ids, v3(by))?}),
            Command::Rotate { ids, center, axis, angle } => json!({"moved": m.rotate(&ids, v3(center), v3(axis), angle)?}),
            Command::Scale { ids, center, factor } => {
                let f = match factor {
                    Factor::Uniform(k) => DVec3::splat(k),
                    Factor::Axes(a) => v3(a),
                };
                json!({"moved": m.scale(&ids, v3(center), f)?})
            }
            Command::Duplicate { ids, by } => json!({"ids": m.duplicate(&ids, v3(by))?}),
            Command::Array { ids, by, count } => json!({"ids": m.array(&ids, v3(by), count)?}),
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
            Command::Pick { at, facing } => {
                let p = v3(at);
                let dir = facing.map(|f| v3(f).normalize_or_zero());
                let candidates: Vec<Id> = m
                    .faces
                    .keys()
                    .copied()
                    .filter(|&id| dir.map_or(true, |d| m.normal(id).dot(d) > 0.94))
                    .collect();
                let on = candidates.iter().copied().find(|&id| m.contains(id, p));
                let face = on.or_else(|| {
                    candidates.iter().copied().min_by(|a, b| {
                        m.center(*a).distance(p).partial_cmp(&m.center(*b).distance(p)).unwrap()
                    })
                });
                let Some(face) = face else { bail!("no face there") };
                json!({"face": face, "exact": on.is_some()})
            }
            Command::Copy { group, by, name } => json!({"group": m.copy_group(group, v3(by), name)?}),
            Command::MakeComponent { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Component {}", m.definitions.len() + 1));
                let inst = m.make_component(&ids, n)?;
                json!({"instance": inst, "component": m.instances[&inst].def})
            }
            Command::Place { component, at, rotate, name } => {
                let key = match &component {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let Some(def) = m.find_definition(&key) else { bail!("no component {key}") };
                json!({"instance": m.place(def, v3(at), rotate, name)?})
            }
            Command::Edit { id } => {
                if m.groups.contains_key(&id) {
                    m.active_group = Some(id); // new geometry and lines go inside the group
                    return Ok(json!({"editing_group": id}));
                }
                let Some(i) = self.model.instances.get(&id) else { bail!("{id} is not a group or component placement") };
                self.ctx = Some(i.def);
                json!({"editing": i.def})
            }
            Command::Close => {
                m.active_group = None;
                self.ctx = None;
                json!({})
            }
            Command::Explode { id } => json!({"group": m.explode(id)?}),
            Command::Components => json!({"components": m.definitions.iter().map(|(id, d)| json!({
                "id": id, "name": d.name, "faces": d.model.faces.len(),
                "placed": m.instances.values().filter(|i| i.def == *id).count(),
            })).collect::<Vec<_>>()}),
            Command::Hide { ids } => {
                m.set_hidden(&ids, true)?;
                json!({})
            }
            Command::Unhide { ids } => {
                if ids.is_empty() { m.unhide_all() } else { m.set_hidden(&ids, false)? }
                json!({})
            }
            Command::Tag { ids, tag } => {
                m.set_tag(&ids, tag)?;
                json!({})
            }
            Command::TagVisible { tag, visible } => {
                m.tags.insert(tag, visible);
                json!({})
            }
            Command::Soften { ids, soft, coplanar } => {
                let n = if coplanar { m.soften_coplanar() } else { m.soften(&ids, soft)? };
                json!({"edges": n})
            }
            Command::Rename { id, name } => {
                if let Some(g) = m.groups.get_mut(&id) {
                    g.name = name;
                } else if let Some(i) = m.instances.get_mut(&id) {
                    i.name = name;
                } else if let Some(d) = m.definitions.get_mut(&id) {
                    d.name = name;
                } else {
                    bail!("{id} can't be renamed (groups and components only)");
                }
                json!({})
            }
            Command::Guide { point, direction } => {
                let id = m.next_id + 1;
                m.next_id = id;
                m.guides.push(crate::model::Guide { id, p: v3(point), dir: direction.map(|d| v3(d).normalize()) });
                json!({"guide": id})
            }
            Command::Dimension { from, to, offset } => {
                let id = m.next_id + 1;
                m.next_id = id;
                let (a, b) = (v3(from), v3(to));
                m.annotations.push(crate::model::Annotation::Dimension { id, a, b, offset: v3(offset) });
                json!({"dimension": id, "length": round(a.distance(b))})
            }
            Command::Text { at, text, leader } => {
                let id = m.next_id + 1;
                m.next_id = id;
                m.annotations.push(crate::model::Annotation::Text { id, at: v3(at), text, leader: leader.map(v3) });
                json!({"text": id})
            }
            Command::ClearGuides => {
                m.guides.clear();
                json!({})
            }
            Command::SaveView { name, eye, target, fov } => {
                m.views.retain(|v| v.name != name);
                m.views.push(crate::model::View { name, eye, target, fov });
                json!({})
            }
            Command::DeleteView { name } => {
                m.views.retain(|v| v.name != name);
                json!({})
            }
            Command::Info => {
                let b = m.world_bounds().map(|(lo, hi)| json!({"min": lo.to_array(), "max": hi.to_array()}));
                json!({
                    "vertices": m.vertices.len(),
                    "edges": m.edges.len(),
                    "faces": m.faces.len(),
                    "groups": m.groups.iter().map(|(id, g)| json!({"id": id, "name": g.name})).collect::<Vec<_>>(),
                    "materials": m.materials.keys().collect::<Vec<_>>(),
                    "active_group": m.active_group,
                    "editing": ctx,
                    "instances": m.instances.iter().map(|(id, i)| json!({"id": id, "name": i.name, "component": i.def})).collect::<Vec<_>>(),
                    "tags": &m.tags,
                    "views": m.views.iter().map(|v| &v.name).collect::<Vec<_>>(),
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
                let whole = &self.model;
                let cam = render::camera_for(whole, &opts);
                if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    render::render(whole, &opts).save(path)?;
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
            Command::ExportGlb { path } => json!({"path": path, "triangles": export::write_glb(&self.model, &path)?}),
            Command::ExportStl { path, scale } => json!({"path": path, "triangles": export::write_stl(&self.model, &path, scale)?}),
            Command::ImportObj { path, text, up, scale, at } => {
                let text = match (text, path) {
                    (Some(t), _) => t,
                    (None, Some(p)) => std::fs::read_to_string(&p)?,
                    _ => bail!("import_obj needs a path or text"),
                };
                let r = crate::import::import_obj(m, &text, up.eq_ignore_ascii_case("y"), scale, v3(at))?;
                json!({"groups": r.groups, "group": r.groups.first(), "faces": r.faces, "skipped": r.skipped})
            }
            Command::Insert { asset, at, rotate } => {
                let def = crate::assets::definition(m, &asset)?;
                json!({"instance": m.place(def, v3(at), rotate, None)?, "component": def})
            }
            Command::Assets => json!({"assets": crate::assets::ASSETS.iter().map(|a| json!({"key": a.key, "name": a.name, "about": a.about})).collect::<Vec<_>>()}),
            Command::ExportObj { path } => {
                let tris = export::write_obj(&self.model, &path)?;
                json!({"path": path, "triangles": tris})
            }
        })
    }
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
