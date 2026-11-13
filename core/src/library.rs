//! The component library: a folder you own (~/Documents/SketchCraft/Library
//! by default). Every SketchUp, FBX, OBJ or SketchCraft file in it can be
//! placed as a component; subfolders are collections. Components from a
//! model can be saved into it, and every entry gets a rendered thumbnail.

use crate::model::{Definition, Id, Instance, Model};
use crate::render::{self, RenderOpts};
use anyhow::{bail, Context, Result};
use glam::{DMat4, DVec3};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

pub const EXTS: [&str; 4] = ["sketchcraft", "skp", "fbx", "obj"];

pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("SketchCraft").join("Library")
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub name: String,
    /// the subfolder it's in ("" at the top)
    pub collection: String,
}

/// A readable name from a file name ("Sink+Base+Cabinet_261102" -> "Sink Base Cabinet").
pub fn nice_name(stem: &str) -> String {
    let s = stem.replace(['+', '_'], " ");
    let words: Vec<&str> = s.split_whitespace().collect();
    let start = words.iter().position(|w| !matches!(w.to_ascii_lowercase().as_str(), "uploads" | "files" | "upload" | "file") && !w.chars().all(|c| c.is_ascii_digit())).unwrap_or(0);
    // drop a trailing number tag (a download id or date)
    let mut end = words.len();
    while end > start + 1 && words[end - 1].chars().all(|c| c.is_ascii_digit()) && words[end - 1].len() >= 4 {
        end -= 1;
    }
    let name = words[start..end].join(" ");
    if name.is_empty() { stem.to_string() } else { name }
}

/// Everything placeable in the library folder (two levels of collections).
pub fn list(dir: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    fn walk(base: &Path, d: &Path, depth: usize, out: &mut Vec<Entry>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut items: Vec<_> = rd.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let p = e.path();
            let fname = e.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                if depth < 2 {
                    walk(base, &p, depth + 1, out);
                }
            } else if p.extension().and_then(|x| x.to_str()).map_or(false, |x| EXTS.contains(&x.to_ascii_lowercase().as_str())) {
                let collection = p.parent().and_then(|par| par.strip_prefix(base).ok()).map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
                out.push(Entry { path: p.to_string_lossy().to_string(), name: nice_name(&p.file_stem().unwrap_or_default().to_string_lossy()), collection });
            }
        }
    }
    walk(dir, dir, 0, &mut out);
    out
}

/// A file's contents as a model (imports land in it as they would in a document).
pub fn load_file(path: &str) -> Result<Model> {
    let p = Path::new(path);
    let ext = p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
    let name = nice_name(&p.file_stem().unwrap_or_default().to_string_lossy());
    let mut m = Model::default();
    match ext.as_str() {
        "sketchcraft" => m = serde_json::from_str(&std::fs::read_to_string(p)?).with_context(|| format!("{name} isn't a SketchCraft file"))?,
        "skp" => {
            crate::skp::import(&mut m, &std::fs::read(p)?, DVec3::ZERO, &name)?;
        }
        "fbx" => {
            crate::fbx::import(&mut m, &std::fs::read(p)?, &name, DVec3::ZERO)?;
        }
        "obj" => {
            crate::import::import_obj(&mut m, &std::fs::read_to_string(p)?, p.parent(), &name, true, 1.0, DVec3::ZERO)?;
        }
        _ => bail!("can't place .{ext} files (use {})", EXTS.join(", ")),
    }
    m.views.clear();
    m.guides.clear();
    m.active_group = None;
    Ok(m)
}

/// Move a loaded model into `root` as one component; its own components come
/// along under new ids. Returns the new component.
pub fn adopt(root: &mut Model, mut src: Model, name: &str, source: Option<String>) -> Id {
    let mut map: HashMap<Id, Id> = HashMap::new();
    let olds: Vec<Id> = src.definitions.keys().copied().collect();
    for old in &olds {
        map.insert(*old, root.new_id());
    }
    let remap = |m: &mut Model, map: &HashMap<Id, Id>| {
        for i in m.instances.values_mut() {
            if let Some(n) = map.get(&i.def) {
                i.def = *n;
            }
        }
    };
    for old in olds {
        let mut d = src.definitions.remove(&old).unwrap();
        remap(std::sync::Arc::make_mut(&mut d.model), &map);
        for (k, mat) in &d.model.materials {
            root.materials.entry(k.clone()).or_insert(mat.clone());
        }
        root.definitions.insert(map[&old], d);
    }
    remap(&mut src, &map);
    for (k, mat) in &src.materials {
        root.materials.entry(k.clone()).or_insert(mat.clone());
    }
    let id = root.new_id();
    root.definitions.insert(id, Definition { name: name.to_string(), model: std::sync::Arc::new(src), attrs: Default::default(), source });
    id
}

