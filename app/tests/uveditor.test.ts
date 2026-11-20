import { describe, expect, it } from "vitest";
import {
  about, after, apply, area, boxOf, cornerKey, coverage, edgeIdsFor, fitView, flip, fromScreen, hitEdge, hitIsland, hitVertex, IDENT, indexOf, inPolygon, islandCorners, islandFaces, islandPaths, islandsSelected, isFlat, livePins, marquee, marqueeIslands, nudge, outside, panBy,
  pictureMaterial, pinList, pivotOf, segDistance, selectedEdges, setCommand, together, toScreen, translation, transformed, wholeIslandsCommand, zoomAt,
  type UvFace, type UvInfo, type V2, type View,
} from "../src/uveditor";

const close = (a: V2, b: V2, eps = 1e-9) => expect(Math.abs(a[0] - b[0]) < eps && Math.abs(a[1] - b[1]) < eps, `${a} vs ${b}`).toBe(true);

/** Two quads sharing an edge (one island: faces 1 and 2) and a third, apart (island 1: face 3).
 *  Face 1 corners: v10 (0,0) v11 (.2,0) v12 (.2,.2) v13 (0,.2); face 2: v11 (.2,0) v14 (.4,0) v15 (.4,.2) v12 (.2,.2). */
function sample(): UvInfo {
  const f = (id: number, verts: number[], uv: V2[], island: number, material: string | null = "Brick"): UvFace => ({ id, material, verts, loops: [verts.length], uv, island });
  return {
    faces: [
      f(1, [10, 11, 12, 13], [[0, 0], [0.2, 0], [0.2, 0.2], [0, 0.2]], 0),
      f(2, [11, 14, 15, 12], [[0.2, 0], [0.4, 0], [0.4, 0.2], [0.2, 0.2]], 0),
      f(3, [20, 21, 22, 23], [[0.6, 0.6], [0.9, 0.6], [0.9, 0.9], [0.6, 0.9]], 1),
    ],
    without: [],
    islands: 2,
    seams: [],
  };
}

const view = (): View => ({ cx: 0.5, cy: 0.5, zoom: 400, w: 600, h: 500 });

describe("the view", () => {
  it("maps UV space to the screen with v up, and back", () => {
    const v = view();
    close(toScreen(v, [0.5, 0.5]), [300, 250]);
    // v up: a higher v is higher on the screen (a smaller y)
    expect(toScreen(v, [0.5, 0.9])[1]).toBeLessThan(250);
    close(toScreen(v, [0.75, 0.5]), [400, 250]);
    for (const p of [[0, 0], [1, 1], [0.3, 0.7]] as V2[]) close(fromScreen(v, toScreen(v, p)), p);
  });

  it("fits the whole square with some air round it", () => {
    const v = fitView(600, 500);
    const [a, b] = [toScreen(v, [0, 0]), toScreen(v, [1, 1])];
    expect(a[0]).toBeGreaterThan(0);
    expect(b[0]).toBeLessThan(600);
    expect(a[1]).toBeLessThan(500);
    expect(b[1]).toBeGreaterThan(0);
    expect(fitView(10, 10).zoom).toBeGreaterThan(0);
  });

  it("zooms about the point under the pointer and clamps", () => {
    const v = view();
    const at: V2 = [450, 120];
    const before = fromScreen(v, at);
    const z = zoomAt(v, at, 2);
    expect(z.zoom).toBe(800);
    close(fromScreen(z, at), before);
    expect(zoomAt(v, at, 1e9).zoom).toBeLessThanOrEqual(20000);
    expect(zoomAt(v, at, 1e-9).zoom).toBeGreaterThanOrEqual(8);
  });

  it("pans with the drag: content follows the pointer", () => {
    const v = view();
    const p = panBy(v, 50, -30);
    close(toScreen(p, [0.5, 0.5]), [350, 220]);
  });
});

