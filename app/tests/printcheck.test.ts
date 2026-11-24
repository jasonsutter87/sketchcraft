import { describe, expect, it } from "vitest";
import { LANGUAGE_IDS, LANGUAGE_TABLE } from "../src/languages";
import { translate } from "../src/locale";
import { EXPORT_FORMATS, leaves } from "../src/menumodel";
import { PRINT_DEFAULTS, printCommand, printLines, problemSelection, type PrintReply } from "../src/printcheck";
import { EMPTY } from "../src/selection";

const en = (id: string) => translate("en", id);
const spot = (id: number, extra = {}) => ({ id, at: [0, 0, 0], ...extra });
const good: PrintReply = { solid: true, size_mm: [20, 20, 20], thin_walls: { count: 0 }, overhangs: { count: 0 }, ready: true };

describe("Print check", () => {
  it("asks about the picked groups and faces, or the whole model", () => {
    expect(printCommand(EMPTY, PRINT_DEFAULTS)).toEqual({ cmd: "print_check", min_wall: 0.8, overhang_angle: 45 });
    const sel = { ...EMPTY, groups: [4], faces: [7], edges: [9] };
    expect(printCommand(sel, { minWall: 1.2, angle: 30 })).toEqual({ cmd: "print_check", ids: [4, 7], min_wall: 1.2, overhang_angle: 30 });
  });

  it("says a good model is fine", () => {
    const lines = printLines(good, PRINT_DEFAULTS, en);
    expect(lines.every((l) => l.ok && !l.warn)).toBe(true);
    expect(lines[0].text).toBe("Size: 20 × 20 × 20 mm");
    expect(lines.map((l) => l.key)).toEqual(["size", "solid", "thin", "over"]);
  });

  it("says what is wrong, with counts", () => {
    const r: PrintReply = {
      solid: false,
      solid_check: { open_edges: [spot(1), spot(2)] },
      thin_walls: { count: 3, thinnest_mm: 0.5 },
      overhangs: { count: 2 },
      size_mm: [0.4, 0.4, 0.4],
      units_hint: { kind: "too_small" },
      fit: { fits: false, fits_if_turned: true, over_mm: [5, 0, 0] },
    };
    const by = Object.fromEntries(printLines(r, PRINT_DEFAULTS, en).map((l) => [l.key, l]));
    expect(by.solid.ok).toBe(false);
    expect(by.solid.text).toContain("open edges: 2");
    expect(by.thin.text).toBe("3 faces sit on walls thinner than 0.8 mm (thinnest 0.5 mm).");
    expect(by.over).toMatchObject({ ok: true, warn: true });
    expect(by.units.ok).toBe(false);
    expect(by.units.text).toContain("0.4 mm");
    expect(by.fit.text).toBe("Too big for the build volume, over by 5 × 0 × 0 mm. It would fit turned.");
  });

  it("an empty model has nothing to check", () => {
    expect(printLines({ solid: false, size_mm: null }, PRINT_DEFAULTS, en)).toEqual([{ key: "empty", ok: false, text: en("print.empty") }]);
  });

  it("picks the problem faces and open edges, skipping what is inside a component", () => {
    const r: PrintReply = {
      thin_walls: { items: [spot(3), spot(3), spot(9, { in_component: true })] },
      overhangs: { items: [spot(5)] },
      solid_check: { open_edges: [spot(11), spot(0)] },
    };
    expect(problemSelection(r, "thin")).toEqual({ faces: [3] });
    expect(problemSelection(r, "over")).toEqual({ faces: [5] });
    expect(problemSelection(r, "open")).toEqual({ edges: [11] });
  });

  it("has every message in every language, with the same placeholders", () => {
    const ids = Object.keys(LANGUAGE_TABLE.en.catalog).filter((k) => k.startsWith("print.") || k === "menu.printCheck");
    expect(ids.length).toBeGreaterThan(20);
    const holes = (t: string) => (t.match(/\{\w+\}/g) ?? []).sort();
    for (const lang of LANGUAGE_IDS) {
      const catalog: Readonly<Record<string, string | null>> = LANGUAGE_TABLE[lang].catalog;
      for (const id of ids) {
        expect(catalog[id], `${lang} ${id}`).toBeTruthy();
        expect(holes(catalog[id] as string), `${lang} ${id}`).toEqual(holes(en(id)));
      }
    }
  });

  it("is in the Tools menu and File > Export has 3MF", () => {
    expect(leaves().map((l) => l.id)).toEqual(expect.arrayContaining(["tools.printcheck", "file.export.3mf", "file.export.stl"]));
    expect(EXPORT_FORMATS.map(([k]) => k)).toContain("3mf");
  });
});
