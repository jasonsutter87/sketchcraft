//! Anchors (design step 5, `docs/dev/drafting-design.md` §2): paper items that stay attached to the model.
//!
//! A dimension on a sheet stores **vertex ids**, never points (`drafting::Anchor`). Every time it is drawn, asked
//! about, or settled after an edit, the ids are looked up in the live model, so moving a wall moves the dimension and
//! its value. This file holds:
//!
//! - **resolving** an anchor to a world point (`Model::resolve_anchor`): compose the instance transforms down the
//!   anchor's `path`, then look the vertex up there. `None` means broken.
//! - **measuring** a dimension (`Model::measure`): the value is the model distance projected onto the viewport's
//!   drawing plane, never paper length times scale.
//! - **remaps**: the few engine operations that move geometry to new ids (`make_component`, `explode`) or split an
//!   edge record a `Remap` in the transient `Model::remaps`; `Engine::exec` rewrites the anchors from them in the
//!   same undo step as the edit (`Model::settle_anchors`). Nothing else ever moves an anchor: geometry that is
//!   deleted, or replaced with new ids by an operation that records no remap, leaves the anchor **broken**, and only
//!   an explicit `sheet_reattach` (or the person) attaches it again (design decision 8).
//! - **model dimensions** (#59): an `Annotation::Dimension` may carry an anchor per end (SketchUp import attaches
//!   them). `settle_anchors` settles those the same way and moves the dimension's points to where they resolve.
//! - **making** anchors for other code (`Model::anchor_near`, `Model::anchors_at`): the one way to attach something
//!   to geometry from a point, used by `sheet_dim` and by importers (SketchUp dimensions, #59).
//!
//! Known limits: a component placement is resolved with its plain `transform` (Dynamic Component attributes that
//! stretch or copy a placement are not applied); `CurveCenter` fits a circle through three of the curve's vertices.

use crate::drafting::{Anchor, AnchorAt, DimMode, Dimension, Extra, ItemKind, SheetItem, ViewKind, Viewport};
use crate::model::{Annotation, Id, Model};
use glam::{DMat4, DVec3};
use std::collections::BTreeSet;
use std::sync::Arc;

/// How close (metres, 1 micrometre) a vertex must be to a broken anchor's last point to be offered as its new home.
pub const REATTACH_TOL: f64 = 1e-6;

/// Remaps kept for one command (a runaway importer can't grow the list without end).
const MAX_REMAPS: usize = 200_000;

/// Something an operation did to ids that anchors may need to follow. Recorded in `Model::remaps` by the operations
/// themselves, used once by `settle_anchors`, then dropped (never saved).
#[derive(Clone, Debug, PartialEq)]
pub enum Remap {
    /// Geometry moved to new ids: vertex `from` in the model at `from_path` is now vertex `to` in the model at
    /// `to_path` (paths are instance ids from the model the operation ran in). `scope`: the operation ran inside this
    /// component definition, so the paths start below any placement of it.
    Vertices { scope: Option<Id>, from_path: Vec<Id>, to_path: Vec<Id>, map: Vec<(Id, Id)> },
    /// An edge `a`-`b` was split by vertex `v`, which lies at `t` (0 at `a`, 1 at `b`).
    Split { a: Id, b: Id, v: Id, t: f64 },
    /// `group` gave the new group its own copy of every corner the selection shared with geometry outside it:
    /// `copies` is (original, copy); `inside` is every vertex now in the group (copies included, originals not);
    /// `nested` are the groups and placements moved into it as they were. A dimension follows the copies only by
    /// the rule in `Model::group_follow`.
    Group { copies: Vec<(Id, Id)>, inside: BTreeSet<Id>, nested: Vec<Id> },
}

impl Model {
    /// Note a remap, if anything on paper could use it (a model with no sheet items has no anchors, and a component
    /// being edited never has sheets of its own: `force` records for that case, to be lifted to the root).
    pub(crate) fn note_remap(&mut self, force: bool, r: impl FnOnce() -> Remap) {
        if (force || !self.sheet_items.is_empty() || self.has_attached_dims()) && self.remaps.len() < MAX_REMAPS {
            self.remaps.push(r());
        }
    }

