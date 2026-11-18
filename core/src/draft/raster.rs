//! A page of the display list as a picture (PNG), so an agent can look at a sheet it laid out and check its work.
//!
//! A small scanline rasterizer: paths are flattened to polygons and filled with four sub-scanlines per pixel row and
//! exact horizontal coverage (anti-aliased), strokes become one quad per segment plus small joins, text is the
//! drawing typeface's own outlines (skrifa), pictures are sampled nearest. Opacity fades each primitive (not the
//! viewport as one group, as the PDF does: overlapping lines inside a faded viewport look a little darker here).
//! It is for looking at, not for printing: the PDF, SVG and DXF are the drawings.

use super::ir::*;
use image::RgbImage;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};

/// The widest picture `page` makes, in pixels.
pub const MAX_PX: u32 = 4000;
/// The narrowest.
pub const MIN_PX: u32 = 200;
/// Points one primitive may flatten to (a runaway curve or a huge polyline is cut off here, not drawn forever).
const MAX_POINTS: usize = 400_000;

type Poly = Vec<[f64; 2]>;

struct Canvas {
    img: RgbImage,
    /// pixels per paper mm
    k: f64,
    cov: Vec<f32>,
}

/// Draw one page `px` pixels wide (clamped to `MIN_PX..=MAX_PX`; the height keeps the paper's proportions).
pub fn page(p: &Page, px: u32) -> RgbImage {
    let px = px.clamp(MIN_PX, MAX_PX);
    let k = px as f64 / p.w.max(1.0);
    let h = ((p.h * k).round() as u32).clamp(1, MAX_PX * 4);
    let bg = p.bg.unwrap_or(WHITE);
    let img = RgbImage::from_pixel(px, h, image::Rgb(bg));
    let mut c = Canvas { img, k, cov: vec![0.0; px as usize + 2] };
    let all = [0.0, 0.0, px as f64, h as f64];
    for it in &p.items {
        c.prim(it, all, 1.0);
    }
    c.img
}

impl Canvas {
    fn prim(&mut self, p: &Prim, clip: [f64; 4], alpha: f64) {
        match p {
            Prim::Group(g) => {
                let clip = match g.clip {
                    Some([x, y, w, h]) => intersect(clip, [x * self.k, y * self.k, (x + w) * self.k, (y + h) * self.k]),
                    None => clip,
                };
                if clip[2] <= clip[0] || clip[3] <= clip[1] {
                    return;
                }
                let a = alpha * g.opacity.clamp(0.0, 1.0);
                for c in g.children.iter() {
                    self.prim(c, clip, a);
                }
            }
            Prim::Path(pp) => {
                if let Some(f) = pp.fill {
                    let polys = self.flatten(&pp.segs, true);
                    self.fill(&polys, f.even_odd, f.rgb, alpha, clip);
                }
                if let Some(pen) = &pp.pen {
                    let lines = self.flatten(&pp.segs, false);
                    self.stroke(&lines, pen, alpha, clip);
                }
            }
            Prim::Hatch(h) => {
                if let Some(t) = h.tint {
                    let polys: Vec<Poly> = h.rings.iter().map(|r| r.iter().map(|q| [q[0] * self.k, q[1] * self.k]).collect()).collect();
                    self.fill(&polys, true, t, alpha, clip);
                } else {
                    let lines: Vec<Poly> = h.lines.iter().map(|[a, b]| vec![[a[0] * self.k, a[1] * self.k], [b[0] * self.k, b[1] * self.k]]).collect();
                    self.stroke(&lines, &h.pen, alpha, clip);
                }
            }
            Prim::Text(t) => self.text(t, alpha, clip),
            Prim::Image(im) => self.image(im, alpha, clip),
        }
    }

