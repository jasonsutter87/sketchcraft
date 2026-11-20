import { describe, expect, it } from "vitest";
import { guide } from "../src/guide";
import { translate } from "../src/locale";
import { TOOL_INFO } from "../src/workspace";
import {
  clampUvRadius,
  extendUvStroke,
  falloff,
  makeTri,
  paintMaterialOf,
  Painter,
  pixelToUv,
  sampleRgba,
  stepUvRadius,
  stroke2DCommand,
  stroke3DCommand,
  trisOf,
  uvAt,
  uvToPixel,
  DEFAULT_TPAINT,
  type TBrush,
} from "../src/texturepaint";

const brush = (over: Partial<TBrush> = {}): TBrush => ({ color: [1, 0, 0], opacity: 1, hardness: 1, erase: false, base: [1, 1, 1], ...over });
const white = (w: number, h: number) => new Uint8ClampedArray(new ArrayBuffer(w * h * 4)).fill(255);
const px = (p: Painter, x: number, y: number) => Array.from(p.cur.slice((y * p.w + x) * 4, (y * p.w + x) * 4 + 4));

describe("UV to pixel", () => {
  it("flips v (the picture's top row is v = 1)", () => {
    expect(uvToPixel([0, 1], 100, 50)).toEqual([0, 0]);
    expect(uvToPixel([1, 0], 100, 50)).toEqual([100, 50]);
    expect(uvToPixel([0.25, 0.5], 100, 50)).toEqual([25, 25]);
    const back = pixelToUv(uvToPixel([0.3, 0.7], 256, 128), 256, 128);
    expect(back[0]).toBeCloseTo(0.3, 12);
    expect(back[1]).toBeCloseTo(0.7, 12);
  });
  it("reads a picture's colour at a coordinate, tiling", () => {
    const rgba = new Uint8ClampedArray([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255]);
    expect(sampleRgba(rgba, 2, 2, [0.25, 0.75])).toEqual([1, 0, 0]);
    expect(sampleRgba(rgba, 2, 2, [0.75, 0.75])).toEqual([0, 1, 0]);
    expect(sampleRgba(rgba, 2, 2, [0.25, 0.25])).toEqual([0, 0, 1]);
    expect(sampleRgba(rgba, 2, 2, [1.25, 1.75])).toEqual([1, 0, 0]);
  });
});

describe("brush falloff (the engine's maths)", () => {
  it("is hard inside the hardness, then smooth to nothing", () => {
    expect(falloff(0, 1, 0.5)).toBe(1);
    expect(falloff(0.5, 1, 0.5)).toBe(1);
    expect(falloff(1, 1, 0.5)).toBe(0);
    expect(falloff(0.75, 1, 0.5)).toBeCloseTo(0.5, 12);
    expect(falloff(0.99, 1, 1)).toBe(1);
    expect(falloff(0.5, 1, 0)).toBeCloseTo(0.5, 12);
    expect(falloff(0.2, 1, 0)).toBeGreaterThan(falloff(0.6, 1, 0));
  });
});

