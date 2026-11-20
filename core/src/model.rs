//! The model: vertices, edges and faces that share vertices (so a box is one
//! connected solid, not six loose squares), plus groups and materials.
//!
//! Rules that make it feel like a push/pull modeler:
//!  - Points closer than WELD snap to the same vertex (within one group).
//!  - A face drawn inside an existing coplanar face cuts a hole in it, so the
//!    new face can be pushed in (window) or pulled out (bay).
//!  - Push/pull on a face whose sides already stand on it just moves it
//!    (pull the top of a box higher); otherwise it extrudes new sides.

use crate::geom;
use anyhow::{anyhow, bail, Result};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub type Id = u32;
pub const WELD: f64 = 1e-6;
const PLANAR: f64 = 1e-5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub p: DVec3,
    #[serde(default)]
    pub group: Option<Id>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Edge {
    pub a: Id,
    pub b: Id,
    #[serde(default)]
    pub group: Option<Id>,
    /// Soft edges are drawn only in X-ray / hidden-line debug: the seams of a
    /// curved surface, or the splits in a long street.
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft: bool,
    /// edges of one arc, circle or welded curve share an id: picked, offset
    /// and followed as one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<Id>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// this edge's own line weight and style on drawing sheets; beats its tag's style. Boxed to keep `Edge` small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<Box<crate::drafting::EdgeDraft>>,
    /// a UV seam: unwrapping cuts the surface along it (see docs/dev/uv-editor.md)
    #[serde(default, skip_serializing_if = "is_false")]
    pub seam: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Face {
    /// Counter-clockwise about the face normal.
    pub outer: Vec<Id>,
    /// Clockwise about the face normal.
    #[serde(default)]
    pub holes: Vec<Vec<Id>>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub group: Option<Id>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// where this face's picture sits when it was painted through from a neighbour (none: the material's own rule)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tex: Option<TexPlace>,
    /// where the face's texture lands, per corner (set by projection mapping); None: the material's own placement
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv: Option<FaceUv>,
    /// vertex colours per corner (set by vertex painting, RGBA 0..1); None: the face shows only its material
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcol: Option<FaceColors>,
    /// the second texture-coordinate set (the lightmap channel): per corner, in 0..1. Exported, never drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv2: Option<FaceUv>,
}

/// A picture's placement on one face: unit axes lying in the face plane and the point the image corner sits on.
/// Used only while the face still wears `material` and the axes still lie in its plane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TexPlace {
    pub material: String,
    #[serde(deserialize_with = "lenient::de3")]
    pub u: [f64; 3],
    #[serde(deserialize_with = "lenient::de3")]
    pub v: [f64; 3],
    #[serde(deserialize_with = "lenient::de3")]
    pub origin: [f64; 3],
}

impl TexPlace {
    /// Every number finite and within `MAX_COORD` (a damaged file may hold others; see `Model::vet_face_data`).
    pub fn sane(&self) -> bool {
        self.u.iter().chain(&self.v).chain(&self.origin).all(|&x| sane(x))
    }
}

/// Per-corner data (texture coordinates, vertex colours) is kept as f32: a value the model can hold is finite and
/// within `MAX_COORD`.
pub fn sane32(v: f32) -> bool {
    v.is_finite() && (v as f64).abs() <= MAX_COORD
}

/// A texture coordinate worked out in f64, as the model keeps it (f32). Refused when it isn't a number or is out of
/// range (a tiny projection scale or a huge transform can make one), so a model never holds one.
pub fn uv32(p: [f64; 2]) -> Result<[f32; 2]> {
    let q = [p[0] as f32, p[1] as f32];
    if !sane32(q[0]) || !sane32(q[1]) {
        bail!("that would put texture coordinates out of range (not a number, or more than {MAX_COORD:e} tiles), so it was not done");
    }
    Ok(q)
}

/// `uv32` for a whole loop of corners.
pub fn uv32_ring(r: &[[f64; 2]]) -> Result<Vec<[f32; 2]>> {
    r.iter().map(|p| uv32(*p)).collect()
}

/// Reading per-corner numbers from a file: a `null` (what JSON holds for a number that isn't one) is read as NaN instead
/// of failing the whole file, so `Model::vet_face_data` can leave out just that face's data, with a notice.
mod lenient {
    use serde::{Deserialize, Deserializer};

    fn nan_for_null<const N: usize>(a: [Option<f64>; N]) -> [f64; N] {
        a.map(|x| x.unwrap_or(f64::NAN))
    }

    fn f32s<const N: usize>(a: [Option<f64>; N]) -> [f32; N] {
        nan_for_null(a).map(|x| x as f32)
    }

    pub fn de3<'de, D: Deserializer<'de>>(d: D) -> Result<[f64; 3], D::Error> {
        Ok(nan_for_null(<[Option<f64>; 3]>::deserialize(d)?))
    }

    pub fn ring2<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<[f32; 2]>, D::Error> {
        Ok(Vec::<[Option<f64>; 2]>::deserialize(d)?.into_iter().map(f32s).collect())
    }

    pub fn rings2<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<[f32; 2]>>, D::Error> {
        Ok(Vec::<Vec<[Option<f64>; 2]>>::deserialize(d)?.into_iter().map(|r| r.into_iter().map(f32s).collect()).collect())
    }

    pub fn ring4<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<[f32; 4]>, D::Error> {
        Ok(Vec::<[Option<f64>; 4]>::deserialize(d)?.into_iter().map(f32s).collect())
    }

    pub fn rings4<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<[f32; 4]>>, D::Error> {
        Ok(Vec::<Vec<[Option<f64>; 4]>>::deserialize(d)?.into_iter().map(|r| r.into_iter().map(f32s).collect()).collect())
    }
}

/// Texture coordinates for a face's corners, in tiles (1 = one whole picture), v up the picture. One entry per vertex
/// of each loop, in the loop's order. A face whose loops have changed size since ignores them.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct FaceUv {
    #[serde(deserialize_with = "lenient::ring2")]
    pub outer: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient::rings2")]
    pub holes: Vec<Vec<[f32; 2]>>,
}

impl FaceUv {
    /// Every coordinate finite and within `MAX_COORD`.
    pub fn sane(&self) -> bool {
        self.outer.iter().chain(self.holes.iter().flatten()).flatten().all(|&x| sane32(x))
    }

    /// How many corners it holds.
    pub fn corners(&self) -> usize {
        self.outer.len() + self.holes.iter().map(Vec::len).sum::<usize>()
    }

    /// Do the coordinates still fit the face's loops?
    pub fn fits(&self, f: &Face) -> bool {
        self.outer.len() == f.outer.len() && self.holes.len() == f.holes.len() && self.holes.iter().zip(&f.holes).all(|(a, b)| a.len() == b.len())
    }
}

/// Vertex colours for a face's corners (RGBA, 0..1, white = no tint): one entry per vertex of each loop, in the loop's
/// order. A face whose loops have changed size since ignores them.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct FaceColors {
    #[serde(deserialize_with = "lenient::ring4")]
    pub outer: Vec<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "lenient::rings4")]
    pub holes: Vec<Vec<[f32; 4]>>,
}

impl FaceColors {
    /// Every channel finite and within `MAX_COORD`.
    pub fn sane(&self) -> bool {
        self.outer.iter().chain(self.holes.iter().flatten()).flatten().all(|&x| sane32(x))
    }

    /// How many corners it holds.
    pub fn corners(&self) -> usize {
        self.outer.len() + self.holes.iter().map(Vec::len).sum::<usize>()
    }

    /// Do the colours still fit the face's loops?
    pub fn fits(&self, f: &Face) -> bool {
        self.outer.len() == f.outer.len() && self.holes.len() == f.holes.len() && self.holes.iter().zip(&f.holes).all(|(a, b)| a.len() == b.len())
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// the group this one sits inside (None: top level)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    /// can't be moved, edited or deleted until unlocked
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

/// A model file written before 0.3.6 holds the absolute path of the library file; keep only its name,
/// so a home folder never travels with a shared model or reaches an agent.
fn de_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v: Option<String> = Option::deserialize(d)?;
    Ok(v.map(|s| match s.rfind(['/', '\\']) {
        Some(i) => s[i + 1..].to_string(),
        None => s,
    }))
}

/// A reusable piece (door, window, tree): geometry kept once, placed many
/// times. Editing the definition updates every placement.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    /// Shared, not copied, between undo steps: a 200,000-triangle import
    /// stays one copy until the component itself is edited.
    pub model: std::sync::Arc<Model>,
    /// defaults for its placements' Dynamic Component attributes
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
    /// the library file it came from, as `<file name>#<hash of its bytes>` (placing the same file again
    /// reuses it; a changed or different file with the same name doesn't). Never a folder path: an older
    /// file's absolute path is cut down to the file name when the model is read (CASHOUT-004).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "de_source")]
    pub source: Option<String>,
    /// placed on a face, it cuts an opening there (doors, windows)
    #[serde(default, skip_serializing_if = "is_false")]
    pub cuts: bool,
    /// always turns to face the camera, around its own blue axis (flat
    /// people and trees, like SketchUp's scale figure). Drawn facing -y.
    #[serde(default, skip_serializing_if = "is_false")]
    pub face_me: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub def: Id,
    pub name: String,
    pub transform: glam::DMat4,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Dynamic Component attributes: values or "=formulas" (see `dynamic`)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
    /// the group this placement sits inside (None: top level)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
}

/// A construction guide: a line (with `dir`) or a point. Snappable, never exported.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guide {
    pub id: Id,
    pub p: DVec3,
    #[serde(default)]
    pub dir: Option<DVec3>,
}

/// A note on the model: a dimension between two points (drawn `offset`
/// away from them), or a text label (with an optional leader to a point).
///
/// A dimension may be attached to geometry: `anchors[0]` holds `a` and `anchors[1]` holds `b` (#59). An attached end
/// follows the geometry (`Model::settle_anchors` moves `a`/`b` after every edit, in the same undo step), so moving a
/// wall moves the dimension and its value; a free end (None) keeps its point. Files without `anchors` (every file
/// before 0.3.8) read as two free ends, and a dimension with none is written exactly as before.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Annotation {
    Dimension {
        id: Id,
        a: DVec3,
        b: DVec3,
        offset: DVec3,
        #[serde(default, skip_serializing_if = "both_free")]
        anchors: Box<[Option<crate::drafting::Anchor>; 2]>,
    },
    Text { id: Id, at: DVec3, text: String, #[serde(default)] leader: Option<DVec3> },
    /// A door's swing on plan: the leaf turns about `hinge` from closed
    /// (reaching `from`, across the opening) to open (reaching `to`).
    Swing { id: Id, hinge: DVec3, from: DVec3, to: DVec3 },
}

fn both_free(a: &[Option<crate::drafting::Anchor>; 2]) -> bool {
    a.iter().all(Option::is_none)
}

impl Annotation {
    /// A dimension between two fixed points (no anchors).
    pub fn dimension(id: Id, a: DVec3, b: DVec3, offset: DVec3) -> Annotation {
        Annotation::Dimension { id, a, b, offset, anchors: Box::new([None, None]) }
    }

    /// Is this a dimension with at least one end attached to geometry?
    pub fn is_attached(&self) -> bool {
        matches!(self, Annotation::Dimension { anchors, .. } if anchors.iter().any(Option::is_some))
    }

    pub fn id(&self) -> Id {
        match self {
            Annotation::Dimension { id, .. } | Annotation::Text { id, .. } | Annotation::Swing { id, .. } => *id,
        }
    }
}

/// A saved camera ("scene" in other modelers).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub eye: [f64; 3],
    pub target: [f64; 3],
    pub fov: f64,
    /// Match Photo: the photo seen from this camera (data URL)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    /// What else the scene remembers (SketchUp's scene properties): the view
    /// style (as the app keeps it), tag visibility, what was hidden, and the
    /// section cut ([origin, normal]). None: not remembered, left as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<Vec<Id>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<[f64; 6]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// The color (for textures: the image's average color, used by renders).
    pub color: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<Texture>,
    /// see-through in the view (reference images); 1 = solid
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
    /// physically based: 0 mirror-smooth .. 1 matte (none: by its name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness: Option<f32>,
    /// 0 paint, plastic, wood .. 1 metal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metalness: Option<f32>,
    /// clear glass (renders see through it)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glass: Option<bool>,
    /// glows (a lamp, a screen): how bright, 0..
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emission: Option<f32>,
}

impl Default for Material {
    fn default() -> Self {
        // solid unless asked: a derived default would make new materials invisible
        Material { color: [0.8, 0.8, 0.8], texture: None, opacity: 1.0, roughness: None, metalness: None, glass: None, emission: None }
    }
}

impl Material {
    /// Roughness and metalness as renders use them (asked for, else from the name).
    pub fn pbr(&self, name: &str) -> (f32, f32) {
        let n = name.to_ascii_lowercase();
        let metal_name = ["metal", "steel", "chrome", "alumin", "iron", "copper", "brass"].iter().any(|w| n.contains(w));
        let metal = self.metalness.unwrap_or(if metal_name { 1.0 } else { 0.0 }).clamp(0.0, 1.0);
        let rough = self.roughness.unwrap_or(if metal >= 0.5 { 0.3 } else { 0.6 }).clamp(0.0, 1.0);
        (rough, metal)
    }
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    (*v - 1.0).abs() < 1e-6
}

