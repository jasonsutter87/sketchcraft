//! Project a matched photo onto faces (SketchUp's "Project photo"): for each
//! face, a straightened image of it is cut from the photo (every texel traced
//! from the face, through the matched camera, into the picture) and laid on
//! the face as an ordinary texture. The result works everywhere textures do:
//! the view, every export and both renderers.

use anyhow::{bail, Context, Result};
use glam::DVec3;
use image::{Rgb, RgbImage};

use crate::geom;
use crate::model::{Id, Material, Model, Texture, View};

pub struct Projected {
    pub faces: Vec<Id>,
    /// faces turned away from the camera (they can't be seen in the photo)
    pub away: usize,
}

/// Where a point lands in the photo: (x, y) in 0-1 from the top-left, or None behind the camera.
fn to_photo(v: &View, aspect: f64, p: DVec3) -> Option<(f64, f64)> {
    let eye = DVec3::from(v.eye);
    let f = (DVec3::from(v.target) - eye).normalize();
    let r = f.cross(DVec3::Z).normalize_or(DVec3::X);
    let u = r.cross(f);
    let d = p - eye;
    let z = d.dot(f);
    if z <= 1e-6 {
        return None;
    }
    // the photo fills the view's height (vertical field of view), centered
    let t = (v.fov.to_radians() / 2.0).tan();
    let ny = d.dot(u) / (z * t);
    let nx = d.dot(r) / (z * t * aspect);
    Some((0.5 + 0.5 * nx, 0.5 - 0.5 * ny))
}

fn bilinear(img: &RgbImage, x: f64, y: f64) -> Option<[f64; 3]> {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let (fx, fy) = (x * w - 0.5, y * h - 0.5);
    if fx < -0.5 || fy < -0.5 || fx > w - 0.5 || fy > h - 0.5 {
        return None;
    }
    let (x0, y0) = (fx.floor().max(0.0), fy.floor().max(0.0));
    let (x1, y1) = ((x0 + 1.0).min(w - 1.0), (y0 + 1.0).min(h - 1.0));
    let (tx, ty) = ((fx - x0).clamp(0.0, 1.0), (fy - y0).clamp(0.0, 1.0));
    let px = |x: f64, y: f64| img.get_pixel(x as u32, y as u32).0;
    let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
    let mut out = [0.0; 3];
    for k in 0..3 {
        let top = a[k] as f64 * (1.0 - tx) + b[k] as f64 * tx;
        let bot = c[k] as f64 * (1.0 - tx) + d[k] as f64 * tx;
        out[k] = top * (1.0 - ty) + bot * ty;
    }
    Some(out)
}

/// Project the photo of saved view `view` onto `faces` (`size`: pixels on a face's long side).
pub fn project(m: &mut Model, view: &str, faces: &[Id], size: u32) -> Result<Projected> {
    let v = m.views.iter().find(|x| x.name == view).cloned().with_context(|| format!("no saved view called {view:?}"))?;
    let Some(photo) = &v.photo else { bail!("{view:?} has no photo: match a photo first (Views › Match photo…)") };
    let tex = Texture { data: photo.clone(), width: 1.0, height: 1.0, project: None, origin: None };
    let (_, bytes) = crate::texmap::bytes(&tex)?;
    let img = image::load_from_memory(&bytes).context("the photo can't be read")?.to_rgb8();
    let aspect = img.width() as f64 / img.height() as f64;
    let size = size.clamp(64, 4096) as f64;
    let mut out = Projected { faces: vec![], away: 0 };
    let eye = DVec3::from(v.eye);
    for &fid in faces {
        let f = m.face(fid)?.clone();
        let pts = m.pts(&f.outer);
        let n = m.normal(fid);
        if n.dot(eye - geom::centroid(&pts)) <= 0.0 {
            out.away += 1;
            continue;
        }
        // the face's own axes (as textures lie on faces) and its extent along them
        let (u, w) = geom::basis(n);
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for p in &pts {
            let (a, b) = (p.dot(u), p.dot(w));
            lo = (lo.0.min(a), lo.1.min(b));
            hi = (hi.0.max(a), hi.1.max(b));
        }
        let (width, height) = ((hi.0 - lo.0).max(1e-4), (hi.1 - lo.1).max(1e-4));
        let k = size / width.max(height);
        let (tw, th) = (((width * k).round() as u32).max(8), ((height * k).round() as u32).max(8));
        let plane_d = pts[0].dot(n);
        let origin = u * lo.0 + w * lo.1 + n * plane_d;
        let mut out_img = RgbImage::new(tw, th);
        for y in 0..th {
            for x in 0..tw {
                // texel centre on the face (rows run top-down in the image, v runs up)
                let s = (x as f64 + 0.5) / tw as f64;
                let t = 1.0 - (y as f64 + 0.5) / th as f64;
                let p = origin + u * (s * width) + w * (t * height);
                let c = to_photo(&v, aspect, p).and_then(|(px, py)| bilinear(&img, px, py)).unwrap_or([150.0, 150.0, 150.0]);
                out_img.put_pixel(x, y, Rgb([c[0].round() as u8, c[1].round() as u8, c[2].round() as u8]));
            }
        }
        let mut jpg = std::io::Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 88).encode_image(&out_img)?;
        use base64::Engine as _;
        let data = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpg.into_inner()));
        let mut name = format!("Photo: {view} {fid}");
        let mut k2 = 2;
        while m.materials.contains_key(&name) {
            name = format!("Photo: {view} {fid} #{k2}");
            k2 += 1;
        }
        // average colour for flat views and exports without images
        let avg = out_img.pixels().fold([0.0f64; 3], |a, p| [a[0] + p.0[0] as f64, a[1] + p.0[1] as f64, a[2] + p.0[2] as f64]);
        let cnt = (tw * th) as f64 * 255.0;
        m.materials.insert(
            name.clone(),
            Material { color: [(avg[0] / cnt) as f32, (avg[1] / cnt) as f32, (avg[2] / cnt) as f32], texture: Some(Texture { data, width, height, project: Some((-n).to_array()), origin: Some(origin.to_array()) }), opacity: 1.0 },
        );
        m.faces.get_mut(&fid).unwrap().material = Some(name);
        out.faces.push(fid);
    }
    Ok(out)
}
