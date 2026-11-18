//! Undo history that survives another program's save, one client's steps at a time.
//!
//! Normally an undo step is a copy of the model as it was before the step (fast, exact, and the
//! only thing a single client needs). Those copies are only valid while nobody else changes the file:
//! going back to one after another program saved would erase that program's work. So when a reload
//! brings in someone else's save, each step is turned into a `Patch`: exactly what that step changed
//! (entity by entity), and undoing a patch puts back only those entities, on top of the model as it is
//! now. A step can be undone only if everything it touched is still as the step left it and nothing
//! built on it since; otherwise Undo refuses with a message, and nothing is guessed. A refused step stays
//! in the list (the other program may undo its change), and asking again skips it.
//!
//! The conversion is lazy: a reload only pairs each model copy with the one after it (`Step::Stale`, no
//! work, no copying), and the changes are worked out when that very step is undone, so a big model
//! never freezes on another program's save and no step is let go for being slow.

use crate::model::Model;
use serde_json::Value;
use std::sync::Arc;

/// One entry of the undo (or redo) list.
pub enum Step {
    /// the model before the step (a step taken since the last reload)
    Snap(Box<Model>),
    /// what the step changed (a step that was carried across another program's save)
    Patch(Patch),
    /// a step taken before another program's save, not yet turned into a `Patch`: the model before the step and
    /// the model after it (the next step's "before", or what was on screen); the changes are worked out when it
    /// is undone (or redone)
    Stale { before: Arc<Model>, after: Arc<Model> },
}

impl Step {
    pub fn approx_bytes(&self) -> usize {
        match self {
            Step::Snap(m) => m.approx_bytes(),
            Step::Patch(p) => p.bytes,
            // the "after" model is shared with the next step's "before" (counted there) unless nothing else holds it: the last
            // step of a reload holds a fresh copy of the model that was on screen, which is as big as any other
            Step::Stale { before, after } => before.approx_bytes() + if Arc::strong_count(after) == 1 { after.approx_bytes() } else { 0 },
        }
    }
}

/// What one step changed: per top-level part of the model (points, edges, faces, groups, ...),
/// the entries that differ, with their value before and after (`None`: not there).
pub struct Patch {
    parts: Vec<(String, Change)>,
    bytes: usize,
}

enum Change {
    /// a part that is one value (the units, a list of views...): replaced whole
    Whole(Option<Value>, Option<Value>),
    /// a part that is a map by id or name: only the entries that changed
    Keys(Vec<(String, Option<Value>, Option<Value>)>),
}

/// The ids and names change counters don't count as a change of the model's content.
const SKIP: [&str; 1] = ["next_id"];

fn norm(v: Option<&Value>) -> Option<Value> {
    v.filter(|v| !v.is_null()).cloned()
}

/// Two stored values are the same, give or take the last digit of a number (a coordinate that went through a
/// file and back can come out one unit in the last place away).
fn close(a: &Option<Value>, b: &Option<Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => values_close(x, y),
        _ => false,
    }
}

/// Two floats are the same number if they are equal, or a few units in the last place apart, or closer than a
/// nanometre (the model's unit is the metre). Why this and not a relative tolerance: the reload goes through a file
/// that writes and reads every float exactly (`float_roundtrip`), so the values a step recorded come back bit for
/// bit and no slack is needed for that; what remains is arithmetic done twice (a move and its inverse) that can end
/// one or two units in the last place away. A units-in-the-last-place test follows the size of the number: at
/// 5,000,000 m a unit is a nanometre, so a 4 mm change by another program is nowhere near it, where the old relative
/// 1e-9 accepted anything under 5 mm there. The absolute nanometre keeps numbers near zero (0.0 against 1e-17) equal.
pub fn floats_match_for_test(x: f64, y: f64) -> bool {
    floats_match(x, y)
}

fn floats_match(x: f64, y: f64) -> bool {
    if x == y {
        return true;
    }
    if !x.is_finite() || !y.is_finite() {
        return false;
    }
    if (x - y).abs() <= 1e-9 {
        return true;
    }
    // distance in representable numbers (the bit patterns of finite floats, ordered)
    let key = |v: f64| -> i64 {
        let b = v.to_bits() as i64;
        if b < 0 { i64::MIN - b } else { b }
    };
    key(x).abs_diff(key(y)) <= 4
}

fn values_close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => floats_match(x, y),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_close(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| values_close(p, q))),
        _ => a == b,
    }
}