    /// Path segments as pixel polylines (one per subpath); `closed` closes every one (for filling).
    fn flatten(&self, segs: &[Seg], closed: bool) -> Vec<Poly> {
        let k = self.k;
        let mut out: Vec<Poly> = Vec::new();
        let mut cur: Poly = Vec::new();
        let mut total = 0usize;
        let mut start = [0.0, 0.0];
        for s in segs {
            if total > MAX_POINTS {
                break;
            }
            match *s {
                Seg::M(p) => {
                    if cur.len() > 1 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    start = [p[0] * k, p[1] * k];
                    cur.push(start);
                }
                Seg::L(p) => {
                    if cur.is_empty() {
                        cur.push(start);
                    }
                    cur.push([p[0] * k, p[1] * k]);
                }
                Seg::C(a, b, e) => {
                    let p0 = *cur.last().unwrap_or(&start);
                    let (a, b, e) = ([a[0] * k, a[1] * k], [b[0] * k, b[1] * k], [e[0] * k, e[1] * k]);
                    let len = dist(p0, a) + dist(a, b) + dist(b, e);
                    let n = (len / 2.0).ceil().clamp(1.0, 64.0) as usize;
                    if cur.is_empty() {
                        cur.push(p0);
                    }
                    for i in 1..=n {
                        let t = i as f64 / n as f64;
                        let u = 1.0 - t;
                        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                        cur.push([w[0] * p0[0] + w[1] * a[0] + w[2] * b[0] + w[3] * e[0], w[0] * p0[1] + w[1] * a[1] + w[2] * b[1] + w[3] * e[1]]);
                    }
                    total += n;
                }
                Seg::Z => {
                    if !cur.is_empty() {
                        cur.push(cur[0]);
                        out.push(std::mem::take(&mut cur));
                    }
                }
            }
            total += 1;
        }
        if !cur.is_empty() {
            if closed {
                cur.push(cur[0]);
            }
            out.push(cur);
        }
        out.retain(|p| p.iter().all(|q| q[0].is_finite() && q[1].is_finite()));
        out
    }

    /// Stroke polylines with a pen (width in paper mm, at least 0.7 px so hairlines show), dashed when it is.
    fn stroke(&mut self, lines: &[Poly], pen: &Pen, alpha: f64, clip: [f64; 4]) {
        let w = (pen.w * self.k).max(0.7);
        let dash: Vec<f64> = pen.dash.iter().map(|d| d * self.k).filter(|d| *d > 0.0).collect();
        let mut quads: Vec<Poly> = Vec::new();
        for line in lines {
            let pieces = if dash.len() >= 2 && dash.iter().sum::<f64>() > 0.5 { dashed(line, &dash) } else { vec![line.clone()] };
            for piece in pieces {
                outline(&piece, w, &mut quads);
            }
            if quads.len() > MAX_POINTS / 4 {
                break;
            }
        }
        self.fill(&quads, false, pen.rgb, alpha, clip);
    }

