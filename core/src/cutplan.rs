//! The cut plan (docs/dev/woodworker.md): the Cut list's parts packed onto sheet goods and boards, what to buy and what
//! it costs, cutting diagrams (SVG and PDF) and part labels (PDF).
//!
//! The parts are `plugins::measure`'s (the same ones `parts` and the Cut list report, numbered in `parts_layout`'s
//! order). Stock, kerf, trim and prices are kept in the model (`Model::cut_stock`). Sheets are packed with a guillotine
//! heuristic (best short side fit, shorter-leftover-axis split: every cut runs edge to edge, so a panel saw can make
//! them), boards with first-fit decreasing. Both are deterministic: ties are broken by size, name and id, never by how
//! the model happens to be stored.

use crate::draft::ir::{Doc, Fill, Page, PathPrim, Pen, Prim, Role, Seg, TextPrim, BLACK};
use crate::draft::{font, pdf, svg};
use crate::model::{Id, Model};
use crate::plugins::{measure, Measured};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const EPS: f64 = 1e-9;
/// Most parts one plan takes (the packing looks at every open sheet for each part).
pub const MAX_PARTS: usize = 5000;
const MAX_MATERIALS: usize = 200;
const MAX_LENGTHS: usize = 32;
const INCH: f64 = 0.0254;

fn d_kerf() -> f64 {
    0.003
}
fn d_sheet() -> [f64; 2] {
    [2.44, 1.22]
}
fn d_lengths() -> Vec<f64> {
    vec![2.44]
}
fn d_currency() -> String {
    "$".into()
}

/// What is bought, per material.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialStock {
    /// the sheet size [length, width] in meters, when it is not the default
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<[f64; 2]>,
    /// the board lengths sold, when they are not the default
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_lengths: Option<Vec<f64>>,
    /// the material has a grain: its parts keep their length along the sheet's length (never turned a quarter turn)
    pub grain: bool,
    /// the price of one sheet
    pub sheet_price: f64,
    /// the price of one board, whatever its size (used when there is no price per board foot)
    pub board_price: f64,
    /// the price of a board foot; wins over `board_price` when it is set
    pub board_foot_price: f64,
}

/// The stock, kerf, trim and prices of a model (saved with it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CutStock {
    /// the saw's cut, meters
    pub kerf: f64,
    /// cut away around a sheet's edge and a board's ends before anything is placed, meters
    pub trim: f64,
    /// the default sheet [length, width], meters (4 × 8 ft)
    pub sheet: [f64; 2],
    /// the default board lengths sold, meters (8 ft)
    pub board_lengths: Vec<f64>,
    /// board lengths by lumber name ("2×4") or "T×W mm" for other boards, where they are not the default
    pub lengths_by_size: BTreeMap<String, Vec<f64>>,
    /// shown before prices
    pub currency: String,
    pub materials: BTreeMap<String, MaterialStock>,
}

impl Default for CutStock {
    fn default() -> Self {
        CutStock { kerf: d_kerf(), trim: 0.0, sheet: d_sheet(), board_lengths: d_lengths(), lengths_by_size: BTreeMap::new(), currency: d_currency(), materials: BTreeMap::new() }
    }
}

impl CutStock {
    pub fn is_default(&self) -> bool {
        *self == CutStock::default()
    }

    fn material(&self, name: &str) -> MaterialStock {
        self.materials.get(name).cloned().unwrap_or_default()
    }

    /// The numbers must be ones a saw and a lumber yard could mean.
    pub fn check(&self) -> Result<()> {
        let size = |v: f64, what: &str| -> Result<()> {
            if !v.is_finite() || !(0.05..=20.0).contains(&v) {
                bail!("{what} must be between 0.05 and 20 meters");
            }
            Ok(())
        };
        if !self.kerf.is_finite() || !(0.0..=0.05).contains(&self.kerf) {
            bail!("the kerf must be between 0 and 0.05 meters");
        }
        if !self.trim.is_finite() || !(0.0..=0.5).contains(&self.trim) {
            bail!("the trim must be between 0 and 0.5 meters");
        }
        size(self.sheet[0], "a sheet's length")?;
        size(self.sheet[1], "a sheet's width")?;
        let lengths = |v: &Vec<f64>| -> Result<()> {
            if v.is_empty() || v.len() > MAX_LENGTHS {
                bail!("give 1 to {MAX_LENGTHS} board lengths");
            }
            v.iter().try_for_each(|l| size(*l, "a board's length"))
        };
        lengths(&self.board_lengths)?;
        if self.lengths_by_size.len() > MAX_MATERIALS || self.materials.len() > MAX_MATERIALS {
            bail!("at most {MAX_MATERIALS} materials and board sizes");
        }
        for (k, v) in &self.lengths_by_size {
            if k.len() > 100 {
                bail!("a board size name is at most 100 characters");
            }
            lengths(v)?;
        }
        if self.currency.chars().count() > 8 {
            bail!("the currency sign is at most 8 characters");
        }
        for (k, m) in &self.materials {
            if k.chars().count() > 100 {
                bail!("a material's name is at most 100 characters");
            }
            if let Some(s) = m.sheet {
                size(s[0], "a sheet's length")?;
                size(s[1], "a sheet's width")?;
            }
            if let Some(l) = &m.board_lengths {
                lengths(l)?;
            }
            for (p, what) in [(m.sheet_price, "a sheet price"), (m.board_price, "a board price"), (m.board_foot_price, "a price per board foot")] {
                if !p.is_finite() || !(0.0..=1e7).contains(&p) {
                    bail!("{what} must be between 0 and 10,000,000");
                }
            }
        }
        Ok(())
    }