fn size_of(v: &Option<Value>) -> usize {
    v.as_ref().map_or(0, |v| v.to_string().len())
}

impl Patch {
    /// What changed from `before` to `after` (both as JSON objects).
    pub fn between(before: &Value, after: &Value) -> Patch {
        let (empty, ob, oa) = (serde_json::Map::new(), before.as_object(), after.as_object());
        let (ob, oa) = (ob.unwrap_or(&empty), oa.unwrap_or(&empty));
        let mut names: Vec<&String> = ob.keys().chain(oa.keys()).collect();
        names.sort();
        names.dedup();
        let mut parts = vec![];
        let mut bytes = 64;
        for name in names {
            if SKIP.contains(&name.as_str()) {
                continue;
            }
            let (b, a) = (norm(ob.get(name)), norm(oa.get(name)));
            if a == b {
                continue;
            }
            // maps keyed by id (points, edges, faces, groups, components...) are compared entry by entry
            let both_maps = b.as_ref().is_none_or(Value::is_object) && a.as_ref().is_none_or(Value::is_object);
            if both_maps {
                let em = serde_json::Map::new();
                let (bm, am) = (b.as_ref().and_then(Value::as_object).unwrap_or(&em), a.as_ref().and_then(Value::as_object).unwrap_or(&em));
                let mut keys: Vec<&String> = bm.keys().chain(am.keys()).collect();
                keys.sort();
                keys.dedup();
                let mut changes = vec![];
                for k in keys {
                    let (kb, ka) = (norm(bm.get(k)), norm(am.get(k)));
                    if kb != ka {
                        bytes += k.len() + size_of(&kb) + size_of(&ka) + 48;
                        changes.push((k.clone(), kb, ka));
                    }
                }
                parts.push((name.clone(), Change::Keys(changes)));
            } else {
                bytes += name.len() + size_of(&b) + size_of(&a) + 48;
                parts.push((name.clone(), Change::Whole(b, a)));
            }
        }
        Patch { parts, bytes }
    }

