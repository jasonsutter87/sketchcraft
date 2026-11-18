//! What a change to the model can have changed on paper (design §4, "Invalidation").
//!
//! `changes(before, after)` compares the two models and says where, in world space, a drawing could now look
//! different: a list of boxes (each changed entity's owner, as it was and as it is), plus flags for the few things
//! that change drawings wherever they are (display units, saved views, the hatch and line type tables, pictures).
//! A viewport's drawing is redrawn only when one of those crosses what it shows (`cache::Volume::hit`).
//!
//! Why owners and not just the entity: a plan or section fills each cut region as one outline per owner (the
//! innermost tagged group or placement, else the face's own group; see `components::WFace::cut`), and a region's
//! outline, fill and hatch are one primitive. Moving one face of an owner can change that primitive anywhere along
//! the outline, so the whole owner's box counts. Owners whose tag has a cut style and whose box overlaps a changed
//! box are added too: regions of different tags may interact where they overlap (a cut priority that subtracts
//! one tag's regions from another's, #88), so they are covered without changes here.
//!
//! Every top-level field of `Model` is named in `changes` (a destructuring pattern): a new field does not compile
//! until someone decides what it does to drawings.

use crate::model::{Id, Model};
use glam::{DMat4, DVec3};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Past this many changed entities, every drawing is redrawn (working out boxes would cost more than it saves).
const MAX_PRECISE: usize = 50_000;

/// An axis-aligned box in world metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bx {
    pub lo: DVec3,
    pub hi: DVec3,
}

impl Bx {
    pub const EMPTY: Bx = Bx { lo: DVec3::splat(f64::INFINITY), hi: DVec3::splat(f64::NEG_INFINITY) };
    pub fn is_empty(&self) -> bool {
        !(self.lo.x <= self.hi.x && self.lo.y <= self.hi.y && self.lo.z <= self.hi.z)
    }
    pub fn add(&mut self, p: DVec3) {
        if p.is_finite() {
            self.lo = self.lo.min(p);
            self.hi = self.hi.max(p);
        }
    }
    pub fn union(&mut self, o: &Bx) {
        if !o.is_empty() {
            self.lo = self.lo.min(o.lo);
            self.hi = self.hi.max(o.hi);
        }
    }
    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.lo, self.hi);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(a.x, b.y, b.z),
            DVec3::new(b.x, b.y, b.z),
        ]
    }
    pub fn transformed(&self, t: &DMat4) -> Bx {
        let mut out = Bx::EMPTY;
        if !self.is_empty() {
            for c in self.corners() {
                out.add(t.transform_point3(c));
            }
        }
        out
    }
    /// Do the two overlap by more than `margin` on every axis (touching boxes don't: a wall standing on a slab, a
    /// wall butting into another)?
    pub fn overlaps_inside(&self, o: &Bx, margin: f64) -> bool {
        !self.is_empty() && !o.is_empty() && (0..3).all(|i| self.lo[i] + margin < o.hi[i] && o.lo[i] + margin < self.hi[i])
    }
}

/// Where drawings may have changed.
#[derive(Clone, Debug, Default)]
pub struct Dirty {
    /// every drawing
    pub all: bool,
    /// every picture (raster) drawing: materials, colours, lights, scans, section planes
    pub raster: bool,
    /// the saved views changed: drawings that take their camera from a saved view by name
    pub views: bool,
    /// world boxes (owners before and after)
    pub boxes: Vec<Bx>,
    /// heights of changed model annotations (notes, swings, model dimensions): plans of floors that show them
    pub notes_z: Vec<f64>,
    /// every drawing that draws a cut (plans and sections): cut priorities (#88) changed, or the model changed while
    /// some are set (resolving them unions each tag's regions over the whole plane, so a cut can change far away)
    pub cuts: bool,
    /// boxes of changed stairs (groups named "stair"): a plan draws a stair that starts or arrives within 0.3 m of its
    /// floor, a little further down than anything else it draws
    pub stairs: Vec<Bx>,
}

impl Dirty {
    pub fn is_empty(&self) -> bool {
        !self.all && !self.raster && !self.views && !self.cuts && self.boxes.is_empty() && self.notes_z.is_empty() && self.stairs.is_empty()
    }
    pub fn everything() -> Dirty {
        Dirty { all: true, ..Default::default() }
    }
}

