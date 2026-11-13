//! Headless renderer: a small CPU rasterizer (z-buffer, flat shading, crisp
//! edge lines, 2x supersampling). No GPU or window needed, so it runs the same
//! in tests, over SSH and from an AI agent. The app draws live with WebGL; this
//! is for "show me what you built" PNGs.

use crate::geom;
use crate::model::Model;

use glam::{DMat4, DVec3, DVec4};
use image::{Rgb, RgbImage};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CameraSpec {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    #[serde(default = "default_fov")]
    pub fov: f64,
}

fn default_fov() -> f64 {
    45.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct RenderOpts {
    pub width: u32,
    pub height: u32,
    pub camera: Option<CameraSpec>,
    /// "iso" (default), "front", "back", "left", "right", "top"
    pub view: String,
    pub edges: bool,
    pub grid: bool,
    /// draw dimensions and text labels
    pub annotations: bool,
    /// parallel projection (architectural elevations) instead of perspective
    pub parallel: bool,
    /// parallel only: meters shown top to bottom (a drawing's scale)
    pub ortho_height: Option<f64>,
    /// plain white, no grid (drawings on a sheet)
    pub paper: bool,
    /// white lines on blueprint blue (faces drawn in the background color)
    pub blueprint: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self { width: 1280, height: 800, camera: None, view: "iso".into(), edges: true, grid: true, annotations: true, parallel: false, ortho_height: None, paper: false, blueprint: false }
    }
}

const SS: usize = 2;
const NEAR: f64 = 0.05;
const DEFAULT_FRONT: [f32; 3] = [0.95, 0.94, 0.91];
const BACK: [f32; 3] = [0.64, 0.68, 0.82];
const EDGE: [f32; 3] = [0.13, 0.14, 0.17];
const GRID: [f32; 3] = [0.80, 0.82, 0.86];
/// blueprint paper and ink
pub const BLUEPRINT: [f32; 3] = [0.11, 0.31, 0.55];
const BLUEPRINT_INK: [f32; 3] = [0.93, 0.96, 1.0];

pub fn camera_for(model: &Model, opts: &RenderOpts) -> CameraSpec {
    if let Some(c) = &opts.camera {
        return c.clone();
    }
    if let Some(v) = model.views.iter().find(|v| v.name == opts.view) {
        return CameraSpec { eye: v.eye, target: v.target, fov: v.fov };
    }
    let (lo, hi) = model.world_bounds().unwrap_or((DVec3::splat(-5.0), DVec3::splat(5.0)));
    let center = (lo + hi) * 0.5;
    let radius = ((hi - lo).length() * 0.5).max(1.0);
    let dir = match opts.view.as_str() {
        "front" => DVec3::new(0.0, -1.0, 0.12),
        "back" => DVec3::new(0.0, 1.0, 0.12),
        "left" => DVec3::new(-1.0, 0.0, 0.12),
        "right" => DVec3::new(1.0, 0.0, 0.12),
        "top" => DVec3::new(0.0, -0.001, 1.0),
        _ => DVec3::new(1.0, -1.35, 0.95),
    }
    .normalize();
    let fov: f64 = 40.0;
    let dist = radius / (fov.to_radians() * 0.5).sin() * 0.9;
    let eye = center + dir * dist;
    CameraSpec { eye: eye.to_array(), target: center.to_array(), fov }
}

struct Canvas {
    w: usize,
    h: usize,
    color: Vec<[f32; 3]>,
    depth: Vec<f64>, // 1 - NDC z, larger = nearer
    vp: DMat4,
    /// how far behind a face an edge may be and still show (depth units)
    edge_bias: Option<f64>,
}

impl Canvas {
    fn to_screen(&self, c: DVec4) -> (f64, f64, f64) {
        let inv = 1.0 / c.w;
        let x = (c.x * inv * 0.5 + 0.5) * self.w as f64;
        let y = (1.0 - (c.y * inv * 0.5 + 0.5)) * self.h as f64;
        // depth: 1 - NDC z (bigger = nearer). Affine in screen space for both
        // perspective and parallel cameras, so it interpolates correctly.
        (x, y, 1.0 - c.z * inv)
    }

    fn triangle(&mut self, tri: [DVec3; 3], col: [f32; 3]) {
        // clip against the near plane in clip space, then fan
        let clip: Vec<DVec4> = tri.iter().map(|p| self.vp * p.extend(1.0)).collect();
        let poly = clip_near(&clip);
        if poly.len() < 3 {
            return;
        }
        let s: Vec<_> = poly.iter().map(|c| self.to_screen(*c)).collect();
        for i in 1..s.len() - 1 {
            self.raster(s[0], s[i], s[i + 1], col);
        }
    }

