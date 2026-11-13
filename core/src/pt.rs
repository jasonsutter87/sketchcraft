//! Photoreal renderer: a CPU path tracer.
//!
//! - triangles from the model (components included) in a BVH (binned SAH)
//! - materials: paint (diffuse + a GGX specular coat), metal, thin glass,
//!   lights; picked from material names, colors and textures from the model
//! - light: a soft sun (next-event estimation) and an analytic sky, over a
//!   ground plane at the model's foot
//! - paths with Russian roulette; all cores; then an edge-aware a-trous
//!   denoiser guided by albedo and normals, and ACES tone mapping
//!
//! Everything is f32 inside; geometry comes in meters.

use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};
use glam::{DVec3, Vec3};
use image::{Rgb, RgbImage};
use std::collections::HashMap;

// ----- materials ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Paint,
    Metal,
    Glass,
    Light,
}

#[derive(Clone, Debug)]
struct Mat {
    kind: Kind,
    color: Vec3,
    rough: f32,
    /// texture: index into `textures`, and its planar mapping
    tex: Option<(usize, Vec3, Vec3, Vec3, f32, f32)>,
}

struct Texture {
    w: usize,
    h: usize,
    px: Vec<Vec3>, // linear
}

impl Texture {
    fn sample(&self, u: f32, v: f32) -> Vec3 {
        let (fu, fv) = (u - u.floor(), v - v.floor());
        let x = ((fu * self.w as f32) as usize).min(self.w - 1);
        let y = (((1.0 - fv) * self.h as f32) as usize).min(self.h - 1);
        self.px[y * self.w + x]
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn kind_of(name: &str) -> Kind {
    let n = name.to_ascii_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if has(&["glass", "window", "glazing", "windshield"]) {
        Kind::Glass
    } else if has(&["metal", "steel", "chrome", "alumin", "iron", "copper", "brass"]) {
        Kind::Metal
    } else if has(&["light", "lamp", "glow", "emissive", "neon"]) {
        Kind::Light
    } else {
        Kind::Paint
    }
}

// ----- geometry ---------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Tri {
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
    n: Vec3,
    mat: u32,
}

#[derive(Clone, Copy)]
struct Aabb {
    lo: Vec3,
    hi: Vec3,
}

impl Aabb {
    fn empty() -> Self {
        Aabb { lo: Vec3::splat(f32::MAX), hi: Vec3::splat(f32::MIN) }
    }
    fn grow(&mut self, p: Vec3) {
        self.lo = self.lo.min(p);
        self.hi = self.hi.max(p);
    }
    fn join(&mut self, b: &Aabb) {
        self.lo = self.lo.min(b.lo);
        self.hi = self.hi.max(b.hi);
    }
    fn area(&self) -> f32 {
        let d = (self.hi - self.lo).max(Vec3::ZERO);
        2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
    }
    /// Entry distance of a ray, if it hits before `tmax`.
    #[inline]
    fn hit(&self, o: Vec3, inv: Vec3, tmax: f32) -> Option<f32> {
        let t0 = (self.lo - o) * inv;
        let t1 = (self.hi - o) * inv;
        let tn = t0.min(t1).max_element();
        let tf = t0.max(t1).min_element();
        (tf >= tn.max(0.0) && tn < tmax).then_some(tn)
    }
}

struct Node {
    b: Aabb,
    /// leaf: first triangle and count; inner: left child index (right = left + 1), count 0
    start: u32,
    count: u32,
}

struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<Tri>,
}