    /// No entry changed.
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|(_, c)| matches!(c, Change::Keys(k) if k.is_empty()))
    }

    /// How many things the step changed: faces, edges, groups, placements and components (the points and
    /// the other bookkeeping that come with them aren't counted twice), and each small part replaced whole.
    pub fn things(&self) -> usize {
        self.parts
            .iter()
            .map(|(name, c)| match c {
                Change::Whole(..) => 1,
                Change::Keys(k) if matches!(name.as_str(), "faces" | "edges" | "groups" | "instances" | "definitions" | "clouds" | "sheets" | "sheet_items") => k.len(),
                Change::Keys(k) => usize::from(!k.is_empty()),
            })
            .sum()
    }

    /// Put `forward` (false: the step's "before") on top of `current`. Err names what stands in the way.
    pub fn apply(&self, current: &Model, forward: bool) -> Result<Model, String> {
        // the small parts of the model go through JSON; the big maps (points, edges, faces...) are worked entry by entry
        let mut v = serde_json::to_value(skeleton(current)).map_err(|e| e.to_string())?;
        let Some(obj) = v.as_object_mut() else { return Err("the model couldn't be read".into()) };
        let mut conflicts: Vec<String> = vec![];
        // what the step left there (to be replaced) and what it should become
        let pick = |before: &Option<Value>, after: &Option<Value>| if forward { (before.clone(), after.clone()) } else { (after.clone(), before.clone()) };
        for (name, change) in &self.parts {
            if TYPED.contains(&name.as_str()) {
                continue;
            }
            match change {
                Change::Whole(b, a) => {
                    let (expect, to) = pick(b, a);
                    if !close(&norm(obj.get(name)), &expect) {
                        conflicts.push(name.clone());
                        continue;
                    }
                    match to {
                        Some(t) => obj.insert(name.clone(), t),
                        None => obj.remove(name),
                    };
                }
                Change::Keys(keys) => {
                    let mut map = match obj.get(name) {
                        Some(Value::Object(m)) => m.clone(),
                        None | Some(Value::Null) => serde_json::Map::new(),
                        Some(_) => {
                            conflicts.push(name.clone());
                            continue;
                        }
                    };
                    for (k, b, a) in keys {
                        let (expect, to) = pick(b, a);
                        if !close(&norm(map.get(k)), &expect) {
                            conflicts.push(format!("{name} {k}"));
                            continue;
                        }
                        match to {
                            Some(t) => map.insert(k.clone(), t),
                            None => map.remove(k),
                        };
                    }
                    // (an empty map stays an empty object: some parts are required when the model is read back)
                    obj.insert(name.clone(), Value::Object(map));
                }
            }
        }
        let mut model: Model = serde_json::from_value(v).map_err(|e| format!("the result wasn't a model ({e})"))?;
        model.vertices = current.vertices.clone();
        model.edges = current.edges.clone();
        model.faces = current.faces.clone();
        model.groups = current.groups.clone();
        model.materials = current.materials.clone();
        model.definitions = current.definitions.clone();
        model.instances = current.instances.clone();
        model.clouds = current.clouds.clone();
        model.sheets = current.sheets.clone();
        model.sheet_items = current.sheet_items.clone();
        for (name, change) in &self.parts {
            let Change::Keys(keys) = change else { continue };
            match name.as_str() {
                "vertices" => apply_map(&mut model.vertices, name, keys, forward, &mut conflicts),
                "edges" => apply_map(&mut model.edges, name, keys, forward, &mut conflicts),
                "faces" => apply_map(&mut model.faces, name, keys, forward, &mut conflicts),
                "groups" => apply_map(&mut model.groups, name, keys, forward, &mut conflicts),
                "materials" => apply_map(&mut model.materials, name, keys, forward, &mut conflicts),
                "definitions" => apply_map(&mut model.definitions, name, keys, forward, &mut conflicts),
                "instances" => apply_map(&mut model.instances, name, keys, forward, &mut conflicts),
                "clouds" => apply_map(&mut model.clouds, name, keys, forward, &mut conflicts),
                "sheets" => apply_map(&mut model.sheets, name, keys, forward, &mut conflicts),
                "sheet_items" => apply_map(&mut model.sheet_items, name, keys, forward, &mut conflicts),
                _ => {}
            }
        }
        if !conflicts.is_empty() {
            return Err(conflicts_text(&conflicts));
        }
        model.check_sane().map_err(|e| e.to_string())?;
        // a step must not make components contain themselves or multiply (it was fine before, or the step wouldn't exist)
        if let Err(p) = model.expanded_count() {
            return Err(format!("{p}, so that step can't be applied"));
        }
        // nothing may be left pointing at something the step took away (another program built on it): a point, an
        // edge, a group, a material... Problems the model already had before are not this step's doing.
        let now = model.broken_refs(true);
        if !now.is_empty() {
            let had = current.broken_refs(true);
            if let Some(what) = now.into_iter().find(|w| !had.contains(w)) {
                return Err(format!("{what} now depends on it"));
            }
        }
        Ok(model)
    }
}

/// The parts of a model that are big maps by id or name, compared and changed entry by entry on the typed model
/// (not through JSON of the whole model, which on a million-entity model takes seconds).
const TYPED: [&str; 10] = ["vertices", "edges", "faces", "groups", "materials", "definitions", "instances", "clouds", "sheets", "sheet_items"];

/// The model with the big maps left empty (what is left is small, or at least rarely changes).
fn skeleton(m: &Model) -> Model {
    // every field is named, so a field added to the model is a compile error here until it is placed
    let Model { next_id, vertices: _, edges: _, faces: _, groups: _, materials: _, active_group, tags, environment, tag_colors, definitions: _, instances: _, guides, photo_points, cages, photo_edges, photo_guides, views, timeline, annotations, units, classes, location, clouds: _, sections, active_section, axes, drafting, sheets: _, sheet_items: _, title_blocks, deadline: _, remaps: _ } = m;
    Model {
        next_id: *next_id,
        vertices: Default::default(),
        edges: Default::default(),
        faces: Default::default(),
        groups: Default::default(),
        materials: Default::default(),
        active_group: *active_group,
        tags: tags.clone(),
        environment: environment.clone(),
        tag_colors: tag_colors.clone(),
        definitions: Default::default(),
        instances: Default::default(),
        guides: guides.clone(),
        photo_points: photo_points.clone(),
        cages: cages.clone(),
        photo_edges: photo_edges.clone(),
        photo_guides: photo_guides.clone(),
        views: views.clone(),
        timeline: timeline.clone(),
        annotations: annotations.clone(),
        units: units.clone(),
        classes: classes.clone(),
        location: location.clone(),
        clouds: Default::default(),
        sections: sections.clone(),
        active_section: *active_section,
        axes: *axes,
        drafting: drafting.clone(),
        sheets: Default::default(),
        sheet_items: Default::default(),
        // a handful of small records: compared as part of the skeleton, entry by entry like any map by id
        title_blocks: title_blocks.clone(),
        deadline: None,
        remaps: Vec::new(),
    }
}