    /// `cut_stock`'s arguments applied: what is given changes, the rest stays. A material given as null is removed.
    pub fn apply(&mut self, a: &Value) -> Result<()> {
        let obj = a.as_object().cloned().unwrap_or_default();
        if obj.get("reset").and_then(Value::as_bool) == Some(true) {
            *self = CutStock::default();
        }
        let num = |k: &str| obj.get(k).and_then(Value::as_f64);
        if let Some(v) = num("kerf") {
            self.kerf = v;
        }
        if let Some(v) = num("trim") {
            self.trim = v;
        }
        let pair = |v: &Value| -> Result<[f64; 2]> {
            match v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()) {
                Some(p) if p.len() == 2 => Ok([p[0], p[1]]),
                _ => bail!("a sheet size is [length, width] in meters"),
            }
        };
        let list = |v: &Value| -> Result<Vec<f64>> {
            match v.as_array() {
                Some(a) if a.iter().all(Value::is_number) => Ok(a.iter().filter_map(Value::as_f64).collect()),
                _ => bail!("board lengths are a list of meters"),
            }
        };
        if let Some(v) = obj.get("sheet") {
            self.sheet = pair(v)?;
        }
        if let Some(v) = obj.get("board_lengths") {
            self.board_lengths = list(v)?;
        }
        if let Some(v) = obj.get("currency").and_then(Value::as_str) {
            self.currency = v.to_string();
        }
        if let Some(by) = obj.get("lengths_by_size").and_then(Value::as_object) {
            for (k, v) in by {
                if v.is_null() {
                    self.lengths_by_size.remove(k);
                } else {
                    self.lengths_by_size.insert(k.clone(), list(v)?);
                }
            }
        }
        if let Some(ms) = obj.get("materials").and_then(Value::as_object) {
            for (name, v) in ms {
                if v.is_null() {
                    self.materials.remove(name);
                    continue;
                }
                let Some(o) = v.as_object() else { bail!("a material is an object: {{sheet_price, board_price, ...}}") };
                let m = self.materials.entry(name.clone()).or_default();
                for (k, x) in o {
                    match k.as_str() {
                        "sheet" => m.sheet = if x.is_null() { None } else { Some(pair(x)?) },
                        "board_lengths" => m.board_lengths = if x.is_null() { None } else { Some(list(x)?) },
                        "grain" => m.grain = x.as_bool().ok_or_else(|| anyhow::anyhow!("grain is true or false"))?,
                        "sheet_price" => m.sheet_price = x.as_f64().ok_or_else(|| anyhow::anyhow!("a price is a number"))?,
                        "board_price" => m.board_price = x.as_f64().ok_or_else(|| anyhow::anyhow!("a price is a number"))?,
                        "board_foot_price" => m.board_foot_price = x.as_f64().ok_or_else(|| anyhow::anyhow!("a price is a number"))?,
                        other => bail!("a material has sheet, board_lengths, grain, sheet_price, board_price and board_foot_price (not {other:?})"),
                    }
                }
            }
        }
        self.check()
    }
}

// ---- the lumber names (the Cut list plugin's table, core/plugins/cutlist/main.js) -------------------------------------

/// Dimensional lumber: nominal [thickness, width] in inches to the actual size.
const NOMINAL_T: [(u32, f64); 4] = [(1, 0.75), (2, 1.5), (4, 3.5), (6, 5.5)];
const NOMINAL_W: [(u32, f64); 7] = [(2, 1.5), (3, 2.5), (4, 3.5), (6, 5.5), (8, 7.25), (10, 9.25), (12, 11.25)];

/// The lumber a thickness and width (meters) are, within 1.5 mm either way round: its name ("2×4") and nominal size.
pub fn lumber(thick: f64, wide: f64) -> Option<(String, [f64; 2])> {
    let (a, b) = (thick.min(wide), thick.max(wide));
    for (nt, at) in NOMINAL_T {
        for (nw, aw) in NOMINAL_W {
            if nw >= nt && (at * INCH - a).abs() <= 0.0015 && (aw * INCH - b).abs() <= 0.0015 {
                return Some((format!("{nt}×{nw}"), [nt as f64, nw as f64]));
            }
        }
    }
    None
}

/// A part is sheet goods when it is at most 40 mm thick and at least 100 mm wide (the Cut list's rule).
pub fn is_sheet(size: [f64; 3]) -> bool {
    size[2] <= 0.04 && size[1] >= 0.1
}

/// Meters to inches to the nearest 1/16: `1 3/4"`.
pub fn inches(m: f64) -> String {
    let sixteenths = (m / INCH * 16.0).round() as i64;
    let (whole, mut f, mut d) = (sixteenths.div_euclid(16), sixteenths.rem_euclid(16), 16);
    while f != 0 && f % 2 == 0 {
        f /= 2;
        d /= 2;
    }
    if f != 0 && whole == 0 {
        format!("{f}/{d}\"")
    } else if f != 0 {
        format!("{whole} {f}/{d}\"")
    } else {
        format!("{whole}\"")
    }
}

