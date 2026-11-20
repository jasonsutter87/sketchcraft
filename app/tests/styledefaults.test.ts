import { describe, expect, it } from "vitest";
import { chosenStyle, CLASSIC_STYLE, readChosenStyle, startingStyles, STYLE_PRESETS, styleForModel } from "../src/styledefaults";
import { BACKGROUNDS } from "../src/viewport/colors";

describe("the Classic look", () => {
  it("is the first preset, with sky, profiles and no line weights", () => {
    expect(STYLE_PRESETS[0].name).toBe("Classic");
    expect(STYLE_PRESETS[0].styles).toBe(CLASSIC_STYLE);
    expect(CLASSIC_STYLE).toMatchObject({ background: "sky", profiles: true, lineWeights: false, edges: true, sketchy: false });
  });
  it("keeps the other presets", () => {
    expect(STYLE_PRESETS.map((p) => p.name)).toEqual(expect.arrayContaining(["Default", "Architectural", "Sketchy on paper", "Hidden line", "Blueprint", "Clay", "Night"]));
  });
  it("every preset names a backdrop the viewport knows", () => {
    for (const p of STYLE_PRESETS) expect(BACKGROUNDS[p.styles.background!]).toBeTruthy();
  });
});

describe("startingStyles (new model, first launch)", () => {
  it("is Classic when nothing was chosen", () => {
    const s = startingStyles(null, true);
    expect(s).toMatchObject({ background: "sky", profiles: true, lineWeights: false, face: "textured", edges: true, xray: false, help: true });
  });
  it("keeps the look the user chose", () => {
    const s = startingStyles({ face: "hiddenline", background: "paper", profiles: false }, false);
    expect(s).toMatchObject({ face: "hiddenline", background: "paper", profiles: false, help: false });
  });
});

describe("styleForModel (a saved model)", () => {
  const now = startingStyles(null, true);
  it("keeps the style a model saved", () => {
    const s = styleForModel(now, { face: "shaded", background: "night", profiles: false });
    expect(s).toMatchObject({ face: "shaded", background: "night", profiles: false });
    expect(s.help).toBe(true);
  });
  it("a model with no saved style takes the current look", () => {
    expect(styleForModel(now, null)).toBe(now);
    expect(styleForModel(now, undefined)).toBe(now);
  });
});

describe("the remembered choice", () => {
  it("round-trips", () => {
    const s = { ...startingStyles(null, true), background: "night" as const, lineWeights: true };
    expect(readChosenStyle(JSON.stringify(chosenStyle(s)))).toMatchObject({ background: "night", lineWeights: true, profiles: true });
  });
  it("drops anything unknown or malformed", () => {
    expect(readChosenStyle(null)).toBeNull();
    expect(readChosenStyle("{{")).toBeNull();
    expect(readChosenStyle("[1]")).toBeNull();
    expect(readChosenStyle('{"face":"neon","background":3,"profiles":"yes"}')).toBeNull();
    expect(readChosenStyle('{"face":"shaded","x":1}')).toEqual({ face: "shaded" });
  });
});
