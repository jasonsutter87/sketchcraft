//! Tags, visibility, locks, names and attributes.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

/// A weight, line style and colour a command gave, checked. Empty or zero means "clear that one".
struct StyleArgs {
    weight: Option<Option<f64>>,
    line: Option<Option<String>>,
    color: Option<Option<String>>,
}

fn check_style(d: &crate::drafting::Drafting, weight_mm: Option<f64>, line: Option<String>, color: Option<String>) -> Result<StyleArgs> {
    let (lo, hi) = crate::drafting::WEIGHT_RANGE;
    let weight = match weight_mm {
        None => None,
        Some(w) if w == 0.0 => Some(None),
        Some(w) if w.is_finite() && (lo..=hi).contains(&w) => Some(Some(w)),
        Some(w) => bail!("a line weight is between {lo} and {hi} mm on the paper (or 0 for the default), not {w}"),
    };
    let line = match line.map(|l| l.trim().to_string()) {
        None => None,
        Some(l) if l.is_empty() => Some(None),
        Some(l) if d.dash_of(&l).is_some() => Some(Some(l)),
        Some(l) => bail!("no line style {l:?}; one of {}, or a line type in the file", crate::drafting::LINE_STYLES.join(", ")),
    };
    let color = match color.map(|c| c.trim().to_string()) {
        None => None,
        Some(c) if c.is_empty() => Some(None),
        Some(c) if crate::draft::ir::parse_hex(&c).is_some() => Some(Some(c.to_ascii_lowercase())),
        Some(c) => bail!("a line colour is \"#rrggbb\", not {c:?}"),
    };
    Ok(StyleArgs { weight, line, color })
}

