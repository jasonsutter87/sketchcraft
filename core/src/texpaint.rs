//! Texture painting: brush strokes that change the pixels of a material's embedded PNG, so a model can be painted on its
//! UV layout. The maths are plain functions (tested on their own); the `Model` methods find the texels a stroke reaches
//! and write the picture back. Design notes: docs/dev/texture-paint.md.
//!
//! A stroke is kept as stroke data, not as a pixel patch: the command carries the brush's path and the engine paints it.
//! The result of a stroke depends only on the picture before it and the stroke (never on how the app batched it), so a
//! file stays deterministic and a replayed stroke gives the same bytes. Undo is the engine's usual whole-step snapshot.

use crate::geom;
use crate::model::{Id, Material, Model, Texture};
use crate::texposition::Frames;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use glam::DVec3;
use std::collections::HashMap;

/// Smallest and biggest paint texture the commands make (pixels on a side).
pub const MIN_SIZE: u32 = 64;
pub const MAX_SIZE: u32 = 4096;
/// How far (in pixels) a triangle's texels are grown so a seam leaves no unpainted line between islands.
const BLEED: f64 = 0.75;
/// The most points one stroke carries (the app thins a long drag to 1,500).
pub const MAX_STROKE_POINTS: usize = 10_000;
/// The biggest brush on the picture, in tiles of its width (1: the whole picture; the app's largest is 0.5).
pub const MAX_UV_RADIUS: f64 = 1.0;
/// The most texels one stroke may look at, counted per dab (and per triangle a dab reaches), about a second of work:
/// a stroke past it is refused before anything is painted. The app spaces a drag's points by the brush size, so even
/// its biggest brush on a 2048 picture covers dozens of picture widths of drag in one stroke.
pub const MAX_STROKE_WORK: u64 = 1 << 30;

/// Count `n` more units of a stroke's work; refused once the stroke is past `MAX_STROKE_WORK`.
fn spend(work: &mut u64, n: u64) -> Result<()> {
    *work = work.saturating_add(n);
    if *work > MAX_STROKE_WORK {
        bail!("that stroke is too much work at once (over {} million texels to look at): use a smaller brush, fewer points or fewer faces", MAX_STROKE_WORK / 1_000_000);
    }
    Ok(())
}

// ----- pixels -------------------------------------------------------------------------------------------------------

/// An RGBA8 picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn filled(w: u32, h: u32, rgb: [u8; 3]) -> Canvas {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            px.extend([rgb[0], rgb[1], rgb[2], 255]);
        }
        Canvas { w, h, px }
    }

    /// Decode a PNG or JPEG data URL.
    pub fn from_data_url(data: &str) -> Result<Canvas> {
        let (head, b64) = data.split_once(',').context("the texture isn't a data URL")?;
        if !head.contains(";base64") {
            bail!("the texture data isn't base64");
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).context("the texture data isn't valid base64")?;
        let img = crate::safety::decode_image(&bytes, None)?.to_rgba8();
        Ok(Canvas { w: img.width(), h: img.height(), px: img.into_raw() })
    }

    /// Encode as a PNG data URL. The encoder settings are fixed, so the same pixels always give the same text.
    pub fn to_data_url(&self) -> Result<String> {
        use image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use image::ImageEncoder;
        let mut out = Vec::new();
        PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive).write_image(&self.px, self.w, self.h, image::ExtendedColorType::Rgba8)?;
        Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = ((y * self.w + x) * 4) as usize;
        self.px[i..i + 4].copy_from_slice(&c);
    }

    /// A 64-bit FNV-1a of the size and every pixel (tests and `texture_pixels` use it to tell pictures apart).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in self.w.to_le_bytes().into_iter().chain(self.h.to_le_bytes()).chain(self.px.iter().copied()) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

/// The pixel (x right, y down, fractional) a texture coordinate (tiles, v up) lands on.
pub fn uv_to_pixel(uv: [f64; 2], w: u32, h: u32) -> [f64; 2] {
    [uv[0] * w as f64, (1.0 - uv[1]) * h as f64]
}

/// The texture coordinate of a pixel position (the inverse of `uv_to_pixel`).
pub fn pixel_to_uv(p: [f64; 2], w: u32, h: u32) -> [f64; 2] {
    [p[0] / w as f64, 1.0 - p[1] / h as f64]
}

// ----- brush maths --------------------------------------------------------------------------------------------------

