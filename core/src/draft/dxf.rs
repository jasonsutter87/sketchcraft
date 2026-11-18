//! The DXF writer: one page of the display list as an AC1015 (AutoCAD 2000) drawing in
//! paper millimeters, y up. Pen widths are the lineweight (group code 370, hundredths of
//! a mm, snapped to the standard set), pen colors are true colors (420), dashes are
//! linetypes made from the pen's pattern, fills are solid HATCH entities, text is TEXT
//! (non-ASCII written as `\U+XXXX`). Images aren't written, and clip rectangles are
//! ignored (line work is already clipped to its viewport when the display list is built).
//!
//! The file carries what R2000 readers expect: every symbol table (VPORT, LTYPE, LAYER, STYLE,
//! VIEW, UCS, APPID, DIMSTYLE, BLOCK_RECORD), owner handles (330), the model and paper space
//! blocks with their layouts, and a root dictionary with the group and layout dictionaries.

use super::ir::*;
use std::collections::BTreeMap;
use std::fmt::Write;

/// The lineweights a DXF may name, in hundredths of a millimeter.
const WEIGHTS: [i32; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

pub fn lineweight(mm: f64) -> i32 {
    let h = mm * 100.0;
    *WEIGHTS.iter().min_by(|a, b| (**a as f64 - h).abs().total_cmp(&(**b as f64 - h).abs())).unwrap()
}

/// The *Model_Space block record, which owns every entity.
const MODEL: &str = "17";

struct W {
    s: String,
    h: u32,
    page_h: f64,
    ltypes: BTreeMap<String, Vec<f64>>,
}

fn flatten(segs: &[Seg]) -> Vec<(Vec<[f64; 2]>, bool)> {
    let mut out: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();
    let mut cur: Vec<[f64; 2]> = Vec::new();
    for s in segs {
        match *s {
            Seg::M(p) => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur = vec![p];
            }
            Seg::L(p) => cur.push(p),
            Seg::C(a, b, c) => {
                let p0 = *cur.last().unwrap_or(&a);
                for k in 1..=12 {
                    let t = k as f64 / 12.0;
                    let u = 1.0 - t;
                    let f = |i: usize| u * u * u * p0[i] + 3.0 * u * u * t * a[i] + 3.0 * u * t * t * b[i] + t * t * t * c[i];
                    cur.push([f(0), f(1)]);
                }
            }
            Seg::Z => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), true));
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

fn true_color(c: Rgb) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

