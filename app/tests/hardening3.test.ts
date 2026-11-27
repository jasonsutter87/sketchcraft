// What a plugin hands the window is cleaned: names in toasts, the text of results and forms, and every invisible
// or direction-changing character, the same set the engine removes.
import { expect, it } from "vitest";
import { cleanForm, cleanLabel, cleanResult, cleanText } from "../src/pluginclean";

const sneaky = ["͏", "ᅟ", "ᅠ", "឴", "᠋", "⠀", "ㅤ", "️", "ﾠ", "￼", "\u{e0041}", "‮", "​"];

it("removes every invisible and direction character from a label", () => {
  for (const c of sneaky) expect(cleanLabel(`Tr${c}usted`), JSON.stringify(c)).toBe("Trusted");
  expect(cleanLabel("Trusted”: Saved‮\nline two")).toBe("Trusted”: Saved line two");
});

it("keeps line breaks in body text but nothing hidden", () => {
  expect(cleanText("one\nt‮wo\u{e0041}", 100)).toBe("one\ntwo");
});

it("results and forms: titles, labels, options and hints are one clean line", () => {
  const r = cleanResult({ title: "A\nVerified by SketchCraft‮", text: "x\ny​", table: { columns: ["c‮1"], rows: [["v⠀"]] } });
  expect(r.title).toBe("A Verified by SketchCraft");
  expect(r.text).toBe("x\ny");
  expect(r.table?.columns).toEqual(["c1"]);
  expect(r.table?.rows[0]).toEqual(["v"]);
  const f = cleanForm({ title: "T‮", text: "t", ok: "O\nK", fields: [{ id: "a", label: "L‮", options: ["o​"], hint: "h\n2" }] });
  expect([f.title, f.ok, f.fields[0].label, f.fields[0].options?.[0], f.fields[0].hint]).toEqual(["T", "O K", "L", "o", "h 2"]);
});

it("also removes variation selectors past U+E0100 and musical formatting characters", () => {
  expect(cleanLabel("A\u{e0100}B\u{1d173}C")).toBe("ABC");
});

it("a result table shows a bounded number of cells, each one clean line", () => {
  const columns = Array.from({ length: 40 }, (_, i) => `c${i}`);
  const rows = Array.from({ length: 2000 }, () => columns.map(() => "x".repeat(400) + "‮"));
  const r = cleanResult({ table: { columns, rows } });
  const cells = (r.table?.rows.length ?? 0) * (r.table?.columns.length ?? 0);
  expect(cells).toBeLessThanOrEqual(10_000);
  expect(String(r.table?.rows[0][0]).length).toBe(200);
  expect(String(r.table?.rows[0][0])).not.toContain("‮");
});

it("every Unicode format character is removed", () => {
  // U+061C, U+08E2, U+110BD and U+13430 are all category Cf
  expect(cleanLabel("a࣢b\u{110bd}c\u{13430}d")).toBe("abcd");
});
