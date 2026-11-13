//! Section cuts: where a plane slices the model. Each group or component
//! gets its cut outline (closed loops, holes nested inside) and a fill, the
//! way SketchUp's section fill shows a cut wall solid. Exported flat, in
//! the plane, as SVG or DXF (millimeters).

use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

pub struct Cut {
    /// the plane's own axes (u right, v up when seen from the front)
    pub u: DVec3,
    pub v: DVec3,
    pub n: DVec3,
    pub origin: DVec3,
    /// every cut segment
    pub segments: Vec<(DVec3, DVec3)>,
    /// per owner (group / instance / loose face): outlines with holes
    pub regions: Vec<(Id, Vec<DVec3>, Vec<Vec<DVec3>>)>,
}

fn key(p: DVec3) -> (i64, i64, i64) {
    ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64)
}

impl Model {
    /// Slice the visible model with the plane through `origin` facing `normal`.
    pub fn section(&self, origin: DVec3, normal: DVec3) -> Result<Cut> {
        if normal.length_squared() < 1e-18 {
            bail!("the section plane needs a direction");
        }
        let n = normal.normalize();
        // how the cut lies on paper: plans north-up (+X right, +Y up);
        // elevations and side cuts with up as up
        let (u, v) = if n.z.abs() > 0.9 {
            (DVec3::X, DVec3::Y)
        } else {
            let right = DVec3::Z.cross(n).normalize();
            (right, DVec3::Z)
        };
        let w = n.dot(origin);
        let mut by_owner: HashMap<Id, Vec<(DVec3, DVec3)>> = HashMap::new();
        let mut segments = Vec::new();
        for f in self.world_faces() {
            // the plane against each face: a line in the face, clipped to it
            let fn_ = f.normal.normalize_or_zero();
            let d = fn_.cross(n);
            if d.length_squared() < 1e-12 {
                continue; // face lies along the plane
            }
            // quick reject: all corners on one side
            let side = |p: &DVec3| n.dot(*p) - w;
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for p in &f.outer {
                lo = lo.min(side(p));
                hi = hi.max(side(p));
            }
            if lo > 1e-9 || hi < -1e-9 {
                continue;
            }
            for seg in crate::intersect::plane_crossing(&f, n, w) {
                segments.push(seg);
                by_owner.entry(f.group.unwrap_or(f.owner)).or_default().push(seg);
            }
        }
        // chain each owner's segments into loops, then nest them
        let mut regions = Vec::new();
        let mut owners: Vec<Id> = by_owner.keys().copied().collect();
        owners.sort_unstable();
        for owner in owners {
            let segs = &by_owner[&owner];
            let mut adj: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
            for (i, (a, b)) in segs.iter().enumerate() {
                adj.entry(key(*a)).or_default().push(i);
                adj.entry(key(*b)).or_default().push(i);
            }
            let mut used = vec![false; segs.len()];
            let mut loops: Vec<Vec<DVec3>> = Vec::new();
            for s0 in 0..segs.len() {
                if used[s0] {
                    continue;
                }
                used[s0] = true;
                let (start, mut cur) = (segs[s0].0, segs[s0].1);
                let mut pts = vec![start];
                let mut closed = false;
                for _ in 0..segs.len() {
                    if key(cur) == key(start) {
                        closed = true;
                        break;
                    }
                    pts.push(cur);
                    let next = adj.get(&key(cur)).and_then(|l| l.iter().copied().find(|&i| !used[i]));
                    let Some(i) = next else { break };
                    used[i] = true;
                    cur = if key(segs[i].0) == key(cur) { segs[i].1 } else { segs[i].0 };
                }
                if closed && pts.len() >= 3 && geom::area(&pts) > 1e-10 {
                    loops.push(pts);
                }
            }
            // nesting depth decides outline (even) or hole (odd)
            let flat: Vec<Vec<DVec2>> = loops.iter().map(|l| l.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect()).collect();
            let inside = |i: usize, j: usize| geom::inside(flat[i][0], &flat[j]) || geom::inside((flat[i][0] + flat[i][1]) / 2.0, &flat[j]);
            let depth: Vec<usize> = (0..loops.len()).map(|i| (0..loops.len()).filter(|&j| j != i && inside(i, j)).count()).collect();
            let mut outs: Vec<(usize, Vec<DVec3>, Vec<Vec<DVec3>>)> = Vec::new();
            for i in (0..loops.len()).filter(|i| depth[*i] % 2 == 0) {
                let mut o = loops[i].clone();
                if geom::newell(&o).dot(n) < 0.0 {
                    o.reverse();
                }
                outs.push((i, o, vec![]));
            }
            for i in (0..loops.len()).filter(|i| depth[*i] % 2 == 1) {
                // the hole belongs to the deepest outline around it
                if let Some(k) = outs.iter().enumerate().filter(|(_, (j, _, _))| inside(i, *j)).max_by_key(|(_, (j, _, _))| depth[*j]).map(|(k, _)| k) {
                    let mut h = loops[i].clone();
                    if geom::newell(&h).dot(n) > 0.0 {
                        h.reverse();
                    }
                    outs[k].2.push(h);
                }
            }
            regions.extend(outs.into_iter().map(|(_, o, h)| (owner, o, h)));
        }
        Ok(Cut { u, v, n, origin, segments, regions })
    }
}

impl Cut {
    fn flat(&self, p: DVec3) -> DVec2 {
        let q = p - self.origin;
        DVec2::new(q.dot(self.u), q.dot(self.v)) * 1000.0 // mm
    }

    fn bounds(&self) -> (DVec2, DVec2) {
        let pts: Vec<DVec2> = self.segments.iter().flat_map(|(a, b)| [self.flat(*a), self.flat(*b)]).collect();
        if pts.is_empty() {
            return (DVec2::ZERO, DVec2::ONE);
        }
        pts.iter().fold((pts[0], pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
    }

    /// The cut as SVG: filled regions (holes cut out) with outlines, in mm.
    pub fn svg(&self) -> String {
        let (lo, hi) = self.bounds();
        let pad = 20.0;
        let (w, h) = (hi.x - lo.x + 2.0 * pad, hi.y - lo.y + 2.0 * pad);
        let tx = |p: DVec3| {
            let q = self.flat(p);
            format!("{:.2},{:.2}", q.x - lo.x + pad, hi.y - q.y + pad)
        };
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.1}mm\" height=\"{h:.1}mm\" viewBox=\"0 0 {w:.2} {h:.2}\">\n<g fill=\"#3a3d4a\" fill-rule=\"evenodd\" stroke=\"#111\" stroke-width=\"{:.2}\">\n",
            (w.max(h) / 400.0).max(0.5)
        );
        for (_, o, holes) in &self.regions {
            let ring = |l: &Vec<DVec3>| format!("M{}Z", l.iter().map(|p| tx(*p)).collect::<Vec<_>>().join("L"));
            s += &format!("<path d=\"{}{}\"/>\n", ring(o), holes.iter().map(ring).collect::<String>());
        }
        s += "</g>\n</svg>\n";
        s
    }

    /// The cut as a DXF (R12, mm): every cut line on layer SECTION.
    pub fn dxf(&self) -> String {
        let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
        for (a, b) in &self.segments {
            let (p, q) = (self.flat(*a), self.flat(*b));
            s += &format!("0\nLINE\n8\nSECTION\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n", p.x, p.y, q.x, q.y);
        }
        s += "0\nENDSEC\n0\nEOF\n";
        s
    }
}
