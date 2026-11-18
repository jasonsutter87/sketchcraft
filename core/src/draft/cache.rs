//! Drawings kept between page views and exports (design §4): one display list per viewport, held by the engine
//! (never saved), and the generated lines of each hatched region.
//!
//! - **Key.** A viewport's drawing is keyed by everything that decides it besides the model: its whole
//!   description (`ViewportSpec`, after the page has placed it), the pen table (paper or blueprint) and the
//!   paper's height. Editing a viewport makes a new key, so it is drawn again; nothing else about it needs saying.
//! - **Invalidation.** After every command that changes the model, the engine asks `dirty::changes` where drawings
//!   may now differ, and `Drawings::invalidate` drops the drawings whose `Volume` (what they can show) meets that.
//!   Moving one wall drops the plans of its floor that frame it, the elevations and sections that see it, and
//!   nothing else. Counters (`Stats`) say how many were drawn, kept and dropped, for tests and `drafting_status`.
//! - **Memory.** Drawings are counted in estimated bytes; past the budget the least recently used go first.
//!   Hatches have a budget of their own.
//! - **Background.** The engine can redraw what was dropped from recently viewed pages on a worker with a copy of
//!   the model (`Engine::warm_sheets`); a result is kept only if nothing changed while it was being drawn.

use super::dirty::{Bx, Dirty};
use super::hidden::Proj;
use super::ir::*;
use crate::layout::ViewportSpec;
use crate::model::Model;
use glam::DVec3;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// How far past its frame (paper mm) a viewport counts as seeing something: line ends, pen widths, a stair's
/// "UP" and the reach of short text all stay within it.
pub const PAD_MM: f64 = 10.0;
/// What the drawings of all viewports may hold (estimated bytes) before the least recently used are let go.
pub const DEFAULT_BUDGET: usize = 1 << 30;
/// What the hatch results may hold (segments) before they are let go.
pub const HATCH_BUDGET_SEGMENTS: usize = 4_000_000;

/// What a viewport can show: the world space a change has to reach to change its drawing.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one per kept drawing: a handful of bytes either way
pub enum Volume {
    /// any change (a drawing fitted to the whole model, worked out from the whole building, or without an anchor)
    All,
    /// a picture: any change, including materials and lights
    Raster,
    /// a parallel view: what projects into its frame, within `z` (plans) and on the kept side of `half` (sections)
    Prism {
        proj: Proj,
        /// the frame, padded by `PAD_MM`
        rect: [f64; 4],
        /// a plan: the heights it draws (below the cut, the cut, overhead), and its floor (for model notes)
        z: Option<(f64, f64)>,
        floor: Option<f64>,
        half: Option<(DVec3, DVec3)>,
        /// its camera comes from a saved view looked up by name
        views: bool,
    },
}

impl Volume {
    /// What `v` (already placed on its page) can show of `m`. Mirrors the choices `project::plan` and `project::view`
    /// make: whatever there depends on the whole model makes the volume `All`.
    pub fn of(m: &Model, v: &ViewportSpec) -> Volume {
        let scale = v.scale.as_deref().and_then(|s| super::paper::scale_of(s).ok()).filter(|d| d.is_finite() && *d > 0.0);
        let centre = [v.x + v.w / 2.0, v.y + v.h / 2.0];
        let rect = [v.x - PAD_MM, v.y - PAD_MM, v.w + 2.0 * PAD_MM, v.h + 2.0 * PAD_MM];
        if let Some(z) = v.cut {
            // fitted, unpinned, or with callouts worked out from the whole building (dimension chains, tags, the
            // section line's reach): any change can move them
            let (Some(d), Some(a)) = (scale, v.anchor) else { return Volume::All };
            if v.callouts || v.section_mark.is_some() || !z.is_finite() {
                return Volume::All;
            }
            let floor = v.floor.unwrap_or(z - 1.22);
            let proj = Proj::new(DVec3::from(a), DVec3::NEG_Z, 1000.0 / d, centre);
            // from the floor (edges lying on it are left out, edges reaching 1 cm below it drawn) through the cut, and
            // overhead up to 2.8 m above the floor
            return Volume::Prism { proj, rect, z: Some((floor - 0.02, z.max(floor + 2.8) + 0.05)), floor: Some(floor), half: None, views: false };
        }
        let Some(d) = scale.filter(|_| !v.raster) else { return Volume::Raster };
        let Some(a) = v.anchor else { return Volume::All };
        let (look, _, views) = super::project::look_of(m, v, DVec3::ZERO);
        if look.length_squared() < 1e-12 {
            return Volume::All;
        }
        let proj = Proj::new(DVec3::from(a), look, 1000.0 / d, centre);
        Volume::Prism { proj, rect, z: None, floor: None, half: super::project::section_of(v), views }
    }

