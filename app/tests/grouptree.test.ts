import { describe, expect, it } from "vitest";
import { buildTree, descendants, flatten, matching, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ROWS, type TreeScene } from "../src/grouptree";
import { groupRepairNotice, groupRepairOf, MAX_GROUP_DEPTH } from "../src/grouprepair";
import { LANGUAGE_IDS, LANGUAGE_TABLE } from "../src/languages";
import { translate } from "../src/locale";

const group = (id: number, parent: number | null, name = `G${id}`) => ({ id, name, hidden: false, locked: false, tag: null, faces: 1, parent });
/** `depth` groups, each inside the one before it */
const chain = (depth: number): TreeScene => ({ groups: Array.from({ length: depth }, (_, i) => group(i + 1, i === 0 ? null : i, `Level ${i + 1}`)), instances: [] });
const open = () => true;

describe("the outliner's tree walks do not recurse", () => {
  it("builds, searches and lists a model nested 50,000 levels deep without overflowing the stack", () => {
    const scene = chain(50_000);
    const roots = buildTree(scene);
    expect(roots).toHaveLength(1);
    // (a recursive walk dies somewhere around ten thousand levels)
    const { rows, cut } = flatten(roots, open);
    expect(rows).toHaveLength(MAX_OUTLINE_DEPTH);
    expect(rows[rows.length - 1].depth).toBe(MAX_OUTLINE_DEPTH - 1);
    expect(cut).toBe(50_000 - MAX_OUTLINE_DEPTH);
    const hits = matching(roots, "level 200");
    expect(hits.size).toBeGreaterThanOrEqual(200);
    expect(flatten(roots, open, hits).rows.length).toBeLessThanOrEqual(MAX_OUTLINE_DEPTH);
  });

  it("lists a realistic model completely, in order, with depth and hidden parents", () => {
    const scene: TreeScene = {
      groups: [group(1, null, "House"), { ...group(2, 1, "Roof"), hidden: true }, group(3, 2, "Tiles"), group(4, 1, "Walls")],
      instances: [{ id: 5, name: "Chair", hidden: false, tag: "Furniture", parent: 1 }],
    };
    const { rows, cut } = flatten(buildTree(scene), open);
    expect(cut).toBe(0);
    // groups first, then components, each by name
    expect(rows.map((r) => [r.node.name, r.depth, r.parentHidden])).toEqual([
      ["House", 0, false],
      ["Roof", 1, false],
      ["Tiles", 2, true],
      ["Walls", 1, false],
      ["Chair", 1, false],
    ]);
    expect(rows[0].hasKids).toBe(true);
    expect(rows[2].hasKids).toBe(false);
  });

  it("leaves what is under a closed row out, and a search opens everything it keeps", () => {
    const roots = buildTree({ groups: [group(1, null, "House"), group(2, 1, "Roof"), group(3, 2, "Tiles")], instances: [] });
    expect(flatten(roots, (id) => id !== 1).rows.map((r) => r.node.name)).toEqual(["House"]);
    const hits = matching(roots, "tile");
    expect([...hits].sort()).toEqual([1, 2, 3]);
    expect(flatten(roots, open, hits).rows.map((r) => r.node.name)).toEqual(["House", "Roof", "Tiles"]);
    expect(matching(roots, "nothing like it").size).toBe(0);
    expect(matching(roots, "  ").size).toBe(0);
  });

  it("does not hang or list groups whose parents come round in a loop", () => {
    const roots = buildTree({ groups: [group(1, 2), group(2, 1), group(3, null, "Fine")], instances: [] });
    expect(flatten(roots, open).rows.map((r) => r.node.name)).toEqual(["Fine"]);
  });

  it("finds a group's descendants in one pass, however deep", () => {
    const scene = chain(80_000);
    expect(descendants(scene.groups, [40_000]).size).toBe(40_001);
    // listed children-first, the order a fixed-point loop is slowest on
    const backwards = [...scene.groups].reverse();
    const t = performance.now();
    expect(descendants(backwards, [1]).size).toBe(80_000);
    expect(performance.now() - t).toBeLessThan(2000);
  });
});

