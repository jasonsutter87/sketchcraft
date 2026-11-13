//! MCP server over stdio (newline-delimited JSON-RPC 2.0), so an AI agent
//! can model in Riseline directly. Three tools:
//!
//!   riseline_run     run a list of commands, get each reply
//!   riseline_render  look at the model (returns a PNG image)
//!   riseline_info    counts, bounds, groups
//!
//! With `--doc FILE` the model is loaded from and saved back to FILE after
//! every change, so the app (or a later session) sees the same model.

use crate::render::{self, RenderOpts};
use crate::Engine;
use base64::Engine as _;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const REFERENCE: &str = r#"Riseline commands (JSON objects). Units meters, Z up. Add "as":"name" to remember a result id; use "@name" anywhere an id goes.
- rect {from:[x,y,z], to:[x,y,z]}: axis-aligned rectangle; corners share one coordinate. Drawn inside an existing coplanar face it cuts a hole there (windows, doors).
- polygon {points:[[x,y,z],...]}: planar face (footprint, gable end, stair profile).
- line {points}: edges; closing the loop makes a face.
- pushpull {face, distance}: + pulls out along the face normal, - pushes in. Pushing a face that fills a hole recesses it (window); pulling the top of a box again just raises it.
- pick {at:[x,y,z], facing?:[x,y,z]}: id of the face under a point.
- faces {group?, facing?}: list faces (id, normal, center, area, holes, group, material).
- move {ids, by}; copy {group, by, name?}; delete {ids}; reverse {ids}
- begin_group {name} / end_group: new geometry goes in that group (groups don't weld to each other). group {ids, name?}
- paint {ids, color:[r,g,b] 0..1}: faces or whole groups.
- undo / redo; info; new
- save {path} / open {path} (.riseline JSON); export_obj {path}; render {out, view?, camera?, width?, height?}
Ground faces face +Z. A rect on the ground at z=0 pulled up makes a building. Holes must be strictly inside the face (not touching its edge)."#;

fn mtime(doc: &Option<String>) -> Option<std::time::SystemTime> {
    doc.as_ref().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

pub fn serve(mut engine: Engine, doc: Option<String>) {
    let mut seen = mtime(&doc);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => Some(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "riseline", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Riseline is a push/pull 3D modeler. Build with riseline_run, check your work with riseline_render (look at the image), iterate."
            })),
            "ping" => Some(json!({})),
            "tools/list" => Some(json!({"tools": tools()})),
            "tools/call" => {
                // the app (or a person) may have changed the model since our last call
                let now = mtime(&doc);
                if now.is_some() && now != seen {
                    engine.exec_json(&json!({"cmd": "open", "path": doc}).to_string());
                }
                let r = call(&mut engine, &doc, &msg["params"]);
                seen = mtime(&doc);
                Some(r)
            }
            _ => None,
        };
        let Some(id) = id else { continue }; // notification
        let reply = match result {
            Some(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            None => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("unknown method {method}")}}),
        };
        writeln!(out, "{reply}").ok();
        out.flush().ok();
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "riseline_run",
            "description": format!("Run Riseline modeling commands in order and return each reply. Stops at the first error (the failed command changes nothing).\n\n{REFERENCE}"),
            "inputSchema": {
                "type": "object",
                "properties": {"commands": {"type": "array", "items": {"type": "object"}, "description": "Command objects, e.g. {\"cmd\":\"rect\",\"from\":[0,0,0],\"to\":[8,12,0],\"as\":\"lot\"}"}},
                "required": ["commands"]
            }
        },
        {
            "name": "riseline_render",
            "description": "Render the current model and return the image. view: iso (default), front, back, left, right, top; or camera {eye, target, fov}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "view": {"type": "string"},
                    "camera": {"type": "object", "properties": {"eye": {"type": "array"}, "target": {"type": "array"}, "fov": {"type": "number"}}},
                    "width": {"type": "integer"},
                    "height": {"type": "integer"},
                    "edges": {"type": "boolean"}
                }
            }
        },
        {
            "name": "riseline_info",
            "description": "Model summary: counts, bounds, groups, materials, and every face (id, normal, center, area).",
            "inputSchema": {"type": "object", "properties": {}}
        }
    ])
}

fn text(t: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": t}], "isError": is_error})
}

fn call(engine: &mut Engine, doc: &Option<String>, params: &Value) -> Value {
    let args = &params["arguments"];
    match params["name"].as_str().unwrap_or("") {
        "riseline_run" => {
            let Some(cmds) = args["commands"].as_array() else { return text("commands must be an array".into(), true) };
            let mut lines = Vec::new();
            let mut failed = false;
            for c in cmds {
                let r = engine.exec_json(&c.to_string());
                lines.push(r.to_string());
                if r["ok"] != true {
                    failed = true;
                    lines.push(format!("stopped at: {c}"));
                    break;
                }
            }
            if let Some(path) = doc {
                if let Some(dir) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                engine.exec_json(&json!({"cmd": "save", "path": path}).to_string());
            }
            text(lines.join("\n"), failed)
        }
        "riseline_render" => {
            let mut opts: RenderOpts = serde_json::from_value(args.clone()).unwrap_or_default();
            opts.width = opts.width.min(1600);
            opts.height = opts.height.min(1200);
            let img = render::render(&engine.model, &opts);
            let mut png = std::io::Cursor::new(Vec::new());
            if let Err(e) = img.write_to(&mut png, image::ImageFormat::Png) {
                return text(e.to_string(), true);
            }
            let cam = render::camera_for(&engine.model, &opts);
            json!({"content": [
                {"type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(png.into_inner())},
                {"type": "text", "text": format!("camera eye {:?} target {:?} fov {}", cam.eye, cam.target, cam.fov)}
            ]})
        }
        "riseline_info" => {
            let info = engine.exec_json(r#"{"cmd":"info"}"#);
            let faces = engine.exec_json(r#"{"cmd":"faces"}"#);
            text(format!("{info}\n{}", faces["faces"]), false)
        }
        other => text(format!("unknown tool {other}"), true),
    }
}
