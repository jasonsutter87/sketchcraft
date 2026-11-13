//! A model shared through a file: whoever changes it saves it, everyone else
//! reloads when the file's timestamp moves. That's how the app, the browser
//! bridge and Claude (over MCP) all edit one model at once.

use crate::Engine;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::SystemTime;

pub const READ_ONLY: &[&str] = &["faces", "info", "pick", "render", "save", "export_obj"];

pub struct Live {
    pub engine: Engine,
    pub doc: PathBuf,
    seen: Option<SystemTime>,
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub fn default_doc() -> PathBuf {
    if let Ok(p) = std::env::var("RISELINE_DOC") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Documents/Riseline/live.riseline")
}

impl Live {
    pub fn open(doc: PathBuf) -> Self {
        let mut engine = Engine::new();
        if doc.exists() {
            engine.exec_json(&json!({"cmd": "open", "path": doc}).to_string());
        }
        let seen = mtime(&doc);
        Self { engine, doc, seen }
    }

    /// Run a command; save the model if it changed.
    pub fn exec(&mut self, cmd: &Value) -> Value {
        let r = self.engine.exec_json(&cmd.to_string());
        let name = cmd["cmd"].as_str().unwrap_or("");
        if r["ok"] == true && !READ_ONLY.contains(&name) {
            if let Some(dir) = self.doc.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            self.engine.exec_json(&json!({"cmd": "save", "path": self.doc}).to_string());
            self.seen = mtime(&self.doc);
        }
        r
    }

    /// Reload if someone else saved since we last looked. True if reloaded.
    pub fn sync(&mut self) -> bool {
        let now = mtime(&self.doc);
        if now.is_some() && now != self.seen {
            self.seen = now;
            let r = self.engine.exec_json(&json!({"cmd": "open", "path": self.doc}).to_string());
            return r["ok"] == true;
        }
        false
    }

    pub fn scene(&self) -> Value {
        let mut s = crate::scene::build(&self.engine.model, self.engine.ctx);
        s["doc"] = json!(self.doc);
        s
    }
}
