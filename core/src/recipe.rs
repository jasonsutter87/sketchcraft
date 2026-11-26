//! Walls, stairs, roofs and columns that remember their settings (#263, from Blender 3D Architect's first look at
//! 0.3.8: "we don't have any option to change the settings for the walls, for the stairs, roof").
//!
//! A Wall, Stairs, Roof or Column tool places ordinary geometry in its own group, as before, and the group now keeps
//! the recipe that made it: the settings, where it has been moved or turned to since (`frame`), and a fingerprint of
//! its geometry (`print`). `rebuild` changes a setting and remakes the geometry in place, in the same group, with
//! doors and windows cut into it again. When the geometry has been edited by hand since (the fingerprint no longer
//! matches), a rebuild would throw those edits away, so it is refused unless the caller says `force`.
//!
//! A recipe lives only on a group the tool made. Room-mode walls are not given one: rooms rebuild their own walls.

use crate::model::{Id, Model};
use anyhow::{anyhow, bail, Result};
use glam::{DMat3, DMat4, DVec3};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// The most corners a remembered wall path or roof outline may have (a file could name any number).
const MAX_POINTS: usize = 2000; // the Wall tool takes no more from an agent (policy MAX_POINTS_UNTRUSTED)

/// What a tool made, and with which settings. Coordinates are where it was first made; `Recipe::frame` carries it to
/// where it is now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    Wall {
        points: Vec<[f64; 3]>,
        thickness: f64,
        height: f64,
        #[serde(default)]
        closed: bool,
    },
    Stairs {
        at: [f64; 3],
        direction: f64,
        width: f64,
        rise: f64,
        tread: f64,
    },
    Roof {
        outline: Vec<[f64; 3]>,
        /// gable, hip or shed
        style: String,
        pitch: f64,
        overhang: f64,
        thickness: f64,
    },
    Column {
        at: [f64; 3],
        size: f64,
        height: f64,
        #[serde(default)]
        round: bool,
    },
}

/// The settings a person may change, per kind (the rest say where it stands).
fn editable(kind: &str) -> &'static [&'static str] {
    match kind {
        "wall" => &["thickness", "height"],
        "stairs" => &["width", "rise", "tread"],
        "roof" => &["style", "pitch", "overhang", "thickness"],
        "column" => &["size", "height", "round"],
        _ => &[],
    }
}

impl Part {
    fn kind(&self) -> &'static str {
        match self {
            Part::Wall { .. } => "wall",
            Part::Stairs { .. } => "stairs",
            Part::Roof { .. } => "roof",
            Part::Column { .. } => "column",
        }
    }

    /// Every number finite and in range, and no more corners than `MAX_POINTS` (a recipe can come from a file).
    fn check(&self) -> Result<()> {
        let ok = |x: f64| crate::model::sane(x);
        let ok3 = |p: &[f64; 3]| p.iter().all(|&x| ok(x));
        let fine = match self {
            Part::Wall { points, thickness, height, .. } => points.len() <= MAX_POINTS && points.iter().all(ok3) && ok(*thickness) && ok(*height),
            Part::Stairs { at, direction, width, rise, tread } => ok3(at) && [*direction, *width, *rise, *tread].iter().all(|&x| ok(x)),
            Part::Roof { outline, style, pitch, overhang, thickness } => {
                outline.len() <= MAX_POINTS && outline.iter().all(ok3) && ["gable", "hip", "shed"].contains(&style.as_str()) && [*pitch, *overhang, *thickness].iter().all(|&x| ok(x))
            }
            Part::Column { at, size, height, .. } => ok3(at) && ok(*size) && ok(*height),
        };
        if !fine {
            bail!("those {} settings are out of range", self.kind());
        }
        Ok(())
    }

    /// The settings a person may change, as JSON.
    fn settings(&self) -> Value {
        let all = serde_json::to_value(self).unwrap_or_default();
        let keep: Map<String, Value> = editable(self.kind()).iter().filter_map(|k| all.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
        Value::Object(keep)
    }
}

/// A part's recipe: the settings, the move or turn since it was made, and a fingerprint of its geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub part: Part,
    /// column-major; identity when it hasn't been moved
    pub frame: [f64; 16],
    /// `Model::part_print` when it was last made
    pub print: u64,
}

/// A recipe in a file that can't be read (damaged, or written by a newer version) is dropped, never fatal: the
/// geometry is all that matters, and it is kept.
pub(crate) fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Recipe>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.and_then(|v| serde_json::from_value::<Recipe>(v).ok()).filter(|r| r.part.check().is_ok() && frame_ok(&r.frame)))
}

