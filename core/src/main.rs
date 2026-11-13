//! riseline CLI
//!
//!   riseline run <script.jsonl> [--open model.riseline] [--save model.riseline]
//!       Run one JSON command per line (blank lines and # comments skipped),
//!       print each reply. Stops at the first error unless --keep-going.
//!
//!   riseline serve [--open model.riseline]
//!       Read commands from stdin, write one JSON reply per line to stdout.
//!       This is the headless protocol the app speaks.
//!
//!   riseline mcp [--doc model.riseline]
//!       MCP server on stdio for AI agents. With --doc, the model is loaded
//!       from FILE (if it exists) and saved back after every change.

use riseline::Engine;
use serde_json::json;
use std::io::{BufRead, Write};

fn usage() -> ! {
    eprintln!("usage:\n  riseline run <script.jsonl> [--open FILE] [--save FILE] [--keep-going]\n  riseline serve [--open FILE]\n  riseline mcp [--doc FILE]");
    std::process::exit(2)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut engine = Engine::new();
    if let Some(path) = flag(&args, "--open") {
        let r = engine.exec_json(&json!({"cmd": "open", "path": path}).to_string());
        if r["ok"] != true {
            eprintln!("{r}");
            std::process::exit(1);
        }
    }
    match args.first().map(String::as_str) {
        Some("run") => {
            let Some(script) = args.get(1) else { usage() };
            let text = std::fs::read_to_string(script).unwrap_or_else(|e| {
                eprintln!("can't read {script}: {e}");
                std::process::exit(1)
            });
            let keep_going = args.iter().any(|a| a == "--keep-going");
            let mut failed = false;
            for (n, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let r = engine.exec_json(line);
                println!("{}: {}", n + 1, r);
                if r["ok"] != true {
                    failed = true;
                    if !keep_going {
                        break;
                    }
                }
            }
            if let Some(path) = flag(&args, "--save") {
                println!("{}", engine.exec_json(&json!({"cmd": "save", "path": path}).to_string()));
            }
            std::process::exit(if failed { 1 } else { 0 });
        }
        Some("serve") => {
            let stdin = std::io::stdin();
            let mut out = std::io::stdout().lock();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let r = engine.exec_json(line);
                writeln!(out, "{r}").ok();
                out.flush().ok();
            }
        }
        Some("mcp") => {
            let doc = flag(&args, "--doc");
            if let Some(path) = &doc {
                if std::path::Path::new(path).exists() {
                    engine.exec_json(&json!({"cmd": "open", "path": path}).to_string());
                }
            }
            riseline::mcp::serve(engine, doc);
        }
        _ => usage(),
    }
}
