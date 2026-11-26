import { describe, expect, it } from "vitest";
import { openingSpot } from "../src/viewport/openingspot";

// the front of a 6 m wall along x, 2.7 m tall, facing -y
const face: [number, number, number][] = [[0, -0.1, 0], [6, -0.1, 0], [6, -0.1, 2.7], [0, -0.1, 2.7]];
const n: [number, number, number] = [0, -1, 0];

describe("Door and Window tool preview (#264)", () => {
  it("centres a window on the cursor at its sill", () => {
    const s = openingSpot("window", face, n, [2, -0.1, 1.7], 1, 1.2, 0.9);
    expect(s.ok).toBe(true);
    if (s.ok) {
      expect(s.corners[0][0]).toBeCloseTo(1.5);
      expect(s.corners[0][2]).toBeCloseTo(0.9);
      expect(s.corners[2][2]).toBeCloseTo(2.1);
      expect(s.corners.every((c) => Math.abs(c[1] + 0.1) < 1e-9)).toBe(true);
    }
  });
  it("stands a door on the floor and keeps it inside the wall", () => {
    const s = openingSpot("door", face, n, [5.9, -0.1, 1], 0.9, 2.1, 1.0);
    expect(s.ok && s.corners[0][2]).toBe(0);
    expect(s.ok && s.corners[1][0]).toBeLessThanOrEqual(6);
  });
  it("says why it won't fit", () => {
    expect(openingSpot("window", face, n, [3, -0.1, 1], 1, 2.5, 0.9).ok).toBe(false);
    expect(openingSpot("door", face, n, [3, -0.1, 1], 7, 2.1, 0).ok).toBe(false);
    expect(openingSpot("door", [[0, 0, 0], [1, 0, 0], [1, 1, 0]], [0, 0, 1], [0.5, 0.5, 0], 0.9, 2.1, 0).ok).toBe(false);
  });
});