/// A frame from a file: a move or turn about a vertical axis, within range (a scaled or huge one would rebuild the
/// part out of shape or out of the model's range).
fn frame_ok(f: &[f64; 16]) -> bool {
    f.iter().all(|&x| crate::model::sane(x)) && rigid(DMat4::from_cols_array(f)) && f[15] == 1.0 && f[3] == 0.0 && f[7] == 0.0 && f[11] == 0.0
}

/// A part's paint and edge styles, kept across a rebuild (see `Model::part_looks`).
struct Looks {
    frame: DMat4,
    faces: Vec<(DVec3, f64, Option<String>)>,
    edges: std::collections::HashMap<([i64; 3], [i64; 3]), (bool, bool, Option<Box<crate::drafting::EdgeDraft>>, bool)>,
}

/// A move or turn about a vertical axis, no scaling and no mirror: a remembered part can follow it.
fn rigid(t: DMat4) -> bool {
    let m = DMat3::from_mat4(t);
    let d = m.transpose() * m - DMat3::IDENTITY;
    d.to_cols_array().iter().all(|x| x.abs() < 1e-6) && m.determinant() > 0.0 && (m * DVec3::Z - DVec3::Z).length() < 1e-6
}

impl Model {
    /// Make `part` in a new group and give the group its recipe. Returns the group and anything the tool reports.
    pub fn build_part(&mut self, part: Part) -> Result<(Id, Value)> {
        let (g, extra) = self.make_part(&part)?;
        if part.check().is_err() {
            // built (the tool checks its own input) but too big to remember, e.g. a wall path of 10,000 corners
            return Ok((g, extra));
        }
        let print = self.part_print(g, DMat4::IDENTITY);
        if let Some(grp) = self.groups.get_mut(&g) {
            grp.recipe = Some(Recipe { part, frame: DMat4::IDENTITY.to_cols_array(), print });
        }
        Ok((g, extra))
    }

    fn make_part(&mut self, part: &Part) -> Result<(Id, Value)> {
        let v = |p: &[f64; 3]| DVec3::from(*p);
        Ok(match part {
            Part::Wall { points, thickness, height, closed } => (self.wall(&points.iter().map(v).collect::<Vec<_>>(), *thickness, *height, *closed)?, json!({})),
            Part::Stairs { at, direction, width, rise, tread } => {
                let (g, n) = self.stairs(v(at), *direction, *width, *rise, *tread)?;
                (g, json!({"risers": n, "riser_height": rise / n as f64}))
            }
            Part::Roof { outline, style, pitch, overhang, thickness } => (self.roof_over(&outline.iter().map(v).collect::<Vec<_>>(), style, *pitch, *overhang, *thickness)?, json!({})),
            Part::Column { at, size, height, round } => (self.column(v(at), *size, *height, *round)?, json!({})),
        })
    }

    /// What a part looks like beyond its shape: each painted face's material by its plane, and each styled edge's
    /// style by its ends, in the part's own frame (so a rebuild can put them back where they still fit).
    fn part_looks(&self, g: Id, frame: DMat4) -> Looks {
        let back = frame.inverse();
        let key = |p: DVec3| {
            let q = back.transform_point3(p);
            [(q.x * 1e4).round() as i64, (q.y * 1e4).round() as i64, (q.z * 1e4).round() as i64]
        };
        let faces = self
            .faces
            .iter()
            .filter(|(_, f)| f.group == Some(g) && f.material.is_some())
            .map(|(&k, f)| {
                let n = back.transform_vector3(self.normal(k));
                let p = back.transform_point3(self.p(f.outer[0]));
                (n, n.dot(p), f.material.clone())
            })
            .collect();
        let edges = self
            .edges
            .values()
            .filter(|e| e.group == Some(g) && (e.soft || e.hidden || e.draft.is_some() || e.seam))
            .map(|e| {
                let (a, b) = (key(self.p(e.a)), key(self.p(e.b)));
                ((a.min(b), a.max(b)), (e.soft, e.hidden, e.draft.clone(), e.seam))
            })
            .collect();
        Looks { frame, faces, edges }
    }

