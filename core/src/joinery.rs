//! Joinery helpers: dado, rabbet, tenon with its mortise (docs/dev/woodworker.md, "CNC and joinery").
//!
//! Each works on two boards that touch. The board's own frame is `plugins::measure`'s (length, width, thickness), so
//! "depth" and "width" mean the same thing here as in the cut list. The cut is a boolean (`csg`), the cut board keeps its
//! name, tag and place in the hierarchy, and the whole thing is one undo step. A tenon makes the board longer by the
//! tenon, and the cut list reports that longer size, which is the length to cut.

use crate::csg::{self, Op, Poly};
use crate::model::{Id, Model};
use crate::plugins::{measure, Measured};
use anyhow::{anyhow, bail, Result};
use glam::DVec3;
use serde_json::{json, Value};

/// Boards closer than this (meters) touch: half a millimeter.
const TOUCH: f64 = 5e-4;
/// A cutter reaches this far past a face it opens through, so the cut never shares a face with the board.
const OVER: f64 = 1e-3;
/// A cut must leave at least this much of the board (meters).
const LEAVE: f64 = 1e-3;

/// Where two boards meet, in the first board's frame.
struct Contact {
    a: Measured,
    b: Measured,
    /// the axis (0 length, 1 width, 2 thickness) of the face B rests on, and whether it is the far face
    k: usize,
    far: bool,
    /// B's reach on the other two axes where it overlaps A's face: (axis, lo, hi)
    f: [(usize, f64, f64); 2],
}

fn axis(a: &Measured, k: usize) -> DVec3 {
    [a.u, a.w, a.n][k]
}

fn extent(a: &Measured, k: usize) -> f64 {
    a.extent.to_array()[k]
}

/// A point in a board's frame (`x` along its length, `y` across, `z` through) in the world.
fn at(a: &Measured, p: [f64; 3]) -> DVec3 {
    a.origin + a.u * p[0] + a.w * p[1] + a.n * p[2]
}

/// The corners of every face of a group or component, in the world.
fn points_of(m: &Model, id: Id) -> Vec<DVec3> {
    let world = m.world_faces();
    if m.groups.contains_key(&id) {
        let tree = m.subtree(id);
        world.iter().filter(|f| f.face.and_then(|fid| m.faces.get(&fid)).and_then(|x| x.group).is_some_and(|x| tree.contains(&x))).flat_map(|f| f.outer.clone()).collect()
    } else {
        world.iter().filter(|f| f.owner == id).flat_map(|f| f.outer.clone()).collect()
    }
}

fn board(m: &Model, id: Id) -> Result<Measured> {
    if !m.groups.contains_key(&id) && !m.instances.contains_key(&id) {
        bail!("{id} is not a group or component");
    }
    measure(m, &[id]).into_iter().next().ok_or_else(|| anyhow!("{} is not a board: it needs faces with some thickness", csg::name_of(m, id)))
}

fn contact(m: &Model, cut: Id, fit: Id) -> Result<Contact> {
    if cut == fit {
        bail!("pick two different boards");
    }
    let (a, b) = (board(m, cut)?, board(m, fit)?);
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for p in points_of(m, fit) {
        let q = DVec3::new((p - a.origin).dot(a.u), (p - a.origin).dot(a.w), (p - a.origin).dot(a.n));
        lo = lo.min(q);
        hi = hi.max(q);
    }
    let (lo, hi) = (lo.to_array(), hi.to_array());
    let mut best: Option<(f64, Contact)> = None;
    // boxes that share volume overlap instead of touching
    let overlapping = (0..3).all(|i| lo[i] < extent(&a, i) - TOUCH && hi[i] > TOUCH);
    for k in 0..3 {
        let e = extent(&a, k);
        let others: Vec<usize> = (0..3).filter(|&i| i != k).collect();
        let span = |i: usize| (lo[i].max(0.0), hi[i].min(extent(&a, i)));
        let (s0, s1) = (span(others[0]), span(others[1]));
        let (w0, w1) = (s0.1 - s0.0, s1.1 - s1.0);
        if w0 <= TOUCH || w1 <= TOUCH {
            continue;
        }
        for far in [false, true] {
            let gap = if far { lo[k] - e } else { -hi[k] };
            if gap.abs() <= TOUCH && best.as_ref().is_none_or(|(area, _)| w0 * w1 > *area) {
                best = Some((w0 * w1, Contact { a: a.clone(), b: b.clone(), k, far, f: [(others[0], s0.0, s0.1), (others[1], s1.0, s1.1)] }));
            }
        }
    }
    match best {
        Some((_, c)) => Ok(c),
        None if overlapping => bail!("{} and {} overlap instead of touching: move one of them so a face of {} rests on a face of the other", csg::name_of(m, cut), csg::name_of(m, fit), csg::name_of(m, fit)),
        None => bail!("{} and {} don't touch: a face of {} must rest on a face of {} (within half a millimeter)", csg::name_of(m, cut), csg::name_of(m, fit), csg::name_of(m, fit), csg::name_of(m, cut)),
    }
}

