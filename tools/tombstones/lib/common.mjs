// Shared pieces of the tombstone tooling (docs/dev/tombstones.md): mark format, ids, manifest, allowlist, hits, lcov.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

/** `// @debug tomb:<id>` on a line of its own (the tag the issue asks for). */
export const MARK_RE = /^([ \t]*)\/\/ @debug tomb:([0-9a-f]{8})[ \t]*$/;
/** The same mark after the Rust rewrite (see rust-rewrite.mjs). */
export const RUST_PROBE_RE = /^([ \t]*)crate::__tomb!\("([0-9a-f]{8})"\);[ \t]*$/;
export const STATE_DIR = ".tombstones";

/** Stable across edits elsewhere in the file: file + symbol + kind + n-th of that triple. */
export function makeId(file, symbol, kind, nth) {
  return crypto.createHash("sha1").update(`${file}|${symbol}|${kind}|${nth}`).digest("hex").slice(0, 8);
}

export function lineStarts(src) {
  const a = [0];
  for (let i = 0; i < src.length; i++) if (src.charCodeAt(i) === 10) a.push(i + 1);
  return a;
}
export function lineOf(starts, off) {
  let lo = 0, hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= off) lo = mid; else hi = mid - 1;
  }
  return lo + 1;
}
export function indentOfLineAt(src, off) {
  const s = src.lastIndexOf("\n", off - 1) + 1;
  return /^[ \t]*/.exec(src.slice(s))[0];
}

/**
 * Turn a list of finder targets into mark edits and apply them.
 * A target is { bodyOpen, start, end, symbol, kind, span? }; bodyOpen is the offset of the `{`.
 * Returns { src, entries }.
 */
export function applyMarks(src, relFile, targets) {
  const starts = lineStarts(src);
  const seen = new Map();
  const entries = [];
  const edits = [];
  for (const t of targets) {
    const key = `${t.symbol}|${t.kind}`;
    const nth = seen.get(key) ?? 0;
    seen.set(key, nth + 1);
    const id = makeId(relFile, t.symbol, t.kind, nth);
    const lineIndent = indentOfLineAt(src, t.bodyOpen);
    const nl = src.indexOf("\n", t.bodyOpen);
    const rest = src.slice(t.bodyOpen + 1, nl < 0 ? src.length : nl);
    let text = `\n${lineIndent}  // @debug tomb:${id}`;
    if (rest.trim() !== "") text += `\n${lineIndent}`;
    edits.push({ at: t.bodyOpen + 1, text });
    entries.push({
      id, file: relFile, line: lineOf(starts, t.start ?? t.bodyOpen), endLine: lineOf(starts, t.end ?? t.bodyOpen),
      bodyLine: lineOf(starts, t.bodyOpen), symbol: t.symbol, kind: t.kind, deprecatable: !!t.span,
    });
  }
  edits.sort((a, b) => b.at - a.at);
  let out = src;
  for (const e of edits) out = out.slice(0, e.at) + e.text + out.slice(e.at);
  return { src: out, entries };
}

export function globToRegex(glob) {
  let re = "";
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i];
    if (c === "*") {
      if (glob[i + 1] === "*") { re += ".*"; i++; if (glob[i + 1] === "/") i++; } else re += "[^/]*";
    } else if (c === "?") re += "[^/]";
    else re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${re}$`);
}

/**
 * The "check by hand" list. One rule per line: `<path glob>[::<symbol glob>]   # why`.
 * A mark that matches a rule is never turned into a deprecation; it goes to its own report section.
 */