describe("picking", () => {
  it("finds the nearest point within a radius, and the corners that sit with it", () => {
    const info = sample();
    const ix = indexOf(info);
    const v = view();
    const s = toScreen(v, [0.2, 0]);
    const hit = hitVertex(info, ix, v, [s[0] + 3, s[1] - 2])!;
    // vertex 11 is a corner of both faces 1 and 2, at the same place: one pick moves both
    expect(hit.sort()).toEqual([cornerKey(1, 1), cornerKey(2, 0)].sort());
    expect(hitVertex(info, ix, v, [s[0] + 40, s[1]])).toBeNull();
    // the nearer one wins
    const near = toScreen(v, [0.4, 0.2]);
    expect(hitVertex(info, ix, v, [near[0] - 2, near[1] + 2])).toEqual([cornerKey(2, 2)]);
  });

  it("does not join corners of one vertex that sit apart (across a seam)", () => {
    const info = sample();
    // pull face 2's copy of vertex 11 away: a seam
    info.faces[1].uv[0] = [0.25, -0.1];
    const ix = indexOf(info);
    expect(together(ix, 1, 1)).toEqual([cornerKey(1, 1)]);
    expect(together(ix, 2, 0)).toEqual([cornerKey(2, 0)]);
  });

  it("finds an edge near a point and gives both its ends", () => {
    const info = sample();
    const ix = indexOf(info);
    const v = view();
    const mid = toScreen(v, [0.1, 0]);
    const hit = hitEdge(info, ix, v, [mid[0], mid[1] + 3])!;
    expect(hit.sort()).toEqual([cornerKey(1, 0), cornerKey(1, 1), cornerKey(2, 0)].sort());
    expect(hitEdge(info, ix, v, [mid[0], mid[1] + 40])).toBeNull();
  });

  it("measures the distance to a segment, including past its ends", () => {
    expect(segDistance([1, 1], [0, 0], [2, 0])).toBeCloseTo(1);
    expect(segDistance([-1, 0], [0, 0], [2, 0])).toBeCloseTo(1);
    expect(segDistance([3, 4], [0, 0], [0, 0])).toBeCloseTo(5);
  });

  it("tests points in a polygon, concave ones included", () => {
    const l: V2[] = [[0, 0], [2, 0], [2, 1], [1, 1], [1, 2], [0, 2]];
    expect(inPolygon([0.5, 0.5], l)).toBe(true);
    expect(inPolygon([1.5, 1.5], l)).toBe(false);
    expect(inPolygon([0.5, 1.5], l)).toBe(true);
  });

  it("finds the island under a point and its faces and corners", () => {
    const info = sample();
    expect(hitIsland(info, [0.3, 0.1])).toBe(0);
    expect(hitIsland(info, [0.75, 0.75])).toBe(1);
    expect(hitIsland(info, [0.5, 0.4])).toBeNull();
    expect(islandFaces(info, 0)).toEqual([1, 2]);
    expect(islandCorners(info, 1)).toHaveLength(4);
  });

  it("reports an island as selected only when every corner is", () => {
    const info = sample();
    const all = new Set(islandCorners(info, 1));
    expect(islandsSelected(info, all)).toEqual([1]);
    all.delete(cornerKey(3, 0));
    expect(islandsSelected(info, all)).toEqual([]);
  });

  it("box-selects corners in any drag direction, and islands the box touches", () => {
    const info = sample();
    const v = view();
    const [a, b] = [toScreen(v, [-0.05, 0.25]), toScreen(v, [0.45, -0.05])];
    const got = marquee(info, v, a, b);
    expect(got).toHaveLength(8);
    expect(marquee(info, v, b, a)).toHaveLength(8);
    expect(marqueeIslands(info, v, a, b)).toEqual([0]);
    const [c, d] = [toScreen(v, [0, 0]), toScreen(v, [1, 1])];
    expect(marqueeIslands(info, v, c, d).sort()).toEqual([0, 1]);
  });
});

describe("transforms", () => {
  it("applies a matrix p' = [a c; b d] p + [e f]", () => {
    close(apply([2, 0, 0, 3, 1, 1], [1, 1]), [3, 4]);
    close(apply(translation(0.1, -0.2), [0.5, 0.5]), [0.6, 0.3]);
  });

  it("turns counter-clockwise about a pivot and leaves the pivot put", () => {
    const m = about([0.5, 0.5], 90);
    close(apply(m, [0.5, 0.5]), [0.5, 0.5]);
    close(apply(m, [1, 0.5]), [0.5, 1]);
  });

  it("scales about a pivot, evenly or per axis", () => {
    close(apply(about([1, 1], 0, 2), [2, 2]), [3, 3]);
    close(apply(about([0, 0], 0, 2, 3), [1, 1]), [2, 3]);
  });

  it("flips a pivot's two sides over", () => {
    close(apply(flip([0.5, 0.5], "h"), [0.2, 0.3]), [0.8, 0.3]);
    close(apply(flip([0.5, 0.5], "v"), [0.2, 0.3]), [0.2, 0.7]);
  });

  it("composes: the second after the first", () => {
    const t = translation(1, 0);
    const r = about([0, 0], 90);
    close(apply(after(t, r), [1, 0]), [1, 1]);
    close(apply(after(r, t), [1, 0]), [0, 2]);
    expect(after(IDENT, t)).toEqual(t);
  });

  it("calls a squashed or broken matrix flat", () => {
    expect(isFlat(about([0, 0], 0, 0, 1))).toBe(true);
    expect(isFlat([NaN, 0, 0, 1, 0, 0])).toBe(true);
    expect(isFlat(about([0, 0], 30, 2))).toBe(false);
    expect(isFlat(flip([0, 0], "h"))).toBe(false);
  });

  it("finds the middle of the chosen corners' box", () => {
    const info = sample();
    close(pivotOf(info, new Set(islandCorners(info, 0)))!, [0.2, 0.1]);
    expect(pivotOf(info, new Set())).toBeNull();
    expect(boxOf(info, null)).toEqual({ x0: 0, y0: 0, x1: 0.9, y1: 0.9 });
  });

  it("moves only the chosen corners and leaves the rest and the original alone", () => {
    const info = sample();
    const sel = new Set([cornerKey(1, 1), cornerKey(2, 0)]);
    const out = transformed(info, sel, translation(0.05, 0.05));
    close(out.faces[0].uv[1], [0.25, 0.05]);
    close(out.faces[1].uv[0], [0.25, 0.05]);
    close(out.faces[0].uv[0], [0, 0]);
    close(info.faces[0].uv[1], [0.2, 0]);
    expect(out.faces[2]).toBe(info.faces[2]);
  });
});

