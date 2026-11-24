//! Plugins: small scripts that add commands to SketchCraft. Each is a folder
//! with `plugin.json` (its name and commands) and `main.js`, run by the app in
//! a sandbox (a Web Worker with no page, network or file access) that can
//! only call engine commands. Built-in plugins ship inside the app; others
//! live in ~/Documents/SketchCraft/Plugins. Also here: `parts`, the sizes a
//! cut list needs.

use crate::model::{Id, Model};
use glam::DVec3;
use serde_json::{json, Value};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const BUILTIN: &[(&str, &str, &str)] = &[
    ("cutlist", include_str!("../plugins/cutlist/plugin.json"), include_str!("../plugins/cutlist/main.js")),
    ("shelves", include_str!("../plugins/shelves/plugin.json"), include_str!("../plugins/shelves/main.js")),
    ("framing", include_str!("../plugins/framing/plugin.json"), include_str!("../plugins/framing/main.js")),
];

/// Where people put their own plugins.
pub fn dir() -> PathBuf {
    crate::live::default_doc().parent().map(|p| p.join("Plugins")).unwrap_or_else(|| PathBuf::from("Plugins"))
}

fn entry(id: &str, manifest: &str, code: String, builtin: bool, folder: Option<&Path>) -> Option<Value> {
    let mut m: Value = serde_json::from_str(manifest).ok()?;
    let o = m.as_object_mut()?;
    o.insert("id".into(), json!(id));
    o.insert("code".into(), json!(code));
    o.insert("builtin".into(), json!(builtin));
    o.insert("enabled".into(), json!(!disabled().contains(id)));
    if let Some(f) = folder {
        o.insert("folder".into(), json!(f));
    }
    // command icons, sent as data URLs: inline SVG ("<svg ...>"), or a small
    // SVG or PNG file beside main.js
    if let Some(cmds) = o.get_mut("commands").and_then(|c| c.as_array_mut()) {
        for c in cmds {
            let Some(icon) = c.get("icon").and_then(|i| i.as_str()).map(str::to_string) else { continue };
            let data = if icon.trim_start().starts_with("<svg") && icon.len() <= 64 * 1024 {
                use base64::Engine as _;
                Some(format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(icon.trim())))
            } else {
                folder.and_then(|f| icon_data(f, &icon))
            };
            c.as_object_mut()?.insert("icon".into(), data.map(Value::from).unwrap_or(Value::Null));
        }
    }
    Some(m)
}

/// Whether a plugin.json says the plugin is a SketchUp Ruby extension. The one test: `list()` uses it
/// to decide how a plugin runs, and `install()` and its preview use it to say what the person is
/// installing, so what is shown is what runs (SPECTER-002).
pub fn manifest_is_ruby(manifest: &Value) -> bool {
    manifest["ruby"] == true
}

/// The menu items and commands a Ruby extension adds, read from its source (`add_item("...")`,
/// `UI::Command.new("...")`). A preview can't run Ruby, so this is a reading of the code, not a promise.
pub fn ruby_menu_labels(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (n, b) in files {
        if !n.to_ascii_lowercase().ends_with(".rb") {
            continue;
        }
        let text = String::from_utf8_lossy(b);
        for marker in ["add_item", "UI::Command.new"] {
            let mut rest: &str = &text;
            while let Some(i) = rest.find(marker) {
                rest = &rest[i + marker.len()..];
                let t = rest.trim_start_matches(|c: char| c == '(' || c == ' ' || c == '\t');
                let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
                if let Some(end) = t[1..].find(q) {
                    let label: String = t[1..1 + end].chars().filter(|c| !c.is_control()).take(80).collect();
                    if !label.trim().is_empty() && !out.contains(&label) && out.len() < 50 {
                        out.push(label);
                    }
                }
            }
        }
    }
    out
}

/// Every .rb file in a Ruby plugin's folder, by path inside it (2 MB in all).
fn ruby_files(folder: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut total = 0usize;
    let mut stack = vec![folder.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("rb")) {
                let text = std::fs::read_to_string(&p)?;
                total += text.len();
                if total > 2 << 20 || out.len() >= 200 {
                    bail!("its Ruby files are over 2 MB");
                }
                let rel = p.strip_prefix(folder).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.insert(rel, text);
            }
        }
    }
    Ok(out)
}

