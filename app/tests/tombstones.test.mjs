// Tombstone tooling (tools/tombstones): finding the places, marking, probes, the report, deprecations.
import { describe, it, expect } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import ts from "typescript";
import { applyMarks, parseAllowlist, allowReason, parseLcov, lcovHits, globToRegex, readHits, MARK_RE } from "../../tools/tombstones/lib/common.mjs";
import { findTs } from "../../tools/tombstones/lib/ts.mjs";
import { findRust } from "../../tools/tombstones/lib/rust.mjs";
import { injectProbes } from "../../tools/tombstones/vite-plugin.mjs";
import { rewriteSource, revertSource } from "../../tools/tombstones/rust-rewrite.mjs";
import { buildReport, renderMarkdown, runReport } from "../../tools/tombstones/report.mjs";
import { commentOut, deprecate } from "../../tools/tombstones/deprecate.mjs";
import { mark } from "../../tools/tombstones/mark.mjs";

const TS_SRC = `import { x } from "./x";
export function used(a: number) {
  return a + 1;
}
const arrow = () => {
  return 2;
};
const quick = (n: number) => n * 2;
export class Box {
  constructor(private w: number) { super(); }
  grow(by: number) {
    if (by > 0) {
      this.w += by;
    } else {
      this.w -= 1;
    }
    try { run(); } catch (e) { log(e); }
    switch (by) { case 1: return 1; default: return 0; }
  }
  get width() { return this.w; }
}
function directive() { "use strict"; return 1; }
[1].map((v) => { return v; });
const s = "function fake() { return 1; }"; // not a function
`;

function marked(src, file, opts) {
  const targets = findTs(ts, src, file, opts);
  return { targets, ...applyMarks(src, file, targets) };
}

describe("tombstones: TypeScript places", () => {
  it("marks named functions and methods, not constructors, callbacks, directives or expression arrows", () => {
    const { entries } = marked(TS_SRC, "a.ts");
    expect(entries.map((e) => `${e.symbol}:${e.kind}`)).toEqual(["used:function", "arrow:function", "Box.grow:method", "Box.get width:method"]);
  });
  it("with blocks it also marks branches, catch and case", () => {
    const { entries } = marked(TS_SRC, "a.ts", { blocks: true });
    const kinds = entries.map((e) => e.kind);
    expect(kinds).toEqual(expect.arrayContaining(["if", "else", "catch", "case"]));
  });
  it("the marked file still parses and keeps its code", () => {
    for (const blocks of [false, true]) {
      const { src } = marked(TS_SRC, "a.ts", { blocks });
      const sf = ts.createSourceFile("a.ts", src, ts.ScriptTarget.Latest, true);
      expect(sf.parseDiagnostics.length).toBe(0);
      expect(src.replace(/^[ \t]*\/\/ @debug tomb:[0-9a-f]{8}\n/gm, "").replace(/\n[ \t]*(?=\S)/g, "\n")).toContain("return a + 1;");
    }
  });
  it("a mark never lands on the same line as a closing brace", () => {
    const { src } = marked("function f() {}\nfunction g() { return 1; }\n", "b.ts");
    expect(src.split("\n").every((l) => !/@debug tomb:[0-9a-f]{8}.*[{};]/.test(l))).toBe(true);
    expect(ts.createSourceFile("b.ts", src, ts.ScriptTarget.Latest, true).parseDiagnostics.length).toBe(0);
  });
  it("ids are stable when code above moves, and differ per symbol", () => {
    const a = marked(TS_SRC, "a.ts").entries;
    const b = marked("// a new comment\n\n" + TS_SRC, "a.ts").entries;
    expect(b.map((e) => e.id)).toEqual(a.map((e) => e.id));
    expect(new Set(a.map((e) => e.id)).size).toBe(a.length);
    expect(b[0].line).toBe(a[0].line + 2);
    expect(marked(TS_SRC, "other.ts").entries[0].id).not.toBe(a[0].id);
  });
  it("overloads and same-named functions get different ids", () => {
    const { entries } = marked("class A { f() {} }\nclass B { f() {} }\nfunction h() { function h() {} }\n", "c.ts");
    expect(new Set(entries.map((e) => e.id)).size).toBe(entries.length);
  });
});