impl Bvh {
    fn build(mut tris: Vec<Tri>) -> Bvh {
        let n = tris.len();
        let bounds = |t: &Tri| {
            let mut b = Aabb::empty();
            b.grow(t.a);
            b.grow(t.a + t.e1);
            b.grow(t.a + t.e2);
            b
        };
        let mut boxes: Vec<Aabb> = tris.iter().map(bounds).collect();
        let mut cent: Vec<Vec3> = boxes.iter().map(|b| (b.lo + b.hi) * 0.5).collect();
        let mut nodes = vec![Node { b: Aabb::empty(), start: 0, count: n as u32 }];
        let mut stack = vec![0usize];
        while let Some(ni) = stack.pop() {
            let (s, c) = (nodes[ni].start as usize, nodes[ni].count as usize);
            let mut b = Aabb::empty();
            let mut cb = Aabb::empty();
            for i in s..s + c {
                b.join(&boxes[i]);
                cb.grow(cent[i]);
            }
            nodes[ni].b = b;
            if c <= 4 {
                continue;
            }
            // binned SAH on the widest centroid axis
            let ext = cb.hi - cb.lo;
            let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
            if ext[axis] < 1e-7 {
                continue;
            }
            const BINS: usize = 12;
            let mut bb = [Aabb::empty(); BINS];
            let mut bc = [0usize; BINS];
            let bin = |p: Vec3| (((p[axis] - cb.lo[axis]) / ext[axis] * BINS as f32) as usize).min(BINS - 1);
            for i in s..s + c {
                let k = bin(cent[i]);
                bb[k].join(&boxes[i]);
                bc[k] += 1;
            }
            let (mut best, mut best_cost) = (0, f32::MAX);
            for split in 1..BINS {
                let (mut l, mut r) = (Aabb::empty(), Aabb::empty());
                let (mut lc, mut rc) = (0, 0);
                for k in 0..split {
                    if bc[k] > 0 {
                        l.join(&bb[k]);
                        lc += bc[k];
                    }
                }
                for k in split..BINS {
                    if bc[k] > 0 {
                        r.join(&bb[k]);
                        rc += bc[k];
                    }
                }
                if lc == 0 || rc == 0 {
                    continue;
                }
                let cost = l.area() * lc as f32 + r.area() * rc as f32;
                if cost < best_cost {
                    best_cost = cost;
                    best = split;
                }
            }
            if best == 0 || best_cost >= b.area() * c as f32 {
                continue; // splitting doesn't pay: leaf
            }
            // partition in place
            let (mut i, mut j) = (s, s + c);
            while i < j {
                if bin(cent[i]) < best {
                    i += 1;
                } else {
                    j -= 1;
                    tris.swap(i, j);
                    boxes.swap(i, j);
                    cent.swap(i, j);
                }
            }
            let left = nodes.len();
            nodes.push(Node { b: Aabb::empty(), start: s as u32, count: (i - s) as u32 });
            nodes.push(Node { b: Aabb::empty(), start: i as u32, count: (s + c - i) as u32 });
            nodes[ni].start = left as u32;
            nodes[ni].count = 0;
            stack.push(left);
            stack.push(left + 1);
        }
        Bvh { nodes, tris }
    }

    /// Nearest hit: (t, triangle index, u, v).
    fn hit(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, usize)> {
        if self.tris.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / d;
        let mut best: Option<(f32, usize)> = None;
        let mut tbest = tmax;
        let mut stack = [0u32; 64];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            if node.b.hit(o, inv, tbest).is_none() {
                continue;
            }
            if node.count > 0 {
                for i in node.start..node.start + node.count {
                    let t = &self.tris[i as usize];
                    // Moller-Trumbore
                    let p = d.cross(t.e2);
                    let det = t.e1.dot(p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let inv_det = 1.0 / det;
                    let s = o - t.a;
                    let u = s.dot(p) * inv_det;
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let q = s.cross(t.e1);
                    let v = d.dot(q) * inv_det;
                    if v < 0.0 || u + v > 1.0 {
                        continue;
                    }
                    let tt = t.e2.dot(q) * inv_det;
                    if tt > 1e-4 && tt < tbest {
                        tbest = tt;
                        best = Some((tt, i as usize));
                    }
                }
            } else {
                let (l, r) = (node.start, node.start + 1);
                // nearer child last, so it's popped first
                let dl = self.nodes[l as usize].b.hit(o, inv, tbest);
                let dr = self.nodes[r as usize].b.hit(o, inv, tbest);
                match (dl, dr) {
                    (Some(a), Some(b)) => {
                        if sp + 2 > stack.len() {
                            continue;
                        }
                        if a < b {
                            stack[sp] = r;
                            stack[sp + 1] = l;
                        } else {
                            stack[sp] = l;
                            stack[sp + 1] = r;
                        }
                        sp += 2;
                    }
                    (Some(_), None) => {
                        stack[sp] = l;
                        sp += 1;
                    }
                    (None, Some(_)) => {
                        stack[sp] = r;
                        sp += 1;
                    }
                    _ => {}
                }
            }
        }
        best
    }
}

