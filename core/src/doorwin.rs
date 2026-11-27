//! Door and Window tools (#264, from Blender 3D Architect's first look at 0.3.8: "we still don't have anything for
//! instance to create windows here, just these components on the left").
//!
//! A door or window of any size is a component that cuts its opening (see `openings`), built from the same recipe as
//! the library's Door and Window and remembered by its size (`Definition::source` is `builtin:door@0.900x2.100`), so
//! twenty windows of one size share one definition. It is placed on the side of a wall facing out of it, centred
//! where it was clicked (moved along to fit), a door on the floor and a window at its sill height, cut through the
//! wall, and classed IfcDoor or IfcWindow for IFC export. In a wall the Wall tool made (#263), a placed one can be
//! resized afterwards: the wall is rebuilt and every opening in it cut again.

use crate::model::{Definition, Id, Model};
use anyhow::{anyhow, bail, Result};
use glam::{DMat4, DVec3};
use serde_json::{json, Map, Value};

/// Sizes a door or window may have, in metres: (width, height) ranges, and the highest sill.
const DOOR: ((f64, f64), (f64, f64)) = ((0.4, 3.0), (1.5, 3.5));
const WINDOW: ((f64, f64), (f64, f64)) = ((0.2, 5.0), (0.2, 3.0));
const MAX_SILL: f64 = 3.0;
/// The least wall left between two openings, in metres.
const GAP: f64 = 0.05;

fn ifc_class(kind: &str) -> &'static str {
    if kind == "door" {
        "IfcDoor"
    } else {
        "IfcWindow"
    }
}

fn source(kind: &str, w: f64, h: f64) -> String {
    format!("builtin:{kind}@{w:.3}x{h:.3}")
}

/// A sized door's or window's kind and size, from its definition's source (None for any other component).
pub fn sized(def: &Definition) -> Option<(&'static str, f64, f64)> {
    let rest = def.source.as_deref()?.strip_prefix("builtin:")?;
    let (kind, size) = rest.split_once('@')?;
    let kind = match kind {
        "door" => "door",
        "window" => "window",
        _ => return None,
    };
    let (w, h) = size.split_once('x')?;
    Some((kind, w.parse().ok()?, h.parse().ok()?))
}

fn check_size(kind: &str, w: f64, h: f64, sill: f64) -> Result<()> {
    let ((w0, w1), (h0, h1)) = if kind == "door" { DOOR } else { WINDOW };
    if !(w0..=w1).contains(&w) || !(h0..=h1).contains(&h) {
        bail!("a {kind} is {w0}-{w1} m wide and {h0}-{h1} m tall");
    }
    if !(0.0..=MAX_SILL).contains(&sill) {
        bail!("a window's sill is 0-{MAX_SILL} m above the floor");
    }
    Ok(())
}

/// The commands that draw a door or window `w` wide and `h` tall: origin at its bottom-left on the wall, facing -Y
/// (as the library's Door and Window are drawn).
fn script(kind: &str, w: f64, h: f64) -> Vec<Value> {
    if kind == "door" {
        let ix = (w * 0.15).min(0.15);
        let (u0, u1, l0, l1) = (h * 0.571, h * 0.929, h * 0.119, h * 0.476);
        vec![
            json!({"cmd": "begin_group", "name": "Door", "as": "door"}),
            json!({"cmd": "polygon", "points": [[0, 0, 0], [w, 0, 0], [w, 0, h], [0, 0, h]], "as": "slab"}),
            json!({"cmd": "pushpull", "face": "@slab", "distance": 0.05}),
            json!({"cmd": "rect", "from": [ix, -0.05, u0], "to": [w - ix, -0.05, u1], "as": "p1"}),
            json!({"cmd": "pushpull", "face": "@p1", "distance": -0.02}),
            json!({"cmd": "rect", "from": [ix, -0.05, l0], "to": [w - ix, -0.05, l1], "as": "p2"}),
            json!({"cmd": "pushpull", "face": "@p2", "distance": -0.02}),
            json!({"cmd": "end_group"}),
            json!({"cmd": "paint", "ids": ["@door"], "color": [0.36, 0.22, 0.15]}),
            json!({"cmd": "begin_group", "name": "Knob", "as": "knob"}),
            json!({"cmd": "circle", "center": [w - (w * 0.14).min(0.14), -0.05, h * 0.5], "normal": [0, -1, 0], "radius": 0.035, "segments": 12, "as": "k"}),
            json!({"cmd": "pushpull", "face": "@k", "distance": 0.06}),
            json!({"cmd": "end_group"}),
            json!({"cmd": "paint", "ids": ["@knob"], "color": [0.78, 0.68, 0.42]}),
        ]
    } else {
        let f = 0.08_f64.min(w.min(h) / 4.0);
        vec![
            json!({"cmd": "begin_group", "name": "Frame", "as": "frame"}),
            json!({"cmd": "polygon", "points": [[0, 0, 0], [w, 0, 0], [w, 0, h], [0, 0, h]], "as": "w"}),
            json!({"cmd": "pushpull", "face": "@w", "distance": 0.08, "as": "front"}),
            json!({"cmd": "offset", "face": "@front", "distance": f, "as": "inner"}),
            json!({"cmd": "pushpull", "face": "@inner", "distance": -0.06, "as": "glass"}),
            json!({"cmd": "end_group"}),
            json!({"cmd": "paint", "ids": ["@frame"], "color": [0.97, 0.97, 0.95]}),
            json!({"cmd": "material", "name": "Glass", "color": [0.45, 0.6, 0.72]}),
            json!({"cmd": "paint", "ids": ["@glass"], "material": "Glass"}),
        ]
    }
}

