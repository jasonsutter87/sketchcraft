import { expect, it } from "vitest";
import { guide } from "../src/guide";
import { translate } from "../src/locale";
import { TOOL_INFO } from "../src/workspace";
import { areaText, dragLabel, fillRects, gridStep, inPoly, insideOf, labelPoint, modeLabel, rectBetween, roomLabel, roomMode, roomModeKey, snapGrid, type Pt } from "../src/viewport/rooms";

it("snaps to 0.1 m in metric and 6 in in feet and inches", () => {
  expect(gridStep("m")).toBe(0.1);
  expect(gridStep("cm")).toBe(0.1);
  expect(gridStep("ft")).toBeCloseTo(0.1524, 6);
  expect(gridStep("in")).toBeCloseTo(0.1524, 6);
  expect(snapGrid(2.34, 0.1)).toBe(2.3);
  expect(snapGrid(2.36, 0.1)).toBe(2.4);
  expect(snapGrid(-0.02, 0.1)).toBe(0);
  expect(snapGrid(1.0, gridStep("ft"))).toBeCloseTo(7 * 0.1524, 6); // 1 m is 39.4 in: nearest 6 in step is 42 in
});

it("reports areas in the document's units", () => {
  expect(areaText(10.64, "m")).toBe("10.6 m²");
  expect(areaText(120, "m")).toBe("120 m²");
  expect(areaText(10.0, "ft")).toBe("108 ft²");
  expect(areaText(30, "ft")).toBe("323 ft²");
});

it("works out the inside of a room from its wall centre lines", () => {
  const r = rectBetween([4, 3], [0, 0]);
  expect(r).toEqual([0, 0, 4, 3]);
  const i = insideOf(r, 0.2);
  expect(i.width).toBeCloseTo(3.8, 9);
  expect(i.depth).toBeCloseTo(2.8, 9);
  expect(i.area).toBeCloseTo(10.64, 9);
  expect(insideOf([0, 0, 0.1, 5], 0.2).area).toBe(0);
});

it("labels a room by name and area, and a drag by size and area", () => {
  expect(roomLabel("Kitchen", 10.64, "m")).toBe("Kitchen · 10.6 m²");
  expect(roomLabel("Room 2", 10.0, "ft")).toBe("Room 2 · 108 ft²");
  expect(dragLabel(3.8, 2.8, "m")).toBe("3.80 m × 2.80 m · 10.6 m²");
});

it("explains Room mode at every step, in both languages", () => {
  const idle = guide("room", { selected: 0, op: null });
  expect(idle.title).toBe("Room");
  expect(idle.step).toMatch(/drag/i);
  const busy = guide("room", { selected: 0, op: "room" });
  expect(busy.step).toMatch(/release/i);
  expect(busy.keys.map((k) => k[0])).toEqual(["Alt", "Esc"]);
  expect(busy.step).toMatch(/Alt/);
  const italian = guide("room", { selected: 0, op: null }, "it");
  expect(italian.step).toMatch(/stanza/i);
  expect(italian.step).not.toBe(idle.step);
});

it("has a toolbar entry with a tooltip in both languages", () => {
  expect(TOOL_INFO.room.name).toMatch(/Room/);
  expect(translate("en", "tool.room")).toBe(TOOL_INFO.room.name);
  expect(translate("it", "tool.room")).toMatch(/stanza/i);
  for (const k of ["build.room", "build.room.how", "room.opening", "room.walls_down"]) expect(translate("it", k)).not.toBe(translate("en", k));
});

// an L: a 4 x 4 room with a 2 x 2 corner taken out of its north-east
const L: Pt[] = [[0, 0], [4, 0], [4, 2], [2, 2], [2, 4], [0, 4]];
// a U: 9 x 6 with a 3-wide slot open to the north
const U: Pt[] = [[0, 0], [9, 0], [9, 6], [6, 6], [6, 2], [3, 2], [3, 6], [0, 6]];

it("puts a label inside an L or a U, not at the bounding box's middle", () => {
  expect(labelPoint([[0, 0], [4, 0], [4, 2], [0, 2]])).toEqual([2, 1]); // a rectangle: its centre
  const l = labelPoint(L);
  expect(inPoly(L, l)).toBe(true);
  expect(inPoly(L, [2.5, 2.5])).toBe(false); // the bounding box's middle (2, 2) sits on the notch's corner: not a safe spot
  const u = labelPoint(U); // the centroid of a U is in its slot
  expect(inPoly(U, u)).toBe(true);
  expect(inPoly(U, [4.5, 4])).toBe(false); // the centroid, (4.5, 2.7), is in the slot: the label moved into the largest part
});

it("fills an outline with grid cells that add up to its area", () => {
  const area = (cs: number[][]) => cs.reduce((a, c) => a + (c[2] - c[0]) * (c[3] - c[1]), 0);
  expect(area(fillRects(L))).toBeCloseTo(12, 9);
  expect(area(fillRects(U))).toBeCloseTo(54 - 3 * 4, 9);
  expect(fillRects([[0, 0], [2, 0], [2, 1], [0, 1]])).toEqual([[0, 0, 2, 1]]);
});

it("combines by default and cuts with Alt held, and names the room in the label", () => {
  expect(roomMode(false)).toBe("combine");
  expect(roomMode(true)).toBe("subtract");
  expect(translate("en", roomModeKey("combine"))).toBe("Combine with {name}");
  expect(modeLabel("combine", "Kitchen", translate("en", roomModeKey("combine")))).toBe("Combine with Kitchen");
  expect(modeLabel("subtract", "Kitchen", translate("en", roomModeKey("subtract")))).toBe("Subtract from Kitchen");
  expect(modeLabel("subtract", "Cucina", translate("it", roomModeKey("subtract")))).toBe("Sottrai da Cucina");
  for (const k of ["room.combine_with", "room.subtract_from", "room.hint.combine", "room.hint.subtract", "room.hint.over", "room.openings_removed", "guide.room.alt"]) expect(translate("it", k)).not.toBe(translate("en", k));
});
