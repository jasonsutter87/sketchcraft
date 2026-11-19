import { expect, it } from "vitest";
import english from "../src/locale-en";
import { FIXTURE_GROUPS, FIXTURES, fixtureOf, sizeText, wallSnap } from "../src/fixtures";
import type { Vec3 } from "../src/engine";

// A 4 x 3 m room, 2.5 m walls, inside faces looking into the room (normals point at the room).
const wall = (id: number, a: Vec3, b: Vec3, normal: Vec3, top = 2.5) => ({
  id,
  normal,
  outer: [[a[0], a[1], 0], [b[0], b[1], 0], [b[0], b[1], top], [a[0], a[1], top]] as Vec3[],
});
const room = [
  wall(1, [0, 0, 0], [4, 0, 0], [0, 1, 0]), // south wall, inside looks +Y
  wall(2, [4, 0, 0], [4, 3, 0], [-1, 0, 0]), // east wall
  wall(3, [4, 3, 0], [0, 3, 0], [0, -1, 0]), // north wall
  wall(4, [0, 3, 0], [0, 0, 0], [1, 0, 0]), // west wall
];

it("lists 26 fixtures in five rooms, each with an English name and a size", () => {
  expect(FIXTURES).toHaveLength(26);
  expect(new Set(FIXTURES.map((f) => f.key)).size).toBe(26);
  for (const g of FIXTURE_GROUPS) {
    expect(FIXTURES.some((f) => f.group === g)).toBe(true);
    expect(english).toHaveProperty(`fixtures.${g}`);
  }
  for (const f of FIXTURES) expect(english).toHaveProperty(`fixture.${f.key}`);
  expect(sizeText(fixtureOf("toilet")!)).toBe("0.38 × 0.70 × 0.78 m (15 × 28 × 31 in)");
  expect(fixtureOf("door")).toBeNull();
});

it("puts the back flush against the nearest wall and turns the front away from it", () => {
  // south wall: the front faces +Y, which is a turn of 180 degrees from the default -Y
  const s = wallSnap([2, 0.3, 0], room, 0.19)!;
  expect(s.face).toBe(1);
  expect(s.at[0]).toBeCloseTo(2, 9);
  expect(s.at[1]).toBeCloseTo(0, 9);
  expect(Math.abs(s.rotate)).toBeCloseTo(180, 6);
  // north wall: the front already faces -Y
  const n = wallSnap([2, 2.8, 0], room, 0.19)!;
  expect(n.face).toBe(3);
  expect(n.at[1]).toBeCloseTo(3, 9);
  expect(n.rotate).toBeCloseTo(0, 9);
  // east wall: the front faces -X, a quarter turn
  const e = wallSnap([3.7, 1.5, 0], room, 0.19)!;
  expect(e.face).toBe(2);
  expect(e.at[0]).toBeCloseTo(4, 9);
  expect(e.rotate).toBeCloseTo(-90, 9);
  // west wall: the front faces +X
  const w = wallSnap([0.2, 1.5, 0], room, 0.19)!;
  expect(w.face).toBe(4);
  expect(w.rotate).toBeCloseTo(90, 9);
});

it("snaps within 0.4 m and not beyond, and ignores the far side of a wall", () => {
  expect(wallSnap([2, 0.39, 0], room)).not.toBeNull();
  expect(wallSnap([2, 0.45, 0], room)).toBeNull();
  expect(wallSnap([2, 1.5, 0], room)).toBeNull(); // middle of the room
  expect(wallSnap([2, -0.3, 0], room)).toBeNull(); // outside the room
});

it("takes the nearer wall in a corner, and keeps the fixture on the wall it snapped to", () => {
  expect(wallSnap([0.3, 0.1, 0], room)!.face).toBe(1);
  expect(wallSnap([0.1, 0.3, 0], room)!.face).toBe(4);
  // a 0.6 m wide fixture near the south wall's end slides in so it doesn't poke past the corner
  const c = wallSnap([3.8, 0.1, 0], room, 0.3)!;
  expect(c.face).toBe(1);
  expect(c.at[0]).toBeCloseTo(3.7, 9);
});

it("ignores small upright faces such as cabinet doors, low walls and hidden faces", () => {
  const door = wall(9, [1, 1, 0], [1.6, 1, 0], [0, 1, 0], 0.8);
  expect(wallSnap([1.3, 1.2, 0], [door])).toBeNull();
  const lowWall = wall(8, [0, 0, 0], [4, 0, 0], [0, 1, 0], 0.9);
  expect(wallSnap([2, 0.2, 0], [lowWall])).toBeNull();
  expect(wallSnap([2, 0.2, 0], [{ ...room[0], hidden: true }])).toBeNull();
  // a wall that stands on another level doesn't catch a fixture on the ground floor
  const up = { ...room[0], outer: room[0].outer.map((q): Vec3 => [q[0], q[1], q[2] + 3]) };
  expect(wallSnap([2, 0.3, 0], [up])).toBeNull();
  expect(wallSnap([2, 0.3, 3], [up])).not.toBeNull();
});
