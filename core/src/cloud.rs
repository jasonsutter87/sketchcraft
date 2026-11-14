//! Point clouds (scans): read from PLY, XYZ / PTS / TXT and LAS, kept as
//! compact arrays, shown in the view and snapped to while modeling over them
//! (SketchUp's Scan Essentials, at its core).
//!
//! Points are f32 meters near the origin: a survey cloud kilometers away (LAS
//! files in projected coordinates) is moved to the origin, and the shift is
//! reported. A cloud bigger than the budget keeps every k-th point.

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use glam::DVec3;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One scan.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cloud {
    pub name: String,
    /// x, y, z per point (meters, model space)
    #[serde(serialize_with = "ser_f32", deserialize_with = "de_f32")]
    pub points: Arc<Vec<f32>>,
    /// r, g, b per point, or empty
    #[serde(default, serialize_with = "ser_u8", deserialize_with = "de_u8")]
    pub colors: Arc<Vec<u8>>,
    #[serde(default)]
    pub hidden: bool,
    /// changes when the points do (the app caches by it)
    #[serde(default)]
    pub version: u32,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn ser_f32<S: Serializer>(v: &Arc<Vec<f32>>, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&b64().encode(f32_bytes(v)))
}

fn de_f32<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Arc<Vec<f32>>, D::Error> {
    let s = String::deserialize(d)?;
    let b = b64().decode(s).map_err(serde::de::Error::custom)?;
    if b.len() % 12 != 0 {
        return Err(serde::de::Error::custom("point data isn't whole points"));
    }
    Ok(Arc::new(b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()))
}

fn ser_u8<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&b64().encode(v.as_slice()))
}

fn de_u8<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Arc<Vec<u8>>, D::Error> {
    let s = String::deserialize(d)?;
    Ok(Arc::new(b64().decode(s).map_err(serde::de::Error::custom)?))
}

pub fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl Cloud {
    pub fn len(&self) -> usize {
        self.points.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Lowest and highest corners.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.points.chunks_exact(3).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// Every k-th point, at most `max` (for showing a huge cloud).
    pub fn thinned(&self, max: usize) -> (Vec<f32>, Vec<u8>) {
        let n = self.len();
        let k = n.div_ceil(max.max(1)).max(1);
        if k == 1 {
            return (self.points.to_vec(), self.colors.to_vec());
        }
        let has = self.colors.len() == self.points.len();
        let (mut p, mut c) = (Vec::with_capacity(n / k * 3 + 3), Vec::new());
        for i in (0..n).step_by(k) {
            p.extend_from_slice(&self.points[i * 3..i * 3 + 3]);
            if has {
                c.extend_from_slice(&self.colors[i * 3..i * 3 + 3]);
            }
        }
        (p, c)
    }
}

/// What reading a file gave: points in the file's units, colors if any.
struct Raw {
    points: Vec<DVec3>,
    colors: Vec<[u8; 3]>,
}

/// Read a cloud file by its extension. `scale`: meters per file unit.
pub fn read(path: &str, bytes: &[u8], scale: f64, max_points: usize) -> Result<(Cloud, DVec3)> {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let raw = match ext.as_str() {
        "ply" => ply(bytes)?,
        "las" => las(bytes)?,
        "laz" => bail!("LAZ files are compressed: convert to LAS first (for example with PDAL or CloudCompare)"),
        "xyz" | "pts" | "txt" | "csv" => ascii(bytes)?,
        _ => bail!("SketchCraft reads point clouds from .ply, .las, .xyz, .pts or .txt"),
    };
    if raw.points.is_empty() {
        bail!("no points in that file");
    }
    let name = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Scan".into());
    Ok(build(name, raw, scale, max_points))
}

/// Points to a cloud: scaled, thinned to the budget, moved near the origin
/// when far away. Returns the shift that was taken off.
fn build(name: String, raw: Raw, scale: f64, max_points: usize) -> (Cloud, DVec3) {
    let n = raw.points.len();
    let k = n.div_ceil(max_points.max(1)).max(1);
    let keep: Vec<usize> = (0..n).step_by(k).collect();
    let pts: Vec<DVec3> = keep.iter().map(|&i| raw.points[i] * scale).filter(|p| p.is_finite()).collect();
    let (lo, hi) = pts.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let center = (lo + hi) / 2.0;
    // far from the origin (a survey's coordinates): bring its middle, at its lowest, to the origin
    let shift = if center.truncate().length() > 10_000.0 || center.z.abs() > 10_000.0 { DVec3::new(center.x, center.y, lo.z) } else { DVec3::ZERO };
    let points: Vec<f32> = pts.iter().flat_map(|p| {
        let q = *p - shift;
        [q.x as f32, q.y as f32, q.z as f32]
    }).collect();
    let colors: Vec<u8> = if raw.colors.len() == n { keep.iter().filter(|&&i| (raw.points[i] * scale).is_finite()).flat_map(|&i| raw.colors[i]).collect() } else { vec![] };
    (Cloud { name, points: Arc::new(points), colors: Arc::new(colors), hidden: false, version: 1 }, shift)
}

/// Text: one point per line, "x y z" with optional colors ("x y z r g b",
/// or PTS's "x y z intensity r g b"); separators are spaces, tabs or commas.
fn ascii(bytes: &[u8]) -> Result<Raw> {
    let text = String::from_utf8_lossy(bytes);
    let mut points = vec![];
    let mut colors = vec![];
    let mut colored = true;
    for line in text.lines() {
        let nums: Vec<f64> = line.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|s| !s.is_empty()).map_while(|s| s.parse().ok()).collect();
        if nums.len() < 3 {
            continue; // a header, a count line (PTS), or junk
        }
        points.push(DVec3::new(nums[0], nums[1], nums[2]));
        let rgb = match nums.len() {
            6 => Some([nums[3], nums[4], nums[5]]),
            n if n >= 7 => Some([nums[4], nums[5], nums[6]]),
            _ => None,
        };
        match rgb {
            Some(c) if colored => colors.push(c),
            _ => colored = false,
        }
        if points.len() > 50_000_000 {
            bail!("that's over 50 million points: thin it first");
        }
    }
    let colors = if colored && colors.len() == points.len() {
        // 0..1 floats or 0..255
        let unit = colors.iter().all(|c| c.iter().all(|v| *v <= 1.0));
        colors.iter().map(|c| c.map(|v| (if unit { v * 255.0 } else { v }).round().clamp(0.0, 255.0) as u8)).collect()
    } else {
        vec![]
    };
    Ok(Raw { points, colors })
}

