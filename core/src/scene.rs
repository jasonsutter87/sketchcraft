//! What a viewer needs to draw a model: triangles (with the face each one
//! belongs to, for picking), edges, face outlines and layers. Shared by the
//! desktop app and the browser bridge.

use crate::geom;
use crate::model::Model;
use serde_json::{json, Value};

const DEFAULT_COLOR: [f32; 3] = [0.95, 0.94, 0.91];

pub fn build(m: &Model) -> Value {
    let mut positions: Vec<f32> = Vec::new();
    let mut colors: Vec<f32> = Vec::new();
    let mut tri_face: Vec<u32> = Vec::new();
    let mut faces = Vec::new();
    for (&id, f) in &m.faces {
        let outer = m.pts(&f.outer);
        let holes: Vec<_> = f.holes.iter().map(|h| m.pts(h)).collect();
        let n = geom::newell(&outer);
        let color = f.material.as_ref().and_then(|k| m.materials.get(k)).map(|k| k.color).unwrap_or(DEFAULT_COLOR);
        for t in geom::triangulate(&outer, &holes, n) {
            for p in t {
                positions.extend([p.x as f32, p.y as f32, p.z as f32]);
                colors.extend(color);
            }
            tri_face.push(id);
        }
        faces.push(json!({
            "id": id,
            "normal": n.to_array(),
            "outer": outer.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
            "group": f.group,
            "area": m.face_area(id),
            "material": f.material,
        }));
    }
    let edges: Vec<f32> = m
        .edges
        .values()
        .flat_map(|e| {
            let (a, b) = (m.p(e.a), m.p(e.b));
            [a.x as f32, a.y as f32, a.z as f32, b.x as f32, b.y as f32, b.z as f32]
        })
        .collect();
    let groups: Vec<Value> = m
        .groups
        .iter()
        .map(|(id, g)| json!({"id": id, "name": g.name, "faces": m.faces.values().filter(|f| f.group == Some(*id)).count()}))
        .collect();
    json!({
        "positions": positions,
        "colors": colors,
        "triFace": tri_face,
        "faces": faces,
        "edges": edges,
        "groups": groups,
        "loose": m.faces.values().filter(|f| f.group.is_none()).count(),
        "activeGroup": m.active_group,
    })
}
