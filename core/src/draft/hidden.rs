//! Hidden-line removal for parallel views: which parts of which model edges can be seen.
//!
//! The model's triangles go into the path tracer's BVH (`pt.rs`). Each projected edge is
//! sampled along its length and each sample asks one question: is anything between this
//! point and the viewer? Where the answer changes between two samples, the interval is
//! bisected until it's no longer than `RESOLUTION` on the paper, so a span's end is exact
//! to half that. Edges too short to matter are dropped, and every edge is first cut to the
//! viewport's rectangle so work (and output) never leaves the frame.

use super::ir::*;
use crate::model::Model;
use crate::pt::{Bvh, Tri};
use glam::{DVec3, Vec3};
use std::sync::Arc;

/// How finely a visible/hidden change is located, in paper millimeters.
pub const RESOLUTION: f64 = 0.05;
/// How far apart edge samples are, in paper millimeters.
pub const SAMPLE_SPACING: f64 = 0.5;
/// An edge shorter than this on the paper isn't drawn.
const MIN_LEN: f64 = 0.02;

/// A parallel projection onto the paper: model point -> millimeters from the page's top left.
#[derive(Clone, Debug)]
pub struct Proj {
    /// the model point pinned to the middle of the viewport
    pub anchor: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    /// the way the viewer looks (into the scene)
    pub look: DVec3,
    /// paper millimeters per model meter
    pub k: f64,
    /// the middle of the viewport on the page (mm)
    pub c: [f64; 2],
}

impl Proj {
    pub fn new(anchor: DVec3, look: DVec3, k: f64, c: [f64; 2]) -> Proj {
        let f = look.normalize_or_zero();
        let reference = if f.cross(DVec3::Z).length() < 1e-3 { DVec3::Y } else { DVec3::Z };
        let right = f.cross(reference).normalize();
        let up = right.cross(f);
        Proj { anchor, right, up, look: f, k, c }
    }
    #[inline]
    pub fn xy(&self, p: DVec3) -> [f64; 2] {
        let d = p - self.anchor;
        [self.c[0] + d.dot(self.right) * self.k, self.c[1] - d.dot(self.up) * self.k]
    }
}

pub struct View<'a> {
    pub m: &'a Model,
    pub proj: Proj,
    /// x, y, w, h on the page, mm
    pub rect: [f64; 4],
    /// only what's on `d`'s side of the plane through `o` (a section)
    pub clip: Option<(DVec3, DVec3)>,
    /// also draw the hidden parts, dashed
    pub beyond: bool,
    pub pens: &'a Pens,
    /// tests: make a worker thread panic, to check the export fails instead of losing line work
    pub inject_panic: bool,
    /// answer "hidden" from the `Cover` where it can (true; false: every sample casts its ray, which tests compare)
    pub cover: bool,
}

fn rect_hit(rect: &[f64; 4], lo: [f64; 2], hi: [f64; 2]) -> bool {
    hi[0] >= rect[0] && lo[0] <= rect[0] + rect[2] && hi[1] >= rect[1] && lo[1] <= rect[1] + rect[3]
}

fn clip_t(a: [f64; 2], b: [f64; 2], r: &[f64; 4]) -> Option<(f64, f64)> {
    clip_line(a, b, r)
}

/// The part of a polygon on `d`'s side of the plane through `o`.
fn clip_poly(tri: &[DVec3; 3], o: DVec3, d: DVec3) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = ((a - o).dot(d), (b - o).dot(d));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}

struct Item<'e> {
    e: &'e crate::components::WEdge,
    a: DVec3,
    b: DVec3,
    t0: f64,
    t1: f64,
    len: f64,
    ta: f64,
    tb: f64,
}

/// Where the view is certainly covered: a grid over the frame (paper mm) holding, per cell, the depth behind which
/// a point is hidden for sure, from the big triangles that cover the whole cell. A sample that lands behind it is
/// hidden without casting its ray. Conservative by margins well past `f32` rounding (in the paper and in depth), so
/// it only ever answers "hidden" where the ray would have: the drawing is the same, made faster (most of a full
/// house's edges are furniture and rooms behind its walls).
struct Cover {
    /// the grid's corner on the paper, its cell size (mm) and size in cells
    x0: f64,
    y0: f64,
    cell: f64,
    nx: usize,
    ny: usize,
    /// per cell: a point deeper than this (metres along the view, from the anchor) is hidden; INFINITY: not known
    depth: Vec<f64>,
    /// how far a point may be from the anchor on any axis and still be judged here (rays reach `tmax`)
    reach: f64,
}