    /// Put `looks` back on the rebuilt part: a face takes the paint of the old face on its plane, facing its way; an
    /// edge with the same two ends takes its old style. Faces and edges the new shape doesn't have are simply gone.
    fn put_back_looks(&mut self, g: Id, looks: &Looks) {
        if looks.faces.is_empty() && looks.edges.is_empty() {
            return;
        }
        let back = looks.frame.inverse();
        let ids: Vec<Id> = self.faces.iter().filter(|(_, f)| f.group == Some(g)).map(|(&k, _)| k).collect();
        for k in ids {
            let n = back.transform_vector3(self.normal(k));
            let w = n.dot(back.transform_point3(self.p(self.faces[&k].outer[0])));
            let paint = looks.faces.iter().filter(|(m, d, _)| m.dot(n) > 0.999 && (d - w).abs() < 1e-4).map(|(_, _, mat)| mat.clone()).next();
            if let (Some(mat), Some(f)) = (paint, self.faces.get_mut(&k)) {
                f.material = mat;
            }
        }
        let key = |p: DVec3| {
            let q = back.transform_point3(p);
            [(q.x * 1e4).round() as i64, (q.y * 1e4).round() as i64, (q.z * 1e4).round() as i64]
        };
        let at: Vec<(Id, ([i64; 3], [i64; 3]))> = self
            .edges
            .iter()
            .filter(|(_, e)| e.group == Some(g))
            .map(|(&k, e)| {
                let (a, b) = (key(self.p(e.a)), key(self.p(e.b)));
                (k, (a.min(b), a.max(b)))
            })
            .collect();
        for (k, ends) in at {
            if let (Some((soft, hidden, draft, seam)), Some(e)) = (looks.edges.get(&ends), self.edges.get_mut(&k)) {
                e.soft = *soft;
                e.hidden = *hidden;
                e.draft = draft.clone();
                e.seam = *seam;
            }
        }
    }

    /// An opening's place on the rebuilt part `g`: where it was, if its back is still on a face, or else slid along the
    /// face's normal `n` onto the nearest face of `g` facing the same way (within 0.6 m, a wall's thickness).
    fn follow_face(&self, g: Id, back: &[DVec3], t: DMat4, n: DVec3) -> Option<(DMat4, Id)> {
        let world: Vec<DVec3> = back.iter().map(|p| t.transform_point3(*p)).collect();
        if let Some(host) = self.wall_face_for(&world, &[g]) {
            return Some((t, host));
        }
        let here = n.dot(*world.first()?);
        let mut shifts: Vec<f64> = self
            .faces
            .iter()
            .filter(|(&k, f)| f.group == Some(g) && !f.outer.is_empty() && self.normal(k).dot(n) > 0.999)
            .map(|(_, f)| n.dot(self.p(f.outer[0])) - here)
            .filter(|d| d.abs() <= 0.6)
            .collect();
        shifts.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
        shifts.into_iter().find_map(|d| {
            let moved = DMat4::from_translation(n * d) * t;
            let world: Vec<DVec3> = back.iter().map(|p| moved.transform_point3(*p)).collect();
            self.wall_face_for(&world, &[g]).map(|host| (moved, host))
        })
    }

