//! Building blocks and terrain: walls, stairs, roofs, columns, doors and windows, and the sandbox tools.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_build(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::RaiseWalls { group, height, max_thickness, sill, head } => {
                let r = crate::dxf::raise_walls_with(m, group, height, max_thickness, sill, head)?;
                json!({"walls": r.walls, "windows": r.windows, "layers": r.layers})
            }
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
            Command::TerrainFromContours { ids, spacing } => json!({"group": m.terrain_from_contours(&ids, spacing)?}),
            Command::TerrainGrid { at, width, depth, spacing } => json!({"group": m.terrain_grid(v3(at), width, depth, spacing)?}),
            Command::Smoove { group, at, radius, height } => json!({"moved": m.smoove(group, v3(at), radius, height)?}),
            Command::Stamp { group, face, offset } => json!({"moved": m.stamp(group, face, offset)?}),
            Command::Drape { group, ids } => json!({"lines": m.drape(group, &ids)?}),
            Command::Wall { points, thickness, height, closed } => {
                let pts: Vec<DVec3> = points.into_iter().map(v3).collect();
                json!({"group": m.wall(&pts, thickness, height, closed)?})
            }
            Command::WallPull { face, by } => {
                let p = m.wall_pull(face, by)?;
                json!({"moved": p.moves.len(), "faces": p.faces, "instances": p.instances})
            }
            Command::WallInfo { face, at } => {
                let face = match at {
                    Some(at) if m.faces.contains_key(&face) && m.normal(face).z.abs() > 0.9999 => m.wall_side_from_top(face, v3(at))?,
                    _ => face,
                };
                let p = m.wall_plan(face)?;
                let a = |v: DVec3| json!([v.x, v.y, v.z]);
                json!({
                    "face": p.a, "other": p.b, "normal": a(p.n), "thickness": p.thickness, "faces": p.faces, "instances": p.instances,
                    "strips": p.strips.iter().map(|q| q.iter().map(|v| a(*v)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                    "moves": p.moves.iter().map(|(v, k)| json!({"v": v, "at": a(m.p(*v)), "per_m": a(*k)})).collect::<Vec<_>>(),
                })
            }
            Command::RoomRect { from, to, thickness, height, name } => {
                let (g, plan, walls, floor) = m.room_rect(v3(from), v3(to), thickness, height, name)?;
                let mut out = room_plan_json(m, &plan);
                out["group"] = json!(g);
                out["name"] = json!(m.groups[&g].name);
                out["walls"] = json!(walls);
                out["floor"] = json!(floor);
                out
            }
            Command::RoomPreview { from, to, thickness } => room_plan_json(m, &m.room_plan(v3(from), v3(to), thickness)?),
            Command::Rooms => {
                let list: Vec<Value> = m
                    .rooms()
                    .iter()
                    .map(|r| json!({"id": r.id, "name": r.name, "floor": r.floor, "inside": r.inside, "z": r.z, "thickness": r.thickness, "area": r.area, "center": [(r.inside[0] + r.inside[2]) / 2.0, (r.inside[1] + r.inside[3]) / 2.0, r.z]}))
                    .collect();
                json!({"rooms": list})
            }
            Command::Stairs { at, rise, direction, width, tread } => {
                let (g, risers) = m.stairs(v3(at), direction, width, rise, tread)?;
                json!({"group": g, "risers": risers, "riser_height": rise / risers as f64})
            }
            Command::Roof { face, kind, pitch, overhang, thickness } => json!({"group": m.roof(face, &kind, pitch, overhang, thickness)?}),
            Command::Column { at, size, height, round } => json!({"group": m.column(v3(at), size, height, round)?}),
            Command::Insert { asset, at, rotate, host } => {
                let def = crate::assets::definition(m, &asset)?;
                let instance = m.place(def, v3(at), rotate, None)?;
                let opening = place_cut(m, instance, host);
                json!({"instance": instance, "component": def, "opening": opening})
            }
            Command::Assets => json!({"assets": crate::assets::ASSETS.iter().map(|a| json!({"key": a.key, "name": a.name, "about": a.about, "face_me": a.face_me})).collect::<Vec<_>>()}),
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}

fn room_plan_json(m: &crate::Model, p: &crate::room::RoomPlan) -> Value {
    let shared: Vec<Value> = p.shared.iter().map(|(id, side)| json!({"room": id, "name": m.groups.get(id).map(|g| g.name.clone()), "side": side})).collect();
    json!({"rect": p.rect, "inside": p.inside, "width": p.inside[2] - p.inside[0], "depth": p.inside[3] - p.inside[1], "area": p.area, "thickness": p.thickness, "shared": shared, "snapped": p.snapped})
}
