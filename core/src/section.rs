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
    /// the cut surfaces: outlines with holes, per innermost tagged owner (`WFace::cut`)
    pub regions: Vec<Region>,
}

/// One cut surface: an outline with its holes, and what it belongs to.
#[derive(Clone, Debug)]
pub struct Region {
    /// the innermost tagged owner (a group or placement in the root model, or the top-level placement a tagged thing
    /// inside a component sits in); untagged: the face's own group, the top-level placement, or the loose face
    pub owner: Id,
    /// the tag whose cut style (`TagStyle.cut`) fills it; None: nothing around it is tagged
    pub tag: Option<String>,
    pub outer: Vec<DVec3>,
    pub holes: Vec<Vec<DVec3>>,
}

fn key(p: DVec3) -> (i64, i64, i64) {
    ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, (p.z * 1e6).round() as i64)
}

impl Model {
    /// Slice the visible model with the plane through `origin` facing `normal`.
    pub fn section(&self, origin: DVec3, normal: DVec3) -> Result<Cut> {
        self.section_styled(origin, normal, &self.drafting)
    }

    /// `section` with the tag styles of `drafting` (the whole model's, when a component is open and `self` is it):
    /// cut regions of different tags that overlap are resolved by `CutStyle::priority` (see `resolve_priorities`).
    pub fn section_styled(&self, origin: DVec3, normal: DVec3, drafting: &crate::drafting::Drafting) -> Result<Cut> {
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
        let mut by_owner: HashMap<(Id, Option<std::sync::Arc<str>>), Vec<(DVec3, DVec3)>> = HashMap::new();
        let mut segments = Vec::new();
        let mut seg_tags: Vec<Option<std::sync::Arc<str>>> = Vec::new();
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
                seg_tags.push(f.tag.clone());
                by_owner.entry((f.cut, f.tag.clone())).or_default().push(seg);
            }
        }
        // chain each owner's segments into loops, then nest them
        let mut regions = Vec::new();
        let mut owners: Vec<(Id, Option<std::sync::Arc<str>>)> = by_owner.keys().cloned().collect();
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
            regions.extend(outs.into_iter().map(|(_, outer, holes)| Region { owner: owner.0, tag: owner.1.as_deref().map(str::to_string), outer, holes }));
        }
        if priorities_in_use(drafting) {
            (regions, segments) = resolve_priorities(drafting, regions, segments, &seg_tags, (u, v, n, w));
        }
        Ok(Cut { u, v, n, origin, segments, regions })
    }
}

/// A cut region smaller than this (m², 0.01 mm²) is numerical dust from a boolean, not a surface: dropped.
const SLIVER_AREA: f64 = 1e-8;

fn priority_of(d: &crate::drafting::Drafting, tag: Option<&str>) -> i32 {
    tag.and_then(|t| d.tag_style(t)).map_or(0, |s| s.cut.priority)
}

/// Whether any tag has an intersection priority. With none, regions stay exactly as the chaining made them
/// (the output before priorities existed).
fn priorities_in_use(d: &crate::drafting::Drafting) -> bool {
    d.tag_styles.values().any(|t| matches!(t, crate::drafting::Keep::Known(t) if t.cut.priority != 0))
}