// ----- the scene ---------------------------------------------------------------------

pub struct PtOpts {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub bounces: u32,
    pub camera: CameraSpec,
    /// direction toward the sun (unit), and how bright
    pub sun: DVec3,
    pub sun_strength: f32,
    pub denoise: bool,
    pub exposure: f32,
}

struct Scene {
    bvh: Bvh,
    mats: Vec<Mat>,
    textures: Vec<Texture>,
    ground_z: f32,
    sun: Vec3,
    sun_strength: f32,
}

const DEFAULT: [f32; 3] = [0.95, 0.94, 0.91];
const SUN_COLOR: Vec3 = Vec3::new(1.0, 0.94, 0.86);
/// cosine of the sun's half-angle (a bit bigger than the real sun: softer shadows)
const SUN_COS: f32 = 0.99995;

fn build_scene(m: &Model, o: &PtOpts) -> Scene {
    let mut mats: Vec<Mat> = Vec::new();
    let mut textures: Vec<Texture> = Vec::new();
    let mut by_key: HashMap<String, u32> = HashMap::new();
    let mut tex_by_name: HashMap<String, usize> = HashMap::new();
    let mut tris = Vec::new();
    let mut zmin = f32::MAX;
    for f in m.world_faces() {
        let name = f.material.clone().unwrap_or_default();
        let color = f.color.unwrap_or(DEFAULT);
        // an image laid on the face maps along the face: one entry per direction
        let along_face = m.find_material(&name).and_then(|mm| mm.texture.as_ref()).map_or(false, |t| t.project.is_none());
        let n = f.normal.normalize_or_zero();
        let key = if along_face { format!("{name}|{color:?}|{:.3},{:.3},{:.3}", n.x, n.y, n.z) } else { format!("{name}|{color:?}") };
        let mat = *by_key.entry(key).or_insert_with(|| {
            let kind = kind_of(&name);
            let lin = Vec3::new(srgb_to_linear(color[0]), srgb_to_linear(color[1]), srgb_to_linear(color[2]));
            // a texture, if the material has an image
            let tex = m.find_material(&name).and_then(|mm| mm.texture.as_ref()).and_then(|t| {
                let idx = match tex_by_name.get(&name) {
                    Some(i) => *i,
                    None => {
                        use base64::Engine as _;
                        let b64 = t.data.split_once(',')?.1;
                        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
                        let img = image::load_from_memory(&bytes).ok()?.to_rgb8();
                        let px = img.pixels().map(|p| Vec3::new(srgb_to_linear(p[0] as f32 / 255.0), srgb_to_linear(p[1] as f32 / 255.0), srgb_to_linear(p[2] as f32 / 255.0))).collect();
                        textures.push(Texture { w: img.width() as usize, h: img.height() as usize, px });
                        tex_by_name.insert(name.clone(), textures.len() - 1);
                        textures.len() - 1
                    }
                };
                // planar mapping as the viewport does it
                let (tu, tv, origin) = match t.project {
                    Some(dir) => {
                        let (u, v) = crate::geom::basis(-DVec3::from(dir).normalize());
                        (u, v, t.origin.map(DVec3::from).unwrap_or_default())
                    }
                    None => {
                        let (u, v) = crate::geom::basis(f.normal.normalize());
                        (u, v, DVec3::ZERO)
                    }
                };
                Some((idx, tu.as_vec3(), tv.as_vec3(), origin.as_vec3(), t.width.max(1e-6) as f32, t.height.max(1e-6) as f32))
            });
            let rough = match kind {
                Kind::Metal => 0.3,
                _ => 0.6,
            };
            mats.push(Mat { kind, color: lin, rough, tex });
            (mats.len() - 1) as u32
        });
        for t in crate::geom::triangulate(&f.outer, &f.holes, f.normal) {
            let (a, b, c) = (t[0].as_vec3(), t[1].as_vec3(), t[2].as_vec3());
            zmin = zmin.min(a.z).min(b.z).min(c.z);
            let n = (b - a).cross(c - a).normalize_or_zero();
            if n == Vec3::ZERO {
                continue;
            }
            tris.push(Tri { a, e1: b - a, e2: c - a, n, mat });
        }
    }
    Scene {
        bvh: Bvh::build(tris),
        mats,
        textures,
        ground_z: if zmin.is_finite() { zmin } else { 0.0 },
        sun: o.sun.as_vec3().normalize(),
        sun_strength: o.sun_strength,
    }
}

