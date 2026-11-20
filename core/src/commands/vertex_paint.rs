//! Vertex painting commands (`vertex_paint`, `vertex_paint_clear`, `vertex_colors`). The maths live in `crate::vertex_paint`.

use super::*;
use crate::vertex_paint::{parse_color, Falloff, Stroke, WHITE};

impl Engine {
    pub(super) fn run_vertex_paint(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::VertexPaint { faces, entity, color, points, radius, strength, falloff, erase } => {
                let mut ids = faces.unwrap_or_default();
                ids.extend(entity);
                if ids.is_empty() {
                    bail!("vertex_paint needs faces or an entity (a group)");
                }
                let target = if erase {
                    WHITE
                } else {
                    parse_color(&color.ok_or_else(|| anyhow::anyhow!("vertex_paint needs a color (or erase: true)"))?)?
                };
                let strength = strength.unwrap_or(1.0);
                if !(0.0..=1.0).contains(&strength) {
                    bail!("strength is 0..1");
                }
                let points: Vec<DVec3> = points.unwrap_or_default().into_iter().map(v3).collect();
                let stroke = Stroke {
                    color: target,
                    points,
                    radius: radius.unwrap_or(0.1),
                    strength,
                    falloff: Falloff::parse(falloff.as_deref().unwrap_or("smooth"))?,
                };
                // a placed component is painted in its own frame (every placement of it shows the colours)
                let (placed, direct): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| m.instances.contains_key(id));
                // one budget for the whole command, however many placed components it reaches
                let mut work = 0u64;
                let (mut nf, mut nc) = if direct.is_empty() { (0, 0) } else { m.vertex_paint_within(&direct, &stroke, &mut work)? };
                for i in placed {
                    m.check_unlocked(&[i])?;
                    let inst = &m.instances[&i];
                    let (def, back) = (inst.def, inst.transform.inverse());
                    let scale = inst.transform.determinant().abs().cbrt().max(1e-9);
                    let local = Stroke { points: stroke.points.iter().map(|p| back.transform_point3(*p)).collect(), radius: stroke.radius / scale, ..stroke.clone() };
                    let dm = std::sync::Arc::make_mut(&mut m.definitions.get_mut(&def).unwrap().model);
                    let all: Vec<Id> = dm.faces.keys().copied().collect();
                    let (a, b) = dm.vertex_paint_within(&all, &local, &mut work)?;
                    nf += a;
                    nc += b;
                }
                json!({"faces": nf, "corners": nc})
            }
            Command::VertexPaintClear { faces, entity } => {
                let mut ids = faces.unwrap_or_default();
                ids.extend(entity);
                if ids.is_empty() {
                    bail!("vertex_paint_clear needs faces or an entity (a group)");
                }
                let (placed, direct): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| m.instances.contains_key(id));
                let mut n = if direct.is_empty() { 0 } else { m.vertex_paint_clear(&direct)? };
                for i in placed {
                    m.check_unlocked(&[i])?;
                    let def = m.instances[&i].def;
                    let dm = std::sync::Arc::make_mut(&mut m.definitions.get_mut(&def).unwrap().model);
                    let all: Vec<Id> = dm.faces.keys().copied().collect();
                    n += dm.vertex_paint_clear(&all)?;
                }
                json!({"cleared": n})
            }
            Command::VertexColors { faces } => {
                let mut out = serde_json::Map::new();
                let one = |out: &mut serde_json::Map<String, Value>, id: Id, f: &crate::model::Face| -> Result<()> {
                    out.insert(id.to_string(), serde_json::to_value(f.vcol.as_ref().filter(|u| u.fits(f)).map(|u| &u.outer))?);
                    Ok(())
                };
                for id in faces {
                    // a face, a group (its faces), or a placed component (the faces of its definition)
                    if let Some(f) = m.faces.get(&id) {
                        one(&mut out, id, f)?;
                    } else if let Some(i) = m.instances.get(&id) {
                        for (fid, f) in &m.definitions[&i.def].model.faces {
                            one(&mut out, *fid, f)?;
                        }
                    } else if m.groups.contains_key(&id) {
                        for fid in m.faces_of(&[id])? {
                            one(&mut out, fid, &m.faces[&fid])?;
                        }
                    } else {
                        let f = m.definitions.values().find_map(|d| d.model.faces.get(&id)).ok_or_else(|| anyhow::anyhow!("no face {id}"))?;
                        one(&mut out, id, f)?;
                    }
                }
                json!({"colors": out})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
