import { expect, it } from "vitest";
import { guide } from "../src/guide";
import { translate } from "../src/locale";
import { TOOL_INFO } from "../src/workspace";

it("explains Wall pull at every step, in both languages", () => {
  const idle = guide("wallpull", { selected: 0, op: null });
  expect(idle.title).toBe("Pull wall");
  expect(idle.step).toMatch(/Hover a wall/);
  const busy = guide("wallpull", { selected: 0, op: "wallpull" });
  expect(busy.step).toMatch(/slide the wall/);
  expect(busy.keys.map((k) => k[0])).toEqual(["Type", "Esc"]);
  const italian = guide("wallpull", { selected: 0, op: null }, "it");
  expect(italian.step).toMatch(/parete/);
  expect(italian.step).not.toBe(idle.step);
  expect(guide("wallpull", { selected: 0, op: "wallpull" }, "it").keys[0][1]).toMatch(/Invio/);
});

it("has a toolbar entry with a tooltip in both languages", () => {
  expect(TOOL_INFO.wallpull.name).toMatch(/Pull wall/);
  expect(translate("en", "tool.wallpull")).toBe(TOOL_INFO.wallpull.name);
  expect(translate("it", "tool.wallpull")).toMatch(/parete/);
  expect(translate("it", "build.wallpull")).not.toBe(translate("en", "build.wallpull"));
});