/// A plugin's icon file as a data URL (SVG or PNG, up to 256 KB, inside its folder).
fn icon_data(folder: &Path, name: &str) -> Option<String> {
    use base64::Engine as _;
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return None;
    }
    let kind = match Path::new(name).extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => return None,
    };
    let bytes = std::fs::read(folder.join(name)).ok().filter(|b| b.len() <= 256 * 1024)?;
    Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Plugins turned off (by id), kept in the Plugins folder.
fn disabled() -> BTreeSet<String> {
    std::fs::read_to_string(dir().join("disabled.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn enable(id: &str, on: bool) -> Result<Value> {
    let mut d = disabled();
    if on {
        d.remove(id);
    } else {
        d.insert(id.to_string());
    }
    std::fs::create_dir_all(dir())?;
    std::fs::write(dir().join("disabled.json"), serde_json::to_string_pretty(&d)?)?;
    Ok(json!({"id": id, "enabled": on}))
}

/// A folder name for a plugin: lowercase letters, digits and dashes.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(40).collect()
}

const MAX_ZIP: usize = 8 << 20;
const MAX_FILES: usize = 64;
const MAX_UNPACKED: u64 = 16 << 20;
/// How many set-aside plugins `.removed` keeps (the newest).
const KEEP_REMOVED: usize = 20;

/// What installing would do, and what the plugin is: shown to the person
/// before anything is written.
pub struct InstallOpts {
    /// describe it, install nothing
    pub preview: bool,
    /// allow replacing a plugin of the same name
    pub replace: bool,
}

/// A fingerprint of a plugin's files (names and contents).
fn files_hash(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut all = Vec::new();
    for (n, b) in sorted {
        all.extend_from_slice(n.as_bytes());
        all.push(0);
        all.extend_from_slice(&(b.len() as u64).to_le_bytes());
        all.extend_from_slice(b);
    }
    crate::safety::sha256_hex(&all)
}

/// The installed copy of `id`: its version and fingerprint, if there is one.
fn installed(id: &str) -> Option<Value> {
    let folder = dir().join(id);
    if !folder.is_dir() {
        return None;
    }
    let manifest: Value = std::fs::read_to_string(folder.join("plugin.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&folder) {
        for e in rd.flatten().take(MAX_FILES) {
            if e.path().is_file() {
                if let Ok(b) = std::fs::read(e.path()) {
                    files.push((e.file_name().to_string_lossy().into_owned(), b));
                }
            }
        }
    }
    Some(json!({"version": manifest["version"], "author": manifest["author"], "sha256": files_hash(&files)}))
}

/// Install a plugin from a .zip's bytes: plugin.json and main.js at its top,
/// or in one folder inside. Only plain files with safe names are unpacked
/// (no paths out of the folder, no hidden files, size limits). Replaces an
/// older copy of the same plugin, which is kept in .removed.
pub fn install(bytes: &[u8], file_name: Option<&str>, opts: &InstallOpts) -> Result<Value> {
    if bytes.len() > MAX_ZIP {
        bail!("that plugin is over 8 MB");
    }
    // a single SketchUp Ruby script (.rb)
    if file_name.is_some_and(|n| n.to_ascii_lowercase().ends_with(".rb")) {
        let name = Path::new(file_name.unwrap()).file_name().and_then(|n| n.to_str()).unwrap_or("script.rb").to_string();
        let text = std::str::from_utf8(bytes).context("that .rb file isn't text")?;
        return install_ruby(vec![(name, text.as_bytes().to_vec())], opts);
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("that isn't a .zip, .rbz or .rb file")?;
    if z.len() > MAX_FILES {
        bail!("that plugin has over {MAX_FILES} files");
    }
    // where plugin.json is: the top, or one folder down
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for i in 0..z.len() {
        let f = z.by_index(i)?;
        let n = f.name()?.to_string();
        // two entries with one name: what is shown and what is installed could differ
        if !f.is_dir() && !seen.insert(n.trim_start_matches("./").to_ascii_lowercase()) {
            bail!("the plugin has two files named {n}");
        }
        names.push(n);
    }
    let manifest_at = names.iter().find(|n| n.trim_start_matches("./") == "plugin.json" || (n.ends_with("/plugin.json") && n.matches('/').count() == 1)).cloned();
    // no plugin.json but Ruby files: a SketchUp extension (.rbz)
    let ruby = manifest_at.is_none() && names.iter().any(|n| n.to_ascii_lowercase().ends_with(".rb"));
    if manifest_at.is_none() && !ruby {
        bail!("no plugin.json in it (or .rb files, for a SketchUp extension)");
    }
    let prefix = manifest_at.as_deref().map(|m| m.trim_start_matches("./").trim_end_matches("plugin.json").to_string()).unwrap_or_default();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name()?.trim_start_matches("./").to_string();
        let Some(rel) = name.strip_prefix(&prefix) else { continue };
        // safe names only: no parent dirs, no absolute paths, no hidden files
        if rel.is_empty() || rel.split('/').any(|p| p.is_empty() || p == ".." || p.starts_with('.')) || rel.contains('\\') || rel.contains(':') {
            bail!("unsafe file name in the plugin: {name}");
        }
        let ext = Path::new(rel).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "json" | "js" | "svg" | "png" | "md" | "txt" | "rb") {
            continue; // other files aren't needed
        }
        // count what is really there, not what the header says it will be
        let mut b = Vec::new();
        f.by_ref().take(MAX_UNPACKED - total + 1).read_to_end(&mut b)?;
        total += b.len() as u64;
        if total > MAX_UNPACKED {
            bail!("that plugin unpacks to over 16 MB");
        }
        files.push((rel.to_string(), b));
    }
    if ruby {
        return install_ruby(files, opts);
    }
    let manifest = files.iter().find(|(n, _)| n == "plugin.json").map(|(_, b)| b.clone()).ok_or_else(|| anyhow::anyhow!("no plugin.json"))?;
    let m: Value = serde_json::from_slice(&manifest).context("plugin.json isn't valid JSON")?;
    if !manifest_is_ruby(&m) && !files.iter().any(|(n, _)| n == "main.js") {
        bail!("no main.js next to plugin.json");
    }
    let name = m.get("name").and_then(|n| n.as_str()).filter(|n| !n.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("plugin.json needs a name"))?;
    if !manifest_is_ruby(&m) && !m.get("commands").is_some_and(|c| c.is_array()) {
        bail!("plugin.json needs a commands list");
    }
    let id = slug(name);
    if id.is_empty() || BUILTIN.iter().any(|(b, ..)| *b == id) {
        bail!("pick another name for this plugin");
    }
    // plugin.json that says "ruby": true is installed, shown and run as a Ruby extension (the same test
    // `list()` uses). One that also ships main.js is two things at once: refused, not guessed at.
    let is_ruby = manifest_is_ruby(&m);
    if is_ruby && files.iter().any(|(n, _)| n == "main.js") {
        bail!("plugin.json says this is a SketchUp Ruby extension (\"ruby\": true) but the zip also has a main.js; a plugin is one or the other");
    }
    if is_ruby && !files.iter().any(|(n, _)| !n.contains('/') && n.to_ascii_lowercase().ends_with(".rb")) {
        bail!("plugin.json says this is a SketchUp Ruby extension (\"ruby\": true) but there is no .rb file at the top of it");
    }
    let about = describe(&id, name, Some(&m), is_ruby, &files);
    if let Some(done) = gate(&about, opts)? {
        return Ok(done);
    }
    let replaced = write_plugin(&id, &files)?;
    // a brand-new plugin is on; a replaced one keeps whatever the person chose for it
    if !replaced && disabled().contains(&id) {
        enable(&id, true)?;
    }
    Ok(json!({"id": id, "name": name, "files": files.len(), "replaced": replaced, "ruby": is_ruby, "sha256": about["sha256"]}))
}

/// What a plugin is, for the person deciding whether to install it.
fn describe(id: &str, name: &str, manifest: Option<&Value>, ruby: bool, files: &[(String, Vec<u8>)]) -> Value {
    let field = |k: &str| manifest.and_then(|m| m.get(k)).and_then(Value::as_str).map(str::to_string);
    // a Ruby extension's commands are the menu items its code adds (the manifest's list isn't used for it)
    let commands: Vec<String> = if ruby {
        ruby_menu_labels(files)
    } else {
        manifest.and_then(|m| m["commands"].as_array()).map(|a| a.iter().filter_map(|c| c["label"].as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    json!({
        "id": id,
        "name": name,
        "version": field("version"),
        "author": field("author"),
        "description": field("description"),
        "kind": if ruby { "SketchUp Ruby extension" } else { "JavaScript plugin" },
        "ruby": ruby,
        "commands": commands,
        "commands_note": if ruby { "Read from the Ruby code. The menu is built when the extension runs, and it runs every time SketchCraft starts." } else { "" },
        "files": files.iter().map(|(n, b)| json!({"name": n, "bytes": b.len()})).collect::<Vec<_>>(),
        "sha256": files_hash(files),
        "installed": installed(id),
    })
}

/// A preview answers with what it is; an install over an existing plugin
/// needs `replace`. Returns the reply to give now, or None to go ahead.
fn gate(about: &Value, opts: &InstallOpts) -> Result<Option<Value>> {
    if opts.preview {
        let mut v = about.clone();
        v["preview"] = json!(true);
        return Ok(Some(v));
    }
    if !about["installed"].is_null() && !opts.replace {
        bail!(
            "{} is already installed (version {}). Installing this would replace it; ask the person first, then use replace: true.",
            about["name"].as_str().unwrap_or("that plugin"),
            about["installed"]["version"].as_str().unwrap_or("unknown")
        );
    }
    Ok(None)
}

/// A SketchUp Ruby extension: its .rb files (the top-level ones load it),
/// with a plugin.json written for it. Its commands are the menu items it adds
/// when it loads (the app finds them by running it).
fn install_ruby(files: Vec<(String, Vec<u8>)>, opts: &InstallOpts) -> Result<Value> {
    let entries: Vec<String> = files.iter().filter(|(n, _)| !n.contains('/') && n.to_ascii_lowercase().ends_with(".rb")).map(|(n, _)| n.clone()).collect();
    let Some(first) = entries.first() else { bail!("no .rb file at the top of it (a SketchUp extension loads from one)") };
    let stem = first.trim_end_matches(".rb").trim_end_matches(".RB");
    let id = slug(stem);
    if id.is_empty() || BUILTIN.iter().any(|(b, ..)| *b == id) {
        bail!("pick another name for this script");
    }
    let name = stem.split(['_', '-']).filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ");
    let mut all = files;
    all.retain(|(n, _)| n != "plugin.json");
    let manifest = json!({"name": name, "ruby": true, "entry": entries, "description": "A SketchUp Ruby extension", "commands": []});
    all.push(("plugin.json".into(), serde_json::to_vec_pretty(&manifest)?));
    let about = describe(&id, &name, Some(&manifest), true, &all);
    if let Some(done) = gate(&about, opts)? {
        return Ok(done);
    }
    let replaced = write_plugin(&id, &all)?;
    if !replaced && disabled().contains(&id) {
        enable(&id, true)?;
    }
    Ok(json!({"id": id, "name": name, "files": all.len(), "replaced": replaced, "ruby": true, "sha256": about["sha256"]}))
}

/// Write a plugin's files into Plugins/<id> (an older copy goes to .removed).
/// Returns whether an older copy was replaced.
fn write_plugin(id: &str, files: &[(String, Vec<u8>)]) -> Result<bool> {
    let root = dir();
    std::fs::create_dir_all(&root)?;
    let target = root.join(id);
    // write the new copy beside it first: if that fails, the old one is untouched
    let staging = root.join(format!(".installing-{id}-{}", std::process::id()));
    std::fs::remove_dir_all(&staging).ok();
    let written = (|| -> Result<()> {
        for (rel, b) in files {
            let p = staging.join(rel);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(p, b)?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        std::fs::remove_dir_all(&staging).ok();
        return Err(e);
    }
    let replaced = target.exists();
    if replaced {
        if let Err(e) = set_aside(&target, id) {
            std::fs::remove_dir_all(&staging).ok();
            return Err(e);
        }
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        std::fs::remove_dir_all(&staging).ok();
        return Err(e.into());
    }
    Ok(replaced)
}

fn set_aside(folder: &Path, id: &str) -> Result<PathBuf> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    // a fresh name, even twice in one second
    let mut away = dir().join(".removed").join(format!("{id}-{stamp}"));
    let mut n = 1;
    while away.exists() {
        n += 1;
        away = dir().join(".removed").join(format!("{id}-{stamp}-{n}"));
    }
    std::fs::create_dir_all(away.parent().unwrap())?;
    std::fs::rename(folder, &away)?;
    prune_removed();
    Ok(away)
}

/// When a set-aside folder was made, from its name `id-<seconds>[-<n>]` (the folder's own
/// modified time doesn't change when it is renamed, so it says nothing about when it was removed).
fn removed_key(name: &str) -> (u64, u64) {
    let toks: Vec<&str> = name.split('-').collect();
    let nums: Vec<(usize, u64)> = toks.iter().enumerate().rev().take(2).filter_map(|(i, t)| t.parse::<u64>().ok().map(|n| (i, n))).collect();
    // seconds (older set-asides) or milliseconds, all compared in milliseconds
    let ms = |t: u64| if t < 100_000_000_000 { t * 1000 } else { t };
    match nums.as_slice() {
        [(_, n), (_, t)] if *t >= 100_000_000 => (ms(*t), *n),
        [(_, t), ..] if *t >= 100_000_000 => (ms(*t), 0),
        _ => (0, 0),
    }
}

/// Keep only the newest `KEEP_REMOVED` folders in `.removed`, by when they were set aside;
/// and clear staging folders a crashed install left behind.
fn prune_removed() {
    let root = dir().join(".removed");
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut all: Vec<(( u64, u64), PathBuf)> = rd.flatten().filter(|e| e.path().is_dir()).map(|e| (removed_key(&e.file_name().to_string_lossy()), e.path())).collect();
        all.sort();
        while all.len() > KEEP_REMOVED {
            std::fs::remove_dir_all(all.remove(0).1).ok();
        }
    }
    clear_stale_staging();
}

/// `.installing-*` folders older than an hour belong to an install that died.
fn clear_stale_staging() {
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with(".installing-") && e.path().is_dir() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() > 3600);
            if old {
                std::fs::remove_dir_all(e.path()).ok();
            }
        }
    }
}

/// Move one of your plugins aside (to Plugins/.removed), so it can be put back.
pub fn remove(id: &str) -> Result<Value> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.starts_with('.') || BUILTIN.iter().any(|(b, ..)| *b == id) {
        bail!("can't remove {id:?}");
    }
    let folder = dir().join(id);
    if !folder.is_dir() {
        bail!("no plugin {id}");
    }
    let away = set_aside(&folder, id)?;
    Ok(json!({"id": id, "moved_to": away}))
}

