//! UV editor commands (`uv_seam`, `uv_unwrap`, `uv_pack`, `uv_lightmap`, `uv_set`, `uv_transform`, `uv_info`). The math
//! lives in `crate::unwrap`. Design notes: docs/dev/uv-editor.md.

use super::*;
use crate::unwrap::{Method, Opts, Pin, SeamMode};

fn set_of(s: Option<u8>) -> Result<u8> {
    match s.unwrap_or(1) {
        n @ (1 | 2) => Ok(n),
        _ => bail!("set must be 1 (the picture's UVs) or 2 (the lightmap)"),
    }
}

/// Run `f` on the faces/groups named directly, and on the whole definition of each placed component named (a component is
/// unwrapped in its own frame, so every placement shows it). Sums what `f` returns.
fn each_target(m: &mut Model, ids: &[Id], mut f: impl FnMut(&mut Model, &[Id]) -> Result<usize>) -> Result<usize> {
    let (placed, direct): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| m.instances.contains_key(id));
    let mut n = 0;
    if !direct.is_empty() {
        n += f(m, &direct)?;
    }
    for i in placed {
        m.check_unlocked(&[i])?;
        let def = m.instances[&i].def;
        let dm = std::sync::Arc::make_mut(&mut m.definitions.get_mut(&def).unwrap().model);
        let all: Vec<Id> = dm.faces.keys().copied().collect();
        n += f(dm, &all)?;
    }
    Ok(n)
}

fn targets(faces: Option<Vec<Id>>, entity: Option<Id>, what: &str) -> Result<Vec<Id>> {
    let mut ids = faces.unwrap_or_default();
    ids.extend(entity);
    if ids.is_empty() {
        bail!("{what} needs faces or an entity (a group or component)");
    }
    Ok(ids)
}

impl Engine {
    pub(super) fn run_uv(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::UvSeam { edges, faces, entity, on, auto } => {
                let on = on.unwrap_or(true);
                let mut ids = edges.unwrap_or_default();
                if let Some(angle) = auto {
                    if !angle.is_finite() || angle <= 0.0 || angle >= 180.0 {
                        bail!("auto must be an angle between 0 and 180 degrees");
                    }
                    let sel = targets(faces, entity, "uv_seam with auto")?;
                    ids.extend(m.hard_edges(&sel, angle)?);
                } else if ids.is_empty() {
                    bail!("uv_seam needs edges, or auto (an angle) with faces or an entity");
                }
                ids.sort_unstable();
                ids.dedup();
                json!({"changed": m.set_seams(&ids, on)?, "edges": ids, "on": on})
            }
            Command::UvUnwrap { faces, entity, method, seams, angle, margin, pins } => {
                let ids = targets(faces, entity, "uv_unwrap")?;
                let mut o = Opts::default();
                if let Some(s) = method {
                    o.method = Method::parse(&s)?;
                }
                if let Some(s) = seams {
                    o.seams = SeamMode::parse(&s)?;
                }
                o.angle = angle.unwrap_or(o.angle);
                o.margin = margin.unwrap_or(o.margin);
                for p in pins.unwrap_or_default() {
                    o.pins.push(Pin { face: p[0] as Id, corner: p[1] as usize, uv: [p[2], p[3]] });
                }
                let mut report = None;
                // placed components are unwrapped in their own definitions; the report is the last one's
                each_target(m, &ids, |mm, i| {
                    let r = mm.unwrap_uv(i, &o)?;
                    let n = r.faces;
                    report = Some(r);
                    Ok(n)
                })?;
                serde_json::to_value(report.unwrap_or_default())?
            }
            Command::UvPack { faces, entity, margin, set } => {
                let ids = targets(faces, entity, "uv_pack")?;
                let set = set_of(set)?;
                let margin = margin.unwrap_or(0.01);
                json!({"islands": each_target(m, &ids, |mm, i| mm.pack_uv(i, margin, set))?})
            }
            Command::UvLightmap { faces, entity, margin, angle, clear } => {
                let ids = targets(faces, entity, "uv_lightmap")?;
                if clear {
                    let n = each_target(m, &ids, |mm, i| {
                        let all = mm.uv_faces(i)?;
                        mm.set_uv_corners(&all.iter().map(|f| (*f, None)).collect::<Vec<_>>(), 2)
                    })?;
                    return Ok(json!({"cleared": n}));
                }
                let o = Opts { margin: margin.unwrap_or(0.02), angle: angle.unwrap_or(60.0), set: 2, ..Opts::default() };
                let mut report = None;
                each_target(m, &ids, |mm, i| {
                    let r = mm.unwrap_uv(i, &o)?;
                    let n = r.faces;
                    report = Some(r);
                    Ok(n)
                })?;
                serde_json::to_value(report.unwrap_or_default())?
            }
            Command::UvSet { uv, set } => {
                let set = set_of(set)?;
                let mut list = vec![];
                for (k, v) in uv {
                    let id: Id = k.parse().map_err(|_| anyhow::anyhow!("\"{k}\" is not a face id"))?;
                    list.push((id, v));
                }
                json!({"faces": m.set_uv_corners(&list, set)?})
            }
            Command::UvTransform { faces, vertices, matrix, set } => {
                let set = set_of(set)?;
                json!({"faces": m.transform_uv(&faces, vertices.as_deref(), matrix, set)?})
            }
            Command::UvInfo { faces, entity, set } => {
                let ids = targets(faces, entity, "uv_info")?;
                let set = set_of(set)?;
                let (placed, direct): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| m.instances.contains_key(id));
                let mut out = if direct.is_empty() { json!({"faces": [], "without": [], "islands": 0, "seams": []}) } else { m.uv_info(&direct, set)? };
                // a placed component is read from its definition (its faces are numbered there)
                for i in placed {
                    let def = m.instances[&i].def;
                    let dm = &m.definitions[&def].model;
                    let all: Vec<Id> = dm.faces.keys().copied().collect();
                    let part = dm.uv_info(&all, set)?;
                    let base = out["islands"].as_u64().unwrap_or(0);
                    for mut f in part["faces"].as_array().cloned().unwrap_or_default() {
                        f["island"] = json!(f["island"].as_u64().unwrap_or(0) + base);
                        out["faces"].as_array_mut().unwrap().push(f);
                    }
                    for k in ["without", "seams"] {
                        out[k].as_array_mut().unwrap().extend(part[k].as_array().cloned().unwrap_or_default());
                    }
                    out["islands"] = json!(base + part["islands"].as_u64().unwrap_or(0));
                }
                out
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
