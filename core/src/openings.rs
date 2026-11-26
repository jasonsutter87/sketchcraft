//! Components that cut openings (SketchUp's "cut opening"): a window or door
//! dropped on a wall makes a hole in it, and in a thick wall goes through to
//! the wall's other face, with reveals (the sides of the opening) between.
//!
//! The opening's outline is the component's back: the biggest face lying on
//! one of its own base planes (x, y or z = 0), placed in the world. Cutting
//! draws that outline on the wall the way the Line tool would (which splits
//! the face, or notches it where the outline meets the wall's edge) and
//! removes the piece inside.

use anyhow::{bail, Result};
use glam::{DMat4, DVec2, DVec3};

use crate::geom;
use crate::model::{Id, Model};

/// Does the segment a-b lie along one of the edges of the closed outline `rim` (within 0.1 mm)?
fn along_rim(a: DVec3, b: DVec3, rim: &[DVec3]) -> bool {
    let on = |p: DVec3, s: DVec3, e: DVec3| {
        let d = e - s;
        let len2 = d.length_squared();
        if len2 < 1e-18 {
            return false;
        }
        let t = (p - s).dot(d) / len2;
        (-1e-6..=1.0 + 1e-6).contains(&t) && (s + d * t).distance(p) < 1e-4
    };
    (0..rim.len()).any(|k| {
        let (s, e) = (rim[k], rim[(k + 1) % rim.len()]);
        on(a, s, e) && on(b, s, e)
    })
}

/// The back outline of a component's geometry, in its own coordinates.
pub fn back_outline(def: &Model) -> Option<Vec<DVec3>> {
    let mut best: Option<(f64, Vec<DVec3>)> = None;
    for f in def.faces.values() {
        let pts: Vec<DVec3> = f.outer.iter().map(|v| def.p(*v)).collect();
        if pts.len() < 3 {
            continue;
        }
        // on x = 0, y = 0 or z = 0?
        let on = (0..3).any(|k| pts.iter().all(|p| p[k].abs() < 1e-6));
        if !on {
            continue;
        }
        let a = geom::area(&pts);
        if best.as_ref().is_none_or(|(b, _)| a > *b) {
            best = Some((a, pts));
        }
    }
    best.map(|(_, p)| p)
}

fn inside(p: DVec2, ring: &[DVec2]) -> bool {
    let mut c = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            c = !c;
        }
    }
    c
}

