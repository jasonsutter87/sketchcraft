//! Room mode: draw a room the way you do in The Sims. Drag a rectangle on the ground and get walls and a floor; drag one
//! over an existing room and combine with it or cut a notch out of it.
//!
//! A room is an ordinary group named "Room 1", "Room 2"... on the tag "Rooms". Inside it are the walls (one or more
//! ordinary Wall groups, on the tag "Walls") and the floor (one face). Nothing here is special to the file format: a room
//! saves, exports and undoes like any other geometry, and every other tool can edit it. A room is an axis-aligned
//! (rectilinear) polygon: a rectangle when drawn, an L or a notched shape after a boolean.
//!
//! **Sharing walls.** A new room that lines up with an existing room's wall does not build a second wall there. Its own
//! walls are built as open runs that stop at the neighbour's wall face (the layout Wall pull already knows: a wall that
//! ends against another, a T-junction), so pulling the shared wall later stretches both rooms. If a side of the new room
//! is within one wall thickness of a neighbour's wall line it is snapped onto it ("snap to touch"). A room that shares a
//! wall takes that wall's thickness.
//!
//! **Booleans.** A new room whose inside overlaps an existing room is refused in mode `new`. In mode `combine` the two
//! become one room (the existing room's name, its walls rebuilt along the union's outline); in mode `subtract` the dragged
//! box is cut out of the existing room. Both work on the wall centre-line polygons (see `rectpoly`), refuse a result that
//! splits in two or has a hole, rebuild only the target room's own walls (other rooms' walls stay as they are), keep the
//! doors and windows whose wall survives, and are one command, so one Undo. `RoomMode` is the switch: Inner, Outer and
//! Split are further `rectpoly::Op` rules plus a mode name away.
//!
//! **Where a room's size comes from.** There is no extra data: the floor face is the room's inside, and the wall
//! thickness is read from its walls. So a room follows whatever Wall pull (or any other tool) did to it.

use crate::model::{Id, Model};
use crate::rectpoly::{self as rp, Edge, Op, Poly, Pt};
use anyhow::{bail, Result};
use glam::{DMat4, DVec3};
use serde::Deserialize;

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

/// What a dragged box does where it overlaps a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomMode {
    /// A new room; an overlap is refused (the original behaviour).
    #[default]
    New,
    /// Union with the room it overlaps (a plain new room where it overlaps none).
    Combine,
    /// The box cut out of the room it overlaps.
    Subtract,
}

