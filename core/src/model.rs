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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub p: DVec3,
    #[serde(default)]
    pub group: Option<Id>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Edge {
    pub a: Id,
    pub b: Id,
    #[serde(default)]
    pub group: Option<Id>,
    /// Soft edges are drawn only in X-ray / hidden-line debug: the seams of a
    /// curved surface, or the splits in a long street.
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    /// the library file it came from (placing that file again reuses it)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Annotation {
    Dimension { id: Id, a: DVec3, b: DVec3, offset: DVec3 },
    Text { id: Id, at: DVec3, text: String, #[serde(default)] leader: Option<DVec3> },
    /// A door's swing on plan: the leaf turns about `hinge` from closed
    /// (reaching `from`, across the opening) to open (reaching `to`).
    Swing { id: Id, hinge: DVec3, from: DVec3, to: DVec3 },
}

impl Annotation {
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
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Material {
    /// The color (for textures: the image's average color, used by renders).
    pub color: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<Texture>,
}

/// An image material. Faces show it at `width` x `height` meters per tile,
/// laid along each face, or, with `project`, projected along one direction so
/// the picture runs across every face it's painted on.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub fn uses_edge(&self, a: Id, b: Id) -> bool {
        self.loops().any(|l| pairs(l).any(|(x, y)| (x == a && y == b) || (x == b && y == a)))
    }
    pub fn reverse(&mut self) {
        self.outer.reverse();
        for h in &mut self.holes {
            h.reverse();
        }
    }
}

impl Model {
    pub(crate) fn new_id(&mut self) -> Id {
        self.next_id += 1;
        self.next_id
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

    pub fn faces_using_edge(&self, a: Id, b: Id) -> Vec<Id> {
        self.faces.iter().filter(|(_, f)| f.uses_edge(a, b)).map(|(&id, _)| id).collect()
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
        for (&id, f) in &self.faces {
            if f.group != group {
                continue;
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
        let gid = self.new_id();
        self.groups.insert(gid, Group { name, parent: self.active_group, ..Default::default() });
        for id in nest {
            if let Some(g) = self.groups.get_mut(&id) {
                g.parent = Some(gid);
            } else if let Some(i) = self.instances.get_mut(&id) {
                i.parent = Some(gid);
            }
        }
        let mut verts = BTreeSet::new();
        for f in &faces {
            let face = self.faces.get_mut(f).unwrap();
            face.group = Some(gid);
            verts.extend(face.loops().flatten().copied());
        }
        for v in &verts {
            self.vertices.get_mut(v).unwrap().group = Some(gid);
        }
        for e in self.edges.values_mut() {
            if verts.contains(&e.a) && verts.contains(&e.b) {
                e.group = Some(gid);
            }
        }
        Ok(gid)
    }

    /// Duplicate a group, offset by `by`. Returns the new group id.
    pub fn copy_group(&mut self, gid: Id, by: DVec3, name: Option<String>) -> Result<Id> {
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
        for e in edges {
            let id = self.new_id();
            self.edges.insert(id, Edge { a: map[&e.a], b: map[&e.b], group: Some(new), soft: e.soft, hidden: e.hidden });
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
        let name = format!(
            "#{:02x}{:02x}{:02x}",
            (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[2].clamp(0.0, 1.0) * 255.0).round() as u8
        );
        self.materials.entry(name.clone()).or_insert(Material { color, texture: None });
        for f in self.faces_of(ids)? {
            self.faces.get_mut(&f).unwrap().material = Some(name.clone());
        }
        Ok(name)
    }

    /// Paint faces (or whole groups) with an existing material by name.
    pub fn paint_material(&mut self, ids: &[Id], name: &str) -> Result<()> {
        if !self.materials.contains_key(name) {
            bail!("no material {name}");
        }
        for f in self.faces_of(ids)? {
            self.faces.get_mut(&f).unwrap().material = Some(name.to_string());
        }
        Ok(())
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
