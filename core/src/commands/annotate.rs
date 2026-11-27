//! Guides, dimensions, text, units, saved views, photo matching and the read-only reports.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

/// What an agent or a plugin may save as scenes (#229's "show only" lists are copied into every undo step).
const MAX_SCENES_UNTRUSTED: usize = 500;
const MAX_SCENE_IDS_UNTRUSTED: usize = 20_000;

impl Engine {
    pub(super) fn run_annotate(&mut self, cmd: Command) -> Result<Value> {
        // the laid-out view is the session's, not the model's (and always of the whole model, never inside `edit`)
        if let Command::LayOutParts { on, ids, gap } = cmd {
            if !on {
                self.laid_out = None;
                self.laid_out_by_other = false;
                return Ok(json!({"laid_out": false}));
            }
            if self.ctx.is_some() {
                bail!("close the component you are editing first, then lay out the parts");
            }
            let view = crate::parts_layout::laid_out_view(&self.model, &ids, gap)?;
            // the person's own view stays theirs when a plugin or an agent asks for it again
            let persons = self.laid_out.is_some() && !self.laid_out_by_other;
            self.laid_out = Some(crate::commands::LayOutRequest { ids, gap });
            self.laid_out_by_other = self.untrusted && !persons;
            let mut v = crate::parts_layout::summary(&view);
            v["laid_out"] = json!(true);
            return Ok(v);
        }
        let ctx = self.ctx;
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Guide { point, direction } => {
                let id = m.new_id();
                m.guides.push(crate::model::Guide { id, p: v3(point), dir: direction.map(|d| v3(d).normalize()) });
                json!({"guide": id})
            }
            Command::Dimension { from, to, offset } => {
                let id = m.new_id();
                let (a, b) = (v3(from), v3(to));
                m.annotations.push(crate::model::Annotation::dimension(id, a, b, v3(offset)));
                json!({"dimension": id, "length": round(a.distance(b))})
            }
            Command::Text { at, text, leader } => {
                let id = m.new_id();
                m.annotations.push(crate::model::Annotation::Text { id, at: v3(at), text, leader: leader.map(v3) });
                json!({"text": id})
            }
            Command::Units { units } => {
                if !["m", "cm", "mm", "ft", "in"].contains(&units.as_str()) {
                    bail!("units must be m, cm, mm, ft or in");
                }
                self.model.units = units.clone();
                json!({"units": units})
            }
            Command::ClearGuides => {
                m.guides.clear();
                json!({})
            }
            Command::ShowView { name } => {
                let v = m.views.iter().find(|v| v.name == name).cloned().ok_or_else(|| anyhow::anyhow!("no view {name}"))?;
                if let Some(tags) = &v.tags {
                    for (t, on) in tags {
                        m.tags.insert(t.clone(), *on);
                    }
                }
                // a list from a file that names nothing left (empty, or every id gone) would hide the whole model:
                // such a scene shows as an ordinary one
                let only: Option<Vec<Id>> = v.only.as_ref().map(|o| o.iter().copied().filter(|id| m.faces.contains_key(id) || m.edges.contains_key(id) || m.groups.contains_key(id) || m.instances.contains_key(id)).collect()).filter(|o: &Vec<Id>| !o.is_empty());
                if let Some(only) = &only {
                    m.show_only(only);
                } else if let Some(hidden) = &v.hidden {
                    let h: std::collections::BTreeSet<Id> = hidden.iter().copied().collect();
                    for (id, g) in m.groups.iter_mut() {
                        g.hidden = h.contains(id);
                    }
                    for (id, i) in m.instances.iter_mut() {
                        i.hidden = h.contains(id);
                    }
                    for (id, f) in m.faces.iter_mut() {
                        f.hidden = h.contains(id);
                    }
                    // lines: a scene saved before lines were remembered lists none, so they all show again (a
                    // "show only" scene shown before it may have hidden them)
                    for (id, e) in m.edges.iter_mut() {
                        e.hidden = h.contains(id);
                    }
                }
                serde_json::to_value(&v)?
            }
            Command::SaveView { name, eye, target, fov, photo, style, section, camera_only, only } => {
                // an agent or a plugin keeps to a few hundred scenes and lists of what a scene shows that fit a
                // model; every scene is copied into each undo step
                if self.untrusted {
                    let new = !m.views.iter().any(|v| v.name == name);
                    if new && m.views.len() >= MAX_SCENES_UNTRUSTED {
                        bail!("this model has {} scenes; an agent or a plugin can add up to {MAX_SCENES_UNTRUSTED}", m.views.len());
                    }
                    if only.as_ref().is_some_and(|o| o.len() > MAX_SCENE_IDS_UNTRUSTED) {
                        bail!("a scene can show up to {MAX_SCENE_IDS_UNTRUSTED} things when an agent or a plugin saves it; name groups instead of every face");
                    }
                }
                let only = match only {
                    None => None,
                    Some(_) if camera_only => bail!("a scene can't be camera only and show only some things"),
                    Some(ids) => {
                        let mut ids: Vec<Id> = ids.into_iter().filter(|id| m.faces.contains_key(id) || m.edges.contains_key(id) || m.groups.contains_key(id) || m.instances.contains_key(id)).collect();
                        ids.sort_unstable();
                        ids.dedup();
                        if ids.is_empty() {
                            bail!("nothing to show: name the groups, components, faces or edges the scene shows");
                        }
                        Some(ids)
                    }
                };
                if let Some(p) = photo.as_deref() {
                    crate::safety::check_picture(p).map_err(|e| anyhow::anyhow!("photo: {e}"))?;
                }
                // re-saving a matched photo's scene keeps its photo
                let photo = photo.or_else(|| m.views.iter().find(|v| v.name == name).and_then(|v| v.photo.clone()));
                m.views.retain(|v| v.name != name);
                let (tags, hidden) = if camera_only {
                    (None, None)
                } else {
                    let hidden: Vec<Id> = m.groups.iter().filter(|(_, g)| g.hidden).map(|(k, _)| *k).chain(m.instances.iter().filter(|(_, i)| i.hidden).map(|(k, _)| *k)).chain(m.faces.iter().filter(|(_, f)| f.hidden).map(|(k, _)| *k)).chain(m.edges.iter().filter(|(_, e)| e.hidden).map(|(k, _)| *k)).collect();
                    (Some(m.tags.clone()), Some(hidden))
                };
                let (style, section) = if camera_only { (None, None) } else { (style, section) };
                m.views.push(crate::model::View { name, eye, target, fov, photo, style, tags, hidden, section, only });
                json!({})
            }
            Command::MatchPhoto { name, width, height, red, green, origin, distance, photo } => {
                // the picture is looked at before any work is done on it
                if let Some(p) = &photo {
                    crate::safety::check_picture(p).map_err(|e| anyhow::anyhow!("photo: {e}"))?;
                }
                let cam = crate::matchphoto::solve(width, height, red, green, origin, distance)?;
                m.views.retain(|v| v.name != name);
                m.views.push(crate::model::View { name: name.clone(), eye: cam.eye.to_array(), target: cam.target.to_array(), fov: cam.fov, photo, style: None, tags: None, hidden: None, section: None, only: None });
                json!({"view": name, "eye": cam.eye.to_array(), "target": cam.target.to_array(), "fov": cam.fov})
            }
            Command::DeleteView { name } => {
                m.views.retain(|v| v.name != name);
                json!({})
            }
            Command::Parts { ids } => json!({"parts": crate::plugins::parts(m, &ids), "units": m.units}),
            Command::PartsLayout { ids, gap } => crate::parts_layout::reply(m, &ids, gap)?,
            Command::LayOutParts { .. } => unreachable!("handled above"),
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
                    "read_only": self.read_only.as_ref().map(|r| r.reasons.clone()),
                })
            }
            Command::ProjectPhoto { view, ids, size } => {
                let faces = m.faces_of(&ids)?;
                let r = crate::photoproject::project(m, &view, &faces, size)?;
                json!({"faces": r.faces, "away": r.away})
            }
            Command::PhotoSolve { points, edges, fixed, scale, place, build, square } => {
                let mut sol = crate::photomodel::solve(m, &points, fixed.as_deref(), scale.map(|s| (s.a, s.b, s.length)))?;
                let squared = if square { crate::photomodel::square(&mut sol.points, &edges, 4.0) } else { 0 };
                // each photo's camera slides to its solved distance (same direction, same lens)
                for v in m.views.iter_mut() {
                    if let Some(d) = sol.distances.get(&v.name) {
                        let eye = DVec3::from(v.eye);
                        let moved = eye.normalize_or_zero() * *d;
                        v.target = (DVec3::from(v.target) + (moved - eye)).to_array();
                        v.eye = moved.to_array();
                    }
                }
                m.photo_points = points;
                m.photo_edges = edges.clone();
                if place {
                    let old = std::mem::take(&mut m.photo_guides);
                    m.guides.retain(|g| !old.contains(&g.id));
                    for p in sol.points.iter().flatten() {
                        let id = m.new_id();
                        m.guides.push(crate::model::Guide { id, p: DVec3::from(*p), dir: None });
                        m.photo_guides.push(id);
                    }
                }
                let mut built = 0;
                if build {
                    for [a, b] in &edges {
                        if let (Some(Some(p)), Some(Some(q))) = (sol.points.get(*a), sol.points.get(*b)) {
                            m.draw_line(DVec3::from(*p), DVec3::from(*q))?;
                            built += 1;
                        }
                    }
                }
                let mut v = serde_json::to_value(&sol)?;
                v["built"] = json!(built);
                v["squared"] = json!(squared);
                v
            }
            Command::Finish { ids, label, roughness, metalness, glass, emission } => {
                let faces = m.faces_of(&ids)?;
                let label: String = label.trim().chars().take(40).collect();
                if label.is_empty() {
                    bail!("name the finish");
                }
                let mut made = std::collections::BTreeSet::new();
                for fid in &faces {
                    let base = m.faces.get(fid).and_then(|f| f.material.clone());
                    // the base without an earlier finish's suffix
                    let root = base.as_deref().map(|b| b.rsplit_once(" (").filter(|(_, s)| s.ends_with(')')).map(|(r, _)| r.to_string()).unwrap_or_else(|| b.to_string()));
                    let name = format!("{} ({label})", root.as_deref().unwrap_or("Default"));
                    if !m.materials.contains_key(&name) {
                        let mut mat = root.as_deref().and_then(|r| m.materials.get(r)).or_else(|| base.as_deref().and_then(|b| m.materials.get(b))).cloned().unwrap_or_default();
                        mat.roughness = roughness.map(|x| x.clamp(0.0, 1.0));
                        mat.metalness = metalness.map(|x| x.clamp(0.0, 1.0));
                        mat.glass = glass;
                        mat.emission = emission.map(|x| x.clamp(0.0, 100.0));
                        m.materials.insert(name.clone(), mat);
                    }
                    if let Some(f) = m.faces.get_mut(fid) {
                        f.material = Some(name.clone());
                    }
                    made.insert(name);
                }
                json!({"faces": faces.len(), "materials": made})
            }
            Command::Options { id } => {
                let i = m.instances.get(&id).ok_or_else(|| anyhow::anyhow!("{id} is not a component placement"))?;
                json!({"options": crate::dynamic::options(i, m.definitions.get(&i.def))})
            }
            Command::Report { path } => {
                let rows = crate::dynamic::report(m);
                if let Some(p) = &path {
                    crate::safety::write_output(p, crate::dynamic::report_csv(&rows))?;
                }
                json!({"rows": rows, "path": path})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