/// Draw `outline` on the faces of its plane in `group`, then delete the
/// pieces now inside it. Returns how many were removed.
pub(crate) fn cut_on_plane(m: &mut Model, outline: &[DVec3], n: DVec3, group: Option<Id>) -> Result<usize> {
    let keep = m.active_group;
    m.active_group = group;
    let r = (|| {
        for i in 0..outline.len() {
            m.draw_line(outline[i], outline[(i + 1) % outline.len()])?;
        }
        let (u, v) = geom::basis(n);
        let to2 = |p: DVec3| DVec2::new(p.dot(u), p.dot(v));
        let ring: Vec<DVec2> = outline.iter().map(|p| to2(*p)).collect();
        let w = n.dot(outline[0]);
        // faces on this plane whose middle is inside the outline, and that
        // don't reach outside it
        let dead: Vec<Id> = m
            .faces
            .iter()
            .filter(|(_, f)| f.group == group)
            .filter(|(id, f)| {
                let pts: Vec<DVec3> = f.outer.iter().map(|x| m.p(*x)).collect();
                pts.iter().all(|p| (p.dot(n) - w).abs() < 1e-5) && m.normal(**id).dot(n).abs() > 1.0 - 1e-6 && {
                    let c = to2(geom::centroid(&pts));
                    inside(c, &ring) && pts.iter().all(|p| {
                        let q = to2(*p);
                        // on or inside (corners of the outline sit on its edge)
                        inside(q, &ring) || ring.iter().zip(ring.iter().cycle().skip(1)).any(|(a, b)| {
                            let d = *b - *a;
                            let t = ((q - *a).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                            (*a + d * t).distance(q) < 1e-5
                        })
                    })
                }
            })
            .map(|(id, _)| *id)
            .collect();
        for id in &dead {
            m.faces.remove(id);
        }
        Ok(dead.len())
    })();
    m.active_group = keep;
    r
}

/// Cut the opening for a component placed with `transform` against face
/// `host`; through the wall when the face behind is near and parallel.
/// Returns how many faces were opened (1, or 2 through a thick wall).
pub fn cut(m: &mut Model, def: &Model, transform: DMat4, host: Id) -> Result<usize> {
    // a door or window in a remembered wall is part of it, not a hand edit: the wall can still be rebuilt (#263)
    let group = m.faces.get(&host).and_then(|f| f.group);
    let fresh = group.and_then(|g| m.groups.get(&g)?.recipe.as_ref().map(|r| (g, glam::DMat4::from_cols_array(&r.frame), r.print))).filter(|&(g, frame, print)| m.part_print(g, frame) == print);
    let r = cut_into(m, def, transform, host);
    if let (Ok(_), Some((g, frame, _))) = (&r, fresh) {
        let print = m.part_print(g, frame);
        if let Some(r) = m.groups.get_mut(&g).and_then(|g| g.recipe.as_mut()) {
            r.print = print;
        }
    }
    r
}

fn cut_into(m: &mut Model, def: &Model, transform: DMat4, host: Id) -> Result<usize> {
    let Some(local) = back_outline(def) else { bail!("this component has no flat back to cut an opening with") };
    let f = m.faces.get(&host).cloned().ok_or_else(|| anyhow::anyhow!("no face {host}"))?;
    let n = m.normal(host);
    let w = n.dot(m.p(f.outer[0]));
    let mut outline: Vec<DVec3> = local.iter().map(|p| transform.transform_point3(*p)).collect();
    // the back sits on the wall: flatten it onto the face's plane (it may float a hair off)
    if outline.iter().any(|p| (p.dot(n) - w).abs() > 0.02) {
        bail!("the component's back isn't on that face");
    }
    for p in &mut outline {
        *p -= n * (p.dot(n) - w);
    }
    // the wall face's own outline, before the cut: an opening edge lying along it (a door's foot) gets no reveal
    let rim = m.pts(&f.outer);
    let group = f.group;
    let mut opened = cut_on_plane(m, &outline, n, group)?;
    if opened == 0 {
        bail!("the opening doesn't fit on that face");
    }
    // a thick wall: the parallel face behind, within 60 cm, that the opening's middle crosses
    let c = geom::centroid(&outline);
    let (u, v) = geom::basis(n);
    let behind = m
        .faces
        .iter()
        .filter(|(_, g)| g.group == group)
        .filter_map(|(id, g)| {
            let gn = m.normal(*id);
            if gn.dot(n) > -1.0 + 1e-6 {
                return None;
            }
            let d = w - n.dot(m.p(g.outer[0])); // how far back
            if !(0.005..=0.6).contains(&d) {
                return None;
            }
            let ring: Vec<DVec2> = g.outer.iter().map(|x| DVec2::new(m.p(*x).dot(u), m.p(*x).dot(v))).collect();
            inside(DVec2::new(c.dot(u), c.dot(v)), &ring).then_some(d)
        })
        .fold(None, |a: Option<f64>, d| Some(a.map_or(d, |a| a.min(d))));
    if let Some(d) = behind {
        let back: Vec<DVec3> = outline.iter().map(|p| *p - n * d).collect();
        opened += cut_on_plane(m, &back, -n, group)?;
        // reveals: a side face for each edge of the opening, facing into it
        let keep = m.active_group;
        m.active_group = group;
        for i in 0..outline.len() {
            let j = (i + 1) % outline.len();
            let quad = [outline[i], outline[j], back[j], back[i]];
            // along the wall's own edge (a door's foot on the floor) there is no side to fill: the quad lies on the
            // wall's bottom face, so drawing it splits that face, and the piece across the doorway goes (kept as a
            // reveal it lay across the opening and left the wall with open edges)
            if along_rim(outline[i], outline[j], &rim) {
                if let Ok(id) = m.add_polygon(&quad) {
                    m.faces.remove(&id);
                }
                continue;
            }
            if let Ok(id) = m.add_polygon(&quad) {
                let mid = (quad[0] + quad[1] + quad[2] + quad[3]) / 4.0;
                if m.normal(id).dot(c - n * (d / 2.0) - mid) < 0.0 {
                    if let Some(face) = m.faces.get_mut(&id) {
                        face.reverse();
                    }
                }
            }
        }
        m.active_group = keep;
    }
    Ok(opened)
}
