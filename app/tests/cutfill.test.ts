import { describe, expect, it } from "vitest";
import { CUT_FILLS, CUT_OUTLINE_MM, CUT_SOLID, cutColorOf, cutFillCommand, cutFillOf, cutLabel, cutOutlineOf } from "../src/cutfill";

describe("cutFillOf", () => {
  it("is the default with no style, no cut or an unknown fill", () => {
    expect(cutFillOf(undefined)).toBe("auto");
    expect(cutFillOf({})).toBe("auto");
    expect(cutFillOf({ cut: {} })).toBe("auto");
    expect(cutFillOf({ cut: { fill: "gradient" as never } })).toBe("auto");
  });

  it("reads solid, none and hatch, and a hatch fill with no hatch draws as the default (as the engine does)", () => {
    expect(cutFillOf({ cut: { fill: "solid" } })).toBe("solid");
    expect(cutFillOf({ cut: { fill: "none" } })).toBe("none");
    expect(cutFillOf({ cut: { fill: "hatch" } })).toBe("auto");
    expect(cutFillOf({ cut: { fill: "hatch", hatch: { pattern: "brick", color: "#000000", weight_mm: 0.13, scale: 1, angle: 0, scale_mode: "paper" } } })).toBe("hatch");
  });
});

describe("colour and outline", () => {
  it("falls back to the poché colour and the heavy pen", () => {
    expect(cutColorOf({ cut: { fill: "solid" } })).toBe(CUT_SOLID);
    expect(cutColorOf({ cut: { fill: "solid", color: "nope" } })).toBe(CUT_SOLID);
    expect(cutColorOf({ cut: { fill: "solid", color: "#ff0000" } })).toBe("#ff0000");
    expect(cutOutlineOf({})).toBe(CUT_OUTLINE_MM);
    expect(cutOutlineOf({ cut: { outline_mm: 0 } })).toBe(CUT_OUTLINE_MM);
    expect(cutOutlineOf({ cut: { outline_mm: 0.7 } })).toBe(0.7);
  });
});

describe("cutFillCommand", () => {
  it("sends only the fill, and starts a hatch fill with a pattern when the tag has none", () => {
    expect(cutFillCommand("Slab", "solid", {})).toEqual({ cmd: "tag_style", tag: "Slab", cut: { fill: "solid" } });
    expect(cutFillCommand("Slab", "none", undefined)).toEqual({ cmd: "tag_style", tag: "Slab", cut: { fill: "none" } });
    expect(cutFillCommand("Walls", "hatch", {})).toEqual({ cmd: "tag_style", tag: "Walls", cut: { fill: "hatch" }, hatch: { pattern: "diagonal" } });
    const kept = { cut: { fill: "solid" as const, hatch: { pattern: "brick", color: "#000000", weight_mm: 0.13, scale: 1, angle: 0, scale_mode: "paper" as const } } };
    expect(cutFillCommand("Walls", "hatch", kept)).toEqual({ cmd: "tag_style", tag: "Walls", cut: { fill: "hatch" } });
  });
});

describe("labels", () => {
  it("says what the cut looks like", () => {
    expect(cutLabel(undefined)).toBe("default poché, outline default");
    expect(cutLabel({ cut: { fill: "solid", color: "#ff0000", outline_mm: 0.7 } })).toBe("solid #ff0000, outline 0.7 mm");
    expect(cutLabel({ cut: { fill: "none" } })).toBe("none (outline only), outline default");
    expect(CUT_FILLS.map((f) => f.kind)).toEqual(["auto", "solid", "hatch", "none"]);
    expect(CUT_FILLS.every((f) => f.hint.length > 0)).toBe(true);
  });
});
