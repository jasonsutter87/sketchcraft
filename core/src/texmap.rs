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

    /// The mapping for one face: its painted-through placement while that still holds (same material, axes in
    /// the face's plane, not a projected texture), else the material's own rule.
    pub fn of_face(t: &Texture, f: &crate::components::WFace) -> Mapping {
        let place = f.place.as_ref().filter(|p| f.material.as_deref() == Some(p.material.as_str()));
        Mapping::of_place(t, place, f.normal)
    }

    /// As `of_face`, from the placement (already checked against the face's material) and the face's normal.
    pub fn of_place(t: &Texture, place: Option<&crate::model::TexPlace>, normal: DVec3) -> Mapping {
        if t.project.is_none() {
            if let Some(p) = place {
                let (u, v) = (DVec3::from(p.u), DVec3::from(p.v));
                let ok = |a: DVec3| (a.length() - 1.0).abs() < 1e-6 && a.dot(normal).abs() < 1e-4;
                if ok(u) && ok(v) {
                    return Mapping { u, v, origin: DVec3::from(p.origin), width: t.width.max(1e-6), height: t.height.max(1e-6) };
                }
            }
        }
        Mapping::of(t, normal)
    }

    /// (u, v) with v up the image (the viewport's convention)
    pub fn uv(&self, p: DVec3) -> [f64; 2] {
        let q = p - self.origin;
        [q.dot(self.u) / self.width, q.dot(self.v) / self.height]
    }
}

/// Where a face's picture lands: the material's plane placement, or the face's own corner coordinates (projection
/// mapping), which win when the face has them.
#[derive(Clone, Debug)]
pub struct FaceMap {
    plane: Mapping,
    corners: Option<Vec<(DVec3, [f64; 2])>>,
}

impl FaceMap {
    pub fn of(t: &Texture, f: &WFace) -> FaceMap {
        let corners = f.uv.as_ref().map(|(o, h)| {
            let mut c: Vec<(DVec3, [f64; 2])> = f.outer.iter().zip(o).map(|(p, u)| (*p, [u[0] as f64, u[1] as f64])).collect();
            for (ring, uv) in f.holes.iter().zip(h) {
                c.extend(ring.iter().zip(uv).map(|(p, u)| (*p, [u[0] as f64, u[1] as f64])));
            }
            c
        });
        FaceMap { plane: Mapping::of_face(t, f), corners }
    }

    /// Does the face carry its own coordinates?
    pub fn has_corners(&self) -> bool {
        self.corners.is_some()
    }

    fn corner(&self, p: DVec3) -> Option<[f64; 2]> {
        self.corners.as_ref()?.iter().find(|(q, _)| *q == p).map(|(_, u)| *u)
    }

    /// (u, v) at a corner of the face's own triangulation (exactly one of its points), v up the image.
    pub fn uv(&self, p: DVec3) -> [f64; 2] {
        self.corner(p).unwrap_or_else(|| self.plane.uv(p))
    }

    /// (u, v) at any point `p` inside the triangle `tri` (made from the face's points): interpolated between the corners.
    pub fn uv_in(&self, tri: [DVec3; 3], p: DVec3) -> [f64; 2] {
        let (Some(a), Some(b), Some(c)) = (self.corner(tri[0]), self.corner(tri[1]), self.corner(tri[2])) else {
            return self.plane.uv(p);
        };
        let (e1, e2, q) = (tri[1] - tri[0], tri[2] - tri[0], p - tri[0]);
        let (d11, d12, d22, d1, d2) = (e1.dot(e1), e1.dot(e2), e2.dot(e2), q.dot(e1), q.dot(e2));
        let den = d11 * d22 - d12 * d12;
        if den.abs() < 1e-18 {
            return a;
        }
        let (s, t) = ((d22 * d1 - d12 * d2) / den, (d11 * d2 - d12 * d1) / den);
        [a[0] + s * (b[0] - a[0]) + t * (c[0] - a[0]), a[1] + s * (b[1] - a[1]) + t * (c[1] - a[1])]
    }
}

/// The lightmap channel (`Face::uv2`) of one world face, looked up by the corner's exact point like `FaceMap`. Only the
/// exporters read it: it never changes how the model is drawn.
pub struct Lightmap(Option<HashMap<[u64; 3], [f32; 2]>>);

/// A face's corner coordinates as `WFace` holds them: the outer loop's, then each hole's.
type CornerUv = (Vec<[f32; 2]>, Vec<Vec<[f32; 2]>>);

impl Lightmap {
    pub fn of(f: &WFace) -> Lightmap {
        Lightmap::from(f, f.uv2.as_ref())
    }

    /// The face's own picture coordinates (`Face::uv`) the same way: the viewport's checker shows them on faces that have
    /// no picture.
    pub fn own(f: &WFace) -> Lightmap {
        Lightmap::from(f, f.uv.as_ref())
    }

    fn from(f: &WFace, uv: Option<&CornerUv>) -> Lightmap {
        let key = |p: &DVec3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        Lightmap(uv.map(|(o, h)| {
            let mut m = HashMap::new();
            for (p, c) in f.outer.iter().zip(o) {
                m.insert(key(p), *c);
            }
            for (ring, cs) in f.holes.iter().zip(h) {
                for (p, c) in ring.iter().zip(cs) {
                    m.insert(key(p), *c);
                }
            }
            m
        }))
    }

    /// Does the face have a lightmap set?
    pub fn any(&self) -> bool {
        self.0.is_some()
    }

    /// (u, v) at a corner, v up the sheet; (0, 0) where the face has none.
    pub fn at(&self, p: DVec3) -> [f32; 2] {
        self.0.as_ref().and_then(|m| m.get(&[p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).copied()).unwrap_or([0.0, 0.0])
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

/// The image file names of one export's textures folder: `file_name` per material, with "-2", "-3"... added when two
/// materials would get the same name (after the name is made file-safe, and ignoring case, as many disks do), so one
/// picture never overwrites another.
#[derive(Default)]
pub struct FileNames(std::collections::HashSet<String>);

impl FileNames {
    pub fn name(&mut self, material: &str, ext: &str) -> String {
        let base = file_name(material, ext);
        let stem = base.strip_suffix(&format!(".{ext}")).unwrap_or(&base).to_string();
        let mut name = base;
        let mut k = 2;
        while !self.0.insert(name.to_ascii_lowercase()) {
            name = format!("{stem}-{k}.{ext}");
            k += 1;
        }
        name
    }
}

/// Decoded images for headless renders, by material name. Images are capped
/// at 2048 px so a huge photo can't blow up memory.
pub struct Images(HashMap<String, RgbImage>);

impl Images {
    pub fn load(m: &Model) -> Images {
        Images::load_if(m, |_| true)
    }

    /// Only the pictures of the materials `want` names (decoding is the expensive part).
    pub fn load_if(m: &Model, want: impl Fn(&str) -> bool) -> Images {
        let mut out = HashMap::new();
        let all = m.materials.iter().chain(m.definitions.values().flat_map(|d| d.model.materials.iter()));
        for (name, mat) in all {
            let Some(t) = &mat.texture else { continue };
            if !want(name) {
                continue;
            }
            if out.contains_key(name) {
                continue;
            }
            let Ok((_, data)) = bytes(t) else {
                crate::safety::note_skipped(name, "its picture isn't readable, so it is drawn in its plain color");
                continue;
            };
            let img = match crate::safety::decode_image(&data, None) {
                Ok(i) => i,
                Err(e) => {
                    crate::safety::note_skipped(name, &format!("{e}; it is drawn in its plain color"));
                    continue;
                }
            };
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