// ----- sampling ----------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }
    #[inline]
    fn f(&mut self) -> f32 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }
}

/// Orthonormal frame around n.
fn frame(n: Vec3) -> (Vec3, Vec3) {
    let s = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    (Vec3::new(1.0 + s * n.x * n.x * a, s * b, -s * n.x), Vec3::new(b, s + n.y * n.y * a, -n.y))
}

fn cosine_dir(n: Vec3, r: &mut Rng) -> Vec3 {
    let (u1, u2) = (r.f(), r.f());
    let rr = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let (t, b) = frame(n);
    (t * (rr * phi.cos()) + b * (rr * phi.sin()) + n * (1.0 - u1).max(0.0).sqrt()).normalize()
}

fn ggx_d(nh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = nh * nh * (a2 - 1.0) + 1.0;
    a2 / (std::f32::consts::PI * d * d).max(1e-12)
}

fn smith_g1(nv: f32, a: f32) -> f32 {
    let a2 = a * a;
    2.0 * nv / (nv + (a2 + (1.0 - a2) * nv * nv).sqrt()).max(1e-12)
}

fn schlick(f0: Vec3, c: f32) -> Vec3 {
    f0 + (Vec3::ONE - f0) * (1.0 - c).clamp(0.0, 1.0).powi(5)
}

/// The sky's light from direction `d` (z up), not counting the sun disk.
fn sky(d: Vec3, sun: Vec3) -> Vec3 {
    let t = d.z.max(0.0);
    let horizon = Vec3::new(0.70, 0.78, 0.90);
    let zenith = Vec3::new(0.18, 0.34, 0.75);
    let base = horizon.lerp(zenith, t.sqrt());
    // brighter toward the sun
    let glow = d.dot(sun).max(0.0).powi(8) * 0.5;
    (base + Vec3::new(1.0, 0.9, 0.75) * glow) * 0.55
}

impl Scene {
    fn material_at(&self, ti: usize, p: Vec3) -> (Kind, Vec3, f32) {
        let tr = &self.bvh.tris[ti];
        let m = &self.mats[tr.mat as usize];
        let mut c = m.color;
        if let Some((idx, tu, tv, origin, w, h)) = m.tex {
            let q = p - origin;
            c = self.textures[idx].sample(q.dot(tu) / w, q.dot(tv) / h);
        }
        (m.kind, c, m.rough)
    }

    /// Light arriving straight from the sun at `p` (glass lets it through, tinted).
    fn sun_visible(&self, p: Vec3, sun: Vec3) -> Vec3 {
        let mut o = p;
        let mut through = Vec3::ONE;
        for _ in 0..4 {
            match self.bvh.hit(o, sun, 1e30) {
                None => return through,
                Some((t, ti)) => {
                    if self.mats[self.bvh.tris[ti].mat as usize].kind != Kind::Glass {
                        return Vec3::ZERO;
                    }
                    through *= Vec3::splat(0.85).lerp(self.mats[self.bvh.tris[ti].mat as usize].color, 0.3);
                    o = o + sun * (t + 1e-3);
                }
            }
        }
        through
    }