/// Bounds of the root model's pieces, worked out on demand.
struct Index<'m> {
    m: &'m Model,
    /// what sits directly in a group: its edges, faces and placements (not its child groups)
    own: HashMap<Id, Bx>,
    /// faces and edges in no group
    loose: Bx,
    kids: HashMap<Id, Vec<Id>>,
    sub: RefCell<HashMap<Id, Bx>>,
    defs: RefCell<HashMap<Id, Bx>>,
    /// the boxes of stairs met on the way (see `Dirty::stairs`)
    stairs: RefCell<Vec<Bx>>,
}

impl<'m> Index<'m> {
    fn new(m: &'m Model) -> Self {
        let mut ix = Index { m, own: HashMap::new(), loose: Bx::EMPTY, kids: HashMap::new(), sub: RefCell::new(HashMap::new()), defs: RefCell::new(HashMap::new()), stairs: RefCell::new(Vec::new()) };
        for (id, g) in &m.groups {
            if let Some(p) = g.parent {
                ix.kids.entry(p).or_default().push(*id);
            }
        }
        let p = |v: &Id| m.vertices.get(v).map(|v| v.p);
        for e in m.edges.values() {
            let slot = match e.group {
                Some(g) => ix.own.entry(g).or_insert(Bx::EMPTY),
                None => &mut ix.loose,
            };
            for v in [e.a, e.b] {
                if let Some(q) = p(&v) {
                    slot.add(q);
                }
            }
        }
        for f in m.faces.values() {
            let slot = match f.group {
                Some(g) => ix.own.entry(g).or_insert(Bx::EMPTY),
                None => &mut ix.loose,
            };
            for v in f.outer.iter() {
                if let Some(q) = p(v) {
                    slot.add(q);
                }
            }
        }
        let placed: Vec<(Option<Id>, Bx)> = m.instances.keys().map(|i| (m.instances[i].parent, ix.instance(*i))).collect();
        // (a placement at the top level is no group's: a change to it counts its own box)
        for (parent, b) in placed {
            if let Some(g) = parent {
                ix.own.entry(g).or_insert(Bx::EMPTY).union(&b);
            }
        }
        ix
    }

    /// A definition's own bounds (its vertices and its placements of other components), in its own frame.
    fn def_box(&self, d: Id, depth: usize) -> Bx {
        if let Some(b) = self.defs.borrow().get(&d) {
            return *b;
        }
        let mut b = Bx::EMPTY;
        if let Some(def) = self.m.definitions.get(&d) {
            for v in def.model.vertices.values() {
                b.add(v.p);
            }
            if depth < crate::nesting::MAX_NEST {
                for inst in def.model.instances.values() {
                    let inner = self.def_box(inst.def, depth + 1);
                    let Some(nd) = self.m.definitions.get(&inst.def) else { continue };
                    for t in crate::model::Model::placement_transforms(self.m, inst, nd) {
                        b.union(&inner.transformed(&t));
                    }
                }
            }
        }
        self.defs.borrow_mut().insert(d, b);
        b
    }

    /// A root placement's world box (every copy a Dynamic Component makes).
    fn instance(&self, i: Id) -> Bx {
        let Some(inst) = self.m.instances.get(&i) else { return Bx::EMPTY };
        let Some(def) = self.m.definitions.get(&inst.def) else { return Bx::EMPTY };
        let local = self.def_box(inst.def, 0);
        let mut b = Bx::EMPTY;
        for t in crate::model::Model::placement_transforms(self.m, inst, def) {
            b.union(&local.transformed(&t));
        }
        b
    }

    /// A group with everything inside it.
    fn subtree(&self, g: Id) -> Bx {
        if let Some(b) = self.sub.borrow().get(&g) {
            return *b;
        }
        let mut b = self.own.get(&g).copied().unwrap_or(Bx::EMPTY);
        let mut stack: Vec<(Id, usize)> = self.kids.get(&g).map(|k| k.iter().map(|c| (*c, 1)).collect()).unwrap_or_default();
        while let Some((c, depth)) = stack.pop() {
            if depth > crate::nesting::MAX_NEST {
                continue;
            }
            if let Some(o) = self.own.get(&c) {
                b.union(o);
            }
            if let Some(k) = self.kids.get(&c) {
                stack.extend(k.iter().map(|x| (*x, depth + 1)));
            }
        }
        self.sub.borrow_mut().insert(g, b);
        b
    }

