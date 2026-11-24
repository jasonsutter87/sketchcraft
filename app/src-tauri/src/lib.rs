//! SketchCraft app backend (desktop and Android). Holds the engine and gives the UI three calls:
//!
//!   exec(cmd)  run one engine command (the UI never edits geometry itself)
//!   scene()    triangles, edges and layers for the 3D view
//!   sync()     pick up changes another client (Claude over MCP) saved
//!
//! The model lives in ~/Documents/SketchCraft/live.sketchcraft (or $SKETCHCRAFT_DOC); on a phone, in the
//! app's own data folder.
//! The app saves there after every change and reloads when someone else
//! writes it, so you and an AI agent can work on the same model at once.

use sketchcraft::live::Live;

mod photocraft;
use serde_json::Value;
use std::sync::Mutex;
use tauri::Manager;

mod ai;

struct App(Mutex<Live>);

/// The engine, even if an earlier call panicked while holding it (a bug in one
/// command shouldn't lock you out of the rest of the app).
fn engine<'a>(state: &'a tauri::State<'_, App>) -> std::sync::MutexGuard<'a, Live> {
    state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run `f`, turning a panic into an error reply instead of a crash.
fn guarded<T>(f: impl FnOnce() -> T, fallback: impl FnOnce(String) -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown".into());
            eprintln!("sketchcraft: a command panicked: {msg}");
            fallback(msg)
        }
    }
}

/// `source` is "plugin" for commands that plugin code asked for (JavaScript or
/// SketchUp Ruby): those run under the plugin allowlist in `sketchcraft::policy`
/// (modeling only: no files, programs, network or plugin management).
#[tauri::command]
fn exec(state: tauri::State<App>, cmd: Value, source: Option<String>, plugin: Option<String>) -> Value {
    let caller = if source.as_deref() == Some("plugin") { sketchcraft::policy::Caller::Plugin } else { sketchcraft::policy::Caller::Ui };
    let mut live = engine(&state);
    guarded(|| live.exec_as_from(&caller, &cmd, plugin.as_deref()), |msg| serde_json::json!({"ok": false, "error": format!("SketchCraft hit a bug running that ({msg}). Undo if anything looks wrong.")}))
}

/// SketchCraft AI box: what the model is told about the model (and the group names it may use as @name).
#[tauri::command]
fn ai_state(state: tauri::State<App>) -> String {
    let mut live = engine(&state);
    guarded(|| ai::state(&mut live), |_| "empty model".into())
}

/// SketchCraft AI box: ask the local model server (loopback only). Runs off the UI thread; takes no engine lock.
#[tauri::command]
async fn ai_chat(messages: Value, url: Option<String>) -> Result<Value, String> {
    let u = ai::endpoint(url.as_deref());
    let text = ai::chat(&u, &messages)?;
    Ok(serde_json::json!({"text": text, "url": u}))
}

/// SketchCraft AI box: run the model's answer as an untrusted agent (policy allowlist), one undo step.
#[tauri::command]
fn ai_apply(state: tauri::State<App>, commands: Value) -> Value {
    let mut live = engine(&state);
    guarded(|| ai::apply(&mut live, &commands), |msg| serde_json::json!({"ok": false, "error": format!("SketchCraft hit a bug running that ({msg}). Undo if anything looks wrong."), "rows": []}))
}

/// SketchCraft AI box debugger: append one turn (request, what the model was told, its raw answer, what ran, every
/// result and error, timing) as a JSON line to `AI debug/<date>.jsonl` beside the model. Local only; nothing is sent.
/// Only the hidden labs AI box calls this; the log is capped (size per day, number of days).
#[tauri::command]
fn ai_log(state: tauri::State<App>, entry: Value) -> Result<String, String> {
    let doc = engine(&state).doc.clone();
    let dir = doc.parent().map(|d| d.join("AI debug")).unwrap_or_else(|| std::path::PathBuf::from("AI debug"));
    let day = entry["at"].as_str().and_then(|s| s.get(..10)).filter(|d| d.chars().all(|c| c.is_ascii_digit() || c == '-')).unwrap_or("undated");
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    if line.len() > 2 << 20 {
        return Err("debug entry too large".into());
    }
    // capped: each day's file at most ai::LOG_MAX_FILE_BYTES, the newest ai::LOG_KEEP_FILES days kept
    ai::append_log(&dir, day, &line, ai::LOG_MAX_FILE_BYTES, ai::LOG_KEEP_FILES).map(|p| p.display().to_string())
}

