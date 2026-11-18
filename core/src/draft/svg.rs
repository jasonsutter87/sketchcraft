//! The SVG writer: one page of the display list as an SVG in paper millimeters
//! (`viewBox` is the page, widths are paper mm). The golden tests compare these.

use super::ir::*;
use std::fmt::Write;

/// Numbers at 0.01 mm, without trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{:.2}", if v.abs() < 0.005 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.to_string() }
}

fn rgb(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

struct W {
    s: String,
    clips: usize,
    /// unique to the page, so ids don't collide when several pages are inlined into one document
    uid: String,
}

impl W {
    fn prims(&mut self, items: &[Prim]) {
        for p in items {
            match p {
                Prim::Path(p) => self.path(p),
                Prim::Text(t) => {
                    let rot = if t.angle.abs() > 1e-9 { format!(" transform=\"rotate({} {} {})\"", n(-t.angle.to_degrees()), n(t.at[0]), n(t.at[1])) } else { String::new() };
                    let _ = writeln!(self.s, "<text x=\"{}\" y=\"{}\" font-size=\"{}\"{} fill=\"{}\"{rot}>{}</text>", n(t.at[0]), n(t.at[1]), n(t.size), if t.bold { " font-weight=\"600\"" } else { "" }, rgb(t.rgb), esc(&t.text));
                }
                Prim::Image(i) => {
                    let mut png = Vec::new();
                    let enc = image::codecs::png::PngEncoder::new(&mut png);
                    use image::ImageEncoder;
                    let _ = enc.write_image(i.rgb.as_raw(), i.rgb.width(), i.rgb.height(), image::ExtendedColorType::Rgb8);
                    use base64::Engine as _;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
                    let _ = writeln!(self.s, "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{b64}\"/>", n(i.rect[0]), n(i.rect[1]), n(i.rect[2]), n(i.rect[3]));
                }
                Prim::Group(g) => {
                    let op = if g.opacity < 1.0 { format!(" opacity=\"{}\"", n(g.opacity.max(0.0))) } else { String::new() };
                    if let Some([x, y, w, h]) = g.clip {
                        self.clips += 1;
                        let id = format!("sc-{}-c{}", self.uid, self.clips);
                        let _ = writeln!(self.s, "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>\n<g clip-path=\"url(#{id})\"{op}>", n(x), n(y), n(w), n(h));
                    } else {
                        let _ = writeln!(self.s, "<g{op}>");
                    }
                    self.prims(&g.children);
                    self.s.push_str("</g>\n");
                }
            }
        }
    }

    fn path(&mut self, p: &PathPrim) {
        let mut d = String::new();
        for s in &p.segs {
            match *s {
                Seg::M(a) => {
                    let _ = write!(d, "M{} {}", n(a[0]), n(a[1]));
                }
                Seg::L(a) => {
                    let _ = write!(d, "L{} {}", n(a[0]), n(a[1]));
                }
                Seg::C(a, b, c) => {
                    let _ = write!(d, "C{} {} {} {} {} {}", n(a[0]), n(a[1]), n(b[0]), n(b[1]), n(c[0]), n(c[1]));
                }
                Seg::Z => d.push('Z'),
            }
        }
        if d.is_empty() {
            return;
        }
        let mut a = format!("d=\"{d}\"");
        match p.fill {
            Some(f) => {
                let _ = write!(a, " fill=\"{}\"{}", rgb(f.rgb), if f.even_odd { " fill-rule=\"evenodd\"" } else { "" });
            }
            None => a.push_str(" fill=\"none\""),
        }
        if let Some(pen) = &p.pen {
            let _ = write!(a, " stroke=\"{}\" stroke-width=\"{}\"", rgb(pen.rgb), n(pen.w));
            if pen.dash.is_empty() {
                a.push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
            } else {
                let _ = write!(a, " stroke-dasharray=\"{}\"", pen.dash.iter().map(|d| n(*d)).collect::<Vec<_>>().join(" "));
            }
        }
        let _ = writeln!(self.s, "<path {a}/>");
    }
}

/// One page as a standalone SVG document.
pub fn page(p: &Page) -> String {
    let hash = format!("{}{}{}", p.title, p.w, p.items.len()).bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let mut w = W { s: String::new(), clips: 0, uid: format!("{:08x}", hash >> 32) };
    let title = if p.title.is_empty() { "Drawing sheet".to_string() } else { esc(&p.title) };
    let _ = writeln!(w.s, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{0}mm\" height=\"{1}mm\" viewBox=\"0 0 {0} {1}\" aria-labelledby=\"sc-{3}-title\" font-family=\"Inter, Helvetica, Arial, sans-serif\">\n<title id=\"sc-{3}-title\">{2}</title>", n(p.w), n(p.h), title, w.uid);
    let _ = writeln!(w.s, "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>", n(p.w), n(p.h), rgb(p.bg.unwrap_or(WHITE)));
    w.prims(&p.items);
    w.s.push_str("</svg>\n");
    w.s
}