    /// One path: returns radiance, plus the first hit's albedo and normal.
    fn trace(&self, mut o: Vec3, mut d: Vec3, bounces: u32, r: &mut Rng) -> (Vec3, Vec3, Vec3) {
        let mut l = Vec3::ZERO;
        let mut beta = Vec3::ONE;
        let (mut first_albedo, mut first_n) = (Vec3::ZERO, Vec3::ZERO);
        let mut specular = true; // camera and mirror-like bounces see the sun disk
        let sun = self.sun;
        let sun_e = SUN_COLOR * self.sun_strength;
        for depth in 0..bounces.max(1) {
            // the model, or the ground plane under it
            let hit = self.bvh.hit(o, d, 1e30);
            let tg = if d.z < -1e-6 { (self.ground_z - o.z) / d.z } else { f32::MAX };
            let (t, n, kind, albedo, rough) = match hit {
                Some((t, ti)) if t < tg => {
                    let tr = &self.bvh.tris[ti];
                    let p = o + d * t;
                    let (k, c, rr) = self.material_at(ti, p);
                    let mut n = tr.n;
                    if k != Kind::Glass && n.dot(d) > 0.0 {
                        n = -n; // shade the side we see
                    }
                    (t, n, k, c, rr)
                }
                _ if tg > 1e-4 && tg < f32::MAX => (tg, Vec3::Z, Kind::Paint, Vec3::new(0.30, 0.30, 0.27), 0.9),
                _ => {
                    let mut s = sky(d, sun);
                    if specular && d.dot(sun) > SUN_COS {
                        s += sun_e * 2000.0;
                    }
                    l += beta * s;
                    break;
                }
            };
            let p = o + d * t;
            if depth == 0 {
                first_albedo = albedo;
                first_n = n;
            }
            match kind {
                Kind::Light => {
                    l += beta * albedo * 6.0;
                    break;
                }
                Kind::Glass => {
                    // thin glass (two surfaces): reflect by Fresnel, else pass
                    // straight through, tinted and dimmed like real glazing
                    let c = n.dot(-d).abs();
                    let f = (schlick(Vec3::splat(0.04), c).x * 2.0).clamp(0.12, 1.0);
                    if r.f() < f {
                        let nn = if n.dot(d) > 0.0 { -n } else { n };
                        d = d - nn * 2.0 * d.dot(nn);
                    } else {
                        beta *= Vec3::splat(0.55).lerp(albedo * 0.7, 0.35);
                    }
                    o = p + d * 1e-3;
                    specular = true;
                    continue;
                }
                _ => {}
            }
            let metal = kind == Kind::Metal;
            let a = (rough * rough).max(0.02);
            let f0 = if metal { albedo } else { Vec3::splat(0.04) };
            let diffuse = if metal { Vec3::ZERO } else { albedo };
            let wo = -d;
            let nwo = n.dot(wo).max(1e-4);
            // direct sun
            let cs = n.dot(sun);
            if cs > 0.0 {
                // jitter within the sun disk for soft shadows
                let (tu, tv) = frame(sun);
                let rad = (1.0 - SUN_COS * SUN_COS).sqrt();
                let jit = (sun + tu * (r.f() - 0.5) * 2.0 * rad + tv * (r.f() - 0.5) * 2.0 * rad).normalize();
                let vis = self.sun_visible(p + n * 1e-3, jit);
                if vis != Vec3::ZERO {
                    let h = (sun + wo).normalize();
                    let nh = n.dot(h).max(0.0);
                    let fr = schlick(f0, h.dot(wo).max(0.0));
                    let spec = fr * (ggx_d(nh, a) * smith_g1(nwo, a) * smith_g1(cs, a) / (4.0 * nwo * cs).max(1e-6));
                    let kd = (Vec3::ONE - fr) * diffuse / std::f32::consts::PI;
                    l += beta * vis * sun_e * (kd + spec) * cs;
                }
            }
            // next direction: diffuse or specular lobe
            let p_spec = if metal { 1.0 } else { 0.25 };
            let wi = if r.f() < p_spec {
                // sample a GGX microfacet normal
                let (u1, u2) = (r.f(), r.f());
                let th = (a * (u1 / (1.0 - u1).max(1e-6)).sqrt()).atan();
                let ph = std::f32::consts::TAU * u2;
                let (tu, tv) = frame(n);
                let h = (tu * (th.sin() * ph.cos()) + tv * (th.sin() * ph.sin()) + n * th.cos()).normalize();
                d - h * 2.0 * d.dot(h)
            } else {
                cosine_dir(n, r)
            };
            let ci = n.dot(wi);
            if ci <= 0.0 {
                break;
            }
            let h = (wi + wo).normalize();
            let nh = n.dot(h).max(0.0);
            let hw = h.dot(wo).max(1e-6);
            let pdf_spec = ggx_d(nh, a) * nh / (4.0 * hw);
            let pdf = p_spec * pdf_spec + (1.0 - p_spec) * ci / std::f32::consts::PI;
            if pdf < 1e-8 {
                break;
            }
            let fr = schlick(f0, hw);
            let spec = fr * (ggx_d(nh, a) * smith_g1(nwo, a) * smith_g1(ci, a) / (4.0 * nwo * ci).max(1e-6));
            let kd = (Vec3::ONE - fr) * diffuse / std::f32::consts::PI;
            beta *= (kd + spec) * ci / pdf;
            specular = rough < 0.15 && r.f() < p_spec;
            // Russian roulette
            if depth >= 3 {
                let q = beta.max_element().min(0.95);
                if r.f() > q {
                    break;
                }
                beta /= q;
            }
            o = p + n * 1e-3;
            d = wi;
        }
        (l, first_albedo, first_n)
    }
}

