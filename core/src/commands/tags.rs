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

/// A cut style a command gave, checked: the fill, the solid colour and the outline weight (None inside: clear it).
struct CutChange {
    fill: Option<crate::drafting::CutFill>,
    color: Option<Option<String>>,
    outline: Option<Option<f64>>,
    priority: Option<i32>,
}

fn check_cut(c: super::command::CutArgs) -> Result<CutChange> {
    use crate::drafting::CutFill;
    let fill = match c.fill.map(|f| f.trim().to_ascii_lowercase()) {
        None => None,
        Some(f) => Some(match f.as_str() {
            "auto" | "default" | "" => CutFill::Auto,
            "solid" => CutFill::Solid,
            "hatch" => CutFill::Hatch,
            "none" | "empty" => CutFill::None,
            o => bail!("a cut fill is \"solid\", \"hatch\", \"none\" or \"auto\" (the drawing's own), not {o:?}"),
        }),
    };
    let color = match c.color.map(|c| c.trim().to_string()) {
        None => None,
        Some(c) if c.is_empty() => Some(None),
        Some(c) if crate::draft::ir::parse_hex(&c).is_some() => Some(Some(c.to_ascii_lowercase())),
        Some(c) => bail!("a cut fill colour is \"#rrggbb\", not {c:?}"),
    };
    let (lo, hi) = crate::drafting::WEIGHT_RANGE;
    let outline = match c.outline_mm {
        None => None,
        Some(w) if w == 0.0 => Some(None),
        Some(w) if w.is_finite() && (lo..=hi).contains(&w) => Some(Some(w)),
        Some(w) => bail!("a cut outline weight is between {lo} and {hi} mm on the paper (or 0 for the default), not {w}"),
    };
    let (plo, phi) = crate::drafting::PRIORITY_RANGE;
    let priority = match c.priority {
        None => None,
        Some(p) if (plo as i64..=phi as i64).contains(&p) => Some(p as i32),
        Some(p) => bail!("an intersection priority is a whole number between {plo} and {phi}, not {p}"),
    };
    Ok(CutChange { fill, color, outline, priority })
}

/// The scale range a hatch accepts (paper or model mm per pattern mm).
pub const HATCH_SCALE: (f64, f64) = (0.01, 1000.0);

fn units_of(u: Option<&str>) -> Result<crate::draft::pat::Units> {
    use crate::draft::pat::Units;
    Ok(match u.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("mm") => Units::Mm,
        Some("inch") | Some("in") | Some("inches") => Units::Inch,
        Some(o) => bail!("units are \"mm\" or \"inch\", not {o:?}"),
    })
}

/// A pattern as the app and agents see it: name, description, whether it's built in, and its line families (mm).
fn pattern_json(p: &crate::draft::pat::Pattern, builtin: bool) -> Value {
    json!({
        "name": p.name,
        "desc": p.desc,
        "builtin": builtin,
        "families": p.families.iter().map(|f| json!({"angle": f.angle, "origin": f.origin, "delta": f.delta, "dash": f.dash})).collect::<Vec<_>>(),
    })
}