describe("2D brush", () => {
  it("paints a disc where the coordinate says and nothing else", () => {
    const p = new Painter(white(64, 64), 64, 64, brush());
    p.dab2D([0.5, 0.5], 0.25);
    expect(px(p, 32, 32)).toEqual([255, 0, 0, 255]);
    expect(px(p, 32 + 15, 32)).toEqual([255, 0, 0, 255]);
    expect(px(p, 32 + 17, 32)).toEqual([255, 255, 255, 255]);
    expect(px(p, 0, 0)).toEqual([255, 255, 255, 255]);
    expect(p.takeDirty()).not.toBeNull();
    expect(p.takeDirty()).toBeNull();
  });
  it("never piles up: overlapping dabs take the strongest weight", () => {
    const a = new Painter(white(32, 32), 32, 32, brush({ hardness: 0, opacity: 0.6 }));
    const b = new Painter(white(32, 32), 32, 32, brush({ hardness: 0, opacity: 0.6 }));
    a.dab2D([0.5, 0.5], 0.3);
    for (let i = 0; i < 5; i++) b.dab2D([0.5, 0.5], 0.3);
    expect(Array.from(b.cur)).toEqual(Array.from(a.cur));
    // and the order of dabs does not matter
    const c = new Painter(white(32, 32), 32, 32, brush({ hardness: 0, opacity: 0.6 }));
    const d = new Painter(white(32, 32), 32, 32, brush({ hardness: 0, opacity: 0.6 }));
    c.dab2D([0.3, 0.5], 0.2);
    c.dab2D([0.6, 0.5], 0.2);
    d.dab2D([0.6, 0.5], 0.2);
    d.dab2D([0.3, 0.5], 0.2);
    expect(Array.from(d.cur)).toEqual(Array.from(c.cur));
  });
  it("erases toward the base colour", () => {
    const p = new Painter(white(16, 16), 16, 16, brush({ erase: true, base: [0, 0, 1] }));
    p.dab2D([0.5, 0.5], 0.5);
    expect(px(p, 8, 8)).toEqual([0, 0, 255, 255]);
  });
  it("blends by opacity", () => {
    const p = new Painter(white(16, 16), 16, 16, brush({ color: [0, 0, 0], opacity: 0.5 }));
    p.dab2D([0.5, 0.5], 0.5);
    expect(px(p, 8, 8)).toEqual([128, 128, 128, 255]);
  });
});

describe("3D brush", () => {
  const tri = (p: number[], px: [number, number][]) => makeTri(p as never, px as never)!;
  it("paints both sides of a UV seam: distance is measured in 3D", () => {
    // two triangles that touch in 3D at x = 1 and sit far apart on the picture
    const a = tri([0, 0, 0, 1, 0, 0, 0, 1, 0], [[2, 30], [30, 30], [2, 2]]);
    const b = tri([1, 0, 0, 2, 0, 0, 1, 1, 0], [[34, 62], [62, 62], [34, 34]]);
    const p = new Painter(white(64, 64), 64, 64, brush());
    p.dab3D([a, b], [1, 0.1, 0], 0.3);
    const any = (x0: number, x1: number, y0: number, y1: number) => {
      for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) if (px(p, x, y)[1] === 0) return true;
      return false;
    };
    expect(any(2, 32, 2, 32)).toBe(true);
    expect(any(34, 64, 34, 64)).toBe(true);
    const far = new Painter(white(64, 64), 64, 64, brush());
    far.dab3D([a, b], [1.8, 0.1, 0], 0.3);
    let hit = false;
    for (let y = 2; y < 32; y++) for (let x = 2; x < 32; x++) if (far.cur[(y * 64 + x) * 4 + 1] === 0) hit = true;
    expect(hit).toBe(false);
  });
  it("stays inside the radius", () => {
    const a = tri([0, 0, 0, 4, 0, 0, 0, 4, 0], [[0, 64], [64, 64], [0, 0]]);
    const p = new Painter(white(64, 64), 64, 64, brush());
    p.dab3D([a], [0.5, 0.5, 0], 0.5);
    expect(px(p, 8, 56)).toEqual([255, 0, 0, 255]);
    expect(px(p, 20, 56)).toEqual([255, 255, 255, 255]);
    expect(px(p, 40, 10)).toEqual([255, 255, 255, 255]);
  });
  it("skips a triangle that has no area", () => {
    expect(makeTri([0, 0, 0, 1, 0, 0, 2, 0, 0], [[0, 0], [1, 0], [2, 0]])).toBeNull();
  });
  it("takes its triangles from the scene by material", () => {
    const s = { positions: [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1], uvs: [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1], triTex: [-1, 1], textures: ["A", "B"] };
    expect(trisOf(s, "A", 8, 8)).toHaveLength(0);
    const t = trisOf(s, "B", 8, 8);
    expect(t).toHaveLength(1);
    expect(t[0].p[2]).toBe(1);
  });
  it("finds the coordinate under a point on a scene triangle", () => {
    const s = { positions: [0, 0, 0, 2, 0, 0, 0, 2, 0], uvs: [0, 0, 1, 0, 0, 1] };
    const uv = uvAt(s, 0, [1, 0.5, 0])!;
    expect(uv[0]).toBeCloseTo(0.5, 9);
    expect(uv[1]).toBeCloseTo(0.25, 9);
  });
});