describe("tombstones: Rust places", () => {
  const RS = `//! doc
use std::fmt;
pub fn used(x: i32) -> i32 { x + 1 }
const fn tiny() -> u8 { 1 }
fn text() -> &'static str { "fn fake() { }" }
macro_rules! m { () => { fn inner() { } } }
pub struct S;
impl S {
    #[inline]
    pub fn method(&self, c: char) -> bool {
        let q = '{'; // a brace char, not a block
        match c {
            'a' => { true }
            _ => false,
        }
    }
    fn decl_only();
}
trait T { fn d(&self) { } fn n(&self); }
#[cfg(test)]
mod tests {
    #[test]
    fn t() { }
    fn helper() { }
}
mod inner { pub fn deep<'a>(s: &'a str) -> impl Iterator<Item = &'a str> { s.split(' ') } }
`;
  it("finds fn bodies, skipping const fn, macros, tests, and braces in strings and chars", () => {
    const syms = findRust(RS).map((t) => t.symbol);
    expect(syms).toEqual(["used", "text", "S::method", "T::d", "inner::deep"]);
  });
  it("with blocks it marks match arms that have a block", () => {
    const t = findRust(RS, { blocks: true }).filter((x) => x.kind === "arm");
    expect(t.map((x) => x.symbol)).toEqual(["S::method#arm"]);
  });
  it("the span of a method includes its attribute and visibility", () => {
    const t = findRust(RS).find((x) => x.symbol === "S::method");
    expect(RS.slice(t.span[0], t.span[1]).startsWith("#[inline]")).toBe(true);
    expect(RS.slice(t.span[0], t.span[1]).endsWith("}")).toBe(true);
  });
  it("the rewrite turns marks into probes and back", () => {
    const targets = findRust(RS);
    const { src } = applyMarks(RS, "core/src/x.rs", targets);
    const w = rewriteSource(src);
    expect(w).toMatch(/crate::__tomb!\("[0-9a-f]{8}"\);/);
    expect(w).not.toMatch(/@debug tomb/);
    expect(revertSource(w, () => true)).toBe(src);
    const gone = revertSource(w, () => false);
    expect(gone).not.toMatch(/__tomb|@debug/);
  });
});

describe("tombstones: probe injection (Vite plugin)", () => {
  it("replaces marks with calls on the same lines and imports the runtime", () => {
    const { src } = marked("function f() {\n  return 1;\n}\n", "d.ts");
    const out = injectProbes(src);
    expect(out).toMatch(/^import \{ __tomb \} from "virtual:tomb-runtime";function f\(\) \{\n  __tomb\("[0-9a-f]{8}"\);\n  return 1;/);
    expect(out.split("\n").length).toBe(src.split("\n").length);
  });
  it("does nothing to a file without marks", () => {
    expect(injectProbes("export const a = 1;\n")).toBeNull();
  });
});

describe("tombstones: allowlist, hits, lcov", () => {
  it("matches path and symbol globs and gives the reason", () => {
    const rules = parseAllowlist("# c\ncore/src/skp*.rs  # old formats\napp/src/**/*android*::on*  # phone\n");
    expect(allowReason(rules, { file: "core/src/skp_vff.rs", symbol: "x" })).toBe("old formats");
    expect(allowReason(rules, { file: "app/src/a/b/android.ts", symbol: "onTap" })).toBe("phone");
    expect(allowReason(rules, { file: "app/src/a/b/android.ts", symbol: "tap" })).toBeNull();
    expect(allowReason(rules, { file: "core/src/model.rs", symbol: "x" })).toBeNull();
    expect(globToRegex("a/*.ts").test("a/b/c.ts")).toBe(false);
  });
  it("reads ids from any number of logs, ignoring junk", () => {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), "tomb-"));
    fs.writeFileSync(path.join(d, "a.log"), "deadbeef\nnot an id\n  0123abcd  \n");
    fs.writeFileSync(path.join(d, "b.log"), "ts 11112222\naaaaaaaabbbbbbbb\n");
    expect([...readHits([path.join(d, "a.log"), path.join(d, "b.log"), path.join(d, "none.log")])].sort()).toEqual(["0123abcd", "11112222", "aaaaaaaa", "bbbbbbbb", "deadbeef"]);
  });
  it("lcov: a function is hit when a counted function starts in its signature", () => {
    const manifest = { entries: [
      { id: "aaaaaaaa", file: "core/src/x.rs", kind: "fn", line: 10, bodyLine: 10, endLine: 20, symbol: "a" },
      { id: "bbbbbbbb", file: "core/src/x.rs", kind: "fn", line: 30, bodyLine: 31, endLine: 40, symbol: "b" },
      { id: "cccccccc", file: "core/src/y.rs", kind: "fn", line: 1, bodyLine: 1, endLine: 2, symbol: "c" },
    ] };
    const lcov = parseLcov("SF:/r/core/src/x.rs\nFN:10,_a\nFN:30,_b\nFNDA:4,_a\nFNDA:0,_b\nend_of_record\n");
    const r = lcovHits(manifest, lcov, "/r");
    expect([...r.hit]).toEqual(["aaaaaaaa"]);
    expect([...r.judged].sort()).toEqual(["aaaaaaaa", "bbbbbbbb"]); // y.rs is not in the lcov: undecided
  });
});

