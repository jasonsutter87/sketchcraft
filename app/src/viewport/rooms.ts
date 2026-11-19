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