/// Resolve overlapping cut regions of different tags by `CutStyle::priority` (design §3.4, #88): the higher
/// priority keeps the overlap and the lower is trimmed back (no seam: the trimmed region simply ends at the winner's
/// edge). Regions of one tag merge (union, no internal outline); so do equal priorities, where the overlap goes to the
/// tag that sorts first by name (one fill, one outline, never two on top of each other). A region keeps its tag; a
/// merged one keeps the owner of the first region (by owner id) it covers. Dust below `SLIVER_AREA` is dropped.
/// The thin cut lines of a trimmed tag are cut back by the same winners.
/// `plane` is the plane's `(u, v, n, w)`: a point is `u*x + v*y + n*w`.
fn resolve_priorities(
    d: &crate::drafting::Drafting,
    regions: Vec<Region>,
    segments: Vec<(DVec3, DVec3)>,
    seg_tags: &[Option<std::sync::Arc<str>>],
    plane: (DVec3, DVec3, DVec3, f64),
) -> (Vec<Region>, Vec<(DVec3, DVec3)>) {
    use geo::{Area, BooleanOps, Contains, Coord, InteriorPoint, LineString, MultiLineString, MultiPolygon, Polygon};
    let (u, v, n, w) = plane;
    let fallback = (regions.clone(), segments.clone());
    let ring2 = |r: &[DVec3]| LineString::from(r.iter().map(|p| Coord { x: p.dot(u), y: p.dot(v) }).collect::<Vec<_>>());
    let to3 = |c: &Coord<f64>| u * c.x + v * c.y + n * w;
    // the order of claiming: higher priority first, then tag name, untagged last
    let rank = |t: &Option<String>| (std::cmp::Reverse(priority_of(d, t.as_deref())), t.is_none(), t.clone());
    let mut tags: Vec<Option<String>> = regions.iter().map(|r| r.tag.clone()).chain(seg_tags.iter().map(|t| t.as_deref().map(str::to_string))).collect();
    tags.sort_by_key(rank);
    tags.dedup();
    let work = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut claimed = MultiPolygon::<f64>::new(vec![]);
        let mut out: Vec<Region> = Vec::new();
        let mut segs_out: Vec<(DVec3, DVec3)> = Vec::new();
        for tag in &tags {
            let mine: Vec<&Region> = regions.iter().filter(|r| &r.tag == tag).collect();
            let mut group = MultiPolygon::<f64>::new(vec![]);
            for r in &mine {
                let poly = MultiPolygon::new(vec![Polygon::new(ring2(&r.outer), r.holes.iter().map(|h| ring2(h)).collect())]);
                group = if group.0.is_empty() { poly } else { group.union(&poly) };
            }
            let visible = if claimed.0.is_empty() { group.clone() } else { group.difference(&claimed) };
            for poly in visible.0 {
                if poly.unsigned_area() < SLIVER_AREA {
                    continue;
                }
                let holes: Vec<LineString<f64>> = poly.interiors().iter().filter(|h| Polygon::new((*h).clone(), vec![]).unsigned_area() >= SLIVER_AREA).cloned().collect();
                let poly = Polygon::new(poly.exterior().clone(), holes);
                // the first source region (by owner id) that covers a point inside the piece
                let probe = poly.interior_point();
                let owner = mine
                    .iter()
                    .filter(|r| probe.is_some_and(|p| Polygon::new(ring2(&r.outer), r.holes.iter().map(|h| ring2(h)).collect()).contains(&p)))
                    .map(|r| r.owner)
                    .min()
                    .or_else(|| mine.iter().map(|r| r.owner).min());
                let Some(owner) = owner else { continue };
                let ring3 = |l: &LineString<f64>| -> Vec<DVec3> {
                    let mut pts: Vec<DVec3> = l.coords().map(to3).collect();
                    if pts.len() > 1 && l.0.first() == l.0.last() {
                        pts.pop();
                    }
                    pts
                };
                let mut outer = ring3(poly.exterior());
                if outer.len() < 3 {
                    continue;
                }
                if crate::geom::newell(&outer).dot(n) < 0.0 {
                    outer.reverse();
                }
                let holes = poly
                    .interiors()
                    .iter()
                    .map(ring3)
                    .filter(|h| h.len() >= 3)
                    .map(|mut h| {
                        if crate::geom::newell(&h).dot(n) > 0.0 {
                            h.reverse();
                        }
                        h
                    })
                    .collect();
                out.push(Region { owner, tag: tag.clone(), outer, holes });
            }
            // this tag's thin cut lines, minus what higher claims cover
            let mine_segs: Vec<&(DVec3, DVec3)> = segments.iter().zip(seg_tags).filter(|(_, t)| t.as_deref() == tag.as_deref()).map(|(s, _)| s).collect();
            if claimed.0.is_empty() {
                segs_out.extend(mine_segs.into_iter().copied());
            } else if !mine_segs.is_empty() {
                let lines: Vec<LineString<f64>> = mine_segs
                    .iter()
                    .map(|(a, b)| LineString::from(vec![Coord { x: a.dot(u), y: a.dot(v) }, Coord { x: b.dot(u), y: b.dot(v) }]))
                    .collect();
                let kept = claimed.clip(&MultiLineString::new(lines), true);
                for l in kept.0 {
                    for pair in l.0.windows(2) {
                        let (a, b) = (to3(&pair[0]), to3(&pair[1]));
                        if (a - b).length() > 1e-7 {
                            segs_out.push((a, b));
                        }
                    }
                }
            }
            claimed = if claimed.0.is_empty() { group } else { claimed.union(&group) };
        }
        (out, segs_out)
    }));
    // a boolean that cannot cope with some degenerate loop must not lose the drawing: the unresolved regions stand
    work.unwrap_or(fallback)
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
        for r in &self.regions {
            let ring = |l: &Vec<DVec3>| format!("M{}Z", l.iter().map(|p| tx(*p)).collect::<Vec<_>>().join("L"));
            s += &format!("<path d=\"{}{}\"/>\n", ring(&r.outer), r.holes.iter().map(ring).collect::<String>());
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