/// PLY (text or binary): the vertex element's x, y, z and red, green, blue.
fn ply(bytes: &[u8]) -> Result<Raw> {
    let end = bytes.windows(10).position(|w| w == b"end_header").context("not a PLY file (no end_header)")?;
    let header = String::from_utf8_lossy(&bytes[..end]);
    let mut body = end + 10;
    // the line break after end_header
    while body < bytes.len() && (bytes[body] == b'\r' || bytes[body] == b'\n') {
        body += 1;
        if bytes[body - 1] == b'\n' {
            break;
        }
    }
    if !header.starts_with("ply") {
        bail!("not a PLY file");
    }
    let mut format = "";
    // elements in order: (name, count, properties (type, name) or a list)
    let mut elements: Vec<(String, usize, Vec<(String, String)>, bool)> = vec![];
    for line in header.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["format", f, ..] => format = f,
            ["element", name, n] => elements.push((name.to_string(), n.parse().context("bad PLY element count")?, vec![], false)),
            ["property", "list", ..] => {
                if let Some(e) = elements.last_mut() {
                    e.3 = true;
                }
            }
            ["property", ty, name] => {
                if let Some(e) = elements.last_mut() {
                    e.2.push((ty.to_string(), name.to_string()));
                }
            }
            _ => {}
        }
    }
    let size = |t: &str| -> Option<usize> {
        Some(match t {
            "char" | "uchar" | "int8" | "uint8" => 1,
            "short" | "ushort" | "int16" | "uint16" => 2,
            "int" | "uint" | "float" | "int32" | "uint32" | "float32" => 4,
            "double" | "float64" => 8,
            _ => return None,
        })
    };
    let vi = elements.iter().position(|e| e.0 == "vertex").context("no vertex element in that PLY")?;
    let (_, count, props, _) = elements[vi].clone();
    if count > 100_000_000 {
        bail!("that PLY says it has over 100 million points");
    }
    let col = |n: &str| props.iter().position(|(_, p)| p == n);
    let (ix, iy, iz) = (col("x").context("no x in that PLY")?, col("y").context("no y")?, col("z").context("no z")?);
    let rgb = [["red", "r", "diffuse_red"], ["green", "g", "diffuse_green"], ["blue", "b", "diffuse_blue"]].map(|names| names.iter().find_map(|n| col(n)));
    let mut points = Vec::with_capacity(count.min(10_000_000));
    let mut colors = vec![];
    let has_rgb = rgb.iter().all(|c| c.is_some());
    let color_of = |i: usize, v: f64| -> u8 {
        // floats are 0..1, integers 0..255 (or 0..65535 for 16-bit)
        match props[i].0.as_str() {
            "float" | "float32" | "double" | "float64" => (v * 255.0).round().clamp(0.0, 255.0) as u8,
            "ushort" | "uint16" | "short" | "int16" => (v / 257.0).round().clamp(0.0, 255.0) as u8,
            _ => v.clamp(0.0, 255.0) as u8,
        }
    };
    match format {
        "ascii" => {
            if elements[..vi].iter().any(|e| e.1 > 0) {
                // elements before the vertices: skip their lines
            }
            let skip: usize = elements[..vi].iter().map(|e| e.1).sum();
            let text = String::from_utf8_lossy(&bytes[body..]);
            for line in text.lines().filter(|l| !l.trim().is_empty()).skip(skip).take(count) {
                let v: Vec<f64> = line.split_whitespace().map(|s| s.parse().unwrap_or(0.0)).collect();
                if v.len() < props.len() {
                    bail!("a PLY vertex line is short");
                }
                points.push(DVec3::new(v[ix], v[iy], v[iz]));
                if has_rgb {
                    colors.push([color_of(rgb[0].unwrap(), v[rgb[0].unwrap()]), color_of(rgb[1].unwrap(), v[rgb[1].unwrap()]), color_of(rgb[2].unwrap(), v[rgb[2].unwrap()])]);
                }
            }
        }
        "binary_little_endian" | "binary_big_endian" => {
            let le = format == "binary_little_endian";
            // elements before the vertices must be fixed-size to skip them
            let mut at = body;
            for e in &elements[..vi] {
                if e.3 {
                    bail!("that PLY has lists before its points; SketchCraft can't skip them");
                }
                let row: usize = e.2.iter().map(|(t, _)| size(t)).sum::<Option<usize>>().context("unknown PLY property type")?;
                at = at.checked_add(row.checked_mul(e.1).context("PLY too big")?).context("PLY too big")?;
            }
            let offs: Vec<usize> = props.iter().scan(0usize, |o, (t, _)| {
                let here = *o;
                *o += size(t).unwrap_or(0);
                Some(here)
            }).collect();
            let row: usize = props.iter().map(|(t, _)| size(t)).sum::<Option<usize>>().context("unknown PLY property type")?;
            if row == 0 {
                bail!("empty PLY vertex");
            }
            let need = row.checked_mul(count).and_then(|n| n.checked_add(at)).context("PLY too big")?;
            if need > bytes.len() {
                bail!("that PLY is cut short ({} points promised)", count);
            }
            let read = |b: &[u8], t: &str| -> f64 {
                macro_rules! num {
                    ($ty:ty, $n:expr) => {{
                        let a: [u8; $n] = b[..$n].try_into().unwrap();
                        (if le { <$ty>::from_le_bytes(a) } else { <$ty>::from_be_bytes(a) }) as f64
                    }};
                }
                match t {
                    "char" | "int8" => b[0] as i8 as f64,
                    "uchar" | "uint8" => b[0] as f64,
                    "short" | "int16" => num!(i16, 2),
                    "ushort" | "uint16" => num!(u16, 2),
                    "int" | "int32" => num!(i32, 4),
                    "uint" | "uint32" => num!(u32, 4),
                    "float" | "float32" => num!(f32, 4),
                    _ => num!(f64, 8),
                }
            };
            for k in 0..count {
                let r = &bytes[at + k * row..at + (k + 1) * row];
                let v = |i: usize| read(&r[offs[i]..], &props[i].0);
                points.push(DVec3::new(v(ix), v(iy), v(iz)));
                if has_rgb {
                    let c = rgb.map(|c| c.unwrap());
                    colors.push([color_of(c[0], v(c[0])), color_of(c[1], v(c[1])), color_of(c[2], v(c[2]))]);
                }
            }
        }
        _ => bail!("unknown PLY format {format:?}"),
    }
    Ok(Raw { points, colors })
}