impl Engine {
    /// `tag_style` and `edge_style`: the drafting line weight and style of a tag, and of single edges.
    fn run_line_styles(&mut self, cmd: Command) -> Result<Value> {
        use crate::drafting::{EdgeDraft, Keep, TagStyle};
        Ok(match cmd {
            Command::TagStyle { tag, weight_mm, line, color, clear } => {
                let tag = tag.trim().to_string();
                if tag.is_empty() {
                    bail!("a tag needs a name");
                }
                let d = &mut self.model.drafting;
                if clear {
                    d.tag_styles.remove(&tag);
                    return Ok(json!({"tag": tag, "cleared": true}));
                }
                let a = check_style(d, weight_mm, line, color)?;
                let entry = d.tag_styles.entry(tag.clone()).or_insert_with(|| Keep::Known(TagStyle::default()));
                let Keep::Known(ts) = entry else { bail!("the style of tag {tag:?} was made by a newer SketchCraft; clear it first") };
                if let Some(w) = a.weight {
                    ts.weight_mm = w.unwrap_or(0.0);
                }
                if let Some(l) = a.line {
                    ts.line = l.unwrap_or_default();
                }
                if let Some(c) = a.color {
                    ts.color = c.unwrap_or_default();
                }
                if *ts == TagStyle::default() {
                    d.tag_styles.remove(&tag);
                }
                json!({"tag": tag, "style": d.tag_styles.get(&tag)})
            }
            Command::EdgeStyle { ids, weight_mm, line, color, clear } => {
                let a = check_style(&self.model.drafting, weight_mm, line, color)?;
                let m: &mut Model = active_model!(self);
                // edges named, or the edges around faces named
                let mut edges: Vec<Id> = Vec::new();
                for id in &ids {
                    if m.edges.contains_key(id) {
                        edges.push(*id);
                    } else if let Some(f) = m.faces.get(id).cloned() {
                        for l in f.loops() {
                            for (p, q) in crate::model::pairs(l) {
                                edges.extend(m.find_edge(p, q));
                            }
                        }
                    } else {
                        bail!("{id} is not an edge or face");
                    }
                }
                for id in &edges {
                    let e = m.edges.get_mut(id).unwrap();
                    if clear {
                        e.draft = None;
                        continue;
                    }
                    let mut dr: EdgeDraft = e.draft.take().map(|b| *b).unwrap_or_default();
                    if let Some(w) = a.weight {
                        dr.weight_mm = w;
                    }
                    if let Some(l) = &a.line {
                        dr.line = l.clone();
                    }
                    if let Some(c) = &a.color {
                        dr.color = c.clone();
                    }
                    e.draft = (!dr.is_empty()).then(|| Box::new(dr));
                }
                json!({"edges": edges.len()})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }

    pub(super) fn run_tags(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Hide { ids } => {
                m.set_hidden(&ids, true)?;
                json!({})
            }
            Command::Unhide { ids } => {
                if ids.is_empty() { m.unhide_all() } else { m.set_hidden(&ids, false)? }
                json!({})
            }
            Command::Tag { ids, tag } => {
                m.set_tag(&ids, tag)?;
                json!({})
            }
            Command::TagVisible { tag, visible } => {
                m.tags.insert(tag, visible);
                json!({})
            }
            Command::TagColor { tag, color } => {
                match color {
                    Some(c) => m.tag_colors.insert(tag, c),
                    None => m.tag_colors.remove(&tag),
                };
                json!({})
            }
            Command::RenameTag { tag, to } => {
                let to = to.trim().to_string();
                if to.is_empty() {
                    bail!("a tag needs a name");
                }
                if let Some(v) = m.tags.remove(&tag) {
                    m.tags.insert(to.clone(), v);
                }
                if let Some(c) = m.tag_colors.remove(&tag) {
                    m.tag_colors.insert(to.clone(), c);
                }
                if let Some(s) = m.drafting.tag_styles.remove(&tag) {
                    m.drafting.tag_styles.insert(to.clone(), s);
                }
                for g in m.groups.values_mut().filter(|g| g.tag.as_deref() == Some(tag.as_str())) {
                    g.tag = Some(to.clone());
                }
                for i in m.instances.values_mut().filter(|i| i.tag.as_deref() == Some(tag.as_str())) {
                    i.tag = Some(to.clone());
                }
                json!({"tag": to})
            }
            Command::TagStyle { .. } | Command::EdgeStyle { .. } => return self.run_line_styles(cmd),
            Command::SetParent { ids, parent } => {
                m.set_parent(&ids, parent)?;
                json!({})
            }
            Command::Lock { ids, locked } => {
                for id in &ids {
                    let Some(g) = m.groups.get_mut(id) else { bail!("{id} is not a group") };
                    g.locked = locked;
                }
                json!({})
            }
            Command::Soften { ids, soft, coplanar } => {
                let n = if coplanar { m.soften_coplanar() } else { m.soften(&ids, soft)? };
                json!({"edges": n})
            }
            Command::Rename { id, name } => {
                if let Some(g) = m.groups.get_mut(&id) {
                    g.name = name;
                } else if let Some(i) = m.instances.get_mut(&id) {
                    i.name = name;
                } else if let Some(d) = m.definitions.get_mut(&id) {
                    d.name = name;
                } else {
                    bail!("{id} can't be renamed (groups and components only)");
                }
                json!({})
            }
            Command::Opacity { material, value } => {
                let Some(mat) = m.materials.get_mut(&material) else { bail!("no material {material}") };
                mat.opacity = value.clamp(0.05, 1.0);
                json!({"opacity": mat.opacity})
            }
            Command::SetAttrs { id, attrs } => {
                let target = if let Some(i) = m.instances.get_mut(&id) {
                    &mut i.attrs
                } else if let Some(d) = m.definitions.get_mut(&id) {
                    &mut d.attrs
                } else {
                    bail!("{id} is not a component placement or a component");
                };
                for (k, v) in attrs {
                    let k = k.trim().to_ascii_lowercase();
                    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        bail!("attribute names are letters, digits and _ ({k:?})");
                    }
                    if v.trim().is_empty() {
                        target.remove(&k);
                    } else {
                        target.insert(k, v.trim().to_string());
                    }
                }
                // check every formula still works out
                let (inst, def) = match m.instances.get(&id) {
                    Some(i) => (Some(i.clone()), m.definitions.get(&i.def)),
                    None => (None, m.definitions.get(&id)),
                };
                let all = match &inst {
                    Some(i) => crate::dynamic::attrs_of(i, def),
                    None => def.map(|d| d.attrs.clone()).unwrap_or_default(),
                };
                let vals = crate::dynamic::resolve(&all, &Default::default());
                for (k, v) in &all {
                    if v.starts_with('=') && !vals.contains_key(k) && !v.contains("parent!") {
                        let err = crate::dynamic::eval(v, &|n| vals.get(n).copied().ok_or_else(|| anyhow::anyhow!("no attribute {n}"))).err();
                        bail!("{k}: {}", err.map(|e| e.to_string()).unwrap_or_else(|| "can't be worked out".into()));
                    }
                }
                json!({"attrs": all, "values": vals})
            }
            Command::Classify { ids, class } => {
                let class = class.trim().to_string();
                if !class.is_empty() && !crate::ifc::CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&class)) {
                    bail!("unknown IFC type {class:?}; one of {}", crate::ifc::CLASSES.join(", "));
                }
                let class = crate::ifc::CLASSES.iter().find(|c| c.eq_ignore_ascii_case(&class)).map(|c| c.to_string());
                for id in &ids {
                    if !m.groups.contains_key(id) && !m.instances.contains_key(id) {
                        bail!("{id} is not a group or component");
                    }
                    match &class {
                        Some(c) => m.classes.insert(*id, c.clone()),
                        None => m.classes.remove(id),
                    };
                }
                json!({"classified": ids.len()})
            }
            Command::Reveal { path } => {
                let p = crate::safety::path_arg(path.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir));
                std::fs::create_dir_all(if p.extension().is_some() { p.parent().unwrap_or(&p) } else { &p }).ok();
                // folders open; files and app bundles are only shown, never launched
                #[cfg(target_os = "macos")]
                let r = {
                    let bundle = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"));
                    if p.is_dir() && !bundle { std::process::Command::new("open").arg(&p).status() } else { std::process::Command::new("open").arg("-R").arg(&p).status() }
                };
                // never `explorer <file>`: that opens (runs) the file. Folders open; a file is only selected.
                #[cfg(target_os = "windows")]
                let r = if p.is_dir() { std::process::Command::new("explorer").arg(&p).status() } else { std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).status() };
                #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                let r = std::process::Command::new("xdg-open").arg(if p.is_dir() { &p } else { p.parent().unwrap_or(&p) }).status();
                anyhow::Context::context(r, "couldn't open the file browser")?;
                json!({"path": p.to_string_lossy()})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
