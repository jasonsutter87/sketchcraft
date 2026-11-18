//! Drawing sheets as data: what a `.sketchcraft` file holds for paper (design step 1, `docs/dev/drafting-design.md` §1).
//!
//! Sheets live in the model file. A sheet is a small record (`Sheet`); everything ON a sheet is a flat item in
//! `Model::sheet_items`, keyed by its own id, so undo patches work per item (an agent's chain dimension and the
//! person's drag of another item on the same page never clash). Viewports keep their own anchor, scale, rotation
//! and crop: nothing is re-fitted after creation.
//!
//! Forward compatibility, from the start: every struct here keeps fields it doesn't know (`extra`), and an item
//! whose kind this build doesn't know (or whose fields don't fit) is held as raw JSON (`SheetItem::Raw`) and written
//! back as it came. Nothing in this file renders anything; that is the next step.

use crate::model::{sane, Id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Extra fields a newer build wrote that this one doesn't know: kept, and written back.
pub type Extra = BTreeMap<String, Value>;

fn is_zero(z: &i32) -> bool {
    *z == 0
}
fn one() -> f64 {
    1.0
}
fn is_one(x: &f64) -> bool {
    *x == 1.0
}
fn hundred() -> f64 {
    100.0
}
fn pen_w() -> f64 {
    0.25
}
fn black() -> String {
    "#000000".into()
}

// ---- paper ---------------------------------------------------------------------------------------------------

/// A paper size in millimetres, landscape (`w_mm` is the long side unless the sheet says otherwise).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paper {
    pub w_mm: f64,
    pub h_mm: f64,
    /// "A3", "Letter", or "" for a custom size
    pub name: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Paper {
    fn default() -> Self {
        Paper { w_mm: 420.0, h_mm: 297.0, name: "A3".into(), extra: Extra::new() }
    }
}

impl Paper {
    /// A named size the sheet export knows (see `layout::sheet_mm`), or a custom one from millimetres.
    pub fn named(name: &str) -> anyhow::Result<Paper> {
        let (w, h) = crate::layout::sheet_mm(name)?;
        let pretty = match name.to_ascii_lowercase().as_str() {
            "letter" => "Letter".to_string(),
            "tabloid" | "11x17" => "Tabloid".to_string(),
            other => other.to_ascii_uppercase(),
        };
        Ok(Paper { w_mm: w, h_mm: h, name: pretty, extra: Extra::new() })
    }
}

/// One revision recorded when a sheet was issued (the commands that make these are the person's; see the design).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Issue {
    pub rev: String,
    pub date: String,
    pub note: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A drawing sheet: a page record. What is drawn on it is in `Model::sheet_items` (items name their sheet).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sheet {
    pub name: String,
    /// "A-101"
    pub number: String,
    pub paper: Paper,
    /// position in the set (lower first; ties by id)
    pub order: i32,
    /// a title block this sheet shows (an id in `Model::title_blocks`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_block: Option<Id>,
    /// text for the title block's fields (project, date, drawn by...)
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issued: Vec<Issue>,
    #[serde(flatten)]
    pub extra: Extra,
}

// ---- anchors (data only: resolving them is a later step) -----------------------------------------------------

/// What an anchor points at, by id (vertices, never edges: an edge's midpoint is `Between` its two vertices).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorAt {
    Vertex(Id),
    Between { a: Id, b: Id, t: f64, #[serde(flatten)] extra: Extra },
    CurveCenter(Id),
    Guide(Id),
}

/// A point on the model a paper item is attached to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// instance ids from the root down (empty: the root model)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<Id>,
    pub at: AnchorAt,
    /// the last good world point, for showing an anchor that broke
    #[serde(default)]
    pub last: [f64; 3],
    #[serde(flatten)]
    pub extra: Extra,
}

/// A value of a type this build knows, or the JSON it came as when it doesn't fit (a newer build's value for a field,
/// an enum value this build has no name for): kept as it is, written back unchanged, and never fails the file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Keep<T> {
    Known(T),
    Raw(Value),
}

// ---- styles --------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    #[default]
    Paper,
    Model,
}