    // ---- resolving ------------------------------------------------------------------------------------------

    /// The model at the end of `path` (instance ids from this one down), and the transform from there to the world.
    fn anchor_frame(&self, path: &[Id]) -> Option<(&Model, DMat4)> {
        let (mut m, mut t) = (self, DMat4::IDENTITY);
        for id in path {
            let inst = m.instances.get(id)?;
            let def = self.definitions.get(&inst.def).or_else(|| m.definitions.get(&inst.def))?;
            t *= inst.transform;
            m = &def.model;
        }
        Some((m, t))
    }

    /// Where an anchor is in the world now, or None when what it points at is gone (broken).
    pub fn resolve_anchor(&self, a: &Anchor) -> Option<DVec3> {
        let (m, t) = self.anchor_frame(&a.path)?;
        let local = match &a.at {
            AnchorAt::Vertex(v) => m.vertices.get(v)?.p,
            AnchorAt::Between { a: va, b: vb, t, .. } => {
                let (p, q) = (m.vertices.get(va)?.p, m.vertices.get(vb)?.p);
                p + (q - p) * *t
            }
            AnchorAt::Guide(g) => {
                let g = m.guides.iter().find(|x| x.id == *g)?;
                if g.dir.is_some() {
                    return None; // a guide line has no one point
                }
                g.p
            }
            AnchorAt::CurveCenter(c) => curve_center(m, *c)?,
        };
        let w = t.transform_point3(local);
        w.is_finite().then_some(w)
    }

    // ---- measuring -------------------------------------------------------------------------------------------

    /// A dimension's segments with their values in metres, measured in the plane `axes` describes (the viewport's
    /// paper right and up, as model directions); None: plan axes. A broken anchor makes its segments value-less.
    pub fn measure(&self, d: &Dimension, axes: Option<(DVec3, DVec3)>) -> Measure {
        let (right, up) = axes.unwrap_or((DVec3::X, DVec3::Y));
        let pts: Vec<Option<DVec3>> = d.anchors.iter().map(|a| self.resolve_anchor(a)).collect();
        let n = pts.len();
        let pairs: Vec<(usize, usize)> = match d.mode {
            DimMode::Baseline => (1..n).map(|i| (0, i)).collect(),
            _ => (1..n).map(|i| (i - 1, i)).collect(),
        };
        let segments = pairs
            .into_iter()
            .map(|(i, j)| {
                let value = match (pts[i], pts[j]) {
                    (Some(a), Some(b)) => {
                        let v = b - a;
                        Some(match d.mode {
                            DimMode::Horizontal => v.dot(right).abs(),
                            DimMode::Vertical => v.dot(up).abs(),
                            _ if axes.is_some() => v.dot(right).hypot(v.dot(up)),
                            _ => v.length(),
                        })
                    }
                    _ => None,
                };
                Segment { from: i, to: j, value }
            })
            .collect();
        Measure { points: pts, segments }
    }

    /// The anchors of `d` that no longer resolve (their indexes).
    pub fn broken_anchors(&self, d: &Dimension) -> Vec<usize> {
        d.anchors.iter().enumerate().filter(|(_, a)| self.resolve_anchor(a).is_none()).map(|(i, _)| i).collect()
    }

    // ---- making anchors --------------------------------------------------------------------------------------

