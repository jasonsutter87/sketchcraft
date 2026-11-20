//! Paint, textures, materials and skins.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_paint(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Paint { ids, color, material, scope, through } => {
                let spreads = scope.as_deref().is_some_and(|s| s != "face");
                if !spreads && !through {
                    return Ok(match (material, color) {
                        (Some(name), _) => {
                            m.paint_material(&ids, &name)?;
                            json!({"material": name})
                        }
                        (None, Some(c)) => json!({"material": m.paint(&ids, c)?}),
                        _ => bail!("paint needs a color or a material"),
                    });
                }
                // the reach is worked out before anything changes (it depends on the old materials)
                let faces = m.paint_targets(&ids, scope.as_deref())?;
                // the clicked faces first: they get the material's own placement, the rest continue from them (a stable
                // split: each keeps its order)
                let clicked: std::collections::HashSet<Id> = m.faces_of(&ids)?.into_iter().collect();
                let (mut faces, rest): (Vec<Id>, Vec<Id>) = faces.into_iter().partition(|f| clicked.contains(f));
                faces.extend(rest);
                let name = match (material, color) {
                    (Some(name), _) => name,
                    (None, Some(c)) => m.color_material(c),
                    _ => bail!("paint needs a color or a material"),
                };
                let continuous = through || matches!(scope.as_deref(), Some("connected" | "object"));
                m.apply_paint(&faces, &name, continuous)?;
                json!({"material": name, "faces": faces})
            }
            Command::Texture { name, data, width, height, color, project, origin } => {
                let mat = m.materials.entry(name.clone()).or_default();
                let mut tex = mat.texture.clone().unwrap_or(crate::model::Texture { data: String::new(), width: 1.0, height: 1.0, project: None, origin: None });
                if let Some(d) = data {
                    crate::safety::check_picture(&d).map_err(|e| anyhow::anyhow!("texture {name}: {e}"))?;
                    tex.data = d;
                }
                if tex.data.is_empty() {
                    bail!("a new texture needs image data");
                }
                if let Some(w) = width {
                    tex.height = height.unwrap_or(tex.height * w / tex.width.max(1e-9));
                    tex.width = w;
                } else if let Some(h) = height {
                    tex.height = h;
                }
                if let Some(p) = project {
                    tex.project = p;
                }
                if let Some(o) = origin {
                    tex.origin = Some(o);
                }
                if let Some(c) = color {
                    mat.color = c;
                } else if mat.color == [0.0; 3] {
                    mat.color = [0.8, 0.8, 0.8];
                }
                mat.texture = Some(tex);
                json!({"material": name})
            }
            Command::Purge => json!({"removed": crate::library::purge(m)}),
            Command::DeleteMaterial { name, replace_with } => {
                let faces = m.remove_material(&name, replace_with.as_deref())?;
                json!({"material": name, "faces": faces})
            }
            Command::PurgeMaterials => {
                let (names, bytes) = m.purge_materials();
                json!({"removed": names.len(), "materials": names, "freed_mb": bytes >> 20})
            }
            Command::Skins => {
                let dir = skins_dir();
                let mut out = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("sketchskin")).collect();
                    files.sort();
                    for p in files.into_iter().take(200) {
                        // JSON files only, up to 4 MB (a skin can carry an icon set)
                        if std::fs::metadata(&p).map(|m| m.len() > 4 << 20).unwrap_or(true) {
                            continue;
                        }
                        if let Some(skin) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
                            out.push(json!({"path": p.to_string_lossy(), "name": p.file_stem().map(|s| s.to_string_lossy().to_string()), "skin": skin}));
                        }
                    }
                }
                json!({"dir": dir.to_string_lossy(), "skins": out})
            }
            Command::SaveSkin { name, skin } => {
                let dir = skins_dir();
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(format!("{}.sketchskin", crate::library::safe(&name)));
                std::fs::write(&path, serde_json::to_string_pretty(&skin)?)?;
                json!({"path": path.to_string_lossy()})
            }
            Command::Material { name, color, roughness, metalness, glass, emission } => {
                let mat = m.materials.entry(name.clone()).or_default();
                if let Some(c) = color {
                    mat.color = c;
                }
                let unit = |x: f32| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.5 };
                if let Some(r) = roughness {
                    mat.roughness = Some(unit(r));
                }
                if let Some(x) = metalness {
                    mat.metalness = Some(unit(x));
                }
                if let Some(g) = glass {
                    mat.glass = Some(g);
                }
                if let Some(e) = emission {
                    mat.emission = Some(if e.is_finite() { e.clamp(0.0, 100.0) } else { 0.0 });
                }
                json!({"material": name})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
