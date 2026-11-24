//! Edit a material's picture in PhotoCraft and bring it back.
//!
//! SketchCraft writes the picture as an OpenRaster file (`.ora`: a zip of PNG layers and a `stack.xml`), with the
//! UV layout on top as a guide layer, into a `Textures` folder beside the model, and opens it in PhotoCraft. When the
//! file is saved there, `read` flattens its visible layers, leaving out the guide, into a PNG the UI hands to the
//! engine's `texture` command (one undo step). OpenRaster is an open format PhotoCraft opens and saves with its
//! layers, so the next edit reopens the same file and the layers made in PhotoCraft are still there.
//!
//! Every path is the `Textures` folder plus a name made from the material's (letters, digits, `-`, `_` only), so
//! the page can't name a file anywhere else; PhotoCraft is started as a fixed program with the file as its only
//! argument, never through a shell.

use base64::Engine as _;
use image::{imageops, ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The guide layer's name. Any layer whose name starts with this is left out when the picture comes back.
pub const GUIDE: &str = "UV layout (SketchCraft guide)";
/// Largest picture taken either way (pixels per side), the engine's own limit for textures.
const MAX_SIDE: u32 = 8192;
/// Most layers read back from one `.ora`, and most pixels decoded across them (a hostile file can't make a poll
/// decode without end).
const MAX_LAYERS: usize = 64;
const MAX_TOTAL_PIXELS: u64 = 256 << 20;

/// Refuse a path that is a symlink (or a folder reached through one). Missing is fine.
pub fn refuse_link(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(format!("{} is a link; SketchCraft won't write through it", path.display())),
        _ => Ok(()),
    }
}

/// Make the `Textures` folder if needed; refuse it when it is a link.
pub fn ensure_dir(dir: &Path) -> Result<(), String> {
    refuse_link(dir)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    refuse_link(dir)
}

/// A new file at `path` that never follows a link: a stale one (or a link) at that name is removed first (removing
/// never follows), then the file is created with `create_new`.
fn create_fresh(path: &Path) -> Result<std::fs::File, String> {
    let _ = std::fs::remove_file(path);
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| format!("can't write {}: {e}", path.display()))
}

/// Write `bytes` to `path` through a temp file in the same folder, then rename it into place. Never writes through a link.
pub fn write_file_safely(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        refuse_link(dir)?;
    }
    refuse_link(path)?;
    let tmp = PathBuf::from(format!("{}.part", path.display()));
    let mut f = create_fresh(&tmp)?;
    f.write_all(bytes).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| format!("can't write {}: {e}", path.display()))
}

/// Keep the old `.ora` as `.bak` (a link is refused, not moved).
pub fn keep_backup(ora: &Path) -> Result<(), String> {
    refuse_link(ora)?;
    let bak = ora.with_extension("ora.bak");
    refuse_link(&bak)?;
    std::fs::rename(ora, &bak).map_err(|e| format!("can't keep {}: {e}", bak.display()))
}

/// Tag at the start of a stored picture hash: the algorithm that made it, so a later change of algorithm is seen
/// rather than guessed at.
const HASH_TAG: &str = "sha256:";

