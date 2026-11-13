//! Riseline desktop backend. Holds the engine and gives the UI three calls:
//!
//!   exec(cmd)  run one engine command (the UI never edits geometry itself)
//!   scene()    triangles, edges and layers for the 3D view
//!   sync()     pick up changes another client (Claude over MCP) saved
//!
//! The model lives in ~/Documents/Riseline/live.riseline (or $RISELINE_DOC).
//! The app saves there after every change and reloads when someone else
//! writes it, so you and an AI agent can work on the same model at once.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use riseline::live::{default_doc, Live};
use serde_json::Value;
use std::sync::Mutex;

struct App(Mutex<Live>);

#[tauri::command]
fn exec(state: tauri::State<App>, cmd: Value) -> Value {
    state.0.lock().unwrap().exec(&cmd)
}

#[tauri::command]
fn scene(state: tauri::State<App>) -> Value {
    state.0.lock().unwrap().scene()
}

#[tauri::command]
fn sync(state: tauri::State<App>) -> bool {
    state.0.lock().unwrap().sync()
}

fn main() {
    tauri::Builder::default()
        .manage(App(Mutex::new(Live::open(default_doc()))))
        .invoke_handler(tauri::generate_handler![exec, scene, sync])
        .run(tauri::generate_context!())
        .expect("error while running Riseline");
}
