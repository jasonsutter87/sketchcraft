import { expect, it } from "vitest";
import { guide } from "../src/guide";
import { translate } from "../src/locale";
import { TOOL_INFO } from "../src/workspace";
import { areaText, dragLabel, gridStep, insideOf, rectBetween, roomLabel, snapGrid } from "../src/viewport/rooms";

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
  expect(busy.keys.map((k) => k[0])).toEqual(["Esc"]);
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