/// Brush weight 0..1 at distance `d` from the centre of a brush of radius `r`: full inside `hardness * r`, then a smooth
/// fall to nothing at `r`. Hardness 1 is a hard disc; 0 is a fully soft brush.
pub fn falloff(d: f64, r: f64, hardness: f64) -> f64 {
    if r <= 0.0 || d >= r {
        return 0.0;
    }
    let inner = hardness.clamp(0.0, 1.0) * r;
    if d <= inner {
        return 1.0;
    }
    let t = ((d - inner) / (r - inner)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// One pixel moved toward `target` (rgb, 0..1) by `w` (0 keeps it, 1 replaces it). Alpha is left alone.
pub fn blend_px(old: [u8; 4], target: [f32; 3], w: f64) -> [u8; 4] {
    let w = w.clamp(0.0, 1.0);
    let ch = |o: u8, t: f32| -> u8 { (o as f64 + (t.clamp(0.0, 1.0) as f64 * 255.0 - o as f64) * w).round().clamp(0.0, 255.0) as u8 };
    [ch(old[0], target[0]), ch(old[1], target[1]), ch(old[2], target[2]), old[3]]
}

/// What one stroke paints with.
#[derive(Clone, Debug)]
pub struct Brush {
    pub color: [f32; 3],
    /// 0..1: how far a texel under the middle of the brush moves toward the colour
    pub opacity: f64,
    /// 0..1, see `falloff`
    pub hardness: f64,
    /// paint back toward `base` (the material's own colour) instead of `color`
    pub erase: bool,
    pub base: [f32; 3],
}

impl Brush {
    pub fn parse_color(c: &[f32]) -> Result<[f32; 3]> {
        if c.len() != 3 && c.len() != 4 {
            bail!("color is [r, g, b] each 0..1");
        }
        if c.iter().any(|x| !x.is_finite()) {
            bail!("color must be numbers");
        }
        Ok([c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0)])
    }
}

/// Weights per texel (max over the stroke's dabs: a stroke never piles up where dabs overlap), 0 where untouched.
pub struct Weights {
    pub w: u32,
    pub h: u32,
    pub v: Vec<f32>,
}

impl Weights {
    pub fn new(w: u32, h: u32) -> Weights {
        Weights { w, h, v: vec![0.0; (w * h) as usize] }
    }
    fn raise(&mut self, x: u32, y: u32, w: f64) {
        let i = (y * self.w + x) as usize;
        if w as f32 > self.v[i] {
            self.v[i] = w as f32;
        }
    }
}

/// A triangle of a face: its corners in 3D and where they land on the picture (pixels).
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [DVec3; 3],
    pub px: [[f64; 2]; 3],
}

/// Barycentric coordinates of pixel point `q` in the pixel triangle (None for a degenerate triangle).
fn bary(t: &[[f64; 2]; 3], q: [f64; 2]) -> Option<[f64; 3]> {
    let d = (t[1][1] - t[2][1]) * (t[0][0] - t[2][0]) + (t[2][0] - t[1][0]) * (t[0][1] - t[2][1]);
    if d.abs() < 1e-12 {
        return None;
    }
    let l0 = ((t[1][1] - t[2][1]) * (q[0] - t[2][0]) + (t[2][0] - t[1][0]) * (q[1] - t[2][1])) / d;
    let l1 = ((t[2][1] - t[0][1]) * (q[0] - t[2][0]) + (t[0][0] - t[2][0]) * (q[1] - t[2][1])) / d;
    Some([l0, l1, 1.0 - l0 - l1])
}

