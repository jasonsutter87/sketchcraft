// Fixtures & furniture: the fixed Library assets for furnishing rooms, and the wall snap used while placing them.
// Every one is built with its origin on the floor at the middle of its BACK edge and its front facing -Y
// (core/assets/*.jsonl, docs/dev/fixtures.md), so "back against a wall" is just "origin on the wall face".
import type { FaceInfo, Vec3 } from "./engine";

export type FixtureGroup = "bath" | "kitchen" | "bedroom" | "living" | "laundry";

export interface Fixture {
  key: string;
  name: string;
  group: FixtureGroup;
  /** metres: width (x), depth (y), height */
  w: number;
  d: number;
  h: number;
}

export const FIXTURE_GROUPS: readonly FixtureGroup[] = ["bath", "kitchen", "bedroom", "living", "laundry"];

export const FIXTURES: readonly Fixture[] = [
  { key: "toilet", name: "Toilet", group: "bath", w: 0.38, d: 0.7, h: 0.78 },
  { key: "vanity_60", name: "Vanity 0.6 m", group: "bath", w: 0.6, d: 0.5, h: 0.95 },
  { key: "vanity_120", name: "Vanity 1.2 m", group: "bath", w: 1.2, d: 0.5, h: 0.95 },
  { key: "bathtub", name: "Bathtub", group: "bath", w: 1.5, d: 0.7, h: 0.6 },
  { key: "shower", name: "Shower stall", group: "bath", w: 0.9, d: 0.9, h: 2.0 },
  { key: "towel_bar", name: "Towel bar", group: "bath", w: 0.6, d: 0.08, h: 0.06 },
  { key: "base_cabinet", name: "Base cabinet", group: "kitchen", w: 0.6, d: 0.6, h: 0.9 },
  { key: "sink_base", name: "Sink base", group: "kitchen", w: 0.8, d: 0.6, h: 1.1 },
  { key: "wall_cabinet", name: "Wall cabinet", group: "kitchen", w: 0.6, d: 0.32, h: 0.72 },
  { key: "pantry", name: "Pantry cabinet", group: "kitchen", w: 0.6, d: 0.6, h: 2.1 },
  { key: "fridge", name: "Fridge", group: "kitchen", w: 0.76, d: 0.7, h: 1.78 },
  { key: "range", name: "Range", group: "kitchen", w: 0.76, d: 0.65, h: 0.92 },
  { key: "dishwasher", name: "Dishwasher", group: "kitchen", w: 0.6, d: 0.6, h: 0.84 },
  { key: "island", name: "Kitchen island", group: "kitchen", w: 1.8, d: 0.9, h: 0.92 },
  { key: "queen_bed", name: "Queen bed", group: "bedroom", w: 1.62, d: 2.13, h: 1.1 },
  { key: "king_bed", name: "King bed", group: "bedroom", w: 2.03, d: 2.13, h: 1.1 },
  { key: "twin_bed", name: "Twin bed", group: "bedroom", w: 1.09, d: 2.0, h: 0.95 },
  { key: "nightstand", name: "Nightstand", group: "bedroom", w: 0.5, d: 0.4, h: 0.55 },
  { key: "dresser", name: "Dresser", group: "bedroom", w: 1.2, d: 0.5, h: 0.85 },
  { key: "sofa", name: "Sofa, 3-seat", group: "living", w: 2.1, d: 0.92, h: 0.85 },
  { key: "armchair", name: "Armchair", group: "living", w: 0.9, d: 0.88, h: 0.85 },
  { key: "coffee_table", name: "Coffee table", group: "living", w: 1.1, d: 0.6, h: 0.45 },
  { key: "tv_stand", name: "TV stand", group: "living", w: 1.5, d: 0.4, h: 0.5 },
  { key: "dining_set", name: "Dining table with 4 chairs", group: "living", w: 1.5, d: 1.62, h: 0.9 },
  { key: "washer", name: "Washer", group: "laundry", w: 0.69, d: 0.74, h: 0.98 },
  { key: "dryer", name: "Dryer", group: "laundry", w: 0.69, d: 0.74, h: 0.98 },
];

