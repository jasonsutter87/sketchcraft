//! Components (define once, place many), visibility (hide, tags) and the
//! flattened "what's actually visible" view every renderer and exporter uses.

use crate::geom;
use crate::model::{Definition, Face, Group, Id, Instance, Model};
use anyhow::{anyhow, bail, Result};
use glam::{DMat4, DVec3};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A visible face in world space. `owner` is the face id for loose/grouped
/// geometry, or the top-level component instance it came from.
pub struct WFace {
    pub owner: Id,
    /// Top-level group of the face (None inside components).
    pub group: Option<Id>,
    pub face: Option<Id>,
    pub outer: Vec<DVec3>,
    pub holes: Vec<Vec<DVec3>>,
    pub normal: DVec3,
    pub color: Option<[f32; 3]>,
    /// material name (look it up in the model, or the component that owns it)
    pub material: Option<String>,
}

pub struct WEdge {
    pub owner: Id,
    pub a: DVec3,
    pub b: DVec3,
    pub soft: bool,
}

impl Model {
    pub fn tag_visible(&self, tag: &Option<String>) -> bool {
        tag.as_ref().map_or(true, |t| self.tags.get(t).copied().unwrap_or(true))
    }

    fn group_visible(&self, g: Option<Id>) -> bool {
        // hidden or tag-hidden anywhere up the chain hides it
        self.ancestors(g).iter().all(|g| self.groups.get(g).map_or(true, |g| !g.hidden && self.tag_visible(&g.tag)))
    }

    pub fn world_faces(&self) -> Vec<WFace> {
        let mut out = Vec::new();
        self.collect_faces(self, DMat4::IDENTITY, None, None, &Default::default(), &mut out);
        out
    }

