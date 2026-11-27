//! Browser bridge: the same three calls the desktop app uses, over HTTP on
//! 127.0.0.1 only. Lets the UI run in a normal browser (and lets us test it
//! there). It is a development tool: the desktop app does not start it.
//!
//!   POST /exec   body: a command          -> reply   (?as=plugin: run it as plugin code)
//!   GET  /scene                           -> scene for the viewer
//!   GET  /sync                            -> true if the model was reloaded
//!
//! Every request must carry the launch's secret in `X-Sketchcraft-Token`. The
//! token is random per launch and written to a file only its owner can read
//! (see docs/dev/building.md), so a web page, another local user or a random
//! local program can't drive the model.

use crate::live::Live;
use crate::policy::Caller;
use serde_json::{json, Value};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Response, Server};

/// The dev page's address: the one website allowed to call the bridge from a browser.
pub const DEFAULT_PAGE: &str = "http://127.0.0.1:5174";
/// Largest command accepted (a video frame or an imported file as text).
const MAX_BODY: u64 = 64 << 20;
/// The request header that carries the token.
pub const TOKEN_HEADER: &str = "X-Sketchcraft-Token";

/// Everything `allowed` needs to know about the launch.
pub struct Access {
    pub token: String,
    /// the dev page's origin (default `DEFAULT_PAGE`); `localhost` spelling of it is accepted too
    pub page: String,
    pub port: u16,
}

fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Who may call: carrying the token; from no Origin (curl, scripts) or the
/// dev page; and only through 127.0.0.1 / localhost on this port (a
/// DNS-rebinding site would arrive with its own host name).
pub fn allowed(access: &Access, origin: Option<&str>, host: Option<&str>, token: Option<&str>) -> bool {
    let page_localhost = access.page.replacen("127.0.0.1", "localhost", 1);
    let origin_ok = match origin {
        None => true,
        Some(o) => o == access.page || o == page_localhost,
    };
    let host_ok = match host {
        None => false,
        Some(h) => h == format!("127.0.0.1:{}", access.port) || h == format!("localhost:{}", access.port),
    };
    let token_ok = token.is_some_and(|t| !access.token.is_empty() && same_token(t, &access.token));
    origin_ok && host_ok && token_ok
}

/// The page origin to name in `Access-Control-Allow-Origin`: the one the request came from when it is the dev page
/// under either spelling (127.0.0.1 or localhost), so a page opened at http://localhost:5174 can read the replies
/// (naming only the 127.0.0.1 spelling made the browser drop every reply: no tool could draw); otherwise the
/// dev page itself, which no other origin matches.
pub fn allow_origin(access: &Access, origin: Option<&str>) -> String {
    let page_localhost = access.page.replacen("127.0.0.1", "localhost", 1);
    match origin {
        Some(o) if o == page_localhost => page_localhost,
        _ => access.page.clone(),
    }
}

/// 32 hex characters from the operating system's randomness.
pub fn new_token() -> String {
    let mut bytes = [0u8; 16];
    let from_os = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).is_ok();
    if !from_os {
        // no /dev/urandom (Windows): std's hasher keys come from the OS's randomness
        use std::hash::{BuildHasher, Hasher};
        for (i, chunk) in bytes.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i as u64 ^ std::process::id() as u64);
            chunk.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the launch's token is written, unless told otherwise: beside the model.
pub fn default_token_file() -> PathBuf {
    crate::live::default_doc().parent().map(|p| p.join(".bridge-token")).unwrap_or_else(|| PathBuf::from(".bridge-token"))
}

/// Write the token where only this user can read it.
pub fn write_token_file(path: &Path, token: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::remove_file(path).ok();
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write as _;
    opts.open(path)?.write_all(token.as_bytes())
}

pub struct Options {
    pub port: u16,
    /// the dev page's origin
    pub page: String,
    /// where to write the token (None: `default_token_file`)
    pub token_file: Option<PathBuf>,
    /// use this token instead of making one (tests, scripts)
    pub token: Option<String>,
}

pub fn serve(live: Live, port: u16) -> anyhow::Result<()> {
    serve_with(live, Options { port, page: DEFAULT_PAGE.into(), token_file: None, token: std::env::var("SKETCHCRAFT_BRIDGE_TOKEN").ok().filter(|t| !t.is_empty()) })
}