/// An image material. Faces show it at `width` x `height` meters per tile,
/// laid along each face, or, with `project`, projected along one direction so
/// the picture runs across every face it's painted on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Texture {
    /// a data: URL (PNG or JPEG)
    pub data: String,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<[f64; 3]>,
    /// for projected textures: a point the image's corner sits on
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Model {
    pub next_id: Id,
    pub vertices: BTreeMap<Id, Vertex>,
    pub edges: BTreeMap<Id, Edge>,
    pub faces: BTreeMap<Id, Face>,
    pub groups: BTreeMap<Id, Group>,
    pub materials: BTreeMap<String, Material>,
    #[serde(default)]
    pub active_group: Option<Id>,
    /// Tag name -> visible.
    #[serde(default)]
    pub tags: BTreeMap<String, bool>,
    /// the ready-made setting it's in ("studio" lights renders differently)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// a color per tag (for Color by tag)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_colors: BTreeMap<String, [f32; 3]>,
    #[serde(default)]
    pub definitions: BTreeMap<Id, Definition>,
    #[serde(default)]
    pub instances: BTreeMap<Id, Instance>,
    #[serde(default)]
    pub guides: Vec<Guide>,
    /// corners marked across matched photos (Photo modeling), kept with the model
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub photo_points: Vec<crate::photomodel::PhotoPoint>,
    /// subdivision objects' editable cages, by group
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cages: BTreeMap<Id, crate::subd::Editable>,
    /// lines to build between photo points (index pairs)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub photo_edges: Vec<[usize; 2]>,
    /// the guide points the last photo solve placed (replaced on the next)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub photo_guides: Vec<Id>,
    #[serde(default)]
    pub views: Vec<View>,
    /// clips played in order for videos (turntables, walks, sun studies)
    #[serde(default, skip_serializing_if = "crate::timeline::Timeline::is_empty")]
    pub timeline: crate::timeline::Timeline,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Display units: "m", "cm", "mm", "ft" (feet and inches) or "in". Geometry is always meters.
    #[serde(default = "meters", skip_serializing_if = "is_meters")]
    pub units: String,
    /// Classifier: IFC types (IfcWall, IfcDoor, ...) by group / placement id
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub classes: BTreeMap<Id, String>,
    /// Geo-location of the model's origin
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Point clouds (scans), by id
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clouds: BTreeMap<Id, crate::cloud::Cloud>,
    /// Section planes (SketchUp's), and the one cutting now
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SectionPlane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_section: Option<Id>,
    /// Drawing axes moved with the Axes tool: origin, red and green directions
    /// (blue is red × green). None: the model's own axes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<[[f64; 3]; 3]>,
    /// Drafting styles (tag line weights, hatches, dimension and text styles): see `drafting`
    #[serde(default, skip_serializing_if = "crate::drafting::Drafting::is_empty")]
    pub drafting: crate::drafting::Drafting,
    /// Drawing sheets (page records), by id. What is on them is in `sheet_items`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sheets: BTreeMap<Id, crate::drafting::Sheet>,
    /// Everything on paper, flat and keyed by its own id so undo works per item. `Arc`: a step's copy of the model shares them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sheet_items: BTreeMap<Id, std::sync::Arc<crate::drafting::SheetItem>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub title_blocks: BTreeMap<Id, std::sync::Arc<crate::drafting::TitleBlock>>,
    /// A time limit for the work in progress (never saved): set by a command that can be slow on a big model,
    /// checked inside the expensive loops (`over_deadline`), so the limit holds end to end.
    #[serde(skip)]
    pub deadline: Option<std::time::Instant>,
    /// Ids an operation moved or split, for anchors on sheets to follow (never saved; `Engine::exec` settles and clears it)
    #[serde(skip)]
    pub remaps: Vec<crate::anchors::Remap>,
}

/// A named section plane: cuts away the side its normal points from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SectionPlane {
    pub id: Id,
    pub name: String,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

/// Where the model's origin is on Earth (+Y is north).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
    /// meters above sea level at the origin
    #[serde(default)]
    pub elevation: f64,
}

fn meters() -> String {
    "m".into()
}

fn is_meters(u: &String) -> bool {
    u == "m"
}

pub fn pairs(l: &[Id]) -> impl Iterator<Item = (Id, Id)> + '_ {
    (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()]))
}

fn same_set(a: &[Id], b: &[Id]) -> bool {
    a.len() == b.len() && a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>()
}

impl Face {
    pub fn loops(&self) -> impl Iterator<Item = &Vec<Id>> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// Its per-corner data and painted-through placement hold only numbers the model can keep (see `sane`).
    pub fn data_sane(&self) -> bool {
        self.uv.as_ref().is_none_or(FaceUv::sane) && self.uv2.as_ref().is_none_or(FaceUv::sane) && self.vcol.as_ref().is_none_or(FaceColors::sane) && self.tex.as_ref().is_none_or(TexPlace::sane)
    }

    /// About how many bytes its texture coordinates, vertex colours and painted-through placement take in the file (on
    /// the high side, like the rest of `Model::size_estimate`).
    pub fn data_estimate(&self) -> usize {
        let uv = |u: &FaceUv| 40 + 28 * u.corners() + 4 * u.holes.len();
        self.uv.as_ref().map_or(0, uv) + self.uv2.as_ref().map_or(0, uv) + self.vcol.as_ref().map_or(0, |c| 40 + 52 * c.corners() + 4 * c.holes.len()) + self.tex.as_ref().map_or(0, |t| 300 + jl(&t.material))
    }

    /// About how many bytes the same take in memory (for the undo budget).
    pub fn data_bytes(&self) -> usize {
        let uv = |u: &FaceUv| 48 + 8 * u.corners() + 24 * u.holes.len();
        self.uv.as_ref().map_or(0, uv) + self.uv2.as_ref().map_or(0, uv) + self.vcol.as_ref().map_or(0, |c| 48 + 16 * c.corners() + 24 * c.holes.len()) + self.tex.as_ref().map_or(0, |t| 96 + t.material.len())
    }
    pub fn uses_edge(&self, a: Id, b: Id) -> bool {
        self.loops().any(|l| pairs(l).any(|(x, y)| (x == a && y == b) || (x == b && y == a)))
    }
    pub fn reverse(&mut self) {
        self.outer.reverse();
        for h in &mut self.holes {
            h.reverse();
        }
        if let Some(uv) = &mut self.uv {
            uv.outer.reverse();
            for h in &mut uv.holes {
                h.reverse();
            }
        }
        if let Some(uv) = &mut self.uv2 {
            uv.outer.reverse();
            for h in &mut uv.holes {
                h.reverse();
            }
        }
        if let Some(c) = &mut self.vcol {
            c.outer.reverse();
            for h in &mut c.holes {
                h.reverse();
            }
        }
    }
}

/// A vertex and where it is.
type Placed = (Id, DVec3);

/// Vertices by place and edges by their ends, for commands that add many of them: the answers are the ones `vertex_in`
/// and `find_edge` give (the lowest id that fits), without going over the whole model each time. Ids only grow, so it
/// takes in whatever was added since it last looked; nothing may be moved or removed while one is in use. One Lookup
/// serves one command, which may look at only so many listed points in all (`LOOKS`).
#[derive(Default)]
pub(crate) struct Lookup {
    vertices_to: Option<Id>,
    /// each cell's points in id order, with where they are
    cells: HashMap<(Option<Id>, [i64; 3]), Vec<Placed>>,
    edges_to: Option<Id>,
    ends: HashMap<(Id, Id), Id>,
    looks: u64,
}

impl Lookup {
    /// Cells a few times the weld distance across, so two points within it are at most one cell apart on each axis.
    const CELL: f64 = 4.0 * WELD;

    fn cell(p: DVec3) -> [i64; 3] {
        (p / Self::CELL).floor().to_array().map(|x| x as i64)
    }

    fn after(last: Option<Id>) -> (std::ops::Bound<Id>, std::ops::Bound<Id>) {
        use std::ops::Bound::*;
        (last.map_or(Unbounded, Excluded), Unbounded)
    }

    /// How many listed points one command may look at in all. Points in ordinary models are far apart next to the
    /// weld distance, so one search looks at a handful; this leaves room for millions of searches through dense spots.
    const LOOKS: u64 = 1 << 26;

    fn vertex(&mut self, m: &Model, p: DVec3, group: Option<Id>) -> Result<Option<Id>> {
        for (&id, v) in m.vertices.range(Self::after(self.vertices_to)) {
            self.cells.entry((v.group, Self::cell(v.p))).or_default().push((id, v.p));
            self.vertices_to = Some(id);
        }
        let c = Self::cell(p);
        let mut best: Option<Id> = None;
        for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                for dz in -1..=1i64 {
                    let k = [c[0].saturating_add(dx), c[1].saturating_add(dy), c[2].saturating_add(dz)];
                    let Some(list) = self.cells.get(&(group, k)) else { continue };
                    // in id order: once past the best so far, nothing further in this cell can beat it
                    let mut seen = 0u64;
                    for &(id, q) in list {
                        if best.is_some_and(|b| id >= b) {
                            break;
                        }
                        seen += 1;
                        if q.distance(p) < WELD {
                            best = Some(id);
                            break;
                        }
                    }
                    self.looks += seen;
                    if self.looks > Self::LOOKS {
                        bail!("too many coincident points to weld; spread the copies apart or copy fewer at once");
                    }
                }
            }
        }
        Ok(best)
    }

    fn edge(&mut self, m: &Model, a: Id, b: Id) -> Option<Id> {
        for (&id, e) in m.edges.range(Self::after(self.edges_to)) {
            self.ends.entry((e.a.min(e.b), e.a.max(e.b))).or_insert(id);
            self.edges_to = Some(id);
        }
        self.ends.get(&(a.min(b), a.max(b))).copied().filter(|id| m.edges.get(id).is_some_and(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a)))
    }
}

/// What a component's own model may refer to besides its own tables: the root model's component definitions and materials.
struct Root {
    defs: BTreeSet<Id>,
    materials: BTreeSet<String>,
}

impl Model {
    pub(crate) fn new_id(&mut self) -> Id {
        // never wraps (a wrapped id would overwrite something that is there): past the end the command fails and is undone
        match self.next_id.checked_add(1) {
            Some(n) => {
                self.next_id = n;
                n
            }
            None => panic!("{IDS_OUT}"),
        }
    }

    // ----- vertices & edges -------------------------------------------------

    /// Vertex at `p` in the active group, reusing one that's already there.
    pub fn vertex(&mut self, p: DVec3) -> Id {
        self.vertex_in(p, self.active_group)
    }

    pub fn vertex_in(&mut self, p: DVec3, group: Option<Id>) -> Id {
        if let Some((&id, _)) = self.vertices.iter().find(|(_, v)| v.group == group && v.p.distance(p) < WELD) {
            return id;
        }
        let id = self.new_id();
        self.vertices.insert(id, Vertex { p, group });
        id
    }

    /// `vertex_in` through `look`: the same vertex, found without going over every vertex.
    pub(crate) fn vertex_with(&mut self, p: DVec3, group: Option<Id>, look: &mut Lookup) -> Result<Id> {
        if let Some(id) = look.vertex(self, p, group)? {
            return Ok(id);
        }
        let id = self.new_id();
        self.vertices.insert(id, Vertex { p, group });
        Ok(id)
    }

    /// `find_edge` through `look`.
    pub(crate) fn find_edge_with(&self, a: Id, b: Id, look: &mut Lookup) -> Option<Id> {
        look.edge(self, a, b)
    }

    /// `edge` through `look`: the same edge, found without going over every edge.
    pub(crate) fn edge_with(&mut self, a: Id, b: Id, look: &mut Lookup) -> Id {
        if let Some(id) = look.edge(self, a, b) {
            return id;
        }
        let id = self.new_id();
        let group = self.vertices[&a].group;
        self.edges.insert(id, Edge { a, b, group, ..Default::default() });
        id
    }

    pub fn p(&self, v: Id) -> DVec3 {
        self.vertices[&v].p
    }

    pub fn pts(&self, l: &[Id]) -> Vec<DVec3> {
        l.iter().map(|v| self.p(*v)).collect()
    }

    pub fn find_edge(&self, a: Id, b: Id) -> Option<Id> {
        self.edges
            .iter()
            .find(|(_, e)| (e.a == a && e.b == b) || (e.a == b && e.b == a))
            .map(|(&id, _)| id)
    }

    pub fn edge(&mut self, a: Id, b: Id) -> Id {
        if let Some(id) = self.find_edge(a, b) {
            return id;
        }
        let id = self.new_id();
        let group = self.vertices[&a].group;
        self.edges.insert(id, Edge { a, b, group, ..Default::default() });
        id
    }

    // ----- faces ------------------------------------------------------------

    pub fn face(&self, f: Id) -> Result<&Face> {
        self.faces.get(&f).ok_or_else(|| anyhow!("no face {f}"))
    }

    pub fn normal(&self, f: Id) -> DVec3 {
        geom::newell(&self.pts(&self.faces[&f].outer))
    }

    pub fn center(&self, f: Id) -> DVec3 {
        geom::centroid(&self.pts(&self.faces[&f].outer))
    }

    pub fn face_area(&self, f: Id) -> f64 {
        let face = &self.faces[&f];
        geom::area(&self.pts(&face.outer)) - face.holes.iter().map(|h| geom::area(&self.pts(h))).sum::<f64>()
    }

    /// Is `p` on face `f` (on its plane, inside the outline, not in a hole)?
    pub fn contains(&self, f: Id, p: DVec3) -> bool {
        let face = &self.faces[&f];
        let outer = self.pts(&face.outer);
        let n = geom::newell(&outer);
        if (p - outer[0]).dot(n).abs() > 1e-4 {
            return false;
        }
        let (u, v) = geom::basis(n);
        let q = geom::to2d(&[p], u, v)[0];
        geom::inside(q, &geom::to2d(&outer, u, v))
            && !face.holes.iter().any(|h| geom::inside(q, &geom::to2d(&self.pts(h), u, v)))
    }