    /// A fingerprint of the group's own geometry, seen from where it was made (so moving it whole keeps the print).
    pub(crate) fn part_print(&self, g: Id, frame: DMat4) -> u64 {
        let back = frame.inverse();
        let mut pts: Vec<[i64; 3]> = self
            .vertices
            .values()
            .filter(|x| x.group == Some(g))
            .map(|x| {
                let p = back.transform_point3(x.p);
                [(p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64]
            })
            .collect();
        pts.sort_unstable();
        let faces = self.faces.values().filter(|f| f.group == Some(g)).count() as u64;
        // FNV-1a: the same on every machine and every version
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for x in pts.iter().flatten().map(|&x| x as u64).chain([faces]) {
            for b in x.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    }

    /// A moved group's recipe follows the move; one that was scaled, mirrored or tipped over is forgotten (its
    /// settings no longer describe it).
    pub(crate) fn follow_recipes(&mut self, groups: &std::collections::BTreeSet<Id>, t: DMat4) {
        let keep = rigid(t);
        for g in groups {
            let Some(grp) = self.groups.get_mut(g) else { continue };
            let Some(r) = grp.recipe.as_mut() else { continue };
            if keep {
                r.frame = (t * DMat4::from_cols_array(&r.frame)).to_cols_array();
            } else {
                grp.recipe = None;
            }
        }
    }

    /// What a group was made as and its settings: `{kind, settings, edited}`, or `{kind: null}` for any other group.
    pub fn part_info(&self, g: Id) -> Result<Value> {
        let grp = self.groups.get(&g).ok_or_else(|| anyhow!("no group {g}"))?;
        let Some(r) = &grp.recipe else { return Ok(json!({"group": g, "kind": null})) };
        let edited = self.part_print(g, DMat4::from_cols_array(&r.frame)) != r.print;
        Ok(json!({"group": g, "kind": r.part.kind(), "settings": r.part.settings(), "edited": edited}))
    }

    /// Change settings of a remembered part and remake it in place: same group (name, tag, place in the Outliner),
    /// doors and windows cut into it again. Refused when it has been edited by hand since, unless `force`.
    pub fn rebuild_part(&mut self, g: Id, set: &Map<String, Value>, force: bool) -> Result<Value> {
        self.check_unlocked(&[g])?;
        let grp = self.groups.get(&g).ok_or_else(|| anyhow!("no group {g}"))?;
        let r = grp.recipe.clone().ok_or_else(|| anyhow!("{} wasn't made by the Wall, Stairs, Roof or Column tool, so it has no settings to change", grp.name))?;
        let frame = DMat4::from_cols_array(&r.frame);
        if !force && self.part_print(g, frame) != r.print {
            bail!("{} has been edited by hand since it was made: rebuilding it would undo those edits (pass force to rebuild anyway)", grp.name);
        }
        let kind = r.part.kind();
        let mut v = serde_json::to_value(&r.part)?;
        for (k, x) in set {
            if !editable(kind).contains(&k.as_str()) {
                bail!("a {kind} has no setting {k} (it has {})", editable(kind).join(", "));
            }
            v[k] = x.clone();
        }
        let part: Part = serde_json::from_value(v).map_err(|e| anyhow!("bad {kind} setting: {e}"))?;
        part.check()?;

        // the doors and windows cut into it, to cut again
        // (with the outward normal of the face each sits on, to follow that face if the wall gets thicker or thinner)
        let mut openings: Vec<(Id, std::sync::Arc<Model>, DMat4, DVec3)> = Vec::new();
        for (&id, inst) in &self.instances {
            let Some(def) = self.definitions.get(&inst.def).filter(|d| d.cuts) else { continue };
            let Some(back) = crate::openings::back_outline(&def.model) else { continue };
            let world: Vec<DVec3> = back.iter().map(|p| inst.transform.transform_point3(*p)).collect();
            if let Some(face) = self.wall_face_for(&world, &[g]) {
                openings.push((id, def.model.clone(), inst.transform, self.normal(face)));
            }
        }

        // the paint on its faces and the styles on its edges, to put back on the matching ones (R0311-R-E07)
        let looks = self.part_looks(g, frame);
        self.check_deadline()?;
        // the new geometry first (if the settings can't be built, nothing has changed), then swap it in
        let prev = self.active_group;
        self.active_group = self.groups[&g].parent;
        let made = self.make_part(&part);
        self.active_group = prev;
        let (fresh, extra) = made?;
        self.check_deadline()?;
        self.faces.retain(|_, f| f.group != Some(g));
        self.edges.retain(|_, e| e.group != Some(g));
        self.vertices.retain(|_, x| x.group != Some(g));
        for f in self.faces.values_mut().filter(|f| f.group == Some(fresh)) {
            f.group = Some(g);
        }
        for e in self.edges.values_mut().filter(|e| e.group == Some(fresh)) {
            e.group = Some(g);
        }
        for x in self.vertices.values_mut().filter(|x| x.group == Some(fresh)) {
            x.group = Some(g);
            x.p = frame.transform_point3(x.p);
        }
        self.groups.remove(&fresh);
        self.put_back_looks(g, &looks);

        let (mut kept, mut lost) = (0, 0);
        for (id, def, t, n) in openings {
            self.check_deadline()?;
            let back = crate::openings::back_outline(&def).unwrap_or_default();
            // where the face it sat on is now: a thicker or thinner wall moves its sides out or in (R0311-R-E07)
            let Some((t, host)) = self.follow_face(g, &back, t, n) else {
                lost += 1;
                continue;
            };
            if crate::openings::cut(self, &def, t, host).is_ok() {
                if let Some(i) = self.instances.get_mut(&id) {
                    i.transform = t;
                }
                kept += 1;
            } else {
                lost += 1;
            }
        }
        let print = self.part_print(g, frame);
        if let Some(grp) = self.groups.get_mut(&g) {
            grp.recipe = Some(Recipe { part, frame: r.frame, print });
        }
        let mut out = self.part_info(g)?;
        out["openings"] = json!(kept);
        if lost > 0 {
            out["openings_off_the_wall"] = json!(lost);
        }
        if let (Value::Object(o), Value::Object(x)) = (&mut out, extra) {
            o.extend(x);
        }
        Ok(out)
    }
}