/// A length the way the model shows lengths: millimeters, or inches for the imperial units.
pub fn fmt_len(m: f64, units: &str) -> String {
    if matches!(units, "ft" | "in") {
        inches(m)
    } else {
        format!("{} mm", (m * 1000.0).round() as i64)
    }
}

fn r6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}
fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---- the plan ------------------------------------------------------------------------------------------------------

/// A part placed on a sheet: its corner (from the sheet's top left, y down) and the room it takes there.
#[derive(Clone, Debug, Serialize)]
pub struct Placed {
    pub number: usize,
    pub id: Id,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// turned a quarter turn (its length runs across the sheet)
    pub rotated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SheetOut {
    pub index: usize,
    pub material: String,
    pub thickness: f64,
    pub size: [f64; 2],
    pub grain: bool,
    pub parts: Vec<Placed>,
    pub waste_pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoardPart {
    pub number: usize,
    pub id: Id,
    pub name: String,
    /// where it starts along the board
    pub x: f64,
    pub length: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoardOut {
    pub index: usize,
    pub material: String,
    /// "2×4", or "T×W mm" for a board that is not dimensional lumber
    pub stock: String,
    pub length: f64,
    /// [thickness, width]
    pub cross: [f64; 2],
    pub parts: Vec<BoardPart>,
    pub waste_pct: f64,
    pub board_feet: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Unplaced {
    pub number: usize,
    pub id: Id,
    pub name: String,
    pub material: String,
    pub size: [f64; 3],
    pub reason: String,
}

/// One part of the model as the plan names it.
#[derive(Clone, Debug, Serialize)]
pub struct PartOut {
    pub number: usize,
    pub id: Id,
    pub name: String,
    pub material: String,
    pub size: [f64; 3],
    /// "sheet" or "board"
    pub kind: &'static str,
    /// the stock it is cut from, "2×4" for lumber
    pub stock: String,
    pub grain: bool,
    /// the sheet or board it is on (1-based), None when it fits nowhere
    pub on: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MaterialCost {
    pub material: String,
    pub sheets: usize,
    pub boards: usize,
    pub board_feet: f64,
    pub cost: f64,
    /// false when the material has no price (its cost is counted as 0)
    pub priced: bool,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub units: String,
    pub stock: CutStock,
    pub parts: Vec<PartOut>,
    pub sheets: Vec<SheetOut>,
    pub boards: Vec<BoardOut>,
    pub unplaced: Vec<Unplaced>,
    pub by_material: Vec<MaterialCost>,
    pub total: f64,
}

#[derive(Clone, Copy)]
struct R {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Item<'a> {
    number: usize,
    p: &'a Measured,
}

/// Parts of one kind, material and (sheets) thickness, or (boards) cross-section.
fn mm(v: f64) -> i64 {
    (v * 1000.0).round() as i64
}

/// A free rectangle chosen for a part: (bin, free rect, score, width, height, turned).
type Fit = (usize, usize, (f64, f64), f64, f64, bool);

/// Pack `items` on sheets of `sheet`: guillotine, best short side fit. Returns each sheet's placements, and the items
/// that fit on no sheet.
fn pack_sheets(items: &[Item], sheet: [f64; 2], trim: f64, kerf: f64, grain: bool) -> (Vec<Vec<Placed>>, Vec<usize>) {
    // every piece and the sheet grow by one kerf, so each cut is paid once and the last piece needs none
    let (bw, bh) = (sheet[0] - 2.0 * trim + kerf, sheet[1] - 2.0 * trim + kerf);
    let mut bins: Vec<(Vec<R>, Vec<Placed>)> = Vec::new();
    let mut unplaced = Vec::new();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (pa, pb) = (items[a].p, items[b].p);
        (pb.size[0] * pb.size[1]).total_cmp(&(pa.size[0] * pa.size[1])).then(pb.size[0].total_cmp(&pa.size[0])).then_with(|| pa.name.cmp(&pb.name)).then_with(|| pa.id.cmp(&pb.id))
    });
    for i in order {
        let p = items[i].p;
        let (l, w) = (p.size[0], p.size[1]);
        // the ways it may lie: length along the sheet, or (no grain) across it
        let mut turns: Vec<(f64, f64, bool)> = vec![(l, w, false)];
        if !grain && (l - w).abs() > EPS {
            turns.push((w, l, true));
        }
        // (bin, free rect, score, width, height, turned)
        let mut best: Option<Fit> = None;
        let offer = |best: &mut Option<Fit>, b: usize, fi: usize, r: &R| {
            for &(pw, ph, rot) in &turns {
                let (iw, ih) = (pw + kerf, ph + kerf);
                if iw <= r.w + EPS && ih <= r.h + EPS {
                    let score = ((r.w - iw).min(r.h - ih), (r.w - iw).max(r.h - ih));
                    if best.as_ref().is_none_or(|bb| score.0.total_cmp(&bb.2 .0).then(score.1.total_cmp(&bb.2 .1)).is_lt()) {
                        *best = Some((b, fi, score, pw, ph, rot));
                    }
                }
            }
        };
        for (b, (free, _)) in bins.iter().enumerate() {
            for (fi, r) in free.iter().enumerate() {
                offer(&mut best, b, fi, r);
            }
        }
        if best.is_none() {
            // a new sheet, if the part fits on an empty one
            let fresh = R { x: 0.0, y: 0.0, w: bw, h: bh };
            offer(&mut best, bins.len(), 0, &fresh);
            if best.is_some() {
                bins.push((vec![fresh], Vec::new()));
            }
        }
        let Some((b, fi, _, pw, ph, rot)) = best else {
            unplaced.push(i);
            continue;
        };
        let r = bins[b].0.remove(fi);
        let (iw, ih) = (pw + kerf, ph + kerf);
        bins[b].1.push(Placed { number: items[i].number, id: p.id, name: p.name.clone(), x: r6(trim + r.x), y: r6(trim + r.y), w: r6(pw), h: r6(ph), rotated: rot });
        // split what is left by the shorter leftover side, so the longer one stays whole
        let (lw, lh) = (r.w - iw, r.h - ih);
        let (right, top) = if lw <= lh {
            (R { x: r.x + iw, y: r.y, w: lw, h: ih }, R { x: r.x, y: r.y + ih, w: r.w, h: lh })
        } else {
            (R { x: r.x + iw, y: r.y, w: lw, h: r.h }, R { x: r.x, y: r.y + ih, w: iw, h: lh })
        };
        for f in [right, top] {
            if f.w > EPS && f.h > EPS {
                bins[b].0.push(f);
            }
        }
    }
    (bins.into_iter().map(|(_, p)| p).collect(), unplaced)
}

/// Pack `items` on boards, longest first, each on the first board with room; a new board is the shortest length sold
/// that holds the part. Returns each board's length and parts, and the items longer than any board.
fn pack_boards(items: &[Item], lengths: &[f64], trim: f64, kerf: f64) -> (Vec<(f64, Vec<BoardPart>)>, Vec<usize>) {
    let mut sold: Vec<f64> = lengths.to_vec();
    sold.sort_by(f64::total_cmp);
    sold.dedup();
    let mut bins: Vec<(f64, f64, Vec<BoardPart>)> = Vec::new();
    let mut unplaced = Vec::new();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (pa, pb) = (items[a].p, items[b].p);
        pb.size[0].total_cmp(&pa.size[0]).then_with(|| pa.name.cmp(&pb.name)).then_with(|| pa.id.cmp(&pb.id))
    });
    for i in order {
        let p = items[i].p;
        let need = p.size[0] + kerf;
        let usable = |len: f64| len - 2.0 * trim + kerf;
        let at = match bins.iter().position(|b| usable(b.0) - b.1 + EPS >= need) {
            Some(k) => k,
            None => match sold.iter().find(|l| usable(**l) + EPS >= need) {
                Some(l) => {
                    bins.push((*l, 0.0, Vec::new()));
                    bins.len() - 1
                }
                None => {
                    unplaced.push(i);
                    continue;
                }
            },
        };
        let b = &mut bins[at];
        b.2.push(BoardPart { number: items[i].number, id: p.id, name: p.name.clone(), x: r6(trim + b.1), length: r6(p.size[0]) });
        b.1 += need;
    }
    (bins.into_iter().map(|(l, _, p)| (l, p)).collect(), unplaced)
}

/// The cut plan for `ids` (groups or components), or every part.
pub fn plan(m: &Model, ids: &[Id], stock: &CutStock) -> Result<Plan> {
    stock.check()?;
    let mut parts = measure(m, ids);
    if parts.len() > MAX_PARTS {
        bail!("{} parts is more than a cut plan takes ({MAX_PARTS}); pick some of them", parts.len());
    }
    crate::parts_layout::sort_parts(&mut parts);
    let units = m.units.clone();
    let mat = |p: &Measured| p.material.clone().unwrap_or_default();
    // sheets by (material, thickness mm); boards by (material, stock name, thickness mm, width mm)
    let mut sheet_groups: BTreeMap<(String, i64), Vec<Item>> = BTreeMap::new();
    let mut board_groups: BTreeMap<(String, String, i64, i64), Vec<Item>> = BTreeMap::new();
    for (i, p) in parts.iter().enumerate() {
        let item = Item { number: i + 1, p };
        if is_sheet(p.size) {
            sheet_groups.entry((mat(p), mm(p.size[2]))).or_default().push(item);
        } else {
            let name = lumber(p.size[2], p.size[1]).map(|l| l.0).unwrap_or_else(|| format!("{}×{} mm", mm(p.size[2]), mm(p.size[1])));
            board_groups.entry((mat(p), name, mm(p.size[2]), mm(p.size[1]))).or_default().push(item);
        }
    }
    let mut sheets: Vec<SheetOut> = Vec::new();
    let mut boards: Vec<BoardOut> = Vec::new();
    let mut unplaced: Vec<Unplaced> = Vec::new();
    let mut on: BTreeMap<usize, usize> = BTreeMap::new();
    let mut note_unplaced = |items: &[Item], idx: &[usize], reason: &str| {
        for &k in idx {
            let p = items[k].p;
            unplaced.push(Unplaced { number: items[k].number, id: p.id, name: p.name.clone(), material: mat(p), size: p.size, reason: reason.into() });
        }
    };
    for ((material, _), items) in &sheet_groups {
        let ms = stock.material(material);
        let size = ms.sheet.unwrap_or(stock.sheet);
        let (placed, un) = pack_sheets(items, size, stock.trim, stock.kerf, ms.grain);
        note_unplaced(items, &un, "bigger than a sheet");
        let thickness = items[0].p.size[2];
        for ps in placed {
            let used: f64 = ps.iter().map(|p| p.w * p.h).sum();
            let index = sheets.len() + 1;
            for p in &ps {
                on.insert(p.number, index);
            }
            sheets.push(SheetOut { index, material: material.clone(), thickness, size, grain: ms.grain, parts: ps, waste_pct: r2(100.0 * (1.0 - used / (size[0] * size[1]))) });
        }
    }
    for ((material, name, _, _), items) in &board_groups {
        let ms = stock.material(material);
        let lengths = stock.lengths_by_size.get(name).or(ms.board_lengths.as_ref()).unwrap_or(&stock.board_lengths);
        let (placed, un) = pack_boards(items, lengths, stock.trim, stock.kerf);
        note_unplaced(items, &un, "longer than any board");
        let p0 = items[0].p;
        let cross = [p0.size[2], p0.size[1]];
        // board feet by the nominal size for dimensional lumber, the actual size for the rest
        let (t_in, w_in) = match lumber(p0.size[2], p0.size[1]) {
            Some((_, nom)) => (nom[0], nom[1]),
            None => (cross[0] / INCH, cross[1] / INCH),
        };
        for (length, ps) in placed {
            let used: f64 = ps.iter().map(|p| p.length).sum();
            let index = boards.len() + 1;
            for p in &ps {
                on.insert(p.number, index);
            }
            boards.push(BoardOut { index, material: material.clone(), stock: name.clone(), length, cross, parts: ps, waste_pct: r2(100.0 * (1.0 - used / length)), board_feet: r6(t_in * w_in * (length / INCH) / 144.0) });
        }
    }
    unplaced.sort_by_key(|u| u.number);
    let part_out: Vec<PartOut> = parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let sheet = is_sheet(p.size);
            let ms = stock.material(&mat(p));
            let name = lumber(p.size[2], p.size[1]).map(|l| l.0).unwrap_or_else(|| format!("{}×{} mm", mm(p.size[2]), mm(p.size[1])));
            PartOut { number: i + 1, id: p.id, name: p.name.clone(), material: mat(p), size: p.size, kind: if sheet { "sheet" } else { "board" }, stock: if sheet { fmt_len(p.size[2], &units) } else { name }, grain: ms.grain || !sheet, on: on.get(&(i + 1)).copied() }
        })
        .collect();
    // what it costs, per material
    let mut names: Vec<String> = sheets.iter().map(|s| s.material.clone()).chain(boards.iter().map(|b| b.material.clone())).collect();
    names.sort();
    names.dedup();
    let mut by_material = Vec::new();
    let mut total = 0.0;
    for name in names {
        let ms = stock.material(&name);
        let ns = sheets.iter().filter(|s| s.material == name).count();
        let bs: Vec<&BoardOut> = boards.iter().filter(|b| b.material == name).collect();
        let bf: f64 = bs.iter().map(|b| b.board_feet).sum();
        let boards_cost = if ms.board_foot_price > 0.0 { bf * ms.board_foot_price } else { bs.len() as f64 * ms.board_price };
        let cost = r2(ns as f64 * ms.sheet_price + boards_cost);
        let priced = (ns == 0 || ms.sheet_price > 0.0) && (bs.is_empty() || ms.board_foot_price > 0.0 || ms.board_price > 0.0);
        total += cost;
        by_material.push(MaterialCost { material: name, sheets: ns, boards: bs.len(), board_feet: r2(bf), cost, priced });
    }
    Ok(Plan { units, stock: stock.clone(), parts: part_out, sheets, boards, unplaced, by_material, total: r2(total) })
}

