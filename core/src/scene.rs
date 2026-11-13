//! What a viewer needs to draw a model: triangles (with the face or component
//! each belongs to, for picking), edges (with ids, for picking lines), face
//! outlines, the outliner tree, tags, guides and saved views. Shared by the
//! desktop app and the browser bridge.

use crate::geom;
use crate::model::Model;
use serde_json::{json, Value};

const DEFAULT_COLOR: [f32; 3] = [0.95, 0.94, 0.91];
const BACK_COLOR: [f32; 3] = [0.65, 0.68, 0.82];

/// `ctx`: a component definition being edited. Its geometry is then listed
/// in `faces` (editable), placed at the first instance's transform.
pub fn build(root: &Model, ctx: Option<u32>) -> Value {
    let mut positions: Vec<f32> = Vec::new();
    let mut colors: Vec<f32> = Vec::new();
    let mut back_colors: Vec<f32> = Vec::new();
    let mut tri_face: Vec<u32> = Vec::new();
    // image materials: texture coordinates per vertex, a texture index per triangle
    let mut uvs: Vec<f32> = Vec::new();
    let mut tri_tex: Vec<i32> = Vec::new();
    let mut textures: Vec<Value> = Vec::new();
    let mut tex_index: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    for f in root.world_faces() {
        let tex = f.material.as_ref().and_then(|name| root.find_material(name).and_then(|m| m.texture.as_ref()).map(|t| (name.clone(), t)));
        let ti = match &tex {
            Some((name, t)) => *tex_index.entry(name.clone()).or_insert_with(|| {
                let _ = t;
                textures.push(json!(name));
                textures.len() as i32 - 1
            }),
            None => -1,
        };
        // tile axes: along the face, or along the projection direction
        let (tu, tv, origin, w, h) = match &tex {
            Some((_, t)) => {
                let dir = t.project.map(|p| glam::DVec3::from(p).normalize_or_zero()).filter(|d| *d != glam::DVec3::ZERO);
                let (u, v) = geom::basis(dir.map(|d| -d).unwrap_or(f.normal));
                let o = if dir.is_some() { t.origin.map(glam::DVec3::from).unwrap_or_default() } else { glam::DVec3::ZERO };
                (u, v, o, t.width.max(1e-6), t.height.max(1e-6))
            }
            None => (glam::DVec3::X, glam::DVec3::Y, glam::DVec3::ZERO, 1.0, 1.0),
        };
        let color = f.color.unwrap_or(DEFAULT_COLOR);
        // painted faces show paint on both sides; bare backs are blue
        let back = f.color.map(|c| [c[0] * 0.85, c[1] * 0.85, c[2] * 0.85]).unwrap_or(BACK_COLOR);
        for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
            // one color per triangle (the app spreads it to the corners)
            colors.extend(color);
            back_colors.extend(back);
            for p in t {
                positions.extend([p.x as f32, p.y as f32, p.z as f32]);
                let q = p - origin;
                uvs.extend([(q.dot(tu) / w) as f32, (q.dot(tv) / h) as f32]);
            }
            tri_face.push(f.owner);
            tri_tex.push(ti);
        }
    }
    let (mut edges, mut edge_ids, mut soft_edges) = (Vec::<f32>::new(), Vec::<u32>::new(), Vec::<f32>::new());
    for e in root.world_edges() {
        let seg = [e.a.x as f32, e.a.y as f32, e.a.z as f32, e.b.x as f32, e.b.y as f32, e.b.z as f32];
        if e.soft {
            soft_edges.extend(seg);
        } else {
            edges.extend(seg);
            edge_ids.push(e.owner);
        }
    }

    // editable faces: top level, or the definition being edited (in world space)
    let (m, t) = match ctx.and_then(|d| root.definitions.get(&d).map(|def| (d, def))) {
        Some((d, def)) => {
            let t = root.instances.values().find(|i| i.def == d).map(|i| i.transform).unwrap_or_default();
            (&*def.model, t)
        }
        None => (root, glam::DMat4::IDENTITY),
    };
    let faces: Vec<Value> = m
        .faces
        .iter()
        .map(|(&id, f)| {
            let outer: Vec<_> = m.pts(&f.outer).into_iter().map(|p| t.transform_point3(p)).collect();
            json!({
                "id": id,
                "normal": geom::newell(&outer).to_array(),
                "outer": outer.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                "group": f.group,
                "area": m.face_area(id),
                "material": f.material,
                "hidden": f.hidden,
            })
        })
        .collect();
    // when editing a component, its own faces are pickable one by one
    let (mut edit_pos, mut edit_tri) = (Vec::<f32>::new(), Vec::<u32>::new());
    if ctx.is_some() {
        for (&id, f) in &m.faces {
            let tp = |l: &[u32]| -> Vec<glam::DVec3> { m.pts(l).into_iter().map(|p| t.transform_point3(p)).collect() };
            let outer = tp(&f.outer);
            let holes: Vec<_> = f.holes.iter().map(|h| tp(h)).collect();
            for tri in geom::triangulate(&outer, &holes, geom::newell(&outer)) {
                for p in tri {
                    edit_pos.extend([p.x as f32, p.y as f32, p.z as f32]);
                }
                edit_tri.push(id);
            }
        }
    }
    let edge_list: Vec<Value> = m
        .edges
        .iter()
        .map(|(&id, e)| {
            json!({"id": id, "a": t.transform_point3(m.p(e.a)).to_array(), "b": t.transform_point3(m.p(e.b)).to_array(), "soft": e.soft, "group": e.group})
        })
        .collect();
    let groups: Vec<Value> = root
        .groups
        .iter()
        .map(|(id, g)| json!({"id": id, "name": g.name, "tag": g.tag, "hidden": g.hidden, "parent": g.parent, "locked": g.locked, "faces": root.faces.values().filter(|f| f.group == Some(*id)).count()}))
        .collect();
    let instances: Vec<Value> = root
        .instances
        .iter()
        .map(|(id, i)| json!({"id": id, "name": i.name, "component": i.def, "tag": i.tag, "hidden": i.hidden, "parent": i.parent, "transform": i.transform.to_cols_array(), "attrs": i.attrs}))
        .collect();
    let components: Vec<Value> = root
        .definitions
        .iter()
        .map(|(id, d)| {
            // placed in the model, and placed inside other components (parts)
            let top = root.instances.values().filter(|i| i.def == *id).count();
            let inside: usize = root.definitions.values().map(|o| o.model.instances.values().filter(|i| i.def == *id).count()).sum();
            json!({"id": id, "name": d.name, "placed": top, "inside": inside, "source": d.source})
        })
        .collect();
    // Rounded so big imports stay a few MB of JSON: 0.01 mm positions,
    // 1/1000 colors; texture coordinates only when something is textured.
    let q = |v: &[f32], step: f64| -> Vec<f64> { v.iter().map(|x| (*x as f64 / step).round() * step).map(|x| (x * 1e6).round() / 1e6).collect() };
    let textured = tri_tex.iter().any(|t| *t >= 0);
    json!({
        "positions": q(&positions, 1e-5),
        "colors": q(&colors, 1e-3),
        "backColors": q(&back_colors, 1e-3),
        "uvs": if textured { q(&uvs, 1e-4) } else { Vec::new() },
        "triTex": tri_tex,
        "textures": textures,
        "materials": root.materials.iter().map(|(name, m)| json!({
            "name": name,
            "color": m.color,
            "texture": m.texture.as_ref().map(|t| json!({"data": t.data, "width": t.width, "height": t.height, "project": t.project})),
            "opacity": m.opacity,
        })).collect::<Vec<_>>(),
        "triFace": tri_face,
        "faces": faces,
        "edgeList": edge_list,
        "edges": q(&edges, 1e-5),
        "edgeIds": edge_ids,
        "softEdges": soft_edges,
        "groups": groups,
        "instances": instances,
        "components": components,
        "loose": root.faces.values().filter(|f| f.group.is_none()).count(),
        "activeGroup": root.active_group,
        "editing": ctx,
        "editPositions": edit_pos,
        "editTriFace": edit_tri,
        "editTransform": t.to_cols_array(),
        "tags": root.tags,
        "tagColors": root.tag_colors,
        "guides": root.guides.iter().map(|g| json!({"id": g.id, "p": g.p.to_array(), "dir": g.dir.map(|d| d.to_array())})).collect::<Vec<_>>(),
        "views": root.views,
        "classes": root.classes,
        "location": root.location,
        "environment": root.environment,
        "photoPoints": root.photo_points,
        "cages": root.cages.iter().filter(|(g, _)| root.groups.contains_key(g)).map(|(g, c)| (g.to_string(), c)).collect::<std::collections::BTreeMap<_, _>>(),
        "photoEdges": root.photo_edges,
        "annotations": root.annotations,
        "units": root.units,
    })
}
