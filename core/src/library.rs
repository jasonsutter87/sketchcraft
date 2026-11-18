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
    // beside the live document, so one folder (or SKETCHCRAFT_DOC) scopes everything
    crate::live::default_doc().parent().map(|p| p.join("Library")).unwrap_or_else(|| PathBuf::from("Library"))
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

/// The most a library file, an inserted file or a thumbnail's source may be.
pub const MAX_LIBRARY_BYTES: u64 = 128 << 20;

/// A file's contents as a model (imports land in it as they would in a document).
pub fn load_file(path: &str) -> Result<Model> {
    let p = Path::new(path);
    // a part, a library file or a thumbnail is read whole into memory by whoever opens the library panel: a smaller limit than a model
    let len = std::fs::metadata(p)?.len();
    if len > MAX_LIBRARY_BYTES {
        bail!("{} is {} MB; the library reads parts up to {} MB (open it as a model instead)", p.display(), len >> 20, MAX_LIBRARY_BYTES >> 20);
    }
    let ext = p.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
    let name = nice_name(&p.file_stem().unwrap_or_default().to_string_lossy());
    let mut m = Model::default();
    match ext.as_str() {
        "sketchcraft" => {
            let text = std::fs::read_to_string(p)?;
            // a part from a newer SketchCraft would come into the model without what this version doesn't know: not placed
            if let Some(ro) = crate::format::check(text.as_bytes()) {
                bail!("{name} was saved by a newer SketchCraft ({}), so it can't be placed here: update SketchCraft to use it", ro.reasons.join("; "));
            }
            m = serde_json::from_str(&text).with_context(|| format!("{name} isn't a SketchCraft file"))?;
            // a file from outside: the same checks as opening it (consistent, ids above every id in it)
            if let Some(what) = m.broken_refs(false).first() {
                bail!("{name} isn't a consistent model ({what} refers to something that isn't there)");
            }
            m.repair_ids();
            m.vet_pictures();
            m.check_sane_with(false)?;
        }
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
    // a part that contains itself, or multiplies into millions of faces, never gets into a model
    m.check_nesting(crate::nesting::MAX_EXPANDED).map_err(|why| anyhow::anyhow!("{name} can't be used: {why}"))?;
    m.views.clear();
    m.guides.clear();
    // drawing sheets belong to a whole model: placed as a component they could never be seen or changed
    m.clear_paper();
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
    root.definitions.insert(id, Definition { name: name.to_string(), model: std::sync::Arc::new(src), attrs: Default::default(), source, cuts: false, face_me: false });
    id
}

/// What identifies a library file's contents inside a model: its file name and a hash of its bytes.
/// Never a folder path (a model gets shared; the path would carry the person's folder names).
pub fn source_key(path: &str) -> Result<String> {
    let p = Path::new(path);
    let len = std::fs::metadata(p)?.len();
    if len > MAX_LIBRARY_BYTES {
        bail!("{} is {} MB; the library reads parts up to {} MB (open it as a model instead)", p.display(), len >> 20, MAX_LIBRARY_BYTES >> 20);
    }
    let hash = crate::safety::sha256_hex(&std::fs::read(p)?);
    let file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok(format!("{file}#{}", &hash[..32]))
}

/// The component for a library file in `root`, loading it on first use. A file is reused by its
/// contents (name and hash), so another file in another folder with the same name, or the same
/// file after it changed, gets its own component.
pub fn definition_for(root: &mut Model, path: &str) -> Result<Id> {
    let key = source_key(path)?;
    if let Some((&id, _)) = root.definitions.iter().find(|(_, d)| d.source.as_deref() == Some(key.as_str())) {
        return Ok(id);
    }
    let src = load_file(path)?;
    let name = nice_name(&Path::new(path).file_stem().unwrap_or_default().to_string_lossy());
    Ok(adopt(root, src, &name, Some(key)))
}

/// Components a component uses inside it (all the way down). Iterative: a chain of any length (or a loop) is safe.
fn used_by(root: &Model, def: Id, out: &mut BTreeSet<Id>) {
    out.extend(root.uses_of(def));
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
    let cache = crate::safety::cache_dir("thumbs");
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

/// A name compared the way a file system might: Unicode case folded (`ſ` is `s`, the Kelvin sign is `k`),
/// full-width Latin letters flattened, trailing dots and spaces dropped (Windows ignores them).
pub fn fold(name: &str) -> String {
    name.trim()
        .trim_end_matches(['.', ' '])
        .chars()
        .map(|c| match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(|c| c.to_uppercase().flat_map(|u| u.to_lowercase()).collect::<Vec<_>>())
        .collect()
}

/// Windows device names (a file called `NUL` or `COM1.obj` is the device).
fn is_device(folded: &str) -> bool {
    let stem = folded.split('.').next().unwrap_or("").trim_end();
    matches!(stem, "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
        || ((stem.starts_with("com") || stem.starts_with("lpt")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0')
}

/// Folder names the library never writes into: the model's own `Backups` and `Plugins` (a
/// file there is a backup or code the app would run), hidden names, Windows short names
/// (`BACKUP~1`) and device names. Compared folded, not by spelling.
pub fn reserved(name: &str) -> bool {
    let n = name.trim();
    let f = fold(n);
    n.starts_with('.') || f == "backups" || f == "plugins" || f.starts_with("backups.") || f.starts_with("plugins.") || is_device(&f) || f.as_bytes().windows(2).any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

/// Are these two paths the same folder on disk (same device and file id, or the same real path)?
/// Spelling does not matter: a case-insensitive volume, a short name or a link all compare equal.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        }
    }
}

/// Is `folder` (a library folder or collection that exists) the model's `Backups` or `Plugins` folder,
/// or inside one, by identity? Checked against the ones beside `base`, beside its parent and beside the
/// live document.
pub fn is_protected(folder: &Path, base: &Path) -> bool {
    let mut homes: Vec<PathBuf> = vec![base.to_path_buf()];
    if let Some(p) = base.parent() {
        homes.push(p.to_path_buf());
    }
    if let Some(p) = crate::live::default_doc().parent() {
        homes.push(p.to_path_buf());
    }
    let real = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    for h in homes {
        for name in ["Backups", "Plugins"] {
            let r = h.join(name);
            if !r.exists() {
                continue;
            }
            if same_folder(folder, &r) || real.ancestors().any(|a| same_folder(a, &r)) {
                return true;
            }
        }
    }
    false
}

/// A collection or file name an agent may give: one plain name, no separators, drive letters
/// or traversal, not hidden, and not a reserved folder.
pub fn plain_name(s: &str) -> Result<()> {
    let t = s.trim();
    if t.is_empty() || t.contains(['/', '\\', ':', '\0']) || t.starts_with('.') || t.contains("..") || t.chars().any(char::is_control) {
        bail!("{s:?} isn't a plain name (no folders, dots at the start or control characters)");
    }
    if reserved(&safe(t)) || reserved(t) {
        bail!("{s:?} is a reserved folder name (Backups and Plugins are not library collections)");
    }
    Ok(())
}

/// A collection name an agent may give: letters, digits, spaces, `_` and `-` only (ASCII), so no
/// look-alike spelling of a reserved folder can be made.
pub fn agent_collection(s: &str) -> Result<()> {
    plain_name(s)?;
    if !s.trim().is_empty() && !s.trim().chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-') {
        bail!("{s:?}: an agent's collection names use only letters, digits, spaces, - and _");
    }
    Ok(())
}

/// Where `add_files` would put each file: (source, destination) pairs, including the `.mtl`
/// beside an OBJ. The destination folder is the library `dir`, or a collection inside it.
pub fn plan_add(dir: &Path, paths: &[String], collection: &str) -> Result<(PathBuf, Vec<(PathBuf, PathBuf)>)> {
    let coll = safe(collection);
    if !collection.trim().is_empty() && reserved(&coll) {
        bail!("{collection:?} is a reserved folder name (Backups and Plugins are not library collections)");
    }
    let to = if collection.trim().is_empty() { dir.to_path_buf() } else { dir.join(coll) };
    let mut plan = Vec::new();
    for p in paths {
        let src = Path::new(p);
        let ext = src.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).unwrap_or_default();
        if !EXTS.contains(&ext.as_str()) {
            bail!("{} isn't a model file ({})", src.display(), EXTS.join(", "));
        }
        let name = src.file_name().context("no file name")?;
        plan.push((src.to_path_buf(), to.join(name)));
        if ext == "obj" {
            if let Ok(text) = std::fs::read_to_string(src) {
                for line in text.lines().filter(|l| l.starts_with("mtllib")) {
                    // the material file sits beside the OBJ: a bare file name, never a path out of its folder
                    let mtl = line[6..].trim();
                    let plain = !mtl.is_empty() && Path::new(mtl).file_name().is_some_and(|n| n == std::ffi::OsStr::new(mtl)) && !mtl.contains(['/', '\\', ':']);
                    if let (true, Some(parent)) = (plain && mtl.to_ascii_lowercase().ends_with(".mtl"), src.parent()) {
                        plan.push((parent.join(mtl), to.join(mtl)));
                    }
                }
            }
        }
    }
    Ok((to, plan))
}

/// Copy files into the library (a collection subfolder if given). OBJ files
/// bring their .mtl along. With `replace` false a file that is already there is
/// an error (nothing is overwritten).
pub fn add_files(dir: &Path, paths: &[String], collection: &str, replace: bool) -> Result<Vec<String>> {
    let (to, plan) = plan_add(dir, paths, collection)?;
    std::fs::create_dir_all(&to)?;
    if is_protected(&to, dir) {
        bail!("that folder is the model's Backups or Plugins folder; the library never writes there");
    }
    let to_real = std::fs::canonicalize(&to)?;
    if !replace {
        for (src, dest) in &plan {
            if dest.exists() && std::fs::canonicalize(src).ok() != std::fs::canonicalize(dest).ok() {
                bail!("{} is already in the library, so nothing was replaced", dest.display());
            }
        }
    }
    let mut added = Vec::new();
    for (src, dest) in &plan {
        let is_model = paths.iter().any(|p| Path::new(p) == src.as_path());
        let r = copy_into(src, dest, &to_real);
        if is_model {
            r?;
            added.push(dest.to_string_lossy().to_string());
        }
    }
    Ok(added)
}

/// Copy `src` to `dest` (inside `to_real`, the folder it must stay in); never onto itself (a copy
/// of a file onto itself empties it).
fn copy_into(src: &Path, dest: &Path, to_real: &Path) -> Result<()> {
    let parent = dest.parent().context("no folder")?;
    if std::fs::canonicalize(parent)? != to_real {
        bail!("{} isn't inside the library folder", dest.display());
    }
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(src), std::fs::canonicalize(dest)) {
        if a == b {
            return Ok(());
        }
    }
    // a symlink at the destination would send the copy elsewhere
    if std::fs::symlink_metadata(dest).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("{} is a link", dest.display());
    }
    std::fs::copy(src, dest)?;
    Ok(())
}

/// A file or folder name made safe ("My: lamps/v2" -> "My lamps v2").
pub fn safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { ' ' }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    // never a name that could be taken for a file SketchCraft sets aside beside a model (`x.unsaved-<time>`, `x.damaged-<time>`)
    let t = t.replace(".unsaved-", "_unsaved-").replace(".damaged-", "_damaged-");
    if t.is_empty() || t.starts_with('.') { "Untitled".into() } else { t }
}