    /// The nearest tagged group from `g` outward.
    fn tagged_up(&self, mut g: Option<Id>) -> Option<Id> {
        for _ in 0..crate::nesting::MAX_NEST {
            let id = g?;
            let grp = self.m.groups.get(&id)?;
            if grp.tag.is_some() {
                return Some(id);
            }
            g = grp.parent;
        }
        None
    }

    /// What a face or edge in group `g` (None: loose) is drawn with: its owner's box.
    fn in_group(&self, g: Option<Id>, out: &mut Vec<Bx>) {
        match g {
            None => out.push(self.loose),
            Some(g) => {
                out.push(self.subtree(self.tagged_up(Some(g)).unwrap_or(g)));
                // a stair or a fixture is drawn from all of its own group (stairs, overhead lines)
                out.push(self.subtree(g));
                let n = self.m.groups.get(&g).map(|g| g.name.to_ascii_lowercase()).unwrap_or_default();
                if n.contains("stair") && !n.contains("rail") {
                    self.stairs.borrow_mut().push(self.subtree(g));
                }
            }
        }
    }

    fn group(&self, g: Id, out: &mut Vec<Bx>) {
        if self.m.groups.contains_key(&g) {
            self.in_group(Some(g), out);
        }
    }

    fn placement(&self, i: Id, out: &mut Vec<Bx>) {
        let Some(inst) = self.m.instances.get(&i) else { return };
        out.push(self.instance(i));
        if inst.tag.is_none() {
            if let Some(t) = self.tagged_up(inst.parent) {
                out.push(self.subtree(t));
            }
        }
    }

    /// Every owner whose tag has a style, with its box.
    fn styled_owners(&self, styled: &HashSet<&str>) -> Vec<Bx> {
        let mut out = Vec::new();
        for (id, g) in &self.m.groups {
            if g.tag.as_deref().is_some_and(|t| styled.contains(t)) {
                out.push(self.subtree(*id));
            }
        }
        for (id, i) in &self.m.instances {
            if i.tag.as_deref().is_some_and(|t| styled.contains(t)) {
                out.push(self.instance(*id));
            }
        }
        out
    }

    /// Everything drawn with one of `tags`: groups and placements carrying it, and placements of components that
    /// have it inside.
    fn tagged(&self, tags: &BTreeSet<String>, out: &mut Vec<Bx>) {
        if tags.is_empty() {
            return;
        }
        for (id, g) in &self.m.groups {
            if g.tag.as_ref().is_some_and(|t| tags.contains(t)) {
                out.push(self.subtree(*id));
            }
        }
        // components with the tag somewhere inside (through components inside components)
        let mut inside: HashSet<Id> = HashSet::new();
        for (d, def) in &self.m.definitions {
            if def.model.groups.values().any(|g| g.tag.as_ref().is_some_and(|t| tags.contains(t))) || def.model.instances.values().any(|i| i.tag.as_ref().is_some_and(|t| tags.contains(t))) {
                inside.insert(*d);
            }
        }
        grow_defs(self.m, &mut inside);
        for (id, i) in &self.m.instances {
            if i.tag.as_ref().is_some_and(|t| tags.contains(t)) || inside.contains(&i.def) {
                self.placement(*id, out);
            }
        }
    }
}

/// Add to `set` every definition that places one of them (to any depth).
fn grow_defs(m: &Model, set: &mut HashSet<Id>) {
    for _ in 0..crate::nesting::MAX_NEST {
        let more: Vec<Id> = m.definitions.iter().filter(|(d, def)| !set.contains(d) && def.model.instances.values().any(|i| set.contains(&i.def))).map(|(d, _)| *d).collect();
        if more.is_empty() {
            return;
        }
        set.extend(more);
    }
}