describe("what goes back to the engine", () => {
  it("sends one uv_set with just the faces that changed, every corner of each", () => {
    const info = sample();
    const sel = new Set([cornerKey(3, 2)]);
    const cmd = setCommand(info, transformed(info, sel, translation(0.1, 0)), 1)!;
    expect(cmd.cmd).toBe("uv_set");
    expect(Object.keys(cmd.uv as object)).toEqual(["3"]);
    expect((cmd.uv as Record<string, V2[]>)["3"]).toHaveLength(4);
    expect((cmd.uv as Record<string, V2[]>)["3"][2]).toEqual([1, 0.9]);
    expect(cmd.set).toBeUndefined();
    expect(setCommand(info, transformed(info, sel, translation(0.1, 0)), 2)!.set).toBe(2);
    // nothing moved, nothing sent
    expect(setCommand(info, info, 1)).toBeNull();
  });

  it("moves whole islands with one uv_transform, and a part of one with uv_set", () => {
    const info = sample();
    const whole = new Set([...islandCorners(info, 0), ...islandCorners(info, 1)]);
    const cmd = wholeIslandsCommand(info, whole, translation(0.1, 0), 1)!;
    expect(cmd.cmd).toBe("uv_transform");
    expect((cmd.faces as number[]).sort()).toEqual([1, 2, 3]);
    expect(cmd.matrix).toEqual([1, 0, 0, 1, 0.1, 0]);
    const part = new Set(islandCorners(info, 0).slice(1));
    expect(wholeIslandsCommand(info, part, translation(0.1, 0), 1)).toBeNull();
  });

  it("finds the model edges between chosen corners and their ids", () => {
    const info = sample();
    const sel = new Set([cornerKey(1, 1), cornerKey(1, 2), cornerKey(2, 0), cornerKey(2, 3)]);
    const pairs = selectedEdges(info, sel);
    // 11-12 is an edge of both faces: one model edge
    expect(pairs).toEqual([[11, 12]]);
    const edges = [{ id: 7, va: 12, vb: 11 }, { id: 8, va: 10, vb: 11 }, { id: 9 }];
    expect(edgeIdsFor(pairs, edges)).toEqual([7]);
    expect(edgeIdsFor([], edges)).toEqual([]);
  });

  it("lists pins as [face, corner, u, v] where the corners are now, and drops pins on faces that went", () => {
    const info = sample();
    const pins = new Set([cornerKey(1, 2), cornerKey(99, 0)]);
    expect(pinList(info, pins)).toEqual([[1, 2, 0.2, 0.2]]);
    expect(livePins(info, pins)).toEqual(new Set([cornerKey(1, 2)]));
  });
});

describe("drawing helpers", () => {
  it("makes one path per island", () => {
    const paths = islandPaths(sample(), view());
    expect([...paths.keys()]).toEqual([0, 1]);
    // island 0 holds two faces: two closed subpaths
    expect(paths.get(0)!.match(/Z/g)).toHaveLength(2);
    expect(paths.get(1)!.match(/M/g)).toHaveLength(1);
  });

  it("measures areas (counter-clockwise is positive) and what the faces cover", () => {
    expect(area([[0, 0], [1, 0], [1, 1], [0, 1]])).toBeCloseTo(1);
    expect(area([[0, 0], [0, 1], [1, 1], [1, 0]])).toBeCloseTo(-1);
    expect(coverage(sample())).toBeCloseTo(0.04 + 0.04 + 0.09);
  });

  it("notices UVs outside the square", () => {
    const info = sample();
    expect(outside(info)).toBe(false);
    info.faces[2].uv[2] = [1.2, 0.9];
    expect(outside(info)).toBe(true);
  });

  it("picks the material most faces wear that has a picture", () => {
    const info = sample();
    info.faces[2].material = "Plain";
    expect(pictureMaterial(info, new Set(["Brick", "Plain"]))).toBe("Brick");
    expect(pictureMaterial(info, new Set(["Plain"]))).toBe("Plain");
    expect(pictureMaterial(info, new Set())).toBeNull();
  });

  it("nudges by a pixel, ten with Shift, whatever the zoom", () => {
    expect(nudge(400, false)).toBeCloseTo(1 / 400);
    expect(nudge(100, true)).toBeCloseTo(0.1);
  });
});