impl Plan {
    fn imperial(&self) -> bool {
        matches!(self.units.as_str(), "ft" | "in")
    }

    /// Waste over everything bought, a percent: the stock's area (sheets) and length (boards) that is not a part.
    pub fn waste_pct(&self) -> f64 {
        let mut bought = 0.0;
        let mut used = 0.0;
        for s in &self.sheets {
            bought += s.size[0] * s.size[1];
            used += s.parts.iter().map(|p| p.w * p.h).sum::<f64>();
        }
        for b in &self.boards {
            let a = b.length * b.cross[1];
            bought += a;
            used += b.parts.iter().map(|p| p.length).sum::<f64>() * b.cross[1];
        }
        if bought <= 0.0 {
            0.0
        } else {
            r2(100.0 * (1.0 - used / bought))
        }
    }

    /// The labels: one per part, in part-number order.
    pub fn labels(&self) -> Vec<Value> {
        let u = &self.units;
        self.parts
            .iter()
            .map(|p| {
                let place = match (p.on, p.kind) {
                    (Some(n), "sheet") => format!("Sheet {n} of {}", self.sheets.len()),
                    (Some(n), _) => format!("Board {n} of {}", self.boards.len()),
                    (None, _) => "not placed".to_string(),
                };
                json!({
                    "number": p.number, "id": p.id, "name": p.name, "material": p.material, "stock": p.stock, "grain": p.grain, "on": p.on,
                    "size": p.size, "size_text": format!("{} × {} × {}", fmt_len(p.size[0], u), fmt_len(p.size[1], u), fmt_len(p.size[2], u)), "place": place,
                })
            })
            .collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "units": self.units,
            "kerf": self.stock.kerf, "trim": self.stock.trim,
            "parts": self.parts, "sheets": self.sheets, "boards": self.boards, "unplaced": self.unplaced,
            "summary": {"parts": self.parts.len(), "sheets": self.sheets.len(), "boards": self.boards.len(), "unplaced": self.unplaced.len(), "waste_pct": self.waste_pct()},
            "cost": {"currency": self.stock.currency, "total": self.total, "by_material": self.by_material},
            "stock": self.stock,
        })
    }
}

