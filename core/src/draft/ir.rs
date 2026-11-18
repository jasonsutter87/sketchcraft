//! The display list: everything on a drawing sheet, in paper space.
//!
//! Units are millimeters on the paper, origin at the page's top left, y down
//! (the way `ViewportSpec` places things). One list feeds the PDF, SVG and DXF
//! writers, so the three always agree. Model-derived line work keeps its
//! provenance (the instance path and the two vertex ids of the edge it came
//! from, and which part of the edge it covers), so an editor can snap to it and
//! make an anchor on it later.

use crate::model::Id;
use std::sync::Arc;

pub type Rgb = [u8; 3];

pub const BLACK: Rgb = [26, 26, 31];
pub const WHITE: Rgb = [255, 255, 255];
/// blueprint paper and ink
pub const BP_PAPER: Rgb = [28, 79, 140];
pub const BP_INK: Rgb = [237, 245, 255];
pub const BP_CUT: Rgb = [158, 189, 224];

pub const PT: f64 = 72.0 / 25.4;

/// A pen: width in paper millimeters (it never scales with a drawing), color, and
/// a dash pattern in paper millimeters (on, off, on, off...; empty is solid).
#[derive(Clone, Debug, PartialEq)]
pub struct Pen {
    pub w: f64,
    pub rgb: Rgb,
    pub dash: Vec<f64>,
}

impl Pen {
    pub fn new(w: f64, rgb: Rgb) -> Self {
        Pen { w, rgb, dash: Vec::new() }
    }
    pub fn dashed(mut self, dash: &[f64]) -> Self {
        self.dash = dash.to_vec();
        self
    }
}

/// The default pen table (ISO 128 widths). Hard-coded weights used to be scattered
/// through the drawing code; they live here now, one place for tag styles (#3) to
/// override per tag later.
#[derive(Clone, Debug)]
pub struct Pens {
    pub ink: Rgb,
    pub paper: Rgb,
    /// the fill of cut walls
    pub cut_fill: Rgb,
    /// faint stuff: things below the cut, extension lines, hatching
    pub hair: f64,
    /// dimensions, leaders, hidden ("beyond") lines
    pub thin: f64,
    /// visible edges, callout outlines
    pub medium: f64,
    /// cut outlines, title block rules
    pub heavy: f64,
    /// the sheet border
    pub border: f64,
    pub blueprint: bool,
}

impl Pens {
    pub fn new(blueprint: bool) -> Self {
        if blueprint {
            Pens { ink: BP_INK, paper: BP_PAPER, cut_fill: BP_CUT, hair: 0.13, thin: 0.18, medium: 0.25, heavy: 0.35, border: 0.5, blueprint }
        } else {
            Pens { ink: BLACK, paper: WHITE, cut_fill: [41, 43, 51], hair: 0.13, thin: 0.18, medium: 0.25, heavy: 0.35, border: 0.5, blueprint }
        }
    }
    /// a lighter ink for what lies below the cut
    pub fn faint(&self) -> Rgb {
        if self.blueprint {
            [191, 214, 242]
        } else {
            [115, 117, 128]
        }
    }
    pub fn ink(&self, w: f64) -> Pen {
        Pen::new(w, self.ink)
    }

    /// The pen for a line of model geometry: `base` (the default pen for that kind of line), then the tag's style over
    /// it, then the edge's own style over both. Each of weight, line style and colour falls through on its own.
    /// Weights are paper millimetres and never change with a drawing's scale. `beyond` is for hidden ("beyond")
    /// spans: the tag's `beyond` weight and colour apply to them, and they stay dashed unless a style says otherwise.
    pub fn for_tag(&self, d: &crate::drafting::Drafting, tag: Option<&str>, edge: Option<&crate::drafting::EdgeDraft>, base: Pen, beyond: bool) -> Pen {
        let mut pen = base;
        let ts = tag.and_then(|t| d.tag_style(t));
        if let Some(t) = ts {
            let (w, c) = if beyond { (t.beyond.weight_mm, t.beyond.color.as_str()) } else { (t.weight_mm, t.color.as_str()) };
            if w > 0.0 && w.is_finite() {
                pen.w = w;
            }
            if let Some(rgb) = parse_hex(c) {
                pen.rgb = rgb;
            }
            if !t.line.is_empty() && !beyond {
                if let Some(dash) = d.dash_of(&t.line) {
                    pen.dash = dash;
                }
            }
        }
        if let Some(e) = edge {
            if let Some(w) = e.weight_mm.filter(|w| *w > 0.0 && w.is_finite()) {
                pen.w = w;
            }
            if let Some(rgb) = e.color.as_deref().and_then(parse_hex) {
                pen.rgb = rgb;
            }
            if let Some(dash) = e.line.as_deref().and_then(|l| d.dash_of(l)) {
                pen.dash = dash;
            }
        }
        pen
    }