type Entry = (String, Option<Value>, Option<Value>);

fn value_of<V: serde::Serialize>(v: &V) -> Option<Value> {
    serde_json::to_value(v).ok().filter(|v| !v.is_null())
}

/// The entries that differ between two maps (compared as typed values; only those that differ are turned into JSON).
fn diff_map<K: Ord + ToString, V: serde::Serialize>(b: &std::collections::BTreeMap<K, V>, a: &std::collections::BTreeMap<K, V>, same: impl Fn(&V, &V) -> bool) -> Vec<Entry> {
    let mut out = vec![];
    let (mut ib, mut ia) = (b.iter().peekable(), a.iter().peekable());
    loop {
        match (ib.peek(), ia.peek()) {
            (None, None) => break,
            (Some((kb, vb)), None) => {
                out.push((kb.to_string(), value_of(*vb), None));
                ib.next();
            }
            (None, Some((ka, va))) => {
                out.push((ka.to_string(), None, value_of(*va)));
                ia.next();
            }
            (Some((kb, vb)), Some((ka, va))) => match kb.cmp(ka) {
                std::cmp::Ordering::Less => {
                    out.push((kb.to_string(), value_of(*vb), None));
                    ib.next();
                }
                std::cmp::Ordering::Greater => {
                    out.push((ka.to_string(), None, value_of(*va)));
                    ia.next();
                }
                std::cmp::Ordering::Equal => {
                    if !same(vb, va) {
                        let (vb, va) = (value_of(*vb), value_of(*va));
                        if vb != va {
                            out.push((kb.to_string(), vb, va));
                        }
                    }
                    ib.next();
                    ia.next();
                }
            },
        }
    }
    out
}

/// Apply entry changes to a typed map: each entry must be as the step left it (give or take the last digit of a number).
fn apply_map<K: Ord + std::str::FromStr, V: serde::Serialize + serde::de::DeserializeOwned>(map: &mut std::collections::BTreeMap<K, V>, part: &str, keys: &[Entry], forward: bool, conflicts: &mut Vec<String>) {
    for (k, b, a) in keys {
        let (expect, to) = if forward { (b, a) } else { (a, b) };
        let Ok(key) = k.parse::<K>() else {
            conflicts.push(format!("{part} {k}"));
            continue;
        };
        if !close(&map.get(&key).and_then(value_of), expect) {
            conflicts.push(format!("{part} {k}"));
            continue;
        }
        match to {
            Some(t) => match serde_json::from_value::<V>(t.clone()) {
                Ok(v) => {
                    map.insert(key, v);
                }
                Err(_) => conflicts.push(format!("{part} {k}")),
            },
            None => {
                map.remove(&key);
            }
        }
    }
}

fn conflicts_text(c: &[String]) -> String {
    let shown: Vec<&str> = c.iter().take(3).map(String::as_str).collect();
    let more = if c.len() > 3 { format!(" and {} more", c.len() - 3) } else { String::new() };
    format!("{}{more} changed since", shown.join(", "))
}

/// The undo and redo lists of an engine, carried across a reload.
pub struct History {
    pub undo: Vec<Step>,
    pub redo: Vec<Step>,
    /// steps that couldn't be carried over, from the undo list and from the redo list
    pub dropped: usize,
    pub dropped_redo: usize,
}