// ---- drawing -------------------------------------------------------------------------------------------------------

const MARGIN: f64 = 10.0;

fn pen(w: f64, rgb: [u8; 3]) -> Pen {
    Pen::new(w, rgb)
}

fn rect(x: f64, y: f64, w: f64, h: f64, stroke: Option<Pen>, fill: Option<[u8; 3]>) -> Prim {
    Prim::Path(PathPrim { segs: vec![Seg::M([x, y]), Seg::L([x + w, y]), Seg::L([x + w, y + h]), Seg::L([x, y + h]), Seg::Z], pen: stroke, fill: fill.map(|rgb| Fill { rgb, even_odd: false }), prov: None })
}

fn line(a: [f64; 2], b: [f64; 2], p: Pen) -> Prim {
    Prim::Path(PathPrim { segs: vec![Seg::M(a), Seg::L(b)], pen: Some(p), fill: None, prov: None })
}

fn text(x: f64, y: f64, size: f64, bold: bool, s: &str) -> Prim {
    Prim::Text(TextPrim { at: [x, y], size, bold, rgb: BLACK, angle: 0.0, text: s.to_string(), role: Role::Paragraph })
}

/// `s` cut short with an ellipsis until it is at most `max` wide.
fn fit(s: &str, size: f64, bold: bool, max: f64) -> String {
    if font::width(s, size, bold) <= max {
        return s.to_string();
    }
    let mut v: Vec<char> = s.chars().collect();
    while !v.is_empty() {
        v.pop();
        let t: String = v.iter().collect::<String>() + "…";
        if font::width(&t, size, bold) <= max {
            return t;
        }
    }
    String::new()
}

