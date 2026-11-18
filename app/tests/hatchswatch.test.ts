import { describe, expect, it } from "vitest";
import { hatchLabel, importName, patOnly, swatchPath, SWATCH_MAX, type HatchFamily } from "../src/hatchswatch";

/** The segments of a path made of `M x y L x y` pairs. */
const segs = (d: string) =>
  [...d.matchAll(/M([-\d.]+) ([-\d.]+)L([-\d.]+) ([-\d.]+)/g)].map((m) => m.slice(1, 5).map(Number) as [number, number, number, number]);

const flat = (angle: number, spacing: number, dash: number[] = []): HatchFamily => ({ angle, origin: [0, 0], delta: [0, spacing], dash });

describe("swatchPath", () => {
  it("draws horizontal lines across the box, spaced by the pattern times scale", () => {
    const s = segs(swatchPath([{ ...flat(0, 2), origin: [0, 0.5] }], 40, 20, 1, 2)!);
    // lines every 4 px from 1 px up (y measured from the bottom, flipped for SVG)
    const ys = [...new Set(s.map((x) => x[1]))].sort((a, b) => a - b);
    expect(ys).toEqual([3, 7, 11, 15, 19]);
    for (const [x0, y0, x1, y1] of s) {
      expect(y0).toBe(y1);
      expect(Math.min(x0, x1)).toBeCloseTo(0);
      expect(Math.max(x0, x1)).toBeCloseTo(40);
    }
  });

  it("turns the pattern by the angle and keeps every line inside the box", () => {
    const s = segs(swatchPath([flat(0, 3)], 30, 30, 1, 1, 45)!);
    expect(s.length).toBeGreaterThan(5);
    for (const [x0, y0, x1, y1] of s) {
      // y is flipped, so a 45° line rises to the right on screen: slope -1 in SVG coordinates
      expect((y1 - y0) / (x1 - x0)).toBeCloseTo(-1, 1);
      for (const v of [x0, y0, x1, y1]) {
        expect(v).toBeGreaterThanOrEqual(-0.01);
        expect(v).toBeLessThanOrEqual(30.01);
      }
    }
  });

  it("lays dashes from the line's origin, dots as zero-length lines", () => {
    const s = segs(swatchPath([{ angle: 0, origin: [0, 0.5], delta: [0, 100], dash: [2, -1, 0, -1] }], 9, 1, 1)!);
    expect(s.map(([x0, , x1]) => [x0, x1])).toEqual([
      [0, 2],
      [3, 3],
      [4, 6],
      [7, 7],
      [8, 9],
    ]);
  });

  it("gives up (a tint) when the pattern is finer than a pixel or too busy", () => {
    expect(swatchPath([flat(0, 0.1)], 40, 20, 1)).toBeNull();
    expect(swatchPath([flat(0, 1, [0.1, -0.1])], 40, 20, 1)).toBeNull();
    expect(swatchPath([flat(0, 0.7, [1, -0.5])], 2000, 2000, 1)).toBeNull();
    expect(SWATCH_MAX).toBeGreaterThan(100);
  });
});

describe("labels and import names", () => {
  it("says what a hatch is", () => {
    expect(hatchLabel(null)).toBe("No hatch");
    expect(hatchLabel({ pattern: "brick", color: "#000000", weight_mm: 0.18, scale: 2, angle: 30, scale_mode: "model" })).toBe("brick, x2, 30°, 0.18 mm, model scale");
    expect(hatchLabel({ pattern: "diagonal", color: "#000000", weight_mm: 0.13, scale: 1, angle: 0, scale_mode: "paper" })).toBe("diagonal, 0.13 mm");
  });

  it("keeps an imported pattern's name unless a built-in has it", () => {
    expect(importName("Shingle", ["brick", "diagonal"])).toBe("Shingle");
    expect(importName("BRICK", ["brick", "diagonal"])).toBe("BRICK-file");
    expect(importName("b".repeat(63), ["b".repeat(63)]).length).toBeLessThanOrEqual(63);
  });

  it("takes one pattern out of a .pat text, with the units line in force", () => {
    const text = ";%UNITS=INCH\r\n*One, first\r\n0,0,0,0,1\r\n*Two\n45,0,0,0,2\n90,0,0,0,2\n*Three\n0,0,0,0,3\n";
    expect(patOnly(text, "two")).toBe(";%UNITS=INCH\n*Two\n45,0,0,0,2\n90,0,0,0,2\n");
    expect(patOnly(text, "One")).toBe(";%UNITS=INCH\n*One, first\n0,0,0,0,1\n");
    expect(patOnly(text, "missing")).toBe("");
  });
});
