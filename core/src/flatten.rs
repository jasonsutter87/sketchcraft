//! Flatten to a height: move points to one Z, join the points that land on each other, drop the lines and faces that
//! collapse. Made for imported DWG/DXF linework drawn at odd heights. See docs/dev/small-tools.md.

use crate::geom;
use crate::model::{Id, Model, MAX_COORD, WELD};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

/// A face is flat when every corner is within this of its plane (m).
const PLANAR: f64 = 1e-5;
/// Smallest area a face may be left with (m2).
const MIN_AREA: f64 = 1e-12;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct FlattenReport {
    pub z: f64,
    /// points that changed height
    pub moved: usize,
    /// points that landed on another point and were joined to it
    pub merged: usize,
    /// lines dropped: collapsed to nothing, or now the same as another line
    pub edges_removed: usize,
    /// faces that collapsed to a line (a wall seen from above) and faces a moved point bent out of flat
    pub faces_removed: usize,
    /// faces turned over so they face up
    pub faces_turned: usize,
}

impl FlattenReport {
    pub fn changes(&self) -> usize {
        self.moved + self.merged + self.edges_removed + self.faces_removed + self.faces_turned
    }
}

fn cell(p: DVec3) -> (i64, i64) {
    ((p.x / (2.0 * WELD)).floor() as i64, (p.y / (2.0 * WELD)).floor() as i64)
}

