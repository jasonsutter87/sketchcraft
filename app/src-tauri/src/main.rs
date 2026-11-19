//! SketchCraft desktop backend. Holds the engine and gives the UI three calls:
//!
//!   exec(cmd)  run one engine command (the UI never edits geometry itself)
//!   scene()    triangles, edges and layers for the 3D view
//!   sync()     pick up changes another client (Claude over MCP) saved
//!
//! The model lives in ~/Documents/SketchCraft/live.sketchcraft (or $SKETCHCRAFT_DOC).
//! The app saves there after every change and reloads when someone else
//! writes it, so you and an AI agent can work on the same model at once.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use sketchcraft::live::{default_doc, Live};
use serde_json::Value;
use std::sync::Mutex;
use tauri::Manager;

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
const RELEASE_BASE: &str = "https://github.com/jasonsutter87/sketchcraft/releases/download/";

/// An update's files must come from the release it says it is: its download
/// address sits under that version's own tag. (The signature proves who made
/// a file, not which version it is, so a manifest claiming a new version
/// can't point at an older signed build.)
fn release_url_ok(version: &str, url: &str) -> bool {
    url.starts_with(&format!("{RELEASE_BASE}v{version}/"))
}

/// Every download in the release is from its own release's address.
fn release_files_ok(release: &tauri_plugin_updater::RemoteRelease) -> bool {
    use tauri_plugin_updater::RemoteReleaseInner::{Dynamic, Static};
    let version = release.version.to_string();
    match &release.data {
        Dynamic(p) => release_url_ok(&version, p.url.as_str()),
        Static { platforms } => !platforms.is_empty() && platforms.values().all(|p| release_url_ok(&version, p.url.as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_open_only_the_named_pages() {
        for k in ["repo", "license", "changelog", "issues"] {
            assert!(link_url(k).is_some_and(|u| u.starts_with("https://github.com/jasonsutter87/sketchcraft")), "{k}");
        }
        assert_eq!(link_url("https://example.com"), None);
        assert_eq!(link_url(""), None);
    }

    #[test]
    fn downloads_come_from_their_own_release() {
        assert!(release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.3.6/SketchCraft.app.tar.gz"));
        assert!(!release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.2.0/SketchCraft.app.tar.gz"));
        assert!(!release_url_ok("0.3.6", "https://evil.example/jasonsutter87/sketchcraft/releases/download/v0.3.6/x"));
        assert!(!release_url_ok("0.3.6", "https://github.com/jasonsutter87/sketchcraft/releases/download/v0.3.60/x"));
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        // signed self-updates from GitHub Releases (see app/src/updater.tsx)
        // (and only newer than this copy, from the matching release: see release_files_ok)
        .plugin(tauri_plugin_updater::Builder::new().default_version_comparator(|current, release| release.version > current && release_files_ok(&release)).build())
        .manage(App(Mutex::new({
            let mut live = Live::open(default_doc());
            // this is the person's window: it says what an agent changed while it was closed
            live.as_person_view();
            live
        })))
        .setup(|app| {
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
        .invoke_handler(tauri::generate_handler![exec, scene, sync, open_releases, portable, open_guide, open_link])
        .build(tauri::generate_context!())
        .expect("error while building SketchCraft")
        .run(|_, event| {
            // a helper program (ffmpeg, Python, a DWG converter) still running goes with the app
            if let tauri::RunEvent::Exit = event {
                sketchcraft::helper::kill_all();
            }
        });
}