/// Most cells in a `Cover` (about 2 MB).
const COVER_CELLS: usize = 250_000;

impl Cover {
    fn new(tris: &[Tri], pr: &Proj, rect: &[f64; 4], tmin: f64, far: f64) -> Cover {
        let cell = (rect[2] * rect[3] / COVER_CELLS as f64).sqrt().max(1.0);
        // in the paper: cells are judged as if a fiftieth of a millimetre larger all round, plus how far a ray drifts
        // sideways over its length where the BVH tilts it off an axis by a millionth (and f32 corners); in depth: a
        // millimetre of the model plus the ray's own start gap
        let pad = 0.02 + 4e-6 * far * pr.k;
        let dpad = 1e-3 + tmin;
        let (x0, y0) = (rect[0] - cell, rect[1] - cell);
        let nx = (rect[2] / cell).ceil() as usize + 3;
        let ny = (rect[3] / cell).ceil() as usize + 3;
        let mut depth = vec![f64::INFINITY; nx * ny];
        let xy = |q: DVec3| -> [f64; 2] { [pr.c[0] + q.dot(pr.right) * pr.k, pr.c[1] - q.dot(pr.up) * pr.k] };
        for t in tris {
            let qs = [t.a.as_dvec3(), (t.a + t.e1).as_dvec3(), (t.a + t.e2).as_dvec3()];
            let ps = qs.map(xy);
            let ds = qs.map(|q| q.dot(pr.look));
            let area2 = (ps[1][0] - ps[0][0]) * (ps[2][1] - ps[0][1]) - (ps[2][0] - ps[0][0]) * (ps[1][1] - ps[0][1]);
            if area2.abs() < cell * cell {
                continue; // too small (or edge-on) to cover a cell
            }
            let lo = [ps[0][0].min(ps[1][0]).min(ps[2][0]), ps[0][1].min(ps[1][1]).min(ps[2][1])];
            let hi = [ps[0][0].max(ps[1][0]).max(ps[2][0]), ps[0][1].max(ps[1][1]).max(ps[2][1])];
            let i0 = ((lo[0] + pad - x0) / cell).ceil().max(0.0) as usize;
            let j0 = ((lo[1] + pad - y0) / cell).ceil().max(0.0) as usize;
            let i1 = (((hi[0] - pad - x0) / cell).floor().max(0.0) as usize).min(nx);
            let j1 = (((hi[1] - pad - y0) / cell).floor().max(0.0) as usize).min(ny);
            if i0 >= i1 || j0 >= j1 {
                continue;
            }
            // barycentric weights of a paper point (the depth is planar over the triangle)
            let bary = |x: f64, y: f64| -> [f64; 3] {
                let w1 = ((x - ps[0][0]) * (ps[2][1] - ps[0][1]) - (ps[2][0] - ps[0][0]) * (y - ps[0][1])) / area2;
                let w2 = ((ps[1][0] - ps[0][0]) * (y - ps[0][1]) - (x - ps[0][0]) * (ps[1][1] - ps[0][1])) / area2;
                [1.0 - w1 - w2, w1, w2]
            };
            for j in j0..j1 {
                for i in i0..i1 {
                    let (cx0, cy0) = (x0 + i as f64 * cell - pad, y0 + j as f64 * cell - pad);
                    let (cx1, cy1) = (cx0 + cell + 2.0 * pad, cy0 + cell + 2.0 * pad);
                    let mut deepest = f64::MIN;
                    let mut inside = true;
                    for (x, y) in [(cx0, cy0), (cx1, cy0), (cx0, cy1), (cx1, cy1)] {
                        let w = bary(x, y);
                        if w.iter().any(|w| *w < 0.0) {
                            inside = false;
                            break;
                        }
                        deepest = deepest.max(w[0] * ds[0] + w[1] * ds[1] + w[2] * ds[2]);
                    }
                    if inside {
                        let k = j * nx + i;
                        depth[k] = depth[k].min(deepest + dpad);
                    }
                }
            }
        }
        // a point judged here is at most `reach` from the anchor on every axis: its ray to anything in the frame is
        // shorter than the BVH's `tmax` (2 x (4 x far + 10))
        Cover { x0, y0, cell, nx, ny, depth, reach: far }
    }

    /// A cover that knows nothing (every sample casts its ray).
    fn none() -> Cover {
        Cover { x0: 0.0, y0: 0.0, cell: 1.0, nx: 0, ny: 0, depth: Vec::new(), reach: 0.0 }
    }

