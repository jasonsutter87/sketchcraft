// Step 4 (only on request): turn report entries into code-janitor `@deprecate` blocks.
// code-janitor's format (see its README): `// @deprecate MM-DD-YYYY reason: ...` followed by the deprecated code
// commented out; it deletes the block once the date has passed. So a deprecation here comments the function out under
// a header line that names the date, the release it goes in, and the tombstone id. The compiler and the tests then
// show at once if something did reach it after all.
import fs from "node:fs";
import path from "node:path";
import { MARK_RE, STATE_DIR, lineOf, lineStarts, loadTs } from "./lib/common.mjs";
import { findTs } from "./lib/ts.mjs";
import { findRust } from "./lib/rust.mjs";

export function header(indent, { date, version, id, symbol }) {
  return `${indent}// @deprecate ${date} reason: tombstone ${id} (${symbol}) was not reached by any test run; remove in ${version}`;
}

/** Comment out whole lines [fromLine, toLine] (1-based) under a header; pulls in the doc comment right above. */
export function commentOut(src, spanStart, spanEnd, h) {
  const lines = src.split("\n");
  const starts = lineStarts(src);
  let from = lineOf(starts, spanStart) - 1;
  const to = lineOf(starts, Math.max(spanStart, spanEnd - 1)) - 1;
  while (from > 0 && /^\s*(\/\/\/?|\/\*|\*)/.test(lines[from - 1]) && !/@deprecate|@debug tomb:/.test(lines[from - 1])) from--;
  const indent = /^[ \t]*/.exec(lines[from])[0];
  const body = lines.slice(from, to + 1).map((l) => (l.trim() ? `${/^[ \t]*/.exec(l)[0]}// ${l.trimStart()}` : "//"));
  lines.splice(from, to - from + 1, header(indent, h), ...body);
  return lines.join("\n");
}

/**
 * @param {string} root
 * @param {{date:string, version:string, ids?:string[], write?:boolean, report?:string}} o
 * @returns the entries that were (or, without write, would be) commented out, and the ones skipped with why
 */
export function deprecate(root, { date, version, ids, write = false, report } = {}) {
  if (!/^\d{2}-\d{2}-\d{4}$/.test(date)) throw new Error("--remove-on must be MM-DD-YYYY (code-janitor's format)");
  const rep = JSON.parse(fs.readFileSync(report ?? path.join(root, STATE_DIR, "report.json"), "utf8"));
  const wanted = rep.candidates.filter((c) => !ids || ids.includes(c.id));
  const applied = [], skipped = [];
  const ts = wanted.some((c) => c.lang === "ts") ? loadTs(root) : null;
  const byFile = new Map();
  for (const c of wanted) { if (!byFile.has(c.file)) byFile.set(c.file, []); byFile.get(c.file).push(c); }
  for (const [rel, list] of byFile) {
    const abs = path.join(root, rel);
    let src = fs.readFileSync(abs, "utf8");
    const targets = rel.endsWith(".rs") ? findRust(src) : findTs(ts, src, rel);
    const spans = [];
    for (const c of list) {
      const t = targets.find((x) => {
        const m = /^[ \t]*\n([ \t]*\/\/ @debug tomb:([0-9a-f]{8}))/.exec(src.slice(x.bodyOpen + 1, x.bodyOpen + 200));
        return m && m[2] === c.id;
      });
      if (!t) { skipped.push({ ...c, why: "its mark is no longer in the file" }); continue; }
      if (!t.span) { skipped.push({ ...c, why: "not a whole declaration: edit by hand" }); continue; }
      spans.push({ c, span: t.span });
    }
    spans.sort((a, b) => b.span[0] - a.span[0]);
    // a function inside another candidate goes with it: one block, not two overlapping ones
    const top = spans.filter((s) => !spans.some((o) => o !== s && o.span[0] <= s.span[0] && o.span[1] >= s.span[1]));
    for (const { c, span } of top) {
      src = commentOut(src, span[0], span[1], { date, version, id: c.id, symbol: c.symbol });
      applied.push(c);
    }
    if (write && spans.length) fs.writeFileSync(abs, src);
  }
  return { applied, skipped, written: write };
}

export { MARK_RE };
