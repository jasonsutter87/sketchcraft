import { expect, it } from "vitest";
import english from "../src/locale-en";
import italian from "../src/locale-it";
import { TEXTURING_SET, setSizeText } from "../src/texturing-set";

const KEYS = ["beaver", "dice", "can", "globe", "crate", "soccer_ball"];

it("lists the six test models, each with a unique key and a real size", () => {
  expect(TEXTURING_SET.map((m) => m.key)).toEqual(KEYS);
  for (const m of TEXTURING_SET) {
    expect(m.w).toBeGreaterThan(0);
    expect(m.d).toBeGreaterThan(0);
    expect(m.h).toBeGreaterThan(0);
  }
  expect(setSizeText(TEXTURING_SET[1])).toBe("0.050 × 0.050 × 0.050 m");
  expect(setSizeText(TEXTURING_SET[4])).toBe("0.60 × 0.60 × 0.60 m");
});

it("has English and Italian strings for the section and every model", () => {
  for (const loc of [english, italian] as Record<string, string>[]) {
    for (const k of ["texset.title", "texset.tip", "texset.place"]) expect(loc[k], k).toBeTruthy();
    for (const key of KEYS) {
      expect(loc[`texset.${key}`], key).toBeTruthy();
      expect(loc[`texset.use.${key}`], key).toBeTruthy();
    }
  }
  expect(italian["texset.title"]).not.toBe(english["texset.title"]);
});
