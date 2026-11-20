//! Texture painting commands (`texture_paint_new`, `texture_paint_stroke`, `texture_paint_status`, `texture_pixels`). The
//! maths live in `crate::texpaint`. Design notes: docs/dev/texture-paint.md.

use super::*;
use crate::texpaint::{Base, Brush, Canvas, Path};

fn ids_of(faces: Option<Vec<Id>>, entity: Option<Id>, what: &str) -> Result<Vec<Id>> {
    let mut ids = faces.unwrap_or_default();
    ids.extend(entity);
    if ids.is_empty() {
        bail!("{what} needs faces or an entity (a group)");
    }
    Ok(ids)
}

impl Engine {
    pub(super) fn run_texpaint(&mut self, cmd: Command) -> Result<Value> {
        if self.ctx.is_some() && !matches!(cmd, Command::TexturePixels { .. }) {
            bail!("close the open component first: paint textures belong to the whole model, so paint with the model open");
        }
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::TexturePaintNew { faces, entity, size, base, name, unwrap } => {
                let ids = ids_of(faces, entity, "texture_paint_new")?;
                if ids.iter().any(|i| m.instances.contains_key(i)) {
                    bail!("a placed component can't be painted from outside: open it and select its faces");
                }
                let base = match base {
                    None => Base::Color([1.0; 3]),
                    Some(Value::String(s)) if s == "material" => Base::Material,
                    Some(Value::Array(a)) => {
                        let c: Vec<f32> = a.iter().map(|v| v.as_f64().map(|x| x as f32)).collect::<Option<_>>().ok_or_else(|| anyhow::anyhow!("base is [r, g, b] numbers or \"material\""))?;
                        Base::Color(Brush::parse_color(&c)?)
                    }
                    Some(_) => bail!("base is [r, g, b] (0..1) or \"material\""),
                };
                let size = size.unwrap_or(1024);
                let (material, n, unwrapped) = m.texture_paint_new(&ids, size, &base, name.as_deref(), unwrap)?;
                json!({"material": material, "faces": n, "size": size, "unwrapped": unwrapped})
            }
            Command::TexturePaintStroke { material, faces, entity, points, radius, uv_points, uv_radius, fill, color, hardness, opacity, erase } => {
                let mut ids = faces.unwrap_or_default();
                ids.extend(entity);
                if ids.iter().any(|i| m.instances.contains_key(i)) {
                    bail!("a placed component can't be painted from outside: open it and select its faces");
                }
                let base = m.materials.get(&material).map_or([1.0; 3], |x| x.color);
                let color = match (&color, erase) {
                    (Some(c), _) => Brush::parse_color(c)?,
                    (None, true) => base,
                    (None, false) => bail!("texture_paint_stroke needs a color (or erase: true)"),
                };
                let brush = Brush { color, opacity: opacity.unwrap_or(1.0), hardness: hardness.unwrap_or(0.5), erase, base };
                let path = match (points, uv_points) {
                    (Some(_), Some(_)) => bail!("give points (3D) or uv_points (on the picture), not both"),
                    (Some(p), None) => Path::World(Some(p.into_iter().map(v3).collect()), radius.unwrap_or(0.05)),
                    (None, Some(p)) => Path::Uv(p, uv_radius.unwrap_or(0.02)),
                    (None, None) if fill => Path::World(None, 0.0),
                    (None, None) => bail!("texture_paint_stroke needs points, uv_points or fill: true"),
                };
                let sel = if ids.is_empty() { None } else { Some(ids.as_slice()) };
                let texels = m.texture_paint(&material, sel, &path, &brush)?;
                json!({"material": material, "texels": texels})
            }
            Command::TexturePaintStatus { faces, entity } => {
                let ids = ids_of(faces, entity, "texture_paint_status")?;
                let s = m.uv_status(&ids)?;
                json!({"faces": s.faces, "without": s.without, "overlap": s.overlap, "outside": s.outside, "paintable": s.ok()})
            }
            Command::TexturePixels { material, x, y, w, h } => {
                let (x, y, w, h) = (x.unwrap_or(0), y.unwrap_or(0), w.unwrap_or(1), h.unwrap_or(1));
                // in u64, so no size wraps around
                let (x, y, w, h) = (x as u64, y as u64, w as u64, h as u64);
                if w == 0 || h == 0 || w * h > 4096 {
                    bail!("read at most 4096 pixels at a time (w * h)");
                }
                let c: Canvas = self.model.texture_canvas(&material)?;
                if x + w > c.w as u64 || y + h > c.h as u64 {
                    bail!("that block is outside the {} x {} picture", c.w, c.h);
                }
                let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
                let rows: Vec<Vec<[u8; 4]>> = (y..y + h).map(|yy| (x..x + w).map(|xx| c.get(xx, yy)).collect()).collect();
                json!({"width": c.w, "height": c.h, "pixels": rows, "hash": format!("{:016x}", c.hash())})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