describe("stroke batching", () => {
  it("lays points along a drag on the picture", () => {
    const pts: [number, number][] = [];
    expect(extendUvStroke(pts, [0.1, 0.1], 0.1)).toEqual([[0.1, 0.1]]);
    expect(extendUvStroke(pts, [0.11, 0.1], 0.1)).toEqual([]);
    const more = extendUvStroke(pts, [0.4, 0.1], 0.1);
    expect(more.length).toBe(10);
    expect(pts.length).toBe(11);
    expect(more[0][0]).toBeCloseTo(0.13, 9);
  });
  it("sends one command for a whole 3D stroke, with the brush settings", () => {
    const cmd = stroke3DCommand({ material: "Paint 1", points: [[0, 0, 0], [0.1234567891, 0, 0]], radius: 0.05, color: [1, 0.5, 0], settings: { ...DEFAULT_TPAINT, hardness: 0.25, opacity: 0.5 }, erase: false })!;
    expect(cmd.cmd).toBe("texture_paint_stroke");
    expect(cmd.material).toBe("Paint 1");
    expect(cmd.points).toEqual([[0, 0, 0], [0.123457, 0, 0]]);
    expect(cmd.hardness).toBe(0.25);
    expect(cmd.opacity).toBe(0.5);
    expect(cmd.color).toEqual([1, 0.5, 0]);
    expect(stroke3DCommand({ material: "x", points: [], radius: 1, color: [0, 0, 0], settings: DEFAULT_TPAINT, erase: false })).toBeNull();
  });
  it("erase sends no colour; a long stroke is thinned", () => {
    const pts = Array.from({ length: 5000 }, (_, i) => [i, 0, 0] as [number, number, number]);
    const cmd = stroke3DCommand({ material: "m", points: pts, radius: 1, color: [1, 1, 1], settings: DEFAULT_TPAINT, erase: true })!;
    expect(cmd.erase).toBe(true);
    expect("color" in cmd).toBe(false);
    expect((cmd.points as unknown[]).length).toBeLessThanOrEqual(1500);
  });
  it("sends a picture stroke in tiles and keeps the radius in range", () => {
    const cmd = stroke2DCommand({ material: "m", points: [[0.5, 0.5]], radius: 5, color: [0, 1, 0], settings: DEFAULT_TPAINT, erase: false })!;
    expect(cmd.uv_points).toEqual([[0.5, 0.5]]);
    expect(cmd.uv_radius).toBe(0.5);
    expect(clampUvRadius(NaN)).toBe(DEFAULT_TPAINT.uvRadius);
    expect(stepUvRadius(0.1, 1)).toBeCloseTo(0.125);
    expect(stepUvRadius(0.1, -1)).toBeCloseTo(0.08);
  });
});

describe("what the selection paints", () => {
  const scene = {
    materials: [{ name: "Brick", texture: { data: "x", width: 1, height: 1, project: null } }, { name: "Red", texture: null }],
    faces: [{ id: 1, material: "Red", group: null }, { id: 2, material: "Brick", group: 7 }],
  };
  it("is the picture of a selected face or of a face in a selected group", () => {
    expect(paintMaterialOf(scene as never, { faces: [2], groups: [] })).toBe("Brick");
    expect(paintMaterialOf(scene as never, { faces: [], groups: [7] })).toBe("Brick");
    expect(paintMaterialOf(scene as never, { faces: [1], groups: [] })).toBeNull();
  });
});

describe("the tool is wired in", () => {
  it("has a name, a step hint and Italian words", () => {
    expect(TOOL_INFO.tpaint.name).toContain("Texture Paint");
    const g = guide("tpaint", { selected: 0, op: null }, "en");
    expect(g.title).toBe("Texture Paint");
    expect(g.step).toMatch(/paint texture/);
    expect(guide("tpaint", { selected: 0, op: "tpaint" }).step).toMatch(/one undo step/);
    for (const k of ["tool.tpaint", "guide.tpaint.step", "tp.title", "tp.create", "menu.newPaintTexture", "menu.exportTexture", "uv.mode.paint"]) {
      expect(translate("it", k)).not.toBe(k);
      expect(translate("it", k)).not.toBe(translate("en", k));
    }
  });
});