/// Where an opening sits on a wall face: the face's outward normal, the direction along it, and how far it reaches
/// along that direction and up.
struct Side {
    n: DVec3,
    along: DVec3,
    plane: DVec3,
    a: (f64, f64),
    z: (f64, f64),
}

/// Do two openings' back outlines (world points) overlap or come within `GAP` of each other, on one wall (parallel
/// upright planes within 0.65 m of each other: either face of it)?
pub(crate) fn outlines_clash(a: &[DVec3], b: &[DVec3]) -> bool {
    let (Some(p), Some(q)) = (a.first(), b.first()) else { return false };
    // the along-the-wall direction from the first outline's plane
    let n = crate::geom::newell(a).normalize_or_zero();
    if n == DVec3::ZERO || n.z.abs() > 1e-3 || crate::geom::newell(b).normalize_or_zero().dot(n).abs() < 0.999 || (n.dot(*p) - n.dot(*q)).abs() > 0.65 {
        return false;
    }
    let along = n.cross(DVec3::Z).normalize();
    let ext = |pts: &[DVec3], g: &dyn Fn(&DVec3) -> f64| pts.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(g(p)), h.max(g(p))));
    let ((a0, a1), (b0, b1)) = (ext(a, &|p| p.dot(along)), ext(b, &|p| p.dot(along)));
    let ((z0, z1), (y0, y1)) = (ext(a, &|p| p.z), ext(b, &|p| p.z));
    a0 < b1 + GAP && b0 < a1 + GAP && z0 < y1 + GAP && y0 < z1 + GAP
}

impl Model {
    /// The definition of a door or window this size, made the first time it's asked for.
    pub fn opening_definition(&mut self, kind: &str, w: f64, h: f64) -> Result<Id> {
        let (w, h) = ((w * 1000.0).round() / 1000.0, (h * 1000.0).round() / 1000.0);
        let src = source(kind, w, h);
        let mut e = crate::Engine::new();
        for c in script(kind, w, h) {
            let r = e.exec_json(&c.to_string());
            if r["ok"] != true {
                bail!("making a {kind}: {}", r["error"]);
            }
        }
        // reuse one already in the model only if it is exactly what this recipe makes: a file could carry a
        // definition under this name with other geometry
        let want = serde_json::to_vec(&e.model).unwrap_or_default();
        let same = |d: &Definition| d.cuts && d.source.as_deref() == Some(src.as_str()) && serde_json::to_vec(&*d.model).is_ok_and(|v| v == want);
        if let Some((&id, _)) = self.definitions.iter().find(|(_, d)| same(d)) {
            return Ok(id);
        }
        let id = self.new_id();
        let name = if kind == "door" { "Door" } else { "Window" };
        self.definitions.insert(id, Definition { name: name.to_string(), model: std::sync::Arc::new(e.model), attrs: Default::default(), source: Some(src), cuts: true, face_me: false });
        Ok(id)
    }

