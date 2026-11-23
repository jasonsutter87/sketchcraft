//! Fix model: look at how solid a model is, and repair it in one step. See docs/dev/fix-model.md.
//!
//! Two entry points, both on `Model`:
//!  - `solid_check(&scope)`: read-only. Lists what keeps the geometry from being a solid (open edges, edges shared by
//!    three or more faces, faces that disagree about which way is out, shells turned inside out, loose lines).
//!    `is_solid(&scope)` is its yes/no.
//!  - `fix_model(&scope, &opts)`: repairs what can be repaired safely and returns a `FixReport` listing every change
//!    (ids and places). It changes the model in place; the engine command wraps it in one undo step.
//!
//! Everything works on shared vertex ids, so a "group" is naturally its own world (points never weld across groups).

use crate::geom;
use crate::model::{pairs, Face, Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Planar tolerance for filling a hole with one face (m).
const PLANAR: f64 = 1e-5;
/// Two faces are one flat surface when their normals agree this closely and they lie in each other's plane (m).
const COPLANAR_DOT: f64 = 1.0 - 1e-9;
const COPLANAR_DIST: f64 = 1e-6;
/// A hole with more edges than this is reported, not filled.
const MAX_HOLE_EDGES: usize = 512;
/// Past this many faces in scope the coplanar merge is skipped (a note says so).
const MAX_MERGE_FACES: usize = 300_000;
const MAX_MERGE_PASSES: usize = 64;

/// What to look at: everything, or only some groups (each with everything nested in it).
#[derive(Clone, Debug, Default)]
pub struct FixScope {
    /// `None`: every face and edge. `Some(set)`: only geometry whose group is in the set (`None` in it is the top level).
    pub groups: Option<BTreeSet<Option<Id>>>,
}

impl FixScope {
    pub fn all() -> FixScope {
        FixScope { groups: None }
    }
    fn has(&self, g: Option<Id>) -> bool {
        self.groups.as_ref().is_none_or(|s| s.contains(&g))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FixOptions {
    /// remove loose lines that hang off the faces (spurs)
    pub stray_edges: bool,
}

impl Default for FixOptions {
    fn default() -> Self {
        FixOptions { stray_edges: true }
    }
}

/// A place in the model: an id (an edge or a face) and where it is.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Spot {
    pub id: Id,
    pub at: [f64; 3],
}

/// One change: the face (or edge) it concerns, where, and the other faces involved (the faces merged away, the pieces a
/// hole was filled with).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Change {
    pub id: Id,
    pub at: [f64; 3],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<Id>,
}

/// How solid the geometry in scope is.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct SolidCheck {
    /// true when there is geometry and none of the problems below
    pub solid: bool,
    pub faces: usize,
    /// edges used by only one face: the model has a hole there
    pub open_edges: Vec<Spot>,
    /// edges used by three or more faces
    pub non_manifold_edges: Vec<Spot>,
    /// edges whose two faces run it the same way: one of them is reversed
    pub inconsistent_edges: Vec<Spot>,
    /// closed shells that face inward (the faces' fronts point into the solid)
    pub inward_shells: Vec<Spot>,
    /// faces that repeat another face (same corners)
    pub duplicate_faces: Vec<Spot>,
    /// loose lines no face uses
    pub loose_edges: Vec<Spot>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct FixReport {
    pub dry_run: bool,
    /// faces that repeated another face (the extra copies were removed)
    pub duplicate_faces: Vec<Change>,
    /// faces inside the solid, where every edge already had two other faces (removed)
    pub internal_faces: Vec<Change>,
    /// faces hanging off an edge that three or more faces shared (removed)
    pub non_manifold_fins: Vec<Change>,
    /// faces turned around (to agree with their neighbours, or so the solid faces out)
    pub reversed_faces: Vec<Change>,
    /// flat neighbours joined into one face (`id` is the face kept, `with` the faces merged into it)
    pub merged_faces: Vec<Change>,
    /// holes closed (`id` is the first new face, `with` any other new faces of that hole)
    pub filled_holes: Vec<Change>,
    /// loose lines removed
    pub stray_edges: Vec<Change>,
    /// things that could not be repaired automatically, in words
    pub notes: Vec<String>,
    /// what is still wrong afterwards (for a dry run: what is wrong now)
    pub remaining: SolidCheck,
}

impl FixReport {
    /// Total number of things changed.
    pub fn changes(&self) -> usize {
        self.duplicate_faces.len()
            + self.internal_faces.len()
            + self.non_manifold_fins.len()
            + self.reversed_faces.len()
            + self.merged_faces.len()
            + self.filled_holes.len()
            + self.stray_edges.len()
    }
}

type Key = (Id, Id);

fn key(a: Id, b: Id) -> Key {
    (a.min(b), a.max(b))
}

/// One use of an edge by a face loop: which face, and whether it runs the edge from the lower id to the higher.
#[derive(Clone, Copy)]
struct Use {
    face: Id,
    forward: bool,
}

fn spot(p: DVec3, id: Id) -> Spot {
    Spot { id, at: p.to_array() }
}

impl Model {
    /// The scope for these ids: groups (with everything nested in them) or faces (their groups). No ids: everything.
    pub fn fix_scope(&self, ids: &[Id]) -> Result<FixScope> {
        if ids.is_empty() {
            return Ok(FixScope::all());
        }
        let mut set = BTreeSet::new();
        for &id in ids {
            if self.groups.contains_key(&id) {
                set.extend(self.subtree(id).into_iter().map(Some));
            } else if let Some(f) = self.faces.get(&id) {
                set.insert(f.group);
            } else if let Some(e) = self.edges.get(&id) {
                set.insert(e.group);
            } else if self.instances.contains_key(&id) {
                bail!("{id} is a placed component: open it for editing (edit) and run this inside, or run it on the whole model");
            } else {
                bail!("{id} is not a group, face or edge");
            }
        }
        Ok(FixScope { groups: Some(set) })
    }

    /// Faces in scope (locked groups are left alone).
    fn faces_in(&self, scope: &FixScope) -> BTreeSet<Id> {
        let mut locked: HashMap<Option<Id>, bool> = HashMap::new();
        self.faces
            .iter()
            .filter(|(_, f)| scope.has(f.group))
            .filter(|(_, f)| !*locked.entry(f.group).or_insert_with(|| self.locked_by(f.group).is_some()))
            .map(|(&id, _)| id)
            .collect()
    }

    fn edge_uses(&self, faces: &BTreeSet<Id>) -> BTreeMap<Key, Vec<Use>> {
        let mut uses: BTreeMap<Key, Vec<Use>> = BTreeMap::new();
        for &f in faces {
            for l in self.faces[&f].loops() {
                for (a, b) in pairs(l) {
                    if a != b {
                        uses.entry(key(a, b)).or_default().push(Use { face: f, forward: a < b });
                    }
                }
            }
        }
        uses
    }

    fn edge_ids(&self) -> HashMap<Key, Id> {
        self.edges.iter().map(|(&id, e)| (key(e.a, e.b), id)).collect()
    }

    fn mid(&self, k: Key) -> DVec3 {
        (self.p(k.0) + self.p(k.1)) / 2.0
    }

    fn face_spot(&self, f: Id) -> [f64; 3] {
        self.center(f).to_array()
    }

    /// Faces that share a corner list: groups of ids whose sorted outer loops are the same.
    fn duplicate_groups(&self, faces: &BTreeSet<Id>) -> Vec<Vec<Id>> {
        let mut by: BTreeMap<(Option<Id>, Vec<Id>, usize), Vec<Id>> = BTreeMap::new();
        for &f in faces {
            let face = &self.faces[&f];
            let mut s = face.outer.clone();
            s.sort_unstable();
            by.entry((face.group, s, face.holes.len())).or_default().push(f);
        }
        by.into_values().filter(|v| v.len() > 1).collect()
    }

    // ----- the check ----------------------------------------------------------------------------------------------

    /// How solid the geometry in `scope` is, and where it isn't. Read-only.
    pub fn solid_check(&self, scope: &FixScope) -> SolidCheck {
        let faces = self.faces_in(scope);
        let mut out = SolidCheck { faces: faces.len(), ..Default::default() };
        let uses = self.edge_uses(&faces);
        let eid = self.edge_ids();
        for (k, u) in &uses {
            let id = eid.get(k).copied().unwrap_or(0);
            match u.len() {
                1 => out.open_edges.push(spot(self.mid(*k), id)),
                2 => {
                    if u[0].forward == u[1].forward {
                        out.inconsistent_edges.push(spot(self.mid(*k), id));
                    }
                }
                _ => out.non_manifold_edges.push(spot(self.mid(*k), id)),
            }
        }
        for g in self.duplicate_groups(&faces) {
            for &f in &g[1..] {
                out.duplicate_faces.push(Spot { id: f, at: self.face_spot(f) });
            }
        }
        // shells that face inward
        let comp = components(&self.faces, &uses, &faces);
        for shell in comp.values_by_shell() {
            if self.shell_closed(&shell, &uses) && self.shell_consistent(&shell, &uses) {
                let v = self.shell_volume(&shell, &BTreeSet::new());
                if v < -1e-12 && !self.shell_nested(&shell, &comp, &uses) {
                    out.inward_shells.push(Spot { id: shell[0], at: self.face_spot(shell[0]) });
                }
            }
        }
        // loose lines
        let used: HashSet<Key> = self.faces.values().flat_map(|f| f.loops().flat_map(|l| pairs(l).map(|(a, b)| key(a, b)).collect::<Vec<_>>()).collect::<Vec<_>>()).collect();
        for (&id, e) in &self.edges {
            if scope.has(e.group) && !used.contains(&key(e.a, e.b)) {
                out.loose_edges.push(spot(self.mid(key(e.a, e.b)), id));
            }
        }
        out.solid = !faces.is_empty()
            && out.open_edges.is_empty()
            && out.non_manifold_edges.is_empty()
            && out.inconsistent_edges.is_empty()
            && out.inward_shells.is_empty()
            && out.duplicate_faces.is_empty();
        out
    }

    /// True when the geometry in `scope` is closed, consistently facing out, with no edge shared by more than two faces.
    /// (Loose lines do not count against it.)
    pub fn is_solid(&self, scope: &FixScope) -> bool {
        self.solid_check(scope).solid
    }

    fn shell_closed(&self, shell: &[Id], uses: &BTreeMap<Key, Vec<Use>>) -> bool {
        shell.iter().all(|f| self.faces[f].loops().all(|l| pairs(l).all(|(a, b)| a == b || uses.get(&key(a, b)).is_some_and(|u| u.len() == 2))))
    }

    fn shell_consistent(&self, shell: &[Id], uses: &BTreeMap<Key, Vec<Use>>) -> bool {
        shell.iter().all(|f| self.faces[f].loops().all(|l| pairs(l).all(|(a, b)| a == b || uses.get(&key(a, b)).is_none_or(|u| u.len() != 2 || u[0].forward != u[1].forward))))
    }

    /// Signed volume of a shell with the faces in `flipped` turned around (negative: inside out). Exact for flat faces.
    fn shell_volume(&self, shell: &[Id], flipped: &BTreeSet<Id>) -> f64 {
        shell
            .iter()
            .map(|&f| {
                let face = &self.faces[&f];
                let mut a = geom::newell_raw(&self.pts(&face.outer));
                for h in &face.holes {
                    a += geom::newell_raw(&self.pts(h));
                }
                let v = self.p(face.outer[0]).dot(a) / 6.0;
                if flipped.contains(&f) {
                    -v
                } else {
                    v
                }
            })
            .sum()
    }

    /// A closed shell inside the bounding box of another closed shell with positive volume is taken to be a cavity.
    fn shell_nested(&self, shell: &[Id], comp: &Components, uses: &BTreeMap<Key, Vec<Use>>) -> bool {
        let (lo, hi) = self.shell_bounds(shell);
        let me = comp.of.get(&shell[0]).copied();
        comp.values_by_shell().iter().any(|other| {
            let id = comp.of.get(&other[0]).copied();
            if id == me || self.faces[&other[0]].group != self.faces[&shell[0]].group || !self.shell_closed(other, uses) {
                return false;
            }
            let (olo, ohi) = self.shell_bounds(other);
            olo.cmplt(lo).all() && ohi.cmpgt(hi).all()
        })
    }

    fn shell_bounds(&self, shell: &[Id]) -> (DVec3, DVec3) {
        let first = self.p(self.faces[&shell[0]].outer[0]);
        shell.iter().flat_map(|f| self.faces[f].outer.iter()).fold((first, first), |(lo, hi), &v| (lo.min(self.p(v)), hi.max(self.p(v))))
    }

    // ----- the fix ------------------------------------------------------------------------------------------------

    /// Repair the geometry in `scope`, in this order: repeated faces, faces inside the solid, fins on shared edges,
    /// reversed faces, flat neighbours joined, holes filled, shells turned outward, stray lines. Returns what changed.
    /// Nothing here can fail halfway: each step is a whole, valid edit.
    pub fn fix_model(&mut self, scope: &FixScope, opts: &FixOptions) -> FixReport {
        let mut r = FixReport::default();
        let locked = self.faces.values().filter(|f| scope.has(f.group) && self.locked_by(f.group).is_some()).count();
        if locked > 0 {
            r.notes.push(format!("{locked} face{} in locked groups {} left alone", if locked == 1 { "" } else { "s" }, if locked == 1 { "was" } else { "were" }));
        }
        self.fix_duplicates(scope, &mut r);
        self.fix_internal(scope, &mut r);
        self.fix_fins(scope, &mut r);
        let mut flips = BTreeMap::new();
        self.fix_orientation(scope, &mut flips);
        self.fix_merge_coplanar(scope, &mut r);
        self.fix_holes(scope, &mut r);
        self.fix_orientation(scope, &mut flips);
        // (a face flipped twice is back as it was, so it is not a change)
        r.reversed_faces = flips.into_iter().filter(|(_, (n, _))| n % 2 == 1).map(|(id, (_, at))| Change { id, at, with: vec![] }).collect();
        if opts.stray_edges {
            self.fix_stray(scope, &mut r);
        }
        if r.changes() > 0 {
            self.gc();
        }
        r.remaining = self.solid_check(scope);
        if !r.remaining.non_manifold_edges.is_empty() {
            r.notes.push(format!("{} edge{} still shared by three or more faces: it needs a hand (split the shapes apart, or delete the extra faces)", r.remaining.non_manifold_edges.len(), if r.remaining.non_manifold_edges.len() == 1 { " is" } else { "s are" }));
        }
        if !r.remaining.open_edges.is_empty() {
            r.notes.push(format!("{} open edge{} left (a hole too big or too ragged to close, or a flat sheet that is meant to be open)", r.remaining.open_edges.len(), if r.remaining.open_edges.len() == 1 { "" } else { "s" }));
        }
        r
    }

    fn remove_face(&mut self, f: Id) {
        self.faces.remove(&f);
    }

    /// Copies of the same face: the lowest id stays. Back-to-back pairs where both sides are covered by other faces are
    /// internal and both go.
    fn fix_duplicates(&mut self, scope: &FixScope, r: &mut FixReport) {
        let faces = self.faces_in(scope);
        for group in self.duplicate_groups(&faces) {
            let canon = |m: &Model, f: Id| canonical(&m.faces[&f].outer);
            let first = group[0];
            let mut seen: Vec<Vec<Id>> = vec![canon(self, first)];
            let mut survivors = vec![first];
            for &f in &group[1..] {
                let c = canon(self, f);
                if seen.contains(&c) {
                    r.duplicate_faces.push(Change { id: f, at: self.face_spot(f), with: vec![first] });
                    self.remove_face(f);
                } else {
                    seen.push(c);
                    survivors.push(f);
                }
            }
            // an opposite pair is the wall between two solids when two other faces already close every one of its edges;
            // otherwise it is one face twice, and the second goes
            if survivors.len() >= 2 {
                let rest: BTreeSet<Id> = self.faces_in(scope).into_iter().filter(|f| !survivors.contains(f)).collect();
                let uses = self.edge_uses(&rest);
                let outer = self.faces[&survivors[0]].outer.clone();
                if outer.len() >= 3 && pairs(&outer).all(|(a, b)| uses.get(&key(a, b)).is_some_and(|u| u.len() >= 2)) {
                    for f in survivors {
                        r.internal_faces.push(Change { id: f, at: self.face_spot(f), with: vec![] });
                        self.remove_face(f);
                    }
                } else {
                    for &f in &survivors[1..] {
                        r.duplicate_faces.push(Change { id: f, at: self.face_spot(f), with: vec![first] });
                        self.remove_face(f);
                    }
                }
            }
        }
    }

    /// A face whose every edge has two other faces is inside the solid.
    fn fix_internal(&mut self, scope: &FixScope, r: &mut FixReport) {
        let faces = self.faces_in(scope);
        let uses = self.edge_uses(&faces);
        let mut count: HashMap<Key, usize> = uses.iter().map(|(k, u)| (*k, u.len())).collect();
        for &f in &faces {
            let outer = self.faces[&f].outer.clone();
            if outer.len() >= 3 && pairs(&outer).all(|(a, b)| count.get(&key(a, b)).copied().unwrap_or(0) >= 3) {
                r.internal_faces.push(Change { id: f, at: self.face_spot(f), with: vec![] });
                for l in self.faces[&f].loops() {
                    for (a, b) in pairs(l) {
                        if let Some(c) = count.get_mut(&key(a, b)) {
                            *c -= 1;
                        }
                    }
                }
                self.remove_face(f);
            }
        }
    }

    /// An edge shared by three or more faces: the faces that are open on their other sides are fins; they go.
    fn fix_fins(&mut self, scope: &FixScope, r: &mut FixReport) {
        let faces = self.faces_in(scope);
        let uses = self.edge_uses(&faces);
        let mut count: HashMap<Key, usize> = uses.iter().map(|(k, u)| (*k, u.len())).collect();
        for (k, u) in &uses {
            if count[k] <= 2 {
                continue;
            }
            let mut cand: Vec<(usize, Id)> = u
                .iter()
                .filter(|x| self.faces.contains_key(&x.face))
                .map(|x| {
                    let free = self.faces[&x.face].loops().flat_map(|l| pairs(l)).filter(|&(a, b)| a != b && count.get(&key(a, b)).copied() == Some(1)).count();
                    (free, x.face)
                })
                .filter(|(free, _)| *free > 0)
                .collect();
            cand.sort_by(|a, b| b.cmp(a));
            for (_, f) in cand {
                if count[k] <= 2 {
                    break;
                }
                if !self.faces.contains_key(&f) {
                    continue;
                }
                r.non_manifold_fins.push(Change { id: f, at: self.face_spot(f), with: vec![] });
                for l in self.faces[&f].loops() {
                    for (a, b) in pairs(l) {
                        if let Some(c) = count.get_mut(&key(a, b)) {
                            *c = c.saturating_sub(1);
                        }
                    }
                }
                self.remove_face(f);
            }
        }
    }

    /// Make faces agree about which side is out. `flips` counts how often each face was turned (and where it is).
    fn fix_orientation(&mut self, scope: &FixScope, flips: &mut BTreeMap<Id, (usize, [f64; 3])>) {
        let faces = self.faces_in(scope);
        let uses = self.edge_uses(&faces);
        let comp = components(&self.faces, &uses, &faces);
        for shell in comp.values_by_shell() {
            // which faces disagree with the first one (walking across edges used by exactly two faces)
            let mut flip: BTreeMap<Id, bool> = BTreeMap::new();
            flip.insert(shell[0], false);
            let mut queue = vec![shell[0]];
            while let Some(f) = queue.pop() {
                let mine = flip[&f];
                for l in self.faces[&f].loops() {
                    for (a, b) in pairs(l) {
                        if a == b {
                            continue;
                        }
                        let u = &uses[&key(a, b)];
                        if u.len() != 2 {
                            continue;
                        }
                        let (me, other) = if u[0].face == f { (u[0], u[1]) } else { (u[1], u[0]) };
                        if other.face == f {
                            continue;
                        }
                        // faces that agree run the shared edge in opposite ways
                        let want = if me.forward == other.forward { !mine } else { mine };
                        if let std::collections::btree_map::Entry::Vacant(e) = flip.entry(other.face) {
                            e.insert(want);
                            queue.push(other.face);
                        }
                    }
                }
            }
            let mut set: BTreeSet<Id> = flip.iter().filter(|(_, &v)| v).map(|(&k, _)| k).collect();
            let all: BTreeSet<Id> = shell.iter().copied().collect();
            let closed = self.shell_closed(&shell, &uses);
            let volume = self.shell_volume(&shell, &set);
            if closed && volume.abs() > 1e-12 {
                if volume < 0.0 && !self.shell_nested(&shell, &comp, &uses) {
                    set = all.difference(&set).copied().collect();
                }
            } else if set.len() * 2 > all.len() {
                // an open surface: turn the fewer faces
                set = all.difference(&set).copied().collect();
            }
            for f in set {
                let at = self.face_spot(f);
                self.faces.get_mut(&f).unwrap().reverse();
                let e = flips.entry(f).or_insert((0, at));
                e.0 += 1;
            }
        }
    }

    /// Join neighbours lying in one plane: same look, no holes, one shared edge.
    fn fix_merge_coplanar(&mut self, scope: &FixScope, r: &mut FixReport) {
        if self.faces_in(scope).len() > MAX_MERGE_FACES {
            r.notes.push("coplanar faces were not merged: the model is too big for that step".into());
            return;
        }
        let mut merged: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for _ in 0..MAX_MERGE_PASSES {
            let faces = self.faces_in(scope);
            let uses = self.edge_uses(&faces);
            let eid = self.edge_ids();
            let mut touched: HashSet<Id> = HashSet::new();
            let mut did = false;
            for (k, u) in &uses {
                if u.len() != 2 || u[0].forward == u[1].forward {
                    continue;
                }
                let (f, g) = (u[0].face, u[1].face);
                if f == g || touched.contains(&f) || touched.contains(&g) || !self.mergeable(f, g, *k, &eid) {
                    continue;
                }
                if self.merge_pair(f, g, *k, &eid) {
                    touched.insert(f);
                    touched.insert(g);
                    let mut gone = merged.remove(&g).unwrap_or_default();
                    gone.push(g);
                    merged.entry(f).or_default().extend(gone);
                    did = true;
                }
            }
            if !did {
                break;
            }
        }
        for (keep, with) in merged {
            if self.faces.contains_key(&keep) {
                r.merged_faces.push(Change { id: keep, at: self.face_spot(keep), with });
            }
        }
    }

    fn mergeable(&self, f: Id, g: Id, k: Key, eid: &HashMap<Key, Id>) -> bool {
        let (a, b) = (&self.faces[&f], &self.faces[&g]);
        if a.group != b.group || !a.holes.is_empty() || !b.holes.is_empty() || a.material != b.material || a.hidden != b.hidden {
            return false;
        }
        if a.tex.is_some() || b.tex.is_some() || a.uv.is_some() || b.uv.is_some() || a.uv2.is_some() || b.uv2.is_some() || a.vcol.is_some() || b.vcol.is_some() {
            return false;
        }
        if let Some(e) = eid.get(&k).and_then(|id| self.edges.get(id)) {
            if e.seam || e.draft.is_some() {
                return false;
            }
        }
        // exactly one edge in common (two would pinch the result)
        let shared = pairs(&a.outer).filter(|&(x, y)| b.outer.iter().zip(b.outer.iter().cycle().skip(1)).any(|(&p, &q)| (p == x && q == y) || (p == y && q == x))).count();
        if shared != 1 {
            return false;
        }
        let (na, nb) = (self.normal(f), self.normal(g));
        if na == DVec3::ZERO || na.dot(nb) < COPLANAR_DOT {
            return false;
        }
        let p0 = self.p(a.outer[0]);
        b.outer.iter().all(|&v| (self.p(v) - p0).dot(na).abs() < COPLANAR_DIST)
    }

    /// Join `g` into `f` across edge `k`. False when the result would not be a simple loop.
    fn merge_pair(&mut self, f: Id, g: Id, k: Key, eid: &HashMap<Key, Id>) -> bool {
        let fo = self.faces[&f].outer.clone();
        let go = self.faces[&g].outer.clone();
        let Some(i) = (0..fo.len()).find(|&i| key(fo[i], fo[(i + 1) % fo.len()]) == k) else { return false };
        let (a, b) = (fo[i], fo[(i + 1) % fo.len()]);
        let Some(j) = (0..go.len()).find(|&j| go[j] == b && go[(j + 1) % go.len()] == a) else { return false };
        // f from b round to a, then g from after a round to before b
        let mut out: Vec<Id> = (0..fo.len()).map(|n| fo[(i + 1 + n) % fo.len()]).collect();
        out.extend((2..go.len()).map(|n| go[(j + n) % go.len()]));
        let set: BTreeSet<Id> = out.iter().copied().collect();
        if set.len() != out.len() || out.len() < 3 {
            return false;
        }
        self.faces.get_mut(&f).unwrap().outer = out;
        self.faces.remove(&g);
        if let Some(e) = eid.get(&k) {
            self.edges.remove(e);
        }
        true
    }

    /// Close each hole with a face (or several, when the rim is not flat).
    fn fix_holes(&mut self, scope: &FixScope, r: &mut FixReport) {
        let faces = self.faces_in(scope);
        let uses = self.edge_uses(&faces);
        let comp = components(&self.faces, &uses, &faces);
        // the way a filler runs each open edge: against the face that already has it
        let mut next: BTreeMap<Id, Vec<(Id, Id)>> = BTreeMap::new(); // from -> [(to, face it borders)]
        for (k, u) in &uses {
            if u.len() != 1 {
                continue;
            }
            let (a, b) = if u[0].forward { (k.0, k.1) } else { (k.1, k.0) }; // the existing face runs a -> b
            next.entry(b).or_default().push((a, u[0].face));
        }
        let mut loops: Vec<(Vec<Id>, Id)> = Vec::new();
        let mut taken: HashSet<(Id, Id)> = HashSet::new();
        let starts: Vec<(Id, Id)> = next.iter().flat_map(|(&from, v)| v.iter().map(move |&(to, _)| (from, to))).collect();
        for (s_from, s_to) in starts {
            if taken.contains(&(s_from, s_to)) {
                continue;
            }
            taken.insert((s_from, s_to));
            let mut path = vec![s_from];
            let mut to = s_to;
            loop {
                if let Some(at) = path.iter().position(|&v| v == to) {
                    // came back to a corner already on the path: that stretch is one closed rim
                    let lp: Vec<Id> = path.split_off(at);
                    if lp.len() >= 3 {
                        let border = next[&lp[0]].iter().find(|(t, _)| *t == lp[1]).map(|x| x.1).unwrap_or(next[&lp[0]][0].1);
                        loops.push((lp, border));
                    }
                }
                path.push(to);
                match next.get(&to).and_then(|v| v.iter().find(|(t, _)| !taken.contains(&(to, *t)))) {
                    Some(&(t, _)) => {
                        taken.insert((to, t));
                        to = t;
                    }
                    None => break,
                }
            }
        }
        let mut open = 0usize;
        for (lp, border) in loops {
            if lp.len() > MAX_HOLE_EDGES {
                open += 1;
                continue;
            }
            // a flat sheet is not a solid with a hole: filling its rim would make a zero-thickness double
            if !comp.of.contains_key(&border) || self.shell_is_flat(&comp, border) {
                continue;
            }
            let pts = self.pts(&lp);
            let n = geom::newell(&pts);
            if n == DVec3::ZERO {
                open += 1;
                continue;
            }
            let flat = pts.iter().all(|p| (*p - pts[0]).dot(n).abs() < PLANAR);
            let proto = self.faces[&border].clone();
            let group = self.vertices[&lp[0]].group;
            let mut made: Vec<Id> = Vec::new();
            let tris: Vec<Vec<Id>> = if flat {
                vec![lp.clone()]
            } else {
                let at: HashMap<[u64; 3], Id> = lp.iter().map(|&v| (self.p(v).to_array().map(f64::to_bits), v)).collect();
                let t = geom::triangulate(&pts, &[], n);
                let ids: Option<Vec<Vec<Id>>> = t.iter().map(|tri| tri.iter().map(|p| at.get(&p.to_array().map(f64::to_bits)).copied()).collect()).collect();
                match ids {
                    Some(v) if !v.is_empty() => v,
                    _ => (1..lp.len() - 1).map(|i| vec![lp[0], lp[i], lp[i + 1]]).collect(),
                }
            };
            for t in tris {
                for (a, b) in pairs(&t) {
                    self.edge(a, b);
                }
                let id = self.new_id();
                self.faces.insert(id, Face { outer: t, group, material: proto.material.clone(), hidden: proto.hidden, ..Default::default() });
                made.push(id);
            }
            if let Some((&first, rest)) = made.split_first() {
                r.filled_holes.push(Change { id: first, at: geom::centroid(&pts).to_array(), with: rest.to_vec() });
            }
        }
        if open > 0 {
            r.notes.push(format!("{open} hole{} too big to close automatically", if open == 1 { " was" } else { "s were" }));
        }
    }

    /// Every corner of the surface this face is part of lies in one plane.
    fn shell_is_flat(&self, comp: &Components, f: Id) -> bool {
        let Some(&c) = comp.of.get(&f) else { return true };
        let members: Vec<Id> = comp.of.iter().filter(|(_, &x)| x == c).map(|(&k, _)| k).collect();
        let mut origin = None;
        let mut normal = DVec3::ZERO;
        for &m in &members {
            let n = self.normal(m);
            if n != DVec3::ZERO {
                normal = n;
                origin = Some(self.p(self.faces[&m].outer[0]));
                break;
            }
        }
        let Some(o) = origin else { return true };
        members.iter().all(|m| self.faces[m].outer.iter().all(|&v| (self.p(v) - o).dot(normal).abs() < PLANAR))
    }

    /// Loose lines that touch the faces (spurs and dangling bits). Lines apart from every face are drawings: left alone.
    fn fix_stray(&mut self, scope: &FixScope, r: &mut FixReport) {
        let used: HashSet<Key> = self.faces.values().flat_map(|f| f.loops().flat_map(|l| pairs(l).map(|(a, b)| key(a, b)).collect::<Vec<_>>()).collect::<Vec<_>>()).collect();
        let face_verts: HashSet<Id> = self.faces.values().flat_map(|f| f.loops().flatten().copied().collect::<Vec<_>>()).collect();
        let loose: Vec<Id> = self.edges.iter().filter(|(_, e)| scope.has(e.group) && !used.contains(&key(e.a, e.b))).map(|(&id, _)| id).collect();
        // chains of loose lines, grown from the vertices the faces use
        let mut touching: HashSet<Id> = face_verts;
        let mut stray: BTreeSet<Id> = BTreeSet::new();
        loop {
            let before = stray.len();
            for &id in &loose {
                let e = &self.edges[&id];
                if !stray.contains(&id) && (touching.contains(&e.a) || touching.contains(&e.b)) {
                    stray.insert(id);
                    touching.insert(e.a);
                    touching.insert(e.b);
                }
            }
            if stray.len() == before {
                break;
            }
        }
        for id in stray {
            let e = self.edges.remove(&id).unwrap();
            r.stray_edges.push(Change { id, at: ((self.p(e.a) + self.p(e.b)) / 2.0).to_array(), with: vec![] });
        }
    }

    /// Fix the whole model: every group, and every component's own geometry too. Reports are merged; the places for a
    /// component's changes are in the component's own space and noted by name.
    pub fn fix_everything(&mut self, opts: &FixOptions) -> FixReport {
        let mut r = self.fix_model(&FixScope::all(), opts);
        let defs: Vec<Id> = self.definitions.keys().copied().collect();
        for d in defs {
            let name = self.definitions[&d].name.clone();
            let def = std::sync::Arc::make_mut(&mut self.definitions.get_mut(&d).unwrap().model);
            let inner = def.fix_model(&FixScope::all(), opts);
            if inner.changes() > 0 {
                r.notes.push(format!("component \"{name}\": {} change{} inside it (ids are the component's own)", inner.changes(), if inner.changes() == 1 { "" } else { "s" }));
                r.duplicate_faces.extend(inner.duplicate_faces);
                r.internal_faces.extend(inner.internal_faces);
                r.non_manifold_fins.extend(inner.non_manifold_fins);
                r.reversed_faces.extend(inner.reversed_faces);
                r.merged_faces.extend(inner.merged_faces);
                r.filled_holes.extend(inner.filled_holes);
                r.stray_edges.extend(inner.stray_edges);
            }
            r.notes.extend(inner.notes);
        }
        r
    }

    /// Whether the whole model, components included, is solid (the check a 3D-printing export wants).
    pub fn everything_is_solid(&self) -> bool {
        self.is_solid(&FixScope::all()) && self.definitions.values().all(|d| d.model.faces.is_empty() || d.model.is_solid(&FixScope::all()))
    }
}

/// A loop in a form that is the same however it is rotated (not reversed: direction matters).
fn canonical(l: &[Id]) -> Vec<Id> {
    let Some(i) = (0..l.len()).min_by_key(|&i| l[i]) else { return vec![] };
    (0..l.len()).map(|n| l[(i + n) % l.len()]).collect()
}

/// Faces grouped into surfaces: faces that share an edge used by exactly two faces.
struct Components {
    of: HashMap<Id, usize>,
}

impl Components {
    fn values_by_shell(&self) -> Vec<Vec<Id>> {
        let mut by: BTreeMap<usize, Vec<Id>> = BTreeMap::new();
        for (&f, &c) in &self.of {
            by.entry(c).or_default().push(f);
        }
        let mut v: Vec<Vec<Id>> = by.into_values().collect();
        for s in &mut v {
            s.sort_unstable();
        }
        v.sort();
        v
    }
}

fn components(_faces: &BTreeMap<Id, Face>, uses: &BTreeMap<Key, Vec<Use>>, set: &BTreeSet<Id>) -> Components {
    let mut parent: HashMap<Id, Id> = set.iter().map(|&f| (f, f)).collect();
    fn root(p: &mut HashMap<Id, Id>, mut f: Id) -> Id {
        while p[&f] != f {
            let g = p[&p[&f]];
            p.insert(f, g);
            f = g;
        }
        f
    }
    for u in uses.values() {
        if u.len() == 2 {
            let (a, b) = (root(&mut parent, u[0].face), root(&mut parent, u[1].face));
            if a != b {
                parent.insert(a.max(b), a.min(b));
            }
        }
    }
    let ids: Vec<Id> = set.iter().copied().collect();
    let mut of = HashMap::new();
    for f in ids {
        let r = root(&mut parent, f);
        of.insert(f, r as usize);
    }
    Components { of }
}