    /// Every vertex within `tol` of the world point `p`, in the root model and inside component placements, as a
    /// ready `Vertex` anchor (nearest first). The one place anything turns a point into an attachment.
    pub fn anchors_at(&self, p: DVec3, tol: f64) -> Vec<Anchor> {
        let mut found: Vec<(f64, Anchor)> = Vec::new();
        self.walk_geometry(&mut |path, m, t, _| {
            for (id, v) in &m.vertices {
                let w = t.transform_point3(v.p);
                let d = w.distance(p);
                if d <= tol {
                    found.push((d, Anchor { path: path.to_vec(), at: AnchorAt::Vertex(*id), last: w.to_array(), extra: Extra::new() }));
                }
            }
        });
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, a)| a).collect()
    }

    /// The best anchor for a world point: a vertex within `tol`, else a point along an edge within `tol` (a
    /// `Between` over the edge's two vertices), else None (nothing is there to attach to).
    pub fn anchor_near(&self, p: DVec3, tol: f64) -> Option<Anchor> {
        if let Some(a) = self.anchors_at(p, tol).into_iter().next() {
            return Some(a);
        }
        let mut best: Option<(f64, Anchor)> = None;
        self.walk_geometry(&mut |path, m, t, _| {
            for e in m.edges.values() {
                let (Some(va), Some(vb)) = (m.vertices.get(&e.a), m.vertices.get(&e.b)) else { continue };
                let (a, b) = (t.transform_point3(va.p), t.transform_point3(vb.p));
                let ab = b - a;
                let len2 = ab.length_squared();
                if len2 < 1e-18 {
                    continue;
                }
                let s = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
                let d = (a + ab * s).distance(p);
                if d <= tol && best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                    best = Some((d, Anchor { path: path.to_vec(), at: AnchorAt::Between { a: e.a, b: e.b, t: s, extra: Extra::new() }, last: (a + ab * s).to_array(), extra: Extra::new() }));
                }
            }
        });
        best.map(|(_, a)| a)
    }

    /// Visit the root model and every component definition placed in it: `f(path, model, to_world, depth)`.
    fn walk_geometry(&self, f: &mut Visit) {
        fn go(root: &Model, m: &Model, path: &mut Vec<Id>, t: DMat4, depth: usize, budget: &mut usize, f: &mut Visit) {
            if depth > crate::nesting::MAX_NEST || *budget == 0 {
                return;
            }
            *budget -= 1;
            f(path, m, t, depth);
            for (id, inst) in &m.instances {
                if let Some(def) = root.definitions.get(&inst.def).or_else(|| m.definitions.get(&inst.def)) {
                    path.push(*id);
                    go(root, &def.model, path, t * inst.transform, depth + 1, budget, f);
                    path.pop();
                }
            }
        }
        let mut budget = 100_000;
        go(self, self, &mut Vec::new(), DMat4::IDENTITY, 0, &mut budget, f);
    }

    // ---- settling after an edit ------------------------------------------------------------------------------

    /// After a command: follow the remaps the operations recorded, then refresh each attached anchor's `last` point
    /// (so a later break shows where it was). Anchors that resolve nowhere are left exactly as they are. Runs inside
    /// the command's own undo step. Returns how many anchors changed.
    pub fn settle_anchors(&mut self) -> usize {
        let remaps = std::mem::take(&mut self.remaps);
        let mut changed = self.settle_model_dims(&remaps);
        if self.sheet_items.is_empty() {
            return changed;
        }
        let dims: Vec<(Id, Dimension)> = self
            .sheet_items
            .iter()
            .filter_map(|(id, i)| if let SheetItem::Known(ItemKind::Dimension(d)) = &**i { Some((*id, d.clone())) } else { None })
            .collect();
        for (id, mut d) in dims {
            if self.settle_list(&mut d.anchors, &remaps, true) {
                changed += 1;
                self.sheet_items.insert(id, Arc::new(SheetItem::Known(ItemKind::Dimension(d))));
            }
        }
        changed
    }

    /// One dimension's anchors after an edit: the `group` rule (when `group` is set), then the remaps, then each
    /// attached anchor's `last` point. True when anything changed.
    fn settle_list(&self, anchors: &mut Vec<Anchor>, remaps: &[Remap], group: bool) -> bool {
        let mut touched = false;
        // `group`: a dimension whose measured geometry all went into the group follows the group's copies, even
        // though the original corners still resolve
        if group {
            if let Some(n) = self.group_follow(anchors, remaps) {
                *anchors = n;
                touched = true;
            }
        }
        for a in anchors.iter_mut() {
            // an anchor that resolves is attached: the remaps are for those that don't
            if self.resolve_anchor(a).is_none() {
                if let Some(n) = self.remapped(a, remaps) {
                    *a = n;
                    touched = true;
                }
            } else {
                // a split that happened while it still resolves keeps a `Between` on the right sub-edge
                if let Some(n) = self.resplit(a, remaps) {
                    *a = n;
                    touched = true;
                }
            }
            if let Some(w) = self.resolve_anchor(a) {
                if w.distance(DVec3::from(a.last)) > 1e-9 {
                    a.last = w.to_array();
                    touched = true;
                }
            }
        }
        touched
    }

    /// Does any model-space dimension have an end attached to geometry? (Then remaps are worth recording.)
    pub fn has_attached_dims(&self) -> bool {
        self.annotations.iter().any(Annotation::is_attached)
    }

    /// The model's own attached dimensions (#59) after an edit: their anchors settle exactly as a sheet dimension's
    /// do (the `group` rule only when both ends are attached), and each attached end's point moves to where its
    /// anchor resolves now, so the drawn dimension and its value follow the geometry. An end whose anchor is broken
    /// keeps its last point. Returns how many dimensions changed.
    fn settle_model_dims(&mut self, remaps: &[Remap]) -> usize {
        if !self.has_attached_dims() {
            return 0;
        }
        let mut changed = 0;
        for i in 0..self.annotations.len() {
            let Annotation::Dimension { anchors, .. } = &self.annotations[i] else { continue };
            if anchors.iter().all(Option::is_none) {
                continue;
            }
            let both = anchors.iter().all(Option::is_some);
            let slots: Vec<usize> = (0..2).filter(|k| anchors[*k].is_some()).collect();
            let mut list: Vec<Anchor> = anchors.iter().flatten().cloned().collect();
            let mut touched = self.settle_list(&mut list, remaps, both);
            let points: Vec<Option<DVec3>> = list.iter().map(|a| self.resolve_anchor(a)).collect();
            let Annotation::Dimension { a, b, anchors, .. } = &mut self.annotations[i] else { continue };
            for ((k, anchor), p) in slots.into_iter().zip(list).zip(points) {
                if let Some(p) = p {
                    let end = if k == 0 { &mut *a } else { &mut *b };
                    if end.distance(p) > 1e-12 {
                        *end = p;
                        touched = true;
                    }
                }
                anchors[k] = Some(anchor);
            }
            if touched {
                changed += 1;
            }
        }
        changed
    }

    /// The rule for `group` (decided 2026-11-18): a dimension follows the group's copies of shared corners when
    /// every one of its anchors is in the group afterwards, and at least one of them is on a copied corner. An
    /// anchor is in the group when it is a vertex (or a `Between` over two vertices) that is now in the group, a
    /// corner the group copied, or a point inside a placement or group moved into it. If any anchor stays outside
    /// (a corner with other geometry, a guide, a curve centre, a loose edge, a placement left out), the whole
    /// dimension stays on the original corners: what it measured is not in the group. All or none, per dimension.
    fn group_follow(&self, list: &[Anchor], remaps: &[Remap]) -> Option<Vec<Anchor>> {
        let mut anchors = list.to_vec();
        let mut moved = false;
        for r in remaps {
            let Remap::Group { copies, inside, nested } = r else { continue };
            let copy_of = |v: &Id| copies.iter().find(|(o, _)| o == v).map(|(_, c)| *c);
            let mut next = Vec::with_capacity(anchors.len());
            let mut any = false;
            for a in &anchors {
                if !a.path.is_empty() {
                    if !nested.contains(&a.path[0]) {
                        return None;
                    }
                    next.push(a.clone());
                    continue;
                }
                let at = match &a.at {
                    AnchorAt::Vertex(v) => match copy_of(v) {
                        Some(c) => {
                            any = true;
                            AnchorAt::Vertex(c)
                        }
                        None if inside.contains(v) => a.at.clone(),
                        None => return None,
                    },
                    AnchorAt::Between { a: va, b: vb, t, extra } => {
                        let (na, nb) = (copy_of(va).unwrap_or(*va), copy_of(vb).unwrap_or(*vb));
                        if !(inside.contains(&na) && inside.contains(&nb)) {
                            return None;
                        }
                        any |= (na, nb) != (*va, *vb);
                        AnchorAt::Between { a: na, b: nb, t: *t, extra: extra.clone() }
                    }
                    _ => return None,
                };
                next.push(Anchor { path: Vec::new(), at, last: a.last, extra: a.extra.clone() });
            }
            if any && next.iter().all(|a| self.resolve_anchor(a).is_some()) {
                anchors = next;
                moved = true;
            }
        }
        moved.then_some(anchors)
    }

    /// An anchor that no longer resolves, moved by the remaps that name what it pointed at (all of it, or none).
    fn remapped(&self, a: &Anchor, remaps: &[Remap]) -> Option<Anchor> {
        let ids: Vec<Id> = match &a.at {
            AnchorAt::Vertex(v) => vec![*v],
            AnchorAt::Between { a, b, .. } => vec![*a, *b],
            _ => return None,
        };
        for r in remaps {
            let Remap::Vertices { scope, from_path, to_path, map } = r else { continue };
            // inside a component being edited the anchor's path starts with a placement of it
            let rest: &[Id] = match scope {
                None => &a.path,
                Some(def) => match a.path.split_first() {
                    Some((first, rest)) if self.instances.get(first).is_some_and(|i| i.def == *def) => rest,
                    _ => continue,
                },
            };
            if rest != from_path.as_slice() {
                continue;
            }
            let to: Option<Vec<Id>> = ids.iter().map(|id| map.iter().find(|(f, _)| f == id).map(|(_, t)| *t)).collect();
            let Some(to) = to else { continue };
            let mut path: Vec<Id> = if scope.is_some() { a.path[..1].to_vec() } else { Vec::new() };
            path.extend(to_path.iter().copied());
            let at = match &a.at {
                AnchorAt::Vertex(_) => AnchorAt::Vertex(to[0]),
                AnchorAt::Between { t, extra, .. } => AnchorAt::Between { a: to[0], b: to[1], t: *t, extra: extra.clone() },
                _ => return None,
            };
            let n = Anchor { path, at, last: a.last, extra: a.extra.clone() };
            if self.resolve_anchor(&n).is_some() {
                return Some(n);
            }
        }
        None
    }

    /// A `Between` over an edge that was split is re-expressed over the piece it falls on (or on the new vertex).
    fn resplit(&self, a: &Anchor, remaps: &[Remap]) -> Option<Anchor> {
        let AnchorAt::Between { a: va, b: vb, t, extra } = &a.at else { return None };
        if !a.path.is_empty() {
            return None;
        }
        let (mut va, mut vb, mut t) = (*va, *vb, *t);
        let mut moved = false;
        for r in remaps {
            let Remap::Split { a: sa, b: sb, v, t: tv } = r else { continue };
            let (fwd, rev) = ((*sa, *sb) == (va, vb), (*sb, *sa) == (va, vb));
            if !(fwd || rev) {
                continue;
            }
            // position along the split edge, in its own direction
            let s = if fwd { t } else { 1.0 - t };
            let (a2, b2, s2) = if (s - tv).abs() < 1e-9 {
                // exactly on the new vertex: attach to it
                return Some(Anchor { path: Vec::new(), at: AnchorAt::Vertex(*v), last: a.last, extra: a.extra.clone() });
            } else if s < *tv {
                (*sa, *v, s / tv)
            } else {
                (*v, *sb, (s - tv) / (1.0 - tv))
            };
            if fwd {
                (va, vb, t) = (a2, b2, s2);
            } else {
                (va, vb, t) = (b2, a2, 1.0 - s2);
            }
            moved = true;
        }
        moved.then(|| Anchor { path: Vec::new(), at: AnchorAt::Between { a: va, b: vb, t, extra: extra.clone() }, last: a.last, extra: a.extra.clone() })
    }
}

