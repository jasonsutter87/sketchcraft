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
    for f in root.world_faces() {
        let color = f.color.unwrap_or(DEFAULT_COLOR);
        // painted faces show paint on both sides; bare backs are blue
        let back = f.color.map(|c| [c[0] * 0.85, c[1] * 0.85, c[2] * 0.85]).unwrap_or(BACK_COLOR);
        for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
            for p in t {
                positions.extend([p.x as f32, p.y as f32, p.z as f32]);
                colors.extend(color);
                back_colors.extend(back);
            }
            tri_face.push(f.owner);
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
            (&def.model, t)
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
        .map(|(id, g)| json!({"id": id, "name": g.name, "tag": g.tag, "hidden": g.hidden, "faces": root.faces.values().filter(|f| f.group == Some(*id)).count()}))
        .collect();
    let instances: Vec<Value> = root
        .instances
        .iter()
        .map(|(id, i)| json!({"id": id, "name": i.name, "component": i.def, "tag": i.tag, "hidden": i.hidden, "transform": i.transform.to_cols_array()}))
        .collect();
    let components: Vec<Value> = root
        .definitions
        .iter()
        .map(|(id, d)| json!({"id": id, "name": d.name, "placed": root.instances.values().filter(|i| i.def == *id).count()}))
        .collect();
    json!({
        "positions": positions,
        "colors": colors,
        "backColors": back_colors,
        "triFace": tri_face,
        "faces": faces,
        "edgeList": edge_list,
        "edges": edges,
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
        "guides": root.guides.iter().map(|g| json!({"id": g.id, "p": g.p.to_array(), "dir": g.dir.map(|d| d.to_array())})).collect::<Vec<_>>(),
        "views": root.views,
        "annotations": root.annotations,
    })
}
