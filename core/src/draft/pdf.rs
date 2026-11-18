//! The PDF writer: a display list to a PDF with vector line work, embedded subsetted
//! fonts (Unicode text works, and copies out of the file as text) and raster images only
//! where a viewport asked for one. Line widths are paper millimeters (`w` = mm x 72/25.4).

use super::font;
use super::ir::*;
use anyhow::{bail, Result};
use pdf_writer::types::{CidFontType, FontFlags, LineCapStyle, LineJoinStyle, StructRole, SystemInfo, TabOrder, UnicodeCmap};
use pdf_writer::writers::Resources;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use std::collections::BTreeMap;
use subsetter::GlyphRemapper;

/// Most pages one file takes (a PDF reader's page tree is flat here).
pub const MAX_PAGES: usize = 500;

struct Face {
    remap: GlyphRemapper,
    /// new glyph id -> (old glyph id, a character it draws)
    used: BTreeMap<u16, (u16, char)>,
}

struct Ctx {
    faces: Vec<Option<Face>>,
    font_refs: Vec<Ref>,
    next: i32,
}

impl Ctx {
    fn alloc(&mut self) -> Ref {
        let r = Ref::new(self.next);
        self.next += 1;
        r
    }
}

fn zlib(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data)?;
    Ok(e.finish()?)
}

fn f(v: f64) -> f32 {
    v as f32
}

/// One tagged marked-content sequence: its structure role, a figure's description, and (a table cell) its place.
struct Tag {
    role: StructRole,
    alt: Option<String>,
    cell: Option<(u16, u16)>,
}

struct PageOut {
    content: Vec<u8>,
    fonts: Vec<usize>,
    images: Vec<(usize, Ref)>,
    /// transparency groups drawn on the page: Fm names, and the ExtGState (Gs names) that fades each
    forms: Vec<(usize, Ref)>,
    gstates: Vec<(usize, Ref)>,
    /// the structure each marked-content id (0..n) belongs to: role, and a figure's description
    tags: Vec<Tag>,
}

struct Writer<'a> {
    c: Content,
    cx: &'a mut Ctx,
    page_h: f64,
    fonts: Vec<usize>,
    images: Vec<(usize, Ref)>,
    forms: Vec<(usize, Ref)>,
    gstates: Vec<(usize, Ref)>,
    tags: Vec<Tag>,
    in_figure: bool,
    page_w: f64,
}

/// The fonts, pictures and transparency groups a content stream (a page or a form) draws, as its resources.
fn resources(res: &mut Resources, font_refs: &[Ref], fonts: &[usize], images: &[(usize, Ref)], forms: &[(usize, Ref)], gstates: &[(usize, Ref)]) {
    if !fonts.is_empty() {
        let mut fd = res.fonts();
        for i in fonts {
            fd.pair(Name(format!("F{i}").as_bytes()), font_refs[*i]);
        }
    }
    if !images.is_empty() || !forms.is_empty() {
        let mut xd = res.x_objects();
        for (n, r) in images {
            xd.pair(Name(format!("Im{n}").as_bytes()), *r);
        }
        for (n, r) in forms {
            xd.pair(Name(format!("Fm{n}").as_bytes()), *r);
        }
    }
    if !gstates.is_empty() {
        let mut gd = res.ext_g_states();
        for (n, r) in gstates {
            gd.pair(Name(format!("Gs{n}").as_bytes()), *r);
        }
    }
}