/// A box in a board's frame as a closed solid.
fn box_in(a: &Measured, lo: [f64; 3], hi: [f64; 3]) -> Vec<Poly> {
    let p = |x: f64, y: f64| at(a, [x, y, lo[2]]);
    let profile = [p(lo[0], lo[1]), p(hi[0], lo[1]), p(hi[0], hi[1]), p(lo[0], hi[1])];
    csg::prism(&profile, a.n * (hi[2] - lo[2]), None)
}

/// A cutter that sits on the contact face: `inside` is how deep, along the face's axis, and `over` the extra the other way.
fn cutter(c: &Contact, along: [(usize, f64, f64); 2], depth: f64) -> Vec<Poly> {
    let e = extent(&c.a, c.k);
    let (mut lo, mut hi) = ([0.0; 3], [0.0; 3]);
    for (i, l, h) in along {
        lo[i] = l;
        hi[i] = h;
    }
    (lo[c.k], hi[c.k]) = if c.far { (e - depth, e + OVER) } else { (-OVER, depth) };
    box_in(&c.a, lo, hi)
}

fn positive(what: &str, v: Option<f64>) -> Result<Option<f64>> {
    match v {
        Some(x) if !x.is_finite() || x <= 0.0 || x > 10.0 => bail!("{what} must be more than 0 and at most 10 meters"),
        v => Ok(v),
    }
}

fn clearance(v: Option<f64>) -> Result<f64> {
    match v {
        Some(x) if !x.is_finite() || !(0.0..=0.01).contains(&x) => bail!("the clearance must be between 0 and 10 mm"),
        v => Ok(v.unwrap_or(0.0)),
    }
}

/// Swap a board's solid for new polygons: same name, tag, parent and children as before.
fn replace(m: &mut Model, target: Id, polys: &[Poly]) -> Result<Id> {
    let name = csg::name_of(m, target);
    let new = csg::to_group(m, polys, &name)?;
    if let Some(group) = m.groups.get(&target).cloned() {
        m.groups.insert(new, group);
        for child in m.groups.values_mut().filter(|g| g.parent == Some(target)) {
            child.parent = Some(new);
        }
        for child in m.instances.values_mut().filter(|i| i.parent == Some(target)) {
            child.parent = Some(new);
        }
    } else if let Some(instance) = m.instances.get(&target) {
        let (parent, tag) = (instance.parent, instance.tag.clone());
        let g = m.groups.get_mut(&new).ok_or_else(|| anyhow!("the new board is missing"))?;
        g.parent = parent;
        g.tag = tag;
    }
    if m.active_group == Some(target) {
        m.active_group = Some(new);
    }
    m.delete(&[target])?;
    Ok(new)
}

/// Take `cutter` out of `target`; refuses a cutter that misses it, or one that would cut it in two.
fn subtract(m: &mut Model, target: Id, cutter: Vec<Poly>) -> Result<(Id, f64)> {
    let before = csg::solid_polys(m, target)?;
    let v0 = csg::volume(&before);
    let after = csg::boolean_for(m, before, cutter, Op::Subtract)?;
    let v1 = csg::volume(&after);
    if !v1.is_finite() || v1 < 0.0 || v1 > v0 + v0.abs() * 1e-9 {
        bail!("the cut could not be made");
    }
    if v0 - v1 <= v0.abs() * 1e-9 {
        bail!("the cut misses the board: nothing was removed");
    }
    let id = replace(m, target, &after)?;
    Ok((id, v0 - v1))
}

fn size_of(m: &Model, id: Id) -> Value {
    measure(m, &[id]).first().map(|p| json!(p.size)).unwrap_or(Value::Null)
}

fn reply(m: &Model, kind: &str, cut: Id, fit: Id, removed: f64, extra: Value) -> Value {
    let mut v = json!({"joint": kind, "cut": cut, "fit": fit, "removed_volume": removed, "cut_size": size_of(m, cut), "fit_size": size_of(m, fit)});
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    v
}

/// The footprint's short and long axes: (short axis, its lo, hi, long axis, its lo, hi).
fn footprint(c: &Contact) -> ((usize, f64, f64), (usize, f64, f64)) {
    let [p, q] = c.f;
    if p.2 - p.1 <= q.2 - q.1 {
        (p, q)
    } else {
        (q, p)
    }
}

