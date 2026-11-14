//! SketchUp's entity operations: welding edges into one curve (and exploding
//! it back), making a component placement unique, orienting a surface's faces
//! to match one, and 3D text from glyph outlines.

use std::collections::{BTreeSet, HashMap, VecDeque};

use anyhow::{bail, Result};
use glam::{DVec2, DVec3};

use crate::geom;
use crate::model::{Id, Model};

impl Model {
    /// Mark edges as one curve: they're picked, offset and followed together.
    /// They must form a single chain (open or closed).
    pub fn weld(&mut self, ids: &[Id]) -> Result<Id> {
        let edges: Vec<Id> = ids.iter().copied().filter(|e| self.edges.contains_key(e)).collect();
        if edges.len() < 2 {
            bail!("weld needs two or more connected edges");
        }
        // a chain: every point is used by at most 2 of these edges, and they're connected
        let mut uses: HashMap<Id, Vec<Id>> = HashMap::new();
        for &e in &edges {
            let ed = &self.edges[&e];
            uses.entry(ed.a).or_default().push(e);
            uses.entry(ed.b).or_default().push(e);
        }
        if uses.values().any(|u| u.len() > 2) {
            bail!("those edges branch: weld makes one curve, so pick a single chain");
        }
        let mut seen = BTreeSet::new();
        let mut q = VecDeque::from([edges[0]]);
        while let Some(e) = q.pop_front() {
            if !seen.insert(e) {
                continue;
            }
            let ed = &self.edges[&e];
            for v in [ed.a, ed.b] {
                q.extend(uses[&v].iter().copied());
            }
        }
        if seen.len() != edges.len() {
            bail!("those edges aren't connected end to end");
        }
        let id = self.new_id();
        for e in edges {
            self.edges.get_mut(&e).unwrap().curve = Some(id);
        }
        Ok(id)
    }

    /// Every edge of the curves these edges belong to (the edges themselves if loose).
    pub fn curve_edges(&self, ids: &[Id]) -> Vec<Id> {
        let curves: BTreeSet<Id> = ids.iter().filter_map(|e| self.edges.get(e)?.curve).collect();
        let mut out: BTreeSet<Id> = ids.iter().copied().filter(|e| self.edges.contains_key(e)).collect();
        out.extend(self.edges.iter().filter(|(_, e)| e.curve.is_some_and(|c| curves.contains(&c))).map(|(&k, _)| k));
        out.into_iter().collect()
    }

    /// Explode curves back into separate edges.
    pub fn explode_curve(&mut self, ids: &[Id]) -> usize {
        let all = self.curve_edges(ids);
        for e in &all {
            self.edges.get_mut(e).unwrap().curve = None;
        }
        all.len()
    }

    /// Mark the edges between consecutive points as one curve (arcs, circles).
    pub(crate) fn curve_through(&mut self, pts: &[DVec3], closed: bool) -> Option<Id> {
        let vid = |m: &Model, p: DVec3| m.vertices.iter().find(|(_, v)| v.group == m.active_group && v.p.distance(p) < 1e-7).map(|(&k, _)| k);
        let mut es = vec![];
        let n = pts.len();
        for i in 0..if closed { n } else { n.saturating_sub(1) } {
            let (Some(a), Some(b)) = (vid(self, pts[i]), vid(self, pts[(i + 1) % n])) else { return None };
            es.push(self.find_edge(a, b)?);
        }
        if es.len() < 2 {
            return None;
        }
        let id = self.new_id();
        for e in es {
            self.edges.get_mut(&e).unwrap().curve = Some(id);
        }
        Some(id)
    }

