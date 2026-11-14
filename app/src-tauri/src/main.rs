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

#[tauri::command]
fn exec(state: tauri::State<App>, cmd: Value) -> Value {
    let mut live = engine(&state);
    guarded(|| live.exec(&cmd), |msg| serde_json::json!({"ok": false, "error": format!("SketchCraft hit a bug running that ({msg}). Undo if anything looks wrong.")}))
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(App(Mutex::new(Live::open(default_doc()))))
        .invoke_handler(tauri::generate_handler![exec, scene, sync])
        .run(tauri::generate_context!())
        .expect("error while running SketchCraft");
}
