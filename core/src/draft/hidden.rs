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

struct Item {
    path: Arc<[crate::model::Id]>,
    v: [crate::model::Id; 2],
    a: DVec3,
    b: DVec3,
    t0: f64,
    t1: f64,
    len: f64,
    ta: f64,
    tb: f64,
    tag: Option<String>,
    draft: Option<Box<crate::drafting::EdgeDraft>>,
}

struct Occluder {
    bvh: Bvh,
    origin: DVec3,
    toward: Vec3,
    tmin: f32,
    tmax: f32,
}

impl Occluder {
    #[inline]
    fn visible(&self, p: DVec3) -> bool {
        let o = (p - self.origin).as_vec3();
        !self.bvh.blocked(o, self.toward, self.tmin, self.tmax)
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
        let occ = Occluder { bvh: Bvh::build(tris), origin: pr.anchor, toward: (-pr.look).as_vec3(), tmin: (reach * 2e-6).max(1e-4) as f32, tmax: (reach * 2.0) as f32 };
        let mut edges = Vec::new();
        self.pens.with_edges(self.m, |all| for e in all.iter().filter(|e| !e.soft) {
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
            edges.push(Item { path: e.path.clone(), v: e.v, a, b, t0, t1, len, ta, tb, tag: e.tag.clone(), draft: e.draft.clone() });
        });
        let base_ink = self.pens.ink(self.pens.medium);
        let base_ghost = Pen::new(self.pens.thin, self.pens.faint()).dashed(&[1.6, 1.0]);
        let styles = &self.m.drafting;
        let work = |chunk: &[Item]| -> Vec<Prim> {
            let mut out = Vec::new();
            for it in chunk {
                let Item { path, v, a, b, t0, t1, len, ta, tb, tag, draft } = it;
                // the pen for this edge: edge override, then its tag, then the default
                let ink = self.pens.for_tag(styles, tag.as_deref(), draft.as_deref(), base_ink.clone(), false);
                let ghost = self.pens.for_tag(styles, tag.as_deref(), draft.as_deref(), base_ghost.clone(), true);
                let (s, e) = (*a + (*b - *a) * *t0, *a + (*b - *a) * *t1);
                let edge = Arc::new(EdgeRef { path: path.to_vec(), v: *v });
                for (u0, u1, vis) in spans(&occ, s, e, *len) {
                    if !vis && !self.beyond {
                        continue;
                    }
                    let (p, q) = (pr.xy(s + (e - s) * u0), pr.xy(s + (e - s) * u1));
                    if (q[0] - p[0]).hypot(q[1] - p[1]) < MIN_LEN {
                        continue;
                    }
                    let (h0, h1) = (t0 + (t1 - t0) * u0, t0 + (t1 - t0) * u1);
                    let (g0, g1) = (ta + (tb - ta) * h0, ta + (tb - ta) * h1);
                    out.push(Prim::Path(PathPrim {
                        segs: vec![Seg::M(p), Seg::L(q)],
                        pen: Some(if vis { ink.clone() } else { ghost.clone() }),
                        fill: None,
                        prov: Some(Prov { edge: edge.clone(), t: [g0 as f32, g1 as f32] }),
                    }));
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