fn seg_dist(a: [f64; 2], b: [f64; 2], q: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-18 { 0.0 } else { (((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    ((a[0] + dx * t - q[0]).powi(2) + (a[1] + dy * t - q[1]).powi(2)).sqrt()
}

/// The texels of a pixel triangle inside the pixel box `lo..=hi` (clipped to the picture): calls `f(x, y, bary)` for each
/// texel whose centre is inside the triangle or within `BLEED` pixels of it (the barycentric coordinates are then pulled
/// back inside, so a point on the triangle is used).
pub fn raster(t: &[[f64; 2]; 3], w: u32, h: u32, lo: [f64; 2], hi: [f64; 2], f: impl FnMut(u32, u32, [f64; 3])) {
    raster_with(t, w, h, lo, hi, BLEED, f)
}

/// As `raster`, with the bleed given; a negative bleed takes only texels strictly inside (no growing, none on an edge).
pub fn raster_with(t: &[[f64; 2]; 3], w: u32, h: u32, lo: [f64; 2], hi: [f64; 2], bleed: f64, mut f: impl FnMut(u32, u32, [f64; 3])) {
    let Some((x0, x1, y0, y1)) = raster_span(t, w, h, lo, hi, bleed) else { return };
    for y in y0..=y1 {
        for x in x0..=x1 {
            let q = [x as f64 + 0.5, y as f64 + 0.5];
            let Some(mut l) = bary(t, q) else { return };
            if bleed < 0.0 {
                if l.iter().any(|v| *v <= 1e-9) {
                    continue;
                }
            } else if l.iter().any(|v| *v < 0.0) {
                let d = seg_dist(t[0], t[1], q).min(seg_dist(t[1], t[2], q)).min(seg_dist(t[2], t[0], q));
                if d > bleed {
                    continue;
                }
                for v in l.iter_mut() {
                    *v = v.max(0.0);
                }
                let s: f64 = l.iter().sum();
                if s <= 0.0 {
                    continue;
                }
                for v in l.iter_mut() {
                    *v /= s;
                }
            }
            f(x as u32, y as u32, l);
        }
    }
}

/// The texel box `raster_with` walks: the triangle's pixel box within `lo..=hi`, grown by the bleed, clipped to the
/// picture (None for a triangle that isn't numbers).
fn raster_span(t: &[[f64; 2]; 3], w: u32, h: u32, lo: [f64; 2], hi: [f64; 2], bleed: f64) -> Option<(i64, i64, i64, i64)> {
    let grow = bleed.max(0.0);
    let minx = t.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let maxx = t.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let miny = t.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let maxy = t.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    if ![minx, maxx, miny, maxy].iter().all(|v| v.is_finite()) {
        return None;
    }
    let x0 = (minx.max(lo[0]) - grow).floor().max(0.0) as i64;
    let x1 = (maxx.min(hi[0]) + grow).ceil().min(w as f64 - 1.0) as i64;
    let y0 = (miny.max(lo[1]) - grow).floor().max(0.0) as i64;
    let y1 = (maxy.min(hi[1]) + grow).ceil().min(h as f64 - 1.0) as i64;
    Some((x0, x1, y0, y1))
}

/// How many texels `raster_with` would look at (for a stroke's work budget).
pub fn raster_area(t: &[[f64; 2]; 3], w: u32, h: u32, lo: [f64; 2], hi: [f64; 2], bleed: f64) -> u64 {
    raster_span(t, w, h, lo, hi, bleed).map_or(0, |(x0, x1, y0, y1)| ((x1 - x0 + 1).max(0) as u64) * ((y1 - y0 + 1).max(0) as u64))
}

/// The largest stretch (pixels per metre) of the map from a triangle's plane to the picture; a bound for how far a dab
/// can reach on the picture. None for a degenerate triangle.
fn max_stretch(t: &Tri) -> Option<f64> {
    let e1 = t.p[1] - t.p[0];
    let e2 = t.p[2] - t.p[0];
    let n = e1.cross(e2);
    if n.length() < 1e-14 {
        return None;
    }
    let a = e1.normalize();
    let b = n.normalize().cross(a);
    // plane coordinates of the corners: (0,0), (|e1|, 0), (e2.a, e2.b)
    let m = [[e1.length(), 0.0], [e2.dot(a), e2.dot(b)]];
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det.abs() < 1e-18 {
        return None;
    }
    let dp = [[t.px[1][0] - t.px[0][0], t.px[1][1] - t.px[0][1]], [t.px[2][0] - t.px[0][0], t.px[2][1] - t.px[0][1]]];
    // J (2x2, columns: d px / d a, d px / d b) = dp^T * m^-1
    let inv = [[m[1][1] / det, -m[0][1] / det], [-m[1][0] / det, m[0][0] / det]];
    let mut j = [[0.0; 2]; 2];
    for r in 0..2 {
        for c in 0..2 {
            j[r][c] = dp[0][r] * inv[0][c] + dp[1][r] * inv[1][c];
        }
    }
    let (p, q, s) = (j[0][0] * j[0][0] + j[1][0] * j[1][0], j[0][0] * j[0][1] + j[1][0] * j[1][1], j[0][1] * j[0][1] + j[1][1] * j[1][1]);
    let tr = p + s;
    let disc = ((p - s) * (p - s) / 4.0 + q * q).sqrt();
    Some((tr / 2.0 + disc).max(0.0).sqrt())
}

fn dist_to_box(p: DVec3, lo: DVec3, hi: DVec3) -> f64 {
    let d = (lo - p).max(p - hi).max(DVec3::ZERO);
    d.length()
}

/// The dabs of a 3D stroke that reach triangle `t`: each stroke point near it, with the pixel box the brush can reach on
/// the triangle's picture.
fn dabs_3d(t: &Tri, points: &[DVec3], radius: f64) -> Vec<(DVec3, [f64; 2], [f64; 2])> {
    let (lo, hi) = (t.p[0].min(t.p[1]).min(t.p[2]), t.p[0].max(t.p[1]).max(t.p[2]));
    let near: Vec<DVec3> = points.iter().copied().filter(|q| dist_to_box(*q, lo, hi) < radius).collect();
    if near.is_empty() {
        return vec![];
    }
    let Some(stretch) = max_stretch(t) else { return vec![] };
    let n = (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).normalize();
    let mut out = Vec::with_capacity(near.len());
    for q in near {
        // where the dab's centre lands on the picture (extrapolated if it is outside the triangle), and how far the
        // brush can reach in the triangle's plane
        let height = (q - t.p[0]).dot(n).abs();
        if height >= radius {
            continue;
        }
        let rho = (radius * radius - height * height).sqrt();
        let foot = q - n * (q - t.p[0]).dot(n);
        let Some(bc) = bary3(t, foot) else { continue };
        let c = [0, 1].map(|k| bc[0] * t.px[0][k] + bc[1] * t.px[1][k] + bc[2] * t.px[2][k]);
        let reach = rho * stretch + 1.5;
        out.push((q, [c[0] - reach, c[1] - reach], [c[0] + reach, c[1] + reach]));
    }
    out
}

/// Brush weights for a 3D stroke: every texel of every triangle whose 3D position is within `radius` of a stroke point.
/// Distances are measured in 3D, so a brush that straddles a UV seam paints both sides, wherever each lands on the
/// picture.
pub fn weights_3d(tris: &[Tri], w: u32, h: u32, points: &[DVec3], radius: f64, hardness: f64) -> Weights {
    weights_3d_within(tris, w, h, points, radius, hardness, &mut 0).expect("a small stroke")
}

/// `weights_3d`, counting the work into `work` and refused once it passes `MAX_STROKE_WORK` (see `spend`).
pub fn weights_3d_within(tris: &[Tri], w: u32, h: u32, points: &[DVec3], radius: f64, hardness: f64, work: &mut u64) -> Result<Weights> {
    // the work is counted first (the dabs' boxes, without painting), so a stroke over the budget is refused at once
    for t in tris {
        spend(work, points.len() as u64)?;
        for (_, lo, hi) in dabs_3d(t, points, radius) {
            spend(work, raster_area(&t.px, w, h, lo, hi, BLEED))?;
        }
    }
    let mut out = Weights::new(w, h);
    for t in tris {
        for (q, lo, hi) in dabs_3d(t, points, radius) {
            raster(&t.px, w, h, lo, hi, |x, y, l| {
                let p = t.p[0] * l[0] + t.p[1] * l[1] + t.p[2] * l[2];
                let wt = falloff(p.distance(q), radius, hardness);
                if wt > 0.0 {
                    out.raise(x, y, wt);
                }
            });
        }
    }
    Ok(out)
}

/// Barycentric coordinates of a point on the triangle's plane (3D).
fn bary3(t: &Tri, p: DVec3) -> Option<[f64; 3]> {
    let (v0, v1, v2) = (t.p[1] - t.p[0], t.p[2] - t.p[0], p - t.p[0]);
    let (d00, d01, d11, d20, d21) = (v0.dot(v0), v0.dot(v1), v1.dot(v1), v2.dot(v0), v2.dot(v1));
    let den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-18 {
        return None;
    }
    let b1 = (d11 * d20 - d01 * d21) / den;
    let b2 = (d00 * d21 - d01 * d20) / den;
    Some([1.0 - b1 - b2, b1, b2])
}

/// Brush weights for a stroke drawn straight on the picture: `points` are texture coordinates (tiles, v up) and `radius`
/// is in tiles of the picture's width.
pub fn weights_2d(w: u32, h: u32, points: &[[f64; 2]], radius: f64, hardness: f64) -> Weights {
    weights_2d_within(w, h, points, radius, hardness, &mut 0).expect("a small stroke")
}

/// `weights_2d`, refused before any painting when the dabs' boxes add up to more than `MAX_STROKE_WORK` texels.
pub fn weights_2d_within(w: u32, h: u32, points: &[[f64; 2]], radius: f64, hardness: f64, work: &mut u64) -> Result<Weights> {
    let r = radius * w as f64;
    let boxes = |p: &[f64; 2]| {
        let c = uv_to_pixel(*p, w, h);
        let (x0, x1) = (((c[0] - r).floor().max(0.0)) as i64, ((c[0] + r).ceil().min(w as f64 - 1.0)) as i64);
        let (y0, y1) = (((c[1] - r).floor().max(0.0)) as i64, ((c[1] + r).ceil().min(h as f64 - 1.0)) as i64);
        (c, x0, x1, y0, y1)
    };
    if r > 0.0 {
        for p in points {
            let (_, x0, x1, y0, y1) = boxes(p);
            spend(work, ((x1 - x0 + 1).max(0) * (y1 - y0 + 1).max(0)) as u64)?;
        }
    }
    let mut out = Weights::new(w, h);
    if r <= 0.0 {
        return Ok(out);
    }
    for p in points {
        let (c, x0, x1, y0, y1) = boxes(p);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f64 + 0.5 - c[0]).powi(2) + (y as f64 + 0.5 - c[1]).powi(2)).sqrt();
                let wt = falloff(d, r, hardness);
                if wt > 0.0 {
                    out.raise(x as u32, y as u32, wt);
                }
            }
        }
    }
    Ok(out)
}