describe("tombstones: report", () => {
  const manifest = { base: "abc", entries: [
    { id: "00000001", file: "app/src/a.ts", line: 3, endLine: 5, bodyLine: 3, symbol: "hit", kind: "function", lang: "ts", deprecatable: true },
    { id: "00000002", file: "app/src/a.ts", line: 9, endLine: 12, bodyLine: 9, symbol: "dead", kind: "function", lang: "ts", deprecatable: true },
    { id: "00000003", file: "core/src/skp.rs", line: 4, endLine: 8, bodyLine: 4, symbol: "old", kind: "fn", lang: "rust", deprecatable: true },
  ] };
  it("splits what is left into candidates and check-by-hand", () => {
    const rules = parseAllowlist("core/src/skp.rs  # old format\n");
    const r = buildReport(manifest, new Set(["00000001"]), { allowRules: rules });
    expect(r.totals).toMatchObject({ marks: 3, hit: 1, candidates: 1, byHand: 1 });
    expect(r.candidates[0]).toMatchObject({ id: "00000002", symbol: "dead", line: 9 });
    expect(r.byHand[0].reason).toBe("old format");
    const md = renderMarkdown(r);
    expect(md).toContain("| `app/src/a.ts` | 9 | `dead` | function | `00000002` |");
    expect(md).toContain("## Check by hand");
  });
  it("end to end: strips only hit marks, keeps the code, writes both reports, refuses an empty log", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "tomb-root-"));
    fs.mkdirSync(path.join(root, "app/src"), { recursive: true });
    fs.mkdirSync(path.join(root, "core/src"), { recursive: true });
    fs.writeFileSync(path.join(root, "app/package.json"), '{"version":"0.3.10"}');
    fs.writeFileSync(path.join(root, "app/src/a.ts"), "export function live() {\n  return 1;\n}\n\nexport function dead() {\n  return 2;\n}\n");
    fs.writeFileSync(path.join(root, "core/src/lib.rs"), "//! x\npub mod m;\n");
    fs.writeFileSync(path.join(root, "core/src/m.rs"), "pub fn r_live() { }\n\npub fn r_dead() {\n    let _ = 1;\n}\n");
    const m = mark(root, { force: true });
    expect(m.entries.map((e) => e.symbol).sort()).toEqual(["dead", "live", "r_dead", "r_live"]);
    const id = (s) => m.entries.find((e) => e.symbol === s).id;
    expect(() => runReport(root, { hits: [path.join(root, "missing.log")] })).toThrow(/no hits/);
    const log = path.join(root, "hits.log");
    fs.writeFileSync(log, `${id("live")}\n${id("r_live")}\n`);
    const { report, removed } = runReport(root, { hits: [log] });
    expect(removed).toBe(2);
    expect(report.candidates.map((c) => c.symbol)).toEqual(["dead", "r_dead"]);
    const a = fs.readFileSync(path.join(root, "app/src/a.ts"), "utf8");
    expect(a).toContain(`// @debug tomb:${id("dead")}`);
    expect(a).not.toContain(id("live"));
    expect(a).toContain("return 1;");
    expect(fs.existsSync(path.join(root, ".tombstones/report.md"))).toBe(true);
    expect(JSON.parse(fs.readFileSync(path.join(root, ".tombstones/report.json"), "utf8")).totals.candidates).toBe(2);

    // deprecate: dry run changes nothing; --write comments the code out in code-janitor's format
    const dry = deprecate(root, { date: "12-15-2026", version: "0.3.12" });
    expect(dry.applied.length).toBe(2);
    expect(fs.readFileSync(path.join(root, "app/src/a.ts"), "utf8")).toBe(a);
    deprecate(root, { date: "12-15-2026", version: "0.3.12", write: true });
    const b = fs.readFileSync(path.join(root, "app/src/a.ts"), "utf8");
    expect(b).toContain(`// @deprecate 12-15-2026 reason: tombstone ${id("dead")} (dead) was not reached by any test run; remove in 0.3.12\n// export function dead() {`);
    expect(b).toContain("export function live() {");
    const rs = fs.readFileSync(path.join(root, "core/src/m.rs"), "utf8");
    expect(rs).toContain("// pub fn r_dead() {");
    expect(rs).toMatch(/^pub fn r_live\(\) \{\n \}$/m);
    expect(() => deprecate(root, { date: "2026-12-15", version: "x" })).toThrow(/MM-DD-YYYY/);
  });
});

describe("tombstones: commenting out", () => {
  it("pulls in the doc comment above and handles blank lines", () => {
    const src = "a();\n/// docs\n#[inline]\nfn f() {\n\n    b();\n}\nc();\n";
    const start = src.indexOf("#[inline]"), end = src.indexOf("}\nc") + 1;
    expect(commentOut(src, start, end, { date: "01-02-2027", version: "1", id: "abcdabcd", symbol: "f" }).split("\n")).toEqual([
      "a();",
      "// @deprecate 01-02-2027 reason: tombstone abcdabcd (f) was not reached by any test run; remove in 1",
      "// /// docs", "// #[inline]", "// fn f() {", "//", "    // b();", "// }", "c();", "",
    ]);
  });
  it("MARK_RE only matches a whole mark line", () => {
    expect(MARK_RE.test("    // @debug tomb:0123abcd")).toBe(true);
    expect(MARK_RE.test("x(); // @debug tomb:0123abcd")).toBe(false);
  });
});