fn keys_changed<K: Ord + Copy, V>(b: &std::collections::BTreeMap<K, V>, a: &std::collections::BTreeMap<K, V>, same: impl Fn(&V, &V) -> bool) -> Vec<K> {
    let mut out: Vec<K> = b.iter().filter(|(k, v)| a.get(k).is_none_or(|w| !same(v, w))).map(|(k, _)| *k).collect();
    out.extend(a.keys().filter(|k| !b.contains_key(k)).copied());
    out
}

fn names_changed<V>(b: &std::collections::BTreeMap<String, V>, a: &std::collections::BTreeMap<String, V>, same: impl Fn(&V, &V) -> bool) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = b.iter().filter(|(k, v)| a.get(*k).is_none_or(|w| !same(v, w))).map(|(k, _)| k.clone()).collect();
    out.extend(a.keys().filter(|k| !b.contains_key(*k)).cloned());
    out
}

fn json_eq<T: serde::Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// The heights of an annotation's points.
fn note_heights(a: &crate::model::Annotation, out: &mut Vec<f64>) {
    use crate::model::Annotation as A;
    match a {
        A::Text { at, leader, .. } => {
            out.push(at.z);
            out.extend(leader.map(|l| l.z));
        }
        A::Swing { hinge, .. } => out.push(hinge.z),
        A::Dimension { a, b, .. } => out.extend([a.z, b.z]),
    }
}

