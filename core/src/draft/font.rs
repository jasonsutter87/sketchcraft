//! The drawing typeface: Inter (SIL OFL 1.1, see `assets/fonts/OFL-Inter.txt`), regular and
//! semibold, the Latin and Latin Extended subsets. They're embedded as WOFF (small),
//! unpacked once to plain TrueType, measured with `skrifa`, and subset into the PDF
//! with `subsetter`. Text outside those scripts draws as the font's "missing" box.

use std::io::Read;
use std::sync::OnceLock;
use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::{GlyphMetrics, Metrics};
use skrifa::{FontRef, GlyphId as Gid, MetadataProvider};

const WOFF: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/inter-latin-400-normal.woff"),
    include_bytes!("../../assets/fonts/inter-latin-ext-400-normal.woff"),
    include_bytes!("../../assets/fonts/inter-latin-600-normal.woff"),
    include_bytes!("../../assets/fonts/inter-latin-ext-600-normal.woff"),
];
const NAMES: [&str; 4] = ["Inter-Regular", "Inter-Regular-Ext", "Inter-SemiBold", "Inter-SemiBold-Ext"];

pub struct Face {
    pub charmap: Charmap<'static>,
    pub metrics: GlyphMetrics<'static>,
    pub font: FontRef<'static>,
    pub upem: f32,
}

pub struct Fonts {
    pub data: [&'static [u8]; 4],
    pub faces: [Face; 4],
}

impl Face {
    /// The font's own measures, in 1/1000 em: ascender, descender, cap height, and the glyph bounding box.
    pub fn measures(&self) -> (f32, f32, f32, [f32; 4]) {
        let m = Metrics::new(&self.font, Size::unscaled(), LocationRef::default());
        let k = 1000.0 / self.upem;
        let b = m.bounds.map_or([0.0; 4], |b| [b.x_min * k, b.y_min * k, b.x_max * k, b.y_max * k]);
        (m.ascent * k, m.descent * k, m.cap_height.unwrap_or(0.7 * self.upem) * k, b)
    }
}

/// WOFF 1 to a plain sfnt (TrueType) file.
fn unwoff(w: &[u8]) -> Option<Vec<u8>> {
    let be32 = |i: usize| w.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let be16 = |i: usize| w.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    if w.get(0..4)? != b"wOFF" {
        return None;
    }
    let flavor = be32(4)?;
    let n = be16(12)?;
    let mut tables = Vec::new();
    for i in 0..n {
        let e = 44 + i * 20;
        let tag = w.get(e..e + 4)?;
        let (off, comp, orig, sum) = (be32(e + 4)?, be32(e + 8)?, be32(e + 12)?, be32(e + 16)?);
        let raw = w.get(off..off + comp)?;
        let data = if comp < orig {
            let mut v = Vec::with_capacity(orig);
            flate2::read::ZlibDecoder::new(raw).read_to_end(&mut v).ok()?;
            v
        } else {
            raw.to_vec()
        };
        tables.push((tag.to_vec(), sum, data));
    }
    let mut out = Vec::new();
    out.extend((flavor as u32).to_be_bytes());
    out.extend((n as u16).to_be_bytes());
    let mut sr = 1usize;
    let mut es = 0u16;
    while sr * 2 <= n {
        sr *= 2;
        es += 1;
    }
    out.extend(((sr * 16) as u16).to_be_bytes());
    out.extend(es.to_be_bytes());
    out.extend(((n * 16 - sr * 16) as u16).to_be_bytes());
    let mut off = 12 + n * 16;
    for (tag, sum, data) in &tables {
        out.extend(tag);
        out.extend((*sum as u32).to_be_bytes());
        out.extend((off as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        off += data.len().div_ceil(4) * 4;
    }
    for (_, _, data) in &tables {
        out.extend(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    Some(out)
}

pub fn fonts() -> &'static Fonts {
    static F: OnceLock<Fonts> = OnceLock::new();
    F.get_or_init(|| {
        let data: Vec<&'static [u8]> = WOFF.iter().map(|w| &*Box::leak(unwoff(w).expect("the bundled font unpacks").into_boxed_slice())).collect();
        let face = |i: usize| {
            let font = FontRef::new(data[i]).expect("the bundled font parses");
            let upem = Metrics::new(&font, Size::unscaled(), LocationRef::default()).units_per_em as f32;
            Face { charmap: font.charmap(), metrics: font.glyph_metrics(Size::unscaled(), LocationRef::default()), upem, font }
        };
        Fonts { data: [data[0], data[1], data[2], data[3]], faces: [face(0), face(1), face(2), face(3)] }
    })
}

pub fn name(i: usize) -> &'static str {
    NAMES[i]
}

/// Which of the four faces draws `c`, and its glyph there. A character neither subset
/// has is the Latin face's glyph 0 (the "missing" box).
pub fn glyph(c: char, bold: bool) -> (usize, u16) {
    let f = fonts();
    let base = if bold { 2 } else { 0 };
    for i in [base, base + 1] {
        if let Some(g) = f.faces[i].charmap.map(c) {
            if g.to_u32() != 0 {
                return (i, g.to_u32() as u16);
            }
        }
    }
    (base, 0)
}

/// The advance of one glyph in 1/1000 em.
pub fn advance_1000(face: usize, g: u16) -> f64 {
    let f = &fonts().faces[face];
    f.metrics.advance_width(Gid::new(g as u32)).unwrap_or(0.0) as f64 * 1000.0 / f.upem as f64
}

/// The width of `s` at `size` (any unit: the result is in it).
pub fn width(s: &str, size: f64, bold: bool) -> f64 {
    s.chars()
        .map(|c| {
            let (f, g) = glyph(c, bold);
            advance_1000(f, g)
        })
        .sum::<f64>()
        * size
        / 1000.0
}

/// `s` split into runs of one face each: (face, glyphs with their characters).
pub fn runs(s: &str, bold: bool) -> Vec<(usize, Vec<(u16, char)>)> {
    let mut out: Vec<(usize, Vec<(u16, char)>)> = Vec::new();
    for c in s.chars() {
        let (f, g) = glyph(c, bold);
        match out.last_mut() {
            Some((lf, v)) if *lf == f => v.push((g, c)),
            _ => out.push((f, vec![(g, c)])),
        }
    }
    out
}