type Visit<'a> = dyn FnMut(&[Id], &Model, DMat4, usize) + 'a;

/// The result of `Model::measure`.
#[derive(Clone, Debug)]
pub struct Measure {
    /// where each anchor is now (None: broken)
    pub points: Vec<Option<DVec3>>,
    pub segments: Vec<Segment>,
}

/// One measured span of a dimension: anchors `from` and `to`, and its length in metres (None: one end is broken).
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub from: usize,
    pub to: usize,
    pub value: Option<f64>,
}

impl Measure {
    pub fn is_broken(&self) -> bool {
        self.points.iter().any(Option::is_none)
    }
}

/// The centre of a curve (arc or circle): the centre of the circle through three of its vertices.
fn curve_center(m: &Model, curve: Id) -> Option<DVec3> {
    let mut ids: Vec<Id> = m.edges.values().filter(|e| e.curve == Some(curve)).flat_map(|e| [e.a, e.b]).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 3 {
        return None;
    }
    let n = ids.len();
    let p = |i: usize| m.vertices.get(&ids[i]).map(|v| v.p);
    let (a, b, c) = (p(0)?, p(n / 3)?, p(2 * n / 3)?);
    let (ab, ac) = (b - a, c - a);
    let w = ab.cross(ac);
    let d = 2.0 * w.length_squared();
    if d < 1e-18 {
        return None;
    }
    Some(a + (w.cross(ab) * ac.length_squared() + ac.cross(w) * ab.length_squared()) / d)
}