impl Writer<'_> {
    fn x(&self, mm: f64) -> f32 {
        f(mm * PT)
    }
    fn y(&self, mm: f64) -> f32 {
        f((self.page_h - mm) * PT)
    }

    fn path(&mut self, p: &PathPrim) {
        if p.segs.is_empty() || (p.pen.is_none() && p.fill.is_none()) {
            return;
        }
        self.c.save_state();
        if let Some(fl) = p.fill {
            self.c.set_fill_rgb(fl.rgb[0] as f32 / 255.0, fl.rgb[1] as f32 / 255.0, fl.rgb[2] as f32 / 255.0);
        }
        if let Some(pen) = &p.pen {
            self.c.set_stroke_rgb(pen.rgb[0] as f32 / 255.0, pen.rgb[1] as f32 / 255.0, pen.rgb[2] as f32 / 255.0);
            self.c.set_line_width(f(pen.w * PT));
            if pen.dash.is_empty() {
                self.c.set_line_cap(LineCapStyle::RoundCap).set_line_join(LineJoinStyle::RoundJoin);
            } else {
                self.c.set_line_cap(LineCapStyle::ButtCap).set_line_join(LineJoinStyle::MiterJoin);
                self.c.set_dash_pattern(pen.dash.iter().map(|d| f(d * PT)), 0.0);
            }
        }
        for s in &p.segs {
            match *s {
                Seg::M(a) => {
                    let (x, y) = (self.x(a[0]), self.y(a[1]));
                    self.c.move_to(x, y);
                }
                Seg::L(a) => {
                    let (x, y) = (self.x(a[0]), self.y(a[1]));
                    self.c.line_to(x, y);
                }
                Seg::C(a, b, d) => {
                    let (a, b, d) = ((self.x(a[0]), self.y(a[1])), (self.x(b[0]), self.y(b[1])), (self.x(d[0]), self.y(d[1])));
                    self.c.cubic_to(a.0, a.1, b.0, b.1, d.0, d.1);
                }
                Seg::Z => {
                    self.c.close_path();
                }
            }
        }
        match (&p.pen, p.fill) {
            (Some(_), Some(fl)) => {
                if fl.even_odd {
                    self.c.fill_even_odd_and_stroke();
                } else {
                    self.c.fill_nonzero_and_stroke();
                }
            }
            (Some(_), None) => {
                self.c.stroke();
            }
            (None, Some(fl)) => {
                if fl.even_odd {
                    self.c.fill_even_odd();
                } else {
                    self.c.fill_nonzero();
                }
            }
            (None, None) => {
                self.c.end_path();
            }
        }
        self.c.restore_state();
    }

    fn text(&mut self, t: &TextPrim) {
        let size_pt = t.size * PT;
        let snap = |v: f64| if v.abs() < 1e-9 { 0.0 } else { v };
        let (cos, sin) = (snap(t.angle.cos()), snap(t.angle.sin()));
        let (mut x, mut y) = (t.at[0], t.at[1]); // paper mm, y down
        let clean: String = t.text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        for (face, glyphs) in font::runs(&clean, t.bold) {
            let fc = self.cx.faces[face].get_or_insert_with(|| {
                let mut remap = GlyphRemapper::new();
                remap.remap(0);
                Face { remap, used: BTreeMap::new() }
            });
            let mut bytes = Vec::with_capacity(glyphs.len() * 2);
            let mut adv = 0.0;
            for (g, ch) in &glyphs {
                let new = fc.remap.remap(*g);
                fc.used.entry(new).or_insert((*g, if *g == 0 { '\u{FFFD}' } else { *ch }));
                bytes.extend(new.to_be_bytes());
                adv += font::advance_1000(face, *g);
            }
            if !self.fonts.contains(&face) {
                self.fonts.push(face);
            }
            let name = format!("F{face}");
            self.c.begin_text();
            self.c.set_fill_rgb(t.rgb[0] as f32 / 255.0, t.rgb[1] as f32 / 255.0, t.rgb[2] as f32 / 255.0);
            self.c.set_font(Name(name.as_bytes()), f(size_pt));
            self.c.set_text_matrix([f(cos), f(sin), f(-sin), f(cos), f(x * PT), f((self.page_h - y) * PT)]);
            self.c.show(Str(&bytes));
            self.c.end_text();
            let w_mm = adv * t.size / 1000.0;
            x += cos * w_mm;
            y -= sin * w_mm;
        }
    }

    fn image(&mut self, im: &ImagePrim, pdf: &mut Pdf) -> Result<()> {
        let id = self.cx.alloc();
        let data = zlib(im.rgb.as_raw())?;
        let mut x = pdf.image_xobject(id, &data);
        x.width(im.rgb.width() as i32).height(im.rgb.height() as i32).bits_per_component(8).filter(Filter::FlateDecode);
        x.color_space().device_rgb();
        x.finish();
        let n = self.images.len() + 1;
        self.images.push((n, id));
        self.c.save_state();
        let [rx, ry, rw, rh] = im.rect;
        self.c.transform([f(rw * PT), 0.0, 0.0, f(rh * PT), self.x(rx), self.y(ry + rh)]);
        self.c.x_object(Name(format!("Im{n}").as_bytes()));
        self.c.restore_state();
        Ok(())
    }

    /// A group that shows through: drawn into a Form XObject with an isolated transparency group (so what
    /// overlaps inside it doesn't darken), placed through an ExtGState with the stroke and fill alpha.
    /// It sits in the page's stream where it comes, so stacked groups blend in z order.
    fn faded(&mut self, g: &Group, pdf: &mut Pdf) -> Result<()> {
        let outer = (
            std::mem::replace(&mut self.c, Content::new()),
            std::mem::take(&mut self.fonts),
            std::mem::take(&mut self.images),
            std::mem::take(&mut self.forms),
            std::mem::take(&mut self.gstates),
        );
        // the form's content is part of the figure around it (tagged content doesn't go inside a form)
        let was_figure = std::mem::replace(&mut self.in_figure, true);
        if let Some([x, y, w, h]) = g.clip {
            self.c.rect(self.x(x), self.y(y + h), f(w * PT), f(h * PT));
            self.c.clip_nonzero();
            self.c.end_path();
        }
        let r = self.prims(&g.children, pdf);
        self.in_figure = was_figure;
        let inner = std::mem::replace(&mut self.c, outer.0);
        let fonts = std::mem::replace(&mut self.fonts, outer.1);
        let images = std::mem::replace(&mut self.images, outer.2);
        let forms = std::mem::replace(&mut self.forms, outer.3);
        let gstates = std::mem::replace(&mut self.gstates, outer.4);
        r?;
        let (form, gs) = (self.cx.alloc(), self.cx.alloc());
        let data = zlib(&inner.finish())?;
        let mut x = pdf.form_xobject(form, &data);
        x.bbox(Rect::new(0.0, 0.0, f(self.page_w * PT), f(self.page_h * PT)));
        x.filter(Filter::FlateDecode);
        x.group().transparency().isolated(true).knockout(false).color_space().device_rgb();
        resources(&mut x.resources(), &self.cx.font_refs, &fonts, &images, &forms, &gstates);
        x.finish();
        let a = f(g.opacity.max(0.0));
        pdf.ext_graphics(gs).stroking_alpha(a).non_stroking_alpha(a);
        let n = self.forms.len() + 1;
        self.forms.push((n, form));
        self.gstates.push((n, gs));
        // a form's fonts are the page's fonts too (the list says which faces the page needs)
        for i in fonts {
            if !self.fonts.contains(&i) {
                self.fonts.push(i);
            }
        }
        self.c.save_state();
        self.c.set_parameters(Name(format!("Gs{n}").as_bytes()));
        self.c.x_object(Name(format!("Fm{n}").as_bytes()));
        self.c.restore_state();
        Ok(())
    }

    fn prims(&mut self, items: &[Prim], pdf: &mut Pdf) -> Result<()> {
        for p in items {
            match p {
                Prim::Path(p) => {
                    // line work is part of its figure; the sheet's own rules are artifacts
                    if self.in_figure {
                        self.path(p);
                    } else {
                        self.c.begin_marked_content(Name(b"Artifact"));
                        self.path(p);
                        self.c.end_marked_content();
                    }
                }
                Prim::Text(t) => {
                    let (role, cell) = match t.role {
                        Role::Paragraph => (StructRole::P, None),
                        Role::Heading1 => (StructRole::H1, None),
                        Role::Heading2 => (StructRole::H2, None),
                        Role::Heading3 => (StructRole::H3, None),
                        Role::Caption => (StructRole::Caption, None),
                        Role::Cell { row, col, header } => (if header { StructRole::TH } else { StructRole::TD }, Some((row, col))),
                    };
                    // inside a figure the text is part of the figure's one tagged sequence (tagged content
                    // never nests); it still extracts as text
                    if self.in_figure {
                        self.text(t);
                        continue;
                    }
                    let mcid = self.tags.len() as i32;
                    self.tags.push(Tag { role, alt: None, cell });
                    let missing = t.text.chars().any(|c| !c.is_control() && font::glyph(c, t.bold).1 == 0);
                    {
                        let mut mc = self.c.begin_marked_content_with_properties(Name(b"P"));
                        let mut props = mc.properties();
                        props.identify(mcid);
                        // a character the fonts lack is drawn as a box: the real text goes in ActualText
                        if missing {
                            props.actual_text(TextStr(&t.text));
                        }
                    }
                    self.text(t);
                    self.c.end_marked_content();
                }
                Prim::Image(i) => {
                    if self.in_figure {
                        self.image(i, pdf)?;
                    } else {
                        self.c.begin_marked_content(Name(b"Artifact"));
                        self.image(i, pdf)?;
                        self.c.end_marked_content();
                    }
                }
                Prim::Group(g) => {
                    // a viewport is a figure, described by its caption
                    let figure = g.alt.is_some() && !self.in_figure;
                    if figure {
                        let mcid = self.tags.len() as i32;
                        self.tags.push(Tag { role: StructRole::Figure, alt: g.alt.clone(), cell: None });
                        self.c.begin_marked_content_with_properties(Name(b"Figure")).properties().identify(mcid);
                        self.in_figure = true;
                    }
                    if g.opacity < 1.0 {
                        self.faded(g, pdf)?;
                    } else {
                        self.c.save_state();
                        if let Some([x, y, w, h]) = g.clip {
                            self.c.rect(self.x(x), self.y(y + h), f(w * PT), f(h * PT));
                            self.c.clip_nonzero();
                            self.c.end_path();
                        }
                        self.prims(&g.children, pdf)?;
                        self.c.restore_state();
                    }
                    if figure {
                        self.c.end_marked_content();
                        self.in_figure = false;
                    }
                }
            }
        }
        Ok(())
    }
}

