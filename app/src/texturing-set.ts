// The texturing test set: six built-in models (the beaver plus dice, can, globe, crate and soccer ball) that every texturing
// feature is built and tested on. Geometry: core/assets/<key>.jsonl, made by tools/texturing_set/texturing_set.py.
// See docs/dev/texturing-test-set.md for what each one is for.
import type { Fixture } from "./fixtures";

export type TexturingModel = Pick<Fixture, "key" | "w" | "d" | "h">;

/** metres: width (x), depth (y), height of each model's bounding box */
export const TEXTURING_SET: readonly TexturingModel[] = [
  { key: "beaver", w: 0.66, d: 0.01, h: 1.5 },
  { key: "dice", w: 0.05, d: 0.05, h: 0.05 },
  { key: "can", w: 0.066, d: 0.066, h: 0.122 },
  { key: "globe", w: 0.3, d: 0.3, h: 0.35 },
  { key: "crate", w: 0.6, d: 0.6, h: 0.6 },
  { key: "soccer_ball", w: 0.22, d: 0.22, h: 0.22 },
];

/** "0.05 × 0.05 × 0.05 m": small things in centimetres or millimetres read better */
export function setSizeText(m: TexturingModel): string {
  const dims = [m.w, m.d, m.h];
  return `${dims.map((x) => (x < 0.1 ? x.toFixed(3) : x.toFixed(2))).join(" × ")} m`;
}
