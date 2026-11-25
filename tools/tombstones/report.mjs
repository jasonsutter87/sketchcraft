// Step 3: after the test runs. Reads the hits (and/or lcov coverage), removes the marks that were hit (only the mark,
// never the code under it), and writes the report of what is left: report.md and report.json.
import fs from "node:fs";
import path from "node:path";
import { MARK_RE, STATE_DIR, allowReason, lcovHits, parseAllowlist, parseLcov, readHits, readManifest } from "./lib/common.mjs";
import { unrewriteRust } from "./rust-rewrite.mjs";

export function buildReport(manifest, hit, { allowRules = [], judged = null, sources = [] } = {}) {
  const remaining = manifest.entries.filter((e) => !hit.has(e.id) && (!judged || judged.has(e.id)));
  const candidates = [], byHand = [];
  for (const e of remaining) {
    const reason = allowReason(allowRules, e);
    const row = { file: e.file, line: e.line, endLine: e.endLine, symbol: e.symbol, kind: e.kind, id: e.id, lang: e.lang, deprecatable: e.deprecatable };
    if (reason) byHand.push({ ...row, reason }); else candidates.push(row);
  }
  const by = (a, b) => a.file.localeCompare(b.file) || a.line - b.line;
  candidates.sort(by); byHand.sort(by);
  return {
    generatedAt: new Date().toISOString(), base: manifest.base, sources,
    totals: { marks: manifest.entries.length, hit: manifest.entries.filter((e) => hit.has(e.id)).length, candidates: candidates.length, byHand: byHand.length },
    candidates, byHand,
  };
}

export function renderMarkdown(r) {
  const L = [];
  L.push("# Tombstone report", "");
  L.push(`Base commit \`${(r.base || "unknown").slice(0, 10)}\`, generated ${r.generatedAt}. Inputs: ${r.sources.join(", ") || "none"}.`, "");
  L.push("| marks | reached | never reached (candidates) | check by hand |", "|---:|---:|---:|---:|");
  L.push(`| ${r.totals.marks} | ${r.totals.hit} | ${r.totals.candidates} | ${r.totals.byHand} |`, "");
  L.push("A candidate is code no test run reached. It is a lead, not a verdict: look at it, then deprecate it (see docs/dev/tombstones.md).", "");
  L.push("## Never reached by any run", "");
  if (!r.candidates.length) L.push("Nothing.", "");
  else {
    L.push("| file | line | symbol | kind | id |", "|---|---:|---|---|---|");
    for (const c of r.candidates) L.push(`| \`${c.file}\` | ${c.line} | \`${c.symbol}\` | ${c.kind} | \`${c.id}\` |`);
    L.push("");
  }
  L.push("## Check by hand", "", "Not reached, but in a place the tests cannot reach (rare menus, old file formats, Android and mobile, native-only code). Never removed automatically.", "");
  if (!r.byHand.length) L.push("Nothing.", "");
  else {
    L.push("| file | line | symbol | why by hand | id |", "|---|---:|---|---|---|");
    for (const c of r.byHand) L.push(`| \`${c.file}\` | ${c.line} | \`${c.symbol}\` | ${c.reason} | \`${c.id}\` |`);
    L.push("");
  }
  return L.join("\n");
}

/** Remove the comment marks whose id was hit (the whole mark line only). Returns the number removed. */
export function stripHitMarks(root, manifest, hit) {
  let removed = 0;
  for (const rel of [...new Set(manifest.entries.map((e) => e.file))]) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) continue;
    const src = fs.readFileSync(abs, "utf8");
    const out = src.split("\n").filter((l) => { const m = MARK_RE.exec(l); if (m && hit.has(m[2])) { removed++; return false; } return true; });
    if (out.length !== src.split("\n").length) fs.writeFileSync(abs, out.join("\n"));
  }
  return removed;
}

/**
 * @param {string} root the throwaway checkout
 * @param {{hits?:string[], lcov?:string[], allow?:string, out?:string, strip?:boolean, allowEmpty?:boolean}} o
 */
export function runReport(root, { hits = [], lcov = [], allow, out, strip = true, allowEmpty = false } = {}) {
  const manifest = readManifest(root);
  const sources = [];
  const hit = readHits(hits);
  const logHits = new Set(hit);
  if (hits.length) sources.push(`hits log (${hit.size} ids)`);
  let judged = null;
  if (lcov.length) {
    judged = new Set();
    for (const f of lcov) {
      const r = lcovHits(manifest, parseLcov(fs.readFileSync(f, "utf8")), root);
      for (const id of r.hit) hit.add(id);
      for (const id of r.judged) judged.add(id);
      sources.push(`lcov ${path.basename(f)} (${r.judged.size} marks decided)`);
    }
    // a language the hits log has anything for was run by the probes: all its marks are decided
    const probed = new Set(manifest.entries.filter((e) => logHits.has(e.id)).map((e) => e.lang));
    for (const e of manifest.entries) if (probed.has(e.lang) || hit.has(e.id)) judged.add(e.id);
  }
  if (!hit.size && !allowEmpty) {
    throw new Error("no hits at all: the tests did not write to the hits log (is SKETCHCRAFT_TOMB_LOG set for the run?). Refusing to call everything dead; pass --allow-empty to override.");
  }
  const rules = allow && fs.existsSync(allow) ? parseAllowlist(fs.readFileSync(allow, "utf8")) : [];
  const report = buildReport(manifest, hit, { allowRules: rules, judged: judged && judged.size ? judged : null, sources });
  let removed = 0;
  if (strip) {
    unrewriteRust(root, hit);
    removed = stripHitMarks(root, manifest, hit);
  }
  const dir = out ?? path.join(root, STATE_DIR);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "report.json"), JSON.stringify(report, null, 1) + "\n");
  fs.writeFileSync(path.join(dir, "report.md"), renderMarkdown(report));
  return { report, removed };
}
