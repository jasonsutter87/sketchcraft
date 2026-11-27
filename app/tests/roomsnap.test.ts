// Room tool: the pointer snaps to corners already in the model, lines up with them, and falls back to the grid.
import { expect, it } from "vitest";
import { centreCorners, groundCorners, snapRoomPoint } from "../src/viewport/rooms";

// a plain top view: 50 pixels to the metre
const screen = (x: number, y: number) => ({ x: x * 50, y: -y * 50 });
const corners: [number, number][] = [[0, 0], [4.2, 0], [4.2, 4.2], [0, 4.2], [8.2, 0], [12.4, 0]];

it("snaps onto a corner within a few pixels", () => {
  expect(snapRoomPoint({ x: 4.25, y: 0.1 }, corners, screen, 0.1)).toEqual({ x: 4.2, y: 0, kind: "corner" });
});

it("lines a side up with a neighbour's wall away from its corners", () => {
  // near x = 8.2 (a wall line), far from any corner in y
  const p = snapRoomPoint({ x: 8.27, y: 2.03 }, corners, screen, 0.1);
  expect(p.kind).toBe("inline");
  expect(p.x).toBe(8.2);
  expect(p.y).toBeCloseTo(2.0, 9);
});

it("falls back to the grid in open ground", () => {
  expect(snapRoomPoint({ x: 20.04, y: 20.07 }, corners, screen, 0.1)).toEqual({ x: 20, y: 20.1, kind: "grid" });
});

it("reads ground corners from scene edges once each, ignoring the tops of walls", () => {
  const edges = [0, 0, 0, 4, 0, 0, 4, 0, 0, 4, 4, 0, 0, 0, 2.7, 4, 0, 2.7];
  expect(groundCorners(edges)).toEqual([[0, 0], [4, 0], [4, 4]]);
});

it("a room's centre-line corners are its inside moved out by half the wall", () => {
  // the inside of a 4 x 3 room with 0.2 m walls
  const inside: [number, number][] = [[0.1, 0.1], [3.9, 0.1], [3.9, 2.9], [0.1, 2.9]];
  const c = centreCorners(inside, 0.2).map(([x, y]) => [Math.round(x * 1e9) / 1e9, Math.round(y * 1e9) / 1e9]);
  expect(c).toEqual([[0, 0], [4, 0], [4, 3], [0, 3]]);
  // and an L: the walls along the notch sit outside the inside too (x 3.1 + 0.1)
  const l: [number, number][] = [[0.1, 0.1], [5.9, 0.1], [5.9, 2.9], [3.1, 2.9], [3.1, 4.9], [0.1, 4.9]];
  const cl = centreCorners(l, 0.2).map(([x, y]) => [Math.round(x * 1e9) / 1e9, Math.round(y * 1e9) / 1e9]);
  expect(cl).toEqual([[0, 0], [6, 0], [6, 3], [3.2, 3], [3.2, 5], [0, 5]]);
});
