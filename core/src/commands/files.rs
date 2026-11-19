//! Files in and out: save and open, the exports and the importers.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_files(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Save { path } => {
                // never write a model with NaN or absurd numbers: it couldn't be opened again
                self.model.check_sane()?;
                crate::safety::write_atomic(&path, crate::format::to_string(&self.model)?.as_bytes())?;
                json!({"path": path})
            }
            Command::Open { path } => {
                // the whole file is read into memory (a model takes several times its file size once opened): look at the size first
                let len = std::fs::metadata(&path)?.len();
                if len > MAX_OPEN_BYTES {
                    bail!("{path} is {} MB; SketchCraft opens models up to {} MB", len >> 20, MAX_OPEN_BYTES >> 20);
                }
                // an agent opens what an agent may hold (its own model cap, with room for the file's escapes), not the person's whole 768 MB
                if self.untrusted && len > MAX_OPEN_BYTES_UNTRUSTED {
                    bail!("{path} is {} MB; an agent or a plugin can open models up to {} MB (the person can open larger ones)", len >> 20, MAX_OPEN_BYTES_UNTRUSTED >> 20);
                }
                let text = std::fs::read_to_string(&path)?;
                // the file's top level is looked at before the model is read: a newer file opens read-only (see `format`)
                let read_only = crate::format::check(text.as_bytes());
                let mut opened: Model = serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{path} isn't a model SketchCraft can read: {e}"))?;
                drop(text);
                if let Some(what) = opened.broken_refs(false).first() {
                    bail!("{path} isn't a consistent model ({what} refers to something that isn't there)");
                }
                // components that contain themselves, nest absurdly deep or multiply into millions of faces would take the program down when drawn
                if let Err(why) = opened.check_nesting(if self.untrusted { crate::nesting::MAX_EXPANDED_UNTRUSTED } else { crate::nesting::MAX_EXPANDED }) {
                    bail!("{path} can't be opened: {why}. Nothing was changed.");
                }
                // ids from outside: the counter must be above every id in the file, or the next drawing overwrites something
                opened.repair_ids();
                opened.vet_pictures();
                let sheets_clamped = opened.vet_sheets();
                if self.untrusted && opened.size_estimate() > crate::model::MAX_MODEL_BYTES_UNTRUSTED {
                    bail!("{path} is a model of about {} MB; an agent or a plugin can open models up to {} MB (the person can open larger ones)", opened.size_estimate() >> 20, crate::model::MAX_MODEL_BYTES_UNTRUSTED >> 20);
                }
                // too much geometry or too many bytes is not a reason to refuse the file: it opens for viewing and cutting down (see `oversize`)
                opened.check_sane_with(false)?;
                self.oversize = opened.over_limits();
                self.model = opened;
                self.read_only = read_only;
                // which group was open for editing is a matter of the program that saved it, not of the model: a program
                // that opens the file must not draw into someone else's open group (and tie its work to that group's undo)
                self.model.active_group = None;
                self.batch = None;
                self.ctx = None;
                self.names.clear();
                let mut v = json!({"path": path, "faces": self.model.faces.len(), "read_only": self.read_only.as_ref().map(|r| r.reasons.clone())});
                if !sheets_clamped.is_empty() {
                    v["sheets_clamped"] = json!(sheets_clamped);
                }
                v
            }
            Command::ExportGlb { path } => json!({"path": path, "triangles": export::write_glb(&self.model, &path)?}),
            Command::ExportStl { path, scale } => json!({"path": path, "triangles": export::write_stl(&self.model, &path, scale)?}),
            Command::ImportDxf { path, text, units, at, name } => {
                let text = match (text, &path) {
                    (Some(t), _) => t,
                    (None, Some(p)) => String::from_utf8_lossy(&std::fs::read(p)?).into_owned(),
                    _ => bail!("import_dxf needs a path or text"),
                };
                let label = name.or_else(|| path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| tidy_name(&s.to_string_lossy()))).unwrap_or_else(|| "DXF drawing".into());
                let r = crate::dxf::import(m, &text, &label, units.as_deref(), v3(at))?;
                json!({"group": r.group, "layers": r.layers, "faces": r.faces, "edges": r.edges, "units": r.units, "skipped": r.skipped, "labels": r.labels, "dimensions": r.dimensions, "focus": [r.focus[0].to_array(), r.focus[1].to_array()]})
            }
            Command::ImportDwg { path, units, at } => {
                let scratch = crate::safety::TempDir::new("sketchcraft-dwg")?;
                let tmp = scratch.path().to_path_buf();
                let dxf = crate::dwg::to_dxf(std::path::Path::new(&path), &tmp);
                let r = dxf.and_then(|d| {
                    let text = String::from_utf8_lossy(&std::fs::read(&d)?).into_owned();
                    let label = std::path::Path::new(&path).file_stem().map(|s| tidy_name(&s.to_string_lossy())).unwrap_or_else(|| "DWG drawing".into());
                    crate::dxf::import(m, &text, &label, units.as_deref(), v3(at))
                });
                let r = r?;
                json!({"group": r.group, "layers": r.layers, "faces": r.faces, "edges": r.edges, "units": r.units, "skipped": r.skipped, "labels": r.labels, "dimensions": r.dimensions, "focus": [r.focus[0].to_array(), r.focus[1].to_array()]})
            }
            Command::ExportDwg { path } => {
                let scratch = crate::safety::TempDir::new("sketchcraft-dwgout")?;
                let tmp = scratch.path().to_path_buf();
                let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
                let dxf = tmp.join(format!("{stem}.dxf"));
                let r = crate::export2::write_dxf(&self.model, dxf.to_str().unwrap()).and_then(|n| crate::dwg::from_dxf(&dxf, std::path::Path::new(&path)).map(|_| n));
                json!({"path": path, "triangles": r?})
            }
            Command::ExportIfc { path, project } => {
                let project = project.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into()));
                json!({"path": path, "elements": crate::ifc::write_ifc(&self.model, &path, &project)?})
            }
            Command::ExportDae { path } => json!({"path": path, "triangles": crate::export2::write_dae(&self.model, &path)?}),
            Command::ExportDxf { path } => json!({"path": path, "triangles": crate::export2::write_dxf(&self.model, &path)?}),
            Command::ExportPdf { path, title, opts } => {
                let img = render::render(&self.model, &opts);
                let t = title.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                crate::export2::write_pdf(&img, &format!("{t}  -  SketchCraft"), &path)?;
                json!({"path": path})
            }
            Command::ImportObj { path, text, up, scale, at, name } => {
                let text = match (text, &path) {
                    (Some(t), _) => t,
                    (None, Some(p)) => std::fs::read_to_string(p)?,
                    _ => bail!("import_obj needs a path or text"),
                };
                let p = path.as_deref().map(std::path::Path::new);
                let label = name.or_else(|| p.and_then(|p| p.file_stem()).map(|s| tidy_name(&s.to_string_lossy()))).unwrap_or_else(|| "Imported".into());
                let r = crate::import::import_obj(m, &text, p.and_then(|p| p.parent()), &label, up.eq_ignore_ascii_case("y"), scale, v3(at))?;
                json!({"instance": r.instance, "component": r.component, "groups": r.groups, "faces": r.faces, "skipped": r.skipped})
            }
            Command::ImportSkp { path, data, at, name } => {
                // its components would land in the open component's own table, under ids that can match the model's: close it first
                if self.ctx.is_some() {
                    bail!("close the component you're editing first: a SketchUp file's components can't be imported inside one");
                }
                use base64::Engine as _;
                let bytes = match (data, &path) {
                    (Some(b64), _) => base64::engine::general_purpose::STANDARD.decode(b64.trim())?,
                    (None, Some(p)) => {
                        // the file is read whole and a model takes several times its size once imported
                        let len = std::fs::metadata(p)?.len();
                        if len > MAX_OPEN_BYTES {
                            bail!("{p} is {} MB; SketchCraft imports SketchUp files up to {} MB", len >> 20, MAX_OPEN_BYTES >> 20);
                        }
                        if self.untrusted && len > MAX_OPEN_BYTES_UNTRUSTED {
                            bail!("{p} is {} MB; an agent or a plugin can import SketchUp files up to {} MB (the person can import larger ones)", len >> 20, MAX_OPEN_BYTES_UNTRUSTED >> 20);
                        }
                        std::fs::read(p)?
                    }
                    _ => bail!("import_skp needs a path or data"),
                };
                let label = name.unwrap_or_else(|| {
                    path.as_deref()
                        .and_then(|p| std::path::Path::new(p).file_stem())
                        .map(|s| crate::library::nice_name(&s.to_string_lossy()))
                        .unwrap_or_else(|| "SketchUp model".into())
                });
                let r = crate::skp::import(m, &bytes, v3(at), &label)?;
                json!({"group": r.group, "version": r.version, "faces": r.faces, "components": r.components, "placed": r.placed, "dimensions": r.dimensions, "labels": r.labels, "skipped_annotations": r.skipped_annotations, "dimensions_attached": r.dimensions_attached, "dimensions_unattached": r.dimensions_unattached, "screen_texts": r.screen_texts, "skipped_labels": crate::policy::untrusted_text(&r.skipped_labels)})
            }
            Command::ImportCloud { path, units, max_points } => {
                let scale = match units.as_deref().unwrap_or("m") {
                    "m" => 1.0,
                    "cm" => 0.01,
                    "mm" => 0.001,
                    "ft" => 0.3048,
                    "in" => 0.0254,
                    u => bail!("units must be m, cm, mm, ft or in (not {u})"),
                };
                let bytes = anyhow::Context::with_context(std::fs::read(&path), || format!("can't read {path}"))?;
                let (c, shift) = crate::cloud::read(&path, &bytes, scale, max_points.clamp(1, 50_000_000))?;
                let id = m.new_id();
                let (count, colored) = (c.len(), !c.colors.is_empty());
                m.clouds.insert(id, c);
                json!({"cloud": id, "points": count, "colored": colored, "shift": shift.to_array()})
            }
            Command::CloudData { id, max } => {
                let c = m.clouds.get(&id).ok_or_else(|| anyhow::anyhow!("no cloud {id}"))?;
                let (p, col) = c.thinned(max.clamp(1, 20_000_000));
                use base64::Engine as _;
                let b = base64::engine::general_purpose::STANDARD;
                json!({"points": b.encode(crate::cloud::f32_bytes(&p)), "colors": b.encode(&col), "count": p.len() / 3, "version": c.version})
            }
            Command::Cloud { id, name, hidden, delete } => {
                if delete {
                    m.clouds.remove(&id).ok_or_else(|| anyhow::anyhow!("no cloud {id}"))?;
                    json!({"deleted": id})
                } else {
                    let c = m.clouds.get_mut(&id).ok_or_else(|| anyhow::anyhow!("no cloud {id}"))?;
                    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                        c.name = n.trim().chars().take(80).collect();
                    }
                    if let Some(h) = hidden {
                        c.hidden = h;
                    }
                    json!({"cloud": id})
                }
            }
            Command::ImportIfc { path } => {
                let r = crate::ifcin::import(m, std::path::Path::new(&path))?;
                json!({"group": r.group, "elements": r.elements, "faces": r.faces, "storeys": r.storeys, "schema": r.schema})
            }
            Command::ImportFbx { path, data, at, name } => {
                use base64::Engine as _;
                let bytes = match (data, &path) {
                    (Some(b64), _) => base64::engine::general_purpose::STANDARD.decode(b64.trim())?,
                    (None, Some(p)) => std::fs::read(p)?,
                    _ => bail!("import_fbx needs a path or data"),
                };
                let label = name.or_else(|| path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| tidy_name(&s.to_string_lossy()))).unwrap_or_else(|| "FBX model".into());
                let r = crate::fbx::import(m, &bytes, &label, v3(at))?;
                json!({"instance": r.instance, "component": r.component, "meshes": r.meshes, "faces": r.faces})
            }
            Command::ExportObj { path } => {
                let tris = export::write_obj(&self.model, &path)?;
                json!({"path": path, "triangles": tris})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