/// Paint the weights onto the picture. Returns how many texels changed.
pub fn apply(c: &mut Canvas, wt: &Weights, b: &Brush) -> usize {
    let target = if b.erase { b.base } else { b.color };
    let mut n = 0;
    for y in 0..c.h {
        for x in 0..c.w {
            let w = wt.v[(y * c.w + x) as usize] as f64;
            if w <= 0.0 {
                continue;
            }
            let old = c.get(x, y);
            let new = blend_px(old, target, w * b.opacity);
            if new != old {
                c.set(x, y, new);
                n += 1;
            }
        }
    }
    n
}

// ----- the model ----------------------------------------------------------------------------------------------------

/// What a stroke is made of.
pub enum Path {
    /// stroke points in world metres (a brush on the model) and the radius in metres; `None` fills the faces' texels
    World(Option<Vec<DVec3>>, f64),
    /// stroke points as texture coordinates and the radius in tiles of the picture's width
    Uv(Vec<[f64; 2]>, f64),
}

/// How a paint texture starts.
pub enum Base {
    Color([f32; 3]),
    /// bake the faces' present material (picture or colour) through the new UVs
    Material,
}

/// The textured triangles of faces that wear `name`: their points and where they land on a `w` x `h` picture.
fn tris_of(m: &Model, ids: &[Id], name: Option<&str>, frames: &Frames, w: u32, h: u32) -> Vec<Tri> {
    let mut out = Vec::new();
    for id in ids {
        let Some(f) = m.faces.get(id) else { continue };
        if f.hidden || (name.is_some() && f.material.as_deref() != name) {
            continue;
        }
        let Ok((pic, loops, (tw, th), _)) = m.picture_corners(*id, frames, false) else { continue };
        out.extend(face_tris(&loops, &pic, (tw, th), m.faces[id].outer.len() >= 3, w, h));
    }
    out
}

