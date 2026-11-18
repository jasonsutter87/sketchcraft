import { describe, expect, it } from "vitest";
import { hatchInk, MAX_DASHES, MAX_FAMILIES, pack, shaderCan, type Family } from "../src/viewport/hatchshader";
import { PX_PER_MM } from "../src/viewport/linepens";

// horizontal lines 10 mm apart through the origin
const rows: Family = { angle: 0, base: [0, 0], offset: [0, 10], dash: [] };
// 45 degree lines, 10 mm apart across, with the next line shifted 3 mm along (as .pat families are)
const diag: Family = { angle: 45, base: [2, 1], offset: [-10 * Math.SQRT1_2 + 3 * Math.SQRT1_2, 10 * Math.SQRT1_2 + 3 * Math.SQRT1_2], dash: [] };

describe("hatchInk: what the shader paints", () => {
  it("inks the lines and leaves the paper between them", () => {
    // 0.1 mm of plane per pixel: a 1 px line (0.13 mm pen is under 1 px) covers about 0.1 mm
    for (const y of [-20, 0, 10, 30]) expect(hatchInk([7.3, y], [rows], 0.13, 0.1)).toBe(1);
    expect(hatchInk([7.3, 5], [rows], 0.13, 0.1)).toBe(0);
    expect(hatchInk([7.3, 0.2], [rows], 0.13, 0.1)).toBe(0);
  });

  it("draws the true weight in screen pixels: a 0.5 mm pen is 3 px wide at 6 px per paper mm", () => {
    const mmPerPx = 0.1;
    const px = 0.5 * PX_PER_MM; // 3 px
    // inked within half the width (blurred over its last pixel), nothing past half a pixel beyond it
    expect(hatchInk([0, ((px / 2) * mmPerPx) * 0.9], [rows], 0.5, mmPerPx)).toBeGreaterThan(0.5);
    expect(hatchInk([0, (px / 2 + 0.6) * mmPerPx], [rows], 0.5, mmPerPx)).toBe(0);
    // the thin pen is narrower at the same spot
    expect(hatchInk([0, 1.2 * mmPerPx], [rows], 0.13, mmPerPx)).toBe(0);
    expect(hatchInk([0, 1.2 * mmPerPx], [rows], 0.5, mmPerPx)).toBeGreaterThan(0.5);
    // and the width stays the same on screen when zoomed (mm per pixel changes, pixels don't)
    expect(hatchInk([0, 1.2 * 0.02], [rows], 0.5, 0.02)).toBeGreaterThan(0.5);
  });

  it("finds lines of a turned family shifted along, as the engine's generator lays them", () => {
    const n = [-Math.SQRT1_2, Math.SQRT1_2];
    const d = [Math.SQRT1_2, Math.SQRT1_2];
    for (const i of [-2, 0, 1, 3]) {
      const p = [diag.base[0] + diag.offset[0] * i, diag.base[1] + diag.offset[1] * i];
      for (const t of [-15, 0, 4.5]) expect(hatchInk([p[0] + d[0] * t, p[1] + d[1] * t], [diag], 0.13, 0.05)).toBeGreaterThan(0.99);
      expect(hatchInk([p[0] + n[0] * 5, p[1] + n[1] * 5], [diag], 0.13, 0.05)).toBe(0);
    }
  });

  it("lays dashes from each line's own origin, leaves the gaps, and draws a 0 dash as a dot", () => {
    const dashed: Family = { angle: 0, base: [0, 0], offset: [0, 10], dash: [4, -2] };
    for (const x of [0.5, 3.5, 6.5, -2.5]) expect(hatchInk([x, 0], [dashed], 0.13, 0.05)).toBe(1);
    for (const x of [5, 11, -1]) expect(hatchInk([x, 0], [dashed], 0.13, 0.05)).toBe(0);
    const dots: Family = { angle: 0, base: [0, 0], offset: [0, 10], dash: [0, -5] };
    expect(hatchInk([0, 0], [dots], 0.13, 0.05)).toBe(1);
    expect(hatchInk([10, 0], [dots], 0.13, 0.05)).toBe(1);
    expect(hatchInk([2.5, 0], [dots], 0.13, 0.05)).toBe(0);
    // a dot is round: as wide along the line as across it
    expect(hatchInk([0.01, 0.01], [dots], 0.13, 0.05)).toBeGreaterThan(0.5);
    expect(hatchInk([0.1, 0], [dots], 0.13, 0.05)).toBe(0);
  });

  it("combines families (a cross hatch inks both)", () => {
    const cols: Family = { angle: 90, base: [0, 0], offset: [-10, 0], dash: [] };
    expect(hatchInk([5, 0], [rows, cols], 0.13, 0.1)).toBe(1);
    expect(hatchInk([0, 5], [rows, cols], 0.13, 0.1)).toBeCloseTo(1, 9);
    expect(hatchInk([5, 5], [rows, cols], 0.13, 0.1)).toBe(0);
  });
});

describe("pack and limits", () => {
  it("packs each family as the shader reads it, and pads the arrays", () => {
    const p = pack([rows, { angle: 90, base: [1, 2], offset: [-4, 0], dash: [3, -1, 0, -1] }]);
    expect(p.n).toBe(2);
    expect(p.a.length).toBe(MAX_FAMILIES);
    expect(p.a[0].toArray().map((v) => Math.round(v * 1e6) / 1e6)).toEqual([1, 0, 10, 0]);
    expect(p.a[1].z).toBeCloseTo(4);
    expect(p.a[1].w).toBeCloseTo(5);
    expect(p.b[1].toArray()).toEqual([1, 2, -4, 0]);
    expect(p.d.slice(MAX_DASHES, MAX_DASHES + 4)).toEqual([3, -1, 0, -1]);
    expect(p.k.slice(0, 3)).toEqual([0, 4, 0]);
  });

  it("leaves out a family whose lines don't step apart", () => {
    expect(pack([{ angle: 0, base: [0, 0], offset: [5, 0], dash: [] }]).n).toBe(0);
  });

  it("says when the engine's lines must be drawn instead", () => {
    expect(shaderCan([rows])).toBe(true);
    expect(shaderCan([])).toBe(false);
    expect(shaderCan(new Array(MAX_FAMILIES + 1).fill(rows))).toBe(false);
    expect(shaderCan([{ ...rows, dash: new Array(MAX_DASHES + 1).fill(1) }])).toBe(false);
    expect(shaderCan([{ ...rows, angle: NaN }])).toBe(false);
  });
});