/// A content hash of a picture's bytes for the `.ora.sketchcraft` file: `sha256:<64 hex>`. SHA-256 is fixed by
/// standard, so it is the same on every platform and every SketchCraft/Rust release (the old `DefaultHasher` was
/// not promised stable).
pub fn hash_of(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut s = String::with_capacity(HASH_TAG.len() + 64);
    s.push_str(HASH_TAG);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Most bytes of a hash file that are read (the tagged line is about 71).
const MAX_HASH_FILE: u64 = 256;

/// Read the hash file beside an `.ora`: `Ok(None)` when it is missing or too big to be one (a mismatch, so the
/// layers are kept as a backup, never lost); an error when it is a link or anything but a plain file (a device or
/// pipe would never end). Reads at most `MAX_HASH_FILE` bytes.
pub fn read_hash_file(path: &Path) -> Result<Option<String>, String> {
    use std::io::Read;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a link; SketchCraft won't read through it", path.display()));
    }
    if !meta.is_file() {
        return Err(format!("{} is not a plain file", path.display()));
    }
    let mut buf = Vec::new();
    let f = std::fs::File::open(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    f.take(MAX_HASH_FILE + 1).read_to_end(&mut buf).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    if buf.len() as u64 > MAX_HASH_FILE {
        return Ok(None);
    }
    Ok(String::from_utf8(buf).ok())
}

/// Does a stored hash carry an algorithm tag (the current format)?
pub fn is_tagged(stored: &str) -> bool {
    stored.trim().starts_with(HASH_TAG)
}

/// The hash files written before 0.3.10: 16 hex digits from std's `DefaultHasher`, no tag. Kept only to read them.
fn legacy_hash_of(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Is the picture `bytes` the one the stored hash file describes?
/// - tagged `sha256:...`: an exact comparison; a difference is a real change.
/// - untagged (written by an older SketchCraft): `DefaultHasher` output may differ between Rust releases, so a
///   mismatch can't be told from a changed picture. To never move someone's PhotoCraft layers to `.bak` on the
///   first run after an update, an untagged value counts as unchanged whether or not it matches the old hasher;
///   the caller rewrites it in the tagged format, so this happens once.
/// - no file, or another tag (a newer algorithm this build doesn't know): not a match.
pub fn stored_hash_matches(stored: Option<&str>, bytes: &[u8]) -> bool {
    let Some(h) = stored.map(str::trim) else { return false };
    if h.starts_with(HASH_TAG) {
        return h == hash_of(bytes);
    }
    if h.contains(':') {
        return false;
    }
    // legacy: compared with the old hasher for the record, but a mismatch can't be judged, so it is accepted too
    let _same_as_old_hasher = h == legacy_hash_of(bytes);
    true
}

/// A file name for a material: letters, digits, `-` and `_`, at most 80 characters, never empty.
pub fn file_stem(material: &str) -> String {
    let s: String = material.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(80).collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { "texture".into() } else { s }
}

/// `<folder>/<stem>.ora`.
pub fn ora_path(folder: &Path, material: &str) -> PathBuf {
    folder.join(format!("{}.ora", file_stem(material)))
}

/// The bytes of a `data:` URL (base64 only).
pub fn decode_data_url(url: &str) -> Result<Vec<u8>, String> {
    let (head, body) = url.split_once(',').ok_or("not a data: URL")?;
    if !head.starts_with("data:") || !head.ends_with(";base64") {
        return Err("not a base64 data: URL".into());
    }
    base64::engine::general_purpose::STANDARD.decode(body.trim()).map_err(|e| format!("bad base64: {e}"))
}

fn png_bytes(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn load(bytes: &[u8]) -> Result<RgbaImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("unreadable picture: {e}"))?.to_rgba8();
    if img.width() == 0 || img.height() == 0 || img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        return Err(format!("the picture is {}×{}; it must be 1 to {MAX_SIDE} pixels a side", img.width(), img.height()));
    }
    Ok(img)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Write `<path>` as OpenRaster: the texture, and (if given, scaled to the texture's size) the UV guide on top.
pub fn write_ora(path: &Path, texture: &[u8], guide: Option<&[u8]>) -> Result<(u32, u32), String> {
    let tex = load(texture)?;
    let (w, h) = tex.dimensions();
    let guide = match guide {
        Some(g) => {
            let g = load(g)?;
            Some(if g.dimensions() == (w, h) { g } else { imageops::resize(&g, w, h, imageops::FilterType::Triangle) })
        }
        None => None,
    };
    let mut merged = tex.clone();
    if let Some(g) = &guide {
        over(&mut merged, g, 0, 0, GUIDE_OPACITY);
    }
    let thumb = {
        let s = 256.0 / w.max(h) as f32;
        if s < 1.0 { imageops::resize(&merged, ((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1), imageops::FilterType::Triangle) } else { merged.clone() }
    };
    let mut layers = String::new();
    if guide.is_some() {
        layers += &format!("    <layer name=\"{}\" src=\"data/guide.png\" x=\"0\" y=\"0\" opacity=\"{GUIDE_OPACITY}\" visibility=\"visible\" composite-op=\"svg:src-over\"/>\n", xml_escape(GUIDE));
    }
    layers += "    <layer name=\"Texture\" src=\"data/texture.png\" x=\"0\" y=\"0\" opacity=\"1.0\" visibility=\"visible\" composite-op=\"svg:src-over\"/>\n";
    let stack = format!("<?xml version='1.0' encoding='UTF-8'?>\n<image version=\"0.0.5\" w=\"{w}\" h=\"{h}\">\n  <stack>\n{layers}  </stack>\n</image>\n");

    if let Some(dir) = path.parent() {
        ensure_dir(dir)?;
    }
    refuse_link(path)?;
    // written beside the target, then moved into place: PhotoCraft never sees half a file
    let tmp = path.with_extension("ora.part");
    {
        let f = create_fresh(&tmp)?;
        let mut z = zip::ZipWriter::new(f);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, bytes: &[u8], opts| -> Result<(), String> {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            z.write_all(bytes).map_err(|e| e.to_string())
        };
        // the spec: "mimetype" first and uncompressed
        put("mimetype", b"image/openraster", stored)?;
        put("stack.xml", stack.as_bytes(), deflated)?;
        put("data/texture.png", &png_bytes(&tex)?, stored)?;
        if let Some(g) = &guide {
            put("data/guide.png", &png_bytes(g)?, stored)?;
        }
        put("mergedimage.png", &png_bytes(&merged)?, stored)?;
        put("Thumbnails/thumbnail.png", &png_bytes(&thumb)?, stored)?;
        z.finish().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    Ok((w, h))
}

const GUIDE_OPACITY: f32 = 0.6;

/// Draw `src` over `dst` at (x, y) with `opacity` (straight alpha, source-over).
fn over(dst: &mut RgbaImage, src: &RgbaImage, x: i64, y: i64, opacity: f32) {
    let o = opacity.clamp(0.0, 1.0);
    for (sx, sy, p) in src.enumerate_pixels() {
        let (dx, dy) = (x + sx as i64, y + sy as i64);
        if dx < 0 || dy < 0 || dx >= dst.width() as i64 || dy >= dst.height() as i64 {
            continue;
        }
        let sa = p[3] as f32 / 255.0 * o;
        if sa <= 0.0 {
            continue;
        }
        let d = dst.get_pixel_mut(dx as u32, dy as u32);
        let da = d[3] as f32 / 255.0;
        let oa = sa + da * (1.0 - sa);
        for c in 0..3 {
            let v = (p[c] as f32 * sa + d[c] as f32 * da * (1.0 - sa)) / oa.max(1e-6);
            d[c] = v.round().clamp(0.0, 255.0) as u8;
        }
        d[3] = (oa * 255.0).round() as u8;
    }
}

/// One `<layer>` of a stack.xml (nested stacks are flattened into document order).
#[derive(Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub src: String,
    pub x: i64,
    pub y: i64,
    pub opacity: f32,
    pub visible: bool,
}

fn attr(tag: &str, key: &str) -> Option<String> {
    let at = tag.find(&format!(" {key}="))? + key.len() + 2;
    let q = tag[at..].chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let rest = &tag[at + 1..];
    let end = rest.find(q)?;
    Some(rest[..end].replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
}

/// The layers of a stack.xml, top first, as OpenRaster lists them. A hidden `<stack>` hides what's inside it.
pub fn parse_stack(xml: &str) -> Vec<Layer> {
    let mut out = Vec::new();
    let mut hidden_depth: Vec<bool> = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        let Some(j) = rest.find('>') else { break };
        let tag = &rest[..=j];
        rest = &rest[j + 1..];
        if tag.starts_with("<stack") {
            let hidden = attr(tag, "visibility").as_deref() == Some("hidden");
            if !tag.ends_with("/>") {
                hidden_depth.push(hidden);
            }
        } else if tag.starts_with("</stack") {
            hidden_depth.pop();
        } else if tag.starts_with("<layer") {
            let inside_hidden = hidden_depth.iter().any(|h| *h);
            out.push(Layer {
                name: attr(tag, "name").unwrap_or_default(),
                src: attr(tag, "src").unwrap_or_default(),
                x: attr(tag, "x").and_then(|v| v.parse().ok()).unwrap_or(0),
                y: attr(tag, "y").and_then(|v| v.parse().ok()).unwrap_or(0),
                opacity: attr(tag, "opacity").and_then(|v| v.parse().ok()).unwrap_or(1.0),
                visible: !inside_hidden && attr(tag, "visibility").as_deref() != Some("hidden"),
            });
        }
    }
    out
}

/// The picture to bring back: the visible layers of `<path>`, bottom to top, without the guide, as a PNG.
pub fn read_ora(path: &Path) -> Result<Vec<u8>, String> {
    refuse_link(path)?;
    let f = std::fs::File::open(path).map_err(|e| format!("can't open {}: {e}", path.display()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("{} isn't an OpenRaster file: {e}", path.display()))?;
    let mut read = |name: &str, limit: u64| -> Result<Vec<u8>, String> {
        let entry = z.by_name(name).map_err(|_| format!("the file has no {name}"))?;
        let mut v = Vec::new();
        entry.take(limit).read_to_end(&mut v).map_err(|e| e.to_string())?;
        Ok(v)
    };
    let xml = String::from_utf8(read("stack.xml", 4 << 20)?).map_err(|_| "stack.xml isn't text".to_string())?;
    let (w, h) = {
        let tag = &xml[xml.find("<image").ok_or("stack.xml has no <image>")?..];
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let w: u32 = attr(tag, "w").and_then(|v| v.parse().ok()).ok_or("stack.xml has no width")?;
        let h: u32 = attr(tag, "h").and_then(|v| v.parse().ok()).ok_or("stack.xml has no height")?;
        (w, h)
    };
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(format!("the picture is {w}×{h}; it must be 1 to {MAX_SIDE} pixels a side"));
    }
    let layers = parse_stack(&xml);
    if layers.len() > MAX_LAYERS {
        return Err(format!("the file has {} layers; at most {MAX_LAYERS} are read", layers.len()));
    }
    let mut canvas = RgbaImage::new(w, h);
    let mut pixels = 0u64;
    // OpenRaster lists the top layer first: paint from the bottom up
    for l in layers.into_iter().rev() {
        if !l.visible || l.name.starts_with(GUIDE) || l.src.is_empty() || l.src.contains("..") {
            continue;
        }
        let bytes = read(&l.src, 512 << 20)?;
        // the size is read from the header first, so a huge layer is refused before it is decoded
        let (lw, lh) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .into_dimensions()
            .map_err(|e| format!("unreadable picture: {e}"))?;
        pixels += lw as u64 * lh as u64;
        if lw > MAX_SIDE || lh > MAX_SIDE || pixels > MAX_TOTAL_PIXELS {
            return Err("the layers hold too many pixels to read back".into());
        }
        let img = load(&bytes)?;
        over(&mut canvas, &img, l.x, l.y, l.opacity);
    }
    png_bytes(&canvas)
}

/// Seconds since the epoch the file was last written (0 when it isn't there).
pub fn modified(path: &Path) -> u64 {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Open `path` in PhotoCraft. A fixed program, the file as its only argument.
#[cfg(desktop)]
pub fn launch(path: &Path) -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    {
        let ok = Command::new("open").args(["-a", "PhotoCraft"]).arg(path).status().map(|s| s.success()).unwrap_or(false);
        return if ok { Ok(()) } else { Err(NOT_FOUND.into()) };
    }
    #[cfg(target_os = "windows")]
    {
        let mut places = Vec::new();
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            places.push(PathBuf::from(l).join(r"Programs\ArtCraft\PhotoCraft\photocraft.exe"));
        }
        for pf in ["ProgramFiles", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(pf) {
                places.push(PathBuf::from(p).join(r"ArtCraft\PhotoCraft\photocraft.exe"));
            }
        }
        let exe = places.into_iter().find(|p| p.is_file()).ok_or(NOT_FOUND)?;
        return Command::new(exe).arg(path).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let exe = ["/usr/bin/photocraft", "/usr/local/bin/photocraft", "/opt/photocraft/photocraft"].iter().map(PathBuf::from).find(|p| p.is_file()).ok_or(NOT_FOUND)?;
        return Command::new(exe).arg(path).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
}

#[cfg(desktop)]
const NOT_FOUND: &str = "PhotoCraft isn't installed (get it from ArtCraft Launcher)";

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        png_bytes(&RgbaImage::from_pixel(w, h, image::Rgba(c))).unwrap()
    }

    #[test]
    fn names_stay_inside_the_folder() {
        assert_eq!(file_stem("Oak floor"), "Oak_floor");
        assert_eq!(file_stem("../../etc/passwd"), "etc_passwd");
        assert_eq!(file_stem("..."), "texture");
        assert_eq!(file_stem(""), "texture");
        let p = ora_path(Path::new("/m/Textures"), "a/b\\c");
        assert_eq!(p, PathBuf::from("/m/Textures/a_b_c.ora"));
        assert!(file_stem(&"x".repeat(500)).len() <= 80);
    }

    #[test]
    fn writes_openraster_that_reads_back_without_the_guide() {
        let dir = std::env::temp_dir().join(format!("sc-ora-{}", std::process::id()));
        let path = dir.join("Wall.ora");
        let red = solid(8, 4, [200, 10, 10, 255]);
        let guide = solid(16, 8, [0, 255, 255, 255]); // a different size: scaled to the texture
        assert_eq!(write_ora(&path, &red, Some(&guide)).unwrap(), (8, 4));
        // the file is OpenRaster: mimetype first and stored
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        {
            let first = z.by_index(0).unwrap();
            assert_eq!(first.name().unwrap(), "mimetype");
            assert_eq!(first.compression(), zip::CompressionMethod::Stored);
        }
        let mut xml = String::new();
        z.by_name("stack.xml").unwrap().read_to_string(&mut xml).unwrap();
        let layers = parse_stack(&xml);
        assert_eq!(layers.len(), 2);
        assert!(layers[0].name.starts_with(GUIDE)); // the guide is on top
        // coming back, the guide is left out: pure red again
        let back = image::load_from_memory(&read_ora(&path).unwrap()).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (8, 4));
        assert!(back.pixels().all(|p| p.0 == [200, 10, 10, 255]));
        std::fs::remove_dir_all(&dir).ok();
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    #[cfg(unix)]
    fn a_linked_part_file_is_not_written_through() {
        let dir = scratch("link1");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "keep me").unwrap();
        let tex = dir.join("Textures");
        std::fs::create_dir_all(&tex).unwrap();
        std::os::unix::fs::symlink(&victim, tex.join("Wall.ora.part")).unwrap();
        write_ora(&tex.join("Wall.ora"), &solid(4, 4, [1, 2, 3, 255]), None).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me");
        assert!(read_ora(&tex.join("Wall.ora")).is_ok());
        // the hash file and a linked target are refused too
        std::os::unix::fs::symlink(&victim, tex.join("Wall.ora.sketchcraft")).unwrap();
        assert!(write_file_safely(&tex.join("Wall.ora.sketchcraft"), b"x").is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me");
        std::os::unix::fs::symlink(&victim, tex.join("L.ora")).unwrap();
        assert!(write_ora(&tex.join("L.ora"), &solid(4, 4, [1, 2, 3, 255]), None).is_err());
        assert!(keep_backup(&tex.join("L.ora")).is_err());
        assert!(read_ora(&tex.join("L.ora")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[cfg(unix)]
    fn a_linked_textures_folder_is_refused() {
        let dir = scratch("link2");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let tex = dir.join("Textures");
        std::os::unix::fs::symlink(&elsewhere, &tex).unwrap();
        assert!(write_ora(&tex.join("Wall.ora"), &solid(4, 4, [1, 2, 3, 255]), None).is_err());
        assert!(ensure_dir(&tex).is_err());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn too_many_layers_are_refused() {
        let dir = scratch("many");
        let path = dir.join("many.ora");
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        let mut stack = String::from(r#"<image w="2" h="2"><stack>"#);
        for _ in 0..(MAX_LAYERS + 1) {
            stack += r#"<layer name="l" src="data/a.png"/>"#;
        }
        stack += "</stack></image>";
        z.start_file("stack.xml", o).unwrap();
        z.write_all(stack.as_bytes()).unwrap();
        z.start_file("data/a.png", o).unwrap();
        z.write_all(&solid(2, 2, [9, 9, 9, 255])).unwrap();
        z.finish().unwrap();
        let err = read_ora(&path).unwrap_err();
        assert!(err.contains("layers"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn flattens_layers_with_offsets_opacity_and_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("sc-ora2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.ora");
        let f = std::fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        let mut put = |n: &str, b: &[u8]| {
            z.start_file(n, o).unwrap();
            z.write_all(b).unwrap();
        };
        put("mimetype", b"image/openraster");
        put(
            "stack.xml",
            br#"<?xml version="1.0"?><image w="4" h="2"><stack>
              <layer name="hidden" src="data/h.png" visibility="hidden"/>
              <stack visibility="hidden"><layer name="in hidden group" src="data/h.png"/></stack>
              <layer name="half" src="data/w.png" x="2" y="0" opacity="0.5"/>
              <layer name="base" src="data/b.png"/>
            </stack></image>"#,
        );
        put("data/h.png", &solid(4, 2, [0, 255, 0, 255]));
        put("data/w.png", &solid(2, 2, [255, 255, 255, 255]));
        put("data/b.png", &solid(4, 2, [0, 0, 0, 255]));
        z.finish().unwrap();
        let img = image::load_from_memory(&read_ora(&path).unwrap()).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [0, 0, 0, 255]); // base only
        assert_eq!(img.get_pixel(3, 1).0, [128, 128, 128, 255]); // half white over black; no green anywhere
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_what_isnt_a_picture_or_a_data_url() {
        assert!(decode_data_url("https://example.com/x.png").is_err());
        assert!(decode_data_url("data:image/png,abc").is_err());
        assert!(write_ora(Path::new("/tmp/never.ora"), b"not a png", None).is_err());
    }

    #[test]
    fn hash_is_stable_and_tagged() {
        // SHA-256 of "abc" (FIPS 180-2 test vector)
        assert_eq!(hash_of(b"abc"), "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hash_of(b"abc"), hash_of(b"abc"));
        assert_ne!(hash_of(b"abc"), hash_of(b"abd"));
        assert!(is_tagged(&hash_of(b"")));
    }

    #[test]
    fn tagged_mismatch_is_a_real_change() {
        let stored = hash_of(b"one");
        assert!(stored_hash_matches(Some(&stored), b"one"));
        assert!(stored_hash_matches(Some(&format!("{stored}\n")), b"one"));
        assert!(!stored_hash_matches(Some(&stored), b"two"));
        assert!(!stored_hash_matches(None, b"one"));
        assert!(!stored_hash_matches(Some("sha512:abcd"), b"one"));
    }

    #[test]
    fn legacy_untagged_hash_keeps_layers_and_is_upgraded() {
        let legacy_ok = legacy_hash_of(b"pic");
        assert!(stored_hash_matches(Some(&legacy_ok), b"pic"));
        // an old value made by another Rust release can't be judged: still unchanged
        assert!(stored_hash_matches(Some("0123456789abcdef"), b"pic"));
        // with files: the .ora stays (no .bak) and the hash file is rewritten tagged
        let dir = std::env::temp_dir().join(format!("sc-hash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ora = dir.join("Wall.ora");
        std::fs::write(&ora, b"layers").unwrap();
        let hf = ora.with_extension("ora.sketchcraft");
        std::fs::write(&hf, &legacy_ok).unwrap();
        let stored = std::fs::read_to_string(&hf).ok();
        assert!(ora.is_file() && stored_hash_matches(stored.as_deref(), b"pic"));
        assert!(!is_tagged(stored.as_deref().unwrap()));
        write_file_safely(&hf, hash_of(b"pic").as_bytes()).unwrap();
        assert!(is_tagged(&std::fs::read_to_string(&hf).unwrap()));
        assert!(ora.is_file() && !ora.with_extension("ora.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hash_file_read_is_bounded_and_refuses_links() {
        let dir = std::env::temp_dir().join(format!("sc-hashread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("a.ora.sketchcraft");
        std::fs::write(&good, hash_of(b"x")).unwrap();
        assert_eq!(read_hash_file(&good).unwrap().as_deref(), Some(hash_of(b"x").as_str()));
        assert_eq!(read_hash_file(&dir.join("none")).unwrap(), None);
        // too big to be a hash file: a mismatch, not an error and not read whole
        let big = dir.join("b.ora.sketchcraft");
        std::fs::write(&big, vec![b'a'; 100_000]).unwrap();
        assert_eq!(read_hash_file(&big).unwrap(), None);
        #[cfg(unix)]
        {
            let start = std::time::Instant::now();
            let link = dir.join("c.ora.sketchcraft");
            std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
            assert!(read_hash_file(&link).is_err());
            assert!(start.elapsed().as_secs() < 2);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