/// The component for a library file in `root`, loading it on first use.
pub fn definition_for(root: &mut Model, path: &str) -> Result<Id> {
    let canon = std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string());
    if let Some((&id, _)) = root.definitions.iter().find(|(_, d)| d.source.as_deref() == Some(canon.as_str())) {
        return Ok(id);
    }
    let src = load_file(path)?;
    let name = nice_name(&Path::new(path).file_stem().unwrap_or_default().to_string_lossy());
    Ok(adopt(root, src, &name, Some(canon)))
}

/// Components a component uses inside it (all the way down).
fn used_by(root: &Model, def: Id, out: &mut BTreeSet<Id>) {
    if let Some(d) = root.definitions.get(&def) {
        for i in d.model.instances.values() {
            if out.insert(i.def) {
                used_by(root, i.def, out);
            }
        }
    }
}

/// A component as a standalone .sketchcraft model (its parts included).
pub fn standalone(root: &Model, def: Id) -> Result<Model> {
    let d = root.definitions.get(&def).context("no such component")?;
    let mut m = (*d.model).clone();
    let mut need = BTreeSet::new();
    used_by(root, def, &mut need);
    for id in need {
        if let Some(x) = root.definitions.get(&id) {
            m.definitions.insert(id, x.clone());
        }
    }
    // keep ids clear of the parts' ids
    m.next_id = m.next_id.max(root.next_id);
    Ok(m)
}

/// Remove components nothing places (anywhere). Returns how many.
pub fn purge(root: &mut Model) -> usize {
    let mut keep = BTreeSet::new();
    for i in root.instances.values() {
        if keep.insert(i.def) {
            used_by(root, i.def, &mut keep);
        }
    }
    let before = root.definitions.len();
    root.definitions.retain(|id, _| keep.contains(id));
    before - root.definitions.len()
}

/// A small picture of a component (PNG bytes).
pub fn thumbnail_of(root: &Model, def: Id, px: u32) -> Result<Vec<u8>> {
    if !root.definitions.contains_key(&def) {
        bail!("no component {def}");
    }
    let mut scene = Model { definitions: root.definitions.clone(), materials: root.materials.clone(), ..Default::default() };
    scene.instances.insert(u32::MAX - 1, Instance { parent: None, def, name: String::new(), transform: DMat4::IDENTITY, tag: None, hidden: false, attrs: Default::default() });
    let opts = RenderOpts { width: px, height: px, paper: true, grid: false, annotations: false, view: "iso".into(), ..Default::default() };
    let img = render::render(&scene, &opts);
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Thumbnail of a library file, cached next to the library (by path and date).
pub fn thumbnail_of_file(path: &str, px: u32) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    let stamp = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let key: String = format!("{path}-{stamp}-{px}").chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let cache = std::env::temp_dir().join("sketchcraft-thumbs");
    std::fs::create_dir_all(&cache).ok();
    let file = cache.join(format!("{}.png", &key[key.len().saturating_sub(120)..]));
    if let Ok(b) = std::fs::read(&file) {
        return Ok(b);
    }
    let mut scratch = Model::default();
    let id = definition_for(&mut scratch, path)?;
    let png = thumbnail_of(&scratch, id, px)?;
    std::fs::write(&file, &png).ok();
    Ok(png)
}

/// Copy files into the library (a collection subfolder if given). OBJ files
/// bring their .mtl along.
pub fn add_files(dir: &Path, paths: &[String], collection: &str) -> Result<Vec<String>> {
    let to = if collection.trim().is_empty() { dir.to_path_buf() } else { dir.join(safe(collection)) };
    std::fs::create_dir_all(&to)?;
    let mut added = Vec::new();
    for p in paths {
        let src = Path::new(p);
        let ext = src.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
        if !EXTS.contains(&ext.as_str()) {
            bail!("{} isn't a model file ({})", src.display(), EXTS.join(", "));
        }
        let dest = to.join(src.file_name().context("no file name")?);
        std::fs::copy(src, &dest)?;
        if ext == "obj" {
            if let Ok(text) = std::fs::read_to_string(src) {
                for line in text.lines().filter(|l| l.starts_with("mtllib")) {
                    let mtl = line[6..].trim();
                    if let Some(parent) = src.parent() {
                        std::fs::copy(parent.join(mtl), to.join(mtl)).ok();
                    }
                }
            }
        }
        added.push(dest.to_string_lossy().to_string());
    }
    Ok(added)
}

/// A file or folder name made safe ("My: lamps/v2" -> "My lamps v2").
pub fn safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { ' ' }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() || t.starts_with('.') { "Untitled".into() } else { t }
}
