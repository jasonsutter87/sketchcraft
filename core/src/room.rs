//! Room mode: draw a room the way you do in The Sims. Drag a rectangle on the ground and get four walls and a floor.
//!
//! A room is an ordinary group named "Room 1", "Room 2"... on the tag "Rooms". Inside it are the walls (one or more
//! ordinary Wall groups, on the tag "Walls") and the floor (one face). Nothing here is special to the file format: a room
//! saves, exports and undoes like any other geometry, and every other tool can edit it.
//!
//! **Sharing walls.** A new room that lines up with an existing room's wall does not build a second wall there. Its own
//! walls are built as open runs that stop at the neighbour's wall face (the layout Wall pull already knows: a wall that
//! ends against another, a T-junction), so pulling the shared wall later stretches both rooms. If a side of the new room
//! is within one wall thickness of a neighbour's wall line it is snapped onto it ("snap to touch"); two rooms whose
//! insides would overlap are refused, with the model untouched. A room that shares a wall takes that wall's thickness.
//!
//! **Where a room's size comes from.** There is no extra data: the floor face is the room's inside, and the wall
//! thickness is read from its walls. So a room follows whatever Wall pull (or any other tool) did to it.

use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;

/// Tag every room group is on, and the tag of the wall groups inside a room.
pub const ROOMS_TAG: &str = "Rooms";
pub const WALLS_TAG: &str = "Walls";
/// Smallest inside width or depth of a room (metres).
const MIN_INSIDE: f64 = 0.3;
/// Largest room side (metres): a sanity bound for typed or runaway drags.
const MAX_SIDE: f64 = 1000.0;
const EPS: f64 = 1e-6;

/// An axis-aligned rectangle `[x0, y0, x1, y1]`.
pub type Rect = [f64; 4];

#[derive(Debug, Clone)]
pub struct RoomInfo {
    pub id: Id,
    pub name: String,
    pub floor: Id,
    /// the inside (the floor's outline)
    pub inside: Rect,
    pub z: f64,
    pub thickness: f64,
    pub area: f64,
}