    fn raster(&mut self, a: (f64, f64, f64), b: (f64, f64, f64), c: (f64, f64, f64), col: [f32; 3]) {
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if area.abs() < 1e-12 {
            return;
        }
        let x0 = a.0.min(b.0).min(c.0).floor().max(0.0) as usize;
        let x1 = (a.0.max(b.0).max(c.0).ceil() as isize).min(self.w as isize - 1);
        let y0 = a.1.min(b.1).min(c.1).floor().max(0.0) as usize;
        let y1 = (a.1.max(b.1).max(c.1).ceil() as isize).min(self.h as isize - 1);
        if x1 < 0 || y1 < 0 {
            return;
        }
        for y in y0..=y1 as usize {
            for x in x0..=x1 as usize {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / area;
                let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a.2 + w1 * b.2 + w2 * c.2;
                let i = y * self.w + x;
                if z > self.depth[i] {
                    self.depth[i] = z;
                    self.color[i] = col;
                }
            }
        }
    }

    fn line(&mut self, p: DVec3, q: DVec3, col: [f32; 3], radius: f64, write_depth: bool) {
        let (mut a, mut b) = (self.vp * p.extend(1.0), self.vp * q.extend(1.0));
        if a.w < NEAR && b.w < NEAR {
            return;
        }
        if a.w < NEAR {
            a = a + (b - a) * ((NEAR - a.w) / (b.w - a.w));
        } else if b.w < NEAR {
            b = b + (a - b) * ((NEAR - b.w) / (a.w - b.w));
        }
        let (a, b) = (self.to_screen(a), self.to_screen(b));
        let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).clamp(1, 200_000);
        let r = radius.ceil() as isize;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let (x, y, z) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t);
            for dy in -r..=r {
                for dx in -r..=r {
                    if ((dx * dx + dy * dy) as f64).sqrt() > radius {
                        continue;
                    }
                    let (xi, yi) = (x as isize + dx, y as isize + dy);
                    if xi < 0 || yi < 0 || xi >= self.w as isize || yi >= self.h as isize {
                        continue;
                    }
                    let i = yi as usize * self.w + xi as usize;
                    // a little bias so edges on a face win against that face
                    let shows = match self.edge_bias {
                        Some(b) => z + b >= self.depth[i],
                        None => z * 1.002 >= self.depth[i],
                    };
                    if shows {
                        self.color[i] = col;
                        if write_depth {
                            self.depth[i] = self.depth[i].max(z);
                        }
                    }
                }
            }
        }
    }
}

impl Canvas {
    /// Screen position of a world point (None if behind the camera).
    fn project(&self, p: DVec3) -> Option<(f64, f64)> {
        let c = self.vp * p.extend(1.0);
        (c.w > NEAR).then(|| {
            let (x, y, _) = self.to_screen(c);
            (x, y)
        })
    }

    fn put(&mut self, x: isize, y: isize, col: [f32; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.color[y as usize * self.w + x as usize] = col;
        }
    }

    /// A screen-space line, drawn on top of everything.
    fn line2d(&mut self, a: (f64, f64), b: (f64, f64), col: [f32; 3], radius: f64) {
        let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).clamp(1, 100_000);
        let r = radius.ceil() as isize;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            for dy in -r..=r {
                for dx in -r..=r {
                    if ((dx * dx + dy * dy) as f64).sqrt() <= radius {
                        self.put(x as isize + dx, y as isize + dy, col);
                    }
                }
            }
        }
    }

    /// Text centered on (cx, cy) with a soft label box behind it.
    fn text(&mut self, cx: f64, cy: f64, s: &str, col: [f32; 3], scale: usize) {
        use font8x8::{UnicodeFonts, BASIC_FONTS};
        let w = s.chars().count() * 8 * scale;
        let h = 8 * scale;
        let (x0, y0) = ((cx - w as f64 / 2.0) as isize, (cy - h as f64 / 2.0) as isize);
        let pad = (3 * scale) as isize;
        for y in (y0 - pad)..(y0 + h as isize + pad) {
            for x in (x0 - pad)..(x0 + w as isize + pad) {
                self.put(x, y, [1.0, 1.0, 1.0]);
            }
        }
        for (i, ch) in s.chars().enumerate() {
            let Some(glyph) = BASIC_FONTS.get(ch) else { continue };
            for (row, bits) in glyph.iter().enumerate() {
                for colm in 0..8 {
                    if bits >> colm & 1 == 1 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let x = x0 + (i * 8 * scale + colm * scale + sx) as isize;
                                let y = y0 + (row * scale + sy) as isize;
                                self.put(x, y, col);
                            }
                        }
                    }
                }
            }
        }
    }
}

