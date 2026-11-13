//! Riseline desktop backend. Holds the engine and gives the UI three calls:
//!
//!   exec(cmd)  run one engine command (the UI never edits geometry itself)
//!   scene()    triangles, edges and layers for the 3D view
//!   sync()     pick up changes another client (Claude over MCP) saved
//!
//! The model lives in ~/Documents/Riseline/live.riseline. The app saves there
//! after every change and reloads when someone else writes it, so you and an
//! AI agent can work on the same model at the same time.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use riseline::{geom, Engine};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

struct App {
    engine: Mutex<Engine>,
    doc: PathBuf,
    seen: Mutex<Option<SystemTime>>,
}

const READ_ONLY: &[&str] = &["faces", "info", "pick", "render", "save", "export_obj"];
const DEFAULT_COLOR: [f32; 3] = [0.95, 0.94, 0.91];

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl App {
    fn save(&self, e: &mut Engine) {
        if let Some(dir) = self.doc.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        e.exec_json(&json!({"cmd": "save", "path": self.doc}).to_string());
        *self.seen.lock().unwrap() = mtime(&self.doc);
    }
}

#[tauri::command]
fn exec(state: tauri::State<App>, cmd: Value) -> Value {
    let mut e = state.engine.lock().unwrap();
    let r = e.exec_json(&cmd.to_string());
    let name = cmd["cmd"].as_str().unwrap_or("");
    if r["ok"] == true && !READ_ONLY.contains(&name) {
        state.save(&mut e);
    }
    r
}

#[tauri::command]
fn scene(state: tauri::State<App>) -> Value {
    let e = state.engine.lock().unwrap();
    let m = &e.model;
    let mut positions: Vec<f32> = Vec::new();
    let mut colors: Vec<f32> = Vec::new();
    let mut tri_face: Vec<u32> = Vec::new();
    let mut faces = Vec::new();
    for (&id, f) in &m.faces {
        let outer = m.pts(&f.outer);
        let holes: Vec<_> = f.holes.iter().map(|h| m.pts(h)).collect();
        let n = geom::newell(&outer);
        let color = f.material.as_ref().and_then(|k| m.materials.get(k)).map(|k| k.color).unwrap_or(DEFAULT_COLOR);
        for t in geom::triangulate(&outer, &holes, n) {
            for p in t {
                positions.extend([p.x as f32, p.y as f32, p.z as f32]);
                colors.extend(color);
            }
            tri_face.push(id);
        }
        faces.push(json!({
            "id": id,
            "normal": n.to_array(),
            "outer": outer.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
            "group": f.group,
            "area": m.face_area(id),
            "material": f.material,
        }));
    }
    let edges: Vec<f32> = m
        .edges
        .values()
        .flat_map(|e| {
            let (a, b) = (m.p(e.a), m.p(e.b));
            [a.x as f32, a.y as f32, a.z as f32, b.x as f32, b.y as f32, b.z as f32]
        })
        .collect();
    let groups: Vec<Value> = m
        .groups
        .iter()
        .map(|(id, g)| json!({"id": id, "name": g.name, "faces": m.faces.values().filter(|f| f.group == Some(*id)).count()}))
        .collect();
    let loose = m.faces.values().filter(|f| f.group.is_none()).count();
    json!({
        "positions": positions,
        "colors": colors,
        "triFace": tri_face,
        "faces": faces,
        "edges": edges,
        "groups": groups,
        "loose": loose,
        "activeGroup": m.active_group,
        "doc": state.doc,
    })
}

/// Reload if another client saved the model since we last saw it.
#[tauri::command]
fn sync(state: tauri::State<App>) -> bool {
    let now = mtime(&state.doc);
    let mut seen = state.seen.lock().unwrap();
    if now.is_some() && now != *seen {
        let mut e = state.engine.lock().unwrap();
        let r = e.exec_json(&json!({"cmd": "open", "path": state.doc}).to_string());
        *seen = now;
        return r["ok"] == true;
    }
    false
}

fn main() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let doc = std::env::var("RISELINE_DOC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(home).join("Documents/Riseline/live.riseline"));
    let mut engine = Engine::new();
    if doc.exists() {
        engine.exec_json(&json!({"cmd": "open", "path": doc}).to_string());
    }
    let seen = Mutex::new(mtime(&doc));
    tauri::Builder::default()
        .manage(App { engine: Mutex::new(engine), doc, seen })
        .invoke_handler(tauri::generate_handler![exec, scene, sync])
        .run(tauri::generate_context!())
        .expect("error while running Riseline");
}