    fn sees(&self, b: &Bx, below: f64) -> bool {
        let Volume::Prism { proj, rect, z, half, .. } = self else { return true };
        if b.is_empty() {
            return false;
        }
        if let Some((z0, z1)) = z {
            let z0 = z0 - below;
            if b.hi.z < z0 || b.lo.z > *z1 {
                return false;
            }
        }
        let cs = b.corners();
        if let Some((o, d)) = half {
            if cs.iter().all(|c| (*c - *o).dot(*d) < -1e-6) {
                return false;
            }
        }
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for c in cs {
            let q = proj.xy(c);
            for i in 0..2 {
                lo[i] = lo[i].min(q[i]);
                hi[i] = hi[i].max(q[i]);
            }
        }
        hi[0] >= rect[0] && lo[0] <= rect[0] + rect[2] && hi[1] >= rect[1] && lo[1] <= rect[1] + rect[3]
    }

    /// Could this change have changed the drawing?
    pub fn hit(&self, d: &Dirty) -> bool {
        if d.is_empty() {
            return false;
        }
        match self {
            Volume::All => d.all || d.views || d.cuts || !d.boxes.is_empty() || !d.notes_z.is_empty() || !d.stairs.is_empty(),
            Volume::Raster => true,
            Volume::Prism { floor, views, z, half, .. } => {
                // plans and sections draw the cut
                if d.cuts && (z.is_some() || half.is_some()) {
                    return true;
                }
                // model notes on a plan's floor (`annotations`: from 5 cm below the floor to 1 m above)
                let note = floor.is_some_and(|f| d.notes_z.iter().any(|z| *z > f - 0.06 && *z < f + 1.01));
                // a stair starting or arriving within 0.3 m of the floor is drawn (`project::stairs`)
                d.all || (*views && d.views) || note || d.boxes.iter().any(|b| self.sees(b, 0.0)) || d.stairs.iter().any(|b| self.sees(b, 0.3))
            }
        }
    }
}

/// The key a viewport's drawing is kept under.
pub fn key(v: &ViewportSpec, pens: &Pens, page_h: f64) -> String {
    format!("{v:?}|{}|{page_h:?}", pens.blueprint)
}

/// How much memory a display list holds (an estimate: what counts is that big drawings count more).
pub fn bytes_of(items: &[Prim]) -> usize {
    items
        .iter()
        .map(|p| match p {
            Prim::Path(pp) => 96 + pp.segs.len() * 56 + pp.prov.as_ref().map_or(0, |pv| 24 + pv.edge.path.len() * 4),
            Prim::Text(t) => 96 + t.text.len(),
            Prim::Image(i) => 64 + i.rgb.as_raw().len() / 8,
            Prim::Group(g) => 96 + g.alt.as_ref().map_or(0, String::len) + bytes_of(&g.children),
            Prim::Hatch(h) => 160 + h.lines.len() * 32 + h.rings.iter().map(|r| r.len() * 16).sum::<usize>(),
            Prim::Lines(l) => 96 + l.bytes(),
        })
        .sum()
}