fn key(p: &DVec3) -> [u64; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// Triangulate a face and give each corner its place on the picture.
fn face_tris(loops: &[Vec<DVec3>], pic: &[Vec<[f64; 2]>], tile: (f64, f64), ok: bool, w: u32, h: u32) -> Vec<Tri> {
    if !ok || loops.is_empty() {
        return vec![];
    }
    let n = geom::newell(&loops[0]);
    let mut uv: HashMap<[u64; 3], [f64; 2]> = HashMap::new();
    for (ring, pts) in pic.iter().zip(loops) {
        for (c, p) in ring.iter().zip(pts) {
            uv.insert(key(p), [c[0] / tile.0, c[1] / tile.1]);
        }
    }
    let holes: Vec<Vec<DVec3>> = loops[1..].to_vec();
    geom::triangulate(&loops[0], &holes, n)
        .into_iter()
        .filter_map(|t| {
            let c = [uv.get(&key(&t[0]))?, uv.get(&key(&t[1]))?, uv.get(&key(&t[2]))?];
            Some(Tri { p: t, px: [uv_to_pixel(*c[0], w, h), uv_to_pixel(*c[1], w, h), uv_to_pixel(*c[2], w, h)] })
        })
        .collect()
}

/// What one face looked like before an unwrap: its id, its old texture coordinates by corner point (if it had a picture),
/// its material colour and the picture material.
type OldLook = (Id, Option<HashMap<[u64; 3], [f64; 2]>>, [f32; 3], Option<String>);

/// Are these faces' own texture coordinates fit for painting? Counts faces with none, and says whether the islands overlap
/// or leave the 0..1 square.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct UvStatus {
    pub faces: usize,
    pub without: Vec<Id>,
    pub overlap: bool,
    pub outside: bool,
}

impl UvStatus {
    pub fn ok(&self) -> bool {
        self.without.is_empty() && !self.overlap && !self.outside
    }
}

impl Model {
    /// Name of a paint texture not yet in use.
    pub fn new_paint_name(&self) -> String {
        (1..).map(|i| format!("Paint {i}")).find(|n| self.find_material(n).is_none()).unwrap()
    }

    /// The material's picture as pixels.
    pub fn texture_canvas(&self, name: &str) -> Result<Canvas> {
        let t = self.find_material(name).with_context(|| format!("no material {name}"))?.texture.as_ref().with_context(|| format!("material {name} has no picture"))?;
        Canvas::from_data_url(&t.data)
    }

    /// Check the faces' own UVs for painting: present, inside 0..1, not overlapping (a coverage grid, so it is cheap).
    pub fn uv_status(&self, ids: &[Id]) -> Result<UvStatus> {
        let faces = self.uv_faces(ids)?;
        const G: u32 = 256;
        let mut st = UvStatus { faces: faces.len(), ..Default::default() };
        let mut cover = vec![0u8; (G * G) as usize];
        for id in &faces {
            let f = &self.faces[id];
            let Some(u) = f.uv.as_ref().filter(|u| u.fits(f)) else {
                st.without.push(*id);
                continue;
            };
            let loops: Vec<Vec<DVec3>> = f.loops().map(|l| self.pts(l)).collect();
            let pic: Vec<Vec<[f64; 2]>> = std::iter::once(&u.outer).chain(u.holes.iter()).map(|r| r.iter().map(|c| [c[0] as f64, c[1] as f64]).collect()).collect();
            if pic.iter().flatten().any(|c| c[0] < -1e-4 || c[0] > 1.0001 || c[1] < -1e-4 || c[1] > 1.0001) {
                st.outside = true;
            }
            for t in face_tris(&loops, &pic, (1.0, 1.0), true, G, G) {
                raster_with(&t.px, G, G, [0.0, 0.0], [G as f64, G as f64], -1.0, |x, y, _| {
                    let c = &mut cover[(y * G + x) as usize];
                    *c = c.saturating_add(1);
                });
            }
        }
        st.overlap = cover.iter().any(|c| *c > 1);
        Ok(st)
    }