impl W {
    fn g(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.s, "{code}\n{v}\n");
    }
    fn handle(&mut self) -> String {
        self.h += 1;
        format!("{:X}", self.h)
    }
    fn y(&self, y: f64) -> f64 {
        self.page_h - y
    }
    /// The start of an entity: its handle, owner and the AcDbEntity part (layer, linetype, lineweight,
    /// color), then the entity's own subclass marker.
    fn ent(&mut self, kind: &str, sub: &str, pen: Option<&Pen>, color: Option<Rgb>) {
        let h = self.handle();
        self.g(0, kind);
        self.g(5, h);
        self.g(330, MODEL);
        self.g(100, "AcDbEntity");
        self.g(8, "0");
        if let Some(p) = pen {
            if !p.dash.is_empty() {
                let name = format!("SC_DASH_{}", p.dash.iter().map(|d| format!("{d:.2}")).collect::<Vec<_>>().join("_"));
                self.ltypes.insert(name.clone(), p.dash.clone());
                self.g(6, name);
            }
            self.g(62, 256);
            self.g(370, lineweight(p.w));
        }
        if let Some(c) = pen.map(|p| p.rgb).or(color) {
            self.g(420, true_color(c));
        }
        self.g(100, sub);
    }

    fn prims(&mut self, items: &[Prim]) {
        for p in items {
            match p {
                Prim::Path(p) => {
                    let subs = flatten(&p.segs);
                    if let Some(f) = p.fill {
                        if !subs.is_empty() {
                            self.hatch(&subs, f.rgb);
                        }
                    }
                    if let Some(pen) = &p.pen {
                        for (pts, closed) in &subs {
                            if pts.len() == 2 && !closed {
                                self.ent("LINE", "AcDbLine", Some(pen), None);
                                self.g(10, pts[0][0]);
                                self.g(20, self.y(pts[0][1]));
                                self.g(30, 0);
                                self.g(11, pts[1][0]);
                                self.g(21, self.y(pts[1][1]));
                                self.g(31, 0);
                            } else {
                                self.ent("LWPOLYLINE", "AcDbPolyline", Some(pen), None);
                                self.g(90, pts.len());
                                self.g(70, i32::from(*closed));
                                for q in pts {
                                    self.g(10, q[0]);
                                    self.g(20, self.y(q[1]));
                                }
                            }
                        }
                    }
                }
                Prim::Text(t) => {
                    let mut s = String::new();
                    for c in t.text.chars() {
                        if c.is_control() {
                            s.push(' ');
                        } else if (c as u32) < 128 {
                            s.push(c);
                        } else {
                            let _ = write!(s, "\\U+{:04X}", c as u32);
                        }
                    }
                    self.ent("TEXT", "AcDbText", None, Some(t.rgb));
                    self.g(10, t.at[0]);
                    self.g(20, self.y(t.at[1]));
                    self.g(30, 0);
                    self.g(40, t.size * 0.72);
                    self.g(1, s);
                    self.g(50, t.angle.to_degrees());
                    self.g(7, "Standard");
                    self.g(100, "AcDbText");
                }
                Prim::Image(_) => {}
                Prim::Group(g) => self.prims(&g.children),
                Prim::Hatch(h) => {
                    let rings: Vec<(Vec<[f64; 2]>, bool)> = h.rings.iter().filter(|r| r.len() >= 3).map(|r| (r.clone(), true)).collect();
                    if rings.is_empty() {
                        continue;
                    }
                    match h.tint {
                        Some(t) => self.hatch(&rings, t),
                        None if !h.families.is_empty() => self.pattern_hatch(h, &rings),
                        None => {}
                    }
                }
            }
        }
    }

    /// A native pattern `HATCH`: the region's boundary and the pattern's line families (already scaled and turned,
    /// in the drawing's frame, as the format stores them), so a CAD program draws and edits it as a hatch.
    fn pattern_hatch(&mut self, h: &HatchPrim, rings: &[(Vec<[f64; 2]>, bool)]) {
        let pen = Pen::new(h.pen.w, h.pen.rgb);
        self.ent("HATCH", "AcDbHatch", Some(&pen), None);
        self.g(10, 0);
        self.g(20, 0);
        self.g(30, 0);
        self.g(210, 0);
        self.g(220, 0);
        self.g(230, 1);
        let name: String = h.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_uppercase() } else { '_' }).collect();
        self.g(2, if name.is_empty() { "SC_HATCH".to_string() } else { name });
        self.g(70, 0); // a pattern, not a solid fill
        self.g(71, 0);
        self.g(91, rings.len());
        for (i, (pts, _)) in rings.iter().enumerate() {
            self.g(92, if i == 0 { 3 } else { 2 }); // polyline; the first is the outer boundary
            self.g(72, 0);
            self.g(73, 1);
            self.g(93, pts.len());
            for q in pts {
                self.g(10, q[0]);
                self.g(20, self.y(q[1]));
            }
            self.g(97, 0);
        }
        self.g(75, 0); // odd parity: holes stay open
        self.g(76, 2); // a custom pattern (its lines follow)
        self.g(52, h.angle);
        self.g(41, h.scale);
        self.g(77, 0);
        self.g(78, h.families.len());
        for f in &h.families {
            self.g(53, f.angle);
            self.g(43, f.base[0]);
            self.g(44, f.base[1]);
            self.g(45, f.offset[0]);
            self.g(46, f.offset[1]);
            self.g(79, f.dash.len());
            for d in &f.dash {
                self.g(49, *d);
            }
        }
        self.g(98, 0);
    }

    fn hatch(&mut self, subs: &[(Vec<[f64; 2]>, bool)], rgb: Rgb) {
        self.ent("HATCH", "AcDbHatch", None, Some(rgb));
        self.g(10, 0);
        self.g(20, 0);
        self.g(30, 0);
        self.g(210, 0);
        self.g(220, 0);
        self.g(230, 1);
        self.g(2, "SOLID");
        self.g(70, 1);
        self.g(71, 0);
        self.g(91, subs.len());
        for (pts, _) in subs {
            self.g(92, 3); // a polyline boundary (external is 1 | 2)
            self.g(72, 0);
            self.g(73, 1);
            self.g(93, pts.len());
            for q in pts {
                self.g(10, q[0]);
                self.g(20, self.y(q[1]));
            }
            self.g(97, 0);
        }
        self.g(75, 0);
        self.g(76, 1);
        self.g(98, 0);
    }

    /// A symbol table's opening: its handle and (as every record's owner) itself.
    fn table(&mut self, name: &str, handle: &str, count: usize) {
        self.g(0, "TABLE");
        self.g(2, name);
        self.g(5, handle);
        self.g(330, 0);
        self.g(100, "AcDbSymbolTable");
        self.g(70, count);
    }
    fn record(&mut self, kind: &str, handle: &str, owner: &str, sub: &str) {
        self.g(0, kind);
        self.g(5, handle);
        self.g(330, owner);
        self.g(100, "AcDbSymbolTableRecord");
        self.g(100, sub);
    }
    fn layout(&mut self, handle: &str, name: &str, block: &str, model: bool, size: (f64, f64)) {
        self.g(0, "LAYOUT");
        self.g(5, handle);
        self.g(330, "D");
        self.g(100, "AcDbPlotSettings");
        self.g(1, "");
        self.g(4, "");
        self.g(6, "");
        for (c, v) in [(40, 0.0), (41, 0.0), (42, 0.0), (43, 0.0)] {
            self.g(c, v);
        }
        self.g(44, size.0);
        self.g(45, size.1);
        for c in [46, 47, 48, 49, 140, 141] {
            self.g(c, 0.0);
        }
        self.g(142, 1.0);
        self.g(143, 1.0);
        self.g(70, if model { 1024 } else { 0 });
        self.g(72, 1);
        self.g(73, 0);
        self.g(74, 5);
        self.g(7, "");
        self.g(75, 16);
        self.g(76, 0);
        self.g(77, 2);
        self.g(78, 300);
        self.g(147, 1.0);
        self.g(148, 0.0);
        self.g(149, 0.0);
        self.g(100, "AcDbLayout");
        self.g(1, name);
        self.g(70, 1);
        self.g(71, if model { 0 } else { 1 });
        self.g(10, 0.0);
        self.g(20, 0.0);
        self.g(11, size.0);
        self.g(21, size.1);
        self.g(12, 0.0);
        self.g(22, 0.0);
        self.g(32, 0.0);
        self.g(14, 1e20);
        self.g(24, 1e20);
        self.g(34, 1e20);
        self.g(15, -1e20);
        self.g(25, -1e20);
        self.g(35, -1e20);
        self.g(146, 0.0);
        self.g(13, 0.0);
        self.g(23, 0.0);
        self.g(33, 0.0);
        self.g(16, 1.0);
        self.g(26, 0.0);
        self.g(36, 0.0);
        self.g(17, 0.0);
        self.g(27, 1.0);
        self.g(37, 0.0);
        self.g(76, 1);
        self.g(330, block);
    }
}