    /// Resize everything by `k` about the origin (Tape Measure's "resize the
    /// model"): geometry, component placements (where they sit, and how big),
    /// guides, notes, saved cameras and point clouds. With a group open, just
    /// that group's contents, about its lowest corner. Components keep their
    /// own geometry and are scaled where placed.
    pub fn resize(&mut self, k: f64) -> Result<()> {
        if !(k.is_finite() && k > 1e-6 && k < 1e6) {
            bail!("the new length needs to be more than zero");
        }
        if let Some(g) = self.active_group {
            let mut inside = vec![g];
            let mut i = 0;
            while i < inside.len() {
                let p = inside[i];
                inside.extend(self.groups.iter().filter(|(_, x)| x.parent == Some(p)).map(|(k, _)| *k));
                i += 1;
            }
            let vs: Vec<Id> = self.vertices.iter().filter(|(_, v)| v.group.is_some_and(|x| inside.contains(&x))).map(|(k, _)| *k).collect();
            let o = vs.iter().map(|v| self.p(*v)).fold(DVec3::splat(f64::MAX), |a, p| a.min(p));
            for v in vs {
                let p = self.vertices[&v].p;
                self.vertices.get_mut(&v).unwrap().p = o + (p - o) * k;
            }
            for i in self.instances.values_mut().filter(|i| i.parent.is_some_and(|x| inside.contains(&x))) {
                let (s, r, t) = i.transform.to_scale_rotation_translation();
                i.transform = glam::DMat4::from_scale_rotation_translation(s * k, r, o + (t - o) * k);
            }
            return Ok(());
        }
        for v in self.vertices.values_mut() {
            v.p *= k;
        }
        for i in self.instances.values_mut() {
            i.transform = glam::DMat4::from_scale(DVec3::splat(k)) * i.transform;
        }
        for g in &mut self.guides {
            g.p *= k;
        }
        for a in &mut self.annotations {
            match a {
                Annotation::Dimension { a, b, offset, .. } => {
                    *a *= k;
                    *b *= k;
                    *offset *= k;
                }
                Annotation::Text { at, leader, .. } => {
                    *at *= k;
                    if let Some(l) = leader {
                        *l *= k;
                    }
                }
                Annotation::Swing { hinge, from, to, .. } => {
                    *hinge *= k;
                    *from *= k;
                    *to *= k;
                }
            }
        }
        for v in &mut self.views {
            v.eye = (DVec3::from(v.eye) * k).to_array();
            v.target = (DVec3::from(v.target) * k).to_array();
        }
        for c in self.clouds.values_mut() {
            c.points = std::sync::Arc::new(c.points.iter().map(|x| (*x as f64 * k) as f32).collect());
            c.version += 1;
        }
        Ok(())
    }

    /// A face or edge and what it touches: with `all`, everything connected
    /// (through shared points, in the same group); else a face's own edges, or
    /// an edge's faces. Returns (faces, edges).
    pub fn connected(&self, id: Id, all: bool) -> Result<(Vec<Id>, Vec<Id>)> {
        let edge_ids = |vs: &BTreeSet<Id>, group: Option<Id>| -> Vec<Id> { self.edges.iter().filter(|(_, e)| e.group == group && vs.contains(&e.a) && vs.contains(&e.b)).map(|(k, _)| *k).collect() };
        if !all {
            if let Some(f) = self.faces.get(&id) {
                let mut out = vec![];
                for l in f.loops() {
                    for (a, b) in pairs(l) {
                        out.extend(self.edges.iter().filter(|(_, e)| (e.a == a && e.b == b) || (e.a == b && e.b == a)).map(|(k, _)| *k));
                    }
                }
                return Ok((vec![id], out));
            }
            if let Some(e) = self.edges.get(&id) {
                return Ok((self.faces_using_edge(e.a, e.b), vec![id]));
            }
            bail!("no face or edge {id}");
        }
        let (start, group): (Vec<Id>, Option<Id>) = if let Some(f) = self.faces.get(&id) {
            (f.loops().flatten().copied().collect(), f.group)
        } else if let Some(e) = self.edges.get(&id) {
            (vec![e.a, e.b], e.group)
        } else {
            bail!("no face or edge {id}");
        };
        // points joined by edges, and by faces
        let mut nb: HashMap<Id, Vec<Id>> = HashMap::new();
        for e in self.edges.values().filter(|e| e.group == group) {
            nb.entry(e.a).or_default().push(e.b);
            nb.entry(e.b).or_default().push(e.a);
        }
        let face_list: Vec<(Id, Vec<Id>)> = self.faces.iter().filter(|(_, f)| f.group == group).map(|(k, f)| (*k, f.loops().flatten().copied().collect())).collect();
        for (_, vs) in &face_list {
            for w in vs.windows(2) {
                nb.entry(w[0]).or_default().push(w[1]);
                nb.entry(w[1]).or_default().push(w[0]);
            }
            if let (Some(a), Some(b)) = (vs.first(), vs.last()) {
                nb.entry(*a).or_default().push(*b);
                nb.entry(*b).or_default().push(*a);
            }
        }
        let mut seen: BTreeSet<Id> = start.iter().copied().collect();
        let mut todo = start;
        while let Some(v) = todo.pop() {
            for w in nb.get(&v).into_iter().flatten() {
                if seen.insert(*w) {
                    todo.push(*w);
                }
            }
        }
        let faces = face_list.iter().filter(|(_, vs)| vs.iter().any(|v| seen.contains(v))).map(|(k, _)| *k).collect();
        Ok((faces, edge_ids(&seen, group)))
    }

    pub fn faces_using_edge(&self, a: Id, b: Id) -> Vec<Id> {
        self.faces.iter().filter(|(_, f)| f.uses_edge(a, b)).map(|(&id, _)| id).collect()
    }

    /// True once the deadline set by the command in progress has passed.
    pub fn over_deadline(&self) -> bool {
        self.deadline.is_some_and(|d| std::time::Instant::now() >= d)
    }

    /// An error once the deadline has passed (see `deadline`).
    pub fn check_deadline(&self) -> Result<()> {
        if self.over_deadline() {
            bail!("that took too long, so it was stopped (use fewer or simpler outlines, or a smaller model)");
        }
        Ok(())
    }

    /// Insert a face as given (no checks), creating its edges.
    pub(crate) fn insert_face(&mut self, face: Face) -> Id {
        for l in face.loops().cloned().collect::<Vec<_>>() {
            for (a, b) in pairs(&l) {
                self.edge(a, b);
            }
        }
        let id = self.new_id();
        self.faces.insert(id, face);
        id
    }

    /// `insert_face` through `look`.
    pub(crate) fn insert_face_with(&mut self, face: Face, look: &mut Lookup) -> Id {
        for l in face.loops() {
            for (a, b) in pairs(l) {
                self.edge_with(a, b, look);
            }
        }
        let id = self.new_id();
        self.faces.insert(id, face);
        id
    }

    /// Add a planar face from a loop of vertex ids. Returns the face id (an
    /// existing one if this loop is already a face).
    pub fn add_face(&mut self, mut lp: Vec<Id>) -> Result<Id> {
        lp.dedup();
        if lp.len() > 1 && lp.first() == lp.last() {
            lp.pop();
        }
        if lp.len() < 3 {
            bail!("a face needs at least 3 distinct points");
        }
        let pts = self.pts(&lp);
        let n = geom::newell(&pts);
        if n == DVec3::ZERO {
            bail!("points are in a line; no face");
        }
        if pts.iter().any(|p| (*p - pts[0]).dot(n).abs() > PLANAR) {
            bail!("points are not on one plane");
        }
        let group = self.active_group;
        if let Some((&id, _)) = self.faces.iter().find(|(_, f)| f.group == group && same_set(&f.outer, &lp)) {
            return Ok(id);
        }

        // Drawn on an existing coplanar face? Strictly inside: it becomes a hole
        // there. Touching the face's edge (a door at the bottom of a wall): the
        // face is split around it.
        let (u, v) = geom::basis(n);
        let mine = geom::to2d(&pts, u, v);
        let mid = mine.iter().copied().sum::<glam::DVec2>() / mine.len() as f64;
        let mut hole_in = None;
        let mut split = None;
        for (k, (&id, f)) in self.faces.iter().enumerate() {
            if f.group != group {
                continue;
            }
            if k % 64 == 0 {
                self.check_deadline()?;
            }
            let fp = self.pts(&f.outer);
            let fnorm = geom::newell(&fp);
            if fnorm.dot(n).abs() < 1.0 - 1e-9 || (fp[0] - pts[0]).dot(n).abs() > PLANAR {
                continue;
            }
            let outer2 = geom::to2d(&fp, u, v);
            let holes2: Vec<_> = f.holes.iter().map(|h| geom::to2d(&self.pts(h), u, v)).collect();
            let in_hole = |p: &glam::DVec2| holes2.iter().any(|h| geom::inside(*p, h));
            if mine.iter().all(|p| geom::inside(*p, &outer2) && !in_hole(p)) {
                hole_in = Some((id, fnorm));
                break;
            }
            if mine.iter().all(|p| geom::inside_or_on(*p, &outer2) && !in_hole(p)) && geom::inside(mid, &outer2) && !in_hole(&mid) {
                split = Some((id, fnorm));
                break;
            }
        }

        if let Some((pid, fnorm)) = hole_in.or(split) {
            if fnorm.dot(n) < 0.0 {
                lp.reverse();
            }
            if hole_in.is_some() {
                let mut hole = lp.clone();
                hole.reverse();
                self.faces.get_mut(&pid).unwrap().holes.push(hole);
            } else {
                self.cut_from(pid, &lp)?;
            }
        } else if n.z < -0.999 {
            // ground-like faces face up
            lp.reverse();
        }
        Ok(self.insert_face(Face { outer: lp, group, ..Default::default() }))
    }

    /// Remove the area of loop `cut` from face `pid` (they share an edge or
    /// touch), rebuilding its outline. Pieces left over become extra faces.
    fn cut_from(&mut self, pid: Id, cut: &[Id]) -> Result<()> {
        use geo::{BooleanOps, Coord, LineString, Polygon};
        let parent = self.faces[&pid].clone();
        let fp = self.pts(&parent.outer);
        let n = geom::newell(&fp);
        let (u, v) = geom::basis(n);
        let depth = fp[0].dot(n);
        let ring = |ids: &[Id]| -> LineString<f64> {
            LineString::from(geom::to2d(&self.pts(ids), u, v).iter().map(|p| Coord { x: p.x, y: p.y }).collect::<Vec<_>>())
        };
        let ppoly = Polygon::new(ring(&parent.outer), parent.holes.iter().map(|h| ring(h)).collect());
        let cpoly = Polygon::new(ring(cut), vec![]);
        let result = ppoly.difference(&cpoly);

        // reuse existing vertices wherever the result lands on one
        let mut known: Vec<(glam::DVec2, Id)> = Vec::new();
        for id in parent.loops().flatten().chain(cut.iter()) {
            let p = self.p(*id);
            known.push((glam::DVec2::new(p.dot(u), p.dot(v)), *id));
        }
        let mut to_ids = |m: &mut Model, ls: &LineString<f64>, ccw: bool| -> Vec<Id> {
            let mut pts2: Vec<glam::DVec2> = ls.coords().map(|c| glam::DVec2::new(c.x, c.y)).collect();
            if pts2.len() > 1 && pts2.first() == pts2.last() {
                pts2.pop();
            }
            if (geom::signed_area(&pts2) > 0.0) != ccw {
                pts2.reverse();
            }
            let mut ids: Vec<Id> = pts2
                .iter()
                .map(|q| {
                    if let Some((_, id)) = known.iter().find(|(k, _)| k.distance(*q) < 1e-6) {
                        *id
                    } else {
                        let id = m.vertex_in(u * q.x + v * q.y + n * depth, parent.group);
                        known.push((*q, id));
                        id
                    }
                })
                .collect();
            ids.dedup();
            ids
        };
        let mut pieces = Vec::new();
        for poly in result.0.iter() {
            let outer = to_ids(self, poly.exterior(), true);
            let holes = poly.interiors().iter().map(|h| to_ids(self, h, false)).collect::<Vec<_>>();
            if outer.len() >= 3 {
                pieces.push(Face { outer, holes, material: parent.material.clone(), group: parent.group, ..Default::default() });
            }
        }
        self.faces.remove(&pid);
        let mut first = true;
        for piece in pieces {
            if first {
                for l in piece.loops().cloned().collect::<Vec<_>>() {
                    for (a, b) in pairs(&l) {
                        self.edge(a, b);
                    }
                }
                self.faces.insert(pid, piece); // keep the id stable
                first = false;
            } else {
                self.insert_face(piece);
            }
        }
        Ok(())
    }

    /// Imported data is rarely perfectly flat: points within 1 cm of the
    /// best-fit plane are moved onto it before the face is made.
    pub fn add_polygon(&mut self, points: &[DVec3]) -> Result<Id> {
        let mut points = points.to_vec();
        let n = geom::newell(&points);
        if n != DVec3::ZERO {
            let c = geom::centroid(&points);
            if points.iter().all(|p| (*p - c).dot(n).abs() < 0.01) {
                for p in &mut points {
                    *p -= n * (*p - c).dot(n);
                }
            }
        }
        let lp: Vec<Id> = points.iter().map(|p| self.vertex(*p)).collect();
        self.add_face(lp)
    }

    // ----- push / pull ----------------------------------------------------------

