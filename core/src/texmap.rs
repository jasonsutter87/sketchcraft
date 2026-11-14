//! Textures outside the viewport: where a face's image lands (UVs, the same
//! rule the viewport uses), the image bytes behind a material's data URL, and
//! sampling them for headless renders. Exporters write the images next to
//! the model (OBJ, Collada) or inside it (glTF).

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use glam::DVec3;
use image::RgbImage;

use crate::components::WFace;
use crate::geom;
use crate::model::{Model, Texture};

/// How a texture lies on a face: uv = ((p - origin)·u / width, (p - origin)·v / height).
#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    pub u: DVec3,
    pub v: DVec3,
    pub origin: DVec3,
    pub width: f64,
    pub height: f64,
}

impl Mapping {
    pub fn of(t: &Texture, normal: DVec3) -> Mapping {
        // tile axes: along the face, or along the projection direction
        let dir = t.project.map(|p| DVec3::from(p).normalize_or_zero()).filter(|d| *d != DVec3::ZERO);
        let (u, v) = geom::basis(dir.map(|d| -d).unwrap_or(normal));
        let origin = if dir.is_some() { t.origin.map(DVec3::from).unwrap_or_default() } else { DVec3::ZERO };
        Mapping { u, v, origin, width: t.width.max(1e-6), height: t.height.max(1e-6) }
    }

    /// (u, v) with v up the image (the viewport's convention)
    pub fn uv(&self, p: DVec3) -> [f64; 2] {
        let q = p - self.origin;
        [q.dot(self.u) / self.width, q.dot(self.v) / self.height]
    }
}

/// A face's texture (the material may live in a component's definition).
pub fn of_face<'a>(m: &'a Model, f: &WFace) -> Option<(&'a str, &'a Texture)> {
    let name = f.material.as_deref()?;
    let mat = m.find_material(name)?;
    let (key, _) = m.materials.get_key_value(name).or_else(|| m.definitions.values().find_map(|d| d.model.materials.get_key_value(name)))?;
    Some((key.as_str(), mat.texture.as_ref()?))
}

/// The image file behind a data URL: (extension, bytes).
pub fn bytes(t: &Texture) -> Result<(&'static str, Vec<u8>)> {
    use base64::Engine as _;
    let Some((head, b64)) = t.data.split_once(',') else { bail!("texture isn't a data URL") };
    if !head.contains(";base64") {
        bail!("texture data isn't base64");
    }
    let ext = if head.contains("jpeg") || head.contains("jpg") { "jpg" } else { "png" };
    let data = base64::engine::general_purpose::STANDARD.decode(b64.trim()).context("texture data isn't valid base64")?;
    Ok((ext, data))
}

/// A file-safe name for a material's image.
pub fn file_name(material: &str, ext: &str) -> String {
    let mut s: String = material.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    s.truncate(60);
    if s.is_empty() {
        s = "texture".into();
    }
    format!("{s}.{ext}")
}

/// Decoded images for headless renders, by material name. Images are capped
/// at 2048 px so a huge photo can't blow up memory.
pub struct Images(HashMap<String, RgbImage>);

impl Images {
    pub fn load(m: &Model) -> Images {
        let mut out = HashMap::new();
        let all = m.materials.iter().chain(m.definitions.values().flat_map(|d| d.model.materials.iter()));
        for (name, mat) in all {
            let Some(t) = &mat.texture else { continue };
            if out.contains_key(name) {
                continue;
            }
            let Ok((_, data)) = bytes(t) else { continue };
            let Ok(img) = image::load_from_memory(&data) else { continue };
            let img = if img.width().max(img.height()) > 2048 { img.thumbnail(2048, 2048) } else { img };
            out.insert(name.clone(), img.to_rgb8());
        }
        Images(out)
    }

    pub fn get(&self, name: &str) -> Option<&RgbImage> {
        self.0.get(name)
    }
}

/// The color at (u, v), tiled, v up (linear 0-1).
pub fn sample(img: &RgbImage, uv: [f64; 2]) -> [f32; 3] {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let x = (uv[0].rem_euclid(1.0) * w).min(w - 1.0).max(0.0) as u32;
    let y = ((1.0 - uv[1].rem_euclid(1.0)) * h).min(h - 1.0).max(0.0) as u32;
    let p = img.get_pixel(x, y).0;
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
}