    /// The pen of a cut outline (plans and sections) for a region on `tag`: the heavy pen, at the tag's
    /// `cut.outline_mm` when it has one, in the tag's line colour when it has one. Continuous whatever the tag's line
    /// style is (a cut is never dashed). Goes through `for_tag`, so the colour rules are the lines' own.
    pub fn for_cut(&self, d: &crate::drafting::Drafting, tag: Option<&str>) -> Pen {
        let base = self.ink(self.heavy);
        let Some(t) = tag.and_then(|t| d.tag_style(t)) else { return base };
        let mut pen = self.for_tag(d, tag, None, base.clone(), false);
        pen.dash = base.dash;
        pen.w = if t.cut.outline_mm > 0.0 && t.cut.outline_mm.is_finite() { t.cut.outline_mm } else { self.heavy };
        pen
    }
}

/// "#rrggbb" as a colour.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// One path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    M([f64; 2]),
    L([f64; 2]),
    C([f64; 2], [f64; 2], [f64; 2]),
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    pub rgb: Rgb,
    pub even_odd: bool,
}

/// The model edge a span of line work came from.
#[derive(Debug, PartialEq)]
pub struct EdgeRef {
    /// instance ids from the root down (empty: the root model)
    pub path: Vec<Id>,
    /// the edge's two vertex ids
    pub v: [Id; 2],
}

/// A span of a model edge: `t` runs 0 to 1 from the first vertex to the second.
#[derive(Clone, Debug, PartialEq)]
pub struct Prov {
    pub edge: Arc<EdgeRef>,
    pub t: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathPrim {
    pub segs: Vec<Seg>,
    pub pen: Option<Pen>,
    pub fill: Option<Fill>,
    pub prov: Option<Prov>,
}

/// What a piece of text is, for the PDF's structure tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    Paragraph,
    /// the project name
    Heading1,
    /// a page's title
    Heading2,
    /// a viewport's caption, or a table's title
    Caption,
    /// a heading inside the notes
    Heading3,
    /// a table cell: its row and column, and whether it heads them
    Cell { row: u16, col: u16, header: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextPrim {
    /// the start of the baseline
    pub at: [f64; 2],
    /// the em size in mm
    pub size: f64,
    pub bold: bool,
    pub rgb: Rgb,
    /// counter-clockwise on the paper, radians
    pub angle: f64,
    pub text: String,
    pub role: Role,
}

#[derive(Clone, Debug)]
pub struct ImagePrim {
    pub rgb: Arc<image::RgbImage>,
    /// x, y, w, h in mm
    pub rect: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct Group {
    /// clip to x, y, w, h in mm
    pub clip: Option<[f64; 4]>,
    /// a drawing or picture (a viewport) with its description, as a figure in a tagged PDF; None: decoration
    pub alt: Option<String>,
    /// 0 to 1, how much of the group shows (1: all of it). Drawn as one transparency group, so lines
    /// overlapping inside it don't darken each other.
    pub opacity: f64,
    pub children: Vec<Prim>,
}

/// A hatched region (design §3.3). PDF and SVG clip to `rings` and draw `lines`; DXF writes a native `HATCH`
/// from `rings` and `families`, so it stays a hatch in CAD. When the pattern was too dense or missing, `lines` is
/// empty and `tint` fills the region instead, with `note` saying why (the export lists it as a warning).
#[derive(Clone, Debug, PartialEq)]
pub struct HatchPrim {
    /// the outer ring and its holes, paper mm, y down (even-odd)
    pub rings: Vec<Vec<[f64; 2]>>,
    /// the pattern's name
    pub name: String,
    /// the pattern placed, in paper mm with y up from the page's bottom left (the DXF's frame)
    pub families: Vec<super::hatch::Placed>,
    /// the angle and scale it was placed with (degrees; paper mm per pattern mm)
    pub angle: f64,
    pub scale: f64,
    pub pen: Pen,
    /// the lines, clipped to the region and dashed, paper mm, y down (a dot is a zero-length line)
    pub lines: Vec<[[f64; 2]; 2]>,
    pub tint: Option<Rgb>,
    pub note: Option<String>,
}

impl HatchPrim {
    /// The region's outline as a path (each ring closed).
    pub fn ring_segs(&self) -> Vec<Seg> {
        let mut segs = Vec::new();
        for r in self.rings.iter().filter(|r| r.len() >= 3) {
            for (i, p) in r.iter().enumerate() {
                segs.push(if i == 0 { Seg::M(*p) } else { Seg::L(*p) });
            }
            segs.push(Seg::Z);
        }
        segs
    }
}

#[derive(Clone, Debug)]
pub enum Prim {
    Path(PathPrim),
    Text(TextPrim),
    Image(ImagePrim),
    Group(Group),
    Hatch(HatchPrim),
}

impl PartialEq for Prim {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Prim::Path(a), Prim::Path(b)) => a == b,
            (Prim::Text(a), Prim::Text(b)) => a == b,
            (Prim::Image(a), Prim::Image(b)) => a.rect == b.rect && a.rgb.as_raw() == b.rgb.as_raw(),
            (Prim::Group(a), Prim::Group(b)) => a.clip == b.clip && a.alt == b.alt && a.opacity == b.opacity && a.children == b.children,
            (Prim::Hatch(a), Prim::Hatch(b)) => a == b,
            _ => false,
        }
    }
}