/// How many drawings on the page are faded: a DXF has no opacity, so it draws them at full strength.
pub fn dropped_opacity(p: &Page) -> usize {
    let mut n = 0;
    p.walk(&mut |x| {
        if matches!(x, Prim::Group(g) if g.opacity < 1.0) {
            n += 1;
        }
    });
    n
}

/// One page as a DXF (AC1015) file's text.
pub fn page(p: &Page) -> String {
    let mut body = W { s: String::new(), h: 0xFFF, page_h: p.h, ltypes: BTreeMap::new() };
    if let Some(bg) = p.bg {
        // the blueprint's paper: a solid rectangle behind everything
        let ring: Vec<[f64; 2]> = vec![[0.0, 0.0], [p.w, 0.0], [p.w, p.h], [0.0, p.h]];
        body.hatch(&[(ring, true)], bg);
    }
    body.prims(&p.items);
    let seed = body.h + 1;
    let (w, h) = (p.w, p.h);
    let mut t = W { s: String::new(), h: 0xFF, page_h: p.h, ltypes: BTreeMap::new() };
    t.g(0, "SECTION");
    t.g(2, "HEADER");
    t.g(9, "$ACADVER");
    t.g(1, "AC1015");
    t.g(9, "$HANDSEED");
    t.g(5, format!("{seed:X}"));
    t.g(9, "$INSUNITS");
    t.g(70, 4);
    t.g(9, "$MEASUREMENT");
    t.g(70, 1);
    t.g(9, "$LWDISPLAY");
    t.g(290, 1);
    t.g(9, "$EXTMIN");
    t.g(10, 0);
    t.g(20, 0);
    t.g(30, 0);
    t.g(9, "$EXTMAX");
    t.g(10, w);
    t.g(20, h);
    t.g(30, 0);
    t.g(0, "ENDSEC");
    // tables
    t.g(0, "SECTION");
    t.g(2, "TABLES");
    t.table("VPORT", "8", 1);
    t.record("VPORT", "23", "8", "AcDbViewportTableRecord");
    t.g(2, "*Active");
    t.g(70, 0);
    for (c, a, b) in [(10, 0.0, 0.0), (11, 1.0, 1.0), (12, w / 2.0, h / 2.0), (13, 0.0, 0.0), (14, 10.0, 10.0), (15, 10.0, 10.0), (16, 0.0, 0.0)] {
        t.g(c, a);
        t.g(c + 10, b);
    }
    t.g(36, 1.0);
    t.g(17, 0.0);
    t.g(27, 0.0);
    t.g(37, 0.0);
    t.g(40, h);
    t.g(41, w / h);
    t.g(42, 50.0);
    t.g(43, 0.0);
    t.g(44, 0.0);
    t.g(50, 0.0);
    t.g(51, 0.0);
    for (c, v) in [(71, 0), (72, 1000), (73, 1), (74, 3), (75, 0), (76, 0), (77, 0), (78, 0), (281, 0), (65, 0)] {
        t.g(c, v);
    }
    t.g(146, 0.0);
    t.g(0, "ENDTAB");
    let dashes = body.ltypes.clone();
    t.table("LTYPE", "2", 3 + dashes.len());
    for (name, h) in [("ByBlock", "24"), ("ByLayer", "25"), ("Continuous", "26")] {
        t.record("LTYPE", h, "2", "AcDbLinetypeTableRecord");
        t.g(2, name);
        t.g(70, 0);
        t.g(3, "");
        t.g(72, 65);
        t.g(73, 0);
        t.g(40, 0.0);
    }
    for (name, pat) in &dashes {
        let hd = t.handle();
        t.record("LTYPE", &hd, "2", "AcDbLinetypeTableRecord");
        t.g(2, name);
        t.g(70, 0);
        t.g(3, "Dashed");
        t.g(72, 65);
        t.g(73, pat.len());
        t.g(40, pat.iter().sum::<f64>());
        for (i, d) in pat.iter().enumerate() {
            t.g(49, if i % 2 == 0 { *d } else { -*d });
            t.g(74, 0);
        }
    }
    t.g(0, "ENDTAB");
    t.table("LAYER", "1", 1);
    t.record("LAYER", "27", "1", "AcDbLayerTableRecord");
    t.g(2, "0");
    t.g(70, 0);
    t.g(62, 7);
    t.g(6, "Continuous");
    t.g(370, -3);
    t.g(0, "ENDTAB");
    t.table("STYLE", "5", 1);
    t.record("STYLE", "29", "5", "AcDbTextStyleTableRecord");
    t.g(2, "Standard");
    t.g(70, 0);
    t.g(40, 0);
    t.g(41, 1);
    t.g(50, 0);
    t.g(71, 0);
    t.g(42, 2.5);
    t.g(3, "txt");
    t.g(4, "");
    t.g(0, "ENDTAB");
    t.table("VIEW", "7", 0);
    t.g(0, "ENDTAB");
    t.table("UCS", "6", 0);
    t.g(0, "ENDTAB");
    t.table("APPID", "3", 1);
    t.record("APPID", "2A", "3", "AcDbRegAppTableRecord");
    t.g(2, "ACAD");
    t.g(70, 0);
    t.g(0, "ENDTAB");
    t.table("DIMSTYLE", "4", 1);
    t.g(100, "AcDbDimStyleTable");
    t.g(0, "DIMSTYLE");
    t.g(105, "2B");
    t.g(330, "4");
    t.g(100, "AcDbSymbolTableRecord");
    t.g(100, "AcDbDimStyleTableRecord");
    t.g(2, "Standard");
    t.g(70, 0);
    t.g(0, "ENDTAB");
    t.table("BLOCK_RECORD", "9", 2);
    t.record("BLOCK_RECORD", MODEL, "9", "AcDbBlockTableRecord");
    t.g(2, "*Model_Space");
    t.g(340, "1A");
    t.record("BLOCK_RECORD", "1B", "9", "AcDbBlockTableRecord");
    t.g(2, "*Paper_Space");
    t.g(340, "1E");
    t.g(0, "ENDTAB");
    t.g(0, "ENDSEC");
    t.g(0, "SECTION");
    t.g(2, "BLOCKS");
    for (name, h, he, owner) in [("*Model_Space", "18", "19", MODEL), ("*Paper_Space", "1C", "1D", "1B")] {
        t.g(0, "BLOCK");
        t.g(5, h);
        t.g(330, owner);
        t.g(100, "AcDbEntity");
        t.g(8, "0");
        t.g(100, "AcDbBlockBegin");
        t.g(2, name);
        t.g(70, 0);
        t.g(10, 0);
        t.g(20, 0);
        t.g(30, 0);
        t.g(3, name);
        t.g(1, "");
        t.g(0, "ENDBLK");
        t.g(5, he);
        t.g(330, owner);
        t.g(100, "AcDbEntity");
        t.g(8, "0");
        t.g(100, "AcDbBlockEnd");
    }
    t.g(0, "ENDSEC");
    t.g(0, "SECTION");
    t.g(2, "ENTITIES");
    t.s.push_str(&body.s);
    t.g(0, "ENDSEC");
    t.g(0, "SECTION");
    t.g(2, "OBJECTS");
    t.g(0, "DICTIONARY");
    t.g(5, "A");
    t.g(330, 0);
    t.g(100, "AcDbDictionary");
    t.g(281, 1);
    t.g(3, "ACAD_GROUP");
    t.g(350, "C");
    t.g(3, "ACAD_LAYOUT");
    t.g(350, "D");
    t.g(0, "DICTIONARY");
    t.g(5, "C");
    t.g(330, "A");
    t.g(100, "AcDbDictionary");
    t.g(281, 1);
    t.g(0, "DICTIONARY");
    t.g(5, "D");
    t.g(330, "A");
    t.g(100, "AcDbDictionary");
    t.g(281, 1);
    t.g(3, "Model");
    t.g(350, "1A");
    t.g(3, "Layout1");
    t.g(350, "1E");
    t.layout("1A", "Model", MODEL, true, (w, h));
    t.layout("1E", "Layout1", "1B", false, (w, h));
    t.g(0, "ENDSEC");
    t.g(0, "EOF");
    t.s
}
