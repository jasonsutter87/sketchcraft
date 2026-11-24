//! The component library and plugins.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_library(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Plugins => crate::plugins::list(),
            Command::PluginEnable { id, on } => crate::plugins::enable(&id, on)?,
            Command::InstallPlugin { path, data, name, preview, replace } => {
                let name = name.or_else(|| path.clone());
                let bytes = match (path, data) {
                    (Some(p), _) => std::fs::read(&p)?,
                    (None, Some(d)) => {
                        use base64::Engine as _;
                        base64::engine::general_purpose::STANDARD.decode(d.trim().trim_start_matches("data:application/zip;base64,"))?
                    }
                    _ => bail!("install_plugin needs a path or data"),
                };
                crate::plugins::install(&bytes, name.as_deref(), &crate::plugins::InstallOpts { preview, replace })?
            }
            Command::RemovePlugin { id } => crate::plugins::remove(&id)?,
            Command::SaveText { path, text } => {
                crate::safety::write_atomic(&path, text.as_bytes())?;
                json!({"path": path})
            }
            Command::Library { dir } => {
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                std::fs::create_dir_all(&d).ok();
                json!({"dir": d.to_string_lossy(), "entries": crate::library::list(&d)})
            }
            Command::InsertFile { path, at, rotate } => {
                let (def, repair) = crate::library::definition_for_reporting(m, &path)?;
                let mut v = json!({"instance": m.place(def, v3(at), rotate, None)?, "component": def});
                repair.add_to(&mut v);
                v
            }
            Command::SaveToLibrary { component, name, collection, dir } => {
                let model = crate::library::standalone(m, component)?;
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                let folder = match collection.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    Some(c) => {
                        let c = crate::library::safe(c);
                        if crate::library::reserved(&c) {
                            bail!("{c:?} is a reserved folder name (Backups and Plugins are not library collections)");
                        }
                        d.join(c)
                    }
                    None => d.clone(),
                };
                std::fs::create_dir_all(&folder)?;
                if crate::library::is_protected(&folder, &d) {
                    bail!("that folder is the model's Backups or Plugins folder; the library never writes there");
                }
                let base = crate::library::safe(&name.unwrap_or_else(|| m.definitions[&component].name.clone()));
                let bytes = crate::format::to_string(&model)?;
                // a new file every time: never over an existing one (create_new also settles a race)
                let mut k = 1;
                let path = loop {
                    let path = folder.join(if k == 1 { format!("{base}.sketchcraft") } else { format!("{base} {k}.sketchcraft") });
                    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                        Ok(mut f) => {
                            use std::io::Write;
                            f.write_all(bytes.as_bytes())?;
                            break path;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && k < 10_000 => k += 1,
                        Err(e) => return Err(e.into()),
                    }
                };
                json!({"path": path.to_string_lossy()})
            }
            Command::LibraryAdd { paths, collection, dir } => {
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                json!({"added": crate::library::add_files(&d, &paths, collection.as_deref().unwrap_or(""), true)?})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