/// One sheet of paper.
#[derive(Clone, Debug)]
pub struct Page {
    pub w: f64,
    pub h: f64,
    /// the paper color when it isn't white (blueprints)
    pub bg: Option<Rgb>,
    /// what the page is (the SVG's accessible name)
    pub title: String,
    pub items: Vec<Prim>,
}

impl Page {
    pub fn new(w: f64, h: f64) -> Self {
        Page { w, h, bg: None, title: String::new(), items: Vec::new() }
    }
    /// Every primitive, groups opened.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Prim)) {
        fn go<'a>(items: &'a [Prim], f: &mut dyn FnMut(&'a Prim)) {
            for p in items {
                f(p);
                if let Prim::Group(g) = p {
                    go(&g.children, f);
                }
            }
        }
        go(&self.items, f);
    }
    /// All the text on the page, one string per run.
    pub fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.walk(&mut |p| {
            if let Prim::Text(t) = p {
                out.push(t.text.clone());
            }
        });
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub title: String,
    /// the language of the text, as a BCP 47 tag ("" means English)
    pub lang: String,
    pub pages: Vec<Page>,
}

/// Where the drawing code puts primitives: millimeters, y down, with a few drawing
/// helpers. The ported drawers (callouts, dimensions, plans) work in PDF points, y up
/// from the bottom of the sheet, exactly as they always did; `pt_*` take that and
/// store paper millimeters.
pub struct Cv {
    pub h_mm: f64,
    pub out: Vec<Prim>,
    /// the role the next text takes
    pub role: Role,
}

impl Cv {
    pub fn new(h_mm: f64) -> Self {
        Cv { h_mm, out: Vec::new(), role: Role::Paragraph }
    }
    /// sheet millimeters (y down) to points (y up)
    pub fn to_pt(&self, p: [f64; 2]) -> (f64, f64) {
        (p[0] * PT, (self.h_mm - p[1]) * PT)
    }
    fn mm(&self, x: f64, y: f64) -> [f64; 2] {
        [x / PT, self.h_mm - y / PT]
    }
    pub fn push(&mut self, p: Prim) {
        self.out.push(p);
    }
    /// A stroked polyline (points, y up).
    pub fn pline(&mut self, pts: &[(f64, f64)], closed: bool, pen: &Pen) {
        self.path(pts, closed, Some(pen), None);
    }
    pub fn path(&mut self, pts: &[(f64, f64)], closed: bool, pen: Option<&Pen>, fill: Option<Fill>) {
        if pts.len() < 2 {
            return;
        }
        let mut segs: Vec<Seg> = pts.iter().enumerate().map(|(i, p)| if i == 0 { Seg::M(self.mm(p.0, p.1)) } else { Seg::L(self.mm(p.0, p.1)) }).collect();
        if closed {
            segs.push(Seg::Z);
        }
        self.out.push(Prim::Path(PathPrim { segs, pen: pen.cloned(), fill, prov: None }));
    }
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), pen: &Pen) {
        self.path(&[a, b], false, Some(pen), None);
    }
    /// A circle (four Béziers), centre and radius in points.
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, pen: Option<&Pen>, fill: Option<Rgb>) {
        let k = 0.552_284_75 * r;
        let p = |x: f64, y: f64| self.mm(x, y);
        let segs = vec![
            Seg::M(p(cx + r, cy)),
            Seg::C(p(cx + r, cy + k), p(cx + k, cy + r), p(cx, cy + r)),
            Seg::C(p(cx - k, cy + r), p(cx - r, cy + k), p(cx - r, cy)),
            Seg::C(p(cx - r, cy - k), p(cx - k, cy - r), p(cx, cy - r)),
            Seg::C(p(cx + k, cy - r), p(cx + r, cy - k), p(cx + r, cy)),
            Seg::Z,
        ];
        self.out.push(Prim::Path(PathPrim { segs, pen: pen.cloned(), fill: fill.map(|rgb| Fill { rgb, even_odd: false }), prov: None }));
    }
    /// Text at a baseline start (points, y up), `size` in points.
    pub fn text(&mut self, x: f64, y: f64, s: &str, size: f64, bold: bool, rgb: Rgb, angle: f64) {
        if s.is_empty() {
            return;
        }
        self.out.push(Prim::Text(TextPrim { at: self.mm(x, y), size: size / PT, bold, rgb, angle, text: s.to_string(), role: self.role }));
    }
    /// Text centred on x, its middle on y (points).
    pub fn text_c(&mut self, x: f64, y: f64, s: &str, size: f64, bold: bool, rgb: Rgb) {
        let w = super::font::width(s, size, bold);
        self.text(x - w / 2.0, y - size * 0.35, s, size, bold, rgb, 0.0);
    }
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, pen: &Pen) {
        self.pline(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], true, pen);
    }
    /// What's drawn inside `f` is clipped to a rectangle (points: x, y of the corner nearest the
    /// bottom left, then width and height).
    pub fn clipped(&mut self, x: f64, y: f64, w: f64, h: f64, f: impl FnOnce(&mut Cv)) {
        let top_left = self.mm(x, y + h);
        let mut inner = Cv::new(self.h_mm);
        f(&mut inner);
        let rect = [top_left[0], top_left[1], w / PT, h / PT];
        let children = cull(inner.out, &rect);
        self.out.push(Prim::Group(Group { clip: Some(rect), alt: None, opacity: 1.0, children }));
    }
}