describe("the notice for repaired groups", () => {
  const en = (id: string) => translate("en", id);
  it("says itself, names the groups and caps the list", () => {
    const one = groupRepairOf({ group_loops_repaired: 1, group_loop_names: ["Roof"] })!;
    const text = groupRepairNotice(en, one, "file");
    expect(text).toContain("1 group (Roof) that sat inside itself (a loop)");
    expect(text).not.toContain("itselves");
    const many = groupRepairOf({ group_loops_repaired: 12, group_loop_names: ["A", "B", "C", "D", "E"], groups_flattened: 2, groups_flattened_names: ["Deep", "Deeper"] })!;
    const both = groupRepairNotice(en, many, "backup");
    expect(both).toContain("This backup had 12 groups (A, B, C, D, E and 7 more) that sat inside each other in loops");
    expect(both).toContain(`and 2 groups (Deep, Deeper) that were nested more than ${MAX_GROUP_DEPTH} levels deep`);
    expect(groupRepairNotice(en, one, "part").startsWith("This part had")).toBe(true);
    expect(groupRepairOf({})).toBeNull();
    expect(groupRepairOf(null)).toBeNull();
    expect(groupRepairOf({ group_loops_repaired: 0 })).toBeNull();
  });

  it("keeps a group's name as plain text", () => {
    const info = groupRepairOf({ group_loops_repaired: 1, group_loop_names: ["$& {n} $1"] })!;
    expect(groupRepairNotice(en, info)).toContain("(");
    expect(groupRepairNotice(en, info)).toContain("$& {n} $1");
  });

  it("comes out in every language with every fact in it", () => {
    const info = groupRepairOf({ group_loops_repaired: 8, group_loop_names: ["Aa", "Bb", "Cc", "Dd", "Ee"], groups_flattened: 3, groups_flattened_names: ["Zz"] })!;
    for (const lang of LANGUAGE_IDS) {
      const t = (id: string) => translate(lang, id);
      const text = groupRepairNotice(t, info, "file");
      expect(text, lang).not.toMatch(/\{\w+\}/);
      expect(text, lang).toContain("Aa, Bb, Cc, Dd, Ee");
      expect(text, lang).toContain("Zz");
      expect(text, lang).toContain("8");
      expect(text, lang).toContain(String(MAX_GROUP_DEPTH));
      // every message exists in the language's own catalog (null would fall back to English)
      if (lang !== "en") for (const id of ["repair.loops.one", "repair.loops.many", "repair.deep.one", "repair.deep.many", "repair.notice", "repair.join", "repair.more", "repair.names"]) expect(LANGUAGE_TABLE[lang].catalog[id], `${lang} ${id}`).toBeTruthy();
    }
    // and the English wording is not what the others show
    expect(groupRepairNotice((id) => translate("de", id), info)).not.toBe(groupRepairNotice(en, info));
  });
});

describe("the outliner's row cap", () => {
  const flat = (n: number): TreeScene => ({ groups: Array.from({ length: n }, (_, i) => group(i + 1, null)), instances: [] });

  it("draws at most the cap and counts the rest", () => {
    const { rows, more } = flatten(buildTree(flat(100_000)), open);
    expect(rows).toHaveLength(MAX_OUTLINE_ROWS);
    expect(more).toBe(100_000 - MAX_OUTLINE_ROWS);
  });

  it("counts the rows under open groups past the cap, not under closed ones", () => {
    const scene: TreeScene = { groups: [...flat(10).groups, group(11, 10), group(12, 10), group(13, 9)], instances: [] };
    const all = flatten(buildTree(scene), open, null, 3);
    expect(all.rows).toHaveLength(3);
    expect(all.more).toBe(13 - 3);
    const closed = flatten(buildTree(scene), (id) => id !== 10, null, 3);
    expect(closed.more).toBe(13 - 3 - 2);
  });

  it("leaves a realistic model alone", () => {
    const { rows, more } = flatten(buildTree(flat(500)), open);
    expect(rows).toHaveLength(500);
    expect(more).toBe(0);
  });

  it("has the 'more rows' lines in every language", () => {
    for (const lang of LANGUAGE_IDS) {
      for (const id of ["outliner.more.one", "outliner.more.many", "outliner.deep.one", "outliner.deep.many"]) {
        expect(translate(lang, id).length, `${lang} ${id}`).toBeGreaterThan(3);
      }
      expect(translate(lang, "outliner.more.many")).toContain("{n}");
    }
    expect(translate("de", "outliner.more.many")).not.toBe(translate("en", "outliner.more.many"));
  });
});