/// `%20` and `+` in a query value.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn serve_with(mut live: Live, opts: Options) -> anyhow::Result<()> {
    let port = opts.port;
    let token = opts.token.unwrap_or_else(new_token);
    let token_file = opts.token_file.unwrap_or_else(default_token_file);
    let access = Access { token: token.clone(), page: opts.page.clone(), port };
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    write_token_file(&token_file, &token).map_err(|e| anyhow::anyhow!("couldn't write the token file {}: {e}", token_file.display()))?;
    // the token file goes when the bridge stops (and only if it is still ours)
    struct TokenFile(PathBuf, String);
    impl Drop for TokenFile {
        fn drop(&mut self) {
            if std::fs::read_to_string(&self.0).is_ok_and(|t| t.trim() == self.1) {
                std::fs::remove_file(&self.0).ok();
            }
        }
    }
    let _token_file = TokenFile(token_file.clone(), token.clone());
    eprintln!("sketchcraft bridge on http://127.0.0.1:{port}  (model: {})", live.doc.display());
    eprintln!("token file: {} (send its contents as {TOKEN_HEADER}; the dev page reads it for you)", token_file.display());
    loop {
        // wake up now and then even when nobody asks for anything, so a plugin run nobody closed is
        // closed and saved on time (the 150 s limit) and not only when the next request comes
        let mut req = match server.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(Some(r)) => r,
            Ok(None) => {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.housekeeping())).ok();
                continue;
            }
            Err(_) => break,
        };
        let header = |name: &str| req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string());
        let (origin, host, supplied) = (header("Origin"), header("Host"), header(TOKEN_HEADER));
        // a preflight carries no token (browsers never send one); it only learns what's allowed
        let preflight = *req.method() == Method::Options && origin.as_deref().is_some_and(|o| o == access.page || o == access.page.replacen("127.0.0.1", "localhost", 1));
        if !preflight && !allowed(&access, origin.as_deref(), host.as_deref(), supplied.as_deref()) {
            req.respond(Response::from_string("{\"ok\":false,\"error\":\"forbidden\"}").with_status_code(403)).ok();
            continue;
        }
        let url = req.url().to_string();
        let (path, query) = url.split_once('?').unwrap_or((&url, ""));
        let caller = if query.split('&').any(|q| q == "as=plugin") { Caller::Plugin } else { Caller::Ui };
        // the plugin the page is running this for (the page names it; the plugin itself can't)
        let from: Option<String> = query.split('&').find_map(|q| q.strip_prefix("plugin=")).map(percent_decode);
        let body: Value = match (req.method(), path) {
            (Method::Options, _) => json!({}),
            (Method::Post, "/exec") => {
                let mut text = String::new();
                if req.body_length().is_some_and(|n| n as u64 > MAX_BODY) {
                    req.respond(Response::from_string("{\"ok\":false,\"error\":\"too large\"}").with_status_code(413)).ok();
                    continue;
                }
                req.as_reader().take(MAX_BODY).read_to_string(&mut text).ok();
                match serde_json::from_str::<Value>(&text) {
                    // a bug in one command answers with an error, not a dead bridge
                    Ok(cmd) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.exec_as_from(&caller, &cmd, from.as_deref()))).unwrap_or_else(|_| json!({"ok": false, "error": "SketchCraft hit a bug running that."})),
                    Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
                }
            }
            // reads are guarded like commands: a bug on a damaged model answers with an error, not a dead bridge
            (Method::Get, "/scene") => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.scene())).unwrap_or_else(|_| json!({"ok": false, "error": "SketchCraft couldn't draw this model."})),
            (Method::Get, "/sync") => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.sync())).map(|b| json!(b)).unwrap_or_else(|_| json!(false)),
            _ => json!({"ok": false, "error": "not found"}),
        };
        let resp = Response::from_string(body.to_string())
            .with_header("Content-Type: application/json".parse::<Header>().unwrap())
            .with_header(format!("Access-Control-Allow-Origin: {}", allow_origin(&access, origin.as_deref())).parse::<Header>().unwrap())
            .with_header("Vary: Origin".parse::<Header>().unwrap())
            .with_header(format!("Access-Control-Allow-Headers: content-type, {TOKEN_HEADER}").parse::<Header>().unwrap())
            .with_header("Cache-Control: no-store".parse::<Header>().unwrap());
        req.respond(resp).ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access() -> Access {
        Access { token: "abc123".into(), page: DEFAULT_PAGE.into(), port: 5175 }
    }

    #[test]
    fn needs_the_token() {
        let a = access();
        assert!(allowed(&a, None, Some("127.0.0.1:5175"), Some("abc123")));
        assert!(!allowed(&a, None, Some("127.0.0.1:5175"), None));
        assert!(!allowed(&a, None, Some("127.0.0.1:5175"), Some("abc124")));
        assert!(!allowed(&a, None, Some("127.0.0.1:5175"), Some("")));
    }

    #[test]
    fn only_the_dev_page_and_this_host() {
        let a = access();
        assert!(allowed(&a, Some(DEFAULT_PAGE), Some("localhost:5175"), Some("abc123")));
        assert!(!allowed(&a, Some("https://evil.example"), Some("127.0.0.1:5175"), Some("abc123")));
        assert!(!allowed(&a, Some("null"), Some("127.0.0.1:5175"), Some("abc123")));
        assert!(!allowed(&a, None, Some("evil.example:5175"), Some("abc123")));
        // another port's page isn't trusted
        assert!(!allowed(&a, Some("http://127.0.0.1:8080"), Some("127.0.0.1:5175"), Some("abc123")));
    }

    #[test]
    fn the_dev_page_is_answered_under_either_spelling() {
        let a = access();
        let local = DEFAULT_PAGE.replacen("127.0.0.1", "localhost", 1);
        assert_ne!(local, DEFAULT_PAGE, "the default page is the 127.0.0.1 spelling");
        assert_eq!(allow_origin(&a, Some(DEFAULT_PAGE)), DEFAULT_PAGE);
        assert_eq!(allow_origin(&a, Some(&local)), local, "a page opened at localhost reads its replies");
        // anyone else is named nothing they match
        assert_eq!(allow_origin(&a, Some("https://evil.example")), DEFAULT_PAGE);
        assert_eq!(allow_origin(&a, Some("http://localhost:8080")), DEFAULT_PAGE);
        assert_eq!(allow_origin(&a, None), DEFAULT_PAGE);
    }

    #[test]
    fn tokens_are_random() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