    fn text(&mut self, t: &TextPrim, alpha: f64, clip: [f64; 4]) {
        let fonts = super::font::fonts();
        let (ca, sa) = (t.angle.cos(), t.angle.sin());
        let mut pen_x = 0.0f64;
        let mut polys: Vec<Poly> = Vec::new();
        for (face, glyphs) in super::font::runs(&t.text, t.bold) {
            let f = &fonts.faces[face];
            let scale = t.size / f.upem as f64;
            let outlines = f.font.outline_glyphs();
            for (g, _) in glyphs {
                if let Some(o) = outlines.get(GlyphId::new(g as u32)) {
                    let mut p = Flat { polys: Vec::new(), cur: Vec::new(), last: [0.0, 0.0] };
                    if o.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut p).is_ok() {
                        p.end();
                        for poly in p.polys {
                            polys.push(
                                poly.into_iter()
                                    .map(|[gx, gy]| {
                                        // glyph units (y up) to the run's frame, turned on the paper (y down), to pixels
                                        let (lx, ly) = (pen_x + gx * scale, -gy * scale);
                                        [(t.at[0] + lx * ca + ly * sa) * self.k, (t.at[1] - lx * sa + ly * ca) * self.k]
                                    })
                                    .collect(),
                            );
                        }
                    }
                }
                pen_x += super::font::advance_1000(face, g) * t.size / 1000.0;
            }
        }
        self.fill(&polys, false, t.rgb, alpha, clip);
    }

    fn image(&mut self, im: &ImagePrim, alpha: f64, clip: [f64; 4]) {
        let [x, y, w, h] = im.rect;
        let r = intersect(clip, [x * self.k, y * self.k, (x + w) * self.k, (y + h) * self.k]);
        let (iw, ih) = (im.rgb.width(), im.rgb.height());
        if r[2] <= r[0] || r[3] <= r[1] || iw == 0 || ih == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0) as f32;
        for py in r[1].floor().max(0.0) as u32..(r[3].ceil() as u32).min(self.img.height()) {
            for px in r[0].floor().max(0.0) as u32..(r[2].ceil() as u32).min(self.img.width()) {
                let u = ((px as f64 + 0.5) / self.k - x) / w;
                let v = ((py as f64 + 0.5) / self.k - y) / h;
                if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                    continue;
                }
                let s = im.rgb.get_pixel(((u * iw as f64) as u32).min(iw - 1), ((v * ih as f64) as u32).min(ih - 1)).0;
                blend(self.img.get_pixel_mut(px, py), s, a);
            }
        }
    }

    /// Fill polygons (pixel coordinates; each closed) with one colour: four sub-scanlines a row, exact coverage across.
    fn fill(&mut self, polys: &[Poly], even_odd: bool, rgb: Rgb, alpha: f64, clip: [f64; 4]) {
        if alpha <= 0.0 {
            return;
        }
        // the edges, each going down (y0 < y1) with its winding direction
        let mut edges: Vec<(f64, f64, f64, f64, i32)> = Vec::new();
        let (mut ylo, mut yhi) = (f64::MAX, f64::MIN);
        for p in polys {
            for w in p.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a[1] - b[1]).abs() < 1e-12 {
                    continue;
                }
                let e = if a[1] < b[1] { (a[0], a[1], b[0], b[1], 1) } else { (b[0], b[1], a[0], a[1], -1) };
                ylo = ylo.min(e.1);
                yhi = yhi.max(e.3);
                edges.push(e);
            }
            // a ring not closed by its points closes itself
            if let (Some(a), Some(b)) = (p.last(), p.first()) {
                if dist(*a, *b) > 1e-9 && (a[1] - b[1]).abs() > 1e-12 {
                    let e = if a[1] < b[1] { (a[0], a[1], b[0], b[1], 1) } else { (b[0], b[1], a[0], a[1], -1) };
                    edges.push(e);
                }
            }
        }
        if edges.is_empty() {
            return;
        }
        edges.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (w, h) = (self.img.width() as f64, self.img.height() as f64);
        let (cx0, cx1) = (clip[0].max(0.0), clip[2].min(w));
        let row0 = ylo.max(clip[1]).max(0.0).floor() as i64;
        let row1 = yhi.min(clip[3]).min(h).ceil() as i64;
        if row1 <= row0 || cx1 <= cx0 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0) as f32;
        let mut xs: Vec<(f64, i32)> = Vec::new();
        for row in row0..row1 {
            let (mut lo, mut hi) = (usize::MAX, 0usize);
            for sub in 0..4 {
                let y = row as f64 + (sub as f64 + 0.5) / 4.0;
                if y < clip[1] || y > clip[3] {
                    continue;
                }
                xs.clear();
                // edges are sorted by their top: the rest start below this line
                for e in &edges {
                    if e.1 > y {
                        break;
                    }
                    if e.3 > y {
                        let t = (y - e.1) / (e.3 - e.1);
                        xs.push((e.0 + (e.2 - e.0) * t, e.4));
                    }
                }
                xs.sort_by(|p, q| p.0.total_cmp(&q.0));
                let mut wind = 0;
                for i in 0..xs.len() {
                    wind += if even_odd { 1 } else { xs[i].1 };
                    let inside = if even_odd { wind & 1 != 0 } else { wind != 0 };
                    if inside && i + 1 < xs.len() {
                        let (x0, x1) = (xs[i].0.max(cx0), xs[i + 1].0.min(cx1));
                        if x1 > x0 {
                            span(&mut self.cov, x0, x1, 0.25);
                            lo = lo.min(x0.floor() as usize);
                            hi = hi.max(x1.ceil() as usize);
                        }
                    }
                }
            }
            if lo <= hi && lo != usize::MAX {
                let hi = hi.min(self.img.width() as usize);
                for x in lo..hi {
                    let c = self.cov[x].min(1.0);
                    self.cov[x] = 0.0;
                    if c > 0.0 {
                        blend(self.img.get_pixel_mut(x as u32, row as u32), rgb, c * a);
                    }
                }
                self.cov[hi] = 0.0;
            }
        }
    }
}

/// Add `amount` of coverage for x0..x1 (pixels) to a row, with partial pixels at the ends.
fn span(cov: &mut [f32], x0: f64, x1: f64, amount: f32) {
    let (i0, i1) = (x0.floor() as usize, x1.floor() as usize);
    if i0 >= cov.len() {
        return;
    }
    if i0 == i1 {
        cov[i0] += amount * (x1 - x0) as f32;
        return;
    }
    cov[i0] += amount * (i0 as f64 + 1.0 - x0) as f32;
    let end = i1.min(cov.len());
    for c in cov.iter_mut().take(end).skip(i0 + 1) {
        *c += amount;
    }
    if i1 < cov.len() {
        cov[i1] += amount * (x1 - i1 as f64) as f32;
    }
}

