//! Browser bridge: the same three calls the desktop app uses, over HTTP on
//! 127.0.0.1 only. Lets the UI run in a normal browser (and lets us test it
//! there).
//!
//!   POST /exec   body: a command          -> reply
//!   GET  /scene                           -> scene for the viewer
//!   GET  /sync                            -> true if the model was reloaded

use crate::live::Live;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};

/// The dev page, the only website allowed to use the bridge.
const PAGE: &str = "http://127.0.0.1:5174";
/// Largest command accepted (a video frame or an imported file as text).
const MAX_BODY: u64 = 64 << 20;

/// Who may call: no Origin (curl, scripts, tests) or the dev page; and only
/// through 127.0.0.1 / localhost on this port (a DNS-rebinding site would
/// arrive with its own host name). Other websites the user visits could
/// otherwise send commands here, since a plain POST needs no preflight.
pub fn allowed(origin: Option<&str>, host: Option<&str>, port: u16) -> bool {
    let origin_ok = match origin {
        None => true,
        Some(o) => o == PAGE || o == format!("http://localhost:{}", 5174),
    };
    let host_ok = match host {
        None => false,
        Some(h) => h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"),
    };
    origin_ok && host_ok
}

pub fn serve(mut live: Live, port: u16) -> anyhow::Result<()> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("sketchcraft bridge on http://127.0.0.1:{port}  (model: {})", live.doc.display());
    for mut req in server.incoming_requests() {
        let header = |name: &str| req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string());
        if !allowed(header("Origin").as_deref(), header("Host").as_deref(), port) {
            req.respond(Response::from_string("{\"ok\":false,\"error\":\"forbidden\"}").with_status_code(403)).ok();
            continue;
        }
        let body: Value = match (req.method(), req.url()) {
            (Method::Options, _) => json!({}),
            (Method::Post, "/exec") => {
                let mut text = String::new();
                if req.body_length().is_some_and(|n| n as u64 > MAX_BODY) {
                    req.respond(Response::from_string("{\"ok\":false,\"error\":\"too large\"}").with_status_code(413)).ok();
                    continue;
                }
                use std::io::Read as _;
                req.as_reader().take(MAX_BODY).read_to_string(&mut text).ok();
                match serde_json::from_str::<Value>(&text) {
                    // a bug in one command answers with an error, not a dead bridge
                    Ok(cmd) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.exec(&cmd))).unwrap_or_else(|_| json!({"ok": false, "error": "SketchCraft hit a bug running that. Undo if anything looks wrong."})),
                    Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
                }
            }
            (Method::Get, "/scene") => live.scene(),
            (Method::Get, "/sync") => json!(live.sync()),
            _ => json!({"ok": false, "error": "not found"}),
        };
        let resp = Response::from_string(body.to_string())
            .with_header("Content-Type: application/json".parse::<Header>().unwrap())
            .with_header("Access-Control-Allow-Origin: http://127.0.0.1:5174".parse::<Header>().unwrap())
            .with_header("Access-Control-Allow-Headers: content-type".parse::<Header>().unwrap());
        req.respond(resp).ok();
    }
    Ok(())
}
