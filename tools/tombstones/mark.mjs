// Step 1: put `// @debug tomb:<id>` marks into a throwaway checkout and write the manifest.
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { applyMarks, globToRegex, loadTs, walk, writeManifest } from "./lib/common.mjs";
import { findTs } from "./lib/ts.mjs";
import { findRust } from "./lib/rust.mjs";

export function tsFiles(root, only) {
  const dir = path.join(root, "app", "src");
  if (!fs.existsSync(dir)) return [];
  return walk(dir, [".ts", ".tsx"])
    .map((f) => path.relative(root, f).split(path.sep).join("/"))
    .filter((f) => !/\.d\.ts$|\.test\.tsx?$/.test(f))
    .filter((f) => !only.length || only.some((r) => r.test(f)));
}

/** Rust files that are part of the library crate (the probe macro lives in it): declared in lib.rs, not tests. */
export function rustFiles(root, only) {
  const dir = path.join(root, "core", "src");
  if (!fs.existsSync(dir)) return [];
  const lib = fs.readFileSync(path.join(dir, "lib.rs"), "utf8");
  const declared = new Set([...lib.matchAll(/\bmod\s+([A-Za-z0-9_]+)\s*;/g)].map((m) => m[1]));
  return walk(dir, [".rs"])
    .map((f) => path.relative(root, f).split(path.sep).join("/"))
    .filter((f) => {
      const rel = f.slice("core/src/".length);
      if (rel === "main.rs" || rel === "lib.rs" || rel === "__tomb.rs" || rel.startsWith("bin/")) return false;
      if (/(^|\/)tests?\.rs$|_tests\.rs$/.test(rel)) return false;
      return declared.has(rel.split("/")[0].replace(/\.rs$/, ""));
    })
    .filter((f) => !only.length || only.some((r) => r.test(f)));
}

function git(root, args) {
  try { return execFileSync("git", ["-C", root, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim(); } catch { return ""; }
}

/**
 * @param {string} root a throwaway checkout (its branch must start with "tombstone" unless force)
 * @param {{langs?:string[], only?:string[], blocks?:boolean, force?:boolean}} opts
 */
export function mark(root, { langs = ["ts", "rust"], only = [], blocks = false, force = false } = {}) {
  const branch = git(root, ["rev-parse", "--abbrev-ref", "HEAD"]);
  if (!force && branch && !/^tombstone/.test(branch)) {
    throw new Error(`refusing to put marks in branch "${branch}": use a throwaway checkout on a branch named tombstone/... (or pass --force)`);
  }
  const onlyRe = only.map(globToRegex);
  const entries = [];
  const jobs = [];
  if (langs.includes("ts")) for (const f of tsFiles(root, onlyRe)) jobs.push(["ts", f]);
  if (langs.includes("rust")) for (const f of rustFiles(root, onlyRe)) jobs.push(["rust", f]);
  const ts = langs.includes("ts") ? loadTs(root) : null;
  for (const [lang, rel] of jobs) {
    const abs = path.join(root, rel);
    const src = fs.readFileSync(abs, "utf8");
    if (src.includes("@debug tomb:")) continue; // already marked
    const targets = lang === "ts" ? findTs(ts, src, rel, { blocks }) : findRust(src, { blocks });
    if (!targets.length) continue;
    const r = applyMarks(src, rel, targets);
    fs.writeFileSync(abs, r.src);
    // line numbers in the report are the original ones (what a person sees on the base branch)
    for (const e of r.entries) entries.push({ ...e, lang });
  }
  const ids = new Set();
  for (const e of entries) {
    if (ids.has(e.id)) throw new Error(`id collision on ${e.id} (${e.file} ${e.symbol})`);
    ids.add(e.id);
  }
  const manifest = { version: 1, createdAt: new Date().toISOString(), base: git(root, ["rev-parse", "HEAD"]), blocks, entries };
  writeManifest(root, manifest);
  return manifest;
}
