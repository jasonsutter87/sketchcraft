//! Where every part of a model would lie if it were taken apart and laid flat on the ground: each part on its
//! biggest face with its length along red, longest first, parts of the same size and material next to each other,
//! a gap between them. Worked out from the model as it is; nothing in the model moves. Each part's frame is
//! `plugins::measure`'s, the same one `parts` reports.

use crate::model::{Id, Model};
use std::collections::BTreeSet;
use crate::plugins::{measure, tidy, tidy3, Measured};
use anyhow::{bail, Result};
use glam::{DMat3, DMat4, DVec3};
use serde_json::{json, Value};

/// The Dynamic Component attributes that place and turn a placement (they win over its transform).
const PLACE_KEYS: [&str; 6] = ["x", "y", "z", "rotx", "roty", "rotz"];

/// The widest gap that can be asked for, in meters.
pub const MAX_GAP: f64 = 10.0;

/// A part and where it lies in the lay-out.
pub struct Laid {
    pub part: Measured,
    /// takes the part from where it is in the model to where it lies: a turn and a move, never a mirror
    pub transform: DMat4,
    /// the box it lies in
    pub min: DVec3,
    pub max: DVec3,
}

/// The gap when none is asked for: 4% of the longest part, between 5 mm and 100 mm, so a shed's studs and a
/// printed bracket's pieces both read.
fn default_gap(parts: &[Measured]) -> f64 {
    let longest = parts.iter().map(|p| p.size[0]).fold(0.0, f64::max);
    (0.04 * longest).clamp(0.005, 0.1)
}

/// Longest first; the same size (to the millimeter) and material is the key Cut list counts by, so equal parts end up
/// next to each other; then by name and id, so the order never depends on how the model is stored. The cut plan numbers
/// its parts in this order too.
pub fn sort_parts(parts: &mut [Measured]) {
    let mm = |p: &Measured| p.size.map(|x| (x * 1000.0).round() as i64);
    parts.sort_by(|a, b| mm(b).cmp(&mm(a)).then_with(|| a.material.cmp(&b.material)).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id)));
}

/// Lay out `ids` (groups or components), or every part when there are none. Returns the parts in lay-out order and
/// the gap used. The lay-out sits under the parts: the same middle seen from above, resting at their lowest point.
pub fn lay_out(m: &Model, ids: &[Id], gap: Option<f64>) -> Result<(Vec<Laid>, f64)> {
    if let Some(g) = gap {
        if !g.is_finite() || !(0.0..=MAX_GAP).contains(&g) {
            bail!("the gap must be between 0 and {MAX_GAP} meters");
        }
    }
    let mut parts = measure(m, ids);
    let gap = gap.unwrap_or_else(|| default_gap(&parts));
    sort_parts(&mut parts);
    // where the parts are now
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for p in &parts {
        for k in 0..8 {
            let c = p.origin + p.u * (if k & 1 == 0 { 0.0 } else { p.extent.x }) + p.w * (if k & 2 == 0 { 0.0 } else { p.extent.y }) + p.n * (if k & 4 == 0 { 0.0 } else { p.extent.z });
            lo = lo.min(c);
            hi = hi.max(c);
        }
    }
    // rows from the back to the front, each filled left to right; about as wide as it is deep
    let area: f64 = parts.iter().map(|p| (p.extent.x + gap) * (p.extent.y + gap)).sum();
    let width = parts.iter().map(|p| p.extent.x).fold(area.sqrt(), f64::max);
    let (mut x, mut y, mut depth) = (0.0, 0.0, 0.0f64);
    let mut slots = Vec::with_capacity(parts.len());
    for p in &parts {
        if x > 0.0 && x + p.extent.x > width + 1e-9 {
            y -= depth + gap;
            x = 0.0;
            depth = 0.0;
        }
        slots.push(DVec3::new(x, y - p.extent.y, 0.0));
        x += p.extent.x + gap;
        depth = depth.max(p.extent.y);
    }
    let (mut flat_lo, mut flat_hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for (p, s) in parts.iter().zip(&slots) {
        flat_lo = flat_lo.min(*s);
        flat_hi = flat_hi.max(*s + p.extent);
    }
    let shift = DVec3::new((lo.x + hi.x - flat_lo.x - flat_hi.x) / 2.0, (lo.y + hi.y - flat_lo.y - flat_hi.y) / 2.0, lo.z);
    let laid = parts
        .into_iter()
        .zip(slots)
        .map(|(part, slot)| {
            let min = slot + shift;
            let max = min + part.extent;
            // the biggest face down and the length along red: u -> +x, n -> -z, and so w -> -y (a turn, no mirror)
            let turn = DMat3::from_cols(part.u, -part.w, -part.n).transpose();
            let turn = DMat3::from_cols_array(&turn.to_cols_array().map(|v| tidy(v, 1e-12)));
            let transform = DMat4::from_translation(min + DVec3::new(0.0, part.extent.y, part.extent.z)) * DMat4::from_mat3(turn) * DMat4::from_translation(-part.origin);
            Laid { part, transform, min, max }
        })
        .collect();
    Ok((laid, gap))
}

/// The reply to `parts_layout`.
pub fn reply(m: &Model, ids: &[Id], gap: Option<f64>) -> Result<Value> {
    let (laid, gap) = lay_out(m, ids, gap)?;
    let bounds = (!laid.is_empty()).then(|| {
        let (lo, hi) = laid.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(lo, hi), l| (lo.min(l.min), hi.max(l.max)));
        json!({"min": tidy3(lo, 1e-9), "max": tidy3(hi, 1e-9)})
    });
    let parts: Vec<Value> = laid
        .iter()
        .map(|l| {
            json!({
                "id": l.part.id, "name": l.part.name, "kind": l.part.kind, "material": l.part.material, "size": l.part.size,
                "transform": l.transform.to_cols_array().map(|v| tidy(v, 1e-12)),
                "min": tidy3(l.min, 1e-9), "max": tidy3(l.max, 1e-9),
            })
        })
        .collect();
    Ok(json!({"parts": parts, "gap": gap, "bounds": bounds, "units": m.units}))
}