impl RoomInfo {
    /// The centre lines of the room's walls.
    pub fn centre(&self) -> Rect {
        grow(self.inside, self.thickness / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct RoomPlan {
    /// the room's wall centre lines after snapping
    pub rect: Rect,
    pub thickness: f64,
    /// the rooms whose walls it shares, with the sides it shares them on
    pub shared: Vec<(Id, &'static str)>,
    pub snapped: bool,
    pub inside: Rect,
    pub area: f64,
}

fn grow(r: Rect, d: f64) -> Rect {
    [r[0] - d, r[1] - d, r[2] + d, r[3] + d]
}

fn overlap_area(a: Rect, b: Rect) -> f64 {
    (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0)
}

fn span(lo0: f64, hi0: f64, lo1: f64, hi1: f64) -> f64 {
    hi0.min(hi1) - lo0.max(lo1)
}

/// The wall bodies of a room: each centre-line side thickened, with square ends (the corner is filled).
fn bodies(centre: Rect, t: f64) -> [Rect; 4] {
    let h = t / 2.0;
    let [x0, y0, x1, y1] = centre;
    [[x0 - h, y0 - h, x1 + h, y0 + h], [x1 - h, y0 - h, x1 + h, y1 + h], [x0 - h, y1 - h, x1 + h, y1 + h], [x0 - h, y0 - h, x0 + h, y1 + h]]
}

/// The four sides of a rectangle as (start, end), counter-clockwise: south, east, north, west.
fn sides(r: Rect) -> [([f64; 2], [f64; 2]); 4] {
    let [x0, y0, x1, y1] = r;
    [([x0, y0], [x1, y0]), ([x1, y0], [x1, y1]), ([x1, y1], [x0, y1]), ([x0, y1], [x0, y0])]
}

const SIDE_NAMES: [&str; 4] = ["south", "east", "north", "west"];

/// Side `i` (0 south, 1 east, 2 north, 3 west) as: runs along x?, the fixed coordinate, the (low, high) extent along it.
fn side_line(r: Rect, i: usize) -> (bool, f64, f64, f64) {
    match i {
        0 => (true, r[1], r[0], r[2]),
        2 => (true, r[3], r[0], r[2]),
        1 => (false, r[2], r[1], r[3]),
        _ => (false, r[0], r[1], r[3]),
    }
}

impl Model {
    /// Every room in the model (groups on the "Rooms" tag with a floor), in id order.
    pub fn rooms(&self) -> Vec<RoomInfo> {
        let mut out = Vec::new();
        for (&id, g) in &self.groups {
            if g.tag.as_deref() != Some(ROOMS_TAG) {
                continue;
            }
            let Some((&floor, f)) = self.faces.iter().find(|(&fid, f)| f.group == Some(id) && f.outer.len() >= 3 && self.normal(fid).z.abs() > 0.9999) else { continue };
            let pts = self.pts(&f.outer);
            let lo = pts.iter().fold(DVec3::splat(f64::MAX), |a, p| a.min(*p));
            let hi = pts.iter().fold(DVec3::splat(f64::MIN), |a, p| a.max(*p));
            out.push(RoomInfo { id, name: g.name.clone(), floor, inside: [lo.x, lo.y, hi.x, hi.y], z: lo.z, thickness: self.room_thickness(id), area: self.face_area(floor) });
        }
        out
    }

    /// The thickness of a room's own walls (the default if it has none that can be measured).
    fn room_thickness(&self, room: Id) -> f64 {
        let walls: Vec<Id> = self.groups.iter().filter(|(_, g)| g.parent == Some(room)).map(|(&k, _)| k).collect();
        for (&fid, f) in &self.faces {
            if f.group.is_some_and(|g| walls.contains(&g)) && f.outer.len() >= 3 && self.normal(fid).z.abs() < 1e-4 {
                if let Ok(p) = self.wall_plan(fid) {
                    return p.thickness;
                }
            }
        }
        0.2
    }

    /// Work out where a room dragged from `a` to `b` (wall centre lines) would go: snap its sides onto neighbours' walls
    /// that are within one thickness, take the thickness of the walls it shares, and refuse anything that overlaps.
    pub fn room_plan(&self, a: DVec3, b: DVec3, thickness: f64) -> Result<RoomPlan> {
        if ![a.x, a.y, a.z, b.x, b.y, b.z].iter().all(|v| v.is_finite()) || !(thickness > 0.0 && thickness <= 1.5) {
            bail!("that isn't a size a room can have");
        }
        let mut r: Rect = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
        if r[2] - r[0] > MAX_SIDE || r[3] - r[1] > MAX_SIDE {
            bail!("that room is too big (over {MAX_SIDE} m on a side)");
        }
        let others: Vec<(RoomInfo, Rect)> = self
            .rooms()
            .into_iter()
            .filter(|x| (x.z - a.z).abs() < 0.05)
            .map(|x| {
                let c = x.centre();
                (x, c)
            })
            .collect();

        // snap to touch: a side within one wall thickness of a neighbour's wall line (running alongside it) goes onto it
        let before = r;
        for i in 0..4 {
            let (horizontal, v, lo, hi) = side_line(before, i);
            let mut best: Option<f64> = None;
            for (_, c) in &others {
                let (_, c_lo_line, c_lo, c_hi) = side_line(*c, if horizontal { 0 } else { 3 });
                let (_, c_hi_line, _, _) = side_line(*c, if horizontal { 2 } else { 1 });
                if span(lo, hi, c_lo, c_hi) < 1e-3 {
                    continue;
                }
                for line in [c_lo_line, c_hi_line] {
                    let d = (line - v).abs();
                    if d > EPS && d <= thickness + EPS && best.is_none_or(|bl| d < (bl - v).abs()) {
                        best = Some(line);
                    }
                }
            }
            if let Some(line) = best {
                r[[1, 2, 3, 0][i]] = line;
            }
        }
        let snapped = r.iter().zip(before.iter()).any(|(x, y)| (x - y).abs() > EPS);

        // the rooms whose walls this one shares: a side on the same line, running alongside for some length
        let mut shared: Vec<(Id, &'static str)> = Vec::new();
        let mut t = thickness;
        let mut t_from: Option<f64> = None;
        for (i, side_name) in SIDE_NAMES.iter().enumerate() {
            let (horizontal, v, lo, hi) = side_line(r, i);
            for (x, c) in &others {
                for j in 0..4 {
                    let (h2, v2, lo2, hi2) = side_line(*c, j);
                    if h2 != horizontal || (v2 - v).abs() > EPS || span(lo, hi, lo2, hi2) <= 1e-3 {
                        continue;
                    }
                    if t_from.is_some_and(|tf| (tf - x.thickness).abs() > EPS) {
                        bail!("the rooms this one would share walls with have different wall thicknesses: draw it so it only touches walls of one thickness");
                    }
                    t_from = Some(x.thickness);
                    t = x.thickness;
                    if !shared.iter().any(|(id, s)| *id == x.id && s == side_name) {
                        shared.push((x.id, side_name));
                    }
                }
            }
        }

        let inside = grow(r, -t / 2.0);
        if inside[2] - inside[0] < MIN_INSIDE || inside[3] - inside[1] < MIN_INSIDE {
            bail!("that room is too small: its inside must be at least {MIN_INSIDE} m each way");
        }
        for (x, c) in &others {
            if overlap_area(inside, x.inside) > EPS {
                bail!("that overlaps {}: drag the new room next to it (its wall will be shared), not over it", x.name);
            }
            if bodies(*c, x.thickness).iter().any(|w| overlap_area(inside, *w) > EPS) {
                bail!("that runs into a wall of {}: drag the new room next to it, not across it", x.name);
            }
            if bodies(r, t).iter().any(|w| overlap_area(*w, x.inside) > EPS) {
                bail!("that room's wall would cut into {}: drag the new room next to it, not across it", x.name);
            }
        }
        Ok(RoomPlan { rect: r, thickness: t, shared, snapped, inside, area: (inside[2] - inside[0]) * (inside[3] - inside[1]) })
    }

    /// Make a room: a group "Room N" on the "Rooms" tag, holding its walls (tag "Walls") and a floor. Everything is
    /// checked before anything is built, so a refusal leaves the model untouched. Returns (room, plan, wall groups, floor).
    pub fn room_rect(&mut self, a: DVec3, b: DVec3, thickness: f64, height: f64, name: Option<String>) -> Result<(Id, RoomPlan, Vec<Id>, Id)> {
        if !(height > 0.0 && height <= 100.0) {
            bail!("a wall needs a height between 0 and 100 m");
        }
        let plan = self.room_plan(a, b, thickness)?;
        let t = plan.thickness;
        let z = a.z;
        let rooms = self.rooms();
        let existing: Vec<(RoomInfo, Rect)> = rooms
            .iter()
            .filter(|x| (x.z - z).abs() < 0.05)
            .map(|x| {
                let c = x.centre();
                (x.clone(), c)
            })
            .collect();
        let n = rooms.iter().filter_map(|r| r.name.strip_prefix("Room ").and_then(|s| s.parse::<u32>().ok())).max().unwrap_or(0) + 1;
        let name = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("Room {n}"));

        // which stretches of each side still need a wall: the rest lies along a neighbour's wall already
        let mut open: Vec<Vec<(f64, f64)>> = Vec::new();
        for i in 0..4 {
            let (horizontal, v, lo, hi) = side_line(plan.rect, i);
            let mut covered: Vec<(f64, f64)> = Vec::new();
            for (_, c) in &existing {
                for j in 0..4 {
                    let (h2, v2, lo2, hi2) = side_line(*c, j);
                    if h2 == horizontal && (v2 - v).abs() < EPS && span(lo, hi, lo2, hi2) > 1e-3 {
                        covered.push((lo2.max(lo), hi2.min(hi)));
                    }
                }
            }
            covered.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut gaps = Vec::new();
            let mut at = lo;
            for (c_lo, c_hi) in covered {
                if c_lo > at + 1e-3 {
                    gaps.push((at, c_lo));
                }
                at = at.max(c_hi);
            }
            if hi > at + 1e-3 {
                gaps.push((at, hi));
            }
            // north and west run toward smaller coordinates
            open.push(if i < 2 { gaps } else { gaps.into_iter().rev().map(|(l, h)| (h, l)).collect() });
        }
        let point = |i: usize, s: f64| -> DVec3 {
            let (horizontal, v, _, _) = side_line(plan.rect, i);
            if horizontal { DVec3::new(s, v, z) } else { DVec3::new(v, s, z) }
        };
        let mut pieces: Vec<(DVec3, DVec3)> = Vec::new();
        for (i, g) in open.iter().enumerate() {
            for &(s, e) in g {
                pieces.push((point(i, s), point(i, e)));
            }
        }
        // chain the new stretches around the room into runs (a full ring when nothing is shared)
        let m = pieces.len();
        let ring = m == 4 && (0..4).all(|k| pieces[k].1.distance(pieces[(k + 1) % 4].0) < 1e-6);
        let mut runs: Vec<Vec<DVec3>> = Vec::new();
        if !ring && m > 0 {
            let joined = |k: usize| m > 1 && pieces[k].1.distance(pieces[(k + 1) % m].0) < 1e-6;
            let mut k = (0..m).find(|&k| !joined((k + m - 1) % m)).unwrap_or(0);
            let mut done = 0;
            while done < m {
                let mut run = vec![pieces[k].0, pieces[k].1];
                done += 1;
                while done < m && joined(k) {
                    k = (k + 1) % m;
                    run.push(pieces[k].1);
                    done += 1;
                }
                runs.push(run);
                k = (k + 1) % m;
            }
        }
        // a run that begins or ends inside a neighbour's wall stops at the wall's face
        let walls: Vec<Rect> = existing.iter().flat_map(|(x, c)| bodies(*c, x.thickness)).collect();
        let exit = |p: DVec3, d: DVec3| -> f64 {
            let mut s = 0.0f64;
            loop {
                let q = p + d * s;
                let Some(w) = walls.iter().find(|w| q.x > w[0] + 1e-6 && q.x < w[2] - 1e-6 && q.y > w[1] + 1e-6 && q.y < w[3] - 1e-6) else { return s };
                let ahead = if d.x > 0.5 { w[2] - p.x } else if d.x < -0.5 { p.x - w[0] } else if d.y > 0.5 { w[3] - p.y } else { p.y - w[1] };
                if ahead <= s + 1e-9 {
                    return s;
                }
                s = ahead;
            }
        };
        let mut trimmed: Vec<Vec<DVec3>> = Vec::new();
        for mut run in runs {
            for end in [false, true] {
                while run.len() >= 2 {
                    let (i, j) = if end { (run.len() - 1, run.len() - 2) } else { (0, 1) };
                    let d = (run[j] - run[i]).normalize();
                    let s = exit(run[i], d);
                    if s <= 1e-9 {
                        break;
                    }
                    if s >= run[i].distance(run[j]) - 1e-3 {
                        run.remove(i); // all of that first stretch is inside the neighbour's wall
                        continue;
                    }
                    run[i] += d * s;
                    break;
                }
            }
            if run.len() >= 2 {
                trimmed.push(run);
            }
        }

        let (room, (wall_groups, floor)) = self.in_new_group(&name, |m| {
            let mut made = Vec::new();
            if ring {
                let c: Vec<DVec3> = sides(plan.rect).iter().map(|(p, _)| DVec3::new(p[0], p[1], z)).collect();
                made.push(m.wall(&c, t, height, true)?);
            }
            for run in &trimmed {
                made.push(m.wall(run, t, height, false)?);
            }
            for &w in &made {
                m.groups.get_mut(&w).unwrap().name = "Walls".to_string();
            }
            let i = plan.inside;
            let f = m.add_polygon(&[DVec3::new(i[0], i[1], z), DVec3::new(i[2], i[1], z), DVec3::new(i[2], i[3], z), DVec3::new(i[0], i[3], z)])?;
            if m.normal(f).z < 0.0 {
                m.faces.get_mut(&f).unwrap().reverse();
            }
            Ok((made, f))
        })?;
        self.set_tag(&[room], Some(ROOMS_TAG.to_string()))?;
        if !wall_groups.is_empty() {
            self.set_tag(&wall_groups, Some(WALLS_TAG.to_string()))?;
        }
        Ok((room, plan, wall_groups, floor))
    }
}