/// The board must stand on an end or an edge: what touches is as thin as the board.
fn standing(m: &Model, c: &Contact, cut: Id, fit: Id, what: &str) -> Result<()> {
    let (s, _) = footprint(c);
    if s.2 - s.1 > c.b.size[2] + 2.0 * TOUCH + 1e-3 * c.b.size[2] {
        bail!(
            "{} lies flat on {}: a {what} needs the second board to stand on an edge or an end, with only its thickness ({:.1} mm) touching",
            csg::name_of(m, fit),
            csg::name_of(m, cut),
            c.b.size[2] * 1000.0
        );
    }
    Ok(())
}

/// A groove across the face of `cut`, as wide as the board `fit` is thick, where it stands on it.
pub fn dado(m: &mut Model, cut: Id, fit: Id, depth: Option<f64>, width: Option<f64>, clear: Option<f64>) -> Result<Value> {
    let (depth, width, clear) = (positive("the depth", depth)?, positive("the width", width)?, clearance(clear)?);
    m.check_unlocked(&[cut])?;
    let c = contact(m, cut, fit)?;
    standing(m, &c, cut, fit, "dado")?;
    let ((si, s0, s1), (li, l0, l1)) = footprint(&c);
    let depth = depth.unwrap_or(extent(&c.a, c.k) / 3.0);
    if depth > extent(&c.a, c.k) - LEAVE {
        bail!("the depth ({:.1} mm) must be less than the board's thickness there ({:.1} mm), or the groove goes through", depth * 1000.0, extent(&c.a, c.k) * 1000.0);
    }
    let w = width.unwrap_or(c.b.size[2]) + clear;
    let mid = (s0 + s1) / 2.0;
    let (g0, g1) = (mid - w / 2.0, mid + w / 2.0);
    if g0 < TOUCH || g1 > extent(&c.a, si) - TOUCH {
        bail!("the groove would reach the edge of {}: use a rabbet there", csg::name_of(m, cut));
    }
    // along the groove: where the board is, and past the edge when it reaches it
    let past = |v: f64, edge: f64, sign: f64| if (v - edge).abs() <= TOUCH { edge + sign * OVER } else { v };
    let (a0, a1) = (past(l0, 0.0, -1.0), past(l1, extent(&c.a, li), 1.0));
    let (id, removed) = subtract(m, cut, cutter(&c, [(si, g0, g1), (li, a0, a1)], depth))?;
    Ok(reply(m, "dado", id, fit, removed, json!({"depth": depth, "width": w})))
}

/// A step along the edge of `cut`, from the edge to the far side of the board `fit`, which stands on it near that edge.
pub fn rabbet(m: &mut Model, cut: Id, fit: Id, depth: Option<f64>, width: Option<f64>, clear: Option<f64>) -> Result<Value> {
    let (depth, width, clear) = (positive("the depth", depth)?, positive("the width", width)?, clearance(clear)?);
    m.check_unlocked(&[cut])?;
    let c = contact(m, cut, fit)?;
    standing(m, &c, cut, fit, "rabbet")?;
    let ((si, s0, s1), (li, l0, l1)) = footprint(&c);
    let e_s = extent(&c.a, si);
    let depth = depth.unwrap_or(extent(&c.a, c.k) / 2.0);
    if depth > extent(&c.a, c.k) - LEAVE {
        bail!("the depth ({:.1} mm) must be less than the board's thickness there ({:.1} mm)", depth * 1000.0, extent(&c.a, c.k) * 1000.0);
    }
    // the nearer edge across the face, and how far the board's far side is from it
    let from_start = s0 <= e_s - s1;
    let reach = if from_start { s1 } else { e_s - s0 };
    let w = width.unwrap_or(reach) + if width.is_some() { 0.0 } else { clear };
    if w > e_s - LEAVE {
        bail!("the width ({:.1} mm) must be less than the board's width there ({:.1} mm)", w * 1000.0, e_s * 1000.0);
    }
    let across = if from_start { (-OVER, w) } else { (e_s - w, e_s + OVER) };
    let past = |v: f64, edge: f64, sign: f64| if (v - edge).abs() <= TOUCH { edge + sign * OVER } else { v };
    let (a0, a1) = (past(l0, 0.0, -1.0), past(l1, extent(&c.a, li), 1.0));
    let (id, removed) = subtract(m, cut, cutter(&c, [(si, across.0, across.1), (li, a0, a1)], depth))?;
    Ok(reply(m, "rabbet", id, fit, removed, json!({"depth": depth, "width": w})))
}

