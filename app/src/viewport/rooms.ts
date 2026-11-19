// Room mode's small pure helpers: the grid a room snaps to, its inside size and area in the document's units, and its labels.
// The engine decides what a room really is (snapping to walls, sharing, refusing overlaps); this is only what the tool shows.
import { formatLength } from "../units";

const INCH = 0.0254;

/** Imperial documents count in feet and inches, everything else in metres. */
export const isImperial = (units: string) => units === "ft" || units === "in";

/** The grid a dragged room corner snaps to: 0.1 m, or 6 in in feet and inches (metres). */
export function gridStep(units: string): number {
  return isImperial(units) ? 6 * INCH : 0.1;
}

/** A coordinate snapped to the grid (never -0). */
export function snapGrid(v: number, step: number): number {
  const s = Math.round(v / step) * step;
  return Math.abs(s) < 1e-12 ? 0 : Math.round(s * 1e9) / 1e9;
}

/** An area in square metres, as the document counts it: m² for metric, ft² for feet and inches. */
export function areaText(areaM2: number, units: string): string {
  if (isImperial(units)) {
    const ft2 = areaM2 / (12 * INCH) ** 2;
    return `${ft2 >= 100 ? Math.round(ft2) : ft2.toFixed(1)} ft²`;
  }
  return `${areaM2 >= 100 ? Math.round(areaM2) : areaM2.toFixed(1)} m²`;
}

export type Rect = [number, number, number, number];

/** The rectangle between two corners, as [x0, y0, x1, y1]. */
export function rectBetween(a: [number, number], b: [number, number]): Rect {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])];
}

/** The inside of a room whose wall centre lines are `r`: half a wall thickness in on every side. */
export function insideOf(r: Rect, thickness: number): { width: number; depth: number; area: number } {
  const width = Math.max(0, r[2] - r[0] - thickness);
  const depth = Math.max(0, r[3] - r[1] - thickness);
  return { width, depth, area: width * depth };
}

/** The label of a finished room: "Kitchen · 10.6 m²". */
export function roomLabel(name: string, areaM2: number, units: string): string {
  return `${name} · ${areaText(areaM2, units)}`;
}

/** The label while dragging: "3.80 m × 2.80 m · 10.6 m²". */
export function dragLabel(width: number, depth: number, units: string): string {
  return `${formatLength(width, units)} × ${formatLength(depth, units)} · ${areaText(width * depth, units)}`;
}

/** What a room drag does where it overlaps a room: join it (default), or with Alt / Option held, cut from it. */
export type RoomMode = "combine" | "subtract";

export const roomMode = (alt: boolean): RoomMode => (alt ? "subtract" : "combine");

/** The locale key of the mode's label template ("Combine with {name}" / "Subtract from {name}"). */
export const roomModeKey = (mode: RoomMode): string => (mode === "subtract" ? "room.subtract_from" : "room.combine_with");

/** The floating label's mode text: the template with the room's name filled in. */
export const modeLabel = (_mode: RoomMode, name: string, template: string): string => template.replace("{name}", name);

export type Pt = [number, number];

const distinct = (v: number[]): number[] => [...new Set(v.map((x) => Math.round(x * 1e9) / 1e9))].sort((a, b) => a - b);

/** Is the point inside the polygon (ray casting)? */
export function inPoly(p: Pt[], q: Pt): boolean {
  let inside = false;
  for (let i = 0, j = p.length - 1; i < p.length; j = i++) {
    const [a, b] = [p[j], p[i]];
    if (a[1] > q[1] !== b[1] > q[1] && q[0] < ((b[0] - a[0]) * (q[1] - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}

/** An axis-aligned polygon as the grid cells (rectangles) it covers: a fill that needs no triangulation. */
export function fillRects(p: Pt[]): Rect[] {
  const xs = distinct(p.map((q) => q[0]));
  const ys = distinct(p.map((q) => q[1]));
  const out: Rect[] = [];
  for (let i = 0; i + 1 < xs.length; i++)
    for (let j = 0; j + 1 < ys.length; j++) if (inPoly(p, [(xs[i] + xs[i + 1]) / 2, (ys[j] + ys[j + 1]) / 2])) out.push([xs[i], ys[j], xs[i + 1], ys[j + 1]]);
  return out;
}

/** Where to hang a room's label: its centroid when that is inside the outline (a rectangle's is its centre), else the
 *  middle of its largest grid cell, so an L or a U is never labelled in the gap. Same rule as the engine's `rooms`. */
export function labelPoint(p: Pt[]): Pt {
  let a = 0;
  let cx = 0;
  let cy = 0;
  for (let i = 0; i < p.length; i++) {
    const [u, v] = [p[i], p[(i + 1) % p.length]];
    const w = u[0] * v[1] - v[0] * u[1];
    a += w / 2;
    cx += (u[0] + v[0]) * w;
    cy += (u[1] + v[1]) * w;
  }
  if (Math.abs(a) > 1e-12) {
    const c: Pt = [cx / (6 * a), cy / (6 * a)];
    if (inPoly(p, c)) return c;
  }
  let best: Rect | null = null;
  for (const c of fillRects(p)) if (!best || (c[2] - c[0]) * (c[3] - c[1]) > (best[2] - best[0]) * (best[3] - best[1])) best = c;
  return best ? [(best[0] + best[2]) / 2, (best[1] + best[3]) / 2] : (p[0] ?? [0, 0]);
}
