// Step 2 (Rust): the pre-compile rewrite. Marks become `crate::__tomb!("<id>");`, and a small module
// (core/src/__tomb.rs) is added to the library. Run it only in the throwaway checkout, right before `cargo test`.
// Each probe site is a static AtomicBool, so after its first hit it costs one relaxed swap; the log gets one line per id
// per test process. The log path is the env var SKETCHCRAFT_TOMB_LOG.
import fs from "node:fs";
import path from "node:path";
import { MARK_RE, RUST_PROBE_RE, readManifest } from "./lib/common.mjs";

export const TOMB_RS = `// tombstone probes (tools/tombstones): only exists in a throwaway tombstone checkout.
use std::io::Write;

#[doc(hidden)]
pub fn write_hit(id: &str) {
    if let Ok(p) = std::env::var("SKETCHCRAFT_TOMB_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            // one write call, so lines from parallel tests never interleave
            let _ = f.write_all(format!("{id}\\n").as_bytes());
        }
    }
}

#[macro_export]
macro_rules! __tomb {
    ($id:literal) => {{
        static HIT: ::std::sync::atomic::AtomicBool = ::std::sync::atomic::AtomicBool::new(false);
        if !HIT.swap(true, ::std::sync::atomic::Ordering::Relaxed) {
            $crate::__tomb::write_hit($id);
        }
    }};
}
`;
export const LIB_LINE = "#[doc(hidden)] pub mod __tomb; // tomb-injected";

export function rewriteSource(src) {
  return src.split("\n").map((l) => { const m = MARK_RE.exec(l); return m ? `${m[1]}crate::__tomb!("${m[2]}");` : l; }).join("\n");
}
/** Probes back to marks (those `keep` says are still unreached) or gone (those that were hit). */
export function revertSource(src, keep) {
  const out = [];
  for (const l of src.split("\n")) {
    const m = RUST_PROBE_RE.exec(l);
    if (!m) { out.push(l); continue; }
    if (keep(m[2])) out.push(`${m[1]}// @debug tomb:${m[2]}`);
  }
  return out.join("\n");
}

function rustFilesOf(root) {
  return [...new Set(readManifest(root).entries.filter((e) => e.lang === "rust").map((e) => e.file))];
}

export function rewriteRust(root) {
  const files = rustFilesOf(root);
  for (const rel of files) {
    const abs = path.join(root, rel);
    fs.writeFileSync(abs, rewriteSource(fs.readFileSync(abs, "utf8")));
  }
  const dir = path.join(root, "core", "src");
  fs.writeFileSync(path.join(dir, "__tomb.rs"), TOMB_RS);
  const libp = path.join(dir, "lib.rs");
  const lib = fs.readFileSync(libp, "utf8");
  if (!lib.includes("tomb-injected")) fs.writeFileSync(libp, lib.replace(/\n*$/, "\n") + LIB_LINE + "\n");
  return files.length;
}

/** Undo the rewrite: probes back to marks (those not in `hit`) or gone (those hit); remove the module. */
export function unrewriteRust(root, hit) {
  for (const rel of rustFilesOf(root)) {
    const abs = path.join(root, rel);
    fs.writeFileSync(abs, revertSource(fs.readFileSync(abs, "utf8"), (id) => !hit.has(id)));
  }
  const dir = path.join(root, "core", "src");
  fs.rmSync(path.join(dir, "__tomb.rs"), { force: true });
  const libp = path.join(dir, "lib.rs");
  if (fs.existsSync(libp)) {
    const lib = fs.readFileSync(libp, "utf8");
    fs.writeFileSync(libp, lib.split("\n").filter((l) => !l.includes("tomb-injected")).join("\n"));
  }
}