/// What the drawings have done, since the engine started (or `reset_stats`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    /// viewports drawn (cold, or again after a change)
    pub drawn: u64,
    /// of those, drawn on the background worker
    pub drawn_background: u64,
    /// viewports taken from the cache
    pub kept: u64,
    /// drawings dropped because a change reached them
    pub dropped: u64,
    /// drawings let go for memory
    pub evicted: u64,
    /// hatched regions generated, and taken from the cache
    pub hatch_made: u64,
    pub hatch_kept: u64,
}

struct Entry {
    items: Arc<Vec<Prim>>,
    vol: Volume,
    bytes: usize,
    used: u64,
}

struct Inner {
    map: HashMap<String, Entry>,
    bytes: usize,
    budget: usize,
    clock: u64,
    /// bumps whenever drawings are dropped: a drawing begun before is not kept
    epoch: u64,
    stats: Stats,
    /// sheets the person looked at last (newest last), redrawn in the background after a change
    recent: Vec<crate::model::Id>,
    /// background requests: the newest wins
    ticket: u64,
    busy: usize,
    /// the model's world geometry, for the model as it is now (dropped with any change that reaches a drawing)
    world: Option<Arc<World>>,
}

impl Default for Inner {
    fn default() -> Self {
        Inner { map: HashMap::new(), bytes: 0, budget: DEFAULT_BUDGET, clock: 0, epoch: 0, stats: Stats::default(), recent: Vec::new(), ticket: 0, busy: 0, world: None }
    }
}

/// The drawings an engine keeps (cheap to clone: one shared store).
#[derive(Clone, Default)]
pub struct Drawings {
    inner: Arc<Mutex<Inner>>,
    pub hatches: Arc<HatchCache>,
    worker: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl std::fmt::Debug for Drawings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Drawings({} kept)", self.len())
    }
}

impl Drawings {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn len(&self) -> usize {
        self.lock().map.len()
    }
    pub fn is_empty(&self) -> bool {
        let g = self.lock();
        g.map.is_empty() && g.world.is_none() && self.hatches.is_empty()
    }

    /// The world geometry of the model as it is now, shared by the drawings until it changes.
    pub fn world(&self) -> Arc<World> {
        self.lock().world.get_or_insert_with(Default::default).clone()
    }
    pub fn bytes(&self) -> usize {
        self.lock().bytes
    }
    pub fn set_budget(&self, bytes: usize) {
        let mut g = self.lock();
        g.budget = bytes;
        evict(&mut g);
    }
    pub fn stats(&self) -> Stats {
        let mut s = self.lock().stats;
        let (made, kept) = self.hatches.counts();
        s.hatch_made = made;
        s.hatch_kept = kept;
        s
    }
    pub fn reset_stats(&self) {
        self.lock().stats = Stats::default();
        self.hatches.reset_counts();
    }
    pub fn epoch(&self) -> u64 {
        self.lock().epoch
    }
    /// Is a background redraw running or waiting?
    pub fn busy(&self) -> bool {
        self.lock().busy > 0
    }

    /// The kept drawing for `key`, if there is one.
    pub fn get(&self, key: &str) -> Option<Arc<Vec<Prim>>> {
        let mut g = self.lock();
        g.clock += 1;
        let now = g.clock;
        let got = g.map.get_mut(key).map(|e| {
            e.used = now;
            e.items.clone()
        });
        if got.is_some() {
            g.stats.kept += 1;
        }
        got
    }

    pub fn contains(&self, key: &str) -> bool {
        self.lock().map.contains_key(key)
    }