/// LAS 1.0-1.4, point formats 0-10 (uncompressed): positions, and colors
/// where the format has them.
fn las(b: &[u8]) -> Result<Raw> {
    if b.len() < 227 || &b[0..4] != b"LASF" {
        bail!("not a LAS file");
    }
    let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let f64_at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let minor = b[25];
    let offset = u32_at(96) as usize;
    let format = b[104];
    if format & 0x80 != 0 || format & 0x40 != 0 {
        bail!("that LAS is compressed (LAZ): convert it to plain LAS first");
    }
    let fmt = format & 0x3f;
    let rec = u16_at(105) as usize;
    let mut count = u32_at(107) as u64;
    if count == 0 && minor >= 4 && b.len() >= 255 {
        count = u64::from_le_bytes(b[247..255].try_into().unwrap());
    }
    let scale = DVec3::new(f64_at(131), f64_at(139), f64_at(147));
    let off = DVec3::new(f64_at(155), f64_at(163), f64_at(171));
    let rgb_at: Option<usize> = match fmt {
        2 => Some(20),
        3 | 5 => Some(28),
        7 | 8 | 10 => Some(30),
        _ => None,
    };
    if rec < 12 || rgb_at.is_some_and(|o| o + 6 > rec) {
        bail!("that LAS has a point record too short for its format");
    }
    if count > 500_000_000 {
        bail!("that LAS says it has over 500 million points");
    }
    let count = count as usize;
    let avail = b.len().saturating_sub(offset) / rec;
    let count = count.min(avail);
    let mut points = Vec::with_capacity(count.min(10_000_000));
    let mut colors = vec![];
    let mut wide = false;
    let mut raw_c = vec![];
    for k in 0..count {
        let r = &b[offset + k * rec..offset + (k + 1) * rec];
        let i = |o: usize| i32::from_le_bytes(r[o..o + 4].try_into().unwrap()) as f64;
        points.push(DVec3::new(i(0) * scale.x + off.x, i(4) * scale.y + off.y, i(8) * scale.z + off.z));
        if let Some(o) = rgb_at {
            let c = [u16::from_le_bytes([r[o], r[o + 1]]), u16::from_le_bytes([r[o + 2], r[o + 3]]), u16::from_le_bytes([r[o + 4], r[o + 5]])];
            wide |= c.iter().any(|v| *v > 255);
            raw_c.push(c);
        }
    }
    if !raw_c.is_empty() {
        // colors are 16-bit by the spec, but some writers store 0..255
        colors = raw_c.iter().map(|c| c.map(|v| if wide { (v >> 8) as u8 } else { v as u8 })).collect();
    }
    Ok(Raw { points, colors })
}