/// Text centered on a box, as big as fits (at most `size`), or nothing when it would be unreadably small.
fn centered(items: &mut Vec<Prim>, bx: [f64; 4], size: f64, bold: bool, s: &str, dy: f64) {
    let mut sz = size;
    while sz >= 1.6 && font::width(s, sz, bold) > bx[2] - 1.0 {
        sz -= 0.2;
    }
    if sz < 1.6 || sz > bx[3] - 0.5 {
        return;
    }
    let w = font::width(s, sz, bold);
    items.push(text(bx[0] + (bx[2] - w) / 2.0, bx[1] + bx[3] / 2.0 + sz * 0.35 + dy, sz, bold, s));
}

const TINTS: [[u8; 3]; 4] = [[236, 222, 196], [222, 232, 210], [210, 224, 236], [238, 214, 214]];

impl Plan {
    fn paper(&self, paper: Option<&str>) -> Result<(f64, f64)> {
        Ok(match paper.map(str::to_ascii_lowercase).as_deref() {
            Some("a4") => (297.0, 210.0),
            Some("letter") => (279.4, 215.9),
            None => {
                if self.imperial() {
                    (279.4, 215.9)
                } else {
                    (297.0, 210.0)
                }
            }
            Some(o) => bail!("paper is \"a4\" or \"letter\" (got {o:?})"),
        })
    }

    fn name_of(m: &str) -> String {
        if m.is_empty() {
            "No material".into()
        } else {
            m.to_string()
        }
    }

