import { describe, expect, it } from "vitest";
import { LANGUAGE_IDS, LANGUAGE_TABLE } from "../src/languages";
import { translate } from "../src/locale";
import { leaves } from "../src/menumodel";
import { FIX_CHANGES, FIX_REMAINING, fixCommand, fixSummary, fixTargets } from "../src/fixmodel";
import { EMPTY } from "../src/selection";

const en = (id: string) => translate("en", id);
const spots = (n: number) => Array.from({ length: n }, (_, i) => ({ id: i + 1, at: [0, 0, 0] }));

describe("Fix model", () => {
  it("asks about the picked groups and faces, or the whole model", () => {
    expect(fixCommand(EMPTY, false)).toEqual({ cmd: "fix_model" });
    const sel = { ...EMPTY, groups: [4], faces: [7, 8], edges: [9], instances: [3] };
    expect(fixTargets(sel)).toEqual([4, 7, 8]);
    expect(fixCommand(sel, true)).toEqual({ cmd: "fix_model", ids: [4, 7, 8], dry_run: true });
  });

  it("says what it fixed, in counts", () => {
    const r = { changes: 3, filled_holes: spots(1), reversed_faces: spots(2), remaining: { solid: true, faces: 6 } };
    const s = fixSummary(r, en);
    expect(s.bad).toBe(false);
    expect(s.text).toBe("Fixed the model: faces turned around: 2, holes closed: 1. Undo takes it all back.");
  });

  it("says a clean model is clean, and an empty one has nothing to check", () => {
    expect(fixSummary({ changes: 0, remaining: { solid: true, faces: 6 } }, en)).toEqual({ text: en("fix.clean"), bad: false });
    expect(fixSummary({ changes: 0, remaining: { solid: false, faces: 0 } }, en).text).toBe(en("fix.empty"));
  });

  it("says what is still wrong, and marks it as a problem", () => {
    const r = { stray_edges: spots(1), remaining: { solid: false, faces: 5, open_edges: spots(4) } };
    const s = fixSummary(r, en);
    expect(s.bad).toBe(true);
    expect(s.text).toContain("loose lines removed: 1");
    expect(s.text).toContain("Still wrong: open edges: 4.");
    const stuck = fixSummary({ remaining: { solid: false, faces: 5, non_manifold_edges: spots(2) } }, en);
    expect(stuck.bad).toBe(true);
    expect(stuck.text).toContain("edges shared by 3+ faces: 2");
  });

  it("counts every loose line removed, not only the ones the reply lists", () => {
    const r = { changes: 1500, stray_edges: spots(1000), more_stray_edges: 500, remaining: { solid: true, faces: 6 } };
    expect(fixSummary(r, en).text).toBe("Fixed the model: loose lines removed: 1500. Undo takes it all back.");
    // nothing listed and only a count still counts as a change
    expect(fixSummary({ more_stray_edges: 7, remaining: { solid: true, faces: 6 } }, en).text).toContain("loose lines removed: 7");
  });

  it("a dry run says what it would do", () => {
    const s = fixSummary({ dry_run: true, merged_faces: spots(2), remaining: { solid: true, faces: 8 } }, en);
    expect(s.text).toBe("Would fix: flat faces joined: 2.");
  });

  it("has every message in every language, with the same placeholders", () => {
    const ids = ["menu.fixModel", "menu.checkModel", "fix.clean", "fix.empty", "fix.fixed", "fix.would", "fix.still", "fix.stuck", ...FIX_CHANGES.map((k) => `fix.cat.${k}`), ...FIX_REMAINING.map((k) => `fix.rem.${k}`)];
    const holes = (t: string) => (t.match(/\{\w+\}/g) ?? []).sort();
    for (const lang of LANGUAGE_IDS) {
      const catalog: Readonly<Record<string, string | null>> = LANGUAGE_TABLE[lang].catalog;
      for (const id of ids) {
        expect(catalog[id], `${lang} ${id}`).toBeTruthy();
        expect(holes(catalog[id] as string), `${lang} ${id}`).toEqual(holes(en(id)));
      }
    }
  });

  it("is in the Tools menu (tests/menus.test.ts checks each item has one action)", () => {
    expect(leaves().map((l) => l.id)).toEqual(expect.arrayContaining(["tools.fixmodel", "tools.checkmodel"]));
  });
});
