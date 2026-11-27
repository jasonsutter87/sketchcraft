#!/usr/bin/env node
// Tombstones: find code no test reaches. See docs/dev/tombstones.md.
//
//   node tools/tombstones/tomb.mjs mark --root <checkout> [--lang ts,rust] [--only <glob,glob>] [--blocks] [--force]
//   node tools/tombstones/tomb.mjs rewrite-rust --root <checkout>
//   node tools/tombstones/tomb.mjs report --root <checkout> [--hits <log>]... [--lcov <file>]... [--allow <file>] [--no-strip]
//   node tools/tombstones/tomb.mjs deprecate --root <checkout> --remove-on MM-DD-YYYY --remove-in <version> [--ids a,b] [--write]
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { mark } from "./mark.mjs";
import { rewriteRust } from "./rust-rewrite.mjs";
import { runReport } from "./report.mjs";
import { deprecate } from "./deprecate.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));

export function parseArgs(argv) {
  const o = { _: [], hits: [], lcov: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) { o._.push(a); continue; }
    const k = a.slice(2);
    if (["blocks", "force", "no-strip", "write", "allow-empty"].includes(k)) o[k] = true;
    else {
      const v = argv[++i];
      if (v === undefined) throw new Error(`--${k} needs a value`);
      if (k === "hits" || k === "lcov") o[k].push(v); else o[k] = v;
    }
  }
  return o;
}

function main() {
  const o = parseArgs(process.argv.slice(2));
  const cmd = o._[0];
  const root = path.resolve(o.root ?? ".");
  const csv = (s) => (s ? s.split(",").map((x) => x.trim()).filter(Boolean) : undefined);
  if (cmd === "mark") {
    const m = mark(root, { langs: csv(o.lang), only: csv(o.only), blocks: !!o.blocks, force: !!o.force });
    const by = {};
    for (const e of m.entries) by[e.lang] = (by[e.lang] ?? 0) + 1;
    console.log(`marked ${m.entries.length} places`, JSON.stringify(by), `-> ${path.join(root, ".tombstones/manifest.json")}`);
  } else if (cmd === "rewrite-rust") {
    console.log(`rewrote marks in ${rewriteRust(root)} Rust files; core/src/__tomb.rs added`);
  } else if (cmd === "report") {
    const allow = o.allow ?? path.join(here, "by-hand.allow");
    const hits = o.hits.length ? o.hits : [process.env.SKETCHCRAFT_TOMB_LOG ?? path.join(root, "tomb-hits.log")];
    const { report, removed } = runReport(root, { hits, lcov: o.lcov, allow, out: o.out, strip: !o["no-strip"], allowEmpty: !!o["allow-empty"] });
    console.log(`removed ${removed} hit marks; ${report.totals.candidates} candidates, ${report.totals.byHand} to check by hand`);
  } else if (cmd === "deprecate") {
    const pkg = JSON.parse(fs.readFileSync(path.join(root, "app", "package.json"), "utf8"));
    const [a, b, c] = pkg.version.split(".").map(Number);
    const r = deprecate(root, { date: o["remove-on"], version: o["remove-in"] ?? `${a}.${b}.${c + 1}`, ids: csv(o.ids), write: !!o.write, report: o.report });
    for (const e of r.applied) console.log(`${r.written ? "deprecated" : "would deprecate"} ${e.file}:${e.line} ${e.symbol} (${e.id})`);
    for (const e of r.skipped) console.log(`skipped ${e.file}:${e.line} ${e.symbol}: ${e.why}`);
    if (!r.written) console.log("dry run: add --write to change files");
  } else {
    console.error("usage: tomb.mjs mark|rewrite-rust|report|deprecate --root <throwaway checkout> (see the header of this file)");
    process.exit(2);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (e) { console.error(`tombstones: ${e.message}`); process.exit(1); }
}