const BY_KEY = new Map(FIXTURES.map((f) => [f.key, f]));
export const fixtureOf = (key: string | null | undefined): Fixture | null => (key ? BY_KEY.get(key) ?? null : null);

/** "0.38 × 0.70 × 0.78 m (15 × 28 × 31 in)": the tooltip size, in both systems. */
export function sizeText(f: Fixture): string {
  const dims = [f.w, f.d, f.h];
  return `${dims.map((x) => x.toFixed(2)).join(" × ")} m (${dims.map((x) => Math.round(x / 0.0254)).join(" × ")} in)`;
}

/** how far from a wall the cursor can be and still snap to it (metres) */
export const WALL_REACH = 0.4;

export interface WallSnap {
  /** where the origin goes: on the wall face, at the cursor's floor height */
  at: Vec3;
  /** degrees about Z, so the front faces away from the wall */
  rotate: number;
  /** the wall face it snapped to */
  face: number;
}

type WallFace = Pick<FaceInfo, "id" | "normal" | "outer"> & { hidden?: boolean; area?: number };

/**
 * If `p` is within `reach` of the room side of an upright wall face, the spot and turn that put a fixture's
 * back flush against it. Only big faces count (a wall, not a cabinet door): at least 1.2 m tall and 2 m².
 * `half` is the fixture's half width: it slides along the wall so it stays on that face instead of
 * poking into the next wall at a corner.
 */
export function wallSnap(p: Vec3, faces: readonly WallFace[], half = 0, reach = WALL_REACH): WallSnap | null {
  let best: { dist: number; snap: WallSnap } | null = null;
  for (const f of faces) {
    if (f.hidden || Math.abs(f.normal[2]) > 0.1 || f.outer.length < 3) continue;
    const len = Math.hypot(f.normal[0], f.normal[1]);
    if (len < 1e-9) continue;
    const nx = f.normal[0] / len;
    const ny = f.normal[1] / len;
    // distance in front of the face (negative: behind it, on the far side of the wall)
    const dist = nx * (p[0] - f.outer[0][0]) + ny * (p[1] - f.outer[0][1]);
    if (dist < -0.02 || dist > reach) continue;
    let zlo = Infinity;
    let zhi = -Infinity;
    let slo = Infinity;
    let shi = -Infinity;
    for (const q of f.outer) {
      zlo = Math.min(zlo, q[2]);
      zhi = Math.max(zhi, q[2]);
      const s = -ny * q[0] + nx * q[1];
      slo = Math.min(slo, s);
      shi = Math.max(shi, s);
    }
    if (zhi - zlo < 1.2 || (f.area ?? (zhi - zlo) * (shi - slo)) < 2) continue;
    if (p[2] < zlo - 0.05 || p[2] > zhi - 0.3) continue; // the wall has to stand at this floor
    const s = -ny * p[0] + nx * p[1];
    if (s < slo - 0.05 || s > shi + 0.05) continue;
    const room = shi - slo - 2 * half;
    const sc = room > 0 ? Math.min(Math.max(s, slo + half), shi - half) : (slo + shi) / 2;
    const d = Math.max(dist, 0);
    if (best && d >= best.dist) continue;
    // the cursor, moved onto the wall plane and along the wall to sc
    let rotate = (Math.atan2(ny, nx) * 180) / Math.PI + 90;
    if (rotate > 180) rotate -= 360;
    const at: Vec3 = [p[0] - nx * dist - ny * (sc - s), p[1] - ny * dist + nx * (sc - s), p[2]];
    best = { dist: d, snap: { at, rotate: Math.round(rotate * 1e4) / 1e4, face: f.id } };
  }
  return best ? best.snap : null;
}
