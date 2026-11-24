import { describe, expect, it } from "vitest";
import { LANGUAGE_IDS, LANGUAGE_TABLE } from "../src/languages";
import { translate } from "../src/locale";
import { leaves } from "../src/menumodel";
import { flattenCommand, flattenHeight, flattenSummary, flattenTargets, makeFacesCommand, makeFacesSummary, makeFacesTargets } from "../src/smalltools";
import { EMPTY } from "../src/selection";
import { parseLength } from "../src/units";

const en = (id: string) => translate("en", id);

describe("Make faces", () => {
  it("works on the picked lines and groups, or on every loose line", () => {
    expect(makeFacesCommand(EMPTY)).toEqual({ cmd: "make_faces" });
    const sel = { ...EMPTY, edges: [3, 4], groups: [9], faces: [1], instances: [2] };
    expect(makeFacesTargets(sel)).toEqual([3, 4, 9]);
    expect(makeFacesCommand(sel)).toEqual({ cmd: "make_faces", ids: [3, 4, 9] });
    expect(makeFacesCommand(EMPTY, true)).toEqual({ cmd: "make_faces", open_holes: true });
  });

  it("says what it made, in counts", () => {
    const s = makeFacesSummary({ created: 3, with_holes: 1, split: 2, non_planar: [], open_edges: 0, degenerate: 0 }, en);
    expect(s).toEqual({ text: "Made faces: faces made: 3, with holes: 1, lines cut where they cross: 2. Undo takes it all back.", bad: false });
  });

  it("says what it left alone, and points a bent loop to Flatten", () => {
    const s = makeFacesSummary({ created: 1, non_planar: [{ id: 5, at: [0, 0, 0] }], open_edges: 2, degenerate: 0 }, en);
    expect(s.bad).toBe(true);
    expect(s.text).toContain("loops that are not flat: 1");
    expect(s.text).toContain("lines that are not in a closed loop: 2");
    expect(s.text).toContain("Flatten to a height first");
    const calm = makeFacesSummary({ created: 1, open_edges: 2 }, en);
    expect(calm.bad).toBe(false);
    expect(calm.text).not.toContain("Flatten");
  });

  it("says when there was nothing to do", () => {
    expect(makeFacesSummary({ created: 0, split: 0 }, en).text).toBe(en("makefaces.none"));
    expect(makeFacesSummary({ created: 0, existing: 2 }, en)).toEqual({ text: en("makefaces.already"), bad: false });
    const stuck = makeFacesSummary({ created: 0, non_planar: [{ id: 1, at: [0, 0, 0] }] }, en);
    expect(stuck.bad).toBe(true);
    expect(stuck.text).toContain("Flatten");
  });
});

describe("Flatten to height", () => {
  it("works on the picked lines, faces and groups", () => {
    const sel = { ...EMPTY, edges: [3], faces: [4], groups: [5], instances: [6] };
    expect(flattenTargets(sel)).toEqual([3, 4, 5]);
    expect(flattenCommand(sel, 0)).toEqual({ cmd: "flatten", ids: [3, 4, 5], z: 0 });
    expect(flattenTargets(EMPTY)).toEqual([]);
  });

  it("reads the height in the model's units, and 0 and below are heights", () => {
    const parse = (units: string) => (text: string) => parseLength(text, units);
    expect(flattenHeight("0", parse("m"))).toBe(0);
    expect(flattenHeight("2.5", parse("m"))).toBe(2.5);
    expect(flattenHeight("-1", parse("m"))).toBe(-1);
    expect(flattenHeight("250", parse("cm"))).toBeCloseTo(2.5);
    expect(flattenHeight("10'", parse("ft"))).toBeCloseTo(3.048);
    expect(flattenHeight("abc", parse("m"))).toBeNull();
    expect(flattenHeight("", parse("m"))).toBeNull();
    expect(flattenHeight(null, parse("m"))).toBeNull();
  });

  it("says what it did, in counts", () => {
    const s = flattenSummary({ moved: 8, merged: 4, edges_removed: 5, faces_removed: 1, faces_turned: 0 }, "0.00 m", en);
    expect(s.text).toBe("Flattened to 0.00 m: points moved: 8, points joined: 4, lines removed: 5, faces removed: 1. Undo takes it all back.");
    expect(flattenSummary({ moved: 0, merged: 0, edges_removed: 0, faces_removed: 0, faces_turned: 0 }, "0.00 m", en).text).toBe(en("flatten.same"));
  });
});

describe("the menus and words", () => {
  it("lists both in the Tools menu", () => {
    const ids = leaves().map((l) => l.id);
    expect(ids).toContain("tools.makefaces");
    expect(ids).toContain("tools.flatten");
    expect(ids).toContain("tool:spline");
  });

  it("has every new message in every language with the same placeholders", () => {
    const keys = Object.keys(LANGUAGE_TABLE.en.catalog).filter((k) => /^(makefaces|flatten|spline)\./.test(k) || ["menu.makeFaces", "menu.flatten", "tool.spline"].includes(k));
    expect(keys.length).toBeGreaterThanOrEqual(28);
    const holes = (t: string) => (t.match(/\{\w+\}/g) ?? []).sort();
    for (const lang of LANGUAGE_IDS) {
      for (const k of keys) {
        const text = translate(lang, k);
        expect(text.length, `${lang} ${k}`).toBeGreaterThan(0);
        expect(holes(text), `${lang} ${k}`).toEqual(holes(en(k)));
      }
    }
  });
});