/// What a clip rectangle leaves: primitives wholly outside it are dropped (so what's far away
/// can't change the page), and a plain line is cut to it (so a DXF, which has no clip, stays inside).
pub fn cull(items: Vec<Prim>, rect: &[f64; 4]) -> Vec<Prim> {
    let (x0, y0, x1, y1) = (rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]);
    items
        .into_iter()
        .filter_map(|p| match p {
            Prim::Path(mut pp) => {
                let mut lo = [f64::MAX; 2];
                let mut hi = [f64::MIN; 2];
                let mut grow = |q: &[f64; 2]| {
                    for i in 0..2 {
                        lo[i] = lo[i].min(q[i]);
                        hi[i] = hi[i].max(q[i]);
                    }
                };
                for s in &pp.segs {
                    match s {
                        Seg::M(a) | Seg::L(a) => grow(a),
                        Seg::C(a, b, c) => {
                            grow(a);
                            grow(b);
                            grow(c);
                        }
                        Seg::Z => {}
                    }
                }
                if lo[0] > hi[0] {
                    return None;
                }
                let pad = pp.pen.as_ref().map_or(0.0, |p| p.w / 2.0);
                if hi[0] + pad < x0 || lo[0] - pad > x1 || hi[1] + pad < y0 || lo[1] - pad > y1 {
                    return None;
                }
                if let ([Seg::M(a), Seg::L(b)], None) = (&pp.segs[..], pp.fill) {
                    if let Some((t0, t1)) = clip_line(*a, *b, rect) {
                        let (a, b) = (*a, *b);
                        pp.segs = vec![Seg::M([a[0] + (b[0] - a[0]) * t0, a[1] + (b[1] - a[1]) * t0]), Seg::L([a[0] + (b[0] - a[0]) * t1, a[1] + (b[1] - a[1]) * t1])];
                    } else {
                        return None;
                    }
                }
                Some(Prim::Path(pp))
            }
            Prim::Hatch(h) => {
                let mut lo = [f64::MAX; 2];
                let mut hi = [f64::MIN; 2];
                for p in h.rings.iter().flatten() {
                    for i in 0..2 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
                (lo[0] <= hi[0] && !(hi[0] < x0 || lo[0] > x1 || hi[1] < y0 || lo[1] > y1)).then_some(Prim::Hatch(h))
            }
            Prim::Text(t) => {
                let reach = t.size * t.text.chars().count() as f64;
                (t.at[0] + reach >= x0 && t.at[0] - reach <= x1 && t.at[1] + t.size >= y0 && t.at[1] - t.size <= y1).then_some(Prim::Text(t))
            }
            other => Some(other),
        })
        .collect()
}

/// The part of the segment a->b inside the rectangle (x, y, w, h), as parameters 0..1.
pub fn clip_line(a: [f64; 2], b: [f64; 2], r: &[f64; 4]) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-(b[0] - a[0]), a[0] - r[0]), (b[0] - a[0], r[0] + r[2] - a[0]), (-(b[1] - a[1]), a[1] - r[1]), (b[1] - a[1], r[1] + r[3] - a[1])] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 < t1).then_some((t0, t1))
}