    /// The diagrams, one page each: a summary, then every sheet, then boards (as many to a page as fit).
    pub fn diagram_pages(&self, paper: Option<&str>) -> Result<Vec<Page>> {
        let (pw, ph) = self.paper(paper)?;
        let u = &self.units;
        let mut pages = Vec::new();
        // the summary
        let mut p = Page::new(pw, ph);
        p.title = "Cutting diagrams".into();
        let mut y = MARGIN + 6.0;
        p.items.push(text(MARGIN, y, 7.0, true, "Cutting plan"));
        y += 9.0;
        let s = &self.stock;
        let lines = [
            format!("{} parts: {} sheet{}, {} board{}, waste {}%", self.parts.len(), self.sheets.len(), if self.sheets.len() == 1 { "" } else { "s" }, self.boards.len(), if self.boards.len() == 1 { "" } else { "s" }, self.waste_pct()),
            format!("Saw kerf {} · trim {}", fmt_len(s.kerf, u), fmt_len(s.trim, u)),
        ];
        for l in lines {
            p.items.push(text(MARGIN, y, 4.0, false, &l));
            y += 6.0;
        }
        y += 3.0;
        p.items.push(text(MARGIN, y, 4.0, true, "Material"));
        for (x, h) in [(110.0, "Sheets"), (140.0, "Boards"), (170.0, "Board ft"), (205.0, "Cost")] {
            p.items.push(text(x, y, 4.0, true, h));
        }
        y += 6.0;
        for c in &self.by_material {
            if y > ph - MARGIN - 20.0 {
                break;
            }
            p.items.push(text(MARGIN, y, 3.8, false, &fit(&Self::name_of(&c.material), 3.8, false, 95.0)));
            p.items.push(text(110.0, y, 3.8, false, &c.sheets.to_string()));
            p.items.push(text(140.0, y, 3.8, false, &c.boards.to_string()));
            p.items.push(text(170.0, y, 3.8, false, &format!("{:.2}", c.board_feet)));
            p.items.push(text(205.0, y, 3.8, false, &if c.priced { format!("{}{:.2}", s.currency, c.cost) } else { "no price".into() }));
            y += 5.5;
        }
        y += 3.0;
        p.items.push(text(MARGIN, y, 4.5, true, &format!("Total {}{:.2}", s.currency, self.total)));
        y += 8.0;
        if !self.unplaced.is_empty() {
            p.items.push(text(MARGIN, y, 4.0, true, &format!("{} part{} fit on no stock:", self.unplaced.len(), if self.unplaced.len() == 1 { "" } else { "s" })));
            y += 5.5;
            for un in self.unplaced.iter().take(8) {
                p.items.push(text(MARGIN, y, 3.6, false, &format!("#{} {} ({})", un.number, un.name, un.reason)));
                y += 5.0;
            }
        }
        pages.push(p);
        // the sheets
        for sh in &self.sheets {
            let mut p = Page::new(pw, ph);
            p.title = format!("Sheet {}", sh.index);
            let head = format!("Sheet {} of {}: {} {}", sh.index, self.sheets.len(), Self::name_of(&sh.material), fmt_len(sh.thickness, u));
            p.items.push(text(MARGIN, MARGIN + 5.0, 5.5, true, &head));
            let sub = format!("{} × {} · {} parts · waste {}%{}", fmt_len(sh.size[0], u), fmt_len(sh.size[1], u), sh.parts.len(), sh.waste_pct, if sh.grain { " · grain runs along the long side" } else { "" });
            p.items.push(text(MARGIN, MARGIN + 11.0, 3.6, false, &sub));
            let (ax, ay, aw, ah) = (MARGIN, MARGIN + 16.0, pw - 2.0 * MARGIN, ph - 2.0 * MARGIN - 16.0);
            let k = (aw / sh.size[0]).min(ah / sh.size[1]);
            p.items.push(rect(ax, ay, sh.size[0] * k, sh.size[1] * k, Some(pen(0.5, BLACK)), Some([250, 247, 240])));
            for (i, q) in sh.parts.iter().enumerate() {
                let b = [ax + q.x * k, ay + q.y * k, q.w * k, q.h * k];
                p.items.push(rect(b[0], b[1], b[2], b[3], Some(pen(0.25, BLACK)), Some(TINTS[i % TINTS.len()])));
                centered(&mut p.items, b, 6.0, true, &format!("#{}", q.number), -2.0);
                centered(&mut p.items, b, 3.2, false, &q.name, 1.5);
                centered(&mut p.items, b, 2.8, false, &format!("{} × {}", fmt_len(q.w, u), fmt_len(q.h, u)), 5.0);
            }
            pages.push(p);
        }
        // the boards, as many to a page as fit
        let pitch = 17.0;
        let per = (((ph - 2.0 * MARGIN - 12.0) / pitch).floor() as usize).max(1);
        let longest = self.boards.iter().map(|b| b.length).fold(0.0, f64::max);
        for (pi, chunk) in self.boards.chunks(per).enumerate() {
            let mut p = Page::new(pw, ph);
            p.title = format!("Boards {}", pi + 1);
            p.items.push(text(MARGIN, MARGIN + 5.0, 5.5, true, &format!("Boards, page {} of {}", pi + 1, self.boards.len().div_ceil(per))));
            let k = (pw - 2.0 * MARGIN) / longest;
            for (r, b) in chunk.iter().enumerate() {
                let y0 = MARGIN + 12.0 + r as f64 * pitch;
                let head = format!("Board {} of {}: {} {} · {} · waste {}%", b.index, self.boards.len(), b.stock, Self::name_of(&b.material), fmt_len(b.length, u), b.waste_pct);
                p.items.push(text(MARGIN, y0 + 3.0, 3.6, true, &head));
                p.items.push(rect(MARGIN, y0 + 4.5, b.length * k, 9.0, Some(pen(0.4, BLACK)), Some([250, 247, 240])));
                for (i, q) in b.parts.iter().enumerate() {
                    let bx = [MARGIN + q.x * k, y0 + 4.5, q.length * k, 9.0];
                    p.items.push(rect(bx[0], bx[1], bx[2], bx[3], Some(pen(0.25, BLACK)), Some(TINTS[i % TINTS.len()])));
                    centered(&mut p.items, bx, 4.0, true, &format!("#{} {} · {}", q.number, q.name, fmt_len(q.length, u)), 0.0);
                }
            }
            pages.push(p);
        }
        Ok(pages)
    }