    /// Push (negative) or pull (positive) a face along its normal. Returns the
    /// ids of faces created or moved; the pushed face's id is first.
    pub fn pushpull(&mut self, fid: Id, d: f64) -> Result<Vec<Id>> {
        self.check_deadline()?;
        let f = self.face(fid)?.clone();
        if d.abs() < WELD {
            return Ok(vec![fid]);
        }
        let n = self.normal(fid);
        let dir = n * d;
        let loops: Vec<Vec<Id>> = f.loops().cloned().collect();

        // Sides already standing on this face -> just slide it.
        let slide = loops.iter().all(|l| {
            pairs(l).all(|(a, b)| {
                let others: Vec<Id> = self.faces_using_edge(a, b).into_iter().filter(|g| *g != fid).collect();
                !others.is_empty() && others.iter().all(|g| self.normal(*g).dot(n).abs() < 1e-6)
            })
        });
        self.check_deadline()?;
        if slide {
            let verts: BTreeSet<Id> = loops.iter().flatten().copied().collect();
            for v in verts {
                self.vertices.get_mut(&v).unwrap().p += dir;
            }
            return Ok(vec![fid]);
        }

        // Is this face filling a hole in a bigger face (window on a wall)?
        let parent = self
            .faces
            .iter()
            .find(|(&id, g)| id != fid && g.holes.iter().any(|h| same_set(h, &f.outer)))
            .map(|(&id, _)| id);
        // Part of a bigger flat surface (a window in a wall, or a door cut from
        // its edge)? Then pushing carves into the solid and the base disappears.
        let embedded = parent.is_some()
            || loops.iter().any(|l| {
                pairs(l).any(|(a, b)| {
                    self.faces_using_edge(a, b)
                        .into_iter()
                        .any(|g| g != fid && self.normal(g).dot(n) > 1.0 - 1e-9)
                })
            });
        let carving = embedded && d < 0.0;
        self.check_deadline()?;

        let mut map: HashMap<Id, Id> = HashMap::new();
        for &v in loops.iter().flatten() {
            if !map.contains_key(&v) {
                let p = self.p(v) + dir;
                let nv = self.vertex_in(p, f.group);
                map.insert(v, nv);
            }
        }

        let mut out = Vec::new();
        let mut sides = Vec::new();
        for l in &loops {
            for (a, b) in pairs(l) {
                let (a2, b2) = (map[&a], map[&b]);
                let mut quad = if d > 0.0 { vec![a, b, b2, a2] } else { vec![a, a2, b2, b] };
                if carving {
                    quad.reverse();
                }
                sides.push(self.insert_face(Face { outer: quad, material: f.material.clone(), group: f.group, ..Default::default() }));
            }
        }
        let mut top = Face {
            outer: f.outer.iter().map(|v| map[v]).collect(),
            holes: f.holes.iter().map(|h| h.iter().map(|v| map[v]).collect()).collect(),
            material: f.material.clone(),
            group: f.group,
            ..Default::default()
        };
        if d < 0.0 && !carving {
            top.reverse();
        }
        let top_id = self.insert_face(top);
        out.push(top_id);
        self.soften_smooth(&sides); // a pulled circle reads as a smooth cylinder
        out.extend(sides);

        if embedded {
            self.faces.remove(&fid); // now inside the solid / open to the recess
        } else if d > 0.0 {
            self.faces.get_mut(&fid).unwrap().reverse(); // becomes the bottom
        }
        Ok(out)
    }

    // ----- editing ------------------------------------------------------------

    /// All vertex ids that belong to the given entities (faces, edges, groups, vertices).
    pub fn vertices_of(&self, ids: &[Id]) -> Result<BTreeSet<Id>> {
        let mut vs = BTreeSet::new();
        for id in ids {
            if let Some(f) = self.faces.get(id) {
                vs.extend(f.loops().flatten().copied());
            } else if let Some(e) = self.edges.get(id) {
                vs.insert(e.a);
                vs.insert(e.b);
            } else if self.groups.contains_key(id) {
                let tree = self.subtree(*id);
                vs.extend(self.vertices.iter().filter(|(_, v)| v.group.is_some_and(|g| tree.contains(&g))).map(|(&k, _)| k));
            } else if self.vertices.contains_key(id) {
                vs.insert(*id);
            } else {
                bail!("no entity {id}");
            }
        }
        Ok(vs)
    }

    pub fn translate(&mut self, ids: &[Id], by: DVec3) -> Result<usize> {
        self.transform(ids, glam::DMat4::from_translation(by))
    }

    /// Faces named directly, or every face of a named group.
    pub fn faces_of(&self, ids: &[Id]) -> Result<Vec<Id>> {
        let mut out = Vec::new();
        for id in ids {
            if self.faces.contains_key(id) {
                out.push(*id);
            } else if self.groups.contains_key(id) {
                let tree = self.subtree(*id);
                out.extend(self.faces.iter().filter(|(_, f)| f.group.is_some_and(|g| tree.contains(&g))).map(|(&k, _)| k));
            } else {
                bail!("{id} is not a face or group");
            }
        }
        Ok(out)
    }

    /// Refuse to change something locked (or inside a locked group).
    pub fn check_unlocked(&self, ids: &[Id]) -> Result<()> {
        for id in ids {
            let g = if self.groups.contains_key(id) {
                Some(*id)
            } else if let Some(i) = self.instances.get(id) {
                i.parent
            } else if let Some(f) = self.faces.get(id) {
                f.group
            } else if let Some(e) = self.edges.get(id) {
                e.group
            } else {
                None
            };
            if let Some(name) = self.locked_by(g) {
                bail!("{name} is locked: unlock it in the Outliner first");
            }
        }
        Ok(())
    }

    pub fn delete(&mut self, ids: &[Id]) -> Result<()> {
        self.check_unlocked(ids)?;
        for id in ids {
            if self.faces.remove(id).is_some() {
                // a face's edges stay behind as lines, like a real modeler
            } else if let Some(e) = self.edges.remove(id) {
                // deleting an edge deletes the faces that need it
                let dead: Vec<Id> = self.faces.iter().filter(|(_, f)| f.uses_edge(e.a, e.b)).map(|(&k, _)| k).collect();
                for f in dead {
                    self.faces.remove(&f);
                }
            } else if let Some(i) = self.annotations.iter().position(|a| a.id() == *id) {
                self.annotations.remove(i);
            } else if self.instances.remove(id).is_some() {
                // component placement removed; its definition stays in the library
            } else if self.groups.contains_key(id) {
                // the group and everything nested in it
                let tree = self.subtree(*id);
                let inside = |g: &Option<Id>| g.is_some_and(|g| tree.contains(&g));
                self.faces.retain(|_, f| !inside(&f.group));
                self.edges.retain(|_, e| !inside(&e.group));
                self.vertices.retain(|_, v| !inside(&v.group));
                self.instances.retain(|_, i| !inside(&i.parent));
                for g in &tree {
                    self.groups.remove(g);
                    self.cages.remove(g);
                }
                if inside(&self.active_group) {
                    self.active_group = None;
                }
            } else {
                bail!("no face, edge or group {id}");
            }
        }
        self.gc();
        Ok(())
    }

    /// Drop vertices nothing uses.
    pub fn gc(&mut self) {
        let mut used = BTreeSet::new();
        for e in self.edges.values() {
            used.insert(e.a);
            used.insert(e.b);
        }
        for f in self.faces.values() {
            used.extend(f.loops().flatten().copied());
        }
        self.vertices.retain(|k, _| used.contains(k));
    }

    pub fn group(&mut self, ids: &[Id], name: String) -> Result<Id> {
        // groups and placements go inside the new group as they are; loose
        // faces become its own geometry
        let (nest, loose): (Vec<Id>, Vec<Id>) = ids.iter().partition(|id| self.groups.contains_key(id) || self.instances.contains_key(id));
        let faces = self.faces_of(&loose)?;
        let nested = nest.clone();
        let gid = self.new_id();
        self.groups.insert(gid, Group { name, parent: self.active_group, ..Default::default() });
        for id in nest {
            if let Some(g) = self.groups.get_mut(&id) {
                g.parent = Some(gid);
            } else if let Some(i) = self.instances.get_mut(&id) {
                i.parent = Some(gid);
            }
        }
        let inside: BTreeSet<Id> = faces.iter().copied().collect();
        let mut verts = BTreeSet::new();
        for f in &faces {
            let face = self.faces.get_mut(f).unwrap();
            face.group = Some(gid);
            verts.extend(face.loops().flatten().copied());
        }
        // a corner that faces outside the group also use can't belong to both: the
        // group gets its own copy (and its own copies of the edges between them)
        let shared: BTreeSet<Id> = self.faces.iter().filter(|(k, _)| !inside.contains(k)).flat_map(|(_, f)| f.loops().flatten().copied()).filter(|v| verts.contains(v)).collect();
        let mut remap: HashMap<Id, Id> = HashMap::new();
        for v in &shared {
            let p = self.vertices[v].p;
            let id = self.new_id();
            self.vertices.insert(id, Vertex { p, group: Some(gid) });
            remap.insert(*v, id);
        }
        if !remap.is_empty() {
            let outside_edges: BTreeSet<(Id, Id)> = self.faces.iter().filter(|(k, _)| !inside.contains(k)).flat_map(|(_, f)| f.loops().flat_map(|l| pairs(l).map(|(a, b)| (a.min(b), a.max(b))).collect::<Vec<_>>()).collect::<Vec<_>>()).collect();
            let used: Vec<(Id, Id)> = faces.iter().flat_map(|f| self.faces[f].loops().flat_map(|l| pairs(l).map(|(a, b)| (a.min(b), a.max(b))).collect::<Vec<_>>()).collect::<Vec<_>>()).collect();
            for f in &faces {
                let face = self.faces.get_mut(f).unwrap();
                for v in face.outer.iter_mut().chain(face.holes.iter_mut().flatten()) {
                    if let Some(n) = remap.get(v) {
                        *v = *n;
                    }
                }
            }
            // one lookup of the edges by their ends (a linear search per edge made this quadratic)
            let key = |a: Id, b: Id| (a.min(b), a.max(b));
            let mut by_ends: HashMap<(Id, Id), Id> = HashMap::new();
            for (k, e) in &self.edges {
                by_ends.entry(key(e.a, e.b)).or_insert(*k);
            }
            for (a, b) in used {
                let Some((eid, e)) = by_ends.get(&key(a, b)).and_then(|k| self.edges.get(k).map(|e| (*k, e.clone()))) else { continue };
                let (na, nb) = (remap.get(&e.a).copied().unwrap_or(e.a), remap.get(&e.b).copied().unwrap_or(e.b));
                if !by_ends.contains_key(&key(na, nb)) {
                    let id = self.new_id();
                    self.edges.insert(id, Edge { a: na, b: nb, group: Some(gid), ..e });
                    by_ends.insert(key(na, nb), id);
                }
                if !outside_edges.contains(&(a, b)) && (remap.contains_key(&e.a) || remap.contains_key(&e.b)) {
                    self.edges.remove(&eid);
                    if by_ends.get(&key(a, b)) == Some(&eid) {
                        by_ends.remove(&key(a, b));
                    }
                }
            }
            verts = verts.iter().map(|v| remap.get(v).copied().unwrap_or(*v)).collect();
        }
        for v in &verts {
            self.vertices.get_mut(v).unwrap().group = Some(gid);
        }
        if !remap.is_empty() {
            // dimensions whose geometry all went into the group follow the copies (see `Model::group_follow`)
            self.note_remap(false, || crate::anchors::Remap::Group { copies: remap.iter().map(|(a, b)| (*a, *b)).collect(), inside: verts.clone(), nested });
        }
        let originals: BTreeSet<Id> = remap.keys().copied().collect();
        for e in self.edges.values_mut() {
            if verts.contains(&e.a) && verts.contains(&e.b) && !originals.contains(&e.a) && !originals.contains(&e.b) {
                e.group = Some(gid);
            }
        }
        Ok(gid)
    }

    /// Duplicate a group, offset by `by`. Returns the new group id.
    pub fn copy_group(&mut self, gid: Id, by: DVec3, name: Option<String>) -> Result<Id> {
        self.check_room(self.copy_cost(&[gid]))?;
        let parent = self.groups.get(&gid).ok_or_else(|| anyhow!("{gid} is not a group"))?.parent;
        self.copy_group_into(gid, by, name, parent)
    }