/// The PDF of `doc`: every page, in order.
pub fn write(doc: &Doc) -> Result<Vec<u8>> {
    if doc.pages.is_empty() {
        bail!("nothing to write: no pages");
    }
    if doc.pages.len() > MAX_PAGES {
        bail!("at most {MAX_PAGES} pages in one PDF");
    }
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let mut cx = Ctx { faces: (0..4).map(|_| None).collect(), font_refs: Vec::new(), next: 3 };
    let font_refs: Vec<Ref> = (0..4).map(|_| cx.alloc()).collect();
    cx.font_refs = font_refs.clone();
    let mut kids = Vec::new();
    let mut outs: Vec<(Ref, Ref, PageOut, [f64; 2])> = Vec::new();
    for page in &doc.pages {
        let mut w = Writer { c: Content::new(), cx: &mut cx, page_h: page.h, fonts: Vec::new(), images: Vec::new(), forms: Vec::new(), gstates: Vec::new(), tags: Vec::new(), in_figure: false, page_w: page.w };
        if let Some(bg) = page.bg {
            w.c.set_fill_rgb(bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0);
            w.c.rect(0.0, 0.0, f(page.w * PT), f(page.h * PT));
            w.c.fill_nonzero();
        }
        w.prims(&page.items, &mut pdf)?;
        let out = PageOut { content: w.c.finish().to_vec(), fonts: w.fonts, images: w.images, forms: w.forms, gstates: w.gstates, tags: w.tags };
        let (pid, cid) = (cx.alloc(), cx.alloc());
        kids.push(pid);
        outs.push((pid, cid, out, [page.w, page.h]));
    }
    for (page_no, (pid, cid, out, [w, h])) in outs.iter().enumerate() {
        let data = zlib(&out.content)?;
        pdf.stream(*cid, &data).filter(Filter::FlateDecode);
        let mut p = pdf.page(*pid);
        p.parent(tree).media_box(Rect::new(0.0, 0.0, f(w * PT), f(h * PT))).contents(*cid);
        p.struct_parents(page_no as i32).tab_order(TabOrder::StructureOrder);
        let mut res = p.resources();
        resources(&mut res, &font_refs, &out.fonts, &out.images, &out.forms, &out.gstates);
        res.finish();
        p.finish();
    }
    // the structure tree: a Document of headings, captions, paragraphs and one figure per viewport (described
    // by its caption); the sheet's own rules are artifacts
    let (root, docelem, ptree) = (cx.alloc(), cx.alloc(), cx.alloc());
    let mut paragraphs: Vec<Vec<Ref>> = Vec::new();
    let mut parents: Vec<Vec<Ref>> = Vec::new();
    let mut tables: Vec<Ref> = Vec::new();
    for (pid, _, out, _) in &outs {
        let refs: Vec<Ref> = out.tags.iter().map(|_| cx.alloc()).collect();
        // a page's schedule is a Table of rows of cells
        let rows: std::collections::BTreeMap<u16, Ref> = out.tags.iter().filter_map(|t| t.cell).map(|(r, _)| (r, cx.alloc())).collect();
        let table = (!rows.is_empty()).then(|| cx.alloc());
        for (mcid, (r, t)) in refs.iter().zip(&out.tags).enumerate() {
            let parent = t.cell.map_or(docelem, |(row, _)| rows[&row]);
            let mut e = pdf.struct_element(*r);
            e.kind(t.role).parent(parent).page(*pid);
            if let Some(a) = &t.alt {
                e.alt(TextStr(a));
            }
            e.marked_content_child().marked_content_id(mcid as i32);
        }
        if let Some(table) = table {
            for (row, tr) in &rows {
                let mut cells: Vec<(u16, Ref)> = out.tags.iter().zip(&refs).filter_map(|(t, r)| t.cell.filter(|c| c.0 == *row).map(|c| (c.1, *r))).collect();
                cells.sort_by_key(|c| c.0);
                let mut e = pdf.struct_element(*tr);
                e.kind(StructRole::TR).parent(table).page(*pid);
                e.insert(Name(b"K")).array().items(cells.iter().map(|c| c.1));
            }
            let mut e = pdf.struct_element(table);
            e.kind(StructRole::Table).parent(docelem).page(*pid);
            e.insert(Name(b"K")).array().items(rows.values().copied());
            tables.push(table);
        }
        paragraphs.push(refs.iter().zip(&out.tags).filter(|(_, t)| t.cell.is_none()).map(|(r, _)| *r).chain(table).collect::<Vec<_>>());
        parents.push(refs);
    }
    {
        let mut e = pdf.struct_element(docelem);
        e.kind(StructRole::Document).parent(root);
        let mut kids = e.insert(Name(b"K")).array();
        kids.items(paragraphs.iter().flatten().copied());
    }
    {
        let mut d = pdf.indirect(ptree).dict();
        let mut nums = d.insert(Name(b"Nums")).array();
        for (i, refs) in parents.iter().enumerate() {
            nums.item(i as i32);
            nums.push().array().items(refs.iter().copied());
        }
    }
    {
        let mut d = pdf.indirect(root).dict();
        d.pair(Name(b"Type"), Name(b"StructTreeRoot"));
        d.pair(Name(b"K"), docelem);
        d.pair(Name(b"ParentTree"), ptree);
    }
    {
        let mut cat = pdf.catalog(catalog);
        cat.pages(tree);
        cat.pair(Name(b"Lang"), TextStr(if doc.lang.is_empty() { "en" } else { &doc.lang }));
        cat.pair(Name(b"StructTreeRoot"), root);
        cat.mark_info().marked(true);
        cat.viewer_preferences().display_doc_title(true);
    }
    pdf.pages(tree).kids(kids.iter().copied()).count(kids.len() as i32);
    // the fonts: only what the text used, as a CID font so any Unicode text works
    for i in 0..4 {
        if cx.faces[i].is_none() {
            continue;
        }
        let (type0, cid, desc, file, cmap) = (font_refs[i], cx.alloc(), cx.alloc(), cx.alloc(), cx.alloc());
        let Some(fc) = &cx.faces[i] else { continue };
        let fonts = font::fonts();
        let face = &fonts.faces[i];
        let subset = subsetter::subset(fonts.data[i], 0, &fc.remap).map_err(|e| anyhow::anyhow!("couldn't subset the font: {e:?}"))?;
        let base = format!("{}{}", tag(i), font::name(i));
        let sub = zlib(&subset)?;
        pdf.stream(file, &sub).filter(Filter::FlateDecode);
        let (ascent, descent, cap, bb) = face.measures();
        pdf.font_descriptor(desc)
            .name(Name(base.as_bytes()))
            .flags(FontFlags::NON_SYMBOLIC)
            .bbox(Rect::new(bb[0], bb[1], bb[2], bb[3]))
            .italic_angle(0.0)
            .ascent(ascent)
            .descent(descent)
            .cap_height(cap)
            .stem_v(if i >= 2 { 120.0 } else { 80.0 })
            .font_file2(file);
        let info = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };
        pdf.type0_font(type0).base_font(Name(base.as_bytes())).encoding_predefined(Name(b"Identity-H")).descendant_font(cid).to_unicode(cmap);
        let mut cf = pdf.cid_font(cid);
        cf.subtype(CidFontType::Type2).base_font(Name(base.as_bytes())).system_info(info).font_descriptor(desc).default_width(0.0);
        cf.cid_to_gid_map_predefined(Name(b"Identity"));
        {
            let mut ws = cf.widths();
            for (new, (old, _)) in &fc.used {
                ws.consecutive(*new, [font::advance_1000(i, *old) as f32]);
            }
        }
        cf.finish();
        let mut um = UnicodeCmap::new(Name(b"Custom"), info);
        for (new, (_, ch)) in &fc.used {
            um.pair(*new, *ch);
        }
        pdf.cmap(cmap, &um.finish()).name(Name(b"Custom"));
    }
    let info = cx.alloc();
    let mut di = pdf.document_info(info);
    di.title(TextStr(if doc.title.is_empty() { "Drawing sheets" } else { &doc.title })).producer(TextStr("SketchCraft"));
    di.finish();
    Ok(pdf.finish())
}

/// A subset's six-letter tag (the PDF spec wants one in the font name).
fn tag(i: usize) -> String {
    let mut s = String::new();
    let mut n = 0x2f_3a1 + i * 7919;
    for _ in 0..6 {
        s.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
        n += 11;
    }
    format!("{s}+")
}