    /// The diagrams as a PDF.
    pub fn diagrams_pdf(&self, paper: Option<&str>) -> Result<Vec<u8>> {
        pdf::write(&Doc { title: "Cutting diagrams".into(), lang: String::new(), pages: self.diagram_pages(paper)? })
    }

    /// The diagrams as one SVG: the pages one under another.
    pub fn diagrams_svg(&self, paper: Option<&str>) -> Result<String> {
        let pages = self.diagram_pages(paper)?;
        let mut all = Page::new(pages[0].w, pages.iter().map(|p| p.h).sum());
        all.title = "Cutting diagrams".into();
        let mut dy = 0.0;
        for p in pages {
            for it in p.items {
                all.items.push(shift(it, dy));
            }
            dy += p.h;
            // a rule between pages
            all.items.push(line([0.0, dy], [p.w, dy], pen(0.2, [180, 180, 180])));
        }
        Ok(svg::page(&all))
    }

    /// The label sheets as a PDF: a grid of labels, one per part.
    pub fn labels_pdf(&self, paper: Option<&str>) -> Result<Vec<u8>> {
        let (w, h) = self.paper(paper)?;
        // portrait
        let (pw, ph) = (w.min(h), w.max(h));
        let (lw, lh) = (63.5, 38.1);
        let cols = (((pw - 10.0) / lw).floor() as usize).max(1);
        let rows = (((ph - 20.0) / lh).floor() as usize).max(1);
        let (x0, y0) = ((pw - cols as f64 * lw) / 2.0, (ph - rows as f64 * lh) / 2.0);
        let labels = self.labels();
        let mut pages = Vec::new();
        if labels.is_empty() {
            bail!("there are no parts to label");
        }
        for chunk in labels.chunks(cols * rows) {
            let mut p = Page::new(pw, ph);
            p.title = "Part labels".into();
            for (i, l) in chunk.iter().enumerate() {
                let (c, r) = (i % cols, i / cols);
                let (x, y) = (x0 + c as f64 * lw, y0 + r as f64 * lh);
                let s = |k: &str| l[k].as_str().unwrap_or_default().to_string();
                p.items.push(rect(x, y, lw, lh, Some(pen(0.15, [150, 150, 150])), None));
                let num = format!("#{}", l["number"]);
                p.items.push(text(x + 3.0, y + 10.0, 9.0, true, &num));
                let nx = x + 4.0 + font::width(&num, 9.0, true);
                p.items.push(text(nx, y + 9.0, 4.2, true, &fit(&s("name"), 4.2, true, x + lw - 3.0 - nx)));
                p.items.push(text(x + 3.0, y + 17.0, 4.0, false, &fit(&s("size_text"), 4.0, false, lw - 6.0)));
                let mat = if s("material").is_empty() { s("stock") } else { format!("{} · {}", s("material"), s("stock")) };
                p.items.push(text(x + 3.0, y + 22.5, 3.4, false, &fit(&mat, 3.4, false, lw - 6.0)));
                p.items.push(text(x + 3.0, y + 27.5, 3.4, false, &fit(&s("place"), 3.4, false, lw - 6.0)));
                // the grain arrow runs along the part's length
                let ay = y + lh - 4.0;
                if l["grain"] == true {
                    let (a, b) = (x + 4.0, x + lw - 4.0);
                    let pn = pen(0.35, BLACK);
                    p.items.push(line([a, ay], [b, ay], pn.clone()));
                    for (tip, dir) in [(a, 1.0), (b, -1.0)] {
                        p.items.push(line([tip, ay], [tip + dir * 2.0, ay - 1.0], pn.clone()));
                        p.items.push(line([tip, ay], [tip + dir * 2.0, ay + 1.0], pn.clone()));
                    }
                    p.items.push(text(x + lw / 2.0 - font::width("grain", 2.6, false) / 2.0, ay - 1.5, 2.6, false, "grain"));
                } else {
                    p.items.push(text(x + 3.0, ay + 1.0, 2.8, false, "no grain direction"));
                }
            }
            pages.push(p);
        }
        pdf::write(&Doc { title: "Part labels".into(), lang: String::new(), pages })
    }
}

fn shift(p: Prim, dy: f64) -> Prim {
    match p {
        Prim::Path(mut q) => {
            for s in &mut q.segs {
                *s = match *s {
                    Seg::M(a) => Seg::M([a[0], a[1] + dy]),
                    Seg::L(a) => Seg::L([a[0], a[1] + dy]),
                    Seg::C(a, b, c) => Seg::C([a[0], a[1] + dy], [b[0], b[1] + dy], [c[0], c[1] + dy]),
                    Seg::Z => Seg::Z,
                };
            }
            Prim::Path(q)
        }
        Prim::Text(mut t) => {
            t.at[1] += dy;
            Prim::Text(t)
        }
        other => other,
    }
}