/// Turn the undo and redo lists (as they are with `current` on screen) into ones that survive a reload.
/// Nothing is worked out here: each model copy is paired with the one that follows it (`Step::Stale`), and
/// the changes are found when that step is undone. Steps already patches stay as they are.
pub fn carry(undo: Vec<Step>, redo: Vec<Step>, current: &Model) -> History {
    let mut dropped = 0;
    let mut dropped_redo = 0;
    let mut cur: Option<Arc<Model>> = None;
    let mut now = || cur.get_or_insert_with(|| Arc::new(current.clone())).clone();
    // undo: [oldest .. newest]. A copy's "after" is the next copy's "before", or what is on screen.
    enum Item {
        Copy(Arc<Model>),
        Other(Step),
    }
    let items: Vec<Item> = undo
        .into_iter()
        .map(|s| match s {
            Step::Snap(m) => Item::Copy(Arc::new(*m)),
            other => Item::Other(other),
        })
        .collect();
    let mut out_undo: Vec<Step> = vec![];
    for i in 0..items.len() {
        match &items[i] {
            Item::Other(Step::Patch(p)) => out_undo.push(Step::Patch(p.clone_patch())),
            Item::Other(Step::Stale { before, after }) => out_undo.push(Step::Stale { before: before.clone(), after: after.clone() }),
            Item::Other(Step::Snap(_)) => {}
            Item::Copy(before) => {
                let after = match items.get(i + 1) {
                    None => Some(now()),
                    Some(Item::Copy(n)) => Some(n.clone()),
                    // a patch above a copy: what the copy led to is unknown
                    Some(Item::Other(_)) => None,
                };
                match after {
                    Some(after) => out_undo.push(Step::Stale { before: before.clone(), after }),
                    None => dropped += 1,
                }
            }
        }
    }
    // redo: [bottom .. next]. From the next step down, each copy leads to the one under it; past a patch the model
    // isn't known, so copies below one can't be paired.
    let mut out_redo: Vec<Step> = vec![];
    let mut prev: Option<Arc<Model>> = Some(now());
    for s in redo.into_iter().rev() {
        match s {
            Step::Snap(m) => {
                let next = Arc::new(*m);
                match prev.take() {
                    Some(before) => {
                        out_redo.push(Step::Stale { before, after: next.clone() });
                        prev = Some(next);
                    }
                    None => dropped_redo += 1,
                }
            }
            Step::Patch(p) => {
                out_redo.push(Step::Patch(p));
                prev = None;
            }
            Step::Stale { before, after } => {
                out_redo.push(Step::Stale { before, after });
                prev = None;
            }
        }
    }
    // out_redo was built next-first; the list keeps the next step last
    out_redo.reverse();
    History { undo: out_undo, redo: out_redo, dropped, dropped_redo }
}

impl Patch {
    fn clone_patch(&self) -> Patch {
        Patch { parts: self.parts.iter().map(|(n, c)| (n.clone(), clone_change(c))).collect(), bytes: self.bytes }
    }
}

fn clone_change(c: &Change) -> Change {
    match c {
        Change::Whole(a, b) => Change::Whole(a.clone(), b.clone()),
        Change::Keys(k) => Change::Keys(k.clone()),
    }
}

/// The changes between two models, as a patch (empty when the step changed nothing in the model's content).
pub fn patch_between(before: &Model, after: &Model) -> Patch {
    let b = serde_json::to_value(skeleton(before)).unwrap_or(Value::Null);
    let a = serde_json::to_value(skeleton(after)).unwrap_or(Value::Null);
    let mut p = Patch::between(&b, &a);
    let mut add = |name: &str, keys: Vec<Entry>| {
        if !keys.is_empty() {
            p.bytes += keys.iter().map(|(k, b, a)| k.len() + size_of(b) + size_of(a) + 48).sum::<usize>();
            p.parts.push((name.to_string(), Change::Keys(keys)));
        }
    };
    add("vertices", diff_map(&before.vertices, &after.vertices, |x, y| x == y));
    add("edges", diff_map(&before.edges, &after.edges, |x, y| x == y));
    add("faces", diff_map(&before.faces, &after.faces, |x, y| x == y));
    add("groups", diff_map(&before.groups, &after.groups, |x, y| x == y));
    add("materials", diff_map(&before.materials, &after.materials, |x, y| x == y));
    // a component's own model is shared between steps unless it was edited: the same Arc is the same content
    add("definitions", diff_map(&before.definitions, &after.definitions, |x, y| std::sync::Arc::ptr_eq(&x.model, &y.model) && x.name == y.name && x.attrs == y.attrs && x.source == y.source && x.cuts == y.cuts && x.face_me == y.face_me));
    add("instances", diff_map(&before.instances, &after.instances, |x, y| x == y));
    add("clouds", diff_map(&before.clouds, &after.clouds, |x, y| x == y));
    add("sheets", diff_map(&before.sheets, &after.sheets, |x, y| x == y));
    // a paper item is shared between steps unless it was edited: the same Arc is the same content
    add("sheet_items", diff_map(&before.sheet_items, &after.sheet_items, |x, y| std::sync::Arc::ptr_eq(x, y) || x == y));
    p
}