// ----- image ------------------------------------------------------------------------------

fn aces(x: Vec3) -> Vec3 {
    let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
    ((x * (x * a + b)) / (x * (x * c + d) + e)).clamp(Vec3::ZERO, Vec3::ONE)
}

fn to_srgb(c: f32) -> u8 {
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Edge-aware a-trous filter: smooths noise but keeps edges where albedo or
/// normals change. Works on demodulated light (color / albedo).
fn denoise(color: &[Vec3], albedo: &[Vec3], normal: &[Vec3], w: usize, h: usize) -> Vec<Vec3> {
    let eps = Vec3::splat(0.02);
    let mut light: Vec<Vec3> = color.iter().zip(albedo).map(|(c, a)| *c / (*a + eps)).collect();
    const K: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let rows_per = h.div_ceil(threads).max(1);
    for pass in 0..5 {
        let step = 1usize << pass;
        let src = light.clone();
        let sigma_l = 4.0 / (1 << pass) as f32 + 0.5;
        let src = &src;
        // rows in parallel: each thread fills its own band of `light`
        std::thread::scope(|s| {
            for (band, out) in light.chunks_mut(rows_per * w).enumerate() {
                s.spawn(move || {
                    let y0 = band * rows_per;
                    for (r, row) in out.chunks_mut(w).enumerate() {
                        let y = y0 + r;
                        for x in 0..w {
                            let i = y * w + x;
                            let (c0, n0, a0) = (src[i], normal[i], albedo[i]);
                            let l0 = c0.dot(Vec3::new(0.2126, 0.7152, 0.0722));
                            let (mut sum, mut wsum) = (Vec3::ZERO, 0.0f32);
                            for (ky, wy) in K.iter().enumerate() {
                                let yy = y as isize + (ky as isize - 2) * step as isize;
                                if yy < 0 || yy >= h as isize {
                                    continue;
                                }
                                for (kx, wx) in K.iter().enumerate() {
                                    let xx = x as isize + (kx as isize - 2) * step as isize;
                                    if xx < 0 || xx >= w as isize {
                                        continue;
                                    }
                                    let j = yy as usize * w + xx as usize;
                                    let wn = n0.dot(normal[j]).max(0.0).powi(32);
                                    if wn < 1e-4 {
                                        continue;
                                    }
                                    let lj = src[j].dot(Vec3::new(0.2126, 0.7152, 0.0722));
                                    // albedo and brightness weights in one exp
                                    let wt = wy * wx * wn * (-(a0 - albedo[j]).length_squared() * 60.0 - (l0 - lj).abs() / sigma_l).exp();
                                    sum += src[j] * wt;
                                    wsum += wt;
                                }
                            }
                            row[x] = if wsum > 0.0 { sum / wsum } else { c0 };
                        }
                    }
                });
            }
        });
    }
    light.iter().zip(albedo).map(|(l, a)| *l * (*a + eps)).collect()
}

/// Render a photoreal image of the model.
pub fn render(m: &Model, o: &PtOpts) -> RgbImage {
    let scene = build_scene(m, o);
    let (w, h) = (o.width.max(16) as usize, o.height.max(16) as usize);
    let (c, a, n) = pass(&scene, o, w, h, 0, o.samples.max(1));
    finish(&c, &a, &n, o.samples.max(1), w, h, o)
}

/// A render in progress: the scene, and every sample so far summed per
/// pixel. Each `progressive` call adds more and returns the picture so far.
pub struct Progress {
    /// what it's a render of (camera, size, light, model revision)
    pub key: String,
    scene: Scene,
    w: usize,
    h: usize,
    color: Vec<Vec3>,
    albedo: Vec<Vec3>,
    normal: Vec<Vec3>,
    /// samples per pixel so far
    pub spp: u32,
    passes: u32,
}

/// Add `o.samples` samples per pixel to `prev` (when it's of the same
/// thing, `key`), or start over. Returns the progress and the denoised
/// picture so far.
pub fn progressive(prev: Option<Progress>, key: String, m: &Model, o: &PtOpts) -> (Progress, RgbImage) {
    let (w, h) = (o.width.max(16) as usize, o.height.max(16) as usize);
    let mut p = match prev {
        Some(p) if p.key == key && p.w == w && p.h == h => p,
        _ => Progress { key, scene: build_scene(m, o), w, h, color: vec![Vec3::ZERO; w * h], albedo: vec![Vec3::ZERO; w * h], normal: vec![Vec3::ZERO; w * h], spp: 0, passes: 0 },
    };
    let add = o.samples.max(1);
    let (c, a, n) = pass(&p.scene, o, w, h, p.passes + 1, add);
    for i in 0..w * h {
        p.color[i] += c[i];
        p.albedo[i] += a[i];
        p.normal[i] += n[i];
    }
    p.spp += add;
    p.passes += 1;
    let img = finish(&p.color, &p.albedo, &p.normal, p.spp, w, h, o);
    (p, img)
}

/// Sums of `spp` samples per pixel (color, albedo, normal), on all cores.
/// `seed` makes each pass's samples different from the last.
fn pass(scene: &Scene, o: &PtOpts, w: usize, h: usize, seed: u32, spp: u32) -> (Vec<Vec3>, Vec<Vec3>, Vec<Vec3>) {
    let eye = DVec3::from(o.camera.eye).as_vec3();
    let target = DVec3::from(o.camera.target).as_vec3();
    let fwd = (target - eye).normalize();
    let up0 = if fwd.cross(Vec3::Z).length() < 1e-3 { Vec3::Y } else { Vec3::Z };
    let right = fwd.cross(up0).normalize();
    let up = right.cross(fwd);
    let tan = (o.camera.fov.to_radians() as f32 * 0.5).tan();
    let aspect = w as f32 / h as f32;

    let mut color = vec![Vec3::ZERO; w * h];
    let mut albedo = vec![Vec3::ZERO; w * h];
    let mut normal = vec![Vec3::ZERO; w * h];
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let rows: Vec<usize> = (0..h).collect();
    let chunk = h.div_ceil(threads * 4);
    std::thread::scope(|s| {
        let mut parts: Vec<(&mut [Vec3], &mut [Vec3], &mut [Vec3], &[usize])> = Vec::new();
        let (mut c, mut a, mut n, mut rr) = (&mut color[..], &mut albedo[..], &mut normal[..], &rows[..]);
        while !rr.is_empty() {
            let k = chunk.min(rr.len());
            let (c1, c2) = c.split_at_mut(k * w);
            let (a1, a2) = a.split_at_mut(k * w);
            let (n1, n2) = n.split_at_mut(k * w);
            let (r1, r2) = rr.split_at(k);
            parts.push((c1, a1, n1, r1));
            c = c2;
            a = a2;
            n = n2;
            rr = r2;
        }
        // hand chunks out round-robin to the threads
        let mut buckets: Vec<Vec<(&mut [Vec3], &mut [Vec3], &mut [Vec3], &[usize])>> = (0..threads).map(|_| Vec::new()).collect();
        for (i, p) in parts.into_iter().enumerate() {
            buckets[i % threads].push(p);
        }
        for bucket in buckets {
            s.spawn(move || {
                for (cs, als, ns, ys) in bucket {
                    for (row, &y) in ys.iter().enumerate() {
                        for x in 0..w {
                            let mut r = Rng::new(((y * w + x) as u64) << 8 | 1 ^ (seed as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
                            let (mut acc, mut aa, mut an) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
                            for _ in 0..spp {
                                let px = (x as f32 + r.f()) / w as f32 * 2.0 - 1.0;
                                let py = 1.0 - (y as f32 + r.f()) / h as f32 * 2.0;
                                let d = (fwd + right * (px * tan * aspect) + up * (py * tan)).normalize();
                                let (l, alb, nn) = scene.trace(eye, d, o.bounces, &mut r);
                                // keep fireflies in check
                                let lum = l.max_element();
                                acc += if lum > 40.0 { l * (40.0 / lum) } else { l };
                                aa += alb;
                                an += nn;
                            }
                            let i = row * w + x;
                            cs[i] = acc;
                            als[i] = aa;
                            ns[i] = an;
                        }
                    }
                }
            });
        }
    });
    (color, albedo, normal)
}

/// Averages, denoise, tone map.
fn finish(color: &[Vec3], albedo: &[Vec3], normal: &[Vec3], spp: u32, w: usize, h: usize, o: &PtOpts) -> RgbImage {
    let k = 1.0 / spp.max(1) as f32;
    let color: Vec<Vec3> = color.iter().map(|c| *c * k).collect();
    let albedo: Vec<Vec3> = albedo.iter().map(|c| *c * k).collect();
    let normal: Vec<Vec3> = normal.iter().map(|c| c.normalize_or_zero()).collect();
    let final_color = if o.denoise { denoise(&color, &albedo, &normal, w, h) } else { color };
    let mut img = RgbImage::new(w as u32, h as u32);
    for (i, p) in img.pixels_mut().enumerate() {
        let c = aces(final_color[i] * o.exposure);
        *p = Rgb([to_srgb(c.x), to_srgb(c.y), to_srgb(c.z)]);
    }
    img
}

/// The camera for a render request, as the regular renderer picks it.
pub fn camera(m: &Model, opts: &RenderOpts) -> CameraSpec {
    render::camera_for(m, opts)
}

/// Where the sun is (compass azimuth from north, clockwise; altitude above
/// the horizon; degrees) at a place and local clock time. NOAA's solar
/// position equations: good to a fraction of a degree for 1900-2100.
/// `utc_offset` is the local clock's hours ahead of UTC (-7 for US Pacific
/// summer time).
pub fn solar_position(lat: f64, lon: f64, year: i32, month: u32, day: u32, hour: f64, utc_offset: f64) -> (f64, f64) {
    // day of the year
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][(month.clamp(1, 12) - 1) as usize] + if leap && month > 2 { 1 } else { 0 };
    let n = (before + day) as f64;
    let days_in_year = if leap { 366.0 } else { 365.0 };
    let g = 2.0 * std::f64::consts::PI / days_in_year * (n - 1.0 + (hour - 12.0) / 24.0);
    // equation of time (minutes) and declination (radians)
    let eqt = 229.18 * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin() - 0.002697 * (3.0 * g).cos() + 0.00148 * (3.0 * g).sin();
    // true solar time (minutes) and hour angle
    let tst = hour * 60.0 + eqt + 4.0 * lon - 60.0 * utc_offset;
    let ha = (tst / 4.0 - 180.0).to_radians();
    let phi = lat.to_radians();
    let cos_zen = (phi.sin() * decl.sin() + phi.cos() * decl.cos() * ha.cos()).clamp(-1.0, 1.0);
    let zen = cos_zen.acos();
    let altitude = 90.0 - zen.to_degrees();
    // azimuth, clockwise from north
    let az = (decl.sin() - phi.sin() * cos_zen) / (phi.cos() * zen.sin()).max(1e-9);
    let mut azimuth = az.clamp(-1.0, 1.0).acos().to_degrees();
    if ha > 0.0 {
        azimuth = 360.0 - azimuth;
    }
    (azimuth, altitude)
}

/// Sun direction from compass azimuth (degrees from north, clockwise) and altitude.
pub fn sun_dir(azimuth: f64, altitude: f64) -> DVec3 {
    let (az, al) = (azimuth.to_radians(), altitude.to_radians());
    // +Y is north, +X east
    DVec3::new(az.sin() * al.cos(), az.cos() * al.cos(), al.sin()).normalize()
}
