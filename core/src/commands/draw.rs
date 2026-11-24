//! Drawing: lines, rectangles, circles and arcs, push/pull, move, rotate, scale, copy and delete.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_draw(&mut self, cmd: Command) -> Result<Value> {
        let untrusted = self.untrusted;
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::New { figure } => {
                // a new model is the whole file's, even when a component is open for editing (that one goes with the old model)
                self.ctx = None;
                self.model = Model::default();
                let m = &mut self.model;
                match figure.as_deref() {
                    None | Some("" | "none") => json!({}),
                    Some(key @ ("person" | "beaver")) => {
                        let def = crate::assets::definition(m, key)?;
                        let id = m.place(def, DVec3::ZERO, 0.0, None)?;
                        json!({"figure": id})
                    }
                    Some(other) => bail!("figure is person, beaver or none (got {other:?})"),
                }
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
                let pts: Vec<DVec3> = points.into_iter().map(v3).collect();
                let r = m.draw_pieces(&pts, untrusted)?;
                json!({"edges": r.edges, "faces": r.faces, "face": r.faces.last()})
            }
            Command::Circle { center, radius, normal, segments } => {
                let f = m.circle(v3(center), v3(normal), radius, segments)?;
                let ring = m.pts(&m.face(f)?.outer.clone());
                m.curve_through(&ring, true);
                json!({"face": f})
            }
            Command::Arc { start, end, bulge, normal, segments } => {
                let pts = Model::arc_points(v3(start), v3(end), bulge, v3(normal), segments)?;
                let mut edges = vec![];
                let mut faces = vec![];
                for w in pts.windows(2) {
                    let r = m.draw_line(w[0], w[1])?;
                    edges.extend(r.edges);
                    faces.extend(r.faces);
                }
                m.curve_through(&pts, false);
                json!({"edges": edges, "faces": faces, "face": faces.last()})
            }
            Command::Spline { points, closed, segments } => {
                let pts = crate::curves::spline_points(&points.into_iter().map(v3).collect::<Vec<_>>(), closed, segments)?;
                let r = m.draw_curve_for(&pts, closed, untrusted)?;
                json!({"edges": r.edges, "faces": r.faces, "face": r.faces.last(), "curve": r.curve, "points": r.points})
            }
            Command::Bezier { points, closed, segments } => {
                let pts = crate::curves::bezier_points(&points.into_iter().map(v3).collect::<Vec<_>>(), closed, segments)?;
                let r = m.draw_curve_for(&pts, closed, untrusted)?;
                json!({"edges": r.edges, "faces": r.faces, "face": r.faces.last(), "curve": r.curve, "points": r.points})
            }
            Command::MakeFaces { ids, open_holes } => {
                let lines = m.make_faces_scope(&ids)?;
                let r = m.make_faces(&lines, &crate::makefaces::MakeFacesOptions { open_holes })?;
                let mut v = serde_json::to_value(&r)?;
                v["unchanged"] = json!(r.created == 0 && r.split == 0);
                v
            }
            Command::Flatten { ids, z } => {
                let verts = m.flatten_scope(&ids)?;
                let r = m.flatten(&verts, z)?;
                let mut v = serde_json::to_value(&r)?;
                v["unchanged"] = json!(r.changes() == 0);
                v
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
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