export function parseAllowlist(text) {
  const rules = [];
  for (const raw of text.split("\n")) {
    const m = /^\s*([^#\s][^#]*?)\s*(?:#\s*(.*))?$/.exec(raw);
    if (!m) continue;
    const [fileGlob, symGlob] = m[1].split("::");
    rules.push({
      file: globToRegex(fileGlob.trim()), sym: symGlob ? globToRegex(symGlob.trim()) : null,
      reason: (m[2] ?? "").trim() || "listed in the allowlist", glob: m[1].trim(),
    });
  }
  return rules;
}
export function allowReason(rules, entry) {
  for (const r of rules) if (r.file.test(entry.file) && (!r.sym || r.sym.test(entry.symbol))) return r.reason;
  return null;
}

/** Hits: one id per line (an optional prefix is ignored), from any number of logs. */
export function readHits(files) {
  const hit = new Set();
  for (const f of files) {
    if (!fs.existsSync(f)) continue;
    for (const line of fs.readFileSync(f, "utf8").split("\n")) {
      const m = /([0-9a-f]{8})\s*$/.exec(line.trim());
      if (m) hit.add(m[1]);
    }
  }
  return hit;
}

/** Minimal lcov reader (cargo llvm-cov --lcov, vitest/c8 coverage): per file, function and line counts. */
export function parseLcov(text) {
  const files = new Map();
  let cur = null;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("SF:")) { cur = { fns: [], das: [] }; files.set(line.slice(3), cur); }
    else if (!cur) continue;
    else if (line.startsWith("FN:")) { const [l, ...n] = line.slice(3).split(","); cur.fns.push({ line: +l, name: n.join(","), hits: 0 }); }
    else if (line.startsWith("FNDA:")) {
      const [c, ...n] = line.slice(5).split(","); const name = n.join(",");
      for (const f of cur.fns) if (f.name === name) f.hits += +c;
    } else if (line.startsWith("DA:")) { const [l, c] = line.slice(3).split(","); cur.das.push({ line: +l, hits: +c }); }
    else if (line === "end_of_record") cur = null;
  }
  return files;
}
/** Which manifest entries does this lcov say ran? `hit` ran; `judged` are the ones it could decide on. */
export function lcovHits(manifest, lcov, root) {
  const hit = new Set(), judged = new Set();
  const byFile = new Map();
  for (const [sf, data] of lcov) {
    const rel = (path.isAbsolute(sf) ? path.relative(root, sf) : sf).split(path.sep).join("/");
    byFile.set(rel, data);
  }
  for (const e of manifest.entries) {
    let data = byFile.get(e.file);
    if (!data) for (const [k, v] of byFile) if (k.endsWith("/" + e.file) || e.file.endsWith("/" + k)) data = v;
    if (!data) continue;
    if (["function", "method", "fn"].includes(e.kind)) {
      const fns = data.fns.filter((f) => f.line >= e.line && f.line <= e.bodyLine);
      if (!fns.length) continue;
      judged.add(e.id);
      if (fns.some((f) => f.hits > 0)) hit.add(e.id);
    } else {
      const das = data.das.filter((d) => d.line > e.bodyLine && d.line <= e.endLine);
      if (!das.length) continue;
      judged.add(e.id);
      if (das.some((d) => d.hits > 0)) hit.add(e.id);
    }
  }
  return { hit, judged };
}

export function walk(dir, exts, out = []) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    if (ent.name === "node_modules" || ent.name === "target" || ent.name.startsWith(".")) continue;
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) walk(p, exts, out);
    else if (exts.some((x) => ent.name.endsWith(x))) out.push(p);
  }
  return out;
}

export function loadTs(root) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  for (const base of [path.join(root, "app", "package.json"), path.join(here, "..", "..", "..", "app", "package.json")]) {
    try { return createRequire(base)("typescript"); } catch { /* try the next place */ }
  }
  throw new Error("typescript not found: run `npm ci` in app/ first");
}

export function writeManifest(root, manifest) {
  const dir = path.join(root, STATE_DIR);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest, null, 1) + "\n");
}
export function readManifest(root) {
  return JSON.parse(fs.readFileSync(path.join(root, STATE_DIR, "manifest.json"), "utf8"));
}
