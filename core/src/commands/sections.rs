//! Section planes and cuts.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

/// On screen, a paper-scaled hatch is drawn as on a 1:50 drawing (the model view has no paper).
pub const SCREEN_HATCH_SCALE: f64 = 50.0;
/// Most hatch segments one `section` answer carries (the rest of the hatched regions get a tint).
const SCREEN_HATCH_SEGMENTS: usize = 60_000;

/// The on-screen hatch of a cut region whose tag has one: segments in world metres on the cut plane.
fn screen_hatch(d: &crate::drafting::Drafting, u: &crate::drafting::HatchUse, cut: &crate::section::Cut, outer: &[DVec3], holes: &[Vec<DVec3>], budget: usize) -> crate::draft::hatch::Hatched {
    use crate::draft::hatch;
    // the plane's own axes, in mm from its origin (as the cut lies on paper: plans north up)
    let flat = |p: &DVec3| {
        let q = *p - cut.origin;
        [q.dot(cut.u) * 1000.0, q.dot(cut.v) * 1000.0]
    };
    let rings: Vec<Vec<[f64; 2]>> = std::iter::once(outer).chain(holes.iter().map(|h| h.as_slice())).map(|r| r.iter().map(flat).collect()).collect();
    let Some(p) = crate::draft::pat::lookup(d, &u.pattern) else {
        return hatch::Hatched { lines: vec![], over: Some(format!("there is no hatch pattern {:?}", u.pattern)) };
    };
    // pattern mm to model mm: a paper-scaled hatch as on a 1:50 drawing, a model-scaled one as it is
    let k = match u.scale_mode {
        crate::drafting::ScaleMode::Paper => hatch::scale_k(u, 1.0) * SCREEN_HATCH_SCALE,
        crate::drafting::ScaleMode::Model => hatch::scale_k(u, 1.0),
    };
    hatch::generate(&rings, &hatch::place(&p, k, if u.angle.is_finite() { u.angle } else { 0.0 }), budget.min(hatch::MAX_SEGMENTS))
}

impl Engine {
    pub(super) fn run_sections(&mut self, cmd: Command) -> Result<Value> {
        // tag styles are the whole model's, whatever component is open
        let drafting = self.model.drafting.clone();
        let root_tags = self.ctx.is_none();
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Section { origin, normal, path } => {
                let cut = m.section(v3(origin), v3(normal))?;
                if let Some(p) = &path {
                    let text = if p.to_ascii_lowercase().ends_with(".dxf") { cut.dxf() } else { cut.svg() };
                    std::fs::write(p, text)?;
                }
                // fills as triangles, ready to draw: solid for most, paper under a tag's hatch, whose lines come
                // per tag (world metres, on the plane)
                let mut fill: Vec<f64> = Vec::new();
                let mut paper: Vec<f64> = Vec::new();
                let mut owners: Vec<Id> = Vec::new();
                let mut by_tag: std::collections::BTreeMap<String, (String, Vec<f64>, Vec<String>)> = Default::default();
                let mut budget = SCREEN_HATCH_SEGMENTS;
                for (owner, o, h) in &cut.regions {
                    let tag = if root_tags { m.owner_tag(*owner) } else { None };
                    let hatch = drafting.cut_hatch(tag.as_deref());
                    let tris = crate::geom::triangulate(o, h, cut.n);
                    match (hatch, tag) {
                        (Some(u), Some(tag)) => {
                            let got = screen_hatch(&drafting, u, &cut, o, h, budget);
                            let e = by_tag.entry(tag).or_insert_with(|| (u.color.clone(), Vec::new(), Vec::new()));
                            if let Some(why) = got.over {
                                if !e.2.contains(&why) {
                                    e.2.push(why);
                                }
                                // too dense to draw here: the solid fill, as without a hatch
                                for t in tris {
                                    fill.extend(t.iter().flat_map(|p| [p.x, p.y, p.z]));
                                    owners.push(*owner);
                                }
                                continue;
                            }
                            budget = budget.saturating_sub(got.lines.len());
                            let to = |q: [f64; 2]| cut.origin + cut.u * (q[0] / 1000.0) + cut.v * (q[1] / 1000.0);
                            for [a, b] in &got.lines {
                                let (a, b) = (to(*a), to(*b));
                                e.1.extend([a.x, a.y, a.z, b.x, b.y, b.z]);
                            }
                            for t in tris {
                                paper.extend(t.iter().flat_map(|p| [p.x, p.y, p.z]));
                            }
                        }
                        _ => {
                            for t in tris {
                                fill.extend(t.iter().flat_map(|p| [p.x, p.y, p.z]));
                                owners.push(*owner);
                            }
                        }
                    }
                }
                let hatch: Vec<Value> = by_tag.into_iter().map(|(tag, (color, lines, notes))| json!({"tag": tag, "color": color, "lines": lines, "notes": notes})).collect();
                let lines: Vec<f64> = cut.segments.iter().flat_map(|(a, b)| [a.x, a.y, a.z, b.x, b.y, b.z]).collect();
                json!({"regions": cut.regions.len(), "fill": fill, "fillOwners": owners, "hatchFill": paper, "hatch": hatch, "lines": lines, "path": path})
            }
            Command::SectionPlane { name, origin, normal } => {
                let n = DVec3::from(normal);
                if !(n.length() > 1e-9) || !crate::model::finite3(&origin) {
                    bail!("a section plane needs a point and a direction");
                }
                let id = m.new_id();
                let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Section {}", m.sections.len() + 1));
                m.sections.push(crate::model::SectionPlane { id, name, origin, normal: n.normalize().to_array() });
                m.active_section = Some(id);
                json!({"section": id})
            }
            Command::SectionEdit { id, name, origin, normal, active, delete } => {
                let i = m.sections.iter().position(|s| s.id == id).ok_or_else(|| anyhow::anyhow!("no section plane {id}"))?;
                if delete {
                    m.sections.remove(i);
                    if m.active_section == Some(id) {
                        m.active_section = None;
                    }
                    return Ok(json!({"deleted": id}));
                }
                let s = &mut m.sections[i];
                if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
                    s.name = n.trim().chars().take(80).collect();
                }
                if let Some(o) = origin.filter(crate::model::finite3) {
                    s.origin = o;
                }
                if let Some(n) = normal.map(DVec3::from).filter(|n| n.length() > 1e-9) {
                    s.normal = n.normalize().to_array();
                }
                match active {
                    Some(true) => m.active_section = Some(id),
                    Some(false) if m.active_section == Some(id) => m.active_section = None,
                    _ => {}
                }
                json!({"section": id})
            }
            Command::SectionOff => {
                m.active_section = None;
                json!({})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