/// A tag's cut hatch with `a` applied over `old` (or the defaults); None: `a` removes it.
fn apply_hatch(d: &crate::drafting::Drafting, old: Option<&crate::drafting::HatchUse>, a: super::command::HatchArgs) -> Result<Option<crate::drafting::HatchUse>> {
    use crate::drafting::{HatchUse, ScaleMode};
    let mut h = old.cloned().unwrap_or_default();
    if let Some(p) = a.pattern.map(|p| p.trim().to_string()) {
        if p.is_empty() {
            return Ok(None);
        }
        let Some(found) = crate::draft::pat::lookup(d, &p) else {
            let mut names: Vec<String> = crate::draft::pat::builtins().iter().map(|p| p.name.clone()).collect();
            names.extend(d.patterns.keys().cloned());
            bail!("no hatch pattern {p:?}; one of {}, or load one with hatch_pattern", names.join(", "));
        };
        h.pattern = found.name;
    } else if old.is_none() {
        // a new hatch needs to say which pattern
        bail!("say which pattern the hatch uses (pattern: \"diagonal\", ...)");
    }
    if let Some(c) = a.color.map(|c| c.trim().to_string()) {
        if c.is_empty() {
            h.color = HatchUse::default().color;
        } else if crate::draft::ir::parse_hex(&c).is_some() {
            h.color = c.to_ascii_lowercase();
        } else {
            bail!("a hatch colour is \"#rrggbb\", not {c:?}");
        }
    }
    if let Some(w) = a.weight_mm {
        let (lo, hi) = crate::drafting::WEIGHT_RANGE;
        if w == 0.0 {
            h.weight_mm = HatchUse::default().weight_mm;
        } else if w.is_finite() && (lo..=hi).contains(&w) {
            h.weight_mm = w;
        } else {
            bail!("a hatch line weight is between {lo} and {hi} mm on the paper (or 0 for the default), not {w}");
        }
    }
    if let Some(s) = a.scale {
        let (lo, hi) = HATCH_SCALE;
        if !(s.is_finite() && (lo..=hi).contains(&s)) {
            bail!("a hatch scale is between {lo} and {hi}, not {s}");
        }
        h.scale = s;
    }
    if let Some(an) = a.angle {
        if !an.is_finite() || an.abs() > 1.0e4 {
            bail!("a hatch angle is in degrees, not {an}");
        }
        h.angle = an.rem_euclid(360.0);
    }
    if let Some(m) = a.scale_mode.map(|m| m.trim().to_ascii_lowercase()) {
        h.scale_mode = match m.as_str() {
            "paper" => ScaleMode::Paper,
            "model" => ScaleMode::Model,
            o => bail!("a hatch's scale_mode is \"paper\" (pattern sizes are paper mm) or \"model\" (model mm), not {o:?}"),
        };
    }
    Ok(Some(h))
}