    /// One brush stroke on the picture of material `name`. `ids`: the faces (or groups) that limit a 3D stroke (none: every
    /// face wearing the material). Returns how many texels changed. Nothing changes if it is refused.
    pub fn texture_paint(&mut self, name: &str, ids: Option<&[Id]>, path: &Path, brush: &Brush) -> Result<usize> {
        if !(0.0..=1.0).contains(&brush.opacity) || !(0.0..=1.0).contains(&brush.hardness) {
            bail!("opacity and hardness are 0..1");
        }
        let mat = self.materials.get(name).with_context(|| format!("no material {name} here (close the open component first)"))?;
        let Some(tex) = &mat.texture else { bail!("material {name} has no picture: make one with texture_paint_new") };
        // the size is read from the picture's header, before it is decoded: painting works on pictures the size of a paint texture
        let (pw, ph) = crate::safety::data_url_dimensions(&tex.data)?;
        if pw > MAX_SIZE || ph > MAX_SIZE {
            bail!("material {name}'s picture is {pw} x {ph} pixels; painting works on pictures up to {MAX_SIZE} on a side: make a paint texture with texture_paint_new (base \"material\" keeps the look)");
        }
        let count = match path {
            Path::Uv(p, _) => p.len(),
            Path::World(p, _) => p.as_ref().map_or(0, Vec::len),
        };
        if count > MAX_STROKE_POINTS {
            bail!("a stroke has at most {MAX_STROKE_POINTS} points ({count} given): send a long stroke as several");
        }
        let mut work = 0u64;
        let mut canvas = self.texture_canvas(name)?;
        let (w, h) = (canvas.w, canvas.h);
        let wt = match path {
            Path::Uv(points, r) => {
                if points.is_empty() || *r <= 0.0 || r.is_nan() || points.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
                    bail!("a stroke needs uv_points and a radius above 0 (in tiles)");
                }
                if *r > MAX_UV_RADIUS {
                    bail!("uv_radius is at most {MAX_UV_RADIUS} (a brush as wide as the picture; fill: true paints all of it)");
                }
                let mut points = points.clone();
                points.dedup();
                weights_2d_within(w, h, &points, *r, brush.hardness, &mut work)?
            }
            Path::World(points, r) => {
                let faces = match ids {
                    Some(i) => {
                        self.check_unlocked(i)?;
                        let mut f = self.faces_of(i)?;
                        f.sort_unstable();
                        f.dedup();
                        f
                    }
                    None => self.faces.iter().filter(|(_, f)| f.material.as_deref() == Some(name)).map(|(k, _)| *k).collect(),
                };
                let frames = self.texture_frames();
                let tris = tris_of(self, &faces, Some(name), &frames, w, h);
                if tris.is_empty() {
                    bail!("none of those faces wears {name}");
                }
                match points {
                    None => {
                        let all = ([0.0, 0.0], [w as f64, h as f64]);
                        for t in &tris {
                            spend(&mut work, raster_area(&t.px, w, h, all.0, all.1, BLEED))?;
                        }
                        let mut wt = Weights::new(w, h);
                        for t in &tris {
                            raster(&t.px, w, h, all.0, all.1, |x, y, _| wt.raise(x, y, 1.0));
                        }
                        wt
                    }
                    Some(p) => {
                        if p.iter().any(|q| !q.is_finite()) || *r <= 0.0 || r.is_nan() {
                            bail!("a stroke needs points and a radius above 0 (metres)");
                        }
                        let mut p = p.clone();
                        p.dedup();
                        weights_3d_within(&tris, w, h, &p, *r, brush.hardness, &mut work)?
                    }
                }
            }
        };
        let n = apply(&mut canvas, &wt, brush);
        if n > 0 {
            let data = canvas.to_data_url()?;
            self.materials.get_mut(name).unwrap().texture.as_mut().unwrap().data = data;
        }
        Ok(n)
    }

    /// Make a paint texture of `size` pixels for faces `ids`, wear it on them and return (name, faces, whether the UVs
    /// were unwrapped first). Faces without usable UVs are refused unless `unwrap`.
    pub fn texture_paint_new(&mut self, ids: &[Id], size: u32, base: &Base, name: Option<&str>, unwrap: bool) -> Result<(String, usize, bool)> {
        if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
            bail!("size is {MIN_SIZE}..{MAX_SIZE} pixels");
        }
        let faces = self.uv_faces(ids)?;
        let name = match name {
            Some(n) if self.find_material(n).is_some() => bail!("there is already a material named {n}"),
            Some(n) if n.trim().is_empty() => bail!("the name is empty"),
            Some(n) => n.to_string(),
            None => self.new_paint_name(),
        };
        let status = self.uv_status(&faces)?;
        // what the faces look like now (before an unwrap moves their coordinates), per corner
        let frames = self.texture_frames();
        // pictures are decoded only to bake the faces' present look, and only those the faces wear
        let worn: std::collections::HashSet<&str> = if matches!(base, Base::Material) { faces.iter().filter_map(|id| self.faces[id].material.as_deref()).collect() } else { Default::default() };
        let images = crate::texmap::Images::load_if(self, |n| worn.contains(n));
        let mut old: Vec<OldLook> = Vec::new();
        if matches!(base, Base::Material) {
            for id in &faces {
                let f = &self.faces[id];
                let mat = f.material.clone();
                let colour = mat.as_deref().and_then(|n| self.find_material(n)).map_or([0.8, 0.8, 0.8], |m| m.color);
                let textured = mat.as_deref().is_some_and(|n| frames.contains_key(n) && images.get(n).is_some());
                let map = if textured {
                    self.picture_corners(*id, &frames, false).ok().map(|(pic, loops, (tw, th), _)| {
                        let mut m = HashMap::new();
                        for (ring, pts) in pic.iter().zip(&loops) {
                            for (c, p) in ring.iter().zip(pts) {
                                m.insert(key(p), [c[0] / tw, c[1] / th]);
                            }
                        }
                        m
                    })
                } else {
                    None
                };
                old.push((*id, map, colour, if textured { mat } else { None }));
            }
        }
        let mut unwrapped = false;
        if !status.ok() {
            if !unwrap {
                bail!("{}", describe(&status));
            }
            self.unwrap_uv(&faces, &crate::unwrap::Opts::default())?;
            unwrapped = true;
        }
        let mut canvas = match base {
            Base::Color(c) => Canvas::filled(size, size, [0, 1, 2].map(|k| (c[k].clamp(0.0, 1.0) * 255.0).round() as u8)),
            Base::Material => Canvas::filled(size, size, [204, 204, 204]),
        };
        let mut average = match base {
            Base::Color(c) => *c,
            Base::Material => [0.8; 3],
        };
        if matches!(base, Base::Material) {
            let (mut sum, mut cnt) = ([0.0f64; 3], 0.0f64);
            let mut covered = vec![false; (size * size) as usize];
            for (id, map, colour, tex) in &old {
                let f = &self.faces[id];
                let loops: Vec<Vec<DVec3>> = f.loops().map(|l| self.pts(l)).collect();
                let u = f.uv.as_ref().filter(|u| u.fits(f)).context("the unwrap left a face without UVs")?;
                let pic: Vec<Vec<[f64; 2]>> = std::iter::once(&u.outer).chain(u.holes.iter()).map(|r| r.iter().map(|c| [c[0] as f64, c[1] as f64]).collect()).collect();
                let n = geom::newell(&loops[0]);
                let holes: Vec<Vec<DVec3>> = loops[1..].to_vec();
                let newuv: HashMap<[u64; 3], [f64; 2]> = pic.iter().zip(&loops).flat_map(|(r, p)| r.iter().zip(p).map(|(c, p)| (key(p), *c))).collect();
                for t in geom::triangulate(&loops[0], &holes, n) {
                    let Some(nu) = [0, 1, 2].map(|k| newuv.get(&key(&t[k])).copied()).into_iter().collect::<Option<Vec<_>>>() else { continue };
                    let ou: Option<Vec<[f64; 2]>> = map.as_ref().and_then(|m| [0, 1, 2].map(|k| m.get(&key(&t[k])).copied()).into_iter().collect());
                    let px = [0, 1, 2].map(|k| uv_to_pixel(nu[k], size, size));
                    raster(&px, size, size, [0.0, 0.0], [size as f64, size as f64], |x, y, l| {
                        let rgb = match (&ou, tex) {
                            (Some(o), Some(tn)) => {
                                let uv = [l[0] * o[0][0] + l[1] * o[1][0] + l[2] * o[2][0], l[0] * o[0][1] + l[1] * o[1][1] + l[2] * o[2][1]];
                                images.get(tn).map_or(*colour, |img| crate::texmap::sample(img, uv))
                            }
                            _ => *colour,
                        };
                        canvas.set(x, y, [(rgb[0] * 255.0).round() as u8, (rgb[1] * 255.0).round() as u8, (rgb[2] * 255.0).round() as u8, 255]);
                        let i = (y * size + x) as usize;
                        if !covered[i] {
                            covered[i] = true;
                            cnt += 1.0;
                            for k in 0..3 {
                                sum[k] += rgb[k] as f64;
                            }
                        }
                    });
                }
            }
            if cnt > 0.0 {
                average = [(sum[0] / cnt) as f32, (sum[1] / cnt) as f32, (sum[2] / cnt) as f32];
                // texels no face covers take the average, so a stroke that bleeds over an edge is not a hard line
                let fill = [0, 1, 2].map(|k| (average[k] * 255.0).round() as u8);
                for (i, c) in covered.iter().enumerate() {
                    if !c {
                        canvas.px[i * 4..i * 4 + 3].copy_from_slice(&fill);
                    }
                }
            }
        }
        let data = canvas.to_data_url()?;
        self.materials.insert(name.clone(), Material { color: average, texture: Some(Texture { data, width: 1.0, height: 1.0, project: None, origin: None }), ..Material::default() });
        self.paint_material(&faces, &name)?;
        Ok((name, faces.len(), unwrapped))
    }
}