/// How a hatch is drawn: a named pattern (`Drafting::patterns`, or a built-in) with its colour, weight and spacing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HatchUse {
    pub pattern: String,
    pub color: String,
    pub weight_mm: f64,
    pub scale: f64,
    /// degrees
    pub angle: f64,
    pub scale_mode: ScaleMode,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for HatchUse {
    fn default() -> Self {
        HatchUse { pattern: "diagonal".into(), color: black(), weight_mm: 0.13, scale: 1.0, angle: 0.0, scale_mode: ScaleMode::Paper, extra: Extra::new() }
    }
}

/// What fills a tag's cut surfaces in plans and sections (design §3.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutFill {
    /// not set: the drawing's own poché (dark solid on paper, pale on a blueprint), as before cut styles
    #[default]
    Auto,
    /// nothing: the cut is left blank (the paper colour, which hides what is beyond it), with its outline
    None,
    /// a solid colour (`CutStyle::color`, or the drawing's poché colour when it has none)
    Solid,
    /// the tag's cut hatch (`CutStyle::hatch`) over the paper colour
    Hatch,
}

impl CutFill {
    pub fn name(self) -> &'static str {
        match self {
            CutFill::Auto => "auto",
            CutFill::None => "none",
            CutFill::Solid => "solid",
            CutFill::Hatch => "hatch",
        }
    }
}

/// The colour of a tag's solid cut fill when it has none of its own (the drawing's poché on paper).
pub const CUT_SOLID: &str = "#292b33";

/// How a tag's cut surface (the poché of a plan or section) is drawn.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CutStyle {
    pub fill: CutFill,
    /// the cut outline's pen in paper mm (0: the drawing's heavy pen)
    pub outline_mm: f64,
    /// a `Solid` fill's colour, "#rrggbb" ("": the drawing's poché colour)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hatch: Option<HatchUse>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Line work beyond the cut (what is seen past the section plane).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Beyond {
    pub weight_mm: f64,
    pub color: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// What lines of a tag look like on paper: weights are pen widths in paper millimetres.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagStyle {
    pub weight_mm: f64,
    /// a line type name (`Drafting::line_types`), "" for continuous
    pub line: String,
    pub color: String,
    pub cut: CutStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_hatch: Option<HatchUse>,
    pub beyond: Beyond,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One edge's own line weight, style and colour: it beats its tag's style (`Edge::draft`). `None` fields fall through
/// to the tag, then to the default pen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeDraft {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_mm: Option<f64>,
    /// a line style name: solid, dashed, hidden, center, or one in `Drafting::line_types`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl EdgeDraft {
    pub fn is_empty(&self) -> bool {
        self.weight_mm.is_none() && self.line.is_none() && self.color.is_none() && self.extra.is_empty()
    }
}

/// The built-in line styles, as dash patterns in paper millimetres (on, off, ...; empty is solid).
pub const LINE_STYLES: [&str; 4] = ["solid", "dashed", "hidden", "center"];