impl Engine {
    /// `hatch_pattern` (keep a `.pat` pattern in the model) and `hatch_patterns` (list them; read-only).
    fn run_hatch_patterns(&mut self, cmd: Command) -> Result<Value> {
        use crate::draft::pat;
        Ok(match cmd {
            Command::HatchPatterns { text, units } => match text {
                Some(t) => {
                    let parsed = pat::parse(&t, units_of(units.as_deref())?);
                    json!({"patterns": parsed.patterns.iter().map(|p| pattern_json(p, false)).collect::<Vec<_>>(), "warnings": parsed.warnings})
                }
                None => {
                    let d = &self.model.drafting;
                    let mut all: Vec<Value> = pat::builtins().iter().map(|p| pattern_json(p, true)).collect();
                    for name in d.patterns.keys() {
                        if let Some(p) = pat::lookup(d, name) {
                            all.push(pattern_json(&p, false));
                        }
                    }
                    json!({"patterns": all})
                }
            },
            Command::HatchPattern { name, text, units, delete } => {
                let name = name.trim().to_string();
                let d = &mut self.model.drafting;
                if delete {
                    let key = d.patterns.keys().find(|k| k.eq_ignore_ascii_case(&name)).cloned();
                    let Some(key) = key else { bail!("the model has no hatch pattern {name:?}") };
                    let users: Vec<String> = d.tag_styles.iter().filter_map(|(t, s)| match s {
                        crate::drafting::Keep::Known(s) if s.cut.hatch.iter().chain(s.surface_hatch.iter()).any(|h| h.pattern.eq_ignore_ascii_case(&key)) => Some(t.clone()),
                        _ => None,
                    }).collect();
                    if !users.is_empty() {
                        bail!("hatch pattern {key:?} is used by tag {}: give {} another hatch first", users.join(", "), if users.len() == 1 { "it" } else { "them" });
                    }
                    d.patterns.remove(&key);
                    return Ok(json!({"deleted": key}));
                }
                if !pat::valid_name(&name) {
                    bail!("a hatch pattern name is 1 to {} letters, digits, spaces, _ - or ., not {name:?}", pat::MAX_NAME);
                }
                if pat::is_builtin(&name) {
                    bail!("{name:?} is a built-in pattern; give this one another name");
                }
                if text.len() > pat::MAX_TEXT {
                    bail!("that .pat text is larger than {} KB", pat::MAX_TEXT / 1024);
                }
                let parsed = pat::parse(&text, units_of(units.as_deref())?);
                let p = match parsed.patterns.iter().find(|p| p.name.eq_ignore_ascii_case(&name)) {
                    Some(p) => p,
                    None if parsed.patterns.len() == 1 => &parsed.patterns[0],
                    None if parsed.patterns.is_empty() => bail!("no hatch pattern could be read from that text{}", if parsed.warnings.is_empty() { String::new() } else { format!(": {}", parsed.warnings.join("; ")) }),
                    None => bail!("that text has {} patterns ({}): name the one to keep", parsed.patterns.len(), parsed.patterns.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")),
                };
                if d.patterns.len() >= 200 && !d.patterns.contains_key(&name) {
                    bail!("the model keeps at most 200 hatch patterns");
                }
                // a renamed copy under any other case of the same name goes
                if let Some(k) = d.patterns.keys().find(|k| k.eq_ignore_ascii_case(&name) && **k != name).cloned() {
                    d.patterns.remove(&k);
                }
                let families = p.families.len();
                d.patterns.insert(name.clone(), pat::to_text(p, &name));
                json!({"pattern": name, "families": families, "warnings": parsed.warnings})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }

    /// `tag_style` and `edge_style`: the drafting line weight and style of a tag, and of single edges.
    fn run_line_styles(&mut self, cmd: Command) -> Result<Value> {
        use crate::drafting::{CutFill, EdgeDraft, Keep, TagStyle};
        Ok(match cmd {
            Command::TagStyle { tag, weight_mm, line, color, hatch, cut, clear } => {
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
                // the hatch is checked before anything changes
                let old_hatch = d.cut_hatch(Some(&tag)).cloned();
                let new_hatch = match hatch {
                    Some(h) => Some(apply_hatch(d, old_hatch.as_ref(), h)?),
                    None => None,
                };
                let had_hatch = d.tag_style(&tag).is_some_and(|t| t.cut.hatch.is_some());
                let new_cut = match cut {
                    Some(c) => Some(check_cut(c)?),
                    None => None,
                };
                // a hatch fill needs a hatch: one set before, or in this command
                let hatch_after = match &new_hatch {
                    Some(h) => h.is_some(),
                    None => had_hatch,
                };
                if new_cut.as_ref().is_some_and(|c| c.fill == Some(CutFill::Hatch)) && !hatch_after {
                    bail!("tag {tag:?} has no hatch to fill its cuts with: give one (hatch: {{pattern: \"diagonal\"}}) or pick solid or none");
                }
                if matches!(d.tag_styles.get(&tag), Some(Keep::Raw(_))) && (new_hatch.is_some() || new_cut.is_some()) {
                    bail!("the style of tag {tag:?} was made by a newer SketchCraft; clear it first");
                }
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
                match new_hatch {
                    Some(Some(h)) => {
                        ts.cut.fill = CutFill::Hatch;
                        ts.cut.hatch = Some(h);
                    }
                    Some(None) => {
                        if ts.cut.fill == CutFill::Hatch {
                            ts.cut.fill = CutFill::Auto;
                        }
                        ts.cut.hatch = None;
                    }
                    None => {}
                }
                if let Some(c) = new_cut {
                    if let Some(f) = c.fill {
                        ts.cut.fill = f;
                    }
                    if let Some(col) = c.color {
                        ts.cut.color = col.unwrap_or_default();
                    }
                    if let Some(w) = c.outline {
                        ts.cut.outline_mm = w.unwrap_or(0.0);
                    }
                    if let Some(pr) = c.priority {
                        ts.cut.priority = pr;
                    }
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
            Command::HatchPattern { .. } | Command::HatchPatterns { .. } => return self.run_hatch_patterns(cmd),
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