// ---- viewports: the drawing plane --------------------------------------------------------------------------------

/// How a saved viewport maps the model onto its sheet, when it can (a perspective 3D view has no scale to measure
/// in). The same mapping the drawing uses (`draft::project`), so a dimension lands where the picture is.
pub fn viewport_proj(v: &Viewport) -> Option<crate::draft::hidden::Proj> {
    let look = match &v.view {
        ViewKind::Plan { .. } => DVec3::NEG_Z,
        ViewKind::Elevation { dir, .. } => -DVec3::from(*dir),
        ViewKind::Section { normal, .. } => DVec3::new(normal[0], normal[1], 0.0),
        ViewKind::View3d { eye, target, parallel: true, .. } => DVec3::from(*target) - DVec3::from(*eye),
        ViewKind::View3d { .. } => return None,
    };
    if look.length_squared() < 1e-12 || !(v.scale.is_finite() && v.scale > 0.0) {
        return None;
    }
    let c = [v.rect_mm[0] + v.rect_mm[2] / 2.0, v.rect_mm[1] + v.rect_mm[3] / 2.0];
    Some(crate::draft::hidden::Proj::new(DVec3::from(v.anchor), look, 1000.0 / v.scale, c))
}

impl Model {
    /// The viewport a dimension is measured and drawn in: the one it names, else the topmost on its sheet whose frame
    /// holds the dimension's attached points (else the topmost that can map the model at all).
    pub fn dimension_viewport(&self, d: &Dimension) -> Option<(Id, Viewport, crate::draft::hidden::Proj)> {
        let mut vps: Vec<(i32, Id, &Viewport)> = self
            .sheet_items
            .iter()
            .filter_map(|(id, i)| if let SheetItem::Known(ItemKind::Viewport(v)) = &**i { (v.sheet == d.sheet).then_some((v.z, *id, v)) } else { None })
            .collect();
        vps.sort_by_key(|a| std::cmp::Reverse((a.0, a.1)));
        if let Some(want) = d.viewport {
            let (_, id, v) = vps.into_iter().find(|(_, id, _)| *id == want)?;
            return viewport_proj(v).map(|p| (id, v.clone(), p));
        }
        let pts: Vec<DVec3> = d.anchors.iter().filter_map(|a| self.resolve_anchor(a)).collect();
        let usable: Vec<(Id, &Viewport, crate::draft::hidden::Proj)> = vps.into_iter().filter_map(|(_, id, v)| viewport_proj(v).map(|p| (id, v, p))).collect();
        let inside = |v: &Viewport, p: &crate::draft::hidden::Proj| {
            !pts.is_empty()
                && pts.iter().all(|w| {
                    let q = p.xy(*w);
                    q[0] >= v.rect_mm[0] && q[0] <= v.rect_mm[0] + v.rect_mm[2] && q[1] >= v.rect_mm[1] && q[1] <= v.rect_mm[1] + v.rect_mm[3]
                })
        };
        let pick = usable.iter().position(|(_, v, p)| inside(v, p)).or(if usable.is_empty() { None } else { Some(0) })?;
        let (id, v, p) = usable.into_iter().nth(pick)?;
        Some((id, v.clone(), p))
    }

    /// A dimension measured in its viewport's plane (plan axes when it has none).
    pub fn measure_dimension(&self, d: &Dimension) -> Measure {
        let axes = self.dimension_viewport(d).map(|(_, _, p)| (p.right, p.up));
        self.measure(d, axes)
    }
}
