import { describe, expect, it } from "vitest";
import { DEFAULT_SPLINE_SEGMENTS, MAX_SPAN_SEGMENTS, segmentCount, splinePoints, type P3 } from "../src/curvemath";

const close = (a: P3, b: P3, tol = 1e-9) => a.every((v, i) => Math.abs(v - b[i]) < tol);

describe("the Curve tool's preview", () => {
  const pts: P3[] = [
    [0, 0, 0],
    [4, 3, 0],
    [8, 0, 1],
  ];

  it("is the curve the engine draws (the same numbers core/tests/small_tools.rs checks)", () => {
    const open = splinePoints(pts, false, 2);
    const want: P3[] = [
      [0, 0, 0],
      [2.0024273571957143, 1.871341025903462, -0.06158675133444605],
      [4, 3, 0],
      [6.002475428341481, 1.8786950274558634, 0.4371935906333746],
      [8, 0, 1],
    ];
    expect(open).toHaveLength(want.length);
    want.forEach((w, i) => expect(close(open[i], w), `point ${i}: ${open[i]}`).toBe(true));
    const closed = splinePoints(pts, true, 2);
    expect(closed).toHaveLength(7);
    expect(close(closed[1], [1.4351977464294634, 1.7061299373954504, -0.10495527126222551])).toBe(true);
    expect(close(closed[5], [3.9925959761097607, -0.5290502671959056, 0.5872495415463711])).toBe(true);
    expect(close(closed[6], closed[0])).toBe(true);
  });

  it("goes through every clicked point and has the asked number of pieces", () => {
    const many: P3[] = [[0, 0, 0], [2, 3, 0], [5, 1, 1], [8, 4, 0.5], [10, 0, 0]];
    const out = splinePoints(many, false, 6);
    expect(out).toHaveLength(4 * 6 + 1);
    many.forEach((p, i) => expect(close(out[i * 6], p)).toBe(true));
  });

  it("ignores repeated clicks on one spot and gives back what it cannot curve", () => {
    expect(splinePoints([[1, 1, 0], [1, 1, 0]], false, 12)).toHaveLength(1);
    expect(splinePoints([[1, 1, 0]], false, 12)).toHaveLength(1);
    expect(splinePoints([[0, 0, 0], [0, 0, 0], [5, 0, 0]], false, 4)).toHaveLength(5);
    // two points closed are not a loop yet
    expect(splinePoints([[0, 0, 0], [4, 0, 0]], true, 4)).toHaveLength(2);
    // a first point clicked again at the end is the same loop
    const loop = splinePoints([[0, 0, 0], [4, 0, 0], [4, 4, 0], [0, 0, 0]], true, 3);
    expect(loop).toHaveLength(3 * 3 + 1);
  });

  it("takes a typed count as a whole number between 1 and 256", () => {
    expect(DEFAULT_SPLINE_SEGMENTS).toBe(12);
    expect(segmentCount(24)).toBe(24);
    expect(segmentCount(0)).toBe(1);
    expect(segmentCount(2.6)).toBe(3);
    expect(segmentCount(100000)).toBe(MAX_SPAN_SEGMENTS);
    expect(segmentCount(NaN)).toBeNull();
    expect(splinePoints(pts, false, 100000)).toHaveLength(2 * MAX_SPAN_SEGMENTS + 1);
  });
});
