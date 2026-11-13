//! Binary FBX import (versions 7.x), clean-room from the published layout:
//!
//! - header: "Kaydara FBX Binary  \0", 0x1A 0x00, u32 version
//! - node: end offset, property count, property bytes (u32 each, u64 from
//!   7.5), u8 name length, name, properties, child nodes, then a null record
//! - properties: Y i16, C bool, I i32, F f32, D f64, L i64, S/R length +
//!   bytes, and arrays f d l i b: count, encoding (1 = zlib), byte length
//!
//! What's used: Objects/Geometry (Vertices, PolygonVertexIndex with the last
//! index of each polygon stored as -(i+1), LayerElementMaterial),
//! Objects/Model (Lcl Translation / Rotation / Scaling, PreRotation),
//! Objects/Material (DiffuseColor), Connections (child -> parent), and
//! GlobalSettings (UpAxis, UnitScaleFactor in cm).
//!
//! The file becomes one component; each mesh model is a group inside it.

use crate::mesh::Mesh;
use crate::model::{Definition, Id, Instance, Material, Model};
use anyhow::{anyhow, bail, Context, Result};
use glam::{DMat4, DVec3, EulerRot};
use std::collections::HashMap;
use std::io::Read;

#[derive(Debug, Clone)]
enum Prop {
    I(i64),
    F(f64),
    S(String),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Other,
}

impl Prop {
    fn int(&self) -> Option<i64> {
        match self {
            Prop::I(v) => Some(*v),
            Prop::F(v) => Some(*v as i64),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Prop::I(v) => Some(*v as f64),
            Prop::F(v) => Some(*v),
            _ => None,
        }
    }
    fn str(&self) -> &str {
        match self {
            Prop::S(s) => s,
            _ => "",
        }
    }
}

#[derive(Debug, Default)]
struct Node {
    name: String,
    props: Vec<Prop>,
    kids: Vec<Node>,
}

impl Node {
    fn get(&self, name: &str) -> Option<&Node> {
        self.kids.iter().find(|k| k.name == name)
    }
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.kids.iter().filter(move |k| k.name == name)
    }
    /// Properties70 "P" entries by name: their values after the 4 header strings.
    fn p70(&self) -> HashMap<String, Vec<Prop>> {
        let mut out = HashMap::new();
        if let Some(ps) = self.get("Properties70") {
            for p in ps.all("P") {
                if let Some(Prop::S(n)) = p.props.first() {
                    out.insert(n.clone(), p.props.iter().skip(4).cloned().collect());
                }
            }
        }
        out
    }
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
    wide: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n).ok_or_else(|| anyhow!("FBX file ends early"))?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn len(&mut self) -> Result<u64> {
        Ok(if self.wide { u64::from_le_bytes(self.take(8)?.try_into()?) } else { self.u32()? as u64 })
    }

    fn array(&mut self, size: usize) -> Result<Vec<u8>> {
        let count = self.u32()? as usize;
        let enc = self.u32()?;
        let bytes = self.u32()? as usize;
        let raw = self.take(bytes)?;
        let want = count * size;
        if want > 512 << 20 {
            bail!("FBX array too large");
        }
        if enc == 1 {
            let mut out = Vec::with_capacity(want);
            flate2::read::ZlibDecoder::new(raw).read_to_end(&mut out)?;
            Ok(out)
        } else {
            Ok(raw.to_vec())
        }
    }

    fn prop(&mut self) -> Result<Prop> {
        let t = self.u8()?;
        Ok(match t {
            b'Y' => Prop::I(i16::from_le_bytes(self.take(2)?.try_into()?) as i64),
            b'C' => Prop::I(self.u8()? as i64),
            b'I' => Prop::I(i32::from_le_bytes(self.take(4)?.try_into()?) as i64),
            b'L' => Prop::I(i64::from_le_bytes(self.take(8)?.try_into()?)),
            b'F' => Prop::F(f32::from_le_bytes(self.take(4)?.try_into()?) as f64),
            b'D' => Prop::F(f64::from_le_bytes(self.take(8)?.try_into()?)),
            b'S' => {
                let n = self.u32()? as usize;
                Prop::S(String::from_utf8_lossy(self.take(n)?).to_string())
            }
            b'R' => {
                let n = self.u32()? as usize;
                self.take(n)?;
                Prop::Other
            }
            b'f' => Prop::Floats(self.array(4)?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap()) as f64).collect()),
            b'd' => Prop::Floats(self.array(8)?.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()),
            b'i' => Prop::Ints(self.array(4)?.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap()) as i64).collect()),
            b'l' => Prop::Ints(self.array(8)?.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect()),
            b'b' => Prop::Ints(self.array(1)?.iter().map(|b| *b as i64).collect()),
            _ => bail!("unknown FBX property type {:?} at {}", t as char, self.p - 1),
        })
    }

    /// One node, or None at a null record.
    fn node(&mut self, depth: usize) -> Result<Option<Node>> {
        if depth > 64 {
            bail!("FBX nesting too deep");
        }
        let end = self.len()? as usize;
        let nprops = self.len()?;
        let _plen = self.len()?;
        let nlen = self.u8()? as usize;
        if end == 0 {
            return Ok(None);
        }
        if end > self.d.len() {
            bail!("bad FBX node length");
        }
        let name = String::from_utf8_lossy(self.take(nlen)?).to_string();
        let mut n = Node { name, ..Default::default() };
        for _ in 0..nprops {
            n.props.push(self.prop()?);
        }
        while self.p < end {
            match self.node(depth + 1)? {
                Some(k) => n.kids.push(k),
                None => break,
            }
        }
        self.p = end;
        Ok(Some(n))
    }
}