/// Where drawings of `after` may differ from drawings of `before`.
pub fn changes(before: &Model, after: &Model) -> Dirty {
    let Model {
        next_id: _,
        vertices,
        edges,
        faces,
        groups,
        materials,
        active_group: _,
        tags,
        environment,
        tag_colors,
        definitions,
        instances,
        guides: _,
        photo_points: _,
        cages: _,
        photo_edges: _,
        photo_guides: _,
        views,
        timeline: _,
        annotations,
        units,
        classes,
        location,
        clouds,
        sections,
        active_section,
        axes: _,
        drafting,
        sheets: _,
        sheet_items: _,
        title_blocks: _,
        deadline: _,
        remaps: _,
    } = before;
    // (cages are drawn through the faces they make; guides, photo points, the timeline and the axes are never
    // drawn on paper; sheets and their items are drawn per page, and a viewport's own settings are its cache key)
    let mut d = Dirty::default();
    if *units != after.units || drafting.line_types != after.drafting.line_types || drafting.extra != after.drafting.extra {
        return Dirty::everything();
    }
    d.raster = *materials != after.materials || *environment != after.environment || *tag_colors != after.tag_colors || !json_eq(location, &after.location) || !json_eq(clouds, &after.clouds) || !json_eq(sections, &after.sections) || *active_section != after.active_section;
    d.views = !json_eq(views, &after.views);
    if !json_eq(annotations, &after.annotations) {
        d.raster = true;
        let (b, a) = (annotations, &after.annotations);
        for i in 0..b.len().max(a.len()) {
            let (x, y) = (b.get(i), a.get(i));
            if x.zip(y).is_some_and(|(x, y)| json_eq(x, y)) {
                continue;
            }
            for n in [x, y].into_iter().flatten() {
                note_heights(n, &mut d.notes_z);
            }
        }
    }

    // tags whose look changed: visibility, line and cut styles (the whole style, so a new field is covered too)
    let mut tagset = names_changed(tags, &after.tags, |x, y| x == y);
    tagset.extend(names_changed(&drafting.tag_styles, &after.drafting.tag_styles, |x, y| x == y));
    let pats = names_changed(&drafting.patterns, &after.drafting.patterns, |x, y| x == y);
    if !pats.is_empty() {
        let lower: HashSet<String> = pats.iter().map(|p| p.to_ascii_lowercase()).collect();
        for dr in [drafting, &after.drafting] {
            for (t, s) in &dr.tag_styles {
                if let crate::drafting::Keep::Known(s) = s {
                    let uses = s.cut.hatch.as_ref().into_iter().chain(s.surface_hatch.as_ref()).any(|h| lower.contains(&h.pattern.to_ascii_lowercase()));
                    if uses {
                        tagset.insert(t.clone());
                    }
                }
            }
        }
    }

    let vs = keys_changed(vertices, &after.vertices, |x, y| x == y);
    let es = keys_changed(edges, &after.edges, |x, y| x == y);
    let fs = keys_changed(faces, &after.faces, |x, y| x == y);
    let gs = keys_changed(groups, &after.groups, |x, y| x == y);
    let is = keys_changed(instances, &after.instances, |x, y| x == y);
    let ds = keys_changed(definitions, &after.definitions, |x, y| std::sync::Arc::ptr_eq(&x.model, &y.model) && x.name == y.name && x.attrs == y.attrs && x.face_me == y.face_me);
    let cls: Vec<Id> = keys_changed(classes, &after.classes, |x, y| x == y);
    if vs.len() + es.len() + fs.len() + gs.len() + is.len() + cls.len() > MAX_PRECISE {
        return Dirty::everything();
    }
    // cut priorities: any change to one, or any change at all while one is set, redraws every cut
    let priorities = |dr: &crate::drafting::Drafting| -> Vec<(String, i32)> {
        dr.tag_styles.iter().filter_map(|(t, s)| match s {
            crate::drafting::Keep::Known(s) if s.cut.priority != 0 => Some((t.clone(), s.cut.priority)),
            _ => None,
        }).collect()
    };
    let (pb, pa) = (priorities(drafting), priorities(&after.drafting));
    if pb != pa {
        d.cuts = true;
    }
    let nothing = vs.is_empty() && es.is_empty() && fs.is_empty() && gs.is_empty() && is.is_empty() && ds.is_empty() && cls.is_empty() && tagset.is_empty();
    if nothing {
        return d;
    }
    let mut changed_defs: HashSet<Id> = ds.iter().copied().collect();
    let vset: HashSet<Id> = vs.iter().copied().collect();
    let mut styled: HashSet<&str> = HashSet::new();
    for dr in [drafting, &after.drafting] {
        styled.extend(dr.tag_styles.keys().map(String::as_str));
    }
    let mut boxes = Vec::new();
    for m in [before, after] {
        let ix = Index::new(m);
        let mut mine = Vec::new();
        if !vset.is_empty() {
            for e in m.edges.values().filter(|e| vset.contains(&e.a) || vset.contains(&e.b)) {
                ix.in_group(e.group, &mut mine);
            }
            for f in m.faces.values().filter(|f| f.loops().flatten().any(|v| vset.contains(v))) {
                ix.in_group(f.group, &mut mine);
            }
        }
        for e in &es {
            if let Some(e) = m.edges.get(e) {
                ix.in_group(e.group, &mut mine);
            }
        }
        for f in &fs {
            if let Some(f) = m.faces.get(f) {
                ix.in_group(f.group, &mut mine);
            }
        }
        for g in gs.iter().chain(cls.iter()) {
            ix.group(*g, &mut mine);
            if m.instances.contains_key(g) {
                ix.placement(*g, &mut mine);
            }
        }
        for i in &is {
            ix.placement(*i, &mut mine);
        }
        if !changed_defs.is_empty() {
            grow_defs(m, &mut changed_defs);
            for (id, i) in &m.instances {
                if changed_defs.contains(&i.def) {
                    ix.placement(*id, &mut mine);
                }
            }
        }
        ix.tagged(&tagset, &mut mine);
        mine.retain(|b| !b.is_empty());
        // owners with a cut style that overlap what changed (regions of different tags meet there)
        if !styled.is_empty() && !mine.is_empty() {
            for o in ix.styled_owners(&styled) {
                if mine.iter().any(|b| b.overlaps_inside(&o, 1e-3)) {
                    mine.push(o);
                }
            }
        }
        boxes.extend(mine);
        d.stairs.extend(ix.stairs.borrow().iter().copied().filter(|b| !b.is_empty()));
    }
    // the same box from both models (or twice from one) is checked once
    boxes.sort_by(|a, b| a.lo.to_array().partial_cmp(&b.lo.to_array()).unwrap_or(std::cmp::Ordering::Equal).then(a.hi.to_array().partial_cmp(&b.hi.to_array()).unwrap_or(std::cmp::Ordering::Equal)));
    boxes.dedup();
    if !pa.is_empty() && !boxes.is_empty() {
        d.cuts = true;
    }
    d.boxes = boxes;
    d
}
