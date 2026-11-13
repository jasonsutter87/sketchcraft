//! The command layer. Everything that changes or inspects a model goes
//! through `Engine::exec`, whether it comes from a mouse in the app, a script,
//! the CLI or an AI agent over MCP. Commands are JSON:
//!
//!   {"cmd":"rect","from":[0,0,0],"to":[8,12,0]}
//!   {"cmd":"pushpull","face":7,"distance":6}
//!
//! Each reply is JSON too: {"ok":true, ...} or {"ok":false,"error":"..."}.

use crate::export;
use crate::model::{Id, Model};
use crate::render::{self, RenderOpts};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Start an empty model.
    New,
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
    },
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
    SaveView { name: String, eye: [f64; 3], target: [f64; 3], #[serde(default = "fov")] fov: f64 },
    DeleteView { name: String },
    Undo,
    Redo,
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
        #[serde(flatten)]
        opts: RenderOpts,
    },
    /// Drawing sheets as a PDF: pages of viewports (saved views or iso / top /
    /// front / ..., optionally at a scale like "1:100"), labels, scale bars
    /// and a title block. No pages: your saved views on one sheet.
    Sheets {
        path: String,
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
    Material { name: String, color: [f32; 3] },
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

fn axis_index(a: &str) -> Result<usize> {
    Ok(match a.to_ascii_lowercase().as_str() {
        "red" | "x" => 0,
        "green" | "y" => 1,
        "blue" | "z" => 2,
        _ => bail!("axis is red, green or blue (x, y, z)"),
    })
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

impl Command {
    fn mutates(&self) -> bool {
        !matches!(
            self,
            Command::Faces { .. }
                | Command::Volume { .. }
                | Command::Photoreal { .. }
                | Command::SaveImage { .. }
                | Command::Sun { .. }
                | Command::Library { .. }
                | Command::Thumbnail { .. }
                | Command::SaveToLibrary { .. }
                | Command::LibraryAdd { .. }
                | Command::Reveal { .. }
                | Command::Skins
                | Command::SaveSkin { .. }
                | Command::Report { .. }
                | Command::Section { .. }
                | Command::Info
                | Command::Components
                | Command::Pick { .. }
                | Command::Undo
                | Command::Redo
                | Command::Render { .. }
                | Command::Save { .. }
                | Command::ExportObj { .. }
                | Command::ExportGlb { .. }
                | Command::ExportStl { .. }
                | Command::ExportDae { .. }
                | Command::ExportIfc { .. }
                | Command::Sheets { .. }
                | Command::Blueprint { .. }
                | Command::Levels
                | Command::Animate { .. }
                | Command::ExportDxf { .. }
                | Command::ExportPdf { .. }
                | Command::Assets
        )
    }
}

#[derive(Default)]
pub struct Engine {
    pub model: Model,
    undo: Vec<Model>,
    redo: Vec<Model>,
    /// Script names: `"as":"front"` on a command stores its result id,
    /// and any later `"@front"` value is replaced by it.
    pub names: HashMap<String, Id>,
    /// Component definition being edited (`edit`), if any.
    pub ctx: Option<Id>,
    /// bumps on every change to the model (progressive renders start over)
    revision: u64,
    /// a photoreal render in progress
    pt: Option<crate::pt::Progress>,
}

const UNDO_LIMIT: usize = 200;

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::from(a)
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run one JSON command line and return a JSON reply (never panics on bad input).
    pub fn exec_json(&mut self, line: &str) -> Value {
        let mut v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return json!({"ok": false, "error": format!("bad JSON: {e}")}),
        };
        let name = v.as_object_mut().and_then(|o| o.remove("as")).and_then(|n| n.as_str().map(String::from));
        if let Err(e) = self.resolve(&mut v) {
            return json!({"ok": false, "error": e.to_string()});
        }
        let r = match serde_json::from_value::<Command>(v) {
            Ok(cmd) => self.exec(cmd),
            Err(e) => return json!({"ok": false, "error": format!("bad command: {e}")}),
        };
        if let (Some(n), true) = (name, r["ok"] == true) {
            if let Some(id) = ["face", "group", "instance", "guide", "dimension", "text"]
                .iter()
                .find_map(|k| r[*k].as_u64())
                .or_else(|| r["ids"][0].as_u64())
                .or_else(|| r["edges"][0].as_u64())
            {
                self.names.insert(n, id as Id);
            }
        }
        r
    }

    /// Build (or replace, or remove) the environment around the model: one
    /// undo step, in a locked group tagged "Environment".
    pub fn environment(&mut self, kind: &str) -> Result<Value> {
        if self.ctx.is_some() {
            bail!("close the component you're editing first");
        }
        let before = self.model.clone();
        let depth = self.undo.len();
        let fail = |e: &mut Engine, msg: String| -> Result<Value> {
            e.model = before.clone();
            e.undo.truncate(depth);
            bail!(msg)
        };
        // the old one goes
        let old: Vec<Id> = self.model.groups.iter().filter(|(_, g)| g.name.starts_with("Environment") && g.parent.is_none()).map(|(&k, _)| k).collect();
        for g in &old {
            self.model.groups.get_mut(g).unwrap().locked = false;
        }
        if !old.is_empty() {
            self.model.delete(&old)?;
        }
        self.model.environment = None;
        self.model.active_group = None;
        if kind == "none" {
            self.undo.push(before);
            return Ok(json!({"environment": null}));
        }
        // around what's there (or a 10 m square at the origin)
        let (lo, hi) = self.model.world_bounds().unwrap_or((DVec3::new(-5.0, -5.0, 0.0), DVec3::new(5.0, 5.0, 0.0)));
        let cmds = crate::environment::build(kind, lo, hi)?;
        let title = crate::environment::KINDS.iter().find(|k| k.0 == kind).map(|k| k.1.split(':').next().unwrap_or(kind)).unwrap_or(kind);
        let outer = self.model.begin_group(format!("Environment: {title}"));
        let saved_names = std::mem::take(&mut self.names);
        for c in cmds {
            let r = self.exec_json(&c.to_string());
            if r["ok"] != true {
                self.names = saved_names;
                return fail(self, format!("building the {kind} environment: {}", r["error"]));
            }
            // pieces made by end_group step back out to the environment group
            if self.model.active_group.is_none() {
                self.model.active_group = Some(outer);
            }
        }
        self.names = saved_names;
        self.model.active_group = None;
        // placed assets sit inside it too
        let loose: Vec<Id> = self.model.instances.iter().filter(|(k, i)| i.parent.is_none() && !before.instances.contains_key(k)).map(|(&k, _)| k).collect();
        self.model.set_parent(&loose, Some(outer))?;
        self.model.set_tag(&[outer], Some("Environment".into()))?;
        self.model.tags.entry("Environment".into()).or_insert(true);
        self.model.groups.get_mut(&outer).unwrap().locked = true;
        self.model.environment = Some(kind.to_string());
        // all of it is one undo step
        self.undo.truncate(depth);
        self.undo.push(before);
        self.redo.clear();
        Ok(json!({"environment": kind, "group": outer}))
    }

    fn resolve(&self, v: &mut Value) -> Result<()> {
        match v {
            Value::String(s) if s.starts_with('@') => {
                let Some(id) = self.names.get(&s[1..]) else { bail!("unknown name {s}") };
                *v = json!(id);
            }
            Value::Array(a) => a.iter_mut().try_for_each(|x| self.resolve(x))?,
            Value::Object(o) => o.values_mut().try_for_each(|x| self.resolve(x))?,
            _ => {}
        }
        Ok(())
    }

    pub fn exec(&mut self, cmd: Command) -> Value {
        if let Command::Environment { kind } = &cmd {
            self.revision += 1;
            return match self.environment(kind) {
                Ok(mut v) => {
                    v["ok"] = json!(true);
                    v
                }
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            };
        }
        let mutates = cmd.mutates();
        if mutates || matches!(cmd, Command::Undo | Command::Redo) {
            self.revision += 1;
        }
        let before = if mutates { Some(self.model.clone()) } else { None };
        match self.run(cmd) {
            Ok(mut v) => {
                if let Some(b) = before {
                    self.undo.push(b);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                v["ok"] = json!(true);
                v
            }
            Err(e) => {
                if let Some(b) = before {
                    self.model = b; // a failed command leaves no half-done edits
                }
                json!({"ok": false, "error": e.to_string()})
            }
        }
    }

    fn run(&mut self, cmd: Command) -> Result<Value> {
        // inside `edit`, geometry commands work on the component's definition
        if let Some(d) = self.ctx {
            if !self.model.definitions.contains_key(&d) {
                self.ctx = None;
            }
        }
        let ctx = self.ctx;
        let m: &mut Model = match ctx {
            Some(d) => std::sync::Arc::make_mut(&mut self.model.definitions.get_mut(&d).unwrap().model),
            None => &mut self.model,
        };
        Ok(match cmd {
            Command::New => {
                *m = Model::default();
                json!({})
            }
            Command::Rect { from, to } => {
                let (a, b) = (v3(from), v3(to));
                let d = b - a;
                let k = (0..3).find(|&k| d[k].abs() < 1e-9);
                let Some(k) = k else { bail!("rect corners must share one coordinate (use polygon for tilted shapes)") };
                let (i, j) = ((k + 1) % 3, (k + 2) % 3);
                if d[i].abs() < 1e-9 || d[j].abs() < 1e-9 {
                    bail!("rect has zero width");
                }
                let mut p1 = a;
                p1[i] = b[i];
                let mut p3 = a;
                p3[j] = b[j];
                let f = m.add_polygon(&[a, p1, b, p3])?;
                json!({"face": f})
            }
            Command::Polygon { points } => {
                let pts: Vec<DVec3> = points.into_iter().map(v3).collect();
                json!({"face": m.add_polygon(&pts)?})
            }
            Command::Line { points } => {
                if points.len() < 2 {
                    bail!("a line needs 2 points");
                }
                let (mut edges, mut faces) = (vec![], vec![]);
                for w in points.windows(2) {
                    let r = m.draw_line(v3(w[0]), v3(w[1]))?;
                    edges.extend(r.edges);
                    faces.extend(r.faces);
                }
                json!({"edges": edges, "faces": faces, "face": faces.last()})
            }
            Command::Circle { center, radius, normal, segments } => json!({"face": m.circle(v3(center), v3(normal), radius, segments)?}),
            Command::Arc { start, end, bulge, normal, segments } => {
                let pts = Model::arc_points(v3(start), v3(end), bulge, v3(normal), segments)?;
                let mut edges = vec![];
                let mut faces = vec![];
                for w in pts.windows(2) {
                    let r = m.draw_line(w[0], w[1])?;
                    edges.extend(r.edges);
                    faces.extend(r.faces);
                }
                json!({"edges": edges, "faces": faces, "face": faces.last()})
            }
            Command::Offset { face, distance } => json!({"face": m.offset(face, distance)?}),
            Command::FollowMe { face, points, edges, around } => {
                let (path, closed) = if let Some(p) = points {
                    let pts: Vec<DVec3> = p.into_iter().map(v3).collect();
                    let closed = pts.len() > 2 && pts[0].distance(*pts.last().unwrap()) < 1e-9;
                    (pts, closed)
                } else if let Some(es) = edges {
                    m.edge_path(&es)?
                } else if let Some(f) = around {
                    (m.pts(&m.face(f)?.outer.clone()), true)
                } else {
                    bail!("follow_me needs points, edges or around");
                };
                let faces = m.follow_me(face, &path, closed)?;
                json!({"faces": faces})
            }
            Command::Pushpull { face, distance, toward } => {
                let distance = match toward {
                    Some(t) if m.normal(face).dot(v3(t)) < 0.0 => -distance,
                    _ => distance,
                };
                let ids = m.pushpull(face, distance)?;
                json!({"face": ids[0], "created": &ids[1..]})
            }
            Command::Move { ids, by } => json!({"moved": m.translate(&ids, v3(by))?}),
            Command::Rotate { ids, center, axis, angle } => json!({"moved": m.rotate(&ids, v3(center), v3(axis), angle)?}),
            Command::Scale { ids, center, factor } => {
                let f = match factor {
                    Factor::Uniform(k) => DVec3::splat(k),
                    Factor::Axes(a) => v3(a),
                };
                json!({"moved": m.scale(&ids, v3(center), f)?})
            }
            Command::Duplicate { ids, by } => json!({"ids": m.duplicate(&ids, v3(by))?}),
            Command::Array { ids, by, count } => json!({"ids": m.array(&ids, v3(by), count)?}),
            Command::Delete { ids } => {
                m.delete(&ids)?;
                json!({})
            }
            Command::Reverse { ids } => {
                let faces = m.faces_of(&ids)?;
                for f in &faces {
                    m.faces.get_mut(f).unwrap().reverse();
                }
                json!({"faces": faces})
            }
            Command::Paint { ids, color, material } => match (material, color) {
                (Some(name), _) => {
                    m.paint_material(&ids, &name)?;
                    json!({"material": name})
                }
                (None, Some(c)) => json!({"material": m.paint(&ids, c)?}),
                _ => bail!("paint needs a color or a material"),
            },
            Command::Texture { name, data, width, height, color, project, origin } => {
                let mat = m.materials.entry(name.clone()).or_default();
                let mut tex = mat.texture.clone().unwrap_or(crate::model::Texture { data: String::new(), width: 1.0, height: 1.0, project: None, origin: None });
                if let Some(d) = data {
                    if !d.starts_with("data:image/") {
                        bail!("texture data must be a data:image/... URL");
                    }
                    tex.data = d;
                }
                if tex.data.is_empty() {
                    bail!("a new texture needs image data");
                }
                if let Some(w) = width {
                    tex.height = height.unwrap_or(tex.height * w / tex.width.max(1e-9));
                    tex.width = w;
                } else if let Some(h) = height {
                    tex.height = h;
                }
                if let Some(p) = project {
                    tex.project = p;
                }
                if let Some(o) = origin {
                    tex.origin = Some(o);
                }
                if let Some(c) = color {
                    mat.color = c;
                } else if mat.color == [0.0; 3] {
                    mat.color = [0.8, 0.8, 0.8];
                }
                mat.texture = Some(tex);
                json!({"material": name})
            }
            Command::BeginGroup { name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.begin_group(n)})
            }
            Command::EndGroup => {
                // back out one level (to the group this one sits in)
                m.active_group = m.active_group.and_then(|g| m.groups.get(&g)).and_then(|g| g.parent);
                json!({})
            }
            Command::Group { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Group {}", m.groups.len() + 1));
                json!({"group": m.group(&ids, n)?})
            }
            Command::Faces { group, facing } => {
                let dir = facing.map(|f| v3(f).normalize_or_zero());
                let list: Vec<Value> = m
                    .faces
                    .iter()
                    .filter(|(_, f)| group.is_none() || f.group == group)
                    .map(|(&id, _)| id)
                    .filter(|&id| dir.map_or(true, |d| m.normal(id).dot(d) > 0.94))
                    .map(|id| {
                        let f = &m.faces[&id];
                        let r = |v: DVec3| [round(v.x), round(v.y), round(v.z)];
                        json!({
                            "id": id,
                            "normal": r(m.normal(id)),
                            "center": r(m.center(id)),
                            "area": round(m.face_area(id)),
                            "points": f.outer.len(),
                            "holes": f.holes.len(),
                            "group": f.group,
                            "material": f.material,
                        })
                    })
                    .collect();
                json!({"faces": list})
            }
            Command::Pick { at, facing } => {
                let p = v3(at);
                let dir = facing.map(|f| v3(f).normalize_or_zero());
                let candidates: Vec<Id> = m
                    .faces
                    .keys()
                    .copied()
                    .filter(|&id| dir.map_or(true, |d| m.normal(id).dot(d) > 0.94))
                    .collect();
                let on = candidates.iter().copied().find(|&id| m.contains(id, p));
                let face = on.or_else(|| {
                    candidates.iter().copied().min_by(|a, b| {
                        m.center(*a).distance(p).partial_cmp(&m.center(*b).distance(p)).unwrap()
                    })
                });
                let Some(face) = face else { bail!("no face there") };
                json!({"face": face, "exact": on.is_some()})
            }
            Command::Copy { group, by, name } => json!({"group": m.copy_group(group, v3(by), name)?}),
            Command::MakeComponent { ids, name } => {
                let n = name.unwrap_or_else(|| format!("Component {}", m.definitions.len() + 1));
                let inst = m.make_component(&ids, n)?;
                json!({"instance": inst, "component": m.instances[&inst].def})
            }
            Command::Place { component, at, rotate, name } => {
                let key = match &component {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let Some(def) = m.find_definition(&key) else { bail!("no component {key}") };
                json!({"instance": m.place(def, v3(at), rotate, name)?})
            }
            Command::Edit { id } => {
                if m.groups.contains_key(&id) {
                    m.active_group = Some(id); // new geometry and lines go inside the group
                    return Ok(json!({"editing_group": id}));
                }
                let Some(i) = self.model.instances.get(&id) else { bail!("{id} is not a group or component placement") };
                self.ctx = Some(i.def);
                json!({"editing": i.def})
            }
            Command::Close => {
                // out of a component; or out of a group, one level at a time
                if self.ctx.is_some() {
                    self.ctx = None;
                } else {
                    m.active_group = m.active_group.and_then(|g| m.groups.get(&g)).and_then(|g| g.parent);
                }
                json!({})
            }
            Command::Explode { id } => json!({"group": m.explode(id)?}),
            Command::Components => json!({"components": m.definitions.iter().map(|(id, d)| json!({
                "id": id, "name": d.name, "faces": d.model.faces.len(),
                "placed": m.instances.values().filter(|i| i.def == *id).count(),
            })).collect::<Vec<_>>()}),
            Command::Hide { ids } => {
                m.set_hidden(&ids, true)?;
                json!({})
            }
            Command::Unhide { ids } => {
                if ids.is_empty() { m.unhide_all() } else { m.set_hidden(&ids, false)? }
                json!({})
            }
            Command::Tag { ids, tag } => {
                m.set_tag(&ids, tag)?;
                json!({})
            }
            Command::TagVisible { tag, visible } => {
                m.tags.insert(tag, visible);
                json!({})
            }
            Command::TagColor { tag, color } => {
                match color {
                    Some(c) => m.tag_colors.insert(tag, c),
                    None => m.tag_colors.remove(&tag),
                };
                json!({})
            }
            Command::RenameTag { tag, to } => {
                let to = to.trim().to_string();
                if to.is_empty() {
                    bail!("a tag needs a name");
                }
                if let Some(v) = m.tags.remove(&tag) {
                    m.tags.insert(to.clone(), v);
                }
                if let Some(c) = m.tag_colors.remove(&tag) {
                    m.tag_colors.insert(to.clone(), c);
                }
                for g in m.groups.values_mut().filter(|g| g.tag.as_deref() == Some(tag.as_str())) {
                    g.tag = Some(to.clone());
                }
                for i in m.instances.values_mut().filter(|i| i.tag.as_deref() == Some(tag.as_str())) {
                    i.tag = Some(to.clone());
                }
                json!({"tag": to})
            }
            Command::SetParent { ids, parent } => {
                m.set_parent(&ids, parent)?;
                json!({})
            }
            Command::Ungroup { id } => {
                m.check_unlocked(&[id])?;
                m.ungroup(id)?;
                json!({})
            }
            Command::Lock { ids, locked } => {
                for id in &ids {
                    let Some(g) = m.groups.get_mut(id) else { bail!("{id} is not a group") };
                    g.locked = locked;
                }
                json!({})
            }
            Command::Soften { ids, soft, coplanar } => {
                let n = if coplanar { m.soften_coplanar() } else { m.soften(&ids, soft)? };
                json!({"edges": n})
            }
            Command::Rename { id, name } => {
                if let Some(g) = m.groups.get_mut(&id) {
                    g.name = name;
                } else if let Some(i) = m.instances.get_mut(&id) {
                    i.name = name;
                } else if let Some(d) = m.definitions.get_mut(&id) {
                    d.name = name;
                } else {
                    bail!("{id} can't be renamed (groups and components only)");
                }
                json!({})
            }
            Command::Guide { point, direction } => {
                let id = m.next_id + 1;
                m.next_id = id;
                m.guides.push(crate::model::Guide { id, p: v3(point), dir: direction.map(|d| v3(d).normalize()) });
                json!({"guide": id})
            }
            Command::Dimension { from, to, offset } => {
                let id = m.next_id + 1;
                m.next_id = id;
                let (a, b) = (v3(from), v3(to));
                m.annotations.push(crate::model::Annotation::Dimension { id, a, b, offset: v3(offset) });
                json!({"dimension": id, "length": round(a.distance(b))})
            }
            Command::Text { at, text, leader } => {
                let id = m.next_id + 1;
                m.next_id = id;
                m.annotations.push(crate::model::Annotation::Text { id, at: v3(at), text, leader: leader.map(v3) });
                json!({"text": id})
            }
            Command::Units { units } => {
                if !["m", "cm", "mm", "ft", "in"].contains(&units.as_str()) {
                    bail!("units must be m, cm, mm, ft or in");
                }
                self.model.units = units.clone();
                json!({"units": units})
            }
            Command::ClearGuides => {
                m.guides.clear();
                json!({})
            }
            Command::SaveView { name, eye, target, fov } => {
                m.views.retain(|v| v.name != name);
                // re-saving a matched photo's scene keeps its photo
                let photo = m.views.iter().find(|v| v.name == name).and_then(|v| v.photo.clone());
                m.views.retain(|v| v.name != name);
                m.views.push(crate::model::View { name, eye, target, fov, photo });
                json!({})
            }
            Command::MatchPhoto { name, width, height, red, green, origin, distance, photo } => {
                let cam = crate::matchphoto::solve(width, height, red, green, origin, distance)?;
                if let Some(p) = &photo {
                    if !p.starts_with("data:image/") {
                        bail!("photo must be a data:image/... URL");
                    }
                }
                m.views.retain(|v| v.name != name);
                m.views.push(crate::model::View { name: name.clone(), eye: cam.eye.to_array(), target: cam.target.to_array(), fov: cam.fov, photo });
                json!({"view": name, "eye": cam.eye.to_array(), "target": cam.target.to_array(), "fov": cam.fov})
            }
            Command::DeleteView { name } => {
                m.views.retain(|v| v.name != name);
                json!({})
            }
            Command::Info => {
                let b = m.world_bounds().map(|(lo, hi)| json!({"min": lo.to_array(), "max": hi.to_array()}));
                json!({
                    "vertices": m.vertices.len(),
                    "edges": m.edges.len(),
                    "faces": m.faces.len(),
                    "groups": m.groups.iter().map(|(id, g)| json!({"id": id, "name": g.name})).collect::<Vec<_>>(),
                    "materials": m.materials.keys().collect::<Vec<_>>(),
                    "active_group": m.active_group,
                    "editing": ctx,
                    "instances": m.instances.iter().map(|(id, i)| json!({"id": id, "name": i.name, "component": i.def})).collect::<Vec<_>>(),
                    "tags": &m.tags,
                    "views": m.views.iter().map(|v| &v.name).collect::<Vec<_>>(),
                    "bounds": b,
                })
            }
            Command::Undo => {
                let Some(prev) = self.undo.pop() else { bail!("nothing to undo") };
                self.redo.push(std::mem::replace(&mut self.model, prev));
                json!({})
            }
            Command::Redo => {
                let Some(next) = self.redo.pop() else { bail!("nothing to redo") };
                self.undo.push(std::mem::replace(&mut self.model, next));
                json!({})
            }
            Command::Photoreal { out, samples, bounces, mut sun_azimuth, mut sun_altitude, sun_strength, denoise, exposure, date, time, utc_offset, progressive, opts } => {
                let model = &self.model;
                let mut sun_note = None;
                if date.is_some() || time.is_some() {
                    let s = sun_at(model, date.as_deref(), time.as_deref(), utc_offset)?;
                    if s.altitude < 1.0 {
                        bail!("the sun is down then ({:.0}° below the horizon); pick a daytime hour", -s.altitude);
                    }
                    sun_azimuth = Some(s.azimuth);
                    sun_altitude = s.altitude;
                    sun_note = Some(serde_json::to_value(&s)?);
                }
                let camera = crate::pt::camera(model, &opts);
                let po = crate::pt::PtOpts {
                    width: opts.width.min(4096),
                    height: opts.height.min(4096),
                    samples: samples.clamp(1, 4096),
                    bounces: bounces.clamp(1, 16),
                    camera: camera.clone(),
                    sun: crate::pt::sun_dir(
                        sun_azimuth.unwrap_or_else(|| {
                            // three-quarter light: over the camera's right shoulder, so the face you see is lit and shadows fall beside it
                            let (e, t) = (glam::DVec3::from(camera.eye), glam::DVec3::from(camera.target));
                            let back = e - t;
                            let az = back.x.atan2(back.y).to_degrees();
                            az + 45.0
                        }),
                        sun_altitude,
                    ),
                    sun_strength,
                    denoise,
                    exposure,
                };
                let t0 = std::time::Instant::now();
                let mut spp = po.samples;
                let img = if progressive {
                    // same picture as last time? keep adding to it
                    let key = format!("{}|{:?}|{:?}|{}|{}|{}|{}|{}", self.revision, po.camera.eye, po.camera.target, po.camera.fov, po.width, po.height, po.sun, po.bounces) + &format!("|{}|{}", po.sun_strength, po.exposure);
                    let (p, img) = crate::pt::progressive(self.pt.take(), key, model, &po);
                    spp = p.spp;
                    self.pt = Some(p);
                    img
                } else {
                    crate::pt::render(model, &po)
                };
                let secs = t0.elapsed().as_secs_f64();
                let data = if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    img.save(path)?;
                    None
                } else {
                    use base64::Engine as _;
                    let mut buf = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut buf, image::ImageFormat::Png)?;
                    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
                };
                json!({"out": out, "data": data, "seconds": secs, "spp": spp, "sun": sun_note, "camera": {"eye": camera.eye, "target": camera.target, "fov": camera.fov}})
            }
            Command::Render { out, opts } => {
                let whole = &self.model;
                let cam = render::camera_for(whole, &opts);
                if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    render::render(whole, &opts).save(path)?;
                }
                json!({"out": out, "camera": {"eye": cam.eye, "target": cam.target, "fov": cam.fov}})
            }
            Command::Save { path } => {
                std::fs::write(&path, serde_json::to_string(&*m)?)?;
                json!({"path": path})
            }
            Command::Open { path } => {
                *m = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                json!({"path": path, "faces": m.faces.len()})
            }
            Command::ExportGlb { path } => json!({"path": path, "triangles": export::write_glb(&self.model, &path)?}),
            Command::ExportStl { path, scale } => json!({"path": path, "triangles": export::write_stl(&self.model, &path, scale)?}),
            Command::Animate { path, fps, transition, delay, views, turntable, opts } => {
                let a = crate::animate::Anim { fps, transition, delay, views, turntable };
                json!({"path": path, "frames": crate::animate::animate(&self.model, &path, &a, &opts)?})
            }
            Command::Blueprint { path, sheet, project, notes } => {
                let project = project.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into()));
                let building = crate::environment::without(&self.model);
                json!({"path": path, "pages": crate::layout::blueprint(&building, &path, &sheet, &project, &notes)?, "levels": crate::layout::floor_levels(&building)})
            }
            Command::Levels => json!({"levels": crate::layout::floor_levels(&crate::environment::without(&self.model))}),
            Command::Subd { name, points, faces, materials, creases, levels } => {
                let n = faces.len();
                let cage = crate::subd::Cage {
                    points: points.into_iter().map(v3).collect(),
                    faces,
                    materials: (0..n).map(|i| materials.get(i).cloned().flatten()).collect(),
                    creases: creases.into_iter().map(|[a, b]| (a.min(b), a.max(b))).collect(),
                };
                let g = crate::subd::build(m, cage, levels, &name.unwrap_or_else(|| "Smooth surface".into()))?;
                json!({"group": g, "faces": m.faces.values().filter(|f| f.group == Some(g)).count()})
            }
            Command::Subdivide { id, levels, keep_sharp } => {
                let Some(old) = m.groups.get(&id).cloned() else { bail!("{id} is not a group") };
                m.check_unlocked(&[id])?;
                let cage = crate::subd::cage_of(m, id, keep_sharp)?;
                m.delete(&[id])?;
                let prev = m.active_group;
                m.active_group = old.parent;
                let g = crate::subd::build(m, cage, levels, &old.name)?;
                m.active_group = prev;
                if let Some(x) = m.groups.get_mut(&g) {
                    x.parent = old.parent;
                    x.tag = old.tag;
                }
                json!({"group": g, "faces": m.faces.values().filter(|f| f.group == Some(g)).count()})
            }
            Command::PhotoSolve { points, edges, fixed, scale, place, build, square } => {
                let mut sol = crate::photomodel::solve(m, &points, fixed.as_deref(), scale.map(|s| (s.a, s.b, s.length)))?;
                let squared = if square { crate::photomodel::square(&mut sol.points, &edges, 4.0) } else { 0 };
                // each photo's camera slides to its solved distance (same direction, same lens)
                for v in m.views.iter_mut() {
                    if let Some(d) = sol.distances.get(&v.name) {
                        let eye = DVec3::from(v.eye);
                        let moved = eye.normalize_or_zero() * *d;
                        v.target = (DVec3::from(v.target) + (moved - eye)).to_array();
                        v.eye = moved.to_array();
                    }
                }
                m.photo_points = points;
                m.photo_edges = edges.clone();
                if place {
                    let old = std::mem::take(&mut m.photo_guides);
                    m.guides.retain(|g| !old.contains(&g.id));
                    for p in sol.points.iter().flatten() {
                        let id = m.new_id();
                        m.guides.push(crate::model::Guide { id, p: DVec3::from(*p), dir: None });
                        m.photo_guides.push(id);
                    }
                }
                let mut built = 0;
                if build {
                    for [a, b] in &edges {
                        if let (Some(Some(p)), Some(Some(q))) = (sol.points.get(*a), sol.points.get(*b)) {
                            m.draw_line(DVec3::from(*p), DVec3::from(*q))?;
                            built += 1;
                        }
                    }
                }
                let mut v = serde_json::to_value(&sol)?;
                v["built"] = json!(built);
                v["squared"] = json!(squared);
                v
            }
            Command::Environment { .. } => bail!("environment runs through Engine::exec"),
            Command::Sun { date, time, utc_offset } => serde_json::to_value(sun_at(&self.model, date.as_deref(), time.as_deref(), utc_offset)?)?,
            Command::DoorSwing { hinge, strike, toward } => {
                let (h, s, t) = (v3(hinge), v3(strike), v3(toward));
                let across = DVec3::new(s.x - h.x, s.y - h.y, 0.0);
                let w = across.length();
                if w < 0.1 {
                    bail!("the hinge and strike points are the same");
                }
                // open: square to the opening, on the side `toward` is
                let mut perp = DVec3::Z.cross(across).normalize();
                if (t - h).dot(perp) < 0.0 {
                    perp = -perp;
                }
                let id = m.new_id();
                m.annotations.push(crate::model::Annotation::Swing { id, hinge: h, from: s, to: h + perp * w });
                json!({"swing": id})
            }
            Command::Sheets { path, sheet, project, pages, blueprint, notes } => {
                let project = project.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into()));
                json!({"path": path, "pages": crate::layout::write_sheets(&self.model, &path, &sheet, &project, pages, blueprint, &notes)?})
            }
            Command::ExportIfc { path, project } => {
                let project = project.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into()));
                json!({"path": path, "elements": crate::ifc::write_ifc(&self.model, &path, &project)?})
            }
            Command::ExportDae { path } => json!({"path": path, "triangles": crate::export2::write_dae(&self.model, &path)?}),
            Command::ExportDxf { path } => json!({"path": path, "triangles": crate::export2::write_dxf(&self.model, &path)?}),
            Command::ExportPdf { path, title, opts } => {
                let img = render::render(&self.model, &opts);
                let t = title.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                crate::export2::write_pdf(&img, &format!("{t}  -  SketchCraft"), &path)?;
                json!({"path": path})
            }
            Command::ImportObj { path, text, up, scale, at, name } => {
                let text = match (text, &path) {
                    (Some(t), _) => t,
                    (None, Some(p)) => std::fs::read_to_string(p)?,
                    _ => bail!("import_obj needs a path or text"),
                };
                let p = path.as_deref().map(std::path::Path::new);
                let label = name.or_else(|| p.and_then(|p| p.file_stem()).map(|s| tidy_name(&s.to_string_lossy()))).unwrap_or_else(|| "Imported".into());
                let r = crate::import::import_obj(m, &text, p.and_then(|p| p.parent()), &label, up.eq_ignore_ascii_case("y"), scale, v3(at))?;
                json!({"instance": r.instance, "component": r.component, "groups": r.groups, "faces": r.faces, "skipped": r.skipped})
            }
            Command::ImportSkp { path, data, at, name } => {
                use base64::Engine as _;
                let bytes = match (data, &path) {
                    (Some(b64), _) => base64::engine::general_purpose::STANDARD.decode(b64.trim())?,
                    (None, Some(p)) => std::fs::read(p)?,
                    _ => bail!("import_skp needs a path or data"),
                };
                let label = name.unwrap_or_else(|| {
                    path.as_deref()
                        .and_then(|p| std::path::Path::new(p).file_stem())
                        .map(|s| crate::library::nice_name(&s.to_string_lossy()))
                        .unwrap_or_else(|| "SketchUp model".into())
                });
                let r = crate::skp::import(m, &bytes, v3(at), &label)?;
                json!({"group": r.group, "version": r.version, "faces": r.faces, "components": r.components, "placed": r.placed})
            }
            Command::Section { origin, normal, path } => {
                let cut = m.section(v3(origin), v3(normal))?;
                if let Some(p) = &path {
                    let text = if p.to_ascii_lowercase().ends_with(".dxf") { cut.dxf() } else { cut.svg() };
                    std::fs::write(p, text)?;
                }
                // fills as triangles, ready to draw
                let mut fill: Vec<f64> = Vec::new();
                let mut owners: Vec<Id> = Vec::new();
                for (owner, o, h) in &cut.regions {
                    for t in crate::geom::triangulate(o, h, cut.n) {
                        fill.extend(t.iter().flat_map(|p| [p.x, p.y, p.z]));
                        owners.push(*owner);
                    }
                }
                let lines: Vec<f64> = cut.segments.iter().flat_map(|(a, b)| [a.x, a.y, a.z, b.x, b.y, b.z]).collect();
                json!({"regions": cut.regions.len(), "fill": fill, "fillOwners": owners, "lines": lines, "path": path})
            }
            Command::TerrainFromContours { ids, spacing } => json!({"group": m.terrain_from_contours(&ids, spacing)?}),
            Command::TerrainGrid { at, width, depth, spacing } => json!({"group": m.terrain_grid(v3(at), width, depth, spacing)?}),
            Command::Smoove { group, at, radius, height } => json!({"moved": m.smoove(group, v3(at), radius, height)?}),
            Command::Stamp { group, face, offset } => json!({"moved": m.stamp(group, face, offset)?}),
            Command::Drape { group, ids } => json!({"lines": m.drape(group, &ids)?}),
            Command::FlipEdge { edge } => {
                m.flip_edge(edge)?;
                json!({})
            }
            Command::SetAttrs { id, attrs } => {
                let target = if let Some(i) = m.instances.get_mut(&id) {
                    &mut i.attrs
                } else if let Some(d) = m.definitions.get_mut(&id) {
                    &mut d.attrs
                } else {
                    bail!("{id} is not a component placement or a component");
                };
                for (k, v) in attrs {
                    let k = k.trim().to_ascii_lowercase();
                    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        bail!("attribute names are letters, digits and _ ({k:?})");
                    }
                    if v.trim().is_empty() {
                        target.remove(&k);
                    } else {
                        target.insert(k, v.trim().to_string());
                    }
                }
                // check every formula still works out
                let (inst, def) = match m.instances.get(&id) {
                    Some(i) => (Some(i.clone()), m.definitions.get(&i.def)),
                    None => (None, m.definitions.get(&id)),
                };
                let all = match &inst {
                    Some(i) => crate::dynamic::attrs_of(i, def),
                    None => def.map(|d| d.attrs.clone()).unwrap_or_default(),
                };
                let vals = crate::dynamic::resolve(&all, &Default::default());
                for (k, v) in &all {
                    if v.starts_with('=') && !vals.contains_key(k) && !v.contains("parent!") {
                        let err = crate::dynamic::eval(v, &|n| vals.get(n).copied().ok_or_else(|| anyhow::anyhow!("no attribute {n}"))).err();
                        bail!("{k}: {}", err.map(|e| e.to_string()).unwrap_or_else(|| "can't be worked out".into()));
                    }
                }
                json!({"attrs": all, "values": vals})
            }
            Command::Report { path } => {
                let rows = crate::dynamic::report(m);
                if let Some(p) = &path {
                    std::fs::write(p, crate::dynamic::report_csv(&rows))?;
                }
                json!({"rows": rows, "path": path})
            }
            Command::Classify { ids, class } => {
                let class = class.trim().to_string();
                if !class.is_empty() && !crate::ifc::CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&class)) {
                    bail!("unknown IFC type {class:?}; one of {}", crate::ifc::CLASSES.join(", "));
                }
                let class = crate::ifc::CLASSES.iter().find(|c| c.eq_ignore_ascii_case(&class)).map(|c| c.to_string());
                for id in &ids {
                    if !m.groups.contains_key(id) && !m.instances.contains_key(id) {
                        bail!("{id} is not a group or component");
                    }
                    match &class {
                        Some(c) => m.classes.insert(*id, c.clone()),
                        None => m.classes.remove(id),
                    };
                }
                json!({"classified": ids.len()})
            }
            Command::AddLocation { lat, lon, size, cells, imagery } => {
                let p = crate::geo::add_location(m, lat, lon, size, cells, imagery, &crate::geo::fetch)?;
                json!({"group": p.group, "elevation": p.elevation, "zoom": p.zoom})
            }
            Command::Library { dir } => {
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                std::fs::create_dir_all(&d).ok();
                json!({"dir": d.to_string_lossy(), "entries": crate::library::list(&d)})
            }
            Command::Thumbnail { component, asset, path, size } => {
                use base64::Engine as _;
                let px = size.clamp(32, 512);
                let png = if let Some(c) = component {
                    crate::library::thumbnail_of(m, c, px)?
                } else if let Some(a) = asset {
                    let mut scratch = Model::default();
                    let id = crate::assets::definition(&mut scratch, &a)?;
                    crate::library::thumbnail_of(&scratch, id, px)?
                } else if let Some(p) = path {
                    crate::library::thumbnail_of_file(&p, px)?
                } else {
                    bail!("thumbnail needs a component, asset or path");
                };
                json!({"data": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))})
            }
            Command::InsertFile { path, at, rotate } => {
                let def = crate::library::definition_for(m, &path)?;
                json!({"instance": m.place(def, v3(at), rotate, None)?, "component": def})
            }
            Command::SaveToLibrary { component, name, collection, dir } => {
                let model = crate::library::standalone(m, component)?;
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                let folder = match collection.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                    Some(c) => d.join(crate::library::safe(c)),
                    None => d,
                };
                std::fs::create_dir_all(&folder)?;
                let base = crate::library::safe(&name.unwrap_or_else(|| m.definitions[&component].name.clone()));
                let mut path = folder.join(format!("{base}.sketchcraft"));
                let mut k = 2;
                while path.exists() {
                    path = folder.join(format!("{base} {k}.sketchcraft"));
                    k += 1;
                }
                std::fs::write(&path, serde_json::to_string(&model)?)?;
                json!({"path": path.to_string_lossy()})
            }
            Command::LibraryAdd { paths, collection, dir } => {
                let d = dir.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                json!({"added": crate::library::add_files(&d, &paths, collection.as_deref().unwrap_or(""))?})
            }
            Command::Purge => json!({"removed": crate::library::purge(m)}),
            Command::Skins => {
                let dir = skins_dir();
                let mut out = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("sketchskin")).collect();
                    files.sort();
                    for p in files.into_iter().take(200) {
                        // small JSON files only
                        if std::fs::metadata(&p).map(|m| m.len() > 64 * 1024).unwrap_or(true) {
                            continue;
                        }
                        if let Some(skin) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
                            out.push(json!({"path": p.to_string_lossy(), "name": p.file_stem().map(|s| s.to_string_lossy().to_string()), "skin": skin}));
                        }
                    }
                }
                json!({"dir": dir.to_string_lossy(), "skins": out})
            }
            Command::SaveSkin { name, skin } => {
                let dir = skins_dir();
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(format!("{}.sketchskin", crate::library::safe(&name)));
                std::fs::write(&path, serde_json::to_string_pretty(&skin)?)?;
                json!({"path": path.to_string_lossy()})
            }
            Command::Reveal { path } => {
                let p = path.map(std::path::PathBuf::from).unwrap_or_else(crate::library::default_dir);
                std::fs::create_dir_all(if p.extension().is_some() { p.parent().unwrap_or(&p) } else { &p }).ok();
                #[cfg(target_os = "macos")]
                let r = if p.is_dir() { std::process::Command::new("open").arg(&p).status() } else { std::process::Command::new("open").arg("-R").arg(&p).status() };
                #[cfg(target_os = "windows")]
                let r = std::process::Command::new("explorer").arg(&p).status();
                #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                let r = std::process::Command::new("xdg-open").arg(if p.is_dir() { &p } else { p.parent().unwrap_or(&p) }).status();
                anyhow::Context::context(r, "couldn't open the file browser")?;
                json!({"path": p.to_string_lossy()})
            }
            Command::Bevel { edges, size, segments } => {
                let groups = m.bevel(&edges, size, segments)?;
                json!({"groups": groups, "group": groups.first()})
            }
            Command::Divide { edge, count } => json!({"points": m.divide(edge, count)?}),
            Command::Slice { id, origin, normal } => {
                let groups = m.slice(id, v3(origin), v3(normal))?;
                json!({"groups": groups, "group": groups.first()})
            }
            Command::Material { name, color } => {
                let mat = m.materials.entry(name.clone()).or_default();
                mat.color = color;
                json!({"material": name})
            }
            Command::SaveImage { path, data } => {
                use base64::Engine as _;
                let b64 = data.strip_prefix("data:image/png;base64,").ok_or_else(|| anyhow::anyhow!("expected a PNG data URL"))?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                std::fs::write(&path, bytes)?;
                json!({"path": path})
            }
            Command::Intersect { ids, with } => json!({"lines": m.intersect(&ids, with != "selection")?}),
            Command::Flip { ids, axis } => json!({"moved": m.flip(&ids, axis_index(&axis)?)?}),
            Command::Solid { op, ids } => {
                let groups = crate::csg::solid(m, &op, &ids)?;
                json!({"groups": groups, "group": groups.first()})
            }
            // on a scratch copy: checking adds paint entries it doesn't need to keep
            Command::Volume { id } => match crate::csg::solid_info(&mut m.clone(), id) {
                Some(v) => json!({"solid": true, "volume": v}),
                None => json!({"solid": false}),
            },
            Command::ImportFbx { path, data, at, name } => {
                use base64::Engine as _;
                let bytes = match (data, &path) {
                    (Some(b64), _) => base64::engine::general_purpose::STANDARD.decode(b64.trim())?,
                    (None, Some(p)) => std::fs::read(p)?,
                    _ => bail!("import_fbx needs a path or data"),
                };
                let label = name.or_else(|| path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|s| tidy_name(&s.to_string_lossy()))).unwrap_or_else(|| "FBX model".into());
                let r = crate::fbx::import(m, &bytes, &label, v3(at))?;
                json!({"instance": r.instance, "component": r.component, "meshes": r.meshes, "faces": r.faces})
            }
            Command::Wall { points, thickness, height, closed } => {
                let pts: Vec<DVec3> = points.into_iter().map(v3).collect();
                json!({"group": m.wall(&pts, thickness, height, closed)?})
            }
            Command::Stairs { at, rise, direction, width, tread } => {
                let (g, risers) = m.stairs(v3(at), direction, width, rise, tread)?;
                json!({"group": g, "risers": risers, "riser_height": rise / risers as f64})
            }
            Command::Roof { face, kind, pitch, overhang, thickness } => json!({"group": m.roof(face, &kind, pitch, overhang, thickness)?}),
            Command::Column { at, size, height, round } => json!({"group": m.column(v3(at), size, height, round)?}),
            Command::Insert { asset, at, rotate } => {
                let def = crate::assets::definition(m, &asset)?;
                json!({"instance": m.place(def, v3(at), rotate, None)?, "component": def})
            }
            Command::Assets => json!({"assets": crate::assets::ASSETS.iter().map(|a| json!({"key": a.key, "name": a.name, "about": a.about})).collect::<Vec<_>>()}),
            Command::ExportObj { path } => {
                let tris = export::write_obj(&self.model, &path)?;
                json!({"path": path, "triangles": tris})
            }
        })
    }
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A readable name from a download's file name: "uploads_files_2787791_Mercedes+Benz+GLS+580"
/// -> "Mercedes Benz GLS 580".
fn tidy_name(stem: &str) -> String {
    let s = stem.replace(['+', '_'], " ");
    let words: Vec<&str> = s.split_whitespace().collect();
    // drop store prefixes like "uploads files 2787791"
    let start = words.iter().position(|w| !matches!(w.to_ascii_lowercase().as_str(), "uploads" | "files" | "upload" | "file") && !w.chars().all(|c| c.is_ascii_digit())).unwrap_or(0);
    let name = words[start..].join(" ");
    if name.is_empty() { stem.to_string() } else { name }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnownLength {
    pub a: usize,
    pub b: usize,
    pub length: f64,
}

#[derive(serde::Serialize)]
struct SunAt {
    azimuth: f64,
    altitude: f64,
    lat: f64,
    lon: f64,
    /// false: no location set, so a default (40 N, 98 W) was used
    located: bool,
    utc_offset: f64,
}

/// The real sun for the model's location at a local date ("2026-06-21") and time ("15:30").
fn sun_at(model: &Model, date: Option<&str>, time: Option<&str>, utc_offset: Option<f64>) -> Result<SunAt> {
    let located = model.location.is_some();
    let (lat, lon) = model.location.as_ref().map(|l| (l.lat, l.lon)).unwrap_or((40.0, -98.0));
    let parts: Vec<&str> = date.unwrap_or("2026-06-21").split('-').collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.trim().parse::<i64>().ok());
    let (Some(y), Some(mo), Some(da)) = (num(0), num(1), num(2)) else { bail!("date is year-month-day, like 2026-06-21") };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        bail!("date is year-month-day, like 2026-06-21");
    }
    let hm: Vec<&str> = time.unwrap_or("15:00").split(':').collect();
    let Some(h) = hm.first().and_then(|s| s.trim().parse::<f64>().ok()) else { bail!("time is hours:minutes, like 15:30") };
    let m = hm.get(1).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let utc_offset = utc_offset.unwrap_or((lon / 15.0).round());
    let (azimuth, altitude) = crate::pt::solar_position(lat, lon, y as i32, mo as u32, da as u32, h + m / 60.0, utc_offset);
    Ok(SunAt { azimuth, altitude, lat, lon, located, utc_offset })
}

/// Where app skins live: ~/Documents/SketchCraft/Skins.
fn skins_dir() -> std::path::PathBuf {
    crate::library::default_dir().parent().map(|p| p.join("Skins")).unwrap_or_else(|| std::path::PathBuf::from("Skins"))
}
