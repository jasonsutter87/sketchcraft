//! FBX export (binary 7.4 by default, ASCII on request), clean-room from the same published layout `fbx.rs` reads.
//!
//! What goes in the file:
//! - one Model per group and per component placement, nested the way they are nested in the model, names kept; the
//!   geometry sits in world space (local transforms are identity), so a copy of a component is just its own mesh
//! - Geometry per Model: triangles (a face with holes can't be an FBX polygon), normals, and a UV layer
//!   (`UVMap`, V up) from each textured face's placement, so painted-through textures line up
//! - one Material per colour or textured material: DiffuseColor, Opacity, shininess from the finish
//! - textures as files in `<name>_textures/` beside the .fbx, referenced by relative path (Blender, Unity and
//!   Unreal all resolve that; nothing embeds an absolute path from this machine)
//! - metres, `UnitScaleFactor` 100, Y up: the model's Z-up is turned the way the glTF export turns it
//!
//! Binary is the default because Blender, Unity, Unreal and Maya read it; Blender does not read ASCII FBX.

use crate::components::WFace;
use crate::geom;
use crate::model::{Id, Model};
use anyhow::Result;
use glam::DVec3;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

/// One property value.
pub(crate) enum V {
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    /// An object's name and class: `Name\0\x01Class` in binary, `Class::Name` in ASCII.
    Obj(&'static str, String),
    ArrF(Vec<f64>),
    ArrI(Vec<i32>),
}

pub(crate) struct N {
    name: &'static str,
    props: Vec<V>,
    kids: Vec<N>,
}

fn n(name: &'static str, props: Vec<V>, kids: Vec<N>) -> N {
    N { name, props, kids }
}
fn s(x: &str) -> V {
    V::Str(x.to_string())
}
/// A Properties70 entry.
fn p(name: &str, ty: &str, label: &str, flags: &str, mut vals: Vec<V>) -> N {
    let mut props = vec![s(name), s(ty), s(label), s(flags)];
    props.append(&mut vals);
    n("P", props, vec![])
}
fn props70(items: Vec<N>) -> N {
    n("Properties70", vec![], items)
}
fn layer_element(kind: &'static str) -> N {
    n("LayerElement", vec![], vec![n("Type", vec![s(kind)], vec![]), n("TypedIndex", vec![V::I32(0)], vec![])])
}

// ---- binary ----------------------------------------------------------------------------------------------------------

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn bin_array(out: &mut Vec<u8>, tag: u8, count: usize, raw: &[u8]) {
    out.push(tag);
    put_u32(out, count as u32);
    // small arrays stay plain; bigger ones are deflated (readers all handle encoding 1)
    if raw.len() > 256 {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(raw).ok();
        if let Ok(c) = z.finish() {
            if c.len() < raw.len() {
                put_u32(out, 1);
                put_u32(out, c.len() as u32);
                out.extend_from_slice(&c);
                return;
            }
        }
    }
    put_u32(out, 0);
    put_u32(out, raw.len() as u32);
    out.extend_from_slice(raw);
}

fn bin_prop(out: &mut Vec<u8>, v: &V) {
    match v {
        V::I32(x) => {
            out.push(b'I');
            out.extend_from_slice(&x.to_le_bytes());
        }
        V::I64(x) => {
            out.push(b'L');
            out.extend_from_slice(&x.to_le_bytes());
        }
        V::F64(x) => {
            out.push(b'D');
            out.extend_from_slice(&x.to_le_bytes());
        }
        V::Bool(x) => {
            out.push(b'C');
            out.push(*x as u8);
        }
        V::Str(t) => {
            out.push(b'S');
            put_u32(out, t.len() as u32);
            out.extend_from_slice(t.as_bytes());
        }
        V::Obj(class, name) => {
            let t = format!("{name}\0\x01{class}");
            out.push(b'S');
            put_u32(out, t.len() as u32);
            out.extend_from_slice(t.as_bytes());
        }
        V::ArrF(a) => {
            let raw: Vec<u8> = a.iter().flat_map(|x| x.to_le_bytes()).collect();
            bin_array(out, b'd', a.len(), &raw);
        }
        V::ArrI(a) => {
            let raw: Vec<u8> = a.iter().flat_map(|x| x.to_le_bytes()).collect();
            bin_array(out, b'i', a.len(), &raw);
        }
    }
}

/// 7.4 node records: u32 end offset, u32 property count, u32 property bytes, u8 name length.
fn bin_node(out: &mut Vec<u8>, node: &N) {
    let at = out.len();
    put_u32(out, 0);
    put_u32(out, node.props.len() as u32);
    put_u32(out, 0);
    out.push(node.name.len() as u8);
    out.extend_from_slice(node.name.as_bytes());
    let ps = out.len();
    for v in &node.props {
        bin_prop(out, v);
    }
    let plen = (out.len() - ps) as u32;
    out[at + 8..at + 12].copy_from_slice(&plen.to_le_bytes());
    if !node.kids.is_empty() {
        for k in &node.kids {
            bin_node(out, k);
        }
        out.extend_from_slice(&[0u8; 13]);
    }
    let end = out.len() as u32;
    out[at..at + 4].copy_from_slice(&end.to_le_bytes());
}

const VERSION: u32 = 7400;

fn to_binary(top: &[N]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Kaydara FBX Binary  \0");
    out.extend_from_slice(&[0x1A, 0x00]);
    put_u32(&mut out, VERSION);
    for t in top {
        bin_node(&mut out, t);
    }
    out.extend_from_slice(&[0u8; 13]);
    // footer: the fixed id, padding to 16, the version, 120 zero bytes, the fixed magic
    out.extend_from_slice(&[0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e]);
    out.extend_from_slice(&[0u8; 4]);
    let pad = ((out.len() + 15) & !15) - out.len();
    out.resize(out.len() + if pad == 0 { 16 } else { pad }, 0);
    put_u32(&mut out, VERSION);
    out.extend_from_slice(&[0u8; 120]);
    out.extend_from_slice(&[0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b]);
    out
}

// ---- ASCII -----------------------------------------------------------------------------------------------------------

fn quote(t: &str) -> String {
    format!("\"{}\"", t.replace('"', "&quot;").replace(['\n', '\r'], " "))
}

fn ascii_node(out: &mut String, node: &N, depth: usize) {
    use std::fmt::Write as _;
    let tab = "\t".repeat(depth);
    let mut line = format!("{tab}{}:", node.name);
    let mut array: Option<String> = None;
    for (i, v) in node.props.iter().enumerate() {
        line.push_str(if i == 0 { " " } else { ", " });
        match v {
            V::I32(x) => write!(line, "{x}").unwrap(),
            V::I64(x) => write!(line, "{x}").unwrap(),
            V::F64(x) => write!(line, "{x}").unwrap(),
            V::Bool(x) => write!(line, "{}", *x as u8).unwrap(),
            V::Str(t) => line.push_str(&quote(t)),
            V::Obj(class, name) => line.push_str(&quote(&format!("{class}::{name}"))),
            V::ArrF(a) => {
                write!(line, "*{}", a.len()).unwrap();
                array = Some(a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
            }
            V::ArrI(a) => {
                write!(line, "*{}", a.len()).unwrap();
                array = Some(a.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
            }
        }
    }
    if let Some(a) = array {
        writeln!(out, "{line} {{\n{tab}\ta: {a}\n{tab}}}").unwrap();
    } else if node.kids.is_empty() {
        writeln!(out, "{line}").unwrap();
    } else {
        writeln!(out, "{line} {{").unwrap();
        for k in &node.kids {
            ascii_node(out, k, depth + 1);
        }
        writeln!(out, "{tab}}}").unwrap();
    }
}

fn to_ascii(top: &[N]) -> Vec<u8> {
    let mut out = String::from("; FBX 7.4.0 project file\n; ----------------------------------------------------\n\n");
    for t in top {
        ascii_node(&mut out, t, 0);
    }
    out.into_bytes()
}

// ---- the model -> an FBX scene ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Ungrouped,
    Group(Id),
    Inst(Id),
}

struct Mat {
    name: String,
    color: [f32; 3],
    opacity: f32,
    pbr: (f32, f32),
    /// the image file's name inside `<stem>_textures/`
    tex: Option<String>,
}

struct Part {
    name: String,
    parent: Option<usize>,
    faces: Vec<WFace>,
}

#[derive(Debug, Clone, Copy)]
pub struct FbxStats {
    pub meshes: usize,
    pub triangles: usize,
}

/// The model's Z up becomes FBX's Y up the way the glTF export does it: (x, y, z) -> (x, z, -y).
fn y_up(p: DVec3) -> [f64; 3] {
    [p.x, p.z, -p.y]
}

fn ensure(m: &Model, key: Key, parts: &mut Vec<Part>, index: &mut HashMap<Key, usize>, depth: usize) -> usize {
    if let Some(i) = index.get(&key) {
        return *i;
    }
    let (name, up) = match key {
        Key::Ungrouped => ("Ungrouped".to_string(), None),
        Key::Group(g) => {
            let grp = m.groups.get(&g);
            (grp.map(|g| g.name.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| "Group".into()), grp.and_then(|g| g.parent))
        }
        Key::Inst(i) => {
            let inst = m.instances.get(&i);
            (inst.map(|i| i.name.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| "Component".into()), inst.and_then(|i| i.parent))
        }
    };
    // a loop or an absurd depth in the model's groups stops here: the part just sits at the top
    let parent = up.filter(|g| m.groups.contains_key(g) && depth < crate::nesting::MAX_NEST).map(|g| ensure(m, Key::Group(g), parts, index, depth + 1));
    parts.push(Part { name, parent, faces: vec![] });
    index.insert(key, parts.len() - 1);
    parts.len() - 1
}

pub fn write_fbx(m: &Model, path: &str, binary: bool) -> Result<FbxStats> {
    let pth = Path::new(path);
    if let Some(dir) = pth.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let stem = pth.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
    let tex_dir = format!("{stem}_textures");

    // the tree: a part for every group / placement that holds faces, and for each one around them
    let mut parts: Vec<Part> = Vec::new();
    let mut index: HashMap<Key, usize> = HashMap::new();
    for f in m.world_faces() {
        let key = if m.instances.contains_key(&f.owner) {
            Key::Inst(f.owner)
        } else if let Some(g) = f.group.filter(|g| m.groups.contains_key(g)) {
            Key::Group(g)
        } else {
            Key::Ungrouped
        };
        let i = ensure(m, key, &mut parts, &mut index, 0);
        parts[i].faces.push(f);
    }

    // ids: a counter, so every object in the file is unique
    let mut next_id: i64 = 1_000_000;
    let mut id = || {
        next_id += 1;
        next_id
    };

    let mut mats: Vec<Mat> = Vec::new();
    let mut mat_ix: HashMap<String, usize> = HashMap::new();
    let mut tex_written: HashMap<String, String> = HashMap::new();
    let mut objects: Vec<N> = Vec::new();
    let mut conns: Vec<N> = Vec::new();
    let part_ids: Vec<i64> = parts.iter().map(|_| id()).collect();
    let mut triangles = 0usize;
    let mut meshes = 0usize;
    // (model id, material indexes in the model's own order), connected once the materials have ids
    let mut uses: Vec<(i64, Vec<usize>)> = Vec::new();

    for (pi, part) in parts.iter().enumerate() {
        let model_id = part_ids[pi];
        let class = if part.faces.is_empty() { "Null" } else { "Mesh" };
        let mut verts: Vec<f64> = Vec::new();
        let mut vindex: HashMap<[u32; 3], i32> = HashMap::new();
        let mut poly: Vec<i32> = Vec::new();
        let mut normals: Vec<f64> = Vec::new();
        let mut uvs: Vec<f64> = Vec::new();
        let mut face_mats: Vec<i32> = Vec::new();
        let mut local_mats: Vec<usize> = Vec::new();
        let mut any_uv = false;
        for f in &part.faces {
            let c = f.color.unwrap_or([0.95, 0.94, 0.91]);
            let tex = crate::texmap::of_face(m, f);
            let pbr = f.material.as_deref().and_then(|n| m.find_material(n).map(|mm| mm.pbr(n))).unwrap_or((0.85, 0.0));
            let key = match tex {
                Some((name, _)) => format!("t:{name}|{:.2},{:.2}", pbr.0, pbr.1),
                None => format!("{}|{:.2},{:.2}", crate::export::color_id(c), pbr.0, pbr.1),
            };
            let mi = *mat_ix.entry(key).or_insert_with(|| {
                let mut file = None;
                if let Some((mname, t)) = tex {
                    file = tex_written.get(mname).cloned();
                    if file.is_none() {
                        if let Ok((ext, data)) = crate::texmap::bytes(t) {
                            let name = crate::texmap::file_name(mname, ext);
                            let dir = pth.with_file_name(&tex_dir);
                            if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(dir.join(&name), data).is_ok() {
                                tex_written.insert(mname.to_string(), name.clone());
                                file = Some(name);
                            }
                        }
                    }
                }
                let name = f.material.clone().filter(|x| !x.is_empty()).unwrap_or_else(|| if f.color.is_some() { crate::export::color_id(c) } else { "Default".into() });
                let opacity = f.material.as_deref().and_then(|n| m.find_material(n)).map(|mm| mm.opacity).unwrap_or(1.0);
                mats.push(Mat { name, color: if file.is_some() { [1.0; 3] } else { c }, opacity, pbr, tex: file });
                mats.len() - 1
            });
            let local = match local_mats.iter().position(|x| *x == mi) {
                Some(l) => l,
                None => {
                    local_mats.push(mi);
                    local_mats.len() - 1
                }
            };
            let map = if mats[mi].tex.is_some() { tex.map(|(_, t)| crate::texmap::FaceMap::of(t, f)) } else { None };
            any_uv |= map.is_some();
            let nrm = y_up(f.normal);
            for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
                for (k, pt) in t.iter().enumerate() {
                    let v = y_up(*pt);
                    let bits = [(v[0] as f32).to_bits(), (v[1] as f32).to_bits(), (v[2] as f32).to_bits()];
                    let vi = *vindex.entry(bits).or_insert_with(|| {
                        verts.extend_from_slice(&v);
                        (verts.len() / 3 - 1) as i32
                    });
                    poly.push(if k == 2 { -vi - 1 } else { vi });
                    normals.extend_from_slice(&nrm);
                    match &map {
                        Some(mp) => uvs.extend_from_slice(&mp.uv(*pt)),
                        None => uvs.extend_from_slice(&[0.0, 0.0]),
                    }
                }
                face_mats.push(local as i32);
                triangles += 1;
            }
        }

        objects.push(n(
            "Model",
            vec![V::I64(model_id), V::Obj("Model", part.name.clone()), s(class)],
            vec![
                n("Version", vec![V::I32(232)], vec![]),
                props70(vec![
                    p("Lcl Translation", "Lcl Translation", "", "A", vec![V::F64(0.0), V::F64(0.0), V::F64(0.0)]),
                    p("Lcl Rotation", "Lcl Rotation", "", "A", vec![V::F64(0.0), V::F64(0.0), V::F64(0.0)]),
                    p("Lcl Scaling", "Lcl Scaling", "", "A", vec![V::F64(1.0), V::F64(1.0), V::F64(1.0)]),
                ]),
                n("Shading", vec![V::Bool(true)], vec![]),
                n("Culling", vec![s("CullingOff")], vec![]),
            ],
        ));
        conns.push(n("C", vec![s("OO"), V::I64(model_id), V::I64(part.parent.map(|q| part_ids[q]).unwrap_or(0))], vec![]));
        if part.faces.is_empty() {
            continue;
        }
        meshes += 1;
        let gid = id();
        let mut kids = vec![n("Vertices", vec![V::ArrF(verts)], vec![]), n("PolygonVertexIndex", vec![V::ArrI(poly)], vec![]), n("GeometryVersion", vec![V::I32(124)], vec![])];
        let mut layer_refs = vec![layer_element("LayerElementNormal")];
        kids.push(n("LayerElementNormal", vec![V::I32(0)], vec![
            n("Version", vec![V::I32(101)], vec![]),
            n("Name", vec![s("")], vec![]),
            n("MappingInformationType", vec![s("ByPolygonVertex")], vec![]),
            n("ReferenceInformationType", vec![s("Direct")], vec![]),
            n("Normals", vec![V::ArrF(normals)], vec![]),
        ]));
        if any_uv {
            let count = (uvs.len() / 2) as i32;
            kids.push(n("LayerElementUV", vec![V::I32(0)], vec![
                n("Version", vec![V::I32(101)], vec![]),
                n("Name", vec![s("UVMap")], vec![]),
                n("MappingInformationType", vec![s("ByPolygonVertex")], vec![]),
                n("ReferenceInformationType", vec![s("IndexToDirect")], vec![]),
                n("UV", vec![V::ArrF(uvs)], vec![]),
                n("UVIndex", vec![V::ArrI((0..count).collect())], vec![]),
            ]));
            layer_refs.push(layer_element("LayerElementUV"));
        }
        kids.push(n("LayerElementMaterial", vec![V::I32(0)], vec![
            n("Version", vec![V::I32(101)], vec![]),
            n("Name", vec![s("")], vec![]),
            n("MappingInformationType", vec![s("ByPolygon")], vec![]),
            n("ReferenceInformationType", vec![s("IndexToDirect")], vec![]),
            n("Materials", vec![V::ArrI(face_mats)], vec![]),
        ]));
        layer_refs.push(layer_element("LayerElementMaterial"));
        kids.push(n("Layer", vec![V::I32(0)], std::iter::once(n("Version", vec![V::I32(100)], vec![])).chain(layer_refs).collect()));
        objects.push(n("Geometry", vec![V::I64(gid), V::Obj("Geometry", part.name.clone()), s("Mesh")], kids));
        conns.push(n("C", vec![s("OO"), V::I64(gid), V::I64(model_id)], vec![]));
        uses.push((model_id, local_mats));
    }

    // material, texture and video objects
    let mut textures = 0usize;
    let mut mat_ids: Vec<i64> = Vec::new();
    for mat in &mats {
        let mid = id();
        mat_ids.push(mid);
        let (r, g, b) = (mat.color[0] as f64, mat.color[1] as f64, mat.color[2] as f64);
        let shin = ((1.0 - mat.pbr.0.clamp(0.0, 1.0)) as f64 * 10.0).powi(2);
        objects.push(n("Material", vec![V::I64(mid), V::Obj("Material", mat.name.clone()), s("")], vec![
            n("Version", vec![V::I32(102)], vec![]),
            n("ShadingModel", vec![s("phong")], vec![]),
            n("MultiLayer", vec![V::I32(0)], vec![]),
            props70(vec![
                p("DiffuseColor", "Color", "", "A", vec![V::F64(r), V::F64(g), V::F64(b)]),
                p("DiffuseFactor", "Number", "", "A", vec![V::F64(1.0)]),
                p("Opacity", "Number", "", "A", vec![V::F64(mat.opacity as f64)]),
                p("Shininess", "Number", "", "A", vec![V::F64(shin)]),
                p("ShininessExponent", "Number", "", "A", vec![V::F64(shin)]),
                p("ReflectionFactor", "Number", "", "A", vec![V::F64(mat.pbr.1 as f64)]),
            ]),
        ]));
        if let Some(file) = &mat.tex {
            textures += 1;
            let (tid, vid) = (id(), id());
            let rel = format!("{tex_dir}/{file}");
            objects.push(n("Video", vec![V::I64(vid), V::Obj("Video", file.clone()), s("Clip")], vec![
                n("Type", vec![s("Clip")], vec![]),
                props70(vec![p("Path", "KString", "XRefUrl", "", vec![s(&rel)])]),
                n("UseMipMap", vec![V::I32(0)], vec![]),
                n("Filename", vec![s(&rel)], vec![]),
                n("RelativeFilename", vec![s(&rel)], vec![]),
            ]));
            objects.push(n("Texture", vec![V::I64(tid), V::Obj("Texture", file.clone()), s("")], vec![
                n("Type", vec![s("TextureVideoClip")], vec![]),
                n("Version", vec![V::I32(202)], vec![]),
                n("TextureName", vec![V::Obj("Texture", file.clone())], vec![]),
                n("Media", vec![V::Obj("Video", file.clone())], vec![]),
                n("FileName", vec![s(&rel)], vec![]),
                n("RelativeFilename", vec![s(&rel)], vec![]),
                n("ModelUVTranslation", vec![V::F64(0.0), V::F64(0.0)], vec![]),
                n("ModelUVScaling", vec![V::F64(1.0), V::F64(1.0)], vec![]),
                n("Texture_Alpha_Source", vec![s("None")], vec![]),
                n("Cropping", vec![V::I32(0), V::I32(0), V::I32(0), V::I32(0)], vec![]),
            ]));
            conns.push(n("C", vec![s("OO"), V::I64(vid), V::I64(tid)], vec![]));
            conns.push(n("C", vec![s("OP"), V::I64(tid), V::I64(mid), s("DiffuseColor")], vec![]));
        }
    }
    for (model_id, list) in &uses {
        for mi in list {
            conns.push(n("C", vec![s("OO"), V::I64(mat_ids[*mi]), V::I64(*model_id)], vec![]));
        }
    }

    let total = 1 + parts.len() + meshes + mats.len() + 2 * textures;
    let mut defs = vec![
        n("Version", vec![V::I32(100)], vec![]),
        n("Count", vec![V::I32(total as i32)], vec![]),
        n("ObjectType", vec![s("GlobalSettings")], vec![n("Count", vec![V::I32(1)], vec![])]),
    ];
    for (ty, c) in [("Model", parts.len()), ("Geometry", meshes), ("Material", mats.len()), ("Texture", textures), ("Video", textures)] {
        if c > 0 {
            defs.push(n("ObjectType", vec![s(ty)], vec![n("Count", vec![V::I32(c as i32)], vec![])]));
        }
    }
    let int = |name: &str, v: i32| p(name, "int", "Integer", "", vec![V::I32(v)]);
    let top = vec![
        n("FBXHeaderExtension", vec![], vec![
            n("FBXHeaderVersion", vec![V::I32(1003)], vec![]),
            n("FBXVersion", vec![V::I32(VERSION as i32)], vec![]),
            n("Creator", vec![s("SketchCraft")], vec![]),
        ]),
        n("GlobalSettings", vec![], vec![
            n("Version", vec![V::I32(1000)], vec![]),
            props70(vec![
                int("UpAxis", 1),
                int("UpAxisSign", 1),
                int("FrontAxis", 2),
                int("FrontAxisSign", 1),
                int("CoordAxis", 0),
                int("CoordAxisSign", 1),
                int("OriginalUpAxis", 1),
                int("OriginalUpAxisSign", 1),
                // geometry is in metres: one unit is 100 cm
                p("UnitScaleFactor", "double", "Number", "", vec![V::F64(100.0)]),
                p("OriginalUnitScaleFactor", "double", "Number", "", vec![V::F64(100.0)]),
            ]),
        ]),
        n("Documents", vec![], vec![
            n("Count", vec![V::I32(1)], vec![]),
            n("Document", vec![V::I64(id()), V::Obj("Scene", "Scene".into()), s("Scene")], vec![props70(vec![]), n("RootNode", vec![V::I64(0)], vec![])]),
        ]),
        n("References", vec![], vec![]),
        n("Definitions", vec![], defs),
        n("Objects", vec![], objects),
        n("Connections", vec![], conns),
    ];
    let bytes = if binary { to_binary(&top) } else { to_ascii(&top) };
    std::fs::write(pth, bytes)?;
    Ok(FbxStats { meshes, triangles })
}

#[cfg(test)]
#[path = "fbx_export_tests.rs"]
mod tests;