const ANNO: [f32; 3] = [0.2, 0.22, 0.3];

pub fn format_length(m: f64, units: &str) -> String {
    crate::units::format(m, units)
}

fn draw_annotations(cv: &mut Canvas, model: &Model) {
    use crate::model::Annotation;
    let s = SS as f64;
    for a in &model.annotations {
        match a {
            Annotation::Dimension { a, b, offset, .. } => {
                let (a2, b2) = (*a + *offset, *b + *offset);
                let (Some(pa), Some(pb), Some(qa), Some(qb)) = (cv.project(*a), cv.project(*b), cv.project(a2), cv.project(b2)) else { continue };
                // extension lines stop a little short of the object
                let short = |p: (f64, f64), q: (f64, f64)| {
                    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                    let l = (dx * dx + dy * dy).sqrt().max(1e-9);
                    ((p.0 + dx / l * 4.0 * s, p.1 + dy / l * 4.0 * s), (q.0 + dx / l * 4.0 * s, q.1 + dy / l * 4.0 * s))
                };
                if offset.length() > 1e-9 {
                    let (s0, e0) = short(pa, qa);
                    let (s1, e1) = short(pb, qb);
                    cv.line2d(s0, e0, ANNO, 0.5 * s);
                    cv.line2d(s1, e1, ANNO, 0.5 * s);
                }
                cv.line2d(qa, qb, ANNO, 0.6 * s);
                // architectural ticks
                for q in [qa, qb] {
                    let t = 5.0 * s;
                    cv.line2d((q.0 - t, q.1 + t), (q.0 + t, q.1 - t), ANNO, 0.9 * s);
                }
                let mid = ((qa.0 + qb.0) / 2.0, (qa.1 + qb.1) / 2.0);
                cv.text(mid.0, mid.1, &format_length(a.distance(*b), &model.units), ANNO, 2 * SS / 2 + 1);
            }
            Annotation::Text { at, text, leader, .. } => {
                let Some(p) = cv.project(*at) else { continue };
                if let Some(l) = leader.and_then(|l| cv.project(l)) {
                    cv.line2d(l, p, ANNO, 0.6 * s);
                    for dy in -2..=2 {
                        for dx in -2..=2 {
                            cv.put(l.0 as isize + dx * SS as isize / 2, l.1 as isize + dy * SS as isize / 2, ANNO);
                        }
                    }
                }
                cv.text(p.0, p.1, text, ANNO, 2 * SS / 2 + 1);
            }
        }
    }
}

fn clip_near(poly: &[DVec4]) -> Vec<DVec4> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ia, ib) = (a.w >= NEAR, b.w >= NEAR);
        if ia {
            out.push(a);
        }
        if ia != ib {
            out.push(a + (b - a) * ((NEAR - a.w) / (b.w - a.w)));
        }
    }
    out
}

fn shade(base: [f32; 3], n: DVec3) -> [f32; 3] {
    let l = DVec3::new(0.35, -0.55, 0.76).normalize();
    let k = (0.62 + 0.38 * n.dot(l).max(0.0)) as f32;
    let sky = (0.04 * n.z.max(0.0)) as f32;
    [base[0] * k + sky, base[1] * k + sky, base[2] * k + sky]
}