    /// Copy a group (and everything nested in it) into `parent`.
    fn copy_group_into(&mut self, gid: Id, by: DVec3, name: Option<String>, parent: Option<Id>) -> Result<Id> {
        let g = self.groups.get(&gid).ok_or_else(|| anyhow!("{gid} is not a group"))?.clone();
        let new = self.new_id();
        self.groups.insert(new, Group { name: name.unwrap_or_else(|| format!("{} copy", g.name)), tag: g.tag.clone(), hidden: false, parent, locked: false });
        if let Some(mut c) = self.cages.get(&gid).cloned() {
            for p in c.points.iter_mut() {
                *p = (DVec3::from(*p) + by).to_array();
            }
            if let Some(s) = c.symmetry.as_mut() {
                s.center += by[s.axis];
            }
            self.cages.insert(new, c);
        }
        // nested groups and placements come along
        let kids: Vec<Id> = self.groups.iter().filter(|(_, c)| c.parent == Some(gid)).map(|(&k, _)| k).collect();
        for k in kids {
            let n = self.groups[&k].name.clone();
            self.copy_group_into(k, by, Some(n), Some(new))?;
        }
        let placed: Vec<crate::model::Instance> = self.instances.values().filter(|i| i.parent == Some(gid)).cloned().collect();
        for mut i in placed {
            i.parent = Some(new);
            i.transform = glam::DMat4::from_translation(by) * i.transform;
            let id = self.new_id();
            self.instances.insert(id, i);
        }
        let mut map: HashMap<Id, Id> = HashMap::new();
        let verts: Vec<(Id, DVec3)> =
            self.vertices.iter().filter(|(_, v)| v.group == Some(gid)).map(|(&k, v)| (k, v.p)).collect();
        for (k, p) in verts {
            let id = self.new_id();
            self.vertices.insert(id, Vertex { p: p + by, group: Some(new) });
            map.insert(k, id);
        }
        let edges: Vec<Edge> = self.edges.values().filter(|e| e.group == Some(gid)).cloned().collect();
        let mut curves: HashMap<Id, Id> = HashMap::new();
        for e in edges {
            // a copy's curves are its own
            let curve = e.curve.map(|c| *curves.entry(c).or_insert_with(|| self.new_id()));
            let id = self.new_id();
            self.edges.insert(id, Edge { a: map[&e.a], b: map[&e.b], group: Some(new), soft: e.soft, hidden: e.hidden, curve, draft: e.draft.clone(), seam: e.seam });
        }
        let faces: Vec<Face> = self.faces.values().filter(|f| f.group == Some(gid)).cloned().collect();
        for f in faces {
            let id = self.new_id();
            let face = Face {
                outer: f.outer.iter().map(|v| map[v]).collect(),
                holes: f.holes.iter().map(|h| h.iter().map(|v| map[v]).collect()).collect(),
                material: f.material.clone(),
                group: Some(new),
                hidden: f.hidden,
                tex: None,
                uv: f.uv.clone(),
                vcol: f.vcol.clone(),
                uv2: f.uv2.clone(),
            };
            self.faces.insert(id, face);
        }
        Ok(new)
    }

    pub fn begin_group(&mut self, name: String) -> Id {
        let gid = self.new_id();
        // inside an open group, the new one nests in it
        self.groups.insert(gid, Group { name, parent: self.active_group, ..Default::default() });
        self.active_group = Some(gid);
        gid
    }

    pub fn paint(&mut self, ids: &[Id], color: [f32; 3]) -> Result<String> {
        let name = self.color_material(color);
        for f in self.faces_of(ids)? {
            let face = self.faces.get_mut(&f).unwrap();
            face.material = Some(name.clone());
            face.tex = None;
        }
        Ok(name)
    }

    /// The plain-color material for `color` (made if it isn't there yet); its name.
    pub fn color_material(&mut self, color: [f32; 3]) -> String {
        let name = format!(
            "#{:02x}{:02x}{:02x}",
            (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[2].clamp(0.0, 1.0) * 255.0).round() as u8
        );
        self.materials.entry(name.clone()).or_insert(Material { color, texture: None, opacity: 1.0, ..Default::default() });
        name
    }

    /// Paint faces (or whole groups) with an existing material by name.
    pub fn paint_material(&mut self, ids: &[Id], name: &str) -> Result<()> {
        if !self.materials.contains_key(name) {
            bail!("no material {name}");
        }
        for f in self.faces_of(ids)? {
            let face = self.faces.get_mut(&f).unwrap();
            face.material = Some(name.to_string());
            face.tex = None;
        }
        Ok(())
    }

    /// The faces a Paint Bucket click reaches from the clicked faces `seeds`, in the seed's group.
    /// - `face` (or none): just the seeds.
    /// - `connected`: faces joined to a seed through shared edges that wear the seed's material; the spread stops
    ///   at a face with any other material (it is not crossed), and it crosses hard and soft edges alike.
    /// - `all`: every face in the group with the seed's material, joined or not.
    /// - `object`: faces with the seed's material anywhere on the connected mesh, crossing faces of other materials.
    ///
    /// Hidden faces are skipped. The result is in face-id order.
    pub fn paint_targets(&self, seeds: &[Id], scope: Option<&str>) -> Result<Vec<Id>> {
        let seeds = self.faces_of(seeds)?;
        let scope = scope.unwrap_or("face");
        if scope == "face" {
            return Ok(seeds);
        }
        if !matches!(scope, "connected" | "all" | "object") {
            bail!("unknown paint scope {scope} (face, connected, all or object)");
        }
        let mut out = BTreeSet::new();
        // faces next to each other through a shared edge, per group
        let mut by_edge: HashMap<(Option<Id>, Id, Id), Vec<Id>> = HashMap::new();
        if scope != "all" {
            for (&id, f) in self.faces.iter().filter(|(_, f)| !f.hidden) {
                for l in f.loops() {
                    for (a, b) in pairs(l) {
                        by_edge.entry((f.group, a.min(b), a.max(b))).or_default().push(id);
                    }
                }
            }
        }
        if scope == "all" {
            // every face of the seeds' groups that wears a seed's material: one pass, however many seeds
            let keys: std::collections::HashSet<(Option<Id>, Option<&str>)> = seeds.iter().map(|s| (self.faces[s].group, self.faces[s].material.as_deref())).collect();
            out.extend(self.faces.iter().filter(|(_, f)| !f.hidden && keys.contains(&(f.group, f.material.as_deref()))).map(|(&k, _)| k));
            return Ok(out.into_iter().collect());
        }
        if scope == "object" {
            // each connected piece is walked once, however many seeds (and materials) it holds; then every face of a
            // seed's piece that wears that seed's material is kept
            let mut piece: HashMap<Id, usize> = HashMap::new();
            let mut members: Vec<Vec<Id>> = Vec::new();
            let mut wanted: BTreeMap<usize, std::collections::HashSet<Option<&str>>> = BTreeMap::new();
            for &s in &seeds {
                let sf = &self.faces[&s];
                let mat = sf.material.as_deref();
                // a hidden seed is kept itself and reaches the pieces next to it
                let starts: Vec<Id> = if sf.hidden {
                    out.insert(s);
                    let mut next = Vec::new();
                    for l in sf.loops() {
                        for (a, b) in pairs(l) {
                            next.extend(by_edge.get(&(sf.group, a.min(b), a.max(b))).into_iter().flatten().copied());
                        }
                    }
                    next
                } else {
                    vec![s]
                };
                for start in starts {
                    let k = match piece.get(&start) {
                        Some(&k) => k,
                        None => {
                            let k = members.len();
                            let group = self.faces[&start].group;
                            piece.insert(start, k);
                            let mut all = vec![start];
                            let mut todo = vec![start];
                            while let Some(id) = todo.pop() {
                                for l in self.faces[&id].loops() {
                                    for (a, b) in pairs(l) {
                                        for &n in by_edge.get(&(group, a.min(b), a.max(b))).into_iter().flatten() {
                                            if let std::collections::hash_map::Entry::Vacant(v) = piece.entry(n) {
                                                v.insert(k);
                                                all.push(n);
                                                todo.push(n);
                                            }
                                        }
                                    }
                                }
                            }
                            members.push(all);
                            k
                        }
                    };
                    wanted.entry(k).or_default().insert(mat);
                }
            }
            for (k, mats) in &wanted {
                out.extend(members[*k].iter().copied().filter(|id| mats.contains(&self.faces[id].material.as_deref())));
            }
            return Ok(out.into_iter().collect());
        }
        for &s in &seeds {
            // a seed already reached from an earlier one gives the same faces again (the walk stays in its group, and
            // a face it kept wears the same material)
            if out.contains(&s) {
                continue;
            }
            let sf = &self.faces[&s];
            let (group, mat) = (sf.group, sf.material.clone());
            let mut seen: BTreeSet<Id> = BTreeSet::from([s]);
            let mut todo = vec![s];
            while let Some(id) = todo.pop() {
                for l in self.faces[&id].loops() {
                    for (a, b) in pairs(l) {
                        for &n in by_edge.get(&(group, a.min(b), a.max(b))).into_iter().flatten() {
                            if self.faces[&n].material == mat && seen.insert(n) {
                                todo.push(n);
                            }
                        }
                    }
                }
            }
            out.extend(seen.into_iter().filter(|id| self.faces[id].material == mat));
        }
        Ok(out.into_iter().collect())
    }

