//! One page of the display list as light JSON, for the app's paper view (design §5, step 11).
//!
//! Paths become SVG path strings (`"M1 2L3 4Z"`, paper mm, y down), which a browser turns into a `Path2D`
//! directly. Neighbouring stroke-only paths with the same pen are joined into one, so the view strokes a
//! whole viewport's line work in a few calls. Numbers are rounded to 0.01 mm, as the SVG writer does.
//!
//! Shape (`"k"` says what each item is):
//!
//! - `{"k":"p", "d", "s"?: pen, "f"?: {"c", "eo"}}`: a path, stroked with `s` and/or filled
//! - `{"k":"t", "x", "y", "size", "bold", "c", "a", "text"}`: text at its baseline start, `a` radians
//!   counter-clockwise on the paper
//! - `{"k":"i", "rect", "png"}`: a picture (base64 PNG; null when it is past the size limit)
//! - `{"k":"g", "clip"?, "o", "alt"?, "items"}`: a group (a viewport's figure), drawn at opacity `o` as one layer
//! - `{"k":"h", "rings", "lines", "s", "tint"?}`: a hatch: clip to `rings` (even-odd), stroke `lines`, or fill `tint`
//!
//! A pen is `{"w" (mm), "c" ("#rrggbb"), "dash" (mm, on/off...; empty is solid)}`.
//!
//! The answer is bounded: past `limit` bytes of path data, text and pictures, the rest is left out and
//! `truncated` is true (the view says so), so a huge sheet can't make a reply the app chokes on.

use super::ir::*;
use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::fmt::Write;

/// The default size limit of one page's answer (bytes of path data, text and pictures).
pub const LIMIT: usize = 16 << 20;
/// Pictures are left out (null) past this many bytes in total, so line work still fits.
pub const PICTURE_LIMIT: usize = 8 << 20;