    /// Keep a drawing begun at `epoch`. Refused (false) when drawings were dropped since: it may show the model as
    /// it was before that change.
    pub fn put(&self, key: String, vol: Volume, items: Arc<Vec<Prim>>, epoch: u64, background: bool) -> bool {
        let mut g = self.lock();
        g.stats.drawn += 1;
        if background {
            g.stats.drawn_background += 1;
        }
        if g.epoch != epoch {
            return false;
        }
        let bytes = bytes_of(&items);
        g.clock += 1;
        let used = g.clock;
        if let Some(old) = g.map.insert(key, Entry { items, vol, bytes, used }) {
            g.bytes -= old.bytes;
        }
        g.bytes += bytes;
        evict(&mut g);
        true
    }

    /// A change to the model: drop every drawing it may have changed. Returns how many were dropped.
    pub fn invalidate(&self, d: &Dirty) -> usize {
        if d.is_empty() {
            return 0;
        }
        if d.all {
            self.hatches.clear();
        }
        let mut g = self.lock();
        let gone: Vec<String> = g.map.iter().filter(|(_, e)| e.vol.hit(d)).map(|(k, _)| k.clone()).collect();
        for k in &gone {
            if let Some(e) = g.map.remove(k) {
                g.bytes -= e.bytes;
            }
        }
        g.stats.dropped += gone.len() as u64;
        // a drawing in progress may have read the model before the change
        g.epoch += 1;
        g.world = None;
        gone.len()
    }

    /// Forget everything (a model replaced from outside a command).
    pub fn clear(&self) {
        let mut g = self.lock();
        let n = g.map.len() as u64;
        g.map.clear();
        g.world = None;
        g.bytes = 0;
        g.stats.dropped += n;
        g.epoch += 1;
        drop(g);
        self.hatches.clear();
    }

    /// The person looked at this sheet: it is redrawn in the background after a change (with the two before it).
    pub fn viewed(&self, sheet: crate::model::Id) {
        let mut g = self.lock();
        g.recent.retain(|s| *s != sheet);
        g.recent.push(sheet);
        let n = g.recent.len();
        if n > 3 {
            g.recent.drain(..n - 3);
        }
    }
    pub fn recent(&self) -> Vec<crate::model::Id> {
        self.lock().recent.clone()
    }

    /// Run `job` on the background worker, after whatever is already there. Only the newest job runs: an older one
    /// that hasn't started is skipped.
    pub fn background(&self, job: impl FnOnce() + Send + 'static) {
        let ticket = {
            let mut g = self.lock();
            g.ticket += 1;
            g.busy += 1;
            g.ticket
        };
        let me = self.clone();
        let mut w = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        let before = w.take();
        *w = Some(std::thread::spawn(move || {
            if let Some(b) = before {
                let _ = b.join();
            }
            let newest = me.lock().ticket == ticket;
            if newest {
                // a drawing that fails or panics leaves the cache as it was
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
            }
            me.lock().busy -= 1;
        }));
    }

    /// Wait for the background worker to finish.
    pub fn wait(&self) {
        let h = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = h {
            let _ = h.join();
        }
    }
}

fn evict(g: &mut Inner) {
    while g.bytes > g.budget && !g.map.is_empty() {
        let Some(k) = g.map.iter().min_by_key(|(_, e)| e.used).map(|(k, _)| k.clone()) else { break };
        if let Some(e) = g.map.remove(&k) {
            g.bytes -= e.bytes;
            g.stats.evicted += 1;
        }
    }
}

// ---- the model's world geometry ---------------------------------------------------------------------------

/// A model's faces, edges and triangles in world space, worked out once (on first use) and shared by every drawing
/// that has it in its pens: all viewports of a set or a page, and the kept drawings until the next change to the
/// model (`Drawings::world`). Each drawing used to expand every component placement again.
#[derive(Default)]
pub struct World {
    faces: std::sync::OnceLock<Vec<crate::components::WFace>>,
    edges: std::sync::OnceLock<Vec<crate::components::WEdge>>,
    tris: std::sync::OnceLock<Vec<[DVec3; 3]>>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "World")
    }
}