/// A tenon on the end of `fit` and the mortise for it in `cut`. `length` is how far it reaches into `cut`, `thickness`
/// its thickness (default a third of the board's), `shoulder` how far in from each edge of the board's width it starts
/// (default a sixth of the width), `clear` how much room the mortise leaves all round.
pub fn tenon(m: &mut Model, cut: Id, fit: Id, length: Option<f64>, thickness: Option<f64>, shoulder: Option<f64>, clear: Option<f64>) -> Result<Value> {
    let (length, thickness) = (positive("the length", length)?, positive("the thickness", thickness)?);
    if shoulder.is_some_and(|s| !s.is_finite() || !(0.0..=10.0).contains(&s)) {
        bail!("the shoulder must be 0 or more, and at most 10 meters");
    }
    let clear = clearance(clear)?;
    m.check_unlocked(&[cut, fit])?;
    let c = contact(m, cut, fit)?;
    let b = &c.b;
    // the tenon board meets the other with its end: its length runs through the face
    let through = axis(&c.a, c.k);
    if b.u.dot(through).abs() < 1.0 - 1e-3 {
        bail!("{} must meet {} with its end (its length square to the face it touches) for a tenon", csg::name_of(m, fit), csg::name_of(m, cut));
    }
    let depth_room = extent(&c.a, c.k);
    let len = length.unwrap_or((depth_room * 0.6).min(0.04));
    if len + clear > depth_room - LEAVE {
        bail!("the tenon ({:.1} mm) is too long for {}, which is {:.1} mm thick there: it must stop short of coming through", len * 1000.0, csg::name_of(m, cut), depth_room * 1000.0);
    }
    let tt = thickness.unwrap_or(b.extent.z / 3.0);
    let sh = shoulder.unwrap_or(b.extent.y / 6.0);
    if tt >= b.extent.z - 1e-4 {
        bail!("the tenon's thickness ({:.1} mm) must be less than the board's ({:.1} mm)", tt * 1000.0, b.extent.z * 1000.0);
    }
    if 2.0 * sh >= b.extent.y - 1e-3 {
        bail!("the shoulders ({:.1} mm each) leave nothing of the board's {:.1} mm width", sh * 1000.0, b.extent.y * 1000.0);
    }
    // which end of the board is the one that touches
    let plane_at_end = |x: f64| {
        let p = at(b, [x, 0.0, 0.0]);
        let d = (p - c.a.origin).dot(through);
        (if c.far { extent(&c.a, c.k) - d } else { d }).abs()
    };
    let at_far_end = plane_at_end(b.extent.x) < plane_at_end(0.0);
    if plane_at_end(if at_far_end { b.extent.x } else { 0.0 }).abs() > TOUCH {
        bail!("the end of {} doesn't touch {}: a tenon board must have its end on the face", csg::name_of(m, fit), csg::name_of(m, cut));
    }
    let (z0, z1) = ((b.extent.z - tt) / 2.0, (b.extent.z + tt) / 2.0);
    let (y0, y1) = (sh, b.extent.y - sh);
    let tenon_x = if at_far_end { (b.extent.x - OVER, b.extent.x + len) } else { (-len, OVER) };
    let tenon_box = box_in(b, [tenon_x.0, y0, z0], [tenon_x.1, y1, z1]);
    // the mortise: the tenon's section grown by the clearance, in the face of `cut`, `len + clear` deep
    let world = |x: f64, y: f64, z: f64| at(b, [x, y, z]);
    let corners: Vec<DVec3> = [(0.0, y0 - clear, z0 - clear), (0.0, y1 + clear, z1 + clear)].iter().map(|&(x, y, z)| world(x, y, z)).collect();
    let local = |p: DVec3| [(p - c.a.origin).dot(c.a.u), (p - c.a.origin).dot(c.a.w), (p - c.a.origin).dot(c.a.n)];
    let (p, q) = (local(corners[0]), local(corners[1]));
    let mut along = [(0usize, 0.0, 0.0); 2];
    for (slot, i) in (0..3).filter(|&i| i != c.k).enumerate() {
        let (l, h) = (p[i].min(q[i]), p[i].max(q[i]));
        if l < LEAVE || h > extent(&c.a, i) - LEAVE {
            bail!("the tenon would reach the edge of {}: make it thinner, or give it a bigger shoulder", csg::name_of(m, cut));
        }
        along[slot] = (i, l, h);
    }
    let mortise = cutter(&c, along, len + clear);
    // the two solids
    let (cut_id, removed) = subtract(m, cut, mortise)?;
    let fit_polys = csg::solid_polys(m, fit)?;
    let grown = csg::boolean_for(m, fit_polys, tenon_box, Op::Union)?;
    let fit_id = replace(m, fit, &grown)?;
    Ok(reply(m, "tenon", cut_id, fit_id, removed, json!({"length": len, "thickness": tt, "shoulder": sh, "clearance": clear})))
}