fn blend(px: &mut image::Rgb<u8>, rgb: Rgb, a: f32) {
    for (d, s) in px.0.iter_mut().zip(rgb) {
        let f = *d as f32;
        *d = (f + (s as f32 - f) * a.clamp(0.0, 1.0)).round() as u8;
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn intersect(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]
}

/// The "on" pieces of a polyline under a dash pattern (pixel lengths: on, off, on, off...).
fn dashed(line: &[[f64; 2]], dash: &[f64]) -> Vec<Poly> {
    let mut out = Vec::new();
    let (mut di, mut left, mut on) = (0usize, dash[0], true);
    let mut cur: Poly = vec![line[0]];
    for w in line.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = dist(a, b);
        while seg > 1e-9 {
            let step = left.min(seg);
            let t = step / seg;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if on {
                cur.push(p);
            }
            seg -= step;
            left -= step;
            a = p;
            if left <= 1e-9 {
                if on && cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                on = !on;
                di = (di + 1) % dash.len();
                left = dash[di];
                if on {
                    cur.push(a);
                }
            }
            if out.len() > 100_000 {
                return out;
            }
        }
    }
    if on && cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// A polyline's stroke as polygons, all wound the same way (so filling them together, non-zero, makes their union):
/// a quad per segment, a small octagon at each joint (and for a dot, a line with no length).
fn outline(line: &[[f64; 2]], w: f64, out: &mut Vec<Poly>) {
    let r = w / 2.0;
    let dot = |c: [f64; 2], out: &mut Vec<Poly>| {
        let mut p: Poly = (0..8).map(|i| std::f64::consts::FRAC_PI_4 * i as f64).map(|a| [c[0] + r * a.cos(), c[1] + r * a.sin()]).collect();
        p.push(p[0]);
        out.push(p);
    };
    let mut any = false;
    for (i, s) in line.windows(2).enumerate() {
        let (a, b) = (s[0], s[1]);
        let l = dist(a, b);
        if l < 1e-9 {
            continue;
        }
        any = true;
        let (nx, ny) = (-(b[1] - a[1]) / l * r, (b[0] - a[0]) / l * r);
        let mut q = vec![[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]];
        if area(&q) < 0.0 {
            q.reverse();
        }
        q.push(q[0]);
        out.push(q);
        if i > 0 && w > 1.5 {
            dot(a, out);
        }
    }
    if !any {
        if let Some(c) = line.first() {
            dot(*c, out);
        }
    }
}

fn area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() / 2.0
}

/// A glyph outline flattened to polygons, in font units (y up).
struct Flat {
    polys: Vec<Poly>,
    cur: Poly,
    last: [f64; 2],
}

impl Flat {
    fn end(&mut self) {
        if self.cur.len() > 2 {
            let first = self.cur[0];
            self.cur.push(first);
            self.polys.push(std::mem::take(&mut self.cur));
        }
        self.cur.clear();
    }
}

impl OutlinePen for Flat {
    fn move_to(&mut self, x: f32, y: f32) {
        self.end();
        self.last = [x as f64, y as f64];
        self.cur.push(self.last);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.last = [x as f64, y as f64];
        self.cur.push(self.last);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, c, e) = (self.last, [cx0 as f64, cy0 as f64], [x as f64, y as f64]);
        for i in 1..=8 {
            let t = i as f64 / 8.0;
            let u = 1.0 - t;
            self.cur.push([u * u * p0[0] + 2.0 * u * t * c[0] + t * t * e[0], u * u * p0[1] + 2.0 * u * t * c[1] + t * t * e[1]]);
        }
        self.last = e;
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, a, b, e) = (self.last, [cx0 as f64, cy0 as f64], [cx1 as f64, cy1 as f64], [x as f64, y as f64]);
        for i in 1..=10 {
            let t = i as f64 / 10.0;
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            self.cur.push([w[0] * p0[0] + w[1] * a[0] + w[2] * b[0] + w[3] * e[0], w[0] * p0[1] + w[1] * a[1] + w[2] * b[1] + w[3] * e[1]]);
        }
        self.last = e;
    }
    fn close(&mut self) {
        self.end();
    }
}