fn describe(s: &UvStatus) -> String {
    let mut why = vec![];
    if !s.without.is_empty() {
        why.push(format!("{} of {} faces have no UVs", s.without.len(), s.faces));
    }
    if s.overlap {
        why.push("their UV islands overlap".to_string());
    }
    if s.outside {
        why.push("their UVs are outside the 0..1 square".to_string());
    }
    format!("these faces can't be painted yet: {}. Unwrap them first (uv_unwrap), or pass unwrap: true.", why.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_pixel_mapping_flips_v() {
        assert_eq!(uv_to_pixel([0.0, 1.0], 100, 50), [0.0, 0.0]);
        assert_eq!(uv_to_pixel([1.0, 0.0], 100, 50), [100.0, 50.0]);
        assert_eq!(uv_to_pixel([0.25, 0.5], 100, 50), [25.0, 25.0]);
        let p = uv_to_pixel([0.3, 0.7], 256, 128);
        let back = pixel_to_uv(p, 256, 128);
        assert!((back[0] - 0.3).abs() < 1e-12 && (back[1] - 0.7).abs() < 1e-12);
    }

    #[test]
    fn falloff_is_hard_then_soft() {
        assert_eq!(falloff(0.0, 1.0, 0.5), 1.0);
        assert_eq!(falloff(0.5, 1.0, 0.5), 1.0);
        assert_eq!(falloff(1.0, 1.0, 0.5), 0.0);
        assert!((falloff(0.75, 1.0, 0.5) - 0.5).abs() < 1e-12);
        // hardness 1: a hard disc; hardness 0: smooth from the middle
        assert_eq!(falloff(0.99, 1.0, 1.0), 1.0);
        assert!((falloff(0.5, 1.0, 0.0) - 0.5).abs() < 1e-12);
        assert!(falloff(0.2, 1.0, 0.0) > falloff(0.6, 1.0, 0.0));
    }

    #[test]
    fn blend_moves_toward_the_colour_and_keeps_alpha() {
        assert_eq!(blend_px([0, 0, 0, 200], [1.0, 1.0, 1.0], 1.0), [255, 255, 255, 200]);
        assert_eq!(blend_px([10, 20, 30, 255], [1.0, 0.0, 0.0], 0.0), [10, 20, 30, 255]);
        assert_eq!(blend_px([0, 100, 200, 255], [1.0, 1.0, 1.0], 0.5), [128, 178, 228, 255]);
    }

    #[test]
    fn two_d_stamp_is_a_disc() {
        let wt = weights_2d(64, 64, &[[0.5, 0.5]], 0.25, 1.0);
        let at = |x: usize, y: usize| wt.v[y * 64 + x];
        assert_eq!(at(32, 32), 1.0);
        assert_eq!(at(32 + 15, 32), 1.0);
        assert_eq!(at(32 + 17, 32), 0.0);
        assert_eq!(at(0, 0), 0.0);
        // overlapping dabs take the strongest weight, not the sum
        let two = weights_2d(64, 64, &[[0.5, 0.5], [0.5, 0.5]], 0.25, 0.0);
        let one = weights_2d(64, 64, &[[0.5, 0.5]], 0.25, 0.0);
        assert_eq!(two.v, one.v);
    }

    #[test]
    fn three_d_stamp_paints_both_sides_of_a_seam() {
        // two triangles that touch in 3D at x = 1 but sit far apart on the picture (a seam)
        let a = Tri { p: [DVec3::new(0.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0)], px: [[2.0, 30.0], [30.0, 30.0], [2.0, 2.0]] };
        let b = Tri { p: [DVec3::new(1.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 0.0), DVec3::new(1.0, 1.0, 0.0)], px: [[34.0, 62.0], [62.0, 62.0], [34.0, 34.0]] };
        let wt = weights_3d(&[a, b], 64, 64, &[DVec3::new(1.0, 0.1, 0.0)], 0.3, 1.0);
        let any_in = |x0: usize, x1: usize, y0: usize, y1: usize| (y0..y1).any(|y| (x0..x1).any(|x| wt.v[y * 64 + x] > 0.0));
        assert!(any_in(2, 32, 2, 32), "the island the point is on");
        assert!(any_in(34, 64, 34, 64), "the island across the seam");
        // a point at x = 1.8 only reaches the second island
        let far = weights_3d(&[a, b], 64, 64, &[DVec3::new(1.8, 0.1, 0.0)], 0.3, 1.0);
        assert!(!(2..32).any(|y| (2..32).any(|x| far.v[y * 64 + x] > 0.0)));
    }

    #[test]
    fn dabs_do_not_reach_texels_beyond_the_radius_in_3d() {
        let a = Tri { p: [DVec3::new(0.0, 0.0, 0.0), DVec3::new(4.0, 0.0, 0.0), DVec3::new(0.0, 4.0, 0.0)], px: [[0.0, 64.0], [64.0, 64.0], [0.0, 0.0]] };
        let wt = weights_3d(&[a], 64, 64, &[DVec3::new(0.5, 0.5, 0.0)], 0.5, 1.0);
        // 16 px per metre: the brush covers about 8 px each way around (8, 56)
        assert_eq!(wt.v[56 * 64 + 8], 1.0);
        assert_eq!(wt.v[56 * 64 + 20], 0.0);
        assert_eq!(wt.v[10 * 64 + 40], 0.0);
    }

    #[test]
    fn apply_counts_and_erase_goes_back_to_base() {
        let mut c = Canvas::filled(8, 8, [255, 255, 255]);
        let wt = weights_2d(8, 8, &[[0.5, 0.5]], 0.5, 1.0);
        let b = Brush { color: [1.0, 0.0, 0.0], opacity: 1.0, hardness: 1.0, erase: false, base: [1.0, 1.0, 1.0] };
        let n = apply(&mut c, &wt, &b);
        assert!(n > 0);
        assert_eq!(c.get(4, 4), [255, 0, 0, 255]);
        assert_eq!(c.get(0, 0), [255, 255, 255, 255]);
        apply(&mut c, &wt, &Brush { erase: true, ..b });
        assert_eq!(c, Canvas::filled(8, 8, [255, 255, 255]));
    }

    #[test]
    fn png_round_trip_is_exact_and_stable() {
        let mut c = Canvas::filled(16, 16, [10, 20, 30]);
        c.set(3, 4, [200, 100, 50, 255]);
        let url = c.to_data_url().unwrap();
        assert_eq!(Canvas::from_data_url(&url).unwrap(), c);
        assert_eq!(url, c.to_data_url().unwrap());
    }
}