/// Remove repeated corners and back-and-forth spikes from a loop; None when nothing face-like is left.
fn clean(lp: &[Id], map: &HashMap<Id, Id>) -> Vec<Id> {
    let mut out: Vec<Id> = Vec::with_capacity(lp.len());
    for v in lp {
        let v = *map.get(v).unwrap_or(v);
        if out.last() != Some(&v) {
            out.push(v);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    // a, b, a: b is a spike
    loop {
        let n = out.len();
        if n < 3 {
            break;
        }
        match (0..n).find(|&i| out[(i + n - 1) % n] == out[(i + 1) % n]) {
            Some(i) => {
                let (a, b) = (i, (i + 1) % n);
                let (hi, lo) = (a.max(b), a.min(b));
                out.remove(hi);
                out.remove(lo);
            }
            None => break,
        }
    }
    out
}

impl Model {
    /// The points a flatten moves: the named points, lines and faces, every point in the named groups, or (no ids) every
    /// point in unlocked groups.
    pub fn flatten_scope(&self, ids: &[Id]) -> Result<BTreeSet<Id>> {
        let mut out = BTreeSet::new();
        if ids.is_empty() {
            let mut locked: HashMap<Option<Id>, bool> = HashMap::new();
            out.extend(self.vertices.iter().filter(|(_, v)| !*locked.entry(v.group).or_insert_with(|| self.locked_by(v.group).is_some())).map(|(&k, _)| k));
            return Ok(out);
        }
        self.check_unlocked(ids)?;
        for &id in ids {
            if let Some(e) = self.edges.get(&id) {
                out.extend([e.a, e.b]);
            } else if let Some(f) = self.faces.get(&id) {
                out.extend(f.loops().flatten().copied());
            } else if self.groups.contains_key(&id) {
                let tree = self.subtree(id);
                out.extend(self.vertices.iter().filter(|(_, v)| v.group.is_some_and(|g| tree.contains(&g))).map(|(&k, _)| k));
            } else if self.vertices.contains_key(&id) {
                out.insert(id);
            } else if self.instances.contains_key(&id) {
                bail!("{id} is a placed component: open it for editing (edit) and flatten inside, or flatten a group");
            } else {
                bail!("{id} is not a point, line, face or group");
            }
        }
        Ok(out)
    }

    /// Move `verts` to height `z`, join points that land together, and tidy what collapsed. In place; the engine
    /// command makes it one undo step.
    pub fn flatten(&mut self, verts: &BTreeSet<Id>, z: f64) -> Result<FlattenReport> {
        if !z.is_finite() || z.abs() > MAX_COORD {
            bail!("that height is not a number the model can hold");
        }
        let mut rep = FlattenReport { z, ..Default::default() };
        let mut moved: Vec<Id> = vec![];
        for v in verts {
            if let Some(x) = self.vertices.get_mut(v) {
                if x.p.z != z {
                    x.p.z = z;
                    rep.moved += 1;
                }
                moved.push(*v);
            }
        }
        if moved.is_empty() {
            return Ok(rep);
        }
        let moved_set: HashSet<Id> = moved.iter().copied().collect();
        // points already at this height are what the moved ones may join; then each moved point joins the first one
        // within WELD of it (lowest id first), or becomes one others can join
        let mut grid: HashMap<(Option<Id>, i64, i64), Vec<Id>> = HashMap::new();
        for (&id, v) in &self.vertices {
            if !moved_set.contains(&id) && (v.p.z - z).abs() < WELD {
                let (cx, cy) = cell(v.p);
                grid.entry((v.group, cx, cy)).or_default().push(id);
            }
        }
        let mut map: HashMap<Id, Id> = HashMap::new();
        for &id in &moved {
            let (p, g) = (self.vertices[&id].p, self.vertices[&id].group);
            let (cx, cy) = cell(p);
            let mut rep_id = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(c) = grid.get(&(g, cx + dx, cy + dy)) {
                        for &o in c {
                            if self.vertices[&o].p.distance(p) < WELD {
                                rep_id = Some(rep_id.map_or(o, |r: Id| r.min(o)));
                            }
                        }
                    }
                }
            }
            match rep_id {
                Some(r) => {
                    map.insert(id, r);
                }
                None => grid.entry((g, cx, cy)).or_default().push(id),
            }
        }
        rep.merged = map.len();

        // lines: follow the joins, drop the collapsed and the repeated
        let mut seen: HashMap<(Id, Id), Id> = HashMap::new();
        let mut dead: Vec<Id> = vec![];
        for (&id, e) in self.edges.iter_mut() {
            if map.contains_key(&e.a) || map.contains_key(&e.b) {
                e.a = *map.get(&e.a).unwrap_or(&e.a);
                e.b = *map.get(&e.b).unwrap_or(&e.b);
            }
            let k = if e.a < e.b { (e.a, e.b) } else { (e.b, e.a) };
            if e.a == e.b || seen.insert(k, id).is_some() {
                dead.push(id);
            }
        }
        rep.edges_removed = dead.len();
        for id in dead {
            self.edges.remove(&id);
        }

        // faces: follow the joins, then drop what collapsed or bent
        let touched: Vec<Id> = self
            .faces
            .iter()
            .filter(|(_, f)| f.loops().flatten().any(|v| moved_set.contains(v) || map.contains_key(v)))
            .map(|(&k, _)| k)
            .collect();
        for fid in touched {
            let f = self.faces.get(&fid).unwrap().clone();
            let outer = clean(&f.outer, &map);
            let holes: Vec<Vec<Id>> = f.holes.iter().map(|h| clean(h, &map)).filter(|h| h.len() >= 3).collect();
            let pinched = outer.iter().collect::<HashSet<_>>().len() != outer.len();
            let pts = self.pts_checked(&outer);
            let bad = outer.len() < 3 || pinched || pts.is_none();
            let n = pts.as_ref().map(|p| geom::newell(p)).unwrap_or(DVec3::ZERO);
            let flat = !bad && n != DVec3::ZERO && geom::area(pts.as_ref().unwrap()) > MIN_AREA && {
                let p0 = pts.as_ref().unwrap()[0];
                outer.iter().chain(holes.iter().flatten()).all(|v| self.vertices.get(v).is_some_and(|x| (x.p - p0).dot(n).abs() <= PLANAR))
            };
            if !flat {
                self.faces.remove(&fid);
                rep.faces_removed += 1;
                continue;
            }
            let same = outer.len() == f.outer.len() && holes.len() == f.holes.len() && holes.iter().zip(&f.holes).all(|(a, b)| a.len() == b.len());
            let face = self.faces.get_mut(&fid).unwrap();
            face.outer = outer;
            face.holes = holes;
            if !same {
                face.uv = None;
                face.uv2 = None;
                face.vcol = None;
            }
            // a face that is now level looks up
            if n.z < -0.999_999_999 {
                face.reverse();
                rep.faces_turned += 1;
            }
        }
        for v in map.keys() {
            self.vertices.remove(v);
        }
        Ok(rep)
    }

    fn pts_checked(&self, l: &[Id]) -> Option<Vec<DVec3>> {
        l.iter().map(|v| self.vertices.get(v).map(|x| x.p)).collect()
    }
}
