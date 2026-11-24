//! Make faces: close every closed loop of loose lines with a face. See docs/dev/small-tools.md.
//!
//! Steps, per group (points never weld across groups):
//!  1. node: lines that cross, or end on the middle of another line, are cut there so loops can form;
//!  2. split the lines into connected pieces; each piece must lie in one plane (a bent piece is skipped and reported);
//!  3. pieces in the same plane are walked together as a planar graph: dangling ends are peeled away, every bounded
//!     region becomes a face, and a piece lying inside a region is a hole in the smallest region around it
//!     (`open_holes`: the region inside such a hole is left empty too, so a ring of nested loops alternates
//!     face, hole, face...).

use crate::fix::Spot;
use crate::geom;
use crate::model::{pairs, Face, Id, Model, WELD};
use anyhow::{bail, Result};
use glam::{DVec2, DVec3};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A piece of linework is flat when every point is within this of its plane (m).
const PLANAR: f64 = 1e-5;
/// More lines than this are not cut where they cross (the pairing is quadratic); a note says so.
const MAX_NODE_EDGES: usize = 3000;
/// A loop longer than this is not walked.
const MAX_LOOP: usize = 100_000;
/// Pairs of holes and faces tried before the rest are left alone.
const MAX_HOLE_TESTS: usize = 40_000_000;
const ON_EDGE: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Default)]
pub struct MakeFacesOptions {
    /// The region inside a hole is left empty too (a plate with a drilled hole: no disc in the hole).
    pub open_holes: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct MakeFacesReport {
    /// the faces made
    pub faces: Vec<Id>,
    pub created: usize,
    /// how many of the faces got one or more holes, and how many holes in all
    pub with_holes: usize,
    pub holes: usize,
    /// loops that already had a face (left alone)
    pub existing: usize,
    /// pieces of linework that are not flat, with a line from each (nothing was made from them)
    pub non_planar: Vec<Spot>,
    /// loops that touch themselves or cross over (skipped)
    pub degenerate: usize,
    /// lines that are not part of any closed loop
    pub open_edges: usize,
    /// lines cut where others cross or meet them
    pub split: usize,
    pub notes: Vec<String>,
}

fn key(a: Id, b: Id) -> (Id, Id) {
    if a < b { (a, b) } else { (b, a) }
}

/// A cycle of the planar walk.
struct Cycle {
    verts: Vec<Id>,
    poly: Vec<DVec2>,
    area: f64,
    comp: usize,
    lo: DVec2,
    hi: DVec2,
}

enum Fit {
    Flat(DVec3, f64),
    Collinear,
    Bent,
}

/// The plane through a set of points (normal turned to face up where it can, then +y, +x) or why there is none.
fn fit_plane(pts: &[DVec3]) -> Fit {
    let p0 = pts[0];
    let Some(p1) = pts.iter().copied().max_by(|a, b| a.distance_squared(p0).total_cmp(&b.distance_squared(p0))) else { return Fit::Collinear };
    if p1.distance(p0) < 1e-9 {
        return Fit::Collinear;
    }
    let dir = (p1 - p0).normalize();
    let off = |p: DVec3| (p - p0 - dir * (p - p0).dot(dir)).length();
    let Some(p2) = pts.iter().copied().max_by(|a, b| off(*a).total_cmp(&off(*b))) else { return Fit::Collinear };
    if off(p2) < PLANAR {
        return Fit::Collinear;
    }
    let mut n = (p1 - p0).cross(p2 - p0).normalize();
    if pts.iter().any(|p| (*p - p0).dot(n).abs() > PLANAR) {
        return Fit::Bent;
    }
    for c in [n.z, n.y, n.x] {
        if c.abs() > 1e-9 {
            if c < 0.0 {
                n = -n;
            }
            break;
        }
    }
    Fit::Flat(n, n.dot(p0))
}

fn on_interior(p: DVec3, a: DVec3, b: DVec3) -> bool {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 < 1e-18 {
        return false;
    }
    let t = (p - a).dot(ab) / l2;
    t > 1e-9 && t < 1.0 - 1e-9 && (a + ab * t).distance(p) < ON_EDGE
}

impl Model {
    /// The lines to make faces from: the named lines (and every loose line in the named groups), or every loose line in
    /// unlocked groups. A line is loose when no face uses it.
    pub fn make_faces_scope(&self, ids: &[Id]) -> Result<Vec<Id>> {
        let used: HashSet<(Id, Id)> = self.faces.values().flat_map(|f| f.loops().flat_map(|l| pairs(l).map(|(a, b)| key(a, b)))).collect();
        let loose = |e: &crate::model::Edge| e.a != e.b && !used.contains(&key(e.a, e.b));
        if ids.is_empty() {
            let mut locked: HashMap<Option<Id>, bool> = HashMap::new();
            return Ok(self.edges.iter().filter(|(_, e)| loose(e) && !*locked.entry(e.group).or_insert_with(|| self.locked_by(e.group).is_some())).map(|(&k, _)| k).collect());
        }
        self.check_unlocked(ids)?;
        let mut out = BTreeSet::new();
        for &id in ids {
            if self.edges.contains_key(&id) {
                out.insert(id);
            } else if self.groups.contains_key(&id) {
                let tree = self.subtree(id);
                out.extend(self.edges.iter().filter(|(_, e)| loose(e) && e.group.is_some_and(|g| tree.contains(&g))).map(|(&k, _)| k));
            } else {
                bail!("{id} is not a line or a group (make_faces takes lines, or groups to take their loose lines from)");
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Make faces from these lines. Changes the model in place (the engine command wraps it in one undo step).
    pub fn make_faces(&mut self, lines: &[Id], opts: &MakeFacesOptions) -> Result<MakeFacesReport> {
        let mut rep = MakeFacesReport::default();
        let mut by_group: BTreeMap<Option<Id>, Vec<Id>> = BTreeMap::new();
        for &id in lines {
            if let Some(e) = self.edges.get(&id) {
                if e.a != e.b {
                    by_group.entry(e.group).or_default().push(id);
                }
            }
        }
        for (group, edges) in by_group {
            self.check_deadline()?;
            let edges = self.node_lines(edges, &mut rep)?;
            self.faces_among(group, &edges, opts, &mut rep)?;
        }
        rep.created = rep.faces.len();
        Ok(rep)
    }

    /// Cut the lines where they cross or one ends on another's middle. Returns the lines now (some are new pieces).
    fn node_lines(&mut self, edges: Vec<Id>, rep: &mut MakeFacesReport) -> Result<Vec<Id>> {
        let n = edges.len();
        if n > MAX_NODE_EDGES {
            rep.notes.push(format!("{n} lines is too many to cut where they cross (the most is {MAX_NODE_EDGES}); only loops that already share points were found"));
            return Ok(edges);
        }
        let segs: Vec<(DVec3, DVec3)> = edges.iter().map(|id| (self.p(self.edges[id].a), self.p(self.edges[id].b))).collect();
        let boxes: Vec<(DVec3, DVec3)> = segs.iter().map(|(a, b)| (a.min(*b) - DVec3::splat(ON_EDGE), a.max(*b) + DVec3::splat(ON_EDGE))).collect();
        let mut cuts: Vec<Vec<DVec3>> = vec![vec![]; n];
        for i in 0..n {
            if i % 256 == 0 {
                self.check_deadline()?;
            }
            for j in i + 1..n {
                let (bi, bj) = (&boxes[i], &boxes[j]);
                if bi.1.cmplt(bj.0).any() || bj.1.cmplt(bi.0).any() {
                    continue;
                }
                let ((pi, qi), (pj, qj)) = (segs[i], segs[j]);
                if let Some((_, x)) = crate::topology::crossing(pi, qi, pj, qj) {
                    cuts[i].push(x);
                }
                if let Some((_, x)) = crate::topology::crossing(pj, qj, pi, qi) {
                    cuts[j].push(x);
                }
                for p in [pj, qj] {
                    if on_interior(p, pi, qi) {
                        cuts[i].push(p);
                    }
                }
                for p in [pi, qi] {
                    if on_interior(p, pj, qj) {
                        cuts[j].push(p);
                    }
                }
            }
        }
        let mut out: BTreeSet<Id> = edges.iter().copied().collect();
        for (i, id) in edges.iter().enumerate() {
            if cuts[i].is_empty() {
                continue;
            }
            let (a, b) = segs[i];
            let ab = b - a;
            let mut pts: Vec<(f64, DVec3)> = cuts[i].iter().map(|p| ((*p - a).dot(ab) / ab.length_squared(), *p)).collect();
            pts.sort_by(|x, y| x.0.total_cmp(&y.0));
            pts.dedup_by(|x, y| x.1.distance(y.1) < WELD);
            let curve = self.edges[id].curve;
            let mut cur = *id;
            out.remove(id);
            for (_, p) in pts {
                let Some(e) = self.edges.get(&cur).cloned() else { break };
                let v = self.split_edge(cur, p);
                if v == e.a || v == e.b {
                    out.insert(cur);
                    continue;
                }
                rep.split += 1;
                let (Some(first), Some(second)) = (self.find_edge(e.a, v), self.find_edge(v, e.b)) else { break };
                for piece in [first, second] {
                    if let Some(ed) = self.edges.get_mut(&piece) {
                        ed.curve = curve;
                    }
                }
                out.insert(first);
                out.remove(&second);
                cur = second;
            }
            out.insert(cur);
        }
        Ok(out.into_iter().filter(|e| self.edges.contains_key(e)).collect())
    }

    fn faces_among(&mut self, group: Option<Id>, edges: &[Id], opts: &MakeFacesOptions, rep: &mut MakeFacesReport) -> Result<()> {
        let pair_set: BTreeSet<(Id, Id)> = edges.iter().filter_map(|id| self.edges.get(id)).filter(|e| e.a != e.b).map(|e| key(e.a, e.b)).collect();
        // connected pieces
        let mut nb: HashMap<Id, Vec<Id>> = HashMap::new();
        for &(a, b) in &pair_set {
            nb.entry(a).or_default().push(b);
            nb.entry(b).or_default().push(a);
        }
        let mut comp_of: HashMap<Id, usize> = HashMap::new();
        let mut comps: Vec<Vec<Id>> = vec![];
        let mut starts: Vec<Id> = nb.keys().copied().collect();
        starts.sort();
        for s in starts {
            if comp_of.contains_key(&s) {
                continue;
            }
            let c = comps.len();
            let mut stack = vec![s];
            let mut members = vec![];
            comp_of.insert(s, c);
            while let Some(v) = stack.pop() {
                members.push(v);
                for &w in &nb[&v] {
                    if comp_of.insert(w, c).is_none() {
                        stack.push(w);
                    }
                }
            }
            comps.push(members);
        }
        // each piece's plane; pieces in the same plane go together
        let mut planes: Vec<(DVec3, f64, Vec<usize>)> = vec![];
        for (c, members) in comps.iter().enumerate() {
            let pts: Vec<DVec3> = members.iter().map(|v| self.p(*v)).collect();
            match fit_plane(&pts) {
                Fit::Collinear => rep.open_edges += members.iter().map(|v| nb[v].len()).sum::<usize>() / 2,
                Fit::Bent => {
                    let at = geom::centroid(&pts);
                    let id = members.iter().filter_map(|v| nb[v].first().and_then(|w| self.find_edge(*v, *w))).min().unwrap_or(0);
                    rep.non_planar.push(Spot { id, at: at.to_array() });
                }
                Fit::Flat(n, d) => match planes.iter_mut().find(|(n2, d2, _)| n.dot(*n2) > 1.0 - 1e-9 && (d - d2).abs() < PLANAR) {
                    Some((_, _, cs)) => cs.push(c),
                    None => planes.push((n, d, vec![c])),
                },
            }
        }
        // faces that exist already, by their corners
        let mut existing: HashSet<Vec<Id>> = HashSet::new();
        for f in self.faces.values().filter(|f| f.group == group) {
            let mut o = f.outer.clone();
            o.sort();
            existing.insert(o);
        }
        for (n, _, cs) in planes {
            self.check_deadline()?;
            let members: HashSet<Id> = cs.iter().flat_map(|c| comps[*c].iter().copied()).collect();
            let plane_pairs: Vec<(Id, Id)> = pair_set.iter().copied().filter(|(a, _)| members.contains(a)).collect();
            self.planar_faces(group, n, &plane_pairs, &comp_of, &existing, opts, rep)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn planar_faces(
        &mut self,
        group: Option<Id>,
        n: DVec3,
        pairs_in: &[(Id, Id)],
        comp_of: &HashMap<Id, usize>,
        existing: &HashSet<Vec<Id>>,
        opts: &MakeFacesOptions,
        rep: &mut MakeFacesReport,
    ) -> Result<()> {
        let (u, v) = geom::basis(n);
        let xy = |m: &Model, id: Id| {
            let p = m.p(id);
            DVec2::new(p.dot(u), p.dot(v))
        };
        let mut nb: HashMap<Id, Vec<Id>> = HashMap::new();
        for &(a, b) in pairs_in {
            nb.entry(a).or_default().push(b);
            nb.entry(b).or_default().push(a);
        }
        // peel the dangling ends: they can't bound a face
        let mut work: Vec<Id> = nb.iter().filter(|(_, l)| l.len() < 2).map(|(k, _)| *k).collect();
        while let Some(x) = work.pop() {
            if let Some(ns) = nb.remove(&x) {
                for w in ns {
                    if let Some(l) = nb.get_mut(&w) {
                        l.retain(|y| *y != x);
                        if l.len() < 2 {
                            work.push(w);
                        }
                    }
                }
            }
        }
        let remaining: usize = nb.values().map(Vec::len).sum::<usize>() / 2;
        rep.open_edges += pairs_in.len() - remaining;
        if nb.is_empty() {
            return Ok(());
        }
        // neighbours in counter-clockwise order
        let mut order: Vec<Id> = nb.keys().copied().collect();
        order.sort();
        for x in &order {
            let c = xy(self, *x);
            let mut ns = nb.remove(x).unwrap();
            ns.sort_by(|a, b| {
                let (da, db) = (xy(self, *a) - c, xy(self, *b) - c);
                da.y.atan2(da.x).total_cmp(&db.y.atan2(db.x)).then(a.cmp(b))
            });
            nb.insert(*x, ns);
        }
        // walk: keep the face on the left
        let mut used: HashSet<(Id, Id)> = HashSet::new();
        let mut pos: Vec<Cycle> = vec![];
        let mut neg: Vec<Cycle> = vec![];
        for &a0 in &order {
            for &b0 in &nb[&a0] {
                if used.contains(&(a0, b0)) {
                    continue;
                }
                let mut cyc = vec![a0];
                let (mut p, mut q) = (a0, b0);
                let mut ok = true;
                loop {
                    used.insert((p, q));
                    if q == a0 {
                        break;
                    }
                    cyc.push(q);
                    if cyc.len() > MAX_LOOP {
                        ok = false;
                        break;
                    }
                    let ns = &nb[&q];
                    let i = ns.iter().position(|x| *x == p).unwrap();
                    let w = ns[(i + ns.len() - 1) % ns.len()];
                    p = q;
                    q = w;
                }
                if !ok || cyc.len() < 3 {
                    continue;
                }
                let poly: Vec<DVec2> = cyc.iter().map(|x| xy(self, *x)).collect();
                let area = (0..poly.len()).map(|i| poly[i].perp_dot(poly[(i + 1) % poly.len()])).sum::<f64>() / 2.0;
                if area.abs() <= 1e-12 {
                    continue;
                }
                if cyc.iter().collect::<HashSet<_>>().len() != cyc.len() {
                    rep.degenerate += 1;
                    continue;
                }
                let lo = poly.iter().fold(DVec2::splat(f64::MAX), |a, p| a.min(*p));
                let hi = poly.iter().fold(DVec2::splat(f64::MIN), |a, p| a.max(*p));
                let c = Cycle { comp: comp_of[&cyc[0]], verts: cyc, poly, area, lo, hi };
                if area > 0.0 { pos.push(c) } else { neg.push(c) }
            }
        }
        // the smallest face (of another piece) around a point, by index
        let mut tests = 0usize;
        let around = |pt: DVec2, comp: usize, skip: Option<usize>, tests: &mut usize| -> Option<usize> {
            let mut best: Option<usize> = None;
            for (i, f) in pos.iter().enumerate() {
                if f.comp == comp || Some(i) == skip || pt.cmplt(f.lo).any() || pt.cmpgt(f.hi).any() {
                    continue;
                }
                *tests += 1;
                if *tests > MAX_HOLE_TESTS {
                    return None;
                }
                if geom::inside(pt, &f.poly) && best.is_none_or(|b| f.area < pos[b].area) {
                    best = Some(i);
                }
            }
            best
        };
        let mut holes: HashMap<usize, Vec<usize>> = HashMap::new();
        for (k, h) in neg.iter().enumerate() {
            if let Some(i) = around(h.poly[0], h.comp, None, &mut tests) {
                holes.entry(i).or_default().push(k);
            }
        }
        if tests > MAX_HOLE_TESTS {
            rep.notes.push("too many nested loops to sort out which are holes; holes were left out".into());
            holes.clear();
        }
        // with open_holes, a face inside a hole is not made: alternate by how many faces are around it
        let skip: Vec<bool> = if opts.open_holes {
            (0..pos.len()).map(|i| {
                let (mut depth, mut pt, mut at) = (0usize, pos[i].poly[0], i);
                while let Some(j) = around(pt, pos[at].comp, Some(at), &mut tests) {
                    depth += 1;
                    (pt, at) = (pos[j].poly[0], j);
                    if depth > 64 {
                        break;
                    }
                }
                depth % 2 == 1
            }).collect()
        } else {
            vec![false; pos.len()]
        };
        for (i, f) in pos.iter().enumerate() {
            if skip[i] {
                continue;
            }
            let mut sorted = f.verts.clone();
            sorted.sort();
            if existing.contains(&sorted) {
                rep.existing += 1;
                continue;
            }
            let hs: Vec<Vec<Id>> = holes.get(&i).map(|ks| ks.iter().map(|k| neg[*k].verts.clone()).collect()).unwrap_or_default();
            rep.holes += hs.len();
            rep.with_holes += usize::from(!hs.is_empty());
            let id = self.insert_face(Face { outer: f.verts.clone(), holes: hs, group, ..Default::default() });
            rep.faces.push(id);
        }
        Ok(())
    }
}