    /// Give `faces` the material `name`. With `through`, a picture material is laid across the faces that touch
    /// along an edge as one continuous sheet: the first face of each touching run gets the material's own
    /// placement, every neighbour's is that placement unfolded across the shared edge (same tile size, lined up
    /// on the edge, turned into the neighbour's plane). Plain colors and projected pictures just fill.
    pub fn apply_paint(&mut self, faces: &[Id], name: &str, through: bool) -> Result<()> {
        let tex = self.materials.get(name).ok_or_else(|| anyhow::anyhow!("no material {name}"))?.texture.clone();
        for f in faces {
            let face = self.faces.get_mut(f).ok_or_else(|| anyhow::anyhow!("{f} is not a face"))?;
            face.material = Some(name.to_string());
            face.tex = None;
        }
        if !through || tex.is_none_or(|t| t.project.is_some()) {
            return Ok(());
        }
        let set: BTreeSet<Id> = faces.iter().copied().collect();
        let mut by_edge: HashMap<(Option<Id>, Id, Id), Vec<Id>> = HashMap::new();
        for &id in &set {
            let f = &self.faces[&id];
            for l in f.loops() {
                for (a, b) in pairs(l) {
                    by_edge.entry((f.group, a.min(b), a.max(b))).or_default().push(id);
                }
            }
        }
        let mut placed: BTreeSet<Id> = BTreeSet::new();
        for &start in faces {
            if placed.contains(&start) {
                continue;
            }
            let n0 = self.normal(start);
            let (u, v) = geom::basis(n0);
            self.faces.get_mut(&start).unwrap().tex = Some(TexPlace { material: name.to_string(), u: u.into(), v: v.into(), origin: [0.0; 3] });
            placed.insert(start);
            let mut todo = vec![start];
            while let Some(a) = todo.pop() {
                let (fa, na) = (self.faces[&a].clone(), self.normal(a));
                let pa = fa.tex.clone().unwrap();
                for l in fa.loops() {
                    for (i, j) in pairs(l) {
                        let key = (fa.group, i.min(j), i.max(j));
                        for &b in by_edge.get(&key).into_iter().flatten() {
                            if placed.contains(&b) {
                                continue;
                            }
                            let nb = self.normal(b);
                            let (p0, p1) = (self.vertices[&i].p, self.vertices[&j].p);
                            let e = (p1 - p0).normalize_or_zero();
                            if e == DVec3::ZERO || nb == DVec3::ZERO {
                                continue;
                            }
                            let angle = na.cross(nb).dot(e).atan2(na.dot(nb));
                            let q = glam::DQuat::from_axis_angle(e, angle);
                            let place = TexPlace {
                                material: name.to_string(),
                                u: (q * DVec3::from(pa.u)).normalize().into(),
                                v: (q * DVec3::from(pa.v)).normalize().into(),
                                origin: (p0 + q * (DVec3::from(pa.origin) - p0)).into(),
                            };
                            self.faces.get_mut(&b).unwrap().tex = Some(place);
                            placed.insert(b);
                            todo.push(b);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// How many faces use material `name`, in the model and in every component.
    pub fn material_uses(&self, name: &str) -> usize {
        self.faces.values().filter(|f| f.material.as_deref() == Some(name)).count() + self.definitions.values().map(|d| d.model.material_uses(name)).sum::<usize>()
    }

    /// Take material `name` (and its picture) out of the model. Faces that use it (here and in components) go to
    /// `replace_with`, an existing material, or back to the plain default look. Returns how many faces were changed.
    pub fn remove_material(&mut self, name: &str, replace_with: Option<&str>) -> Result<usize> {
        if !self.materials.contains_key(name) && self.material_uses(name) == 0 && !self.definitions.values().any(|d| d.model.materials.contains_key(name)) {
            bail!("no material {name}");
        }
        if let Some(r) = replace_with {
            if r == name {
                bail!("a material can't replace itself");
            }
            if !self.materials.contains_key(r) {
                bail!("no material {r} to put in its place");
            }
        }
        fn go(m: &mut Model, name: &str, to: Option<&str>) -> usize {
            let mut n = 0;
            for f in m.faces.values_mut().filter(|f| f.material.as_deref() == Some(name)) {
                f.material = to.map(str::to_string);
                n += 1;
            }
            m.materials.remove(name);
            for d in m.definitions.values_mut() {
                // look first: a shared component model is only copied if it uses the material
                if d.model.material_uses(name) > 0 || d.model.materials.contains_key(name) {
                    n += go(std::sync::Arc::make_mut(&mut d.model), name, to);
                }
            }
            n
        }
        Ok(go(self, name, replace_with))
    }

    /// Remove every material that no face uses (and no placement or component names in its `material` attribute).
    /// Returns the names removed, and about how many bytes of pictures went with them.
    pub fn purge_materials(&mut self) -> (Vec<String>, usize) {
        let named: BTreeSet<String> = {
            fn collect(m: &Model, out: &mut BTreeSet<String>) {
                for a in m.instances.values().map(|i| &i.attrs).chain(m.definitions.values().map(|d| &d.attrs)) {
                    for (k, v) in a {
                        if k.eq_ignore_ascii_case("material") {
                            out.insert(v.trim().to_string());
                        }
                    }
                }
                for d in m.definitions.values() {
                    collect(&d.model, out);
                }
            }
            let mut out = BTreeSet::new();
            collect(self, &mut out);
            out
        };
        let unused: Vec<String> = self.materials.keys().filter(|k| self.material_uses(k) == 0 && !named.contains(*k)).cloned().collect();
        let mut bytes = 0;
        for k in &unused {
            bytes += self.materials.get(k).and_then(|m| m.texture.as_ref()).map_or(0, |t| t.data.len());
            let _ = self.remove_material(k, None);
        }
        (unused, bytes)
    }

    /// A group and every group nested in it, at any depth.
    pub fn subtree(&self, gid: Id) -> BTreeSet<Id> {
        let mut out = BTreeSet::from([gid]);
        loop {
            let more: Vec<Id> = self.groups.iter().filter(|(k, g)| !out.contains(k) && g.parent.is_some_and(|p| out.contains(&p))).map(|(&k, _)| k).collect();
            if more.is_empty() {
                return out;
            }
            out.extend(more);
        }
    }

    /// A group and the groups it sits inside, innermost first.
    pub fn ancestors(&self, gid: Option<Id>) -> Vec<Id> {
        let mut out = Vec::new();
        let mut cur = gid;
        while let Some(g) = cur {
            if out.contains(&g) || out.len() > 256 {
                break; // a loop (shouldn't happen): stop rather than spin
            }
            out.push(g);
            cur = self.groups.get(&g).and_then(|x| x.parent);
        }
        out
    }

    /// Is this group, or anything it sits inside, locked? (Its name, if so.)
    pub fn locked_by(&self, gid: Option<Id>) -> Option<String> {
        self.ancestors(gid).into_iter().find(|g| self.groups[g].locked).map(|g| self.groups[&g].name.clone())
    }

    /// Put groups and placements inside `parent` (None: top level).
    pub fn set_parent(&mut self, ids: &[Id], parent: Option<Id>) -> Result<()> {
        if let Some(p) = parent {
            if !self.groups.contains_key(&p) {
                bail!("{p} is not a group");
            }
        }
        for id in ids {
            if self.groups.contains_key(id) {
                if parent.is_some_and(|p| self.subtree(*id).contains(&p)) {
                    bail!("a group can't go inside itself");
                }
                self.groups.get_mut(id).unwrap().parent = parent;
            } else if let Some(i) = self.instances.get_mut(id) {
                i.parent = parent;
            } else {
                bail!("{id} is not a group or component");
            }
        }
        Ok(())
    }

    /// Dissolve a group: its geometry, groups and placements move up to
    /// the group it sat in.
    pub fn ungroup(&mut self, gid: Id) -> Result<()> {
        let g = self.groups.remove(&gid).ok_or_else(|| anyhow!("{gid} is not a group"))?;
        let up = g.parent;
        for f in self.faces.values_mut().filter(|f| f.group == Some(gid)) {
            f.group = up;
        }
        for e in self.edges.values_mut().filter(|e| e.group == Some(gid)) {
            e.group = up;
        }
        for v in self.vertices.values_mut().filter(|v| v.group == Some(gid)) {
            v.group = up;
        }
        for c in self.groups.values_mut().filter(|c| c.parent == Some(gid)) {
            c.parent = up;
        }
        for i in self.instances.values_mut().filter(|i| i.parent == Some(gid)) {
            i.parent = up;
        }
        if self.active_group == Some(gid) {
            self.active_group = up;
        }
        Ok(())
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.vertices.values().map(|v| v.p);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }
}

/// A model whose counter or any id is above this is refused on Open, Restore and insert (the program never writes one:
/// that is 4 billion ids; a file from outside might). The room left is more than any command can use.
pub const ID_LIMIT: Id = u32::MAX - 50_000_000;
/// What the panic says when the counter would pass the top of the id range (turned into a plain error reply).
pub const IDS_OUT: &str = "this model has used up its ids";

/// Free text (names, notes) a model may hold in total.
///
/// The size budget (`MAX_MODEL_BYTES`) is what bounds the whole model; this one stops text from using all of it. 64 MB
/// holds a model of a million named, tagged placements. An agent or a plugin gets a smaller share (`MAX_TEXT_BYTES_UNTRUSTED`:
/// it may not take the model's text past 2 MB), so the person's own naming, notes and colours always have room.
pub const MAX_TEXT_BYTES: usize = 64 << 20;
pub const MAX_TEXT_BYTES_UNTRUSTED: usize = 2 << 20;

fn attr_bytes(m: &BTreeMap<String, String>) -> usize {
    m.iter().map(|(k, v)| k.len() + v.len() + 8).sum()
}

/// What the text costs in the file: its bytes, plus what escaping adds (`"` and `\\` are written as two bytes; a control
/// character as two or six). Counted in one pass over the bytes, so the estimate never falls under the real size
/// whatever characters a string holds (non-ASCII is written as it is, so its length already counts).
pub fn jl(s: &str) -> usize {
    let b = s.as_bytes();
    let mut extra = 0usize;
    // 240 bytes at a time in byte-sized counters: the compiler turns this into vector instructions (a 300 MB picture in
    // a few tens of milliseconds, where counting with two passes and word-sized counters took 0.2 s)
    let mut chunks = b.chunks_exact(240);
    for chunk in &mut chunks {
        let (mut quoted, mut ctrl) = (0u8, 0u8);
        for &c in chunk {
            quoted += ((c == b'"') | (c == b'\\')) as u8;
            ctrl += (c < 0x20) as u8;
        }
        extra += quoted as usize + 5 * ctrl as usize;
    }
    for &c in chunks.remainder() {
        extra += ((c == b'"') | (c == b'\\')) as usize + 5 * (c < 0x20) as usize;
    }
    b.len() + extra
}

/// `attr_bytes` as the file holds it (escaping counted).
fn attr_json(m: &BTreeMap<String, String>) -> usize {
    m.iter().map(|(k, v)| jl(k) + jl(v) + 8).sum()
}

/// Bytes `serde_json` would write for a value (counted, never stored).
fn jsz<T: Serialize + ?Sized>(v: &T) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 += b.len();
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Count(0);
    serde_json::to_writer(&mut c, v).ok();
    c.0
}

/// Largest coordinate (meters) the model accepts anywhere.
pub const MAX_COORD: f64 = 1e9;

/// Every number of a point or direction is finite (no range check: see `sane3`).
pub fn finite3(v: &[f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// A number the model can hold: finite and within `MAX_COORD`.
pub fn sane(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_COORD
}

pub fn sane3(v: DVec3) -> bool {
    sane(v.x) && sane(v.y) && sane(v.z)
}

/// The most one command may add (copies of heavy groups), and the most a model may hold:
/// past these a model can't be saved, reloaded or drawn in reasonable time or memory.
///
/// Why 2 million: the whole model is written to one file on every change and read back by every
/// program that shares it. About 70 bytes of JSON per point, edge or face puts 2 million at around
/// 140 MB, which is about a second to save and a few to reload on an ordinary disk; the old 8 million
/// meant a 470 MB file, 4 s per command and 1.2 GB of memory (measured in the round-4 tests). Real
/// models are far below it (a 360,000-line site plan is under 1 million).
pub const MAX_GROWTH: usize = 500_000;
pub const MAX_ENTITIES: usize = 2_000_000;
/// The most bulk data (textures, photos on views, scan points) a model may hold, and the most one command may add:
/// the person's (imports of photos or scans) and an agent's or a plugin's. The geometry limits above don't see these,
/// and every one of them is rewritten on each save, read by each program and copied into each backup.
///
/// The one limit that counts everything is the model's estimated file size (`size_estimate`: every string, map and
/// image is in it). The person's models may go up to `MAX_MODEL_BYTES`; the file of such a model is always under
/// `commands::MAX_OPEN_BYTES`, so a model the program wrote can be opened again. An agent or a plugin may add at most
/// `MAX_GROWTH_BYTES_UNTRUSTED` in one command and may not take a model past `MAX_MODEL_BYTES_UNTRUSTED` in all.
pub const MAX_MODEL_BYTES: usize = 512 << 20;
pub const MAX_MODEL_BYTES_UNTRUSTED: usize = 160 << 20;
pub const MAX_GROWTH_BYTES_UNTRUSTED: usize = 32 << 20;
/// The most keys one component or placement may carry in its Dynamic Component attributes.
pub const MAX_ATTRS: usize = 256;

thread_local! {
    /// what the command running on this thread may add (see `Engine::exec`)
    static GROWTH: std::cell::Cell<usize> = const { std::cell::Cell::new(MAX_GROWTH) };
}

/// Set how many points, edges and faces the command about to run may add in one go.
pub fn set_growth_limit(n: usize) {
    GROWTH.with(|g| g.set(n));
}

impl Model {
    /// Every number the model holds is finite and within `MAX_COORD`. A model
    /// that fails this can't be saved (NaN would be written as `null` and the
    /// file could never be opened again), so commands that produce one are
    /// undone.
    pub fn check_sane(&self) -> Result<()> {
        self.check_sane_with(true)
    }

    /// `check_sane`, with or without the limit on how much geometry the model holds (a model over it
    /// is opened for viewing and for deleting parts, never refused outright).
    pub fn check_sane_with(&self, count: bool) -> Result<()> {
        self.check_sane_inner(count, None)
    }

    /// `check_sane` for a caller that has just worked out the size estimate (a walk over every string and picture).
    pub fn check_sane_est(&self, estimate: usize) -> Result<()> {
        self.check_sane_inner(true, Some(estimate))
    }

    fn check_sane_inner(&self, count: bool, estimate: Option<usize>) -> Result<()> {
        let bad = |what: &str| -> Result<()> { bail!("that would put a non-finite or absurdly large number ({what}) in the model, so it was not done") };
        if self.next_id > ID_LIMIT || self.max_own_id() > ID_LIMIT {
            bail!("this model's ids are too close to the top of the range ({} is the most the program accepts), so it can't be used: a file edited by hand or damaged may do this; open an earlier copy", ID_LIMIT);
        }
        if !self.vertices.values().all(|v| sane3(v.p)) {
            return bad("a point");
        }
        if let Some(id) = self.faces.iter().find(|(_, f)| !f.data_sane()).map(|(id, _)| *id) {
            return bad(&format!("the texture coordinates or vertex colours of face {id}"));
        }
        if !self.guides.iter().all(|g| sane3(g.p) && g.dir.is_none_or(sane3)) {
            return bad("a guide");
        }
        for a in &self.annotations {
            let ok = match a {
                Annotation::Dimension { a, b, offset, .. } => sane3(*a) && sane3(*b) && sane3(*offset),
                Annotation::Text { at, leader, .. } => sane3(*at) && leader.is_none_or(sane3),
                Annotation::Swing { hinge, from, to, .. } => sane3(*hinge) && sane3(*from) && sane3(*to),
            };
            if !ok {
                return bad("an annotation");
            }
        }
        if !self.instances.values().all(|i| i.transform.to_cols_array().iter().all(|&x| sane(x))) {
            return bad("a placement");
        }
        if !self.views.iter().all(|v| v.eye.iter().chain(v.target.iter()).all(|&x| sane(x)) && sane(v.fov)) {
            return bad("a view");
        }
        if !self.sections.iter().all(|s| s.origin.iter().chain(s.normal.iter()).all(|&x| sane(x))) {
            return bad("a section plane");
        }
        if let Some(ax) = &self.axes {
            if !ax.iter().flatten().all(|&x| sane(x)) {
                return bad("the axes");
            }
        }
        if !self.cages.values().all(|c| c.points.iter().flatten().all(|&x| sane(x))) {
            return bad("a cage");
        }
        if !self.sheets.values().all(|s| s.sane()) || !self.sheet_items.values().all(|i| i.sane()) || !self.title_blocks.values().all(|t| t.sane()) || !self.drafting.sane() {
            return bad("a number on a drawing sheet");
        }
        for c in self.clouds.values() {
            if !c.points.iter().all(|&x| sane(x as f64)) {
                return bad("a point cloud");
            }
        }
        for d in self.definitions.values() {
            d.model.check_sane_with(false)?;
        }
        if count && self.entity_count() > MAX_ENTITIES {
            bail!("the model has more than {MAX_ENTITIES} points, edges and faces ({}), so it can't be saved: delete parts of it until it is under the limit", self.entity_count());
        }
        let est = if count { estimate.unwrap_or_else(|| self.size_estimate()) } else { 0 };
        if count && est > MAX_MODEL_BYTES {
            bail!("the model would be about {} MB (names, notes, attributes, images and scans included), over the limit of {} MB, so that was not done", est >> 20, MAX_MODEL_BYTES >> 20);
        }
        if count && self.attrs_over() {
            bail!("a component or placement has more than {MAX_ATTRS} attributes, so that was not done");
        }
        if count && self.text_bytes() > MAX_TEXT_BYTES {
            bail!("the model's names, notes and attributes add up to more than {} MB ({}), so that was not done", MAX_TEXT_BYTES >> 20, self.text_use());
        }
        Ok(())
    }

    /// Take out any picture (a texture, a photo on a view; components' too) that is over the picture limits, which only a file from
    /// outside can hold: the person's window would decode it at whatever size it claims. The material keeps its color, the view its
    /// camera. Returns what was taken out, for the notice ("texture Brick: 30000 x 30000 pixels").
    pub fn vet_pictures(&mut self) {
        for what in self.drop_oversize_pictures() {
            crate::safety::note_skipped(&what, "over the picture limits, so the picture was left out of the model (the rest is as it was)");
        }
    }

    /// Bring the drawing sheets a file holds (paper sizes, tables, paper texts, hatch scales, hatch patterns) within
    /// what the commands make (see `drafting::clamp_to_limits`); a file within the limits is not touched. Sheets are
    /// the root model's, so components' models are not looked at. Returns what was changed (at most 20), for a notice.
    pub fn vet_sheets(&mut self) -> Vec<String> {
        let mut out = crate::drafting::clamp_to_limits(&mut self.sheets, &mut self.sheet_items, &mut self.drafting);
        if out.len() > 20 {
            let more = out.len() - 20;
            out.truncate(20);
            out.push(format!("and {more} more"));
        }
        out
    }

    /// Take out per-face data (texture coordinates, the second set, vertex colours, a painted-through placement) that
    /// holds something that isn't a number or is out of range, which only a damaged file can hold (one written with
    /// `null`s, say): those faces fall back to their material's own placement and colour, and the rest of the model
    /// opens as it was. Components' models too. A notice says how many faces lost it.
    pub fn vet_face_data(&mut self) {
        let n = self.drop_bad_face_data();
        if n > 0 {
            crate::safety::note_skipped(&format!("{n} face{}", if n == 1 { "" } else { "s" }), "held texture coordinates or vertex colours that weren't numbers (a damaged file), so those were left out; the faces keep their material");
        }
    }

    fn drop_bad_face_data(&mut self) -> usize {
        let mut n = 0;
        for f in self.faces.values_mut() {
            if f.data_sane() {
                continue;
            }
            n += 1;
            if f.uv.as_ref().is_some_and(|u| !u.sane()) {
                f.uv = None;
            }
            if f.uv2.as_ref().is_some_and(|u| !u.sane()) {
                f.uv2 = None;
            }
            if f.vcol.as_ref().is_some_and(|c| !c.sane()) {
                f.vcol = None;
            }
            if f.tex.as_ref().is_some_and(|t| !t.sane()) {
                f.tex = None;
            }
        }
        for d in self.definitions.values_mut() {
            if !d.model.face_data_sane() {
                n += std::sync::Arc::make_mut(&mut d.model).drop_bad_face_data();
            }
        }
        n
    }

    /// Every face's per-corner data is sane, components' too.
    fn face_data_sane(&self) -> bool {
        self.faces.values().all(Face::data_sane) && self.definitions.values().all(|d| d.model.face_data_sane())
    }

    fn drop_oversize_pictures(&mut self) -> Vec<String> {
        let mut out = vec![];
        let bad = |d: &str| crate::safety::picture_too_big(d);
        for (k, m) in self.materials.iter_mut() {
            if let Some((w, h)) = m.texture.as_ref().and_then(|t| bad(&t.data)) {
                m.texture = None;
                out.push(format!("texture {k}: {w} x {h} pixels"));
            }
        }
        for v in self.views.iter_mut() {
            if let Some((w, h)) = v.photo.as_deref().and_then(bad) {
                v.photo = None;
                out.push(format!("photo of view {}: {w} x {h} pixels", v.name));
            }
        }
        for d in self.definitions.values_mut() {
            // look first: a shared component model is only copied if it holds a picture to take out
            let any = d.model.materials.values().any(|m| m.texture.as_ref().is_some_and(|t| bad(&t.data).is_some())) || d.model.views.iter().any(|v| v.photo.as_deref().is_some_and(|p| bad(p).is_some())) || d.model.definitions.values().any(|x| x.model.materials.values().any(|m| m.texture.is_some()));
            if any {
                out.extend(std::sync::Arc::make_mut(&mut d.model).drop_oversize_pictures());
            }
        }
        out.truncate(20);
        out
    }

    /// A component or placement carries more attributes than a command may give it (a file from outside can).
    pub fn attrs_over(&self) -> bool {
        self.definitions.values().any(|d| d.attrs.len() > MAX_ATTRS) || self.instances.values().any(|i| i.attrs.len() > MAX_ATTRS)
    }

    /// Over any of the limits a command may not take a model past (geometry, bytes, text, attributes): such a model is
    /// opened for looking at and cutting down, never refused (see `check_sane_with`).
    pub fn over_limits(&self) -> bool {
        self.entity_count() > MAX_ENTITIES || self.size_estimate() > MAX_MODEL_BYTES || self.text_bytes() > MAX_TEXT_BYTES || self.attrs_over()
    }

    /// What the text in the model is made of, for a message ("placements 7.9 MB, components 0.1 MB, everything else 0.2 MB").
    pub fn text_use(&self) -> String {
        let mb = |n: usize| format!("{:.1} MB", n as f64 / (1 << 20) as f64);
        let places: usize = self.instances.values().map(|i| i.name.len() + i.tag.as_ref().map_or(0, String::len) + attr_bytes(&i.attrs)).sum();
        let comps: usize = self.definitions.values().map(|d| d.name.len() + d.source.as_ref().map_or(0, String::len) + attr_bytes(&d.attrs) + d.model.text_bytes()).sum();
        let all = self.text_bytes();
        format!("placements {}, components {}, everything else {}", mb(places), mb(comps), mb(all.saturating_sub(places + comps)))
    }

    /// Bytes of bulk data the model holds: texture images, photos on views, scan points (components' own too).
    pub fn bulk_bytes(&self) -> usize {
        let tex: usize = self.materials.values().map(|m| m.texture.as_ref().map_or(0, |t| t.data.len())).sum();
        let photos: usize = self.views.iter().map(|v| v.photo.as_ref().map_or(0, String::len)).sum();
        let clouds: usize = self.clouds.values().map(|c| 4 * c.points.len() + c.colors.len()).sum();
        tex + photos + clouds + self.definitions.values().map(|d| d.model.bulk_bytes()).sum::<usize>()
    }

    /// Bytes of names, notes and other free text the model holds (counted
    /// toward the undo budget and capped so they can't be used to exhaust memory).
    pub fn text_bytes(&self) -> usize {
        let notes: usize = self.annotations.iter().map(|a| if let Annotation::Text { text, .. } = a { text.len() } else { 0 }).sum();
        let views: usize = self.views.iter().map(|v| v.name.len() + v.style.as_ref().map_or(0, |s| s.to_string().len()) + v.tags.as_ref().map_or(0, |t| t.keys().map(|k| k.len() + 8).sum())).sum();
        self.materials.keys().map(|k| k.len() + 16).sum::<usize>()
            + self.tags.keys().map(|k| k.len() + 16).sum::<usize>()
            + self.tag_colors.keys().map(|k| k.len() + 16).sum::<usize>()
            + self.groups.values().map(|g| g.name.len() + g.tag.as_ref().map_or(0, String::len)).sum::<usize>()
            + self.classes.values().map(String::len).sum::<usize>()
            + self.sections.iter().map(|s| s.name.len()).sum::<usize>()
            + self.clouds.values().map(|c| c.name.len()).sum::<usize>()
            + self.definitions.values().map(|d| d.name.len() + d.source.as_ref().map_or(0, String::len) + attr_bytes(&d.attrs) + d.model.text_bytes()).sum::<usize>()
            + self.instances.values().map(|i| i.name.len() + i.tag.as_ref().map_or(0, String::len) + attr_bytes(&i.attrs)).sum::<usize>()
            + self.timeline.clips.iter().map(|c| c.name.as_ref().map_or(0, String::len)).sum::<usize>()
            + self.environment.as_ref().map_or(0, String::len)
            + self.units.len()
            + self.sheets.values().map(|s| s.text_bytes()).sum::<usize>()
            + self.sheet_items.values().map(|i| i.text_bytes()).sum::<usize>()
            + self.title_blocks.values().map(|t| t.text_bytes()).sum::<usize>()
            + self.drafting.text_bytes()
            + self.photo_points.iter().map(|p| p.name.len() + p.marks.keys().map(|k| k.len() + 8).sum::<usize>()).sum::<usize>()
            + notes
            + views
    }

    /// About how big this model's file would be (what `serde_json` writes, a little over): every free string,
    /// map and image counted by its length, geometry by its count. It never serializes the model, so it costs a
    /// walk over the small collections and a count of the big ones (a few milliseconds at a million entities),
    /// which is why every command can afford it. The constants are on the high side of what the writer produces (a
    /// test compares them with real files), so the open limit has room over this limit.
    pub fn size_estimate(&self) -> usize {
        let faces: usize = self.faces.values().map(|f| 80 + 10 * f.outer.len() + f.holes.iter().map(|h| 10 * h.len() + 4).sum::<usize>() + f.material.as_ref().map_or(0, |m| jl(m) + 4) + f.data_estimate()).sum();
        let mats: usize = self.materials.iter().map(|(k, m)| jl(k) + 200 + m.texture.as_ref().map_or(0, |t| jl(&t.data) + 200)).sum();
        let clouds: usize = self.clouds.values().map(|c| jl(&c.name) + 128 + (4 * c.points.len() + c.colors.len()) * 4 / 3 + 8).sum();
        let views: usize = self.views.iter().map(|v| 320 + jl(&v.name) + v.photo.as_ref().map_or(0, |p| jl(p)) + v.style.as_ref().map_or(0, jsz) + v.tags.as_ref().map_or(0, |t| t.keys().map(|k| jl(k) + 12).sum()) + v.hidden.as_ref().map_or(0, |h| 12 * h.len())).sum();
        let anns: usize = self.annotations.iter().map(|a| 260 + if let Annotation::Text { text, .. } = a { jl(text) } else { 0 }).sum();
        let groups: usize = self.groups.values().map(|g| 110 + jl(&g.name) + g.tag.as_ref().map_or(0, |t| jl(t))).sum();
        let insts: usize = self.instances.values().map(|i| 420 + jl(&i.name) + i.tag.as_ref().map_or(0, |t| jl(t)) + attr_json(&i.attrs)).sum();
        let defs: usize = self.definitions.values().map(|d| 160 + jl(&d.name) + d.source.as_ref().map_or(0, |t| jl(t)) + attr_json(&d.attrs) + d.model.size_estimate()).sum();
        let tags: usize = self.tags.keys().chain(self.tag_colors.keys()).map(|k| jl(k) + 40).sum();
        let photo: usize = self.photo_points.iter().map(|p| 40 + jl(&p.name) + p.marks.keys().map(|k| jl(k) + 50).sum::<usize>()).sum();
        256 + self.vertices.len() * 120 + self.edges.len() * 80 + faces + mats + clouds + views + anns + groups + insts + defs + tags + photo
            + self.guides.len() * 150
            + self.cages.values().map(|c| 16 + jsz(c)).sum::<usize>()
            + self.photo_edges.len() * 16
            + self.photo_guides.len() * 12
            + jsz(&self.timeline)
            + self.classes.values().map(|c| jl(c) + 30).sum::<usize>()
            + self.sections.iter().map(|s| 280 + jl(&s.name)).sum::<usize>()
            + self.environment.as_ref().map_or(0, |e| jl(e) + 20)
            + jl(&self.units)
            + self.sheets.values().map(|s| 400 + 2 * s.text_bytes()).sum::<usize>()
            + self.sheet_items.values().map(|i| i.size_estimate()).sum::<usize>()
            + self.title_blocks.values().map(|t| 300 + 2 * t.text_bytes() + 60 * t.lines_mm.len() + 80 * t.fields.len()).sum::<usize>()
            + if self.drafting.is_empty() { 0 } else { 2 * self.drafting.text_bytes() + 400 * (self.drafting.tag_styles.len() + self.drafting.dim_styles.len() + self.drafting.text_styles.len() + self.drafting.line_types.len()) + 64 }
    }

    /// Things in the model that refer to something that isn't there, as short phrases ("edge 7", "material #ff0000"),
    /// sorted, at most `MAX_PROBLEMS`. `full` false: only what would make drawing or saving panic or fail (a point,
    /// a component definition that is missing); `full` true: also every group, material and other reference.
    pub fn broken_refs(&self, full: bool) -> Vec<String> {
        let root = Root { defs: self.definitions.keys().copied().collect(), materials: self.materials.keys().cloned().collect() };
        self.broken_refs_in(full, &root)
    }

    /// `broken_refs` for a model that is a component's own: its placements name definitions in the *root* model's
    /// table (`root`), not in its own (usually empty) one; a standalone file (a library part) has its own as well.
    fn broken_refs_in(&self, full: bool, root: &Root) -> Vec<String> {
        const MAX_PROBLEMS: usize = 50;
        let mut out: BTreeSet<String> = BTreeSet::new();
        let mut add = |s: String| {
            if out.len() < MAX_PROBLEMS {
                out.insert(s);
            }
        };
        let edge_keys: Option<std::collections::HashSet<(Id, Id)>> = full.then(|| self.edges.values().map(|e| (e.a.min(e.b), e.a.max(e.b))).collect());
        let group_ok = |g: &Option<Id>| g.is_none_or(|g| self.groups.contains_key(&g));
        for (id, v) in &self.vertices {
            if full && !group_ok(&v.group) {
                add(format!("point {id} (group)"));
            }
        }
        for (id, e) in &self.edges {
            if !self.vertices.contains_key(&e.a) || !self.vertices.contains_key(&e.b) {
                add(format!("edge {id}"));
            }
            if full && !group_ok(&e.group) {
                add(format!("edge {id} (group)"));
            }
        }
        for (id, f) in &self.faces {
            for l in f.loops() {
                if l.iter().any(|v| !self.vertices.contains_key(v)) {
                    add(format!("face {id}"));
                    continue;
                }
                if let Some(keys) = &edge_keys {
                    for i in 0..l.len() {
                        let (a, b) = (l[i], l[(i + 1) % l.len()]);
                        if !keys.contains(&(a.min(b), a.max(b))) {
                            add(format!("face {id}"));
                            break;
                        }
                    }
                }
            }
            if full {
                if !group_ok(&f.group) {
                    add(format!("face {id} (group)"));
                }
                if let Some(m) = f.material.as_ref().filter(|m| !self.materials.contains_key(*m) && !root.materials.contains(*m)) {
                    add(format!("material {m}"));
                }
            }
        }
        for (id, i) in &self.instances {
            if !self.definitions.contains_key(&i.def) && !root.defs.contains(&i.def) {
                add(format!("placement {id}"));
            }
            if full && !group_ok(&i.parent) {
                add(format!("placement {id} (group)"));
            }
        }
        if full {
            for (id, g) in &self.groups {
                if !group_ok(&g.parent) {
                    add(format!("group {id}"));
                }
            }
            if !group_ok(&self.active_group) {
                add("the group being edited".into());
            }
            for id in self.cages.keys() {
                if !self.groups.contains_key(id) {
                    add(format!("subdivision cage {id}"));
                }
            }
            for (id, s) in &self.sheets {
                if s.title_block.is_some_and(|t| !self.title_blocks.contains_key(&t)) {
                    add(format!("sheet {id} (title block)"));
                }
            }
            for (id, i) in &self.sheet_items {
                match i.sheet() {
                    Some(s) if self.sheets.contains_key(&s) => {}
                    _ => add(format!("sheet item {id}")),
                }
                if i.title_block().is_some_and(|t| !self.title_blocks.contains_key(&t)) {
                    add(format!("sheet item {id} (title block)"));
                }
            }
            for id in self.classes.keys() {
                if !self.groups.contains_key(id) && !self.instances.contains_key(id) {
                    add(format!("class of {id}"));
                }
            }
            let guide_ids: std::collections::HashSet<Id> = self.guides.iter().map(|g| g.id).collect();
            for id in &self.photo_guides {
                if !guide_ids.contains(id) {
                    add(format!("photo guide {id}"));
                }
            }
        }
        for (id, d) in &self.definitions {
            // a component is a model of its own: held to the same rules, down the whole tree
            for p in d.model.broken_refs_in(full, root).into_iter().take(MAX_PROBLEMS) {
                add(format!("component {id}: {p}"));
            }
        }
        out.into_iter().collect()
    }

    /// The highest id used anywhere in this model (not inside components: each has its own counter).
    fn max_own_id(&self) -> Id {
        let top = |m: Option<&Id>| m.copied().unwrap_or(0);
        let mut hi = top(self.vertices.keys().next_back()).max(top(self.edges.keys().next_back())).max(top(self.faces.keys().next_back())).max(top(self.groups.keys().next_back())).max(top(self.instances.keys().next_back())).max(top(self.definitions.keys().next_back())).max(top(self.clouds.keys().next_back())).max(top(self.cages.keys().next_back())).max(top(self.classes.keys().next_back())).max(top(self.sheets.keys().next_back())).max(top(self.sheet_items.keys().next_back())).max(top(self.title_blocks.keys().next_back()));
        hi = hi.max(self.sheets.values().filter_map(|s| s.title_block).max().unwrap_or(0)).max(self.sheet_items.values().map(|i| i.max_id()).max().unwrap_or(0));
        hi = hi.max(self.guides.iter().map(|g| g.id).max().unwrap_or(0)).max(self.annotations.iter().map(Annotation::id).max().unwrap_or(0)).max(self.sections.iter().map(|s| s.id).max().unwrap_or(0)).max(self.photo_guides.iter().copied().max().unwrap_or(0));
        hi = hi.max(self.edges.values().filter_map(|e| e.curve).max().unwrap_or(0)).max(self.active_group.unwrap_or(0)).max(self.active_section.unwrap_or(0));
        hi
    }

    /// Is there any paper (sheets, items, title blocks, drafting styles) in this model or the models of its components?
    pub fn has_paper(&self) -> bool {
        !(self.sheets.is_empty() && self.sheet_items.is_empty() && self.title_blocks.is_empty() && self.drafting.is_empty()) || self.definitions.values().any(|d| d.model.has_paper())
    }

    /// Drop the drawing sheets, what is on them, the title blocks and the drafting styles (this model, and the models of its
    /// components): a model used as a part has no paper of its own.
    pub fn clear_paper(&mut self) {
        self.sheets.clear();
        self.sheet_items.clear();
        self.title_blocks.clear();
        self.drafting = Default::default();
        for d in self.definitions.values_mut() {
            // look first: a shared component model is only copied if it has paper to drop
            if d.model.has_paper() {
                std::sync::Arc::make_mut(&mut d.model).clear_paper();
            }
        }
    }

    /// Make every id counter in this model (and in every component) higher than any id in use, so that the next
    /// thing drawn can never take the id of something already there. A file written by another tool, edited by hand
    /// or damaged may carry a counter that is too low; the program's own files never do. Returns whether anything changed.
    pub fn repair_ids(&mut self) -> bool {
        let mut changed = false;
        let hi = self.max_own_id();
        if self.next_id < hi {
            self.next_id = hi;
            changed = true;
        }
        for d in self.definitions.values_mut() {
            // look first: a shared component model is only copied if it needs the repair
            if d.model.needs_id_repair() {
                changed |= std::sync::Arc::make_mut(&mut d.model).repair_ids();
            }
        }
        changed
    }

    fn needs_id_repair(&self) -> bool {
        self.next_id < self.max_own_id() || self.definitions.values().any(|d| d.model.needs_id_repair())
    }

    /// How much geometry the model holds (points, edges, faces and placements, with every
    /// component's own once). Copying and saving are bounded by this, not by how many ids
    /// a command names, since one id can stand for a whole subdivided model.
    pub fn entity_count(&self) -> usize {
        self.vertices.len() + self.edges.len() + self.faces.len() + self.instances.len() + self.definitions.values().map(|d| d.model.entity_count()).sum::<usize>()
    }

    /// What copying these faces, groups and placements once would add (see `entity_count`).
    pub fn copy_cost(&self, ids: &[Id]) -> usize {
        let mut cost = 0usize;
        let mut trees: BTreeSet<Id> = BTreeSet::new();
        for id in ids {
            if self.groups.contains_key(id) {
                trees.extend(self.subtree(*id));
            } else if self.faces.contains_key(id) {
                cost += 1 + self.faces[id].loops().map(|l| l.len()).sum::<usize>();
            } else if self.instances.contains_key(id) {
                cost += 1;
            }
        }
        if !trees.is_empty() {
            let inside = |g: &Option<Id>| g.is_some_and(|g| trees.contains(&g));
            cost += self.vertices.values().filter(|v| inside(&v.group)).count() + self.edges.values().filter(|e| inside(&e.group)).count() + self.faces.values().filter(|f| inside(&f.group)).count() + self.instances.values().filter(|i| inside(&i.parent)).count();
        }
        cost
    }

    /// Refuse to grow past the limits: `adding` more entities in one go, or the model's total.
    pub fn check_room(&self, adding: usize) -> Result<()> {
        let per_command = GROWTH.with(|g| g.get());
        if adding > per_command || self.entity_count().saturating_add(adding) > MAX_ENTITIES {
            bail!("that would add about {adding} points, edges and faces to a model of {}; the limits are {per_command} at a time and {MAX_ENTITIES} in all, so it was not done", self.entity_count());
        }
        Ok(())
    }

    /// Nothing drawn yet (a fresh model): not worth a backup.
    pub fn is_blank(&self) -> bool {
        self.vertices.is_empty() && self.edges.is_empty() && self.faces.is_empty() && self.groups.is_empty() && self.instances.is_empty() && self.definitions.is_empty() && self.clouds.is_empty() && self.annotations.is_empty() && self.guides.is_empty() && self.views.is_empty() && self.materials.is_empty() && self.sheets.is_empty()
    }

    /// A rough size in bytes, for budgeting undo history (not exact; shared
    /// component geometry isn't counted, since undo steps share it).
    pub fn approx_bytes(&self) -> usize {
        let tex: usize = self.materials.values().map(|m| 64 + m.texture.as_ref().map_or(0, |t| t.data.len())).sum();
        let faces: usize = self.faces.values().map(|f| 96 + 4 * (f.outer.len() + f.holes.iter().map(Vec::len).sum::<usize>()) + f.data_bytes()).sum();
        let photos: usize = self.views.iter().map(|v| 160 + v.photo.as_ref().map_or(0, String::len)).sum();
        let clouds: usize = self.clouds.values().map(|c| 64 + 4 * c.points.len() + c.colors.len()).sum();
        self.text_bytes() + clouds + self.vertices.len() * 64 + self.edges.len() * 72 + faces + self.groups.len() * 96 + self.instances.len() * 200 + self.definitions.len() * 128 + tex + photos + self.annotations.len() * 120 + self.guides.len() * 80 + self.sheets.len() * 160 + self.sheet_items.len() * 160
    }
}

#[cfg(test)]
mod lookup_tests {
    use super::*;

    /// A small deterministic stream of numbers in 0..1.
    fn stream(seed: u64) -> impl FnMut() -> f64 {
        let mut s = seed;
        move || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn the_lookup_finds_the_vertex_and_edge_a_full_scan_finds() {
        let mut m = Model::default();
        let mut r = stream(7);
        // points packed closer than the weld distance, in two groups and at large and small coordinates
        let place = |r: &mut dyn FnMut() -> f64, k: usize| {
            let base = [DVec3::ZERO, DVec3::splat(-3.0), DVec3::new(1e7, -2e6, 0.5)][k % 3];
            base + DVec3::new(r(), r(), r()) * 6e-6
        };
        let mut look = Lookup::default();
        for round in 0..4 {
            for k in 0..300 {
                let id = m.new_id();
                let p = place(&mut r, k);
                m.vertices.insert(id, Vertex { p, group: (k % 5 == 0).then_some(9) });
            }
            let ids: Vec<Id> = m.vertices.keys().copied().collect();
            for k in 0..200 {
                let (a, b) = (ids[(r() * ids.len() as f64) as usize], ids[(r() * ids.len() as f64) as usize]);
                let id = m.new_id();
                m.edges.insert(id, Edge { a, b, group: None, ..Default::default() });
                if k % 3 == 0 {
                    // the same ends again, and reversed
                    let id = m.new_id();
                    m.edges.insert(id, Edge { a: b, b: a, group: None, ..Default::default() });
                }
            }
            for k in 0..2000 {
                let p = place(&mut r, k);
                let group = (k % 4 == 0).then_some(9);
                let scan = m.vertices.iter().find(|(_, v)| v.group == group && v.p.distance(p) < WELD).map(|(&id, _)| id);
                assert_eq!(look.vertex(&m, p, group).unwrap(), scan, "round {round} point {k}");
                let (a, b) = (ids[(r() * ids.len() as f64) as usize], ids[(r() * ids.len() as f64) as usize]);
                assert_eq!(look.edge(&m, a, b), m.find_edge(a, b));
            }
            // edges from the stream's earlier picks are found too
            let some: Vec<(Id, Id)> = m.edges.values().map(|e| (e.a, e.b)).collect();
            for (a, b) in some {
                assert_eq!(look.edge(&m, b, a), m.find_edge(b, a));
            }
        }
    }

    /// `duplicate` of loose faces the plain way: a full scan for every corner and side.
    fn duplicate_by_scan(m: &mut Model, ids: &[Id], by: DVec3) {
        let mut map: HashMap<Id, Id> = HashMap::new();
        for fid in ids {
            let f = m.faces[fid].clone();
            let mut remap = |m: &mut Model, l: &[Id]| -> Vec<Id> {
                l.iter()
                    .map(|v| {
                        *map.entry(*v).or_insert_with(|| {
                            let p = m.p(*v) + by;
                            if by.length() < WELD {
                                let id = m.new_id();
                                m.vertices.insert(id, Vertex { p, group: f.group });
                                id
                            } else {
                                m.vertex_in(p, f.group)
                            }
                        })
                    })
                    .collect()
            };
            let outer = remap(m, &f.outer);
            let holes = f.holes.iter().map(|h| remap(m, h)).collect();
            m.insert_face(Face { outer, holes, ..f.clone() });
        }
    }

    fn tables(m: &Model) -> String {
        serde_json::to_string(&(&m.vertices, &m.edges, &m.faces)).unwrap()
    }

    #[test]
    fn copies_through_the_lookup_match_copies_by_full_scan() {
        let mut r = stream(11);
        for seed in 0..40 {
            let mut a = Model::default();
            // a few unit squares side by side, some in a group, each piled a few times in place
            let mut faces = Vec::new();
            for k in 0..4 {
                let group = (k % 3 == 2).then_some(5);
                let x = k as f64 + [0.0, 1e9 - 7.0, -12345.678][seed % 3];
                let c = [DVec3::new(x, 0.0, 0.0), DVec3::new(x + 1.0, 0.0, 0.0), DVec3::new(x + 1.0, 1.0, 0.0), DVec3::new(x, 1.0, 0.0)];
                let outer = c.iter().map(|p| a.vertex_in(*p, group)).collect();
                faces.push(a.insert_face(Face { outer, group, ..Default::default() }));
            }
            let mut b = a.clone();
            for step in 0..5 {
                let pick: Vec<Id> = faces.iter().copied().filter(|_| r() < 0.7).collect();
                let noise = [0.0, 1e-9, 0.999e-6, 1.001e-6, 1.5e-6, 3.999e-6, 4.001e-6][(r() * 7.0) as usize];
                let by = match step % 3 {
                    0 => DVec3::new(noise, 0.0, 0.0),
                    1 => DVec3::new(1.0 + noise, -noise, 0.0),
                    _ => DVec3::new(-noise, 1.0, noise),
                };
                let count = if step == 4 { 3 } else { 1 };
                let new = if count > 1 { a.array(&pick, by, count).unwrap() } else { a.duplicate(&pick, by).unwrap() };
                let before: BTreeSet<Id> = b.faces.keys().copied().collect();
                for k in 1..=count {
                    duplicate_by_scan(&mut b, &pick, by * k as f64);
                }
                assert_eq!(tables(&a), tables(&b), "seed {seed} step {step} by {by}");
                assert_eq!(new, b.faces.keys().copied().filter(|f| !before.contains(f)).collect::<Vec<_>>());
                faces.extend(new);
            }
        }
    }
}