    /// Is the point `q` (from the anchor), at `xy` on the paper, certainly hidden?
    #[inline]
    fn hides(&self, q: DVec3, xy: [f64; 2], look: DVec3) -> bool {
        let (fx, fy) = ((xy[0] - self.x0) / self.cell, (xy[1] - self.y0) / self.cell);
        if !(fx >= 0.0 && fy >= 0.0) || q.abs().max_element() > self.reach {
            return false;
        }
        let (i, j) = (fx as usize, fy as usize);
        i < self.nx && j < self.ny && q.dot(look) > self.depth[j * self.nx + i]
    }
}

struct Occluder<'p> {
    bvh: Bvh,
    cover: Cover,
    proj: &'p Proj,
    origin: DVec3,
    toward: Vec3,
    tmin: f32,
    tmax: f32,
}

impl Occluder<'_> {
    #[inline]
    fn visible(&self, p: DVec3) -> bool {
        let q = p - self.origin;
        if self.cover.hides(q, self.proj.xy(p), self.proj.look) {
            return false;
        }
        !self.bvh.blocked(q.as_vec3(), self.toward, self.tmin, self.tmax)
    }
}

/// Visible and hidden spans of the edge a->b (paper length `len` mm): (t0, t1, visible).
fn spans(occ: &Occluder, a: DVec3, b: DVec3, len: f64) -> Vec<(f64, f64, bool)> {
    // a sample every half millimeter on the paper: an occluder or a gap narrower than a pen is below what
    // the page can show
    let n = ((len / SAMPLE_SPACING).ceil() as usize).clamp(8, 4000);
    let at = |t: f64| a + (b - a) * t;
    let vis: Vec<bool> = (0..=n).map(|i| occ.visible(at(i as f64 / n as f64))).collect();
    let mut cuts = vec![(0.0, vis[0])];
    for i in 0..n {
        if vis[i] != vis[i + 1] {
            let (mut lo, mut hi) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            while (hi - lo) * len > RESOLUTION {
                let mid = (lo + hi) / 2.0;
                if occ.visible(at(mid)) == vis[i] {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            cuts.push(((lo + hi) / 2.0, vis[i + 1]));
        }
    }
    let mut out = Vec::new();
    for (i, (t, v)) in cuts.iter().enumerate() {
        let end = cuts.get(i + 1).map_or(1.0, |c| c.0);
        out.push((*t, end, *v));
    }
    out
}

impl View<'_> {
    /// The occluding triangles (relative to the anchor), and how far from it they reach.
    fn triangles(&self) -> (Vec<Tri>, f64) {
        let pr = &self.proj;
        let mut tris = Vec::new();
        let mut far = 1.0f64;
        // the model's faces as triangles (shared by the drawings of a set: `Pens::with_tris`)
        self.pens.with_tris(self.m, |all| {
            for t in all.iter().copied() {
                let polys: Vec<[DVec3; 3]> = match self.clip {
                    None => vec![t],
                    Some((o, d)) => {
                        let p = clip_poly(&t, o, d);
                        (1..p.len().saturating_sub(1)).map(|k| [p[0], p[k], p[k + 1]]).collect()
                    }
                };
                for t in polys {
                    let q: Vec<[f64; 2]> = t.iter().map(|p| pr.xy(*p)).collect();
                    let lo = [q.iter().map(|p| p[0]).fold(f64::MAX, f64::min), q.iter().map(|p| p[1]).fold(f64::MAX, f64::min)];
                    let hi = [q.iter().map(|p| p[0]).fold(f64::MIN, f64::max), q.iter().map(|p| p[1]).fold(f64::MIN, f64::max)];
                    if !rect_hit(&self.rect, lo, hi) {
                        continue;
                    }
                    let a = (t[0] - pr.anchor).as_vec3();
                    let (b, c) = ((t[1] - pr.anchor).as_vec3(), (t[2] - pr.anchor).as_vec3());
                    let (e1, e2) = (b - a, c - a);
                    for p in [a, b, c] {
                        far = far.max(p.abs().max_element() as f64);
                    }
                    tris.push(Tri { a, e1, e2, n: e1.cross(e2).normalize_or_zero(), mat: 0 });
                }
            }
        });
        (tris, far)
    }

    /// The drawing's line work: visible spans in the pen, and (when asked) the hidden spans dashed.
    pub fn lines(&self) -> anyhow::Result<Vec<Prim>> {
        let pr = &self.proj;
        let (tris, far) = self.triangles();
        // how far a ray must go to leave what's in the frame, and how close to its start it ignores
        // (worked out from the triangles in the frame alone: what's elsewhere can't change a drawing)
        let reach = far * 4.0 + 10.0;
        let tmin = (reach * 2e-6).max(1e-4) as f32;
        let cover = if self.cover { Cover::new(&tris, pr, &self.rect, tmin as f64, far) } else { Cover::none() };
        let occ = Occluder { bvh: Bvh::build(tris), cover, proj: pr, origin: pr.anchor, toward: (-pr.look).as_vec3(), tmin, tmax: (reach * 2.0) as f32 };
        self.pens.with_edges(self.m, |all| self.edge_lines(all, &occ))
    }

    fn edge_lines(&self, all: &[crate::components::WEdge], occ: &Occluder) -> anyhow::Result<Vec<Prim>> {
        let pr = &self.proj;
        let mut edges = Vec::new();
        for e in all.iter().filter(|e| !e.soft) {
            let (mut ta, mut tb) = (0.0f64, 1.0f64);
            if let Some((o, d)) = self.clip {
                let (da, db) = ((e.a - o).dot(d), (e.b - o).dot(d));
                if da < 0.0 && db < 0.0 {
                    continue;
                }
                if da < 0.0 {
                    ta = da / (da - db);
                } else if db < 0.0 {
                    tb = da / (da - db);
                }
            }
            let (a, b) = (e.a + (e.b - e.a) * ta, e.a + (e.b - e.a) * tb);
            let (pa, pb) = (pr.xy(a), pr.xy(b));
            let Some((t0, t1)) = clip_t(pa, pb, &self.rect) else { continue };
            let len = ((pb[0] - pa[0]).hypot(pb[1] - pa[1])) * (t1 - t0);
            if len < MIN_LEN {
                continue;
            }
            // the edge's own parameter runs along the whole edge, including any section-clipped part
            edges.push(Item { e, a, b, t0, t1, len, ta, tb });
        }
        let base_ink = self.pens.ink(self.pens.medium);
        let base_ghost = Pen::new(self.pens.thin, self.pens.faint()).dashed(&[1.6, 1.0]);
        let styles = &self.m.drafting;
        let work = |chunk: &[Item]| -> Vec<Prim> {
            let mut out = Vec::new();
            for it in chunk {
                let Item { e: we, a, b, t0, t1, len, ta, tb } = it;
                // the pen for this edge (edge override, then its tag, then the default) and its provenance, made once
                // something of it is drawn
                let (mut ink, mut ghost, mut edge): (Option<Pen>, Option<Pen>, Option<Arc<EdgeRef>>) = (None, None, None);
                let (s, e) = (*a + (*b - *a) * *t0, *a + (*b - *a) * *t1);
                for (u0, u1, vis) in spans(occ, s, e, *len) {
                    if !vis && !self.beyond {
                        continue;
                    }
                    let (p, q) = (pr.xy(s + (e - s) * u0), pr.xy(s + (e - s) * u1));
                    if (q[0] - p[0]).hypot(q[1] - p[1]) < MIN_LEN {
                        continue;
                    }
                    let (h0, h1) = (t0 + (t1 - t0) * u0, t0 + (t1 - t0) * u1);
                    let (g0, g1) = (ta + (tb - ta) * h0, ta + (tb - ta) * h1);
                    let pen = if vis {
                        ink.get_or_insert_with(|| self.pens.for_tag(styles, we.tag.as_deref(), we.draft.as_deref(), base_ink.clone(), false))
                    } else {
                        ghost.get_or_insert_with(|| self.pens.for_tag(styles, we.tag.as_deref(), we.draft.as_deref(), base_ghost.clone(), true))
                    };
                    let edge = edge.get_or_insert_with(|| Arc::new(EdgeRef { path: we.path.to_vec(), v: we.v }));
                    out.push(Prim::Path(PathPrim { segs: vec![Seg::M(p), Seg::L(q)], pen: Some(pen.clone()), fill: None, prov: Some(Prov { edge: edge.clone(), t: [g0 as f32, g1 as f32] }) }));
                }
            }
            out
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
        if threads < 2 || edges.len() < 2000 {
            return Ok(work(&edges));
        }
        let per = edges.len().div_ceil(threads);
        std::thread::scope(|s| {
            let hs: Vec<_> = edges.chunks(per).enumerate().map(|(i, c)| s.spawn(move || {
                if self.inject_panic && i == 0 {
                    panic!("injected");
                }
                work(c)
            })).collect();
            let mut out = Vec::new();
            for h in hs {
                // a worker that stopped would leave a hole in the drawing: fail the whole drawing
                out.extend(h.join().map_err(|_| anyhow::anyhow!("a hidden-line worker stopped unexpectedly, so the drawing would be incomplete"))?);
            }
            Ok(out)
        })
    }
}