fn parse(d: &[u8]) -> Result<Node> {
    if d.len() < 27 || &d[..20] != b"Kaydara FBX Binary  " {
        if d.starts_with(b"; FBX") {
            bail!("this is a text (ASCII) FBX; export it as binary FBX");
        }
        bail!("not a binary FBX file");
    }
    let version = u32::from_le_bytes(d[23..27].try_into()?);
    let mut r = Reader { d, p: 27, wide: version >= 7500 };
    let mut root = Node { name: "root".into(), ..Default::default() };
    while r.p + 13 < d.len() {
        match r.node(0)? {
            Some(n) => root.kids.push(n),
            None => break,
        }
    }
    Ok(root)
}

/// The id and display name of an object node ("Name\0\x01Class" -> "Name").
fn ident(n: &Node) -> (i64, String) {
    let id = n.props.first().and_then(|p| p.int()).unwrap_or(0);
    let name = n.props.get(1).map(|p| p.str().split('\0').next().unwrap_or("").to_string()).unwrap_or_default();
    (id, name)
}

fn vec3(p: Option<&Vec<Prop>>, default: DVec3) -> DVec3 {
    match p {
        Some(v) if v.len() >= 3 => DVec3::new(v[0].num().unwrap_or(0.0), v[1].num().unwrap_or(0.0), v[2].num().unwrap_or(0.0)),
        _ => default,
    }
}

pub struct FbxResult {
    pub instance: Id,
    pub component: Id,
    pub meshes: usize,
    pub faces: usize,
}