/// The narrowest and widest pen a command accepts, in paper millimetres (ISO 128 runs 0.13 to 2.0).
pub const WEIGHT_RANGE: (f64, f64) = (0.05, 2.0);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineType {
    /// dash, gap, dash, gap... in paper millimetres (empty: continuous)
    pub dash_mm: Vec<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DimStyle {
    pub arrow: String,
    pub text_style: String,
    pub units: String,
    pub precision: u8,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub font: String,
    pub height_mm: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Drafting styles: small, rarely changing, kept whole (it is part of the model's "skeleton" in `history`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Drafting {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_styles: BTreeMap<String, Keep<TagStyle>>,
    /// hatch patterns kept in the file (parsed `.pat` text by name), so the file is self-contained
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub patterns: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub line_types: BTreeMap<String, Keep<LineType>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub dim_styles: BTreeMap<String, Keep<DimStyle>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub text_styles: BTreeMap<String, Keep<TextStyle>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Drafting {
    pub fn is_empty(&self) -> bool {
        *self == Drafting::default()
    }

    /// The dash pattern a line style name means: the file's own `line_types` first, then the built-ins. None: no such
    /// style.
    pub fn dash_of(&self, name: &str) -> Option<Vec<f64>> {
        if let Some(Keep::Known(l)) = self.line_types.get(name) {
            return Some(l.dash_mm.clone());
        }
        match name {
            "solid" | "continuous" => Some(vec![]),
            "dashed" => Some(vec![3.0, 1.5]),
            "hidden" => Some(vec![2.0, 1.0]),
            "center" => Some(vec![8.0, 1.5, 1.5, 1.5]),
            _ => None,
        }
    }

    /// The hatch a tag's cut surfaces get (`cut.fill` is `Hatch` and a hatch is set), if any.
    pub fn cut_hatch(&self, tag: Option<&str>) -> Option<&HatchUse> {
        let t = self.tag_style(tag?)?;
        (t.cut.fill == CutFill::Hatch).then_some(t.cut.hatch.as_ref()).flatten()
    }

    /// How a tag's cut surfaces are filled: `Auto` for no tag or no style, and `Auto` for a `Hatch` with no hatch set.
    pub fn cut_fill(&self, tag: Option<&str>) -> CutFill {
        match tag.and_then(|t| self.tag_style(t)) {
            Some(t) if t.cut.fill == CutFill::Hatch && t.cut.hatch.is_none() => CutFill::Auto,
            Some(t) => t.cut.fill,
            None => CutFill::Auto,
        }
    }

    /// The style of a tag, when it has one this build understands.
    pub fn tag_style(&self, tag: &str) -> Option<&TagStyle> {
        match self.tag_styles.get(tag) {
            Some(Keep::Known(t)) => Some(t),
            _ => None,
        }
    }
}

// ---- title blocks --------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleField {
    /// the key in `Sheet::fields` this fills
    pub key: String,
    pub label: String,
    pub at_mm: [f64; 2],
    pub height_mm: f64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A reusable title block: a frame of lines and labelled fields, in paper millimetres from its own corner.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleBlock {
    pub name: String,
    pub size_mm: [f64; 2],
    pub lines_mm: Vec<[[f64; 2]; 2]>,
    pub fields: Vec<TitleField>,
    #[serde(flatten)]
    pub extra: Extra,
}

// ---- items ---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    #[default]
    Vector,
    Raster,
    Hybrid,
}

/// How one tag looks inside one viewport (over its `TagStyle`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// What a viewport shows. (The item's own tag is `kind`, so this one is `type`.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewKind {
    /// a floor plan: the model cut at `cut_z`, with what is between `floor_z` and the cut in thin lines
    Plan { cut_z: f64, floor_z: f64, #[serde(default)] depth: f64, #[serde(flatten)] extra: Extra },
    /// a side view, parallel: `dir` is the horizontal direction from the model toward the viewer (the side you
    /// look at: a front elevation is [0, -1, 0], as the camera of the "front" standard view)
    Elevation { dir: [f64; 3], #[serde(default)] depth: f64, #[serde(flatten)] extra: Extra },
    /// the model cut by the vertical plane through `origin` with `normal`; `normal` is the direction looked along (the viewer is on the side it points away from)
    Section { origin: [f64; 3], normal: [f64; 3], #[serde(default)] depth: f64, #[serde(flatten)] extra: Extra },
    View3d { eye: [f64; 3], target: [f64; 3], fov: f64, #[serde(default)] parallel: bool, #[serde(flatten)] extra: Extra },
}

/// A window on the model. It keeps its own anchor, scale, rotation and crop: nothing re-fits it when the model changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Viewport {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    /// x, y (from the sheet's top left), width, height, in paper millimetres
    pub rect_mm: [f64; 4],
    /// an outline (paper mm, from the rect's top left) the view is cut to; none: the whole rect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crop: Option<Vec<[f64; 2]>>,
    /// the model point pinned to the centre of the rect
    pub anchor: [f64; 3],
    /// degrees, counter-clockwise on paper
    #[serde(skip_serializing_if = "is_zero_f")]
    pub rotation: f64,
    /// the N of 1:N (1 paper mm is N model mm); a positive number
    pub scale: f64,
    pub view: ViewKind,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, TagOverride>,
    #[serde(skip_serializing_if = "is_one")]
    pub opacity: f64,
    pub render: RenderMode,
    /// a saved view (scene) this viewport follows for tag visibility and hidden entities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// a caption under the rect (empty: none)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// who made it: "agent", "plugin ...", or empty for the person
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn is_zero_f(x: &f64) -> bool {
    *x == 0.0
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            sheet: 0,
            z: 0,
            rect_mm: [0.0; 4],
            crop: None,
            anchor: [0.0; 3],
            rotation: 0.0,
            scale: hundred(),
            view: ViewKind::Plan { cut_z: 1.2, floor_z: 0.0, depth: 0.0, extra: Extra::new() },
            tags: BTreeMap::new(),
            opacity: one(),
            render: RenderMode::Vector,
            scene: None,
            label: String::new(),
            made_by: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimMode {
    #[default]
    Aligned,
    Horizontal,
    Vertical,
    Baseline,
}

/// A dimension attached to the model: two anchors make one, more make a chain. The value is never stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Dimension {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub anchors: Vec<Anchor>,
    pub mode: DimMode,
    /// the viewport whose drawing plane the value is measured in (none: the topmost viewport on the sheet that holds it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Id>,
    pub offset_mm: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_override: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Text {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub at_mm: [f64; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_mm: Option<[f64; 2]>,
    pub paragraphs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pen {
    pub w_mm: f64,
    pub color: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dash_mm: Vec<f64>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Pen {
    fn default() -> Self {
        Pen { w_mm: pen_w(), color: black(), dash_mm: vec![], extra: Extra::new() }
    }
}

/// Paper line work: a line, polyline, arc (three points) or rectangle (two corners).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shape {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub pts_mm: Vec<[f64; 2]>,
    pub pen: Pen,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hatch {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub ring_mm: Vec<[f64; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<[f64; 2]>>,
    pub hatch: HatchUse,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Table {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub at_mm: [f64; 2],
    pub cols_mm: Vec<f64>,
    /// rows of cell text (the first row is the header)
    pub rows: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub cell_styles: BTreeMap<String, String>,
    /// what feeds it ("doors", "windows"...): empty for a typed one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The sheet's title block, placed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleBlockRef {
    pub sheet: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub z: i32,
    pub title_block: Id,
    pub at_mm: [f64; 2],
    #[serde(flatten)]
    pub extra: Extra,
}

/// The kinds of thing on a sheet this build knows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemKind {
    Viewport(Viewport),
    Dimension(Dimension),
    Text(Text),
    Line(Shape),
    Polyline(Shape),
    Arc(Shape),
    Rect(Shape),
    Hatch(Hatch),
    Table(Table),
    TitleBlockRef(TitleBlockRef),
}

/// One thing on a sheet. A kind this build doesn't know (or a known one whose fields don't fit) is kept as the JSON
/// it came as, and written back unchanged: opening and saving a newer file here loses nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SheetItem {
    Known(ItemKind),
    Raw(Value),
}

impl ItemKind {
    pub fn sheet(&self) -> Id {
        match self {
            ItemKind::Viewport(v) => v.sheet,
            ItemKind::Dimension(v) => v.sheet,
            ItemKind::Text(v) => v.sheet,
            ItemKind::Line(v) | ItemKind::Polyline(v) | ItemKind::Arc(v) | ItemKind::Rect(v) => v.sheet,
            ItemKind::Hatch(v) => v.sheet,
            ItemKind::Table(v) => v.sheet,
            ItemKind::TitleBlockRef(v) => v.sheet,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ItemKind::Viewport(_) => "viewport",
            ItemKind::Dimension(_) => "dimension",
            ItemKind::Text(_) => "text",
            ItemKind::Line(_) => "line",
            ItemKind::Polyline(_) => "polyline",
            ItemKind::Arc(_) => "arc",
            ItemKind::Rect(_) => "rect",
            ItemKind::Hatch(_) => "hatch",
            ItemKind::Table(_) => "table",
            ItemKind::TitleBlockRef(_) => "title_block_ref",
        }
    }

    /// Every number on the item (paper millimetres, scales, model points in anchors) is finite and sane.
    pub fn sane(&self) -> bool {
        let p2 = |p: &[f64; 2]| p.iter().all(|&x| sane(x));
        let p3 = |p: &[f64; 3]| p.iter().all(|&x| sane(x));
        let anchors_ok = |a: &Anchor| {
            p3(&a.last)
                && match &a.at {
                    AnchorAt::Between { t, .. } => sane(*t),
                    _ => true,
                }
        };
        match self {
            ItemKind::Viewport(v) => {
                v.rect_mm.iter().all(|&x| sane(x))
                    && p3(&v.anchor)
                    && sane(v.rotation)
                    && sane(v.scale)
                    && v.scale > 0.0
                    && sane(v.opacity)
                    && v.crop.as_ref().is_none_or(|c| c.iter().all(p2))
                    && v.tags.values().all(|t| t.weight_mm.is_none_or(sane))
                    && match &v.view {
                        ViewKind::Plan { cut_z, floor_z, depth, .. } => sane(*cut_z) && sane(*floor_z) && sane(*depth),
                        ViewKind::Elevation { dir, depth, .. } => p3(dir) && sane(*depth),
                        ViewKind::Section { origin, normal, depth, .. } => p3(origin) && p3(normal) && sane(*depth),
                        ViewKind::View3d { eye, target, fov, .. } => p3(eye) && p3(target) && sane(*fov),
                    }
            }
            ItemKind::Dimension(d) => sane(d.offset_mm) && d.anchors.iter().all(anchors_ok),
            ItemKind::Text(t) => p2(&t.at_mm) && t.box_mm.as_ref().is_none_or(p2) && t.leader.as_ref().is_none_or(p2),
            ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s) => s.pts_mm.iter().all(p2) && sane(s.pen.w_mm) && s.pen.dash_mm.iter().all(|&x| sane(x)),
            ItemKind::Hatch(h) => h.ring_mm.iter().all(p2) && h.holes.iter().flatten().all(p2) && sane(h.hatch.weight_mm) && sane(h.hatch.scale) && sane(h.hatch.angle),
            ItemKind::Table(t) => p2(&t.at_mm) && t.cols_mm.iter().all(|&x| sane(x)),
            ItemKind::TitleBlockRef(t) => p2(&t.at_mm),
        }
    }

    /// Characters of free text, for the text budget.
    pub fn text_bytes(&self) -> usize {
        let extra = |e: &Extra| e.iter().map(|(k, v)| k.len() + v.to_string().len()).sum::<usize>();
        match self {
            ItemKind::Viewport(v) => v.scene.as_ref().map_or(0, String::len) + v.label.len() + v.made_by.len() + v.tags.keys().map(|k| k.len() + 16).sum::<usize>() + extra(&v.extra),
            ItemKind::Dimension(d) => d.text_override.as_ref().map_or(0, String::len) + d.style.as_ref().map_or(0, String::len) + d.made_by.len() + extra(&d.extra),
            ItemKind::Text(t) => t.paragraphs.iter().map(String::len).sum::<usize>() + t.style.as_ref().map_or(0, String::len) + t.made_by.len() + extra(&t.extra),
            ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s) => s.pen.color.len() + s.made_by.len() + extra(&s.extra),
            ItemKind::Hatch(h) => h.hatch.pattern.len() + h.made_by.len() + extra(&h.extra),
            ItemKind::Table(t) => t.rows.iter().flatten().map(String::len).sum::<usize>() + t.cell_styles.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() + t.source.as_ref().map_or(0, String::len) + t.made_by.len() + extra(&t.extra),
            ItemKind::TitleBlockRef(t) => extra(&t.extra),
        }
    }
}

impl SheetItem {
    /// The sheet the item says it is on, if it says (a raw item might not).
    pub fn sheet(&self) -> Option<Id> {
        match self {
            SheetItem::Known(k) => Some(k.sheet()),
            SheetItem::Raw(v) => v.get("sheet").and_then(Value::as_u64).and_then(|n| Id::try_from(n).ok()),
        }
    }

    /// "viewport", "dimension"... or the kind a newer build wrote ("?" if there is none).
    pub fn kind_name(&self) -> String {
        match self {
            SheetItem::Known(k) => k.name().to_string(),
            SheetItem::Raw(v) => v.get("kind").and_then(Value::as_str).unwrap_or("?").to_string(),
        }
    }

    /// A kind this build has no code for (it is kept, shown as an "unsupported item", and never changed).
    pub fn is_raw(&self) -> bool {
        matches!(self, SheetItem::Raw(_))
    }

    pub fn sane(&self) -> bool {
        match self {
            SheetItem::Known(k) => k.sane(),
            SheetItem::Raw(_) => true,
        }
    }

    /// About how many bytes this item takes in the file (on the high side, like `Model::size_estimate`'s other parts).
    pub fn size_estimate(&self) -> usize {
        let pts = match self {
            SheetItem::Known(ItemKind::Line(s) | ItemKind::Polyline(s) | ItemKind::Arc(s) | ItemKind::Rect(s)) => s.pts_mm.len(),
            SheetItem::Known(ItemKind::Hatch(h)) => h.ring_mm.len() + h.holes.iter().map(Vec::len).sum::<usize>(),
            SheetItem::Known(ItemKind::Viewport(v)) => v.crop.as_ref().map_or(0, Vec::len),
            SheetItem::Known(ItemKind::Dimension(d)) => 12 * d.anchors.len(),
            SheetItem::Known(ItemKind::Table(t)) => t.rows.iter().map(|r| r.len() * 2).sum(),
            _ => 0,
        };
        match self {
            SheetItem::Raw(v) => v.to_string().len() + 16,
            _ => 400 + 2 * self.text_bytes() + 30 * pts,
        }
    }

    pub fn text_bytes(&self) -> usize {
        match self {
            SheetItem::Known(k) => k.text_bytes(),
            SheetItem::Raw(v) => v.to_string().len(),
        }
    }

    /// Ids this item points at on the model's side: anchored vertices are the later step's business; here, the title
    /// block a `title_block_ref` names.
    pub fn title_block(&self) -> Option<Id> {
        match self {
            SheetItem::Known(ItemKind::TitleBlockRef(t)) => Some(t.title_block),
            _ => None,
        }
    }

    /// The highest id this item mentions (its sheet, a title block, the model ids of its anchors).
    pub fn max_id(&self) -> Id {
        let anchor_ids = |a: &Anchor| {
            let at = match &a.at {
                AnchorAt::Vertex(v) | AnchorAt::CurveCenter(v) | AnchorAt::Guide(v) => *v,
                AnchorAt::Between { a, b, .. } => (*a).max(*b),
            };
            at.max(a.path.iter().copied().max().unwrap_or(0))
        };
        match self {
            SheetItem::Known(ItemKind::Dimension(d)) => d.sheet.max(d.viewport.unwrap_or(0)).max(d.anchors.iter().map(anchor_ids).max().unwrap_or(0)),
            SheetItem::Known(ItemKind::TitleBlockRef(t)) => t.sheet.max(t.title_block),
            SheetItem::Known(k) => k.sheet(),
            SheetItem::Raw(_) => self.sheet().unwrap_or(0),
        }
    }
}

impl Sheet {
    pub fn sane(&self) -> bool {
        sane(self.paper.w_mm) && sane(self.paper.h_mm) && self.paper.w_mm > 0.0 && self.paper.h_mm > 0.0
    }
    pub fn text_bytes(&self) -> usize {
        self.name.len() + self.number.len() + self.paper.name.len() + self.fields.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() + self.issued.iter().map(|i| i.rev.len() + i.date.len() + i.note.len()).sum::<usize>()
    }
}

impl TitleBlock {
    pub fn text_bytes(&self) -> usize {
        self.name.len() + self.fields.iter().map(|f| f.key.len() + f.label.len()).sum::<usize>()
    }
    pub fn sane(&self) -> bool {
        self.size_mm.iter().all(|&x| sane(x)) && self.lines_mm.iter().flatten().flatten().all(|&x| sane(x)) && self.fields.iter().all(|f| f.at_mm.iter().all(|&x| sane(x)) && sane(f.height_mm))
    }
}

impl Drafting {
    pub fn text_bytes(&self) -> usize {
        self.patterns.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
            + self.tag_styles.keys().map(|k| k.len() + 64).sum::<usize>()
            + self.line_types.keys().map(|k| k.len() + 16).sum::<usize>()
            + self.dim_styles.keys().map(|k| k.len() + 32).sum::<usize>()
            + self.text_styles.keys().map(|k| k.len() + 32).sum::<usize>()
    }
    pub fn sane(&self) -> bool {
        let w = |x: f64| sane(x);
        self.tag_styles.values().all(|t| matches!(t, Keep::Known(t) if w(t.weight_mm) && w(t.cut.outline_mm) && w(t.beyond.weight_mm) && t.cut.hatch.iter().chain(t.surface_hatch.iter()).all(|h| w(h.weight_mm) && w(h.scale) && w(h.angle))) || matches!(t, Keep::Raw(_)))
            && self.line_types.values().all(|l| matches!(l, Keep::Known(l) if l.dash_mm.iter().all(|&x| w(x))) || matches!(l, Keep::Raw(_)))
            && self.text_styles.values().all(|t| matches!(t, Keep::Known(t) if w(t.height_mm)) || matches!(t, Keep::Raw(_)))
    }
}
