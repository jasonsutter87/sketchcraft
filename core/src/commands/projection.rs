//! Projection mapping commands (`texture_project`, `texture_uv`). The math lives in `crate::projection`.

use super::*;
use crate::projection::{Mode, Projection};

impl Engine {
    pub(super) fn run_projection(&mut self, cmd: Command) -> Result<Value> {
        // the root model knows every material and picture size, even when a component is open for editing
        let tiles = self.model.texture_tiles();
        let root_has = |name: &str| self.model.find_material(name).is_some();
        let known = match &cmd {
            Command::TextureProject { material: Some(name), .. } => root_has(name),
            _ => true,
        };
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::TextureProject { faces, entity, mode, material, origin, axis, scale, rotation, fit } => {
                let mut ids = faces.unwrap_or_default();
                ids.extend(entity);
                if ids.is_empty() {
                    bail!("texture_project needs faces or an entity (a group or component)");
                }
                // a placed component is projected in its own frame, so the picture stays put when it moves
                let (placed, direct): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| m.instances.contains_key(id));
                if mode == "clear" {
                    let mut n = if direct.is_empty() { 0 } else { m.clear_texture_projection(&direct)? };
                    for i in placed {
                        m.check_unlocked(&[i])?;
                        let def = m.instances[&i].def;
                        let dm = std::sync::Arc::make_mut(&mut m.definitions.get_mut(&def).unwrap().model);
                        let all: Vec<Id> = dm.faces.keys().copied().collect();
                        n += dm.clear_texture_projection(&all)?;
                    }
                    return Ok(json!({"cleared": n}));
                }
                let mut p = Projection::new(Mode::parse(&mode)?);
                p.origin = origin.map(v3);
                p.axis = axis.map(v3);
                p.scale = scale.unwrap_or(1.0);
                p.rotation = rotation.unwrap_or(0.0);
                p.fit = fit;
                if !known {
                    bail!("no material {}", material.as_deref().unwrap_or_default());
                }
                let mat = material.as_deref();
                let mut n = 0;
                if !direct.is_empty() {
                    n += m.project_texture_with(&direct, &p, mat, &tiles)?;
                }
                for i in placed {
                    m.check_unlocked(&[i])?;
                    let def = m.instances[&i].def;
                    let dm = std::sync::Arc::make_mut(&mut m.definitions.get_mut(&def).unwrap().model);
                    let all: Vec<Id> = dm.faces.keys().copied().collect();
                    n += dm.project_texture_with(&all, &p, mat, &tiles)?;
                }
                json!({"faces": n, "mode": mode, "material": material})
            }
            Command::TextureUv { faces } => {
                let mut out = serde_json::Map::new();
                for id in faces {
                    // a face of the open model, or of a component placed in it
                    let f = match m.faces.get(&id) {
                        Some(f) => f,
                        None => m.definitions.values().find_map(|d| d.model.faces.get(&id)).ok_or_else(|| anyhow::anyhow!("no face {id}"))?,
                    };
                    out.insert(id.to_string(), serde_json::to_value(f.uv.as_ref().filter(|u| u.fits(f)).map(|u| &u.outer))?);
                }
                json!({"uv": out})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