    fn wall_side(&self, host: Id) -> Result<Side> {
        let f = self.faces.get(&host).ok_or_else(|| anyhow!("no face {host}"))?;
        let n = self.normal(host);
        if n.z.abs() > 1e-3 || n.length() < 0.5 {
            bail!("click the side of a wall: doors and windows go in upright faces");
        }
        let along = (-n).cross(DVec3::Z).normalize();
        let pts = self.pts(&f.outer);
        let ext = |g: &dyn Fn(&DVec3) -> f64| pts.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(g(p)), h.max(g(p))));
        Ok(Side { n, along, plane: pts[0], a: ext(&|p| p.dot(along)), z: ext(&|p| p.z) })
    }

    /// Where a `w` x `h` opening goes on `side`, centred at `centre` along the wall (moved along to fit) and `up`
    /// above the wall's foot.
    fn opening_frame(side: &Side, kind: &str, centre: f64, w: f64, h: f64, up: f64) -> Result<DMat4> {
        let (a0, a1) = side.a;
        if w > a1 - a0 - 1e-3 {
            bail!("that {kind} is wider than the wall ({:.2} m)", a1 - a0);
        }
        let z0 = side.z.0 + up;
        if z0 + h > side.z.1 - 1e-3 {
            bail!("that {kind} doesn't fit under the top of the wall ({:.2} m tall)", side.z.1 - side.z.0);
        }
        let left = (centre - w / 2.0).clamp(a0, a1 - w);
        let origin = side.plane + side.along * (left - side.plane.dot(side.along)) + DVec3::Z * (z0 - side.plane.z);
        // the component is drawn facing -Y with its back on y = 0: -Y goes out of the wall
        Ok(DMat4::from_cols(side.along.extend(0.0), (-side.n).extend(0.0), DVec3::Z.extend(0.0), origin.extend(1.0)))
    }

    /// Refuse an opening at `t` (`w` x `h`) that would overlap a door or window already cut into this wall (either of
    /// its faces): the cuts would cross and leave the wall with open edges (R0311-R-E03). `skip` is the one being
    /// resized. At least `GAP` of wall stays between two openings: cuts that touch leave faces back to back too.
    fn refuse_overlap(&self, side: &Side, kind: &str, t: DMat4, w: f64, h: f64, skip: Option<Id>) -> Result<()> {
        let a0 = t.w_axis.truncate().dot(side.along);
        let (z0, plane) = (t.w_axis.z, side.n.dot(side.plane));
        for (&id, i) in &self.instances {
            if Some(id) == skip {
                continue;
            }
            let Some(d) = self.definitions.get(&i.def).filter(|d| d.cuts) else { continue };
            let Some(back) = crate::openings::back_outline(&d.model) else { continue };
            let pts: Vec<DVec3> = back.iter().map(|p| i.transform.transform_point3(*p)).collect();
            // in this wall: on its face or the one behind (walls are at most 0.6 m thick), and upright like it
            if pts.is_empty() || !pts.iter().all(|p| (side.n.dot(*p) - plane).abs() < 0.65) {
                continue;
            }
            let ext = |g: &dyn Fn(&DVec3) -> f64| pts.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(g(p)), h.max(g(p))));
            let ((b0, b1), (y0, y1)) = (ext(&|p| p.dot(side.along)), ext(&|p| p.z));
            if a0 < b1 + GAP && b0 < a0 + w + GAP && z0 < y1 + GAP && y0 < z0 + h + GAP {
                bail!("that {kind} would overlap or touch {} already in this wall: leave at least {} cm of wall between them", i.name, GAP * 100.0);
            }
        }
        Ok(())
    }

    /// Put a door or window in the wall face `host`, centred near `at`. A window sits `sill` above the wall's foot.
    pub fn place_opening(&mut self, kind: &str, host: Id, at: DVec3, w: f64, h: f64, sill: f64) -> Result<Value> {
        let sill = if kind == "door" { 0.0 } else { sill };
        check_size(kind, w, h, sill)?;
        self.check_unlocked(&[host])?;
        let side = self.wall_side(host)?;
        let t = Self::opening_frame(&side, kind, at.dot(side.along), w, h, sill)?;
        self.refuse_overlap(&side, kind, t, w, h, None)?;
        let def = self.opening_definition(kind, w, h)?;
        let inst = self.place(def, DVec3::ZERO, 0.0, None)?;
        self.instances.get_mut(&inst).unwrap().transform = t;
        let model = self.definitions[&def].model.clone();
        if let Err(e) = crate::openings::cut(self, &model, t, host) {
            self.instances.remove(&inst);
            return Err(e);
        }
        self.classes.insert(inst, ifc_class(kind).to_string());
        Ok(json!({"instance": inst, "kind": kind, "width": w, "height": h, "sill": sill}))
    }

    /// A placed door's or window's size and sill, and whether it can be resized (it's in a wall the Wall tool made).
    pub fn opening_info(&self, inst: Id) -> Result<Value> {
        let i = self.instances.get(&inst).ok_or_else(|| anyhow!("no component {inst}"))?;
        let Some((kind, w, h)) = self.definitions.get(&i.def).and_then(sized) else { return Ok(json!({"instance": inst, "kind": null})) };
        let wall = self.opening_wall(inst);
        let foot = wall.and_then(|(_, f)| self.wall_side(f).ok()).map(|s| s.z.0);
        let sill = foot.map(|z| ((i.transform.w_axis.z - z) * 1000.0).round() / 1000.0);
        Ok(json!({"instance": inst, "kind": kind, "width": w, "height": h, "sill": sill, "resizable": wall.is_some()}))
    }

    /// The remembered wall (group) a placed opening is cut into, and the face it sits on.
    fn opening_wall(&self, inst: Id) -> Option<(Id, Id)> {
        let i = self.instances.get(&inst)?;
        let def = self.definitions.get(&i.def)?;
        let back = crate::openings::back_outline(&def.model)?;
        let world: Vec<DVec3> = back.iter().map(|p| i.transform.transform_point3(*p)).collect();
        self.groups.iter().filter(|(_, g)| g.recipe.is_some()).find_map(|(&g, _)| self.wall_face_for(&world, &[g]).map(|f| (g, f)))
    }

    /// Resize a placed door or window, or raise a window's sill: it keeps its place along the wall, and the wall is
    /// rebuilt with every opening in it cut again. Only in a wall the Wall tool made (#263).
    pub fn edit_opening(&mut self, inst: Id, width: Option<f64>, height: Option<f64>, sill: Option<f64>) -> Result<Value> {
        self.check_unlocked(&[inst])?;
        let i = self.instances.get(&inst).ok_or_else(|| anyhow!("no component {inst}"))?.clone();
        let (kind, w0, h0) = self.definitions.get(&i.def).and_then(sized).ok_or_else(|| anyhow!("{} isn't a door or window from the Door or Window tool", i.name))?;
        let Some((wall, face)) = self.opening_wall(inst) else {
            bail!("only a door or window in a wall the Wall tool made can be resized; delete this one and place a new one");
        };
        if self.part_info(wall)?["edited"] == true {
            bail!("that wall has been edited by hand since it was made, so it can't be rebuilt around the new size");
        }
        let side = self.wall_side(face)?;
        let (w, h) = (width.unwrap_or(w0), height.unwrap_or(h0));
        let up = if kind == "door" { 0.0 } else { sill.unwrap_or(i.transform.w_axis.z - side.z.0) };
        check_size(kind, w, h, up)?;
        let centre = i.transform.w_axis.truncate().dot(side.along) + w0 / 2.0;
        let t = Self::opening_frame(&side, kind, centre, w, h, up)?;
        self.refuse_overlap(&side, kind, t, w, h, Some(inst))?;
        let def = self.opening_definition(kind, w, h)?;
        let before = self.instances[&inst].clone();
        if let Some(x) = self.instances.get_mut(&inst) {
            x.def = def;
            x.transform = t;
        }
        match self.rebuild_part(wall, &Map::new(), false) {
            Ok(r) if r["openings_off_the_wall"].is_null() && r["openings_overlapping"].is_null() =>self.opening_info(inst),
            Ok(_) => bail!("the {kind} no longer fits that wall"),
            Err(e) => {
                self.instances.insert(inst, before);
                Err(e)
            }
        }
    }
}