/// The cloud point nearest a ray (within `radius` of it, in meters at that
/// distance per meter along the ray: a cone), for snapping.
pub fn nearest_to_ray(c: &Cloud, origin: DVec3, dir: DVec3, cone: f64) -> Option<DVec3> {
    let d = dir.normalize_or_zero();
    let mut best: Option<(f64, DVec3)> = None;
    for p in c.points.chunks_exact(3) {
        let q = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        let t = (q - origin).dot(d);
        if t <= 0.0 {
            continue;
        }
        let off = (q - origin - d * t).length();
        if off <= cone * t {
            let score = off / t + t * 1e-6;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, q));
            }
        }
    }
    best.map(|(_, q)| q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_with_and_without_colors() {
        let r = ascii(b"3\n0 0 0 255 0 0\n1 2 3 0 255 0\n4,5,6,0,0,255\n").unwrap();
        assert_eq!(r.points.len(), 3);
        assert_eq!(r.colors, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]]);
        // PTS: x y z intensity r g b
        let r = ascii(b"0 0 0 -500 10 20 30\n").unwrap();
        assert_eq!(r.colors, vec![[10, 20, 30]]);
        let r = ascii(b"x y z\n0 0 0\n1 1 1\n").unwrap();
        assert_eq!((r.points.len(), r.colors.len()), (2, 0));
    }

    #[test]
    fn ply_ascii_and_binary() {
        let a = b"ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n0 0 0 255 0 0\n1 2 3 0 0 255\n";
        let r = ply(a).unwrap();
        assert_eq!(r.points[1], DVec3::new(1.0, 2.0, 3.0));
        assert_eq!(r.colors[1], [0, 0, 255]);
        let mut bin = b"ply\nformat binary_little_endian 1.0\nelement vertex 2\nproperty double x\nproperty double y\nproperty double z\nproperty float red\nproperty float green\nproperty float blue\nend_header\n".to_vec();
        for (p, c) in [([1.0f64, 2.0, 3.0], [1.0f32, 0.5, 0.0]), ([4.0, 5.0, 6.0], [0.0, 0.0, 1.0])] {
            for v in p {
                bin.extend(v.to_le_bytes());
            }
            for v in c {
                bin.extend(v.to_le_bytes());
            }
        }
        let r = ply(&bin).unwrap();
        assert_eq!(r.points, vec![DVec3::new(1.0, 2.0, 3.0), DVec3::new(4.0, 5.0, 6.0)]);
        assert_eq!(r.colors[0], [255, 128, 0]);
        // cut short: an error, not a panic
        assert!(ply(&bin[..bin.len() - 3]).is_err());
    }

    #[test]
    fn las_format_2_with_colors_far_away() {
        // a minimal LAS 1.2 header (227 bytes) and two format-2 points (26 bytes each)
        let mut b = vec![0u8; 227];
        b[0..4].copy_from_slice(b"LASF");
        b[24] = 1;
        b[25] = 2;
        b[94..96].copy_from_slice(&227u16.to_le_bytes());
        b[96..100].copy_from_slice(&227u32.to_le_bytes());
        b[104] = 2;
        b[105..107].copy_from_slice(&26u16.to_le_bytes());
        b[107..111].copy_from_slice(&2u32.to_le_bytes());
        for (o, v) in [(131, 0.01f64), (139, 0.01), (147, 0.01), (155, 500_000.0), (163, 4_000_000.0), (171, 0.0)] {
            b[o..o + 8].copy_from_slice(&v.to_le_bytes());
        }
        for (x, y, z, c) in [(0i32, 0i32, 100i32, 65535u16), (1000, 200, 300, 0)] {
            let mut r = vec![0u8; 26];
            r[0..4].copy_from_slice(&x.to_le_bytes());
            r[4..8].copy_from_slice(&y.to_le_bytes());
            r[8..12].copy_from_slice(&z.to_le_bytes());
            for k in 0..3 {
                r[20 + k * 2..22 + k * 2].copy_from_slice(&c.to_le_bytes());
            }
            b.extend(r);
        }
        let raw = las(&b).unwrap();
        assert_eq!(raw.points[1], DVec3::new(500_010.0, 4_000_002.0, 3.0));
        assert_eq!(raw.colors, vec![[255, 255, 255], [0, 0, 0]]);
        // survey coordinates come in near the origin, and the shift is given
        let (c, shift) = build("t".into(), raw, 1.0, 1_000_000);
        assert!((shift.x - 500_005.0).abs() < 1e-6 && (shift.y - 4_000_001.0).abs() < 1e-6 && (shift.z - 1.0).abs() < 1e-6);
        assert_eq!(&c.points[..3], &[-5.0, -1.0, 0.0]);
        // truncated: fewer points, no panic
        assert_eq!(las(&b[..b.len() - 10]).unwrap().points.len(), 1);
    }

    #[test]
    fn thinning_and_budget() {
        let raw = Raw { points: (0..1000).map(|i| DVec3::new(i as f64, 0.0, 0.0)).collect(), colors: vec![] };
        let (c, _) = build("t".into(), raw, 1.0, 100);
        assert_eq!(c.len(), 100);
        let (p, _) = c.thinned(10);
        assert_eq!(p.len() / 3, 10);
        // round-trips through the save format
        let j = serde_json::to_string(&c).unwrap();
        let back: Cloud = serde_json::from_str(&j).unwrap();
        assert_eq!(back.points, c.points);
    }
}
