import { describe, expect, it } from "vitest";
import { cleanForm, cleanPoint, cleanPreview, cleanResult } from "../src/pluginclean";

describe("cleanResult (what a plugin shows)", () => {
  it("keeps a title, text and a table", () => {
    const r = cleanResult({ title: "T", text: "body", table: { columns: ["a", "b"], rows: [[1, "x"], [2, "y"]] } });
    expect(r).toEqual({ title: "T", text: "body", table: { columns: ["a", "b"], rows: [[1, "x"], [2, "y"]] } });
  });
  it("cuts long strings, wide and tall tables", () => {
    const r = cleanResult({ title: "t".repeat(500), text: "x".repeat(50_000), table: { columns: Array.from({ length: 60 }, (_, i) => `c${i}`), rows: Array.from({ length: 3000 }, () => Array.from({ length: 60 }, () => 1)) } });
    expect(r.title).toHaveLength(120);
    expect(r.text).toHaveLength(20_000);
    expect(r.table!.columns).toHaveLength(40);
    // 40 columns: 10,000 cells is 250 rows
    expect(r.table!.rows).toHaveLength(250);
    expect(r.table!.rows[0]).toHaveLength(40);
  });
  it("turns cells into strings or finite numbers", () => {
    const r = cleanResult({ table: { columns: ["a"], rows: [[NaN], [Infinity], [{ x: 1 }], [null], [true], ["s".repeat(900)]] } });
    expect(r.table!.rows.slice(0, 5)).toEqual([["NaN"], ["Infinity"], ["[object Object]"], [""], ["true"]]);
    expect(r.table!.rows[5][0]).toHaveLength(200);
  });
  it("makes a row that is not a list an empty row", () => {
    expect(cleanResult({ table: { columns: ["a"], rows: ["nope", 5, null] } }).table!.rows).toEqual([[], [], []]);
  });
  it("survives anything", () => {
    for (const v of [null, undefined, 5, "s", [], true, { table: 5 }, { table: { columns: "x", rows: 1 } }, { title: { toString: () => { throw new Error("no"); } } }]) expect(() => cleanResult(v)).not.toThrow();
    expect(cleanResult(5)).toEqual({ title: undefined, text: undefined });
  });
});

describe("cleanForm (what a plugin asks you to fill in)", () => {
  it("keeps good fields", () => {
    const f = cleanForm({ title: "Wall", ok: "Go", fields: [{ id: "h", label: "Height", type: "length", value: 2.4, min: 0.5, max: 10, hint: "m" }, { id: "k", type: "choice", options: ["a", "b"], value: "a" }, { id: "c", type: "check", value: true }] });
    expect(f.title).toBe("Wall");
    expect(f.ok).toBe("Go");
    expect(f.fields).toHaveLength(3);
    expect(f.fields[0]).toMatchObject({ id: "h", label: "Height", type: "length", value: 2.4, min: 0.5, max: 10, hint: "m" });
    expect(f.fields[1]).toMatchObject({ options: ["a", "b"], value: "a", label: "k" });
    expect(f.fields[2].value).toBe(true);
  });
  it("makes an unknown type text and a missing value empty", () => {
    const f = cleanForm({ fields: [{ id: "a", type: "slider" }, { id: "b" }] });
    expect(f.fields.map((x) => [x.type, x.value])).toEqual([["text", ""], ["text", ""]]);
  });
  it("drops fields with no string id, and anything that is not an object", () => {
    expect(cleanForm({ fields: [{ label: "x" }, { id: 5 }, null, "s", 3, { id: "ok" }] }).fields.map((x) => x.id)).toEqual(["ok"]);
  });
  it("drops min and max that are not finite numbers", () => {
    const [x] = cleanForm({ fields: [{ id: "a", min: NaN, max: "9" }] }).fields;
    expect(x.min).toBeUndefined();
    expect(x.max).toBeUndefined();
  });
  it("limits fields, options and lengths", () => {
    const f = cleanForm({ title: "t".repeat(500), text: "x".repeat(5000), ok: "o".repeat(99), fields: Array.from({ length: 60 }, (_, i) => ({ id: `f${i}`.padEnd(200, "x"), label: "l".repeat(300), options: Array.from({ length: 150 }, () => "o".repeat(200)), value: "v".repeat(900) })) });
    expect(f.title).toHaveLength(120);
    expect(f.text).toHaveLength(2000);
    expect(f.ok).toHaveLength(30);
    expect(f.fields).toHaveLength(40);
    expect(f.fields[0].id).toHaveLength(80);
    expect(f.fields[0].label).toHaveLength(120);
    expect(f.fields[0].options).toHaveLength(100);
    expect(f.fields[0].options![0]).toHaveLength(80);
    expect(String(f.fields[0].value)).toHaveLength(400);
  });
  it("survives anything", () => {
    for (const v of [null, undefined, 5, "s", [], { fields: "x" }, { fields: { length: 3 } }]) expect(cleanForm(v).fields).toEqual([]);
  });
});

describe("cleanPoint and cleanPreview (a plugin tool's preview)", () => {
  it("keeps three finite numbers and clamps them to the model's range", () => {
    expect(cleanPoint([1, 2, 3])).toEqual([1, 2, 3]);
    expect(cleanPoint([1e12, -1e12, 0])).toEqual([1e9, -1e9, 0]);
  });
  it("drops anything else", () => {
    for (const p of [[1, 2], [1, 2, 3, 4], [1, 2, NaN], [1, 2, Infinity], ["1", 2, 3], [null, 1, 2], null, "xyz", {}, 5]) expect(cleanPoint(p), JSON.stringify(p)).toBeNull();
  });
  it("keeps only lines of exactly two good points, up to a limit", () => {
    const out = cleanPreview([[[0, 0, 0], [1, 0, 0]], [[0, 0, 0]], [[0, 0, 0], [NaN, 0, 0]], "x", [[0, 0, 0], [0, 1, 0]]]);
    expect(out).toEqual([[[0, 0, 0], [1, 0, 0]], [[0, 0, 0], [0, 1, 0]]]);
    const many = Array.from({ length: 20 }, () => [[0, 0, 0], [1, 1, 1]]);
    expect(cleanPreview(many, 5)).toHaveLength(5);
  });
  it("gives nothing for what is not a list", () => {
    for (const v of [null, undefined, 5, "s", {}]) expect(cleanPreview(v)).toEqual([]);
  });
});
