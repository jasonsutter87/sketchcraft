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

pub fn serve(mut live: Live, port: u16) -> anyhow::Result<()> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("sketchcraft bridge on http://127.0.0.1:{port}  (model: {})", live.doc.display());
    for mut req in server.incoming_requests() {
        let body: Value = match (req.method(), req.url()) {
            (Method::Options, _) => json!({}),
            (Method::Post, "/exec") => {
                let mut text = String::new();
                req.as_reader().read_to_string(&mut text).ok();
                match serde_json::from_str::<Value>(&text) {
                    Ok(cmd) => live.exec(&cmd),
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