/// Every plugin: manifest fields, its code, and whether it's built in. A
/// folder that can't be read is listed in `problems` instead.
pub fn list() -> Value {
    let mut out: Vec<Value> = BUILTIN.iter().filter_map(|(id, m, c)| entry(id, m, c.to_string(), true, None)).collect();
    let mut problems = Vec::new();
    let d = dir();
    std::fs::create_dir_all(&d).ok(); // so "Open plugins folder" has somewhere to go
    clear_stale_staging();
    if let Ok(rd) = std::fs::read_dir(&d) {
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
        dirs.sort();
        for p in dirs {
            let id = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let manifest = std::fs::read_to_string(p.join("plugin.json"));
            let code = std::fs::read_to_string(p.join("main.js"));
            // a SketchUp Ruby extension has .rb files instead of main.js
            let is_ruby = manifest.as_ref().ok().and_then(|m| serde_json::from_str::<Value>(m).ok()).is_some_and(|v| manifest_is_ruby(&v));
            if is_ruby {
                match (manifest, ruby_files(&p)) {
                    (Ok(m), Ok(f)) => match entry(&id, &m, String::new(), false, Some(&p)) {
                        Some(mut v) => {
                            v["files"] = json!(f);
                            out.push(v);
                        }
                        None => problems.push(json!({"id": id, "error": "plugin.json isn't valid JSON"})),
                    },
                    (_, Err(e)) => problems.push(json!({"id": id, "error": e.to_string()})),
                    _ => problems.push(json!({"id": id, "error": "needs plugin.json"})),
                }
                continue;
            }
            match (manifest, code) {
                (Ok(m), Ok(c)) if c.len() <= 1 << 20 => match entry(&id, &m, c, false, Some(&p)) {
                    Some(v) => out.push(v),
                    None => problems.push(json!({"id": id, "error": "plugin.json isn't valid JSON"})),
                },
                (Ok(_), Ok(_)) => problems.push(json!({"id": id, "error": "main.js is over 1 MB"})),
                _ => problems.push(json!({"id": id, "error": "needs plugin.json and main.js"})),
            }
        }
    }
    json!({"dir": d, "plugins": out, "problems": problems})
}

