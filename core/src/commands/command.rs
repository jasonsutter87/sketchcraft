//! The command set: one enum variant per command, with its JSON shape (`{"cmd":"rect",...}`).

use super::{CageOp, KnownLength};
use crate::model::Id;
use crate::render::RenderOpts;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Start an empty model.
    /// A blank model; `figure` places a scale figure ("person" or "beaver") at the origin.
    New {
        #[serde(default)]
        figure: Option<String>,
    },
    /// Axis-aligned rectangle between two opposite corners (one coordinate must match).
    Rect { from: [f64; 3], to: [f64; 3] },
    /// Planar polygon through the points (a building footprint, a gable end...).
    Polygon { points: Vec<[f64; 3]> },
    /// Connected lines; if the last point closes the loop, a face is made.
    Line { points: Vec<[f64; 3]> },
    /// Circle (or regular polygon with few `segments`) as a face.
    Circle {
        center: [f64; 3],
        radius: f64,
        #[serde(default = "up")]
        normal: [f64; 3],
        #[serde(default = "circle_segments")]
        segments: usize,
    },
    /// Arc as edges, from `start` to `end`, bowing out by `bulge` meters.
    Arc {
        start: [f64; 3],
        end: [f64; 3],
        bulge: f64,
        #[serde(default = "up")]
        normal: [f64; 3],
        #[serde(default = "arc_segments")]
        segments: usize,
    },
    /// Offset a face's outline: + inward (new face inside), - outward (ring).
    Offset { face: Id, distance: f64 },
    /// Sweep a profile face along a path: `points`, `edges`, or `around` a face's outline.
    FollowMe {
        face: Id,
        #[serde(default)]
        points: Option<Vec<[f64; 3]>>,
        #[serde(default)]
        edges: Option<Vec<Id>>,
        #[serde(default)]
        around: Option<Id>,
    },
    /// Pull a face out (positive) or push it in (negative) along its normal.
    /// `toward`: pull that way whichever way the face happens to look
    /// (a new rectangle on the ground can face up or down).
    #[serde(alias = "push_pull")]
    Pushpull {
        face: Id,
        distance: f64,
        #[serde(default)]
        toward: Option<[f64; 3]>,
    },
    /// Move faces, edges, vertices or whole groups.
    Move { ids: Vec<Id>, by: [f64; 3] },
    /// Rotate around `center` about `axis` (default: vertical), in degrees.
    Rotate {
        ids: Vec<Id>,
        center: [f64; 3],
        #[serde(default = "up")]
        axis: [f64; 3],
        angle: f64,
    },
    /// Scale from `center`: one number for uniform, or [x, y, z].
    Scale { ids: Vec<Id>, center: [f64; 3], factor: Factor },
    /// Copy faces, groups or components, offset by `by`.
    Duplicate { ids: Vec<Id>, by: [f64; 3] },
    /// `count` more copies spaced `by` apart (a row of houses, a fence).
    Array { ids: Vec<Id>, by: [f64; 3], count: usize },
    Delete { ids: Vec<Id> },
    /// Flip faces (or every face in a group).
    Reverse { ids: Vec<Id> },
    /// Color faces (or every face in a group), 0..1 RGB.
    Paint {
        ids: Vec<Id>,
        #[serde(default)]
        color: Option<[f32; 3]>,
        /// a material by name (an image material from `texture`)
        #[serde(default)]
        material: Option<String>,
    },
    /// Create or update an image material. `data` is a data: URL; `width` and
    /// `height` are meters per tile; `project` [x,y,z] projects it along a
    /// direction (null turns projection off); `color` is its average color.
    Texture {
        name: String,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        color: Option<[f32; 3]>,
        #[serde(default)]
        project: Option<Option<[f64; 3]>>,
        #[serde(default)]
        origin: Option<[f64; 3]>,
    },
    /// New geometry goes into a fresh group until end_group.
    BeginGroup { #[serde(default)] name: Option<String> },
    EndGroup,
    /// Wrap existing faces into a group.
    Group { ids: Vec<Id>, #[serde(default)] name: Option<String> },
    /// List faces, optionally only one group's, or only those facing a direction.
    Faces {
        #[serde(default)]
        group: Option<Id>,
        #[serde(default)]
        facing: Option<[f64; 3]>,
    },
    /// The face under a point (on its plane, inside its outline), or the
    /// nearest face center if none contains it. Optional facing filter.
    Pick {
        at: [f64; 3],
        #[serde(default)]
        facing: Option<[f64; 3]>,
    },
    /// Duplicate a group (a whole house), offset by `by`.
    Copy { group: Id, by: [f64; 3], #[serde(default)] name: Option<String> },
    /// Counts, bounds, groups, materials.
    Info,
    /// Each part's true size (along its own faces, so a turned board reads
    /// right): `ids` (groups or components), or every leaf group and component.
    Parts {
        #[serde(default)]
        ids: Vec<Id>,
    },
    /// The plugins: built in, and those in ~/Documents/SketchCraft/Plugins.
    Plugins,
    /// Turn a plugin on or off (remembered in the Plugins folder).
    PluginEnable { id: String, on: bool },
    /// Install a plugin from a .zip: a file `path`, or its bytes as base64 `data`.
    InstallPlugin {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        data: Option<String>,
        /// the file's name, when sending `data` (a .rb script is told apart by it)
        #[serde(default)]
        name: Option<String>,
        /// only describe the plugin (what it is, its files, what it would replace); install nothing
        #[serde(default)]
        preview: bool,
        /// allow replacing a plugin that's already installed (the person has been asked)
        #[serde(default)]
        replace: bool,
    },
    /// Remove a plugin you installed (moved aside to Plugins/.removed, not deleted).
    RemovePlugin { id: String },
    /// Write text to a file (the app's Save CSV; plugins can't reach it).
    SaveText { path: String, text: String },
    /// Make faces or a group into a reusable component (placed where it was).
    MakeComponent { ids: Vec<Id>, #[serde(default)] name: Option<String> },
    /// Place a component (by id or name) at a point, optionally turned about Z.
    Place {
        component: Value,
        at: [f64; 3],
        #[serde(default)]
        rotate: f64,
        #[serde(default)]
        name: Option<String>,
        /// the face it's placed on: a component that cuts openings cuts one there
        #[serde(default)]
        host: Option<Id>,
    },
    /// Whether a component cuts an opening in the face it's placed on.
    ComponentCuts { id: Id, cuts: bool },
    /// Whether a component always turns to face the camera (flat figures).
    ComponentFaceMe { id: Id, on: bool },
    /// Open a group, or a component's definition through one of its placements; `close` to finish.
    Edit { id: Id },
    Close,
    /// Turn a component placement back into plain geometry.
    Explode { id: Id },
    /// List the component library.
    Components,
    Hide { ids: Vec<Id> },
    /// Unhide `ids`, or everything if none given.
    Unhide { #[serde(default)] ids: Vec<Id> },
    /// Put groups/components on a tag (layer); null tag removes it.
    Tag { ids: Vec<Id>, tag: Option<String> },
    TagVisible { tag: String, visible: bool },
    /// A tag's color (for Color by tag); null clears it.
    TagColor { tag: String, color: Option<[f32; 3]> },
    /// A tag's line weight (paper mm, 0.05 to 2; 0 clears it), line style (solid, dashed, hidden, center, or a line
    /// type name; "" clears it) and colour ("#rrggbb"; "" clears it) on drawing sheets and in the model view. Only
    /// what is given changes; `clear: true` removes the tag's whole style.
    TagStyle {
        tag: String,
        #[serde(default)]
        weight_mm: Option<f64>,
        #[serde(default)]
        line: Option<String>,
        #[serde(default)]
        color: Option<String>,
        /// the hatch the tag's cut surfaces get in plans and sections: `{pattern, color?, weight_mm?, scale?, angle?,
        /// scale_mode?: paper|model}`; only what is given changes; `{"pattern": ""}` removes it
        #[serde(default)]
        hatch: Option<HatchArgs>,
        /// how the tag's cut surfaces are filled and outlined in plans and sections (sheets and the on-screen
        /// section): `{fill?: auto|solid|hatch|none, color? ("#rrggbb" for solid; "" clears), outline_mm? (0.05 to 2;
        /// 0 clears), priority? (0 to 999: the higher tag wins where cuts overlap)}`; only what is given changes
        #[serde(default)]
        cut: Option<CutArgs>,
        #[serde(default)]
        clear: bool,
    },
    /// Keep a hatch pattern in the model (`.pat` text, AutoCAD format) under `name`, so tag hatches can use it.
    /// `text` may hold several patterns: the one called `name` is taken (or the only one). `units` is what a file
    /// that doesn't say means (`mm` or `inch`; AutoCAD's own set is in inches). `delete: true` removes it.
    HatchPattern {
        name: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        units: Option<String>,
        #[serde(default)]
        delete: bool,
    },
    /// Read-only: every hatch pattern a tag can use (built-in and in the model), with its lines; with `text`, the
    /// patterns in that `.pat` text instead (nothing is kept), and what in it couldn't be read.
    HatchPatterns {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        units: Option<String>,
    },
    /// The same for single edges (`ids`: edges, or faces for all their edges); an edge's own style beats its tag's.
    /// `clear: true` removes the edges' own style.
    EdgeStyle {
        ids: Vec<Id>,
        #[serde(default)]
        weight_mm: Option<f64>,
        #[serde(default)]
        line: Option<String>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        clear: bool,
    },
    /// Rename a tag everywhere it's used.
    RenameTag { tag: String, to: String },
    /// Put groups and components inside a group (`parent`), or at the top (null).
    SetParent { ids: Vec<Id>, parent: Option<Id> },
    /// Dissolve a group: what's inside moves up a level.
    Ungroup { id: Id },
    /// Lock or unlock groups (locked: can't be moved, edited or deleted).
    Lock { ids: Vec<Id>, #[serde(default = "yes")] locked: bool },
    /// Soft edges aren't drawn. `ids` edges or faces; `coplanar:true` softens every flat seam.
    Soften {
        #[serde(default)]
        ids: Vec<Id>,
        #[serde(default = "yes")]
        soft: bool,
        #[serde(default)]
        coplanar: bool,
    },
    Rename { id: Id, name: String },
    /// Construction guide: a line through `point` along `direction`, or a point.
    Guide { point: [f64; 3], #[serde(default)] direction: Option<[f64; 3]> },
    ClearGuides,
    /// Display units: m, cm, mm, ft (feet-inches) or in. Geometry stays in meters.
    Units { units: String },
    /// A dimension from `from` to `to`, drawn `offset` away (e.g. [0,-1,0] = 1 m toward the street).
    Dimension { from: [f64; 3], to: [f64; 3], #[serde(default)] offset: [f64; 3] },
    /// A text label at `at`, with an optional leader line to `leader`.
    Text { at: [f64; 3], text: String, #[serde(default)] leader: Option<[f64; 3]> },
    /// Save a camera as a named view.
    /// `photo`: a picture seen from this camera (data:image URL), as Match Photo makes
    /// Tag visibility and what's hidden are remembered with it; `style` (the
    /// app's view style) and `section` ([origin, normal], or none) when given.
    SaveView {
        name: String,
        eye: [f64; 3],
        target: [f64; 3],
        #[serde(default = "fov")]
        fov: f64,
        #[serde(default)]
        photo: Option<String>,
        #[serde(default)]
        style: Option<serde_json::Value>,
        #[serde(default)]
        section: Option<[f64; 6]>,
        /// true: the camera only (SketchUp's scene with its other properties unticked)
        #[serde(default)]
        camera_only: bool,
    },
    /// Go to a saved view's state: tags shown / hidden and what's hidden as
    /// they were. Returns the view (camera, style, section) for the app.
    ShowView { name: String },
    DeleteView { name: String },
    Undo,
    Redo,
    /// Start one undo step that spans many commands (a plugin run); `end_batch` closes it.
    /// While it is open the model isn't saved after each command, and Undo is off.
    BeginBatch,
    /// `id`: the number `begin_batch` answered with; a stale one (its batch is already over) does nothing
    EndBatch {
        #[serde(default)]
        id: Option<u64>,
    },
    /// Render a PNG. Without `out`, just reports the camera it would use.
    Render {
        #[serde(default)]
        out: Option<String>,
        #[serde(flatten)]
        opts: RenderOpts,
    },
    Save { path: String },
    Open { path: String },
    /// Wavefront OBJ (+ .mtl) for Blender and friends.
    ExportObj { path: String },
    /// Binary glTF (.glb): Blender, three.js, game engines, Quick Look.
    ExportGlb { path: String },
    /// Binary STL for 3D printing; `scale` 1000 gives millimeters.
    ExportStl { path: String, #[serde(default = "one")] scale: f64 },
    /// Animation through saved views (or a turntable): `path` .mp4/.mov
    /// (needs ffmpeg) or a folder for PNG frames. Render options apply.
    Animate {
        path: String,
        #[serde(default = "anim_fps")]
        fps: f64,
        #[serde(default = "anim_transition")]
        transition: f64,
        #[serde(default = "one")]
        delay: f64,
        #[serde(default)]
        views: Vec<String>,
        #[serde(default = "anim_turntable")]
        turntable: f64,
        /// render the timeline instead of a tour of the scenes
        #[serde(default)]
        timeline: bool,
        #[serde(flatten)]
        opts: RenderOpts,
    },
    /// CREATE saved drawing sheets from a description of pages: each page becomes a sheet in the model
    /// (undoable, kept in the file) with its viewports (saved views or iso / top / front / ..., optionally at a
    /// scale like "1:100") and table. No pages: one sheet of your saved views. To write a PDF, use `export_sheets`.
    Sheets {
        /// no longer accepted here (it used to write a PDF): `export_sheets` does that
        #[serde(default)]
        path: Option<String>,
        #[serde(default = "sheet_a3")]
        sheet: String,
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        pages: Vec<crate::layout::PageSpec>,
        /// white on blueprint blue
        #[serde(default)]
        blueprint: bool,
        /// lines in a box above the title block ("# " makes a heading)
        #[serde(default)]
        notes: Vec<String>,
    },
    /// Write drawing sheets as a PDF (or, for a `.svg` or `.dxf` path, one page of them). With `pages` as numbers (sheet ids), or none: the saved sheets (all, or those).
    /// With `pages` as descriptions (what `sheets` takes): a one-shot set that saves nothing. No saved sheets and no
    /// pages: your saved views on one sheet.
    ExportSheets {
        path: String,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        sheet: Option<String>,
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        pages: Vec<ExportPage>,
        #[serde(default)]
        blueprint: Option<bool>,
        #[serde(default)]
        notes: Option<Vec<String>>,
        /// a `.svg` or `.dxf` path writes one page of the set: which one, from 1 (default 1)
        #[serde(default)]
        page: Option<usize>,
    },
    /// A new empty drawing sheet in the model.
    SheetAdd {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        number: Option<String>,
        /// "a4", "a3", "a2", "letter" or "tabloid", or {w_mm, h_mm, name?} (default a3)
        #[serde(default)]
        paper: Option<PaperArg>,
        /// position in the set (default: last)
        #[serde(default)]
        order: Option<i32>,
        #[serde(default)]
        fields: std::collections::BTreeMap<String, String>,
        #[serde(default)]
        title_block: Option<Id>,
    },
    /// Change a sheet's name, number, paper, order, title block (0 clears it) or fields (null removes one).
    SheetEdit {
        sheet: Id,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        number: Option<String>,
        #[serde(default)]
        paper: Option<PaperArg>,
        #[serde(default)]
        order: Option<i32>,
        #[serde(default)]
        fields: std::collections::BTreeMap<String, Option<String>>,
        #[serde(default)]
        title_block: Option<Id>,
    },
    /// Delete a sheet and everything on it.
    SheetDelete { sheet: Id },
    /// Put a viewport (a window on the model, with its own scale, anchor and crop) on a sheet. `viewports` adds
    /// several at once (one undo step).
    ViewportAdd {
        #[serde(flatten)]
        args: ViewportArgs,
        #[serde(default)]
        viewports: Vec<ViewportArgs>,
    },
    /// Change a viewport. Only what is given changes: its anchor, scale and crop are never re-fitted.
    ViewportEdit {
        id: Id,
        #[serde(flatten)]
        args: ViewportArgs,
        /// remove the crop
        #[serde(default)]
        clear_crop: bool,
    },
    /// Delete things on sheets (viewports, dimensions, text, lines...) by id.
    SheetItemDelete { ids: Vec<Id> },
    /// A dimension on a sheet, attached to the model: two anchors make one, more make a chain. Each anchor is
    /// `{"point": [x, y, z]}` (the nearest vertex within `tol`, else a point along the nearest edge), or exactly:
    /// `{"vertex": id}`, `{"between": [a, b], "t": 0.5}`, `{"curve": id}` or `{"guide": id}`, with an optional
    /// `"path": [instance ids]` for geometry inside components. The value is never stored: it is measured from the
    /// live model each time. Moving the model moves the dimension; deleting its geometry breaks it (see `sheets_info`).
    SheetDim {
        sheet: Id,
        anchors: Vec<AnchorArg>,
        /// aligned (default), horizontal, vertical or baseline (every segment from the first anchor)
        #[serde(default)]
        mode: Option<crate::drafting::DimMode>,
        /// millimetres from the measured line on paper (default 8)
        #[serde(default)]
        offset_mm: Option<f64>,
        /// the viewport whose drawing plane it is measured in (default: the topmost on the sheet that frames it)
        #[serde(default)]
        viewport: Option<Id>,
        #[serde(default)]
        style: Option<String>,
        #[serde(default)]
        text_override: Option<String>,
        /// how close a `point` anchor must be to geometry, in metres (default 0.001)
        #[serde(default)]
        tol: Option<f64>,
    },
    /// Paper line work on a sheet (drawn in the PDF, SVG and DXF). `kind` is `line` (2 points), `polyline` (2 or more),
    /// `arc` (3 points: start, a point on it, end) or `rect` (2 opposite corners); default: a line for 2 points, else a
    /// polyline. Points are paper millimetres from the sheet's top left. `w_mm` (0.05 to 2, default 0.25), `color`
    /// ("#rrggbb"; default the sheet's ink) and `dash_mm` (on, off, ...) make the pen. `lines: [..]` adds several as
    /// one step. With `id`, changes that item instead (only what is given; its kind stays). Delete with `sheet_item_delete`.
    SheetLine {
        #[serde(flatten)]
        args: LineArgs,
        #[serde(default)]
        lines: Vec<LineArgs>,
    },
    /// Paper text on a sheet: `text` (lines split on newlines) with its first line's baseline starting at `at_mm`
    /// (paper mm from the sheet's top left), `height_mm` tall (1 to 50, default 3.5). With `id`, changes that text
    /// instead (only what is given). Delete with `sheet_item_delete`.
    SheetText {
        #[serde(default)]
        id: Option<Id>,
        #[serde(default)]
        sheet: Option<Id>,
        #[serde(default)]
        at_mm: Option<[f64; 2]>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        height_mm: Option<f64>,
        #[serde(default)]
        z: Option<i32>,
    },
    /// Trim paper line work: cut away the span of `item` that holds `at_mm` (the point you pick on it), between the
    /// nearest places other things cross it. Boundaries are the other paper line work on the sheet, the lines the
    /// sheet's viewports draw of the model (never trimmed themselves), and viewport frames. A span in the middle
    /// leaves two pieces (the second is a new item); one at the end leaves one. With no crossing the command refuses.
    /// `pick_tol_mm` (default 2) is how far `at_mm` may be from the line work. `use_model: false` ignores the model's lines.
    SheetTrim {
        item: Id,
        at_mm: [f64; 2],
        #[serde(default)]
        pick_tol_mm: Option<f64>,
        #[serde(default)]
        use_model: Option<bool>,
        /// work out what it would do (the reply's `cut` / `to`) and change nothing: a hover preview, read-only
        #[serde(default)]
        preview: bool,
    },
    /// Extend paper line work: the end of `item` nearest `at_mm` runs on, along its own direction (an arc along its
    /// circle), to the nearest boundary ahead of it: other line work, a model line a viewport draws, or a viewport
    /// frame. Refuses when nothing lies ahead, and for closed shapes.
    SheetExtend {
        item: Id,
        at_mm: [f64; 2],
        #[serde(default)]
        pick_tol_mm: Option<f64>,
        #[serde(default)]
        use_model: Option<bool>,
        /// work out what it would do (the reply's `cut` / `to`) and change nothing: a hover preview, read-only
        #[serde(default)]
        preview: bool,
    },
    /// Offset paper line work: a parallel copy `distance_mm` away (a new item with the same pen). Say which side
    /// with `at_mm` (the side that point is on), or `side`: `left` or `right` of the way the line runs from its first
    /// point (as the sheet looks), or `inside` / `outside` for a rect or a closed polyline. Lines move over,
    /// polylines get mitred corners, arcs get a bigger or smaller radius, rects grow or shrink.
    SheetOffset {
        item: Id,
        distance_mm: f64,
        #[serde(default)]
        at_mm: Option<[f64; 2]>,
        #[serde(default)]
        side: Option<String>,
    },
    /// Attach broken anchors again. `{"item": id, "anchor": 1, "to": {..an anchor..}}` attaches one where you say.
    /// `{"all_exact": true}` (optionally with `item` or `sheet`) attaches every broken anchor that has exactly one
    /// vertex within 1 micrometre of where it last was, and leaves the rest. Nothing reattaches by itself.
    SheetReattach {
        #[serde(default)]
        item: Option<Id>,
        #[serde(default)]
        sheet: Option<Id>,
        #[serde(default)]
        anchor: Option<usize>,
        #[serde(default)]
        to: Option<AnchorArg>,
        #[serde(default)]
        all_exact: bool,
        /// with `all_exact`: how far from where it last was a vertex may be, in metres (default 0.000001)
        #[serde(default)]
        tol: Option<f64>,
    },
    /// Read-only: every sheet with what is on it (and the kinds of item this version doesn't know).
    SheetsInfo {
        #[serde(default)]
        sheet: Option<Id>,
    },
    /// Read-only: the points of a viewport's drawing a dimension can attach to, where each is on the paper (mm from the
    /// sheet's top left) and the anchor to give `sheet_dim` for it. `kinds`: `vertex` (default: the model's corners
    /// the drawing shows), `midpoint` (the middle of each edge), `opening` (the sides of doors and windows, plans
    /// only). `limit` (default 500, at most 5,000) caps the list; `total` says how many there were.
    ViewportPoints {
        viewport: Id,
        #[serde(default)]
        kinds: Vec<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Attached dimension strings worked out from a plan viewport's floor (`callouts::auto_dims`): `exterior` (the
    /// overall width on the south and depth on the east), `openings` (a chain through both sides of every door and
    /// window on each face), `interior` (every wall face along the building, drawn on the north and the west), `grids`
    /// (column centres and guide points). `scope` is one of these or a list (default exterior and openings). Adds
    /// only: dimensions it made before (marked `source: "auto"`) are skipped, never doubled, and nobody else's is
    /// touched; `refresh: true` replaces its own for those scopes. One undo step.
    DimensionAuto {
        viewport: Id,
        #[serde(default)]
        scope: Option<ScopeArg>,
        #[serde(default)]
        refresh: bool,
    },
    /// Lay out a set of saved, editable sheets from the building in one undo step. `preset: "permit"`: a cover with
    /// the sheet index, a plan per floor, the four elevations, a section through the stairs (when there are stairs)
    /// and the door and window schedule. `paper` (default a3), `scale` (one scale for every drawing; default the
    /// largest that fits), `project`.
    LayoutPreset {
        #[serde(default = "preset_permit")]
        preset: String,
        #[serde(default)]
        paper: Option<String>,
        #[serde(default)]
        scale: Option<ScaleArg>,
        #[serde(default)]
        project: Option<String>,
    },
    /// Read-only: a saved sheet's drawing as its display list, in light JSON (paper mm, y down; paths as SVG path
    /// strings, joined per pen): what the app's paper view draws. `limit_kb` caps the answer (default and most
    /// 16384); past it the rest is left out and `truncated` is true.
    SheetIr {
        sheet: Id,
        #[serde(default)]
        limit_kb: Option<usize>,
    },
    /// Put the sheets in this order (every sheet's id once, first first): orders 0, 10, 20... One undo step.
    SheetOrder { sheets: Vec<Id> },
    /// A copy of a sheet with everything on it (viewports, dimensions, line work, text), placed right after it in
    /// the set. One undo step.
    SheetDuplicate {
        sheet: Id,
        #[serde(default)]
        name: Option<String>,
    },
    /// A saved sheet as a PNG picture (`path`), `px` wide (default 1600, 200 to 4000), to look at and check.
    SheetRender {
        sheet: Id,
        path: String,
        #[serde(default)]
        px: Option<u32>,
    },
    /// Read-only: how the kept drawings of sheet viewports stand (`revision` moves when a change drops some;
    /// `drawing`: the background worker is redrawing), with counters.
    DraftingStatus,
    /// A blueprint set in one go: every floor's plan and the four
    /// elevations, white on blue, at a true scale that fits the sheet.
    Blueprint {
        path: String,
        #[serde(default = "sheet_a3")]
        sheet: String,
        #[serde(default)]
        project: Option<String>,
        /// general notes under the area table
        #[serde(default)]
        notes: Vec<String>,
    },
    /// A door's swing for plans: the hinge, the strike point across the
    /// opening, and any point on the side it swings into.
    DoorSwing { hinge: [f64; 3], strike: [f64; 3], toward: [f64; 3] },
    /// The building's floor heights (m), found from its floors.
    Levels,
    /// Collada (.dae), Z up, meters: one object per group / component.
    ExportDae { path: String },
    /// 3D DXF: 3DFACE triangles in millimeters, a layer per object.
    ExportDxf { path: String },
    /// PDF: a rendered view (same options as render) on a landscape A4 sheet.
    ExportPdf {
        path: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(flatten)]
        opts: RenderOpts,
    },
    /// Import an OBJ file; each object becomes a group. `up` "y" (default) or "z".
    ImportObj {
        #[serde(default)]
        path: Option<String>,
        /// the file's contents, instead of a path (the app sends this)
        #[serde(default)]
        text: Option<String>,
        #[serde(default = "y_up")]
        up: String,
        #[serde(default = "one")]
        scale: f64,
        #[serde(default)]
        at: [f64; 3],
        /// component name (default: the file name)
        #[serde(default)]
        name: Option<String>,
    },
    /// Import a SketchUp file (SketchUp 6-8 era). `path`, or the file as base64 in `data`.
    ImportSkp {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        at: [f64; 3],
        #[serde(default)]
        name: Option<String>,
    },
    /// Solid tools on closed groups / components: union, subtract,
    /// intersect, trim, split, outer_shell. Subtract and trim: the first
    /// cuts the second.
    Solid { op: String, ids: Vec<Id> },
    /// Intersect Faces: draw lines where the picked faces / groups cross the
    /// model (`with: "model"`, default) or each other (`"selection"`).
    Intersect {
        ids: Vec<Id>,
        #[serde(default = "with_model")]
        with: String,
    },
    /// Flip Along an axis (red/x, green/y, blue/z) across the pick's middle.
    Flip { ids: Vec<Id>, axis: String },
    /// Section cut through `origin` facing `normal`: the cut lines and filled
    /// regions per object. With `path` (.svg or .dxf), writes it flat, in mm.
    Section {
        origin: [f64; 3],
        normal: [f64; 3],
        #[serde(default)]
        path: Option<String>,
    },
    /// Sandbox From Contours: contour edges (or groups of them) -> terrain.
    TerrainFromContours {
        ids: Vec<Id>,
        #[serde(default = "contour_spacing")]
        spacing: f64,
    },
    /// Sandbox From Scratch: a flat triangulated grid.
    TerrainGrid {
        #[serde(default)]
        at: [f64; 3],
        width: f64,
        depth: f64,
        #[serde(default = "one")]
        spacing: f64,
    },
    /// Sandbox Smoove: raise (or lower) a soft round area of a terrain.
    Smoove { group: Id, at: [f64; 3], radius: f64, height: f64 },
    /// Sandbox Stamp: flatten a pad under a footprint face, sloping out over `offset`.
    Stamp {
        group: Id,
        face: Id,
        #[serde(default = "one")]
        offset: f64,
    },
    /// Sandbox Drape: lay edges onto a terrain, splitting its faces.
    Drape { group: Id, ids: Vec<Id> },
    /// Sandbox Flip Edge: swap the diagonal between two triangles.
    FlipEdge { edge: Id },
    /// Match Photo: solve the camera from two red and two green lines drawn
    /// along a photo's edges (pixels, first line of each pair toward +axis)
    /// and the origin's spot; saves it as a view that keeps the photo.
    MatchPhoto {
        name: String,
        width: f64,
        height: f64,
        red: [[f64; 4]; 2],
        green: [[f64; 4]; 2],
        origin: [f64; 2],
        #[serde(default = "photo_distance")]
        distance: f64,
        #[serde(default)]
        photo: Option<String>,
    },
    /// Dynamic Component attributes on a placement (or defaults on a
    /// component, by its component id). An empty value removes one.
    SetAttrs { id: Id, attrs: std::collections::BTreeMap<String, String> },
    /// A point cloud (scan) from .ply, .las, .xyz, .pts or .txt. `units` the
    /// file's (default m); at most `max_points` (thinned evenly beyond that).
    ImportCloud {
        path: String,
        #[serde(default)]
        units: Option<String>,
        #[serde(default = "cloud_budget")]
        max_points: usize,
    },
    /// A cloud's points for showing: base64 little-endian f32 x, y, z and
    /// u8 r, g, b (empty when uncolored), at most `max` points.
    CloudData {
        id: Id,
        #[serde(default = "cloud_show")]
        max: usize,
    },
    /// Rename, hide / show, or delete a cloud.
    Cloud {
        id: Id,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        hidden: Option<bool>,
        #[serde(default)]
        delete: bool,
    },
    /// IFC (and so Revit's exports) through IfcOpenShell, if installed: one
    /// group per element, typed for the Classifier, grouped by storey.
    ImportIfc { path: String },
    /// Selection helpers: a face (or edge) and what it touches. `all`: everything
    /// connected to it (SketchUp's triple-click); else the face and its edges
    /// (double-click; an edge gives the faces on it). Read-only.
    Connected {
        id: Id,
        #[serde(default)]
        all: bool,
    },
    /// Every face with this material (null: unpainted) / every group and
    /// placement with this tag, in the open context. Read-only.
    SameAs {
        #[serde(default)]
        material: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        #[serde(default)]
        by: String,
    },
    /// Resize the model (or the open group) by `factor` (Tape Measure: measure
    /// something, type its real length).
    Resize { factor: f64 },
    /// A named section plane through `origin`, keeping the side `normal`
    /// points to; it becomes the active cut. Named "Section N" when unnamed.
    SectionPlane {
        #[serde(default)]
        name: Option<String>,
        origin: [f64; 3],
        normal: [f64; 3],
    },
    /// Change a section plane: rename, move, make it the active cut, or delete.
    SectionEdit {
        id: Id,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        origin: Option<[f64; 3]>,
        #[serde(default)]
        normal: Option<[f64; 3]>,
        #[serde(default)]
        active: Option<bool>,
        #[serde(default)]
        delete: bool,
    },
    /// No section cut (the planes stay, ready to switch back on).
    SectionOff,
    /// Move the drawing axes (the Axes tool): origin, red direction, green
    /// direction (made square to red). `reset`: back to the model's own.
    SetAxes {
        #[serde(default)]
        origin: Option<[f64; 3]>,
        #[serde(default)]
        x: Option<[f64; 3]>,
        #[serde(default)]
        y: Option<[f64; 3]>,
        #[serde(default)]
        reset: bool,
    },
    /// A finish on faces (or everything in groups): each keeps its color or
    /// image, as a material "<its material> (<label>)" with these values.
    Finish {
        ids: Vec<Id>,
        label: String,
        #[serde(default)]
        roughness: Option<f32>,
        #[serde(default)]
        metalness: Option<f32>,
        #[serde(default)]
        glass: Option<bool>,
        #[serde(default)]
        emission: Option<f32>,
    },
    /// Component Options: the attributes a placement offers its user (those
    /// with an `_name_access` of textbox, list or view).
    Options { id: Id },
    /// Generate Report: every group and component with size, area, volume
    /// and attributes; `path` writes CSV.
    Report {
        #[serde(default)]
        path: Option<String>,
    },
    /// Classifier: mark groups / placements as an IFC type (IfcWall, IfcDoor,
    /// IfcWindow, IfcSlab, IfcRoof, IfcColumn, IfcBeam, IfcStair, ...);
    /// an empty type clears it.
    Classify { ids: Vec<Id>, class: String },
    /// IFC4 export: Project > Site > Building > Storey with every group and
    /// component as a typed, triangulated element.
    ExportIfc {
        path: String,
        #[serde(default)]
        project: Option<String>,
    },
    /// Add Location: terrain (and map imagery) of the area around lat / lon,
    /// `size` meters across, origin at ground level, +Y north.
    AddLocation {
        lat: f64,
        lon: f64,
        #[serde(default = "loc_size")]
        size: f64,
        #[serde(default = "loc_cells")]
        cells: usize,
        #[serde(default = "yes")]
        imagery: bool,
    },
    /// The component library: files you can place (default folder
    /// ~/Documents/SketchCraft/Library; subfolders are collections).
    Library {
        #[serde(default)]
        dir: Option<String>,
    },
    /// A thumbnail (PNG data URL) of a component in the model, a built-in
    /// asset, or a library file.
    Thumbnail {
        #[serde(default)]
        component: Option<Id>,
        #[serde(default)]
        asset: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default = "thumb_px")]
        size: u32,
    },
    /// Place a library file as a component (the same file again reuses it).
    InsertFile {
        path: String,
        at: [f64; 3],
        #[serde(default)]
        rotate: f64,
    },
    /// Save a component into the library (its parts come along).
    SaveToLibrary {
        component: Id,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        collection: Option<String>,
        #[serde(default)]
        dir: Option<String>,
    },
    /// Copy model files into the library.
    LibraryAdd {
        paths: Vec<String>,
        #[serde(default)]
        collection: Option<String>,
        #[serde(default)]
        dir: Option<String>,
    },
    /// Remove components nothing places.
    Purge,
    /// Take a material and its picture out of the model (the person's own command: an agent or a plugin can't).
    /// Faces that use it get `replace_with` (a material that exists), or the plain default look.
    DeleteMaterial {
        name: String,
        #[serde(default)]
        replace_with: Option<String>,
    },
    /// Remove every material no face uses (the person's own command).
    PurgeMaterials,
    /// Show a file or the library folder in Finder / Explorer.
    Reveal {
        #[serde(default)]
        path: Option<String>,
    },
    /// App skins in ~/Documents/SketchCraft/Skins (.sketchskin files, JSON).
    Skins,
    /// Save a skin there (`skin` is its JSON); returns the file's path.
    SaveSkin { name: String, skin: Value },
    /// Bevel (segments 1, a flat chamfer) or round (segments > 1) outside
    /// edges of solid groups by `size` meters.
    Bevel {
        edges: Vec<Id>,
        size: f64,
        #[serde(default = "one_seg")]
        segments: usize,
    },
    /// Split an edge into `count` equal parts.
    Divide { edge: Id, count: usize },
    /// Cut a solid group / component in two along a plane.
    Slice { id: Id, origin: [f64; 3], normal: [f64; 3] },
    /// A named material with a color (paint with it by name). Names say what
    /// it is to the photoreal renderer: glass / window, metal / steel /
    /// chrome, light / lamp; anything else is paint.
    /// `roughness` 0..1, `metalness` 0..1, `glass`, `emission`: how it looks
    /// in renders (and the viewport's physically based style).
    Material {
        name: String,
        #[serde(default)]
        color: Option<[f32; 3]>,
        #[serde(default)]
        roughness: Option<f32>,
        #[serde(default)]
        metalness: Option<f32>,
        #[serde(default)]
        glass: Option<bool>,
        #[serde(default)]
        emission: Option<f32>,
    },
    /// Photoreal render (path traced): sun from azimuth (degrees from north,
    /// clockwise) and altitude; `samples` per pixel (16 preview, 128+ final).
    Photoreal {
        #[serde(default)]
        out: Option<String>,
        #[serde(default = "pt_samples")]
        samples: u32,
        #[serde(default = "pt_bounces")]
        bounces: u32,
        /// sun direction; default: front-left of the view, for shadows
        #[serde(default)]
        sun_azimuth: Option<f64>,
        #[serde(default = "pt_altitude")]
        sun_altitude: f64,
        /// the real sun for the model's location (add_location; else 40°N
        /// 98°W): date "2026-06-21" and local time "15:30"
        #[serde(default)]
        date: Option<String>,
        #[serde(default)]
        time: Option<String>,
        /// hours the local clock is ahead of UTC (default: from the longitude)
        #[serde(default)]
        utc_offset: Option<f64>,
        /// add `samples` more to the same render each call (it sharpens);
        /// changing the camera, size, light or model starts over
        #[serde(default)]
        progressive: bool,
        #[serde(default = "pt_sun")]
        sun_strength: f32,
        #[serde(default = "yes")]
        denoise: bool,
        #[serde(default = "pt_exposure")]
        exposure: f32,
        /// how bright the sky's light is (1 = normal, 0 = sun only)
        #[serde(default = "pt_exposure")]
        sky_strength: f32,
        /// a plain backdrop instead of the sky, as "#rrggbb" (the sky still lights the model)
        #[serde(default)]
        background: Option<String>,
        /// the ground plane under the model (false: the model floats)
        #[serde(default = "yes")]
        ground: bool,
        /// the ground's color, "#rrggbb"
        #[serde(default)]
        ground_color: Option<String>,
        /// the sun's color: -1 cool .. 0 daylight .. 1 warm (golden hour)
        #[serde(default)]
        warmth: f32,
        #[serde(flatten)]
        opts: RenderOpts,
    },
    /// Model from several matched photos: corners marked in two or more
    /// photos are solved to 3D together with each photo's distance (see
    /// photomodel). Places guide points; `build` draws `edges` between them.
    PhotoSolve {
        points: Vec<crate::photomodel::PhotoPoint>,
        /// point index pairs to connect with lines (with `build`)
        #[serde(default)]
        edges: Vec<[usize; 2]>,
        /// keep this photo's distance (default: the first one marked)
        #[serde(default)]
        fixed: Option<String>,
        /// a known length: {a, b, length} (point indexes, meters)
        #[serde(default)]
        scale: Option<KnownLength>,
        #[serde(default = "yes")]
        place: bool,
        #[serde(default)]
        build: bool,
        /// make edges within a few degrees of an axis exactly square to it
        #[serde(default = "yes")]
        square: bool,
    },
    /// A smooth surface from a cage (Catmull-Clark, like Blender's
    /// Subdivision modifier): `points`, `faces` (index lists), optional
    /// per-face `materials`, sharp `creases` (index pairs), `levels` 0-5.
    Subd {
        #[serde(default)]
        name: Option<String>,
        points: Vec<[f64; 3]>,
        faces: Vec<Vec<usize>>,
        #[serde(default)]
        materials: Vec<Option<String>>,
        #[serde(default)]
        creases: Vec<[usize; 2]>,
        #[serde(default = "two")]
        levels: usize,
        /// keep the cage for SubD editing (cage_edit, the app's SubD mode)
        #[serde(default = "yes")]
        editable: bool,
        /// mirror plane for symmetric edits (found by itself when left out)
        #[serde(default)]
        symmetry: Option<crate::subd::Symmetry>,
    },
    /// Edit a subdivision object's cage, then rebuild its surface: move
    /// points ([[i, x, y, z], ...], mirrored when the cage is symmetric),
    /// crease/uncrease edges, and topology ops. One undo step.
    CageEdit {
        id: Id,
        #[serde(default)]
        moves: Vec<[f64; 4]>,
        #[serde(default = "yes")]
        mirror: bool,
        #[serde(default)]
        crease: Vec<[usize; 2]>,
        #[serde(default)]
        uncrease: Vec<[usize; 2]>,
        #[serde(default)]
        op: Option<CageOp>,
        #[serde(default)]
        levels: Option<usize>,
        /// "auto" (find the plane), "off", or an axis 0/1/2 (through the middle)
        #[serde(default)]
        symmetry: Option<Value>,
    },
    /// Smooth a group already modeled: it becomes its subdivision surface.
    /// `keep_sharp`: edges at a sharp angle stay creased.
    Subdivide {
        id: Id,
        #[serde(default = "two")]
        levels: usize,
        #[serde(default)]
        keep_sharp: bool,
    },
    /// Set the timeline: clips played in order (scenes, turntable, walk, sun).
    Timeline { clips: Vec<crate::timeline::Clip> },
    /// Every frame of the timeline: camera (and sun) per frame.
    TimelineFrames {
        #[serde(default = "anim_fps")]
        fps: f64,
    },
    /// The app's rendered frames for a video: one PNG/JPEG data URL each...
    VideoFrame { session: String, index: usize, data: String },
    /// ...then encode them (ffmpeg) to `path` (.mp4 / .mov) and clean up.
    VideoFinish { session: String, path: String, #[serde(default = "anim_fps")] fps: f64 },
    /// Import a DXF drawing (text DXF): a group with a group per layer, tagged
    /// by layer; closed outlines become faces. `units` overrides the file's.
    ImportDxf {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        units: Option<String>,
        #[serde(default)]
        at: [f64; 3],
        #[serde(default)]
        name: Option<String>,
    },
    /// Import an AutoCAD DWG (through LibreDWG or the ODA File Converter, if installed).
    ImportDwg {
        path: String,
        #[serde(default)]
        units: Option<String>,
        #[serde(default)]
        at: [f64; 3],
    },
    /// Walls from an imported plan: the thin faces between wall lines, pulled up
    /// `height` meters (in the group's wall layers if it has any).
    RaiseWalls {
        group: Id,
        #[serde(default = "wall_height")]
        height: f64,
        #[serde(default = "wall_thickness")]
        max_thickness: f64,
        /// windows found on the plan: wall below the sill and above the head
        #[serde(default = "window_sill")]
        sill: f64,
        #[serde(default = "window_head")]
        head: f64,
    },
    /// Export the model as an AutoCAD DWG (3D faces, through a converter).
    ExportDwg { path: String },
    /// Project a matched photo (a saved view with a photo) onto faces, or the
    /// faces of groups: each gets a straightened texture cut from the photo.
    ProjectPhoto {
        view: String,
        ids: Vec<Id>,
        #[serde(default = "photo_px")]
        size: u32,
    },
    /// Join connected edges into one curve (picked, offset and followed as one).
    Weld { edges: Vec<Id> },
    /// Split curves back into separate edges.
    ExplodeCurve { edges: Vec<Id> },
    /// Give one placement of a shared component its own definition.
    MakeUnique { id: Id },
    /// Flip a surface's faces to face the same way as `face`.
    OrientFaces { face: Id },
    /// 3D text from outlines (the app traces the font): `rings` in meters in the
    /// text plane, laid at `at` along `right` and `up`, pulled `depth` (0 = flat).
    #[serde(rename = "text_3d", alias = "text3d")]
    Text3d {
        rings: Vec<Vec<[f64; 2]>>,
        at: [f64; 3],
        #[serde(default = "x_axis")]
        right: [f64; 3],
        #[serde(default = "z_axis")]
        up: [f64; 3],
        #[serde(default)]
        depth: f64,
        #[serde(default)]
        name: Option<String>,
    },
    /// How see-through a material is in the view (reference images): 0-1.
    Opacity { material: String, value: f32 },
    /// Put the model in a ready-made setting: suburban, city, park, desert,
    /// waterfront, studio; or none to take it away.
    Environment { kind: String },
    /// Where the real sun is for the model's location at a local date and time.
    Sun {
        #[serde(default)]
        date: Option<String>,
        #[serde(default)]
        time: Option<String>,
        #[serde(default)]
        utc_offset: Option<f64>,
    },
    /// Write an image the app already has (a PNG data URL) to a file.
    SaveImage { path: String, data: String },
    /// Volume of a closed group / component (m³), or not a solid.
    Volume { id: Id },
    /// Import a binary FBX (7.x): `path`, or the file as base64 in `data`.
    /// The file becomes one component; each mesh a group inside it.
    ImportFbx {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        at: [f64; 3],
        #[serde(default)]
        name: Option<String>,
    },
    /// Walls along a path: `thickness` and `height`; `closed` makes a room.
    Wall {
        points: Vec<[f64; 3]>,
        #[serde(default = "wall_t")]
        thickness: f64,
        #[serde(default = "wall_h")]
        height: f64,
        #[serde(default)]
        closed: bool,
    },
    /// A straight stair climbing `rise` along `direction` (degrees from +X).
    Stairs {
        at: [f64; 3],
        rise: f64,
        #[serde(default)]
        direction: f64,
        #[serde(default = "stair_w")]
        width: f64,
        #[serde(default = "stair_t")]
        tread: f64,
    },
    /// A roof over a flat face: kind gable | hip | shed, pitch in degrees.
    Roof {
        face: Id,
        #[serde(default = "gable")]
        kind: String,
        #[serde(default = "pitch")]
        pitch: f64,
        #[serde(default = "overhang")]
        overhang: f64,
        #[serde(default = "roof_t")]
        thickness: f64,
    },
    /// A square (or `round`) column.
    Column {
        at: [f64; 3],
        #[serde(default = "col_size")]
        size: f64,
        #[serde(default = "wall_h")]
        height: f64,
        #[serde(default)]
        round: bool,
    },
    /// Place a built-in asset (door, window, garage_door, tree, palm, car, streetlight, bench, hydrant).
    Insert {
        asset: String,
        at: [f64; 3],
        #[serde(default)]
        rotate: f64,
        /// the face it's placed on (doors and windows cut their opening there)
        #[serde(default)]
        host: Option<Id>,
    },
    /// List the built-in assets.
    Assets,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Factor {
    Uniform(f64),
    Axes([f64; 3]),
}

fn pt_samples() -> u32 {
    32
}

fn pt_bounces() -> u32 {
    6
}

fn pt_altitude() -> f64 {
    32.0
}

fn pt_sun() -> f32 {
    2.4
}

fn pt_exposure() -> f32 {
    1.0
}

fn one_seg() -> usize {
    1
}

fn thumb_px() -> u32 {
    160
}

fn loc_size() -> f64 {
    300.0
}

fn loc_cells() -> usize {
    60
}

fn sheet_a3() -> String {
    "a3".into()
}

fn preset_permit() -> String {
    "permit".into()
}

/// `dimension_auto`'s scope: one name or a list of them.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScopeArg {
    One(String),
    Many(Vec<String>),
}

/// A paper size as a command gives it: a name, or millimetres.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PaperArg {
    Name(String),
    Custom {
        w_mm: f64,
        h_mm: f64,
        #[serde(default)]
        name: String,
    },
}

/// A scale as a command gives it: 100 or "1:100".
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScaleArg {
    N(f64),
    Text(String),
}

/// Where a dimension's anchor goes: a point to snap to geometry, or the ids themselves.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnchorArg {
    /// a world point: attaches to the nearest vertex (else edge) within the tolerance
    pub point: Option<[f64; 3]>,
    pub vertex: Option<Id>,
    pub between: Option<[Id; 2]>,
    pub t: Option<f64>,
    pub curve: Option<Id>,
    pub guide: Option<Id>,
    /// component placements from the root down, for geometry inside components
    pub path: Vec<Id>,
}

/// What a paper line work command may set. All optional on an edit; a new item needs `sheet` and `pts_mm`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LineArgs {
    /// the item to change (leave out to add one)
    pub id: Option<Id>,
    pub sheet: Option<Id>,
    /// line, polyline, arc or rect
    pub kind: Option<String>,
    pub pts_mm: Option<Vec<[f64; 2]>>,
    pub w_mm: Option<f64>,
    pub color: Option<String>,
    pub dash_mm: Option<Vec<f64>>,
    pub z: Option<i32>,
}

/// What a viewport command may set. All optional: a new viewport gets sensible defaults (see `sheets.rs`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ViewportArgs {
    pub sheet: Option<Id>,
    /// x, y, width, height on the sheet, millimetres from its top left
    pub rect_mm: Option<[f64; 4]>,
    /// the N of 1:N, or "1:N"
    pub scale: Option<ScaleArg>,
    /// the model point pinned to the middle of the rect (default: the middle of the model, once; never re-fitted)
    pub anchor: Option<[f64; 3]>,
    /// degrees
    pub rotation: Option<f64>,
    /// an outline in paper mm from the rect's top left
    pub crop: Option<Vec<[f64; 2]>>,
    /// {"type":"plan","cut_z":1.2,"floor_z":0}, {"type":"elevation","dir":[0,-1,0]}, {"type":"section","origin":[..],"normal":[..]} or {"type":"view3d","eye":[..],"target":[..],"fov":40}
    pub view: Option<crate::drafting::ViewKind>,
    /// a saved view's name, or iso / top / front / back / left / right: a 3D view from there (parallel, when a scale is given)
    pub from_view: Option<String>,
    pub z: Option<i32>,
    pub opacity: Option<f64>,
    pub render: Option<crate::drafting::RenderMode>,
    pub scene: Option<String>,
    pub label: Option<String>,
    pub tags: Option<std::collections::BTreeMap<String, crate::drafting::TagOverride>>,
}

/// One entry of `export_sheets`' `pages`: a saved sheet's id, or a page description.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ExportPage {
    Saved(Id),
    Spec(crate::layout::PageSpec),
}

fn photo_distance() -> f64 {
    15.0
}

fn anim_fps() -> f64 {
    24.0
}

fn anim_transition() -> f64 {
    2.0
}

fn anim_turntable() -> f64 {
    8.0
}

fn contour_spacing() -> f64 {
    1.0
}

fn with_model() -> String {
    "model".into()
}

fn circle_segments() -> usize {
    24
}

fn arc_segments() -> usize {
    12
}

fn wall_t() -> f64 {
    0.2
}

fn wall_h() -> f64 {
    2.7
}

fn stair_w() -> f64 {
    1.0
}

fn stair_t() -> f64 {
    0.28
}

fn gable() -> String {
    "gable".into()
}

fn pitch() -> f64 {
    30.0
}

fn overhang() -> f64 {
    0.4
}

fn roof_t() -> f64 {
    0.2
}

fn col_size() -> f64 {
    0.3
}

fn wall_height() -> f64 {
    2.7
}

fn wall_thickness() -> f64 {
    0.6
}

fn cloud_budget() -> usize {
    5_000_000
}

fn cloud_show() -> usize {
    2_000_000
}

fn window_sill() -> f64 {
    0.9
}

fn window_head() -> f64 {
    2.1
}

fn one() -> f64 {
    1.0
}

fn y_up() -> String {
    "y".into()
}

fn yes() -> bool {
    true
}

fn fov() -> f64 {
    45.0
}

fn two() -> usize {
    2
}

fn up() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

fn photo_px() -> u32 {
    1024
}

fn x_axis() -> [f64; 3] {
    [1.0, 0.0, 0.0]
}

fn z_axis() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

/// The cut fields `tag_style` takes; each optional (only what is given changes).
#[derive(Debug, Default, Deserialize)]
pub struct CutArgs {
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub outline_mm: Option<f64>,
    /// intersection priority, a whole number 0 to 999 (higher wins where cuts of different tags overlap)
    #[serde(default)]
    pub priority: Option<i64>,
}

/// The hatch fields `tag_style` takes; each optional (only what is given changes).
#[derive(Debug, Default, Deserialize)]
pub struct HatchArgs {
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub weight_mm: Option<f64>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub angle: Option<f64>,
    #[serde(default)]
    pub scale_mode: Option<String>,
}