    /// Give one placement of a shared component its own copy of the
    /// definition, so editing it leaves the others alone.
    pub fn make_unique(&mut self, inst: Id) -> Result<Id> {
        let Some(i) = self.instances.get(&inst) else { bail!("no component placement {inst}") };
        let old = i.def;
        if self.instances.values().filter(|x| x.def == old).count() < 2 {
            return Ok(old); // already the only one
        }
        let d = &self.definitions[&old];
        let mut copy = d.clone();
        copy.model = std::sync::Arc::new((*d.model).clone());
        let base = d.name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '#').trim_end().to_string();
        let mut n = 2;
        while self.definitions.values().any(|x| x.name == format!("{base} #{n}")) {
            n += 1;
        }
        copy.name = format!("{base} #{n}");
        let id = self.new_id();
        self.definitions.insert(id, copy);
        self.instances.get_mut(&inst).unwrap().def = id;
        Ok(id)
    }

    /// Flip the faces of a connected surface so they all face the same way as `seed`
    /// (neighbours across an edge should run that edge in opposite directions).
    pub fn orient_faces(&mut self, seed: Id) -> Result<Vec<Id>> {
        self.face(seed)?;
        let group = self.faces[&seed].group;
        // directed edge (a→b) → the faces that use it
        let ring = |m: &Model, f: Id| -> Vec<(Id, Id)> {
            m.faces[&f].loops().flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()])).collect::<Vec<_>>()).collect()
        };
        let mut by_edge: HashMap<(Id, Id), Vec<Id>> = HashMap::new();
        for (&f, face) in &self.faces {
            if face.group != group {
                continue;
            }
            for (a, b) in ring(self, f) {
                by_edge.entry((a.min(b), a.max(b))).or_default().push(f);
            }
        }
        let mut done = BTreeSet::from([seed]);
        let mut flipped = vec![];
        let mut q = VecDeque::from([seed]);
        while let Some(f) = q.pop_front() {
            for (a, b) in ring(self, f) {
                let shared = &by_edge[&(a.min(b), a.max(b))];
                if shared.len() != 2 {
                    continue; // a border or a fin: nothing to agree with
                }
                let g = if shared[0] == f { shared[1] } else { shared[0] };
                if !done.insert(g) {
                    continue;
                }
                // agreeing neighbours run the shared edge b→a
                if ring(self, g).contains(&(a, b)) {
                    self.faces.get_mut(&g).unwrap().reverse();
                    flipped.push(g);
                }
                q.push_back(g);
            }
        }
        Ok(flipped)
    }

    /// 3D text from glyph outlines: `rings` are closed outlines in the text's own
    /// plane (meters, x right, y up). Rings inside an odd number of others are
    /// holes (the middle of an O). Laid in the plane through `at` spanned by
    /// `right` and `up`, then pulled `depth` toward right × up (0 = flat).
    pub fn text_3d(&mut self, rings: &[Vec<[f64; 2]>], at: DVec3, right: DVec3, up: DVec3, depth: f64) -> Result<Vec<Id>> {
        let rings: Vec<Vec<DVec2>> = rings
            .iter()
            .map(|r| {
                let mut v: Vec<DVec2> = r.iter().map(|p| DVec2::new(p[0], p[1])).collect();
                v.dedup_by(|a, b| a.distance(*b) < 1e-6);
                if v.len() > 1 && v[0].distance(*v.last().unwrap()) < 1e-6 {
                    v.pop();
                }
                v
            })
            .filter(|r| r.len() >= 3 && area(r).abs() > 1e-10)
            .collect();
        if rings.is_empty() {
            bail!("no outlines to make text from");
        }
        let (right, up) = (right.normalize(), up.normalize());
        let n = right.cross(up);
        if n.length() < 1e-9 {
            bail!("right and up are parallel");
        }
        let to3 = |p: DVec2| at + right * p.x + up * p.y;
        // nesting depth: how many rings each one sits inside
        let depth_of: Vec<usize> = rings.iter().enumerate().map(|(i, r)| rings.iter().enumerate().filter(|(j, o)| *j != i && geom::inside(r[0], o)).count()).collect();
        let mut solids = vec![];
        // outers first, so holes land in them
        let mut order: Vec<usize> = (0..rings.len()).collect();
        order.sort_by_key(|&i| depth_of[i]);
        for i in order {
            let mut r = rings[i].clone();
            if area(&r) < 0.0 {
                r.reverse(); // counter-clockwise, facing out of the text plane
            }
            let pts: Vec<DVec3> = r.iter().map(|p| to3(*p)).collect();
            let f = self.add_polygon(&pts)?;
            if depth_of[i] % 2 == 1 {
                // a hole: the face drawn inside it fills the counter; remove that fill
                self.faces.remove(&f);
            } else {
                solids.push(f);
            }
        }
        // the faces may have been split or re-made by later holes: find them again
        let mut out = vec![];
        for f in solids {
            if !self.faces.contains_key(&f) {
                continue;
            }
            if self.normal(f).dot(n) < 0.0 {
                self.faces.get_mut(&f).unwrap().reverse();
            }
            if depth.abs() > 1e-9 {
                out.extend(self.pushpull(f, depth)?.into_iter().take(1));
            } else {
                out.push(f);
            }
        }
        Ok(out)
    }
}

/// Signed area (counter-clockwise positive).
fn area(r: &[DVec2]) -> f64 {
    (0..r.len()).map(|i| r[i].perp_dot(r[(i + 1) % r.len()])).sum::<f64>() / 2.0
}