/// A part as measured: what `parts` reports, and what laying it flat or drawing its cutting diagram needs.
#[derive(Clone)]
pub struct Measured {
    pub id: Id,
    pub name: String,
    /// "group" or "component"
    pub kind: &'static str,
    pub material: Option<String>,
    /// [length, width, thickness], largest first, to 0.1 mm
    pub size: [f64; 3],
    /// the corner its box starts from (world space): the part fills `origin + a·u + b·w + c·n` for a, b, c from 0 to `extent`
    pub origin: DVec3,
    /// which way it lies (world space, unit length): `n` is the normal of its biggest face; `u` runs along that
    /// face's longest edge, or square to it when the part reaches further that way; `w` is n × u. For a board:
    /// length, width and thickness.
    pub u: DVec3,
    pub w: DVec3,
    pub n: DVec3,
    /// how far it reaches along `u`, `w` and `n`
    pub extent: DVec3,
}

/// Each part's size, where it is and which way it lies, measured along its biggest face: `ids` (groups or
/// components), or every group with nothing grouped inside it and every component. This frame is the one answer
/// to "which way does this part lie" (`parts`, `parts_layout`).
pub fn measure(m: &Model, ids: &[Id]) -> Vec<Measured> {
    let wanted: Vec<Id> = if ids.is_empty() {
        // every group with nothing grouped inside it, and every component
        let mut v: Vec<Id> = m.groups.iter().filter(|(id, g)| !g.hidden && !m.groups.values().any(|o| o.parent == Some(**id))).map(|(id, _)| *id).collect();
        // components, but not scale figures (flat people aren't parts)
        v.extend(m.instances.iter().filter(|(_, i)| !i.hidden && !m.definitions.get(&i.def).is_some_and(|d| d.face_me)).map(|(id, _)| *id));
        v
    } else {
        ids.to_vec()
    };
    let world = m.world_faces();
    wanted
        .into_iter()
        .filter_map(|id| {
            let (name, kind, faces): (String, &str, Vec<_>) = if let Some(g) = m.groups.get(&id) {
                let tree = m.subtree(id);
                let mine = |f: &&crate::components::WFace| f.face.and_then(|fid| m.faces.get(&fid)).and_then(|x| x.group).is_some_and(|x| tree.contains(&x));
                (g.name.clone(), "group", world.iter().filter(mine).map(|f| (f.outer.clone(), f.normal, f.material.clone(), f.color)).collect())
            } else if let Some(i) = m.instances.get(&id) {
                (i.name.clone(), "component", world.iter().filter(|f| f.owner == id).map(|f| (f.outer.clone(), f.normal, f.material.clone(), f.color)).collect())
            } else {
                return None;
            };
            if faces.is_empty() {
                return None;
            }
            // axes: the biggest face's normal and its longest edge
            let area = |pts: &Vec<DVec3>, n: DVec3| -> f64 {
                let mut s = DVec3::ZERO;
                for k in 0..pts.len() {
                    s += pts[k].cross(pts[(k + 1) % pts.len()]);
                }
                (s.dot(n) * 0.5).abs()
            };
            // a board has two faces that big: take the one that looks down most, so a part lying flat is already on it
            let most = faces.iter().map(|f| area(&f.0, f.1)).fold(0.0, f64::max);
            let big = faces.iter().filter(|f| area(&f.0, f.1) >= most * (1.0 - 1e-9)).min_by(|a, b| a.1.normalize_or_zero().z.total_cmp(&b.1.normalize_or_zero().z))?;
            let n = big.1.normalize_or_zero();
            let mut u = DVec3::ZERO;
            for k in 0..big.0.len() {
                let e = big.0[(k + 1) % big.0.len()] - big.0[k];
                let e = e - n * e.dot(n);
                if e.length() > u.length() {
                    u = e;
                }
            }
            let u = u.normalize_or_zero();
            let w = n.cross(u).normalize_or_zero();
            if u == DVec3::ZERO || w == DVec3::ZERO {
                return None;
            }
            // how far the part reaches along `u`, `w` and `n`
            let reach = |u: DVec3, w: DVec3| -> (DVec3, DVec3) {
                let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for (pts, ..) in &faces {
                    for p in pts {
                        let q = DVec3::new(p.dot(u), p.dot(w), p.dot(n));
                        lo = lo.min(q);
                        hi = hi.max(q);
                    }
                }
                (lo, hi)
            };
            // the longest edge is not always the long way (a long side made of a curve's segments, or notched):
            // `u` is whichever of the two directions in the face the part reaches further along
            let (lo, hi) = reach(u, w);
            let u = if hi.y - lo.y > (hi.x - lo.x) * (1.0 + 1e-9) { w } else { u };
            // an edge runs either way: point `u` along the positive side of the axis it follows most
            let lead = if u.x.abs() >= u.y.abs() && u.x.abs() >= u.z.abs() { u.x } else if u.y.abs() >= u.z.abs() { u.y } else { u.z };
            let u = if lead < 0.0 { -u } else { u };
            let w = n.cross(u).normalize_or_zero();
            let (lo, hi) = reach(u, w);
            let extent = hi - lo;
            let mut size = extent.to_array();
            size.sort_by(|a, b| b.total_cmp(a));
            if size[2] < 1e-4 {
                return None; // flat: a drawing or a figure, not something to cut
            }
            // the material most of its faces wear
            let mut count: BTreeMap<String, usize> = BTreeMap::new();
            for (.., mat, col) in &faces {
                let key = mat.clone().or_else(|| col.map(|c| format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8))).unwrap_or_default();
                *count.entry(key).or_default() += 1;
            }
            let material = count.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k).filter(|k| !k.is_empty());
            Some(Measured { id, name, kind, material, size: size.map(|x| (x * 10000.0).round() / 10000.0), origin: u * lo.x + w * lo.y + n * lo.z, u, w, n, extent })
        })
        .collect()
}

/// A number for a reply: rounded to `step` (a power of ten, like 1e-9), and never "-0".
pub(crate) fn tidy(x: f64, step: f64) -> f64 {
    // by the whole number of steps in one: multiplying by 1e-9 would leave 0.30000000000000004
    let per = (1.0 / step).round();
    (x * per).round() / per + 0.0
}

pub(crate) fn tidy3(v: DVec3, step: f64) -> [f64; 3] {
    v.to_array().map(|x| tidy(x, step))
}

/// What `parts` answers for each part: name, kind, material, `size` as [length, width, thickness] (largest first),
/// and its frame: `origin`, `axes` {u, w, n} and the `extent` along each (see `Measured`).
pub fn parts(m: &Model, ids: &[Id]) -> Vec<Value> {
    measure(m, ids)
        .into_iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "kind": p.kind, "material": p.material, "size": p.size,
                "origin": tidy3(p.origin, 1e-9),
                "axes": {"u": tidy3(p.u, 1e-12), "w": tidy3(p.w, 1e-12), "n": tidy3(p.n, 1e-12)},
                "extent": tidy3(p.extent, 1e-9),
            })
        })
        .collect()
}