fn n(v: f64) -> String {
    let s = format!("{:.2}", if v.abs() < 0.005 || !v.is_finite() { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.to_string() }
}

fn num(v: f64) -> Value {
    let r = (v * 100.0).round() / 100.0;
    if r.is_finite() { json!(r) } else { json!(0) }
}

fn rgb(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn pen(p: &Pen) -> Value {
    json!({"w": num(p.w), "c": rgb(p.rgb), "dash": p.dash.iter().map(|d| num(*d)).collect::<Vec<_>>()})
}

fn segs_d(out: &mut String, segs: &[Seg]) {
    for s in segs {
        let _ = match *s {
            Seg::M(a) => write!(out, "M{} {}", n(a[0]), n(a[1])),
            Seg::L(a) => write!(out, "L{} {}", n(a[0]), n(a[1])),
            Seg::C(a, b, c) => write!(out, "C{} {} {} {} {} {}", n(a[0]), n(a[1]), n(b[0]), n(b[1]), n(c[0]), n(c[1])),
            Seg::Z => {
                out.push('Z');
                Ok(())
            }
        };
    }
}

/// What was written, for the answer's `counts`.
#[derive(Default)]
struct Counts {
    paths: usize,
    texts: usize,
    images: usize,
    groups: usize,
    hatches: usize,
    /// path prims before joining
    joined_from: usize,
}

struct W {
    used: usize,
    pictures: usize,
    limit: usize,
    truncated: bool,
    c: Counts,
}

impl W {
    fn room(&mut self, bytes: usize) -> bool {
        if self.truncated || self.used + bytes > self.limit {
            self.truncated = true;
            return false;
        }
        self.used += bytes;
        true
    }

    fn items(&mut self, prims: &[Prim]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < prims.len() && !self.truncated {
            match &prims[i] {
                Prim::Path(p) => {
                    let mut d = String::new();
                    segs_d(&mut d, &p.segs);
                    self.c.joined_from += 1;
                    // join the stroke-only paths that follow with the same pen (filled ones keep their own shape)
                    let mut j = i + 1;
                    if p.fill.is_none() && p.pen.is_some() {
                        while let Some(Prim::Path(q)) = prims.get(j) {
                            if q.fill.is_some() || q.pen != p.pen {
                                break;
                            }
                            segs_d(&mut d, &q.segs);
                            self.c.joined_from += 1;
                            j += 1;
                        }
                    }
                    i = j;
                    if d.is_empty() || !self.room(d.len() + 48) {
                        continue;
                    }
                    let mut o = Map::new();
                    o.insert("k".into(), json!("p"));
                    o.insert("d".into(), json!(d));
                    if let Some(s) = &p.pen {
                        o.insert("s".into(), pen(s));
                    }
                    if let Some(f) = p.fill {
                        o.insert("f".into(), json!({"c": rgb(f.rgb), "eo": f.even_odd}));
                    }
                    self.c.paths += 1;
                    out.push(Value::Object(o));
                    continue;
                }
                Prim::Text(t) => {
                    if self.room(t.text.len() + 96) {
                        self.c.texts += 1;
                        out.push(json!({"k": "t", "x": num(t.at[0]), "y": num(t.at[1]), "size": num(t.size), "bold": t.bold, "c": rgb(t.rgb), "a": (t.angle * 1e4).round() / 1e4, "text": t.text}));
                    }
                }
                Prim::Image(img) => {
                    // a picture that can't fit even well compressed isn't encoded at all
                    let png = if self.pictures + img.rgb.as_raw().len() / 64 <= PICTURE_LIMIT {
                        let mut png = Vec::new();
                        use image::ImageEncoder;
                        let enc = image::codecs::png::PngEncoder::new(&mut png);
                        enc.write_image(img.rgb.as_raw(), img.rgb.width(), img.rgb.height(), image::ExtendedColorType::Rgb8).ok().map(|_| base64::engine::general_purpose::STANDARD.encode(&png))
                    } else {
                        None
                    };
                    let png = png.filter(|p| self.pictures + p.len() <= PICTURE_LIMIT);
                    if self.room(png.as_ref().map_or(0, |p| p.len()) + 64) {
                        self.pictures += png.as_ref().map_or(0, |p| p.len());
                        self.c.images += 1;
                        out.push(json!({"k": "i", "rect": img.rect.iter().map(|v| num(*v)).collect::<Vec<_>>(), "png": png}));
                    }
                }
                Prim::Group(g) => {
                    if self.room(64) {
                        self.c.groups += 1;
                        let items = self.items(&g.children);
                        let mut o = Map::new();
                        o.insert("k".into(), json!("g"));
                        if let Some(c) = g.clip {
                            o.insert("clip".into(), json!(c.iter().map(|v| num(*v)).collect::<Vec<_>>()));
                        }
                        o.insert("o".into(), num(g.opacity.clamp(0.0, 1.0)));
                        if let Some(a) = &g.alt {
                            o.insert("alt".into(), json!(a));
                        }
                        o.insert("items".into(), Value::Array(items));
                        out.push(Value::Object(o));
                    }
                }
                Prim::Hatch(h) => {
                    let mut rings = String::new();
                    segs_d(&mut rings, &h.ring_segs());
                    let mut lines = String::new();
                    if h.tint.is_none() {
                        for [a, b] in &h.lines {
                            let _ = write!(lines, "M{} {}L{} {}", n(a[0]), n(a[1]), n(b[0]), n(b[1]));
                        }
                    }
                    if !rings.is_empty() && self.room(rings.len() + lines.len() + 96) {
                        self.c.hatches += 1;
                        out.push(json!({"k": "h", "rings": rings, "lines": lines, "s": pen(&h.pen), "tint": h.tint.map(rgb), "name": h.name}));
                    }
                }
            }
            i += 1;
        }
        out
    }
}

/// The page as JSON (see the module's doc), within `limit` bytes (`LIMIT` when None).
pub fn page(p: &Page, limit: Option<usize>) -> Value {
    let mut w = W { used: 0, pictures: 0, limit: limit.unwrap_or(LIMIT), truncated: false, c: Counts::default() };
    let items = w.items(&p.items);
    json!({
        "w": num(p.w),
        "h": num(p.h),
        "bg": p.bg.map(rgb),
        "title": p.title,
        "items": items,
        "truncated": w.truncated,
        "bytes": w.used,
        "counts": {"paths": w.c.paths, "path_prims": w.c.joined_from, "texts": w.c.texts, "images": w.c.images, "groups": w.c.groups, "hatches": w.c.hatches},
    })
}