impl World {
    pub fn faces(&self, m: &Model) -> &[crate::components::WFace] {
        self.faces.get_or_init(|| m.world_faces())
    }
    pub fn edges(&self, m: &Model) -> &[crate::components::WEdge] {
        self.edges.get_or_init(|| m.world_edges())
    }
    pub fn tris(&self, m: &Model) -> &[[DVec3; 3]] {
        self.tris.get_or_init(|| World::triangles(self.faces(m)))
    }
    /// Faces as triangles (faces with no normal left out).
    pub fn triangles(faces: &[crate::components::WFace]) -> Vec<[DVec3; 3]> {
        faces.iter().filter(|f| f.normal.length_squared() >= 1e-18).flat_map(|f| crate::geom::triangulate(&f.outer, &f.holes, f.normal)).collect()
    }
}

// ---- hatches ----------------------------------------------------------------------------------------------

/// Generated hatches, by region and `HatchUse` (design §3.3: "cached per (region hash, HatchUse)"). The region is
/// its rings on the page (paper mm), so the same wall drawn again in the same place reuses its lines.
#[derive(Default)]
pub struct HatchCache {
    inner: Mutex<HatchInner>,
}

#[derive(Default)]
struct HatchInner {
    map: HashMap<u64, Vec<(String, Arc<HatchPrim>)>>,
    segments: usize,
    made: u64,
    kept: u64,
}

impl std::fmt::Debug for HatchCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HatchCache")
    }
}

impl HatchCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, HatchInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn is_empty(&self) -> bool {
        self.lock().map.is_empty()
    }
    pub fn counts(&self) -> (u64, u64) {
        let g = self.lock();
        (g.made, g.kept)
    }
    fn reset_counts(&self) {
        let mut g = self.lock();
        g.made = 0;
        g.kept = 0;
    }
    pub fn clear(&self) {
        let mut g = self.lock();
        g.map.clear();
        g.segments = 0;
    }
}

fn hash_rings(rings: &[Vec<[f64; 2]>], sig: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sig.hash(&mut h);
    for r in rings {
        r.len().hash(&mut h);
        for p in r {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    h.finish()
}

/// `hatch::on_sheet`, through the pens' hatch cache when they have one. The result is the same either way: the key
/// holds every input (the rings exactly, the `HatchUse`, the pattern it resolves to, the page height, the scale and
/// the pens it falls back on).
pub fn hatch_on_sheet(d: &crate::drafting::Drafting, u: &crate::drafting::HatchUse, rings: Vec<Vec<[f64; 2]>>, page_h: f64, denom: f64, pens: &Pens) -> HatchPrim {
    let Some(cache) = &pens.hatches else { return super::hatch::on_sheet(d, u, rings, page_h, denom, pens) };
    let sig = format!("{u:?}|{:?}|{page_h:?}|{denom:?}|{:?}|{:?}|{:?}", super::pat::lookup(d, &u.pattern), pens.hair, pens.ink, pens.paper);
    let h = hash_rings(&rings, &sig);
    {
        let mut g = cache.lock();
        let hit = g.map.get(&h).and_then(|b| b.iter().find(|(s, p)| *s == sig && p.rings == rings).map(|(_, p)| p.clone()));
        if let Some(p) = hit {
            g.kept += 1;
            return (*p).clone();
        }
    }
    let made = super::hatch::on_sheet(d, u, rings, page_h, denom, pens);
    let mut g = cache.lock();
    g.made += 1;
    let n = made.lines.len() + made.rings.iter().map(Vec::len).sum::<usize>();
    if g.segments + n > HATCH_BUDGET_SEGMENTS {
        g.map.clear();
        g.segments = 0;
    }
    if n <= HATCH_BUDGET_SEGMENTS {
        g.segments += n;
        g.map.entry(h).or_default().push((sig, Arc::new(made.clone())));
    }
    made
}