#[tauri::command]
fn scene(state: tauri::State<App>) -> Value {
    let live = engine(&state);
    guarded(|| live.scene(), |_| serde_json::json!({"ok": false}))
}

#[tauri::command]
fn sync(state: tauri::State<App>) -> bool {
    let mut live = engine(&state);
    guarded(|| live.sync(), |_| false)
}

/// Running from the portable ZIP (a PORTABLE.txt beside the program)? Then
/// updates are a new ZIP to download, not an installer.
#[tauri::command]
fn portable() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("PORTABLE.txt").is_file())).unwrap_or(false)
}

/// Open the guide in the browser. The address is fixed, never from the page.
#[tauri::command]
fn open_guide() -> bool {
    open_url("https://github.com/jasonsutter87/sketchcraft/blob/main/docs/guide/README.md")
}

fn open_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.is_ok()
}

/// The few pages the About and Help menus link to. The address is chosen here by name, never taken from the page.
fn link_url(which: &str) -> Option<&'static str> {
    Some(match which {
        "website" => "https://trysketchcraft.com/",
        "repo" => "https://github.com/jasonsutter87/sketchcraft",
        "license" => "https://github.com/jasonsutter87/sketchcraft/blob/main/LICENSE",
        "changelog" => "https://github.com/jasonsutter87/sketchcraft/blob/main/CHANGELOG.md",
        "issues" => "https://github.com/jasonsutter87/sketchcraft/issues",
        _ => return None,
    })
}

/// Open one of those pages in the browser (Help > Report a bug, About's links).
#[tauri::command]
fn open_link(which: &str) -> bool {
    link_url(which).is_some_and(open_url)
}

/// Open the download page in the browser (for installs the updater can't
/// replace, like a .deb or .rpm). The address is fixed, never from the page.
#[tauri::command]
fn open_releases() -> bool {
    const URL: &str = "https://github.com/jasonsutter87/sketchcraft/releases/latest";
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(URL).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(URL).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(URL).spawn();
    r.is_ok()
}

/// Where this project's releases are downloaded from.
#[cfg(desktop)]
const RELEASE_BASE: &str = "https://github.com/jasonsutter87/sketchcraft/releases/download/";

/// An update's files must come from the release it says it is: its download
/// address sits under that version's own tag. (The signature proves who made
/// a file, not which version it is, so a manifest claiming a new version
/// can't point at an older signed build.)
#[cfg(desktop)]
fn release_url_ok(version: &str, url: &str) -> bool {
    url.starts_with(&format!("{RELEASE_BASE}v{version}/"))
}

/// Every download in the release is from its own release's address.
#[cfg(desktop)]
fn release_files_ok(release: &tauri_plugin_updater::RemoteRelease) -> bool {
    use tauri_plugin_updater::RemoteReleaseInner::{Dynamic, Static};
    let version = release.version.to_string();
    match &release.data {
        Dynamic(p) => release_url_ok(&version, p.url.as_str()),
        Static { platforms } => !platforms.is_empty() && platforms.values().all(|p| release_url_ok(&version, p.url.as_str())),
    }
}


/// Where the PhotoCraft files live: a `Textures` folder beside the model.
fn textures_dir(state: &tauri::State<App>) -> std::path::PathBuf {
    let doc = engine(state).doc.clone();
    doc.parent().map(|d| d.join("Textures")).unwrap_or_else(|| std::path::PathBuf::from("Textures"))
}

/// The picture SketchCraft last wrote to, or read from, a material's `.ora` (to tell whether it still matches).
fn last_hash_path(ora: &std::path::Path) -> std::path::PathBuf {
    ora.with_extension("ora.sketchcraft")
}

