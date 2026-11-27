import { describe, expect, it } from "vitest";
import { CLASSIC_ICONS, CLASSIC_ICON_TOOLS, classicIconSvg } from "../src/classicicons";
import { iconOk, readSkin, SKINS } from "../src/skins";
import { TOOL_INFO } from "../src/workspace";

describe("the Classic Color icon set", () => {
  it("draws every tool on the tool bar", () => {
    const missing = Object.keys(TOOL_INFO).filter((t) => !CLASSIC_ICON_TOOLS.includes(t));
    expect(missing).toEqual([]);
  });

  it("draws nothing for a tool that does not exist", () => {
    const extra = CLASSIC_ICON_TOOLS.filter((t) => !Object.hasOwn(TOOL_INFO, t));
    expect(extra).toEqual([]);
  });

  it("makes images a skin may carry (SVG data URLs, small, nothing that can run)", () => {
    for (const [tool, url] of Object.entries(CLASSIC_ICONS)) {
      expect(iconOk(url), tool).toBe(true);
      expect(url.length, tool).toBeLessThan(6000);
    }
    for (const tool of CLASSIC_ICON_TOOLS) {
      const svg = classicIconSvg(tool)!;
      expect(svg).toMatch(/^<svg [^>]*viewBox="0 0 24 24"/);
      expect(svg).not.toMatch(/<script|onload|href=|<image|<foreignObject/i);
    }
  });

  it("has a different picture for each tool", () => {
    expect(new Set(Object.values(CLASSIC_ICONS)).size).toBe(CLASSIC_ICON_TOOLS.length);
  });
});

describe("the Classic Color skin", () => {
  const skin = SKINS.find((s) => s.id === "classic-color")!;
  it("is a built-in with the Classic layout, the tray and the icons", () => {
    expect(skin.layout).toBe("classic");
    expect(skin.tray).toBe(true);
    expect(Object.keys(skin.icons ?? {}).sort()).toEqual([...CLASSIC_ICON_TOOLS].sort());
  });
  it("leaves the default Classic skin alone: its own icons, no tray", () => {
    const classic = SKINS[0];
    expect(classic.id).toBe("classic");
    expect(classic.icons).toBeUndefined();
    expect(classic.tray).toBe(false);
  });
  it("keeps its icons through a save and read of the skin file", () => {
    const back = readSkin(JSON.parse(JSON.stringify(skin)), "x");
    expect(Object.keys(back?.icons ?? {}).length).toBe(CLASSIC_ICON_TOOLS.length);
  });
});