pub fn render(model: &Model, opts: &RenderOpts) -> RgbImage {
    let (w, h) = (opts.width.max(16) as usize * SS, opts.height.max(16) as usize * SS);
    let cam = camera_for(model, opts);
    let eye = DVec3::from(cam.eye);
    let target = DVec3::from(cam.target);
    let up = if (target - eye).normalize().cross(DVec3::Z).length() < 1e-3 { DVec3::Y } else { DVec3::Z };
    let view = DMat4::look_at_rh(eye, target, up);
    let aspect = w as f64 / h as f64;
    let proj = if opts.parallel {
        let half = opts.ortho_height.map(|h| h / 2.0).unwrap_or_else(|| eye.distance(target) * (cam.fov.to_radians() * 0.5).tan());
        // depth range fitted to the model, so depth tests stay precise
        let reach = model.world_bounds().map(|(lo, hi)| (lo.distance(eye)).max(hi.distance(eye)) + lo.distance(hi)).unwrap_or(1000.0) + 1.0;
        DMat4::orthographic_rh_gl(-half * aspect, half * aspect, -half, half, -reach, reach)
    } else {
        DMat4::perspective_rh_gl(cam.fov.to_radians(), aspect, NEAR, 100_000.0)
    };

    let mut color = Vec::with_capacity(w * h);
    for y in 0..h {
        let t = y as f32 / h as f32;
        let (top, bot) = if opts.blueprint {
            (BLUEPRINT, BLUEPRINT)
        } else if opts.paper {
            ([1.0, 1.0, 1.0], [1.0, 1.0, 1.0])
        } else {
            ([0.86, 0.90, 0.96], [0.97, 0.97, 0.96])
        };
        color.extend(std::iter::repeat([
            top[0] + (bot[0] - top[0]) * t,
            top[1] + (bot[1] - top[1]) * t,
            top[2] + (bot[2] - top[2]) * t,
        ]).take(w));
    }
    // parallel: depth is linear over 2 x reach; let edges sit up to ~2 cm behind
    let edge_bias = if opts.parallel {
        let reach = model.world_bounds().map(|(lo, hi)| (lo.distance(eye)).max(hi.distance(eye)) + lo.distance(hi)).unwrap_or(1000.0) + 1.0;
        Some(0.02 / reach)
    } else {
        None
    };
    let mut cv = Canvas { w, h, color, depth: vec![0.0; w * h], vp: proj * view, edge_bias };

    if opts.grid && !opts.paper && !opts.blueprint {
        let (lo, hi) = model.world_bounds().unwrap_or((DVec3::splat(-5.0), DVec3::splat(5.0)));
        let pad = ((hi - lo).length() * 0.35).max(4.0);
        let gz = lo.z.min(0.0);
        let step = if (hi - lo).length() > 200.0 { 10.0 } else { 1.0 };
        let (x0, x1) = (((lo.x - pad) / step).floor() * step, ((hi.x + pad) / step).ceil() * step);
        let (y0, y1) = (((lo.y - pad) / step).floor() * step, ((hi.y + pad) / step).ceil() * step);
        let mut x = x0;
        while x <= x1 {
            cv.line(DVec3::new(x, y0, gz), DVec3::new(x, y1, gz), GRID, 0.6, false);
            x += step;
        }
        let mut y = y0;
        while y <= y1 {
            cv.line(DVec3::new(x0, y, gz), DVec3::new(x1, y, gz), GRID, 0.6, false);
            y += step;
        }
    }

    for f in model.world_faces() {
        let n = f.normal;
        let front = (eye - geom::centroid(&f.outer)).dot(n) >= 0.0;
        let base = f.color.unwrap_or(DEFAULT_FRONT);
        // painted faces show their paint on both sides; bare backs stay blue
        // so you can still see which way unpainted faces point
        let col = match (front, f.color) {
            // blueprint: faces only hide what's behind them
            _ if opts.blueprint => BLUEPRINT,
            (true, _) => shade(base, n),
            (false, Some(c)) => shade([c[0] * 0.85, c[1] * 0.85, c[2] * 0.85], -n),
            (false, None) => shade(BACK, -n),
        };
        for tri in geom::triangulate(&f.outer, &f.holes, n) {
            cv.triangle(tri, col);
        }
    }

    if opts.edges {
        for e in model.world_edges().into_iter().filter(|e| !e.soft) {
            cv.line(e.a, e.b, if opts.blueprint { BLUEPRINT_INK } else { EDGE }, 1.1, false);
        }
    }

    if opts.annotations {
        draw_annotations(&mut cv, model);
    }
    // downsample
    let mut img = RgbImage::new(opts.width.max(16), opts.height.max(16));
    for (x, y, px) in img.enumerate_pixels_mut() {
        let mut acc = [0f32; 3];
        for sy in 0..SS {
            for sx in 0..SS {
                let c = cv.color[(y as usize * SS + sy) * w + x as usize * SS + sx];
                for k in 0..3 {
                    acc[k] += c[k];
                }
            }
        }
        let n = (SS * SS) as f32;
        *px = Rgb([
            (acc[0] / n * 255.0).clamp(0.0, 255.0) as u8,
            (acc[1] / n * 255.0).clamp(0.0, 255.0) as u8,
            (acc[2] / n * 255.0).clamp(0.0, 255.0) as u8,
        ]);
    }
    img
}