/// The model as it looks with its parts laid out: a copy with every part turned and moved to its place, and
/// whatever is not a part (loose geometry, the faces of groups that hold other groups, scale figures, notes,
/// guides, section planes, point clouds) left out. For drawing only: it is never saved and nothing edits it.
pub struct LaidOutView {
    pub model: Model,
    pub laid: Vec<Laid>,
    pub gap: f64,
    /// visible faces outside every group: geometry that is no part, so it is not shown (and the view says so)
    pub loose_faces: usize,
}

pub fn laid_out_view(m: &Model, ids: &[Id], gap: Option<f64>) -> Result<LaidOutView> {
    let (laid, gap) = lay_out(m, ids, gap)?;
    let mut v = m.clone();
    let placements: BTreeSet<Id> = laid.iter().filter(|l| l.part.kind == "component").map(|l| l.part.id).collect();
    // a group part is measured with every group inside it (asked for by id, a group can hold groups): it moves whole
    let mut shown: BTreeSet<Id> = BTreeSet::new();
    let mut placements = placements;
    // each group part's tree, and which parts every group belongs to (a pass over the model, not one per part)
    let laid_ids: std::collections::HashSet<Id> = laid.iter().map(|o| o.part.id).collect();
    let mut kids: std::collections::HashMap<Id, Vec<Id>> = std::collections::HashMap::new();
    for (&id, g) in &m.groups {
        if let Some(p) = g.parent {
            kids.entry(p).or_default().push(id);
        }
    }
    let mut member: std::collections::HashMap<Id, Vec<usize>> = std::collections::HashMap::new();
    for (n, l) in laid.iter().enumerate().filter(|(_, l)| l.part.kind == "group") {
        let mut seen: std::collections::HashSet<Id> = std::collections::HashSet::from([l.part.id]);
        let mut stack = vec![l.part.id];
        while let Some(x) = stack.pop() {
            member.entry(x).or_default().push(n);
            for &k in kids.get(&x).into_iter().flatten() {
                if seen.insert(k) {
                    stack.push(k);
                }
            }
        }
        shown.extend(seen);
    }
    // the corners each part's faces and edges use: every group has its own, so each moves once
    let mut corners: std::collections::HashMap<usize, BTreeSet<Id>> = std::collections::HashMap::new();
    for f in m.faces.values() {
        for &n in f.group.and_then(|g| member.get(&g)).into_iter().flatten() {
            corners.entry(n).or_default().extend(f.loops().flatten().copied());
        }
    }
    for e in m.edges.values() {
        for &n in e.group.and_then(|g| member.get(&g)).into_iter().flatten() {
            corners.entry(n).or_default().extend([e.a, e.b]);
        }
    }
    // components placed inside a part (that are not parts of their own) go with it
    let mut inside: std::collections::HashMap<usize, Vec<Id>> = std::collections::HashMap::new();
    for (id, i) in &m.instances {
        if laid_ids.contains(id) {
            continue;
        }
        for &n in i.parent.and_then(|p| member.get(&p)).into_iter().flatten() {
            inside.entry(n).or_default().push(*id);
        }
    }
    for (n, l) in laid.iter().enumerate() {
        if l.part.kind == "group" {
            for c in corners.remove(&n).unwrap_or_default() {
                if let Some(x) = v.vertices.get_mut(&c) {
                    x.p = l.transform.transform_point3(x.p);
                }
            }
            for id in inside.remove(&n).unwrap_or_default() {
                if let Some(i) = v.instances.get_mut(&id) {
                    i.transform = l.transform * i.transform;
                    placements.insert(id);
                }
            }
        } else if let Some(i) = v.instances.get_mut(&l.part.id) {
            let def = m.definitions.get(&i.def);
            let drawn = def.map(|d| Model::placement_transforms(m, &m.instances[&l.part.id], d)).unwrap_or_default();
            i.transform = l.transform * i.transform;
            // a Dynamic Component may take its place and turn from its attributes (x, y, z, rotx, roty, rotz), which
            // win over its transform: give the copy the laid-out ones, worked out from where it is drawn now
            let placed_by_attrs = crate::dynamic::attrs_of(i, def).keys().any(|k| PLACE_KEYS.contains(&k.as_str()));
            if let (true, Some(base)) = (placed_by_attrs, drawn.first()) {
                let (_, rot, pos) = (l.transform * *base).to_scale_rotation_translation();
                let (z, y, x) = rot.to_euler(glam::EulerRot::ZYX);
                i.attrs.retain(|k, _| !PLACE_KEYS.contains(&k.to_ascii_lowercase().as_str()));
                for (k, val) in [("x", pos.x), ("y", pos.y), ("z", pos.z), ("rotx", x.to_degrees()), ("roty", y.to_degrees()), ("rotz", z.to_degrees())] {
                    i.attrs.insert(k.to_string(), format!("{val}"));
                }
            }
        }
    }
    let loose_faces = m.faces.values().filter(|f| f.group.is_none() && !f.hidden).count();
    for f in v.faces.values_mut() {
        if !f.group.is_some_and(|g| shown.contains(&g)) {
            f.hidden = true;
        }
    }
    for e in v.edges.values_mut() {
        if !e.group.is_some_and(|g| shown.contains(&g)) {
            e.hidden = true;
        }
    }
    for (id, i) in v.instances.iter_mut() {
        if !placements.contains(id) {
            i.hidden = true;
        }
    }
    // what points at places in the assembled model has no place in the laid-out one
    v.annotations.clear();
    v.guides.clear();
    v.sections.clear();
    v.active_section = None;
    v.cages.clear();
    v.photo_points.clear();
    v.photo_edges.clear();
    v.photo_guides.clear();
    v.clouds.clear();
    Ok(LaidOutView { model: v, laid, gap, loose_faces })
}

/// What the app shows about the laid-out view: each part (to name a clicked one), the gap, and what was left out.
pub fn summary(view: &LaidOutView) -> Value {
    let bounds = (!view.laid.is_empty()).then(|| {
        let (lo, hi) = view.laid.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(lo, hi), l| (lo.min(l.min), hi.max(l.max)));
        json!({"min": tidy3(lo, 1e-9), "max": tidy3(hi, 1e-9)})
    });
    json!({
        "parts": view.laid.iter().map(|l| json!({"id": l.part.id, "name": l.part.name, "kind": l.part.kind, "material": l.part.material, "size": l.part.size})).collect::<Vec<_>>(),
        "gap": view.gap,
        "bounds": bounds,
        "loose_faces": view.loose_faces,
    })
}