impl RoomMode {
    pub fn name(self) -> &'static str {
        match self {
            RoomMode::New => "new",
            RoomMode::Combine => "combine",
            RoomMode::Subtract => "subtract",
        }
    }
    fn op(self) -> Option<Op> {
        match self {
            RoomMode::New => None,
            RoomMode::Combine => Some(Op::Union),
            RoomMode::Subtract => Some(Op::Subtract),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoomInfo {
    pub id: Id,
    pub name: String,
    pub floor: Id,
    /// the bounding rectangle of the inside
    pub inside: Rect,
    /// the inside outline (the floor's), counter-clockwise
    pub outline: Poly,
    /// a point inside the outline to hang the label on
    pub label: Pt,
    pub z: f64,
    pub thickness: f64,
    pub area: f64,
}

impl RoomInfo {
    /// The centre lines of the room's walls: the inside outline moved out by half a wall.
    pub fn centre_poly(&self) -> Poly {
        rp::offset(&self.outline, self.thickness / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct RoomPlan {
    /// the bounding rectangle of the room's wall centre lines after snapping
    pub rect: Rect,
    pub thickness: f64,
    /// the rooms whose walls it shares, with the sides it shares them on
    pub shared: Vec<(Id, &'static str)>,
    pub snapped: bool,
    /// the bounding rectangle of the inside
    pub inside: Rect,
    pub area: f64,
    /// the inside outline and the wall centre-line outline, counter-clockwise
    pub outline: Poly,
    pub centre_outline: Poly,
    pub label: Pt,
    /// what will really happen: `New` (a room of its own), or `Combine` / `Subtract` on `target`
    pub mode: RoomMode,
    pub target: Option<(Id, String)>,
}

/// What `room_rect` made.
#[derive(Debug, Clone)]
pub struct RoomBuilt {
    pub room: Id,
    pub plan: RoomPlan,
    pub walls: Vec<Id>,
    pub floor: Id,
    /// doors and windows kept on the rebuilt walls / removed because their wall is gone
    pub kept: usize,
    pub removed: usize,
}

fn grow(r: Rect, d: f64) -> Rect {
    [r[0] - d, r[1] - d, r[2] + d, r[3] + d]
}

fn span(lo0: f64, hi0: f64, lo1: f64, hi1: f64) -> f64 {
    hi0.min(hi1) - lo0.max(lo1)
}

fn rect_overlap(poly: &[Pt], r: Rect) -> f64 {
    rp::overlap_area(poly, &rp::rect_poly(r))
}

/// Do two edges lie on the same line and run alongside each other for some length?
fn collinear(e: &Edge, f: &Edge) -> bool {
    e.horizontal == f.horizontal && (e.v - f.v).abs() <= EPS && span(e.lo, e.hi, f.lo, f.hi) > 1e-3
}

/// The wall body of a stretch of centre line: thickened, with square ends.
fn body(e: &Edge, t: f64) -> Rect {
    let h = t / 2.0;
    if e.horizontal {
        [e.lo - h, e.v - h, e.hi + h, e.v + h]
    } else {
        [e.v - h, e.lo - h, e.v + h, e.hi + h]
    }
}

/// What is left of `lo..hi` once `covers` are taken away.
fn gaps(lo: f64, hi: f64, mut covers: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    covers.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out = Vec::new();
    let mut at = lo;
    for (c_lo, c_hi) in covers {
        if c_lo > at + 1e-3 {
            out.push((at, c_lo.min(hi)));
        }
        at = at.max(c_hi);
    }
    if hi > at + 1e-3 {
        out.push((at, hi));
    }
    out
}

/// Another room on the same level, with what its walls are (nominally, by its outline; and physically, by its wall faces).
struct Other {
    info: RoomInfo,
    centre: Poly,
    edges: Vec<Edge>,
    bodies: Vec<Rect>,
    /// the stretches of centre line it really has wall on, and those wall bodies
    built_edges: Vec<Edge>,
    built_bodies: Vec<Rect>,
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
            let z = pts.iter().fold(f64::MAX, |a, p| a.min(p.z));
            let xy: Vec<Pt> = pts.iter().map(|p| [p.x, p.y]).collect();
            let mut outline = rp::simplify(&xy);
            if !rp::is_rectilinear(&outline) {
                // a floor that isn't axis-aligned any more: its bounding rectangle stands in for it
                outline = rp::rect_poly(rp::bbox(&xy));
            }
            out.push(RoomInfo {
                id,
                name: g.name.clone(),
                floor,
                inside: rp::bbox(&outline),
                label: rp::label_point(&outline),
                outline,
                z,
                thickness: self.room_thickness(id),
                area: self.face_area(floor),
            });
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

    /// The wall bodies a room really has, read from its wall faces: pairs of long upright faces `thickness` apart.
    fn physical_walls(&self, room: &RoomInfo) -> (Vec<Edge>, Vec<Rect>) {
        let groups: Vec<Id> = self.groups.iter().filter(|(_, g)| g.parent == Some(room.id)).map(|(&k, _)| k).collect();
        let t = room.thickness;
        // (runs along x?, the face's coordinate across it, its extent along it)
        let mut long: Vec<(bool, f64, f64, f64)> = Vec::new();
        for (&fid, f) in &self.faces {
            if !f.group.is_some_and(|g| groups.contains(&g)) || f.outer.len() < 3 {
                continue;
            }
            let n = self.normal(fid);
            if n.z.abs() > 1e-4 {
                continue;
            }
            let pts = self.pts(&f.outer);
            let (lo, hi) = pts.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
            if hi.z - lo.z < 0.05 {
                continue;
            }
            if n.y.abs() > 0.9999 && hi.y - lo.y < 1e-5 && hi.x - lo.x > t + EPS {
                long.push((true, lo.y, lo.x, hi.x));
            } else if n.x.abs() > 0.9999 && hi.x - lo.x < 1e-5 && hi.y - lo.y > t + EPS {
                long.push((false, lo.x, lo.y, hi.y));
            }
        }
        let mut bodies = Vec::new();
        for a in &long {
            for b in &long {
                if a.0 == b.0 && (b.1 - a.1 - t).abs() < 1e-4 && span(a.2, a.3, b.2, b.3) > 1e-3 {
                    let (lo, hi) = (a.2.min(b.2), a.3.max(b.3));
                    bodies.push(if a.0 { [lo, a.1, hi, b.1] } else { [a.1, lo, b.1, hi] });
                }
            }
        }
        let edges = bodies
            .iter()
            .map(|w| if w[2] - w[0] >= w[3] - w[1] { Edge { horizontal: true, v: (w[1] + w[3]) / 2.0, lo: w[0], hi: w[2], fwd: true } } else { Edge { horizontal: false, v: (w[0] + w[2]) / 2.0, lo: w[1], hi: w[3], fwd: true } })
            .collect();
        (edges, bodies)
    }

    fn others_at(&self, z: f64, skip: Option<Id>) -> Vec<Other> {
        self.rooms()
            .into_iter()
            .filter(|x| (x.z - z).abs() < 0.05 && Some(x.id) != skip)
            .map(|info| {
                let centre = info.centre_poly();
                let edges = rp::edges(&centre);
                let bodies = edges.iter().map(|e| body(e, info.thickness)).collect();
                let (built_edges, built_bodies) = self.physical_walls(&info);
                Other { info, centre, edges, bodies, built_edges, built_bodies }
            })
            .collect()
    }

    /// Work out where a room dragged from `a` to `b` (wall centre lines) would go: snap its sides onto neighbours' walls
    /// that are within one thickness, take the thickness of the walls it shares, and refuse anything that overlaps.
    pub fn room_plan(&self, a: DVec3, b: DVec3, thickness: f64) -> Result<RoomPlan> {
        self.room_plan_mode(a, b, thickness, RoomMode::New)
    }

    /// As `room_plan`, in a mode: `Combine` and `Subtract` act on the one room the dragged box overlaps (and `Combine`
    /// over bare ground is just a new room); `plan.mode` says which really applies.
    pub fn room_plan_mode(&self, a: DVec3, b: DVec3, thickness: f64, mode: RoomMode) -> Result<RoomPlan> {
        if ![a.x, a.y, a.z, b.x, b.y, b.z].iter().all(|v| v.is_finite()) || !(thickness > 0.0 && thickness <= 1.5) {
            bail!("that isn't a size a room can have");
        }
        let mut r: Rect = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
        if r[2] - r[0] > MAX_SIDE || r[3] - r[1] > MAX_SIDE {
            bail!("that room is too big (over {MAX_SIDE} m on a side)");
        }
        let others = self.others_at(a.z, None);

        // snap to touch: a side within one wall thickness of a neighbour's wall line (running alongside it) goes onto it
        let before = r;
        for (i, e) in rp::edges(&rp::rect_poly(before)).iter().enumerate() {
            let mut best: Option<f64> = None;
            for o in &others {
                for f in &o.edges {
                    if f.horizontal != e.horizontal || span(e.lo, e.hi, f.lo, f.hi) < 1e-3 {
                        continue;
                    }
                    let d = (f.v - e.v).abs();
                    if d > EPS && d <= thickness + EPS && best.is_none_or(|bl| d < (bl - e.v).abs()) {
                        best = Some(f.v);
                    }
                }
            }
            if let Some(line) = best {
                r[[1, 2, 3, 0][i]] = line;
            }
        }
        let snapped = r.iter().zip(before.iter()).any(|(x, y)| (x - y).abs() > EPS);

        // which room does a combine / subtract act on: the one whose inside the dragged box's inside overlaps
        let mut target: Option<usize> = None;
        if mode != RoomMode::New {
            if r[2] - r[0] < 0.05 || r[3] - r[1] < 0.05 {
                bail!("that box is too small to combine with or cut from a room");
            }
            let probe = grow(r, -thickness / 2.0);
            let hits: Vec<usize> = if probe[2] > probe[0] && probe[3] > probe[1] { (0..others.len()).filter(|&k| rect_overlap(&others[k].info.outline, probe) > EPS).collect() } else { Vec::new() };
            match hits.len() {
                0 if mode == RoomMode::Subtract => bail!("drag the box over a room to cut a notch out of it"),
                0 => {}
                1 => target = Some(hits[0]),
                _ => bail!("that overlaps {} and {}: Combine and Subtract work on one room at a time", others[hits[0]].info.name, others[hits[1]].info.name),
            }
        }

        let rest: Vec<&Other> = others.iter().enumerate().filter(|(k, _)| Some(*k) != target).map(|(_, o)| o).collect();
        let Some(k) = target else {
            return Self::plan_common(rp::rect_poly(r), thickness, false, &rest, snapped, RoomMode::New, None);
        };
        let tg = &others[k];
        let op = mode.op().expect("a target only exists for a boolean mode");
        let centre = rp::boolean_single(&tg.centre, &rp::rect_poly(r), op)?;
        if (rp::overlap_area(&centre, &tg.centre) - rp::area(&tg.centre)).abs() < EPS && (rp::area(&centre) - rp::area(&tg.centre)).abs() < EPS {
            bail!("that doesn't change {}: drag the box so it adds to or cuts into the room", tg.info.name);
        }
        let t = tg.info.thickness;

        // the walls other rooms lean on (an open side of theirs lies along this room's wall) must still be there
        for o in &rest {
            for fe in &o.edges {
                for oe in &tg.edges {
                    if !collinear(fe, oe) {
                        continue;
                    }
                    let (lo, hi) = (fe.lo.max(oe.lo), fe.hi.min(oe.hi));
                    let slack = o.info.thickness / 2.0 + EPS;
                    let own: Vec<(f64, f64)> = o.built_edges.iter().filter(|b| b.horizontal == fe.horizontal && (b.v - fe.v).abs() < 1e-4).map(|b| (b.lo - slack, b.hi + slack)).collect();
                    let new_edges = rp::edges(&centre);
                    for (g_lo, g_hi) in gaps(lo, hi, own) {
                        let on_line: Vec<(f64, f64)> = new_edges.iter().filter(|n| n.horizontal == fe.horizontal && (n.v - fe.v).abs() <= EPS).map(|n| (n.lo, n.hi)).collect();
                        if !gaps(g_lo, g_hi, on_line).is_empty() {
                            bail!("that would take away the wall {} shares with {}: leave that wall where it is", o.info.name, tg.info.name);
                        }
                    }
                }
            }
        }
        Self::plan_common(centre, t, true, &rest, snapped, mode, Some((tg.info.id, tg.info.name.clone())))
    }

    /// Everything after the room's centre-line outline is known: shared walls, thickness, and what it may not run into.
    #[allow(clippy::too_many_arguments)]
    fn plan_common(centre: Poly, thickness: f64, fixed: bool, others: &[&Other], snapped: bool, mode: RoomMode, target: Option<(Id, String)>) -> Result<RoomPlan> {
        let ne = rp::edges(&centre);
        // the rooms whose walls this one shares: a side on the same line, running alongside for some length
        let mut shared: Vec<(Id, &'static str)> = Vec::new();
        let mut t = thickness;
        let mut t_from: Option<f64> = None;
        for e in &ne {
            for o in others {
                for f in &o.edges {
                    if !collinear(e, f) {
                        continue;
                    }
                    let clash = if fixed { (o.info.thickness - thickness).abs() > EPS } else { t_from.is_some_and(|tf| (tf - o.info.thickness).abs() > EPS) };
                    if clash {
                        bail!("the rooms this one would share walls with have different wall thicknesses: draw it so it only touches walls of one thickness");
                    }
                    if !fixed {
                        t_from = Some(o.info.thickness);
                        t = o.info.thickness;
                    }
                    if !shared.iter().any(|(id, s)| *id == o.info.id && *s == e.side()) {
                        shared.push((o.info.id, e.side()));
                    }
                }
            }
        }

        let inside = rp::offset(&centre, -t / 2.0);
        if !inside_ok(&centre, &inside) {
            bail!("that room is too small: its inside must be at least {MIN_INSIDE} m each way");
        }
        let mine: Vec<Rect> = ne.iter().map(|e| body(e, t)).collect();
        for o in others {
            if rp::overlap_area(&inside, &o.info.outline) > EPS {
                if mode == RoomMode::New {
                    bail!("that overlaps {}: drag the new room next to it (its wall will be shared), not over it", o.info.name);
                }
                bail!("that would run into {}: keep the result off other rooms", o.info.name);
            }
            if o.bodies.iter().any(|w| rect_overlap(&inside, *w) > EPS) {
                bail!("that runs into a wall of {}: drag the new room next to it, not across it", o.info.name);
            }
            if mine.iter().any(|w| rect_overlap(&o.info.outline, *w) > EPS) {
                bail!("that room's wall would cut into {}: drag the new room next to it, not across it", o.info.name);
            }
        }
        Ok(RoomPlan {
            rect: rp::bbox(&centre),
            thickness: t,
            shared,
            snapped,
            inside: rp::bbox(&inside),
            area: rp::area(&inside),
            label: rp::label_point(&inside),
            outline: inside,
            centre_outline: centre,
            mode,
            target,
        })
    }

    /// Make a room: a group "Room N" on the "Rooms" tag, holding its walls (tag "Walls") and a floor. Everything is
    /// checked before anything is built, so a refusal leaves the model untouched. In `Combine` / `Subtract` mode over a
    /// room, that room's walls and floor are rebuilt along the new outline instead (one command, one Undo).
    pub fn room_rect(&mut self, a: DVec3, b: DVec3, thickness: f64, height: f64, name: Option<String>, mode: RoomMode) -> Result<RoomBuilt> {
        if !(height > 0.0 && height <= 100.0) {
            bail!("a wall needs a height between 0 and 100 m");
        }
        let plan = self.room_plan_mode(a, b, thickness, mode)?;
        let z = a.z;
        if let Some((tid, _)) = plan.target.clone() {
            let backup = self.clone();
            let r = self.rebuild_room(plan, tid, z, height);
            if r.is_err() {
                *self = backup;
            }
            return r;
        }
        let t = plan.thickness;
        let rooms = self.rooms();
        let n = rooms.iter().filter_map(|r| r.name.strip_prefix("Room ").and_then(|s| s.parse::<u32>().ok())).max().unwrap_or(0) + 1;
        let name = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("Room {n}"));
        let others = self.others_at(z, None);
        let covers: Vec<Edge> = others.iter().flat_map(|o| o.edges.clone()).collect();
        let bodies: Vec<Rect> = others.iter().flat_map(|o| o.bodies.clone()).collect();
        let (ring, runs) = wall_runs(&plan.centre_outline, &covers, &bodies, z);
        let (room, (wall_groups, floor)) = self.in_new_group(&name, |m| {
            let made = m.build_walls(&plan.centre_outline, ring, &runs, t, height, z)?;
            let f = m.build_floor(&plan.outline, z)?;
            Ok((made, f))
        })?;
        self.set_tag(&[room], Some(ROOMS_TAG.to_string()))?;
        if !wall_groups.is_empty() {
            self.set_tag(&wall_groups, Some(WALLS_TAG.to_string()))?;
        }
        Ok(RoomBuilt { room, plan, walls: wall_groups, floor, kept: 0, removed: 0 })
    }

    /// The walls of a room (a closed ring, or open runs) as wall groups in the active group, named "Walls".
    fn build_walls(&mut self, centre: &[Pt], ring: bool, runs: &[Vec<DVec3>], t: f64, height: f64, z: f64) -> Result<Vec<Id>> {
        let mut made = Vec::new();
        if ring {
            let c: Vec<DVec3> = centre.iter().map(|p| DVec3::new(p[0], p[1], z)).collect();
            made.push(self.wall(&c, t, height, true)?);
        }
        for run in runs {
            made.push(self.wall(run, t, height, false)?);
        }
        for &w in &made {
            self.groups.get_mut(&w).unwrap().name = "Walls".to_string();
        }
        Ok(made)
    }

    /// A floor face facing up, in the active group.
    fn build_floor(&mut self, outline: &[Pt], z: f64) -> Result<Id> {
        let pts: Vec<DVec3> = outline.iter().map(|p| DVec3::new(p[0], p[1], z)).collect();
        let f = self.add_polygon(&pts)?;
        if self.normal(f).z < 0.0 {
            self.faces.get_mut(&f).unwrap().reverse();
        }
        Ok(f)
    }

    /// Replace a room's walls and floor with ones along the plan's outline. Walls shared with other rooms stay as they are;
    /// doors and windows whose wall still exists are cut into the new wall again, the rest are removed.
    fn rebuild_room(&mut self, plan: RoomPlan, tid: Id, z: f64, height: f64) -> Result<RoomBuilt> {
        let info = self.rooms().into_iter().find(|r| r.id == tid).ok_or_else(|| anyhow::anyhow!("that room is gone"))?;
        let t = plan.thickness;
        let others = self.others_at(z, Some(tid));
        let covers: Vec<Edge> = others.iter().flat_map(|o| o.built_edges.clone()).collect();
        let bodies: Vec<Rect> = others.iter().flat_map(|o| o.built_bodies.clone()).collect();
        let (ring, runs) = wall_runs(&plan.centre_outline, &covers, &bodies, z);

        // the doors and windows on this room's walls, before they go
        let old_walls: Vec<Id> = self.groups.iter().filter(|(_, g)| g.parent == Some(tid)).map(|(&k, _)| k).collect();
        let mut openings: Vec<(Id, std::sync::Arc<Model>, DMat4)> = Vec::new();
        for (&id, inst) in &self.instances {
            let Some(def) = self.definitions.get(&inst.def).filter(|d| d.cuts) else { continue };
            let Some(back) = crate::openings::back_outline(&def.model) else { continue };
            let world: Vec<DVec3> = back.iter().map(|p| inst.transform.transform_point3(*p)).collect();
            if self.wall_face_for(&world, &old_walls).is_some() {
                openings.push((id, def.model.clone(), inst.transform));
            }
        }

        // out with the old walls and floor (and the floor's loose edges)
        self.delete(&old_walls)?;
        self.faces.remove(&info.floor);
        let loose: Vec<Id> = self.edges.iter().filter(|(_, e)| e.group == Some(tid) && !self.faces.values().any(|f| f.uses_edge(e.a, e.b))).map(|(&k, _)| k).collect();
        for k in loose {
            self.edges.remove(&k);
        }
        self.gc();

        // in with the new
        let prev = self.active_group;
        self.active_group = Some(tid);
        let built = (|| -> Result<(Vec<Id>, Id)> {
            let walls = self.build_walls(&plan.centre_outline, ring, &runs, t, height, z)?;
            let floor = self.build_floor(&plan.outline, z)?;
            Ok((walls, floor))
        })();
        self.active_group = prev;
        let (walls, floor) = built?;
        if !walls.is_empty() {
            self.set_tag(&walls, Some(WALLS_TAG.to_string()))?;
        }

        // doors and windows: back into a wall that is still there, or gone with it
        let (mut kept, mut removed) = (0, 0);
        for (id, def, transform) in openings {
            let back = crate::openings::back_outline(&def).unwrap_or_default();
            let world: Vec<DVec3> = back.iter().map(|p| transform.transform_point3(*p)).collect();
            let ok = self.wall_face_for(&world, &walls).is_some_and(|host| crate::openings::cut(self, &def, transform, host).is_ok());
            if ok {
                kept += 1;
            } else {
                self.delete(&[id])?;
                removed += 1;
            }
        }
        Ok(RoomBuilt { room: tid, plan, walls, floor, kept, removed })
    }

    /// The upright face in `groups` that an opening's back outline (world points) lies on and within, if any.
    fn wall_face_for(&self, world: &[DVec3], groups: &[Id]) -> Option<Id> {
        if world.is_empty() {
            return None;
        }
        self.faces
            .iter()
            .filter(|(_, f)| f.group.is_some_and(|g| groups.contains(&g)) && f.outer.len() >= 3)
            .find(|(fid, f)| {
                let fid = **fid;
                let n = self.normal(fid);
                if n.z.abs() > 1e-4 {
                    return false;
                }
                let pts = self.pts(&f.outer);
                let w = n.dot(pts[0]);
                if !world.iter().all(|p| (n.dot(*p) - w).abs() < 0.03) {
                    return false;
                }
                let along = DVec3::new(-n.y, n.x, 0.0);
                let ext = |g: &dyn Fn(&DVec3) -> f64, v: &[DVec3]| v.iter().fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(g(p)), h.max(g(p))));
                let (a0, a1) = ext(&|p| p.dot(along), &pts);
                let (z0, z1) = ext(&|p| p.z, &pts);
                world.iter().all(|p| p.dot(along) >= a0 - 1e-4 && p.dot(along) <= a1 + 1e-4 && p.z >= z0 - 1e-4 && p.z <= z1 + 1e-4)
            })
            .map(|(&k, _)| k)
    }
}

/// Is the inside a real shape: same corners as the centre line (none flipped by a too-thin part), no edge shorter than
/// the smallest wall, and nowhere narrower than the smallest room?
fn inside_ok(centre: &[Pt], inside: &[Pt]) -> bool {
    if centre.len() != inside.len() || inside.len() < 4 || rp::signed_area(inside) <= 0.0 {
        return false;
    }
    let (ce, ie) = (rp::edges(centre), rp::edges(inside));
    if !ce.iter().zip(&ie).all(|(c, i)| c.horizontal == i.horizontal && c.fwd == i.fwd && i.hi - i.lo >= MIN_INSIDE - EPS) {
        return false;
    }
    // two facing edges closer than the smallest room, with room between them
    for e in &ie {
        for f in &ie {
            // e is a low side (the inside lies above / east of it) and f a high side (the inside lies below / west of it)
            let low = |x: &Edge| if x.horizontal { x.fwd } else { !x.fwd };
            if e.horizontal != f.horizontal || !low(e) || low(f) {
                continue;
            }
            let gap = f.v - e.v;
            let (lo, hi) = (e.lo.max(f.lo), e.hi.min(f.hi));
            if gap > EPS && gap < MIN_INSIDE - EPS && hi - lo > 1e-3 {
                let mid = (e.v + f.v) / 2.0;
                let c = (lo + hi) / 2.0;
                if rp::contains(inside, if e.horizontal { [c, mid] } else { [mid, c] }) {
                    return false;
                }
            }
        }
    }
    true
}

/// The new walls around a centre-line outline: which stretches still need a wall (the rest lies along a neighbour's
/// wall, in `covers`), chained into one closed ring when nothing is covered, or into open runs that stop at the
/// neighbours' wall faces (`bodies`). Returns (ring, runs).
fn wall_runs(centre: &[Pt], covers: &[Edge], bodies: &[Rect], z: f64) -> (bool, Vec<Vec<DVec3>>) {
    let ne = rp::edges(centre);
    let point = |e: &Edge, s: f64| -> DVec3 {
        if e.horizontal {
            DVec3::new(s, e.v, z)
        } else {
            DVec3::new(e.v, s, z)
        }
    };
    let mut pieces: Vec<(DVec3, DVec3)> = Vec::new();
    for e in &ne {
        let covered: Vec<(f64, f64)> = covers.iter().filter(|c| collinear(e, c)).map(|c| (c.lo.max(e.lo), c.hi.min(e.hi))).collect();
        for (s, f) in gaps(e.lo, e.hi, covered) {
            // a side that runs toward smaller coordinates is walked from its high end
            pieces.push(if e.fwd { (point(e, s), point(e, f)) } else { (point(e, f), point(e, s)) });
        }
    }
    // chain the new stretches around the room into runs (a full ring when nothing is shared)
    let m = pieces.len();
    let ring = m == ne.len() && (0..m).all(|k| pieces[k].1.distance(pieces[(k + 1) % m].0) < 1e-6);
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
    let exit = |p: DVec3, d: DVec3| -> f64 {
        let mut s = 0.0f64;
        loop {
            let q = p + d * s;
            let Some(w) = bodies.iter().find(|w| q.x > w[0] + 1e-6 && q.x < w[2] - 1e-6 && q.y > w[1] + 1e-6 && q.y < w[3] - 1e-6) else { return s };
            let ahead = if d.x > 0.5 {
                w[2] - p.x
            } else if d.x < -0.5 {
                p.x - w[0]
            } else if d.y > 0.5 {
                w[3] - p.y
            } else {
                p.y - w[1]
            };
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
    (ring, trimmed)
}
