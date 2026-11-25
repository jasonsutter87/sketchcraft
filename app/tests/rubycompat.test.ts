// Real extensions, run through SketchCraft's Ruby layer. Their code is not in this repository (licenses differ, and
// some are encrypted): point the variables below at a folder you downloaded each into, and the matching test runs;
// without them the tests are skipped. See docs/guide/12-ruby-extensions.md for where to get each one.
//   SC_EXT_SOLID_INSPECTOR2=/path/to/solid-inspector/src   (the folder that holds tt_solid_inspector2.rb)
import fs from "node:fs";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { runRuby } from "./support/ruby";

vi.setConfig({ testTimeout: 300_000 });

/** every .rb under dir, by path relative to dir (the way an .rbz unpacks) */
function rubyFiles(dir: string): Record<string, string> {
  const out: Record<string, string> = {};
  const walk = (d: string) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.rb$/i.test(e.name)) out[path.relative(dir, p)] = fs.readFileSync(p, "utf8");
    }
  };
  walk(dir);
  return out;
}

const si2 = process.env.SC_EXT_SOLID_INSPECTOR2;

describe.skipIf(!si2)("Solid Inspector² (real, MIT)", () => {
  it("loads, registers its menu and starts its tool", () => {
    const files = rubyFiles(si2!);
    const r = runRuby({ files, entry: ["tt_solid_inspector2"] });
    expect(r.error, r.out).toBeUndefined();
    expect(r.menu.some((m) => /inspect/i.test(m))).toBe(true);
  });

  // a 1 m cube in a group, its top left off: a hole SI² must report
  const q = (z: number, ...p: number[][]) => p;
  void q;
  const face = (id: number, normal: number[], outer: number[][]) => ({ id, normal, outer, inner: [], group: 10, material: null });
  const openBox = {
    groups: [{ id: 10, name: "Box", kind: "group" }],
    faces: [
      face(1, [0, 0, -1], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
      face(2, [0, -1, 0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
      face(3, [1, 0, 0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
      face(4, [0, 1, 0], [[1, 1, 0], [0, 1, 0], [0, 1, 1], [1, 1, 1]]),
      face(5, [-1, 0, 0], [[0, 1, 0], [0, 0, 0], [0, 0, 1], [0, 1, 1]]),
    ],
  };

  it("inspects a group by a click and lists its surface borders", () => {
    const files = rubyFiles(si2!);
    const r = runRuby({
      files,
      entry: ["tt_solid_inspector2"],
      item: "Solid Inspector²",
      model: openBox,
      events: [{ type: "mouse_move", x: 5, y: 5, at: [0.5, 0.5, 0.5], pick: [10] }, { type: "lbutton_up", x: 5, y: 5, at: [0.5, 0.5, 0.5], pick: [10] }],
    });
    expect(r.error, r.out).toBeUndefined();
    expect(r.dialogs.length).toBeGreaterThan(0);
    const calls = r.dialogs.flatMap((d) => d.scripts).filter((s) => s.startsWith("list_errors"));
    expect(calls.length, r.out).toBeGreaterThan(0);
    expect(calls[calls.length - 1], r.out).toMatch(/Surface Border|Hole|border/i);
  });
});
