//! Groups and components: making, placing, opening and closing them, and picking what is in the model.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_groups(&mut self, cmd: Command) -> Result<Value> {
        let self_untrusted = self.untrusted;
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::BeginGroup { name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.begin_group(n)})
            }
            Command::EndGroup => {
                // back out one level (to the group this one sits in)
                m.active_group = m.active_group.and_then(|g| m.groups.get(&g)).and_then(|g| g.parent);
                json!({})
            }
            Command::Group { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.group(&ids, n)?})
            }
            Command::Faces { group, facing, outlines } => {
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
                        let mut o = json!({
                            "id": id,
                            "normal": r(m.normal(id)),
                            "center": r(m.center(id)),
                            "area": round(m.face_area(id)),
                            "points": f.outer.len(),
                            "holes": f.holes.len(),
                            "group": f.group,
                            "material": f.material,
                        });
                        if outlines {
                            o["outer"] = json!(m.pts(&f.outer).into_iter().map(r).collect::<Vec<_>>());
                            o["inner"] = json!(f.holes.iter().map(|h| m.pts(h).into_iter().map(r).collect::<Vec<_>>()).collect::<Vec<_>>());
                        }
                        o
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
            Command::Copy { group, by, name } => json!({"group": m.with_deadline(self_untrusted.then_some(UNTRUSTED_COPY_SECONDS), |m| m.copy_group(group, v3(by), name))?}),
            Command::MakeComponent { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Component {}", m.definitions.len() + 1));
                let inst = m.with_deadline(self_untrusted.then_some(UNTRUSTED_COPY_SECONDS), |m| m.make_component(&ids, n))?;
                json!({"instance": inst, "component": m.instances[&inst].def})
            }
            Command::Place { component, at, rotate, name, host } => {
                if self.ctx.is_some() {
                    bail!("Close the component you're editing first: a component can't be placed while another one is open for editing");
                }
                let key = match &component {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let Some(def) = m.find_definition(&key) else { bail!("no component {key}") };
                let (instance, opening) = m.keep_closed(|m| {
                    let i = m.place(def, v3(at), rotate, name)?;
                    Ok((i, place_cut(m, i, host)))
                })?;
                json!({"instance": instance, "opening": opening})
            }
            Command::ComponentFaceMe { id, on } => {
                let def = m.instances.get(&id).map(|i| i.def).unwrap_or(id);
                let d = m.definitions.get_mut(&def).ok_or_else(|| anyhow::anyhow!("{id} is not a component"))?;
                d.face_me = on;
                json!({"component": def, "face_me": on})
            }
            Command::ComponentCuts { id, cuts } => {
                let def = m.instances.get(&id).map(|i| i.def).unwrap_or(id);
                let d = m.definitions.get_mut(&def).ok_or_else(|| anyhow::anyhow!("{id} is not a component"))?;
                d.cuts = cuts;
                json!({"component": def, "cuts": cuts})
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
                // out of a component; or out of a group, one level at a time
                if self.ctx.is_some() {
                    self.ctx = None;
                } else {
                    m.active_group = m.active_group.and_then(|g| m.groups.get(&g)).and_then(|g| g.parent);
                }
                json!({})
            }
            Command::Explode { id } => json!({"group": m.with_deadline(self_untrusted.then_some(UNTRUSTED_COPY_SECONDS), |m| m.explode(id))?}),
            Command::Components => json!({"components": m.definitions.iter().map(|(id, d)| json!({
                "id": id, "name": d.name, "faces": d.model.faces.len(),
                "placed": m.instances.values().filter(|i| i.def == *id).count(),
            })).collect::<Vec<_>>()}),
            Command::Ungroup { id } => {
                m.check_unlocked(&[id])?;
                m.ungroup(id)?;
                json!({})
            }
            Command::MakeUnique { id } => json!({"component": m.make_unique(id)?}),
            Command::Connected { id, all } => {
                let (faces, edges) = m.connected(id, all)?;
                json!({"faces": faces, "edges": edges})
            }
            Command::SameAs { material, tag, by } => {
                let ctx = m.active_group;
                if by == "tag" {
                    let groups: Vec<Id> = m.groups.iter().filter(|(_, g)| g.parent == ctx && g.tag == tag).map(|(k, _)| *k).collect();
                    let instances: Vec<Id> = m.instances.iter().filter(|(_, i)| i.parent == ctx && i.tag == tag).map(|(k, _)| *k).collect();
                    json!({"groups": groups, "instances": instances})
                } else {
                    let faces: Vec<Id> = m.faces.iter().filter(|(_, f)| f.group == ctx && f.material == material && !f.hidden).map(|(k, _)| *k).collect();
                    json!({"faces": faces})
                }
            }
            Command::Resize { factor } => {
                m.resize(factor)?;
                json!({"factor": factor})
            }
            Command::SetAxes { id, origin, x, y, reset } => {
                let frame = if reset {
                    None
                } else {
                    let (Some(o), Some(x), Some(y)) = (origin, x, y) else { bail!("set_axes needs origin, x and y (or reset)") };
                    Some(crate::model::axes_frame(o, x, y)?)
                };
                let (whose, axes) = set_axes_of(m, id, frame)?;
                json!({"axes": axes, "of": whose})
            }
            Command::AlignAxes { face, id } => {
                let frame = m.face_axes(face)?;
                let (whose, axes) = set_axes_of(m, id, Some(frame))?;
                json!({"axes": axes, "of": whose})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}

/// Give `frame` (None: the default) to whoever's axes `id` names: a group, a component placement (whose
/// component keeps them in its own coordinates, for all its copies), or with no `id` the open group, else the
/// model being edited. Says whose they were and what they are now, in the coordinates the command used.
fn set_axes_of(m: &mut Model, id: Option<Id>, frame: Option<[[f64; 3]; 3]>) -> Result<(Value, Option<[[f64; 3]; 3]>)> {
    match id {
        Some(g) if m.groups.contains_key(&g) => {
            m.groups.get_mut(&g).unwrap().axes = frame;
            Ok((json!({"group": g}), frame))
        }
        Some(i) => {
            let Some(inst) = m.instances.get(&i) else { bail!("{i} is not a group or component placement") };
            let (def, t) = (inst.def, inst.transform);
            let local = match frame {
                Some(f) => Some(crate::model::axes_through(t.inverse(), f).ok_or_else(|| anyhow::anyhow!("that placement is flattened, so it can't have axes"))?),
                None => None,
            };
            let Some(d) = m.definitions.get_mut(&def) else { bail!("{i} is not a group or component placement") };
            std::sync::Arc::make_mut(&mut d.model).axes = local;
            Ok((json!({"component": def}), frame))
        }
        None => match m.active_group {
            Some(g) if m.groups.contains_key(&g) => {
                m.groups.get_mut(&g).unwrap().axes = frame;
                Ok((json!({"group": g}), frame))
            }
            _ => {
                m.axes = frame;
                Ok((json!("model"), frame))
            }
        },
    }
}