    /// A component's own size (its definition's bounds), for Dynamic
    /// Component lengths.
    fn def_size(root: &Model, def: &crate::model::Definition) -> DVec3 {
        let mut fs = Vec::new();
        def.model.collect_faces(root, DMat4::IDENTITY, Some(0), None, &Default::default(), &mut fs);
        let mut it = fs.iter().flat_map(|f| f.outer.iter());
        let Some(first) = it.next() else { return DVec3::ZERO };
        let (lo, hi) = it.fold((*first, *first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        hi - lo
    }

    /// How a placement draws (copies, stretch, fill, hidden), Dynamic
    /// Component attributes worked out.
    fn placement(root: &Model, inst: &crate::model::Instance, def: &crate::model::Definition, scope: &BTreeMap<String, f64>) -> crate::dynamic::Draw {
        let has = !inst.attrs.is_empty() || !def.attrs.is_empty();
        let size = if has { Self::def_size(root, def) } else { DVec3::ZERO };
        crate::dynamic::draw(root, inst, Some(def), size, scope)
    }

    /// `root` holds the definitions: components inside components are looked
    /// up there, not in the definition's own (empty) table.
    fn collect_faces(&self, root: &Model, t: DMat4, owner: Option<Id>, fill: Option<[f32; 3]>, scope: &BTreeMap<String, f64>, out: &mut Vec<WFace>) {
        for (&id, f) in &self.faces {
            if f.hidden || !self.group_visible(f.group) {
                continue;
            }
            let tp = |l: &[Id]| -> Vec<DVec3> { l.iter().map(|v| t.transform_point3(self.p(*v))).collect() };
            let outer = tp(&f.outer);
            let normal = geom::newell(&outer);
            out.push(WFace {
                owner: owner.unwrap_or(id),
                group: if owner.is_none() { f.group } else { None },
                face: owner.is_none().then_some(id),
                holes: f.holes.iter().map(|h| tp(h)).collect(),
                outer,
                normal,
                color: f.material.as_ref().and_then(|m| self.materials.get(m)).map(|m| m.color).or(fill),
                material: f.material.clone(),
            });
        }
        for (&iid, inst) in &self.instances {
            if inst.hidden || !self.tag_visible(&inst.tag) || !self.group_visible(inst.parent) {
                continue;
            }
            if let Some(def) = root.definitions.get(&inst.def).or_else(|| self.definitions.get(&inst.def)) {
                let d = Self::placement(root, inst, def, scope);
                if d.hidden {
                    continue;
                }
                for p in &d.placements {
                    def.model.collect_faces(root, t * *p, Some(owner.unwrap_or(iid)), d.color.or(fill), &d.scope, out);
                }
            }
        }
    }

    pub fn world_edges(&self) -> Vec<WEdge> {
        let mut out = Vec::new();
        self.collect_edges(self, DMat4::IDENTITY, None, &Default::default(), &mut out);
        out
    }

    fn collect_edges(&self, root: &Model, t: DMat4, owner: Option<Id>, scope: &BTreeMap<String, f64>, out: &mut Vec<WEdge>) {
        // an edge shows if it isn't hidden and something visible uses it (or it's a free line)
        let mut used: HashMap<(Id, Id), bool> = HashMap::new();
        for f in self.faces.values() {
            let vis = !f.hidden && self.group_visible(f.group);
            for l in f.loops() {
                for (a, b) in crate::model::pairs(l) {
                    let k = (a.min(b), a.max(b));
                    let e = used.entry(k).or_insert(false);
                    *e |= vis;
                }
            }
        }
        for (&id, e) in &self.edges {
            let k = (e.a.min(e.b), e.a.max(e.b));
            let visible = used.get(&k).copied().unwrap_or(self.group_visible(e.group));
            if e.hidden || !visible {
                continue;
            }
            out.push(WEdge {
                owner: owner.unwrap_or(id),
                a: t.transform_point3(self.p(e.a)),
                b: t.transform_point3(self.p(e.b)),
                soft: e.soft,
            });
        }
        for (&iid, inst) in &self.instances {
            if inst.hidden || !self.tag_visible(&inst.tag) || !self.group_visible(inst.parent) {
                continue;
            }
            if let Some(def) = root.definitions.get(&inst.def).or_else(|| self.definitions.get(&inst.def)) {
                let d = Self::placement(root, inst, def, scope);
                if d.hidden {
                    continue;
                }
                for p in &d.placements {
                    def.model.collect_edges(root, t * *p, Some(owner.unwrap_or(iid)), &d.scope, out);
                }
            }
        }
    }

    /// Bounds of everything, components included.
    /// A material by name, here or in any component definition.
    pub fn find_material(&self, name: &str) -> Option<&crate::model::Material> {
        self.materials.get(name).or_else(|| self.definitions.values().find_map(|d| d.model.find_material(name)))
    }

    pub fn world_bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.world_faces().into_iter().flat_map(|f| f.outer).chain(self.world_edges().into_iter().flat_map(|e| [e.a, e.b]));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    // ----- components -------------------------------------------------------

    /// Turn faces (or a group) into a component and put one instance where
    /// they were. The component's origin is its bounding-box min corner.
    pub fn make_component(&mut self, ids: &[Id], name: String) -> Result<Id> {
        let faces = self.faces_of(ids)?;
        if faces.is_empty() {
            bail!("nothing to make a component from");
        }
        let verts: BTreeSet<Id> = faces.iter().flat_map(|f| self.faces[f].loops().flatten().copied().collect::<Vec<_>>()).collect();
        let lo = verts.iter().map(|v| self.p(*v)).fold(DVec3::splat(f64::MAX), |a, p| a.min(p));
        let mut def = Model::default();
        let mut map: HashMap<Id, Id> = HashMap::new();
        for v in &verts {
            map.insert(*v, def.vertex_in(self.p(*v) - lo, None));
        }
        for f in &faces {
            let face = &self.faces[f];
            let outer = face.outer.iter().map(|v| map[v]).collect();
            let holes = face.holes.iter().map(|h| h.iter().map(|v| map[v]).collect()).collect();
            if let Some(m) = &face.material {
                def.materials.insert(m.clone(), self.materials[m].clone());
            }
            def.insert_face(Face { outer, holes, material: face.material.clone(), ..Default::default() });
        }
        // carry soft edges over
        for e in self.edges.values() {
            if e.soft {
                if let (Some(&a), Some(&b)) = (map.get(&e.a), map.get(&e.b)) {
                    if let Some(id) = def.find_edge(a, b) {
                        def.edges.get_mut(&id).unwrap().soft = true;
                    }
                }
            }
        }
        // remove the originals (and the group, if a whole group was converted)
        for f in &faces {
            self.faces.remove(f);
        }
        for id in ids {
            if self.groups.contains_key(id) {
                self.delete(&[*id])?;
            }
        }
        self.edges.retain(|_, e| !(verts.contains(&e.a) && verts.contains(&e.b)) || self.faces.values().any(|f| f.uses_edge(e.a, e.b)));
        self.gc();
        let did = self.new_id();
        self.definitions.insert(did, Definition { name: name.clone(), model: std::sync::Arc::new(def), attrs: Default::default(), source: None });
        self.place(did, lo, 0.0, None)
    }

    pub fn find_definition(&self, key: &str) -> Option<Id> {
        key.parse::<Id>().ok().filter(|id| self.definitions.contains_key(id)).or_else(|| {
            self.definitions.iter().find(|(_, d)| d.name.eq_ignore_ascii_case(key)).map(|(&k, _)| k)
        })
    }

    pub fn place(&mut self, def: Id, at: DVec3, rotate_deg: f64, name: Option<String>) -> Result<Id> {
        let d = self.definitions.get(&def).ok_or_else(|| anyhow!("no component {def}"))?;
        let name = name.unwrap_or_else(|| d.name.clone());
        let t = DMat4::from_translation(at) * DMat4::from_rotation_z(rotate_deg.to_radians());
        let id = self.new_id();
        self.instances.insert(id, Instance { parent: self.active_group, def, name, transform: t, tag: None, hidden: false, attrs: Default::default() });
        Ok(id)
    }

    /// Replace an instance with plain geometry (in a new group).
    pub fn explode(&mut self, inst: Id) -> Result<Id> {
        let i = self.instances.remove(&inst).ok_or_else(|| anyhow!("{inst} is not a component"))?;
        let def = self.definitions[&i.def].model.clone();
        let gid = self.new_id();
        self.groups.insert(gid, Group { name: i.name.clone(), ..Default::default() });
        let mut map: HashMap<Id, Id> = HashMap::new();
        for (&k, v) in &def.vertices {
            let id = self.new_id();
            self.vertices.insert(id, crate::model::Vertex { p: i.transform.transform_point3(v.p), group: Some(gid) });
            map.insert(k, id);
        }
        for e in def.edges.values() {
            let id = self.edge(map[&e.a], map[&e.b]);
            self.edges.get_mut(&id).unwrap().soft = e.soft;
        }
        let mirror = i.transform.determinant() < 0.0;
        for f in def.faces.values() {
            let mut nf = Face {
                outer: f.outer.iter().map(|v| map[v]).collect(),
                holes: f.holes.iter().map(|h| h.iter().map(|v| map[v]).collect()).collect(),
                material: f.material.clone(),
                group: Some(gid),
                hidden: false,
            };
            if mirror {
                nf.reverse();
            }
            if let Some(m) = &f.material {
                self.materials.entry(m.clone()).or_insert_with(|| def.materials[m].clone());
            }
            self.insert_face(nf);
        }
        Ok(gid)
    }

    // ----- visibility -------------------------------------------------------

    pub fn set_hidden(&mut self, ids: &[Id], hidden: bool) -> Result<()> {
        for id in ids {
            if let Some(f) = self.faces.get_mut(id) {
                f.hidden = hidden;
            } else if let Some(e) = self.edges.get_mut(id) {
                e.hidden = hidden;
            } else if let Some(g) = self.groups.get_mut(id) {
                g.hidden = hidden;
            } else if let Some(i) = self.instances.get_mut(id) {
                i.hidden = hidden;
            } else {
                bail!("no entity {id}");
            }
        }
        Ok(())
    }

    pub fn unhide_all(&mut self) {
        self.faces.values_mut().for_each(|f| f.hidden = false);
        self.edges.values_mut().for_each(|e| e.hidden = false);
        self.groups.values_mut().for_each(|g| g.hidden = false);
        self.instances.values_mut().for_each(|i| i.hidden = false);
    }

    pub fn set_tag(&mut self, ids: &[Id], tag: Option<String>) -> Result<()> {
        if let Some(t) = &tag {
            self.tags.entry(t.clone()).or_insert(true);
        }
        for id in ids {
            if let Some(g) = self.groups.get_mut(id) {
                g.tag = tag.clone();
            } else if let Some(i) = self.instances.get_mut(id) {
                i.tag = tag.clone();
            } else {
                bail!("tags go on groups and components ({id} is neither)");
            }
        }
        Ok(())
    }

    pub fn soften(&mut self, ids: &[Id], soft: bool) -> Result<usize> {
        let mut n = 0;
        for id in ids {
            if let Some(e) = self.edges.get_mut(id) {
                e.soft = soft;
                n += 1;
            } else if let Some(f) = self.faces.get(id).cloned() {
                // a face: soften its edges shared with coplanar neighbours
                for l in f.loops() {
                    for (a, b) in crate::model::pairs(l) {
                        if let Some(e) = self.find_edge(a, b) {
                            self.edges.get_mut(&e).unwrap().soft = soft;
                            n += 1;
                        }
                    }
                }
            } else {
                bail!("{id} is not an edge or face");
            }
        }
        Ok(n)
    }

    /// Soften every edge that lies between two coplanar faces (cleans up
    /// imported streets and split walls).
    pub fn soften_coplanar(&mut self) -> usize {
        let keys: Vec<(Id, Id, Id)> = self.edges.iter().map(|(&k, e)| (k, e.a, e.b)).collect();
        let mut n = 0;
        for (k, a, b) in keys {
            let fs = self.faces_using_edge(a, b);
            if fs.len() == 2 && self.normal(fs[0]).dot(self.normal(fs[1])) > 1.0 - 1e-6 {
                self.edges.get_mut(&k).unwrap().soft = true;
                n += 1;
            }
        }
        n
    }
}