/// Open a material's picture in PhotoCraft: `texture` is its data: URL, `guide` the UV layout drawn at the same size.
/// The `.ora` from last time is reopened while the material still has the picture that came back from it (so its
/// PhotoCraft layers are kept); otherwise it's written fresh, the old one kept as `.ora.bak`.
#[tauri::command]
fn photocraft_open(state: tauri::State<App>, material: String, texture: String, guide: Option<String>) -> Result<Value, String> {
    let ora = photocraft::ora_path(&textures_dir(&state), &material);
    let tex = photocraft::decode_data_url(&texture)?;
    let current = photocraft::hash_of(&tex);
    let stored = std::fs::read_to_string(last_hash_path(&ora)).ok();
    let reuse = ora.is_file() && photocraft::stored_hash_matches(stored.as_deref(), &tex);
    if reuse && stored.as_deref().is_some_and(|h| !photocraft::is_tagged(h)) {
        // a hash file from before the tagged format: keep the layers and upgrade the file (once)
        photocraft::write_file_safely(&last_hash_path(&ora), current.as_bytes())?;
    }
    if !reuse {
        photocraft::ensure_dir(&textures_dir(&state))?;
        if ora.is_file() {
            photocraft::keep_backup(&ora)?;
        }
        let guide = guide.map(|g| photocraft::decode_data_url(&g)).transpose()?;
        photocraft::write_ora(&ora, &tex, guide.as_deref())?;
        photocraft::write_file_safely(&last_hash_path(&ora), current.as_bytes())?;
    }
    #[cfg(desktop)]
    photocraft::launch(&ora)?;
    #[cfg(mobile)]
    return Err("PhotoCraft runs on computers".into());
    #[allow(unreachable_code)]
    Ok(serde_json::json!({"path": ora.display().to_string(), "modified": photocraft::modified(&ora), "reused": reuse}))
}

/// Has the material's `.ora` been saved since `since` (ms)? Then its picture, flattened without the UV guide, as a
/// PNG data: URL. `psd`: PhotoCraft saved a PSD beside it instead (the document holds something OpenRaster can't).
#[tauri::command]
fn photocraft_poll(state: tauri::State<App>, material: String, since: u64) -> Result<Value, String> {
    use base64::Engine as _;
    let ora = photocraft::ora_path(&textures_dir(&state), &material);
    let m = photocraft::modified(&ora);
    let psd = photocraft::modified(&ora.with_extension("psd"));
    if m <= since {
        return Ok(serde_json::json!({"modified": m, "psd": psd > since && psd > m}));
    }
    let png = photocraft::read_ora(&ora)?;
    photocraft::write_file_safely(&last_hash_path(&ora), photocraft::hash_of(&png).as_bytes())?;
    let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
    Ok(serde_json::json!({"modified": m, "data": data}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_open_only_the_named_pages() {
        for k in ["repo", "license", "changelog", "issues"] {
            assert!(link_url(k).is_some_and(|u| u.starts_with("https://github.com/jasonsutter87/sketchcraft")), "{k}");
        }
        assert_eq!(link_url("website"), Some("https://trysketchcraft.com/"));
        assert_eq!(link_url("https://example.com"), None);
        assert_eq!(link_url(""), None);
    }

    #[test]
    #[cfg(desktop)]
    fn downloads_come_from_their_own_release() {
        assert!(release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.3.6/SketchCraft.app.tar.gz"));
        assert!(!release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.2.0/SketchCraft.app.tar.gz"));
        assert!(!release_url_ok("0.3.6", "https://evil.example/jasonsutter87/sketchcraft/releases/download/v0.3.6/x"));
        assert!(!release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.3.60/x"));
    }
}

/// Where the model lives: ~/Documents/SketchCraft/live.sketchcraft (or $SKETCHCRAFT_DOC) on a computer,
/// the app's private data folder on a phone (there is no shared Documents folder to write to).
fn doc_for(app: &tauri::App) -> std::path::PathBuf {
    #[cfg(mobile)]
    if std::env::var_os("SKETCHCRAFT_DOC").is_none() {
        if let Ok(dir) = app.path().app_data_dir() {
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("live.sketchcraft");
        }
    }
    let _ = app;
    sketchcraft::live::default_doc()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init());
    // signed self-updates from GitHub Releases (see app/src/updater.tsx), computers only: phones update
    // through their store or a new APK
    // (and only newer than this copy, from the matching release: see release_files_ok)
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().default_version_comparator(|current, release| release.version > current && release_files_ok(&release)).build());
    builder
        .setup(|app| {
            let mut live = Live::open(doc_for(app));
            // this is the person's window: it says what an agent changed while it was closed
            live.as_person_view();
            app.manage(App(Mutex::new(live)));
            // closes a plugin run nobody ended (the 150 s limit) and saves it, even when the page is silent
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let state = handle.state::<App>();
                let mut live = engine(&state);
                guarded(|| live.housekeeping(), |_| ());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![exec, ai_state, ai_chat, ai_apply, ai_log, scene, sync, open_releases, portable, open_guide, open_link, photocraft_open, photocraft_poll])
        .build(tauri::generate_context!())
        .expect("error while building SketchCraft")
        .run(|_, event| {
            // a helper program (ffmpeg, Python, a DWG converter) still running goes with the app
            if let tauri::RunEvent::Exit = event {
                sketchcraft::helper::kill_all();
            }
        });
}