pub fn import(m: &mut Model, d: &[u8], name: &str, at: DVec3) -> Result<FbxResult> {
    let root = parse(d)?;
    let objects = root.get("Objects").context("the FBX has no Objects")?;

    // units and axes
    let gs = root.get("GlobalSettings").map(|g| g.p70()).unwrap_or_default();
    let unit_cm = gs.get("UnitScaleFactor").and_then(|v| v.first()?.num()).unwrap_or(1.0);
    let up = gs.get("UpAxis").and_then(|v| v.first()?.int()).unwrap_or(1);
    let to_m = unit_cm * 0.01;
    // FBX up axis -> SketchCraft's Z up
    let axes = if up == 1 { DMat4::from_cols_array(&[1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.]) } else { DMat4::IDENTITY };
    let world = DMat4::from_scale(DVec3::splat(to_m)) * axes;

    // objects by id
    let mut geoms: HashMap<i64, &Node> = HashMap::new();
    let mut models: HashMap<i64, (&Node, String)> = HashMap::new();
    let mut mats: HashMap<i64, [f32; 3]> = HashMap::new();
    for n in &objects.kids {
        let (id, nm) = ident(n);
        match n.name.as_str() {
            "Geometry" if n.props.get(2).map(|p| p.str()) == Some("Mesh") => {
                geoms.insert(id, n);
            }
            "Model" => {
                models.insert(id, (n, nm));
            }
            "Material" => {
                let p = n.p70();
                let c = vec3(p.get("DiffuseColor").or_else(|| p.get("Diffuse")), DVec3::splat(0.8));
                mats.insert(id, [c.x as f32, c.y as f32, c.z as f32]);
            }
            _ => {}
        }
    }
    // connections: child -> parent (object-object only)
    let mut parent: HashMap<i64, i64> = HashMap::new();
    let mut geom_of: HashMap<i64, i64> = HashMap::new(); // model -> geometry
    let mut mats_of: HashMap<i64, Vec<i64>> = HashMap::new(); // model -> materials in order
    if let Some(cs) = root.get("Connections") {
        for c in cs.all("C") {
            if c.props.first().map(|p| p.str()) != Some("OO") {
                continue;
            }
            let (child, par) = (c.props.get(1).and_then(|p| p.int()).unwrap_or(0), c.props.get(2).and_then(|p| p.int()).unwrap_or(0));
            if geoms.contains_key(&child) && models.contains_key(&par) {
                geom_of.insert(par, child);
            } else if mats.contains_key(&child) && models.contains_key(&par) {
                mats_of.entry(par).or_default().push(child);
            } else if models.contains_key(&child) {
                parent.insert(child, par);
            }
        }
    }
    let local = |n: &Node| -> DMat4 {
        let p = n.p70();
        let t = vec3(p.get("Lcl Translation"), DVec3::ZERO);
        let r = vec3(p.get("Lcl Rotation"), DVec3::ZERO);
        let pre = vec3(p.get("PreRotation"), DVec3::ZERO);
        let s = vec3(p.get("Lcl Scaling"), DVec3::ONE);
        let rot = |e: DVec3| DMat4::from_euler(EulerRot::ZYX, e.z.to_radians(), e.y.to_radians(), e.x.to_radians());
        DMat4::from_translation(t) * rot(pre) * rot(r) * DMat4::from_scale(s)
    };
    let global = |mut id: i64| -> DMat4 {
        let mut t = DMat4::IDENTITY;
        for _ in 0..64 {
            let Some((n, _)) = models.get(&id) else { break };
            t = local(n) * t;
            match parent.get(&id) {
                Some(p) if *p != 0 => id = *p,
                _ => break,
            }
        }
        t
    };

    let mut def = Model::default();
    let mut faces = 0;
    let mut meshes = 0;
    let mut ids: Vec<&i64> = geom_of.keys().collect();
    ids.sort();
    for mid in ids {
        let g = geoms[&geom_of[mid]];
        let (_, mname) = &models[mid];
        let verts = match g.get("Vertices").and_then(|v| v.props.first()) {
            Some(Prop::Floats(v)) => v.clone(),
            _ => continue,
        };
        let index = match g.get("PolygonVertexIndex").and_then(|v| v.props.first()) {
            Some(Prop::Ints(v)) => v.clone(),
            _ => continue,
        };
        let t = world * global(*mid);
        let pts: Vec<DVec3> = verts.chunks_exact(3).map(|c| t.transform_point3(DVec3::new(c[0], c[1], c[2]))).collect();
        // per-polygon material, or one for the whole mesh
        let lem = g.get("LayerElementMaterial");
        let by_poly = lem.and_then(|l| l.get("MappingInformationType")).and_then(|p| p.props.first()).map(|p| p.str() == "ByPolygon").unwrap_or(false);
        let poly_mats: Vec<i64> = match lem.and_then(|l| l.get("Materials")).and_then(|p| p.props.first()) {
            Some(Prop::Ints(v)) => v.clone(),
            _ => vec![],
        };
        let model_mats = mats_of.get(mid).cloned().unwrap_or_default();
        let mat_key = |k: usize| -> Option<String> {
            let mi = if by_poly { *poly_mats.get(k)? } else { *poly_mats.first().unwrap_or(&0) };
            let id = *model_mats.get(mi.max(0) as usize)?;
            mats.get(&id).map(|_| format!("fbx:{id}"))
        };
        let gid = def.begin_group(if mname.is_empty() { "Mesh".into() } else { mname.clone() });
        let mut mesh = Mesh::new(&mut def, Some(gid));
        let mut poly: Vec<DVec3> = Vec::new();
        let mut k = 0;
        for &i in &index {
            let (v, last) = if i < 0 { ((-i - 1) as usize, true) } else { (i as usize, false) };
            if let Some(p) = pts.get(v) {
                poly.push(*p);
            }
            if last {
                if poly.len() >= 3 {
                    mesh.face(&poly, &[], mat_key(k));
                }
                poly.clear();
                k += 1;
            }
        }
        let (f, _) = mesh.finish();
        def.active_group = None;
        faces += f;
        meshes += 1;
    }
    if meshes == 0 {
        bail!("no meshes in this FBX");
    }
    for (id, c) in &mats {
        let mat = Material { color: *c, texture: None };
        def.materials.insert(format!("fbx:{id}"), mat.clone());
        m.materials.entry(format!("fbx:{id}")).or_insert(mat);
    }
    let component = m.new_id();
    m.definitions.insert(component, Definition { name: name.to_string(), model: std::sync::Arc::new(def), attrs: Default::default() });
    let instance = m.new_id();
    m.instances.insert(instance, Instance { def: component, name: name.to_string(), transform: DMat4::from_translation(at), tag: None, hidden: false, attrs: Default::default() });
    Ok(FbxResult { instance, component, meshes, faces })
}
