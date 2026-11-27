import { describe, expect, it } from "vitest";
import { cleanLabel, cleanMenu, MAX_MENU_ITEMS } from "../src/pluginclean";

describe("names and menus", () => {
  it("removes bidi and control characters from a name", () => {
    expect(cleanLabel("m‮RB\u0001x‏y")).toBe("mRBxy");
    expect(cleanLabel("a\nb\tc")).toBe("a b c");
    expect(cleanLabel("x".repeat(500), 60)).toHaveLength(60);
    expect(cleanLabel(null)).toBe("");
  });
  it("caps a menu's items and labels", () => {
    const m = cleanMenu(Array.from({ length: 60000 }, (_, i) => "Flood" + "x".repeat(2000) + i));
    expect(m.items).toHaveLength(MAX_MENU_ITEMS);
    expect(m.items[0]).toHaveLength(80);
    expect(m.dropped).toBe(60000 - MAX_MENU_ITEMS);
    expect(cleanMenu("nope").items).toEqual([]);
  });
});
