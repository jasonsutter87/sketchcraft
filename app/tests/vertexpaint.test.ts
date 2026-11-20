import { describe, expect, it } from "vitest";
import { guide } from "../src/guide";
import { translate } from "../src/locale";
import { TOOL_INFO } from "../src/workspace";
import { autoRadius, clampRadius, clampStrength, DEFAULT_VPAINT, effectiveRadius, extendStroke, MAX_STROKE_POINTS, ringPoints, sceneDiagonal, SPACING, stepRadius, strokeCommand, thin, type P3 } from "../src/vertexpaint";

describe("brush size", () => {
  it("picks a size from the model, and never an absurd one", () => {
    expect(autoRadius(10)).toBeCloseTo(0.6);
    expect(autoRadius(0)).toBe(0.1);
    expect(autoRadius(NaN)).toBe(0.1);
    expect(autoRadius(1e9)).toBe(1000);
    expect(autoRadius(1e-9)).toBe(0.001);
  });

  it("uses the setting, or the automatic size while it is 0", () => {
    expect(effectiveRadius({ ...DEFAULT_VPAINT, radius: 0 }, 10)).toBeCloseTo(0.6);
    expect(effectiveRadius({ ...DEFAULT_VPAINT, radius: 0.25 }, 10)).toBe(0.25);
  });

  it("[ and ] change it by a quarter and stay in range", () => {
    expect(stepRadius(1, 1)).toBeCloseTo(1.25);
    expect(stepRadius(1, -1)).toBeCloseTo(0.8);
    expect(stepRadius(0.001, -1)).toBe(0.001);
    expect(stepRadius(1000, 1)).toBe(1000);
    expect(clampRadius(NaN)).toBe(0.1);
    expect(clampStrength(5)).toBe(1);
    expect(clampStrength(-1)).toBe(0.01);
  });

  it("measures the scene once", () => {
    const s = { positions: [0, 0, 0, 3, 4, 0, 0, 0, 12] };
    expect(sceneDiagonal(s)).toBeCloseTo(Math.hypot(3, 4, 12));
    expect(sceneDiagonal({ positions: [] })).toBe(0);
  });
});

describe("stroke batching", () => {
  it("starts with the first sample", () => {
    const pts: P3[] = [];
    expect(extendStroke(pts, [0, 0, 0], 1)).toEqual([[0, 0, 0]]);
    expect(pts).toHaveLength(1);
  });

  it("ignores moves smaller than the spacing", () => {
    const pts: P3[] = [[0, 0, 0]];
    expect(extendStroke(pts, [SPACING * 0.5, 0, 0], 1)).toEqual([]);
    expect(pts).toHaveLength(1);
  });

  it("lays points along a long move so a fast drag leaves no gaps", () => {
    const pts: P3[] = [[0, 0, 0]];
    const added = extendStroke(pts, [3, 0, 0], 1);
    expect(added.length).toBe(Math.floor(3 / SPACING));
    for (let i = 1; i < pts.length; i++) expect(Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1])).toBeLessThanOrEqual(SPACING + 1e-9);
    expect(pts[pts.length - 1][0]).toBeLessThanOrEqual(3);
  });

  it("thins a very long stroke to the limit, keeping both ends", () => {
    const pts: P3[] = Array.from({ length: 10_000 }, (_, i) => [i, 0, 0]);
    const t = thin(pts);
    expect(t).toHaveLength(MAX_STROKE_POINTS);
    expect(t[0]).toEqual([0, 0, 0]);
    expect(t[t.length - 1]).toEqual([9999, 0, 0]);
    const short: P3[] = [[0, 0, 0]];
    expect(thin(short)).toBe(short);
  });
});

describe("the stroke command", () => {
  const base = { targets: [7, 7, 9], points: [[0, 0, 0], [0.1, 0, 0]] as P3[], color: [1, 0.5, 0] as [number, number, number], radius: 0.25, settings: { ...DEFAULT_VPAINT, strength: 0.4 }, erase: false };

  it("sends one vertex_paint with every point, and each target once", () => {
    const c = strokeCommand(base)!;
    expect(c.cmd).toBe("vertex_paint");
    expect(c.faces).toEqual([7, 9]);
    expect(c.points).toEqual([[0, 0, 0], [0.1, 0, 0]]);
    expect(c.radius).toBe(0.25);
    expect(c.strength).toBe(0.4);
    expect(c.falloff).toBe("smooth");
    expect(c.color).toEqual([1, 0.5, 0]);
    expect(c.erase).toBeUndefined();
  });

  it("Alt erases instead of sending a colour", () => {
    const c = strokeCommand({ ...base, erase: true })!;
    expect(c.erase).toBe(true);
    expect(c.color).toBeUndefined();
  });

  it("sends nothing for an empty stroke", () => {
    expect(strokeCommand({ ...base, points: [] })).toBeNull();
    expect(strokeCommand({ ...base, targets: [] })).toBeNull();
  });

  it("keeps numbers in range and tidy", () => {
    const c = strokeCommand({ ...base, color: [2, -1, 0.123456789], radius: 1e9, settings: { ...DEFAULT_VPAINT, strength: 9 }, points: [[0.12345678901, 0, 0]] })!;
    expect(c.color).toEqual([1, 0, 0.123457]);
    expect(c.radius).toBe(1000);
    expect(c.strength).toBe(1);
    expect(c.points).toEqual([[0.123457, 0, 0]]);
  });
});

describe("the brush cursor", () => {
  it("is a ring of points all one radius from the centre, in the plane of the surface", () => {
    const c: P3 = [1, 2, 3];
    const n: P3 = [0, 0, 2];
    const ring = ringPoints(c, n, 0.5, 24);
    expect(ring).toHaveLength(24);
    for (const p of ring) {
      expect(Math.hypot(p[0] - c[0], p[1] - c[1], p[2] - c[2])).toBeCloseTo(0.5);
      expect(p[2]).toBeCloseTo(3);
    }
  });

  it("copes with a normal along the helper axis", () => {
    const ring = ringPoints([0, 0, 0], [1, 0, 0], 1, 8);
    for (const p of ring) {
      expect(p[0]).toBeCloseTo(0);
      expect(Math.hypot(p[1], p[2])).toBeCloseTo(1);
    }
  });
});

describe("the tool's words", () => {
  it("is in the tool list with a name", () => {
    expect(TOOL_INFO.vpaint.name).toMatch(/Vertex Paint/);
    expect(translate("en", "tool.vpaint")).toBe(TOOL_INFO.vpaint.name);
  });

  it("explains itself before and during a stroke, in English and Italian", () => {
    const idle = guide("vpaint", { selected: 0, op: null });
    expect(idle.title).toBe("Vertex Paint");
    expect(idle.step).toMatch(/Drag/);
    expect(idle.keys.map((k) => k[0])).toEqual(["Alt", "[ ]"]);
    const busy = guide("vpaint", { selected: 0, op: "vpaint" });
    expect(busy.step).toMatch(/one undo step/);
    const it = guide("vpaint", { selected: 0, op: null }, "it");
    expect(it.step).toMatch(/Trascina/);
    expect(translate("it", "vp.title")).not.toBe(translate("en", "vp.title"));
    expect(translate("it", "menu.vertexColors")).not.toBe(translate("en", "menu.vertexColors"));
  });
});
