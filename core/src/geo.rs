//! Add Location: real ground under the model. For a latitude / longitude it
//! fetches open elevation tiles (AWS Terrain Tiles, "terrarium" PNGs) and,
//! optionally, OpenStreetMap map tiles; builds a terrain of the area (the
//! origin at ground level, +Y north) and lays the map on it from above.
//! Tiles are cached; downloads use the system's curl.

use crate::model::{Id, Location, Material, Model, Texture};
use anyhow::{bail, Context, Result};
use glam::DVec3;
use image::{GenericImageView, RgbImage};

const EARTH: f64 = 40_075_016.686; // meters around the equator
const TERRAIN: &str = "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
const MAP: &str = "https://tile.openstreetmap.org";

/// Fetch a URL (cached on disk).
pub fn fetch(url: &str) -> Result<Vec<u8>> {
    let key: String = url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let dir = std::env::temp_dir().join("sketchcraft-tiles");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join(key);
    if let Ok(b) = std::fs::read(&file) {
        return Ok(b);
    }
    let out = std::process::Command::new("curl")
        .args(["-sSfL", "--max-time", "20", "-A", "SketchCraft/0.2 (3D modeling app)", url])
        .output()
        .context("downloading needs curl")?;
    if !out.status.success() {
        bail!("couldn't download {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    std::fs::write(&file, &out.stdout).ok();
    Ok(out.stdout)
}

/// Fractional tile coordinates of a point at a zoom.
fn tile_xy(lat: f64, lon: f64, z: u32) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    let r = lat.to_radians();
    ((lon + 180.0) / 360.0 * n, (1.0 - (r.tan() + 1.0 / r.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
}

/// The area as one image: tiles stitched, then cut to the box between the
/// tile coordinates (x0, y0)-(x1, y1).
fn stitched(base: &str, z: u32, x0: f64, y0: f64, x1: f64, y1: f64, out: u32, get: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<RgbImage> {
    let (tx0, ty0, tx1, ty1) = (x0.floor() as i64, y0.floor() as i64, x1.floor() as i64, y1.floor() as i64);
    if (tx1 - tx0 + 1) * (ty1 - ty0 + 1) > 36 {
        bail!("that area needs too many tiles");
    }
    let (w, h) = (((tx1 - tx0 + 1) * 256) as u32, ((ty1 - ty0 + 1) * 256) as u32);
    let mut all = RgbImage::new(w, h);
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let bytes = get(&format!("{base}/{z}/{tx}/{ty}.png"))?;
            let img = image::load_from_memory(&bytes).context("a map tile isn't an image")?.to_rgb8();
            image::imageops::overlay(&mut all, &img, (tx - tx0) * 256, (ty - ty0) * 256);
        }
    }
    // resample the box to out x out
    let mut res = RgbImage::new(out, out);
    let (ox, oy) = ((x0 - tx0 as f64) * 256.0, (y0 - ty0 as f64) * 256.0);
    let (sx, sy) = ((x1 - x0) * 256.0 / out as f64, (y1 - y0) * 256.0 / out as f64);
    for (px, py, p) in res.enumerate_pixels_mut() {
        let (fx, fy) = (ox + (px as f64 + 0.5) * sx, oy + (py as f64 + 0.5) * sy);
        *p = *all.get_pixel((fx as u32).min(w - 1), (fy as u32).min(h - 1));
    }
    Ok(res)
}

fn height(img: &RgbImage, fx: f64, fy: f64) -> f64 {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let at = |x: f64, y: f64| {
        let p = img.get_pixel(x.clamp(0.0, w - 1.0) as u32, y.clamp(0.0, h - 1.0) as u32);
        // below sea level is the sea floor: show the water's surface instead
        (p[0] as f64 * 256.0 + p[1] as f64 + p[2] as f64 / 256.0 - 32768.0).max(0.0)
    };
    // bilinear
    let (x, y) = (fx * (w - 1.0), fy * (h - 1.0));
    let (x0, y0) = (x.floor(), y.floor());
    let (dx, dy) = (x - x0, y - y0);
    at(x0, y0) * (1.0 - dx) * (1.0 - dy) + at(x0 + 1.0, y0) * dx * (1.0 - dy) + at(x0, y0 + 1.0) * (1.0 - dx) * dy + at(x0 + 1.0, y0 + 1.0) * dx * dy
}

pub struct Placed {
    pub group: Id,
    pub elevation: f64,
    pub zoom: u32,
}

/// Build the location: a `size` x `size` meter terrain centered on the
/// origin, `cells` squares a side, with the map on it if `imagery`.
pub fn add_location(m: &mut Model, lat: f64, lon: f64, size: f64, cells: usize, imagery: bool, get: &dyn Fn(&str) -> Result<Vec<u8>>) -> Result<Placed> {
    if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        bail!("latitude is -85..85, longitude -180..180");
    }
    if !(20.0..=5000.0).contains(&size) {
        bail!("the area is 20 m to 5 km across");
    }
    let cells = cells.clamp(4, 200);
    let cos = lat.to_radians().cos();
    // a zoom where ~3 tiles span the area
    let z = ((EARTH * cos / (size / 3.0)).log2().round() as u32).clamp(10, 18);
    let half_lat = size / 2.0 / 111_320.0;
    let half_lon = size / 2.0 / (111_320.0 * cos);
    let tz = z.min(15);
    let (tx0, ty0) = tile_xy(lat + half_lat, lon - half_lon, tz);
    let (tx1, ty1) = tile_xy(lat - half_lat, lon + half_lon, tz);
    let dem = stitched(TERRAIN, tz, tx0, ty0, tx1, ty1, 256, get)?;
    let h0 = height(&dem, 0.5, 0.5);
    // the grid: x east, y north, z above the origin's ground
    let n = cells;
    let mut pts = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let (fx, fy) = (i as f64 / n as f64, j as f64 / n as f64);
            let z = height(&dem, fx, 1.0 - fy) - h0;
            pts.push(DVec3::new(-size / 2.0 + fx * size, -size / 2.0 + fy * size, z));
        }
    }
    let idx = |i: usize, j: usize| j * (n + 1) + i;
    let mut tris = Vec::new();
    for j in 0..n {
        for i in 0..n {
            tris.push([idx(i, j), idx(i + 1, j), idx(i + 1, j + 1)]);
            tris.push([idx(i, j), idx(i + 1, j + 1), idx(i, j + 1)]);
        }
    }
    let gid = m.terrain_group("Location terrain", &pts, &tris)?;
    if imagery {
        let (mx0, my0) = tile_xy(lat + half_lat, lon - half_lon, z);
        let (mx1, my1) = tile_xy(lat - half_lat, lon + half_lon, z);
        let map = stitched(MAP, z, mx0, my0, mx1, my1, 1024, get)?;
        // projected from above: the image's x runs south->north, y west->east
        // (how a top-down projection lays a texture); turn the map to match
        let w = map.width();
        let turned = RgbImage::from_fn(w, w, |c, r| *map.get_pixel(w - 1 - r, c));
        let mut jpg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(turned.clone()).write_to(&mut jpg, image::ImageFormat::Jpeg)?;
        use base64::Engine as _;
        let data = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpg.into_inner()));
        let avg = {
            let small = image::DynamicImage::ImageRgb8(turned).thumbnail(1, 1);
            let p = small.get_pixel(0, 0);
            [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
        };
        let name = "Location imagery".to_string();
        m.materials.insert(name.clone(), Material { color: avg, texture: Some(Texture { data, width: size, height: size, project: Some([0.0, 0.0, -1.0]), origin: Some([-size / 2.0, -size / 2.0, 0.0]) }) });
        let faces: Vec<Id> = m.faces.iter().filter(|(_, f)| f.group == Some(gid)).map(|(k, _)| *k).collect();
        for f in faces {
            m.faces.get_mut(&f).unwrap().material = Some(name.clone());
        }
    }
    m.location = Some(Location { lat, lon, elevation: h0 });
    Ok(Placed { group: gid, elevation: h0, zoom: z })
}
