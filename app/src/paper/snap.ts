// Snapping on paper (design §5): where a dragged frame, a handle or a clicked point lands. Pure, so it is unit tested
// (app/tests/sheeteditor.test.ts). Everything is in paper millimetres, y down from the sheet's top left.
//
// What snaps, nearest first within the tolerance: points (the model's corners and middles from `viewport_points`, the
// ends and middles of paper line work), then lines (the page's edges, its margin and middle, other items' edges and
// centres), then the paper grid. A frame being moved snaps its left edge, centre or right edge (and top, middle or
// bottom), whichever is nearest a line; the same on each axis on its own.

export type P = [number, number];
/** x, y, width, height */
export type Rect = [number, number, number, number];

/** A line something may snap to: x = v (a vertical line) or y = v, and what it is, for the marker. */
export interface Guide {
  v: number;
  why: string;
}
export interface SnapPoint {
  p: P;
  why: string;
  /** what the point stands for (a `viewport_points` entry's anchor, for a dimension) */
  ref?: unknown;
  /** the viewport it came from */
  viewport?: number;
}
export interface Guides {
  xs: Guide[];
  ys: Guide[];
  points: SnapPoint[];
}
export interface SnapOpts {
  /** the grid's spacing, mm */
  grid: number;
  gridOn: boolean;
  /** how near counts, mm (a few screen pixels at the zoom) */
  tol: number;
}
/** What the overlay shows for a snap: a guide line across the page, a point marker, or a grid cross. */
export type Marker = { kind: "x"; v: number; why: string } | { kind: "y"; v: number; why: string } | { kind: "point"; at: P; why: string } | { kind: "grid"; at: P; why: string };

/** The sheet border sits this far in from the paper's edge (draft/sheet.rs), mm. */
export const MARGIN = 10;
export const GRID_DEFAULT = 5;
/** Snaps reach this many screen pixels. */
export const SNAP_PX = 8;
/** The smallest frame a drag makes, mm. */
export const MIN_SIZE = 5;

export const emptyGuides = (): Guides => ({ xs: [], ys: [], points: [] });

/** The page's own lines: its edges, the border margin, and the middle. */
export function pageGuides(w: number, h: number, margin = MARGIN): Guides {
  return {
    xs: [
      { v: 0, why: "page edge" },
      { v: w, why: "page edge" },
      { v: margin, why: "margin" },
      { v: w - margin, why: "margin" },
      { v: w / 2, why: "page middle" },
    ],
    ys: [
      { v: 0, why: "page edge" },
      { v: h, why: "page edge" },
      { v: margin, why: "margin" },
      { v: h - margin, why: "margin" },
      { v: h / 2, why: "page middle" },
    ],
    points: [],
  };
}

/** Other items' frames: their edges and centres. */
export function rectGuides(rects: Rect[]): Guides {
  const g = emptyGuides();
  for (const [x, y, w, h] of rects) {
    g.xs.push({ v: x, why: "edge" }, { v: x + w / 2, why: "centre" }, { v: x + w, why: "edge" });
    g.ys.push({ v: y, why: "edge" }, { v: y + h / 2, why: "centre" }, { v: y + h, why: "edge" });
    g.points.push({ p: [x + w / 2, y + h / 2], why: "centre" });
  }
  return g;
}

export function mergeGuides(...all: Guides[]): Guides {
  return { xs: all.flatMap((g) => g.xs), ys: all.flatMap((g) => g.ys), points: all.flatMap((g) => g.points) };
}

export const snapGrid = (v: number, grid: number) => (grid > 0 ? Math.round(v / grid) * grid : v);

/** The guide nearest any of `vs` (within tol): which of them, and how far to move. */
function nearest(vs: number[], guides: Guide[], tol: number): { i: number; d: number; g: Guide } | null {
  let best: { i: number; d: number; g: Guide } | null = null;
  for (let i = 0; i < vs.length; i++)
    for (const g of guides) {
      const d = g.v - vs[i];
      if (Math.abs(d) <= tol && (!best || Math.abs(d) < Math.abs(best.d) - 1e-9)) best = { i, d, g };
    }
  return best;
}

/** A frame moved by (dx, dy), snapped: each axis on its own, to a guide, else (grid on) its corner to the grid. */
export function snapMove(r: Rect, dx: number, dy: number, guides: Guides, o: SnapOpts): { rect: Rect; markers: Marker[] } {
  let [x, y] = [r[0] + dx, r[1] + dy];
  const [w, h] = [r[2], r[3]];
  const markers: Marker[] = [];
  const nx = nearest([x, x + w / 2, x + w], guides.xs, o.tol);
  if (nx) {
    x += nx.d;
    markers.push({ kind: "x", v: nx.g.v, why: nx.g.why });
  } else if (o.gridOn) x = snapGrid(x, o.grid);
  const ny = nearest([y, y + h / 2, y + h], guides.ys, o.tol);
  if (ny) {
    y += ny.d;
    markers.push({ kind: "y", v: ny.g.v, why: ny.g.why });
  } else if (o.gridOn) y = snapGrid(y, o.grid);
  if (o.gridOn && !nx && !ny) markers.push({ kind: "grid", at: [x, y], why: "grid" });
  return { rect: [x, y, w, h], markers };
}

/**
 * The 8 handles, in the order PaperView draws them: 0 top left, 1 top, 2 top right, 3 right, 4 bottom right, 5 bottom,
 * 6 bottom left, 7 left. Which edges each one moves: [left, top, right, bottom].
 */
export const HANDLE_EDGES: [boolean, boolean, boolean, boolean][] = [
  [true, true, false, false],
  [false, true, false, false],
  [false, true, true, false],
  [false, false, true, false],
  [false, false, true, true],
  [false, false, false, true],
  [true, false, false, true],
  [true, false, false, false],
];
export const HANDLE_CURSORS = ["nwse-resize", "ns-resize", "nesw-resize", "ew-resize", "nwse-resize", "ns-resize", "nesw-resize", "ew-resize"];
export const HANDLE_NAMES = ["top left corner", "top edge", "top right corner", "right edge", "bottom right corner", "bottom edge", "bottom left corner", "left edge"];

/** Handle positions on a frame (paper mm), in `HANDLE_EDGES` order. */
export function handlePoints([x, y, w, h]: Rect): P[] {
  return [
    [x, y],
    [x + w / 2, y],
    [x + w, y],
    [x + w, y + h / 2],
    [x + w, y + h],
    [x + w / 2, y + h],
    [x, y + h],
    [x, y + h / 2],
  ];
}

/**
 * A frame with handle `handle` dragged to `p`, snapped: the edges that handle moves go to a guide or the grid. A frame
 * never gets smaller than MIN_SIZE, and an edge dragged past the opposite one stops there (it never turns inside out).
 */
export function snapResize(r: Rect, handle: number, p: P, guides: Guides, o: SnapOpts): { rect: Rect; markers: Marker[] } {
  const [moveL, moveT, moveR, moveB] = HANDLE_EDGES[handle] ?? HANDLE_EDGES[4];
  let [l, t, rr, b] = [r[0], r[1], r[0] + r[2], r[1] + r[3]];
  const markers: Marker[] = [];
  const snapX = (v: number) => {
    const n = nearest([v], guides.xs, o.tol);
    if (n) {
      markers.push({ kind: "x", v: n.g.v, why: n.g.why });
      return n.g.v;
    }
    return o.gridOn ? snapGrid(v, o.grid) : v;
  };
  const snapY = (v: number) => {
    const n = nearest([v], guides.ys, o.tol);
    if (n) {
      markers.push({ kind: "y", v: n.g.v, why: n.g.why });
      return n.g.v;
    }
    return o.gridOn ? snapGrid(v, o.grid) : v;
  };
  if (moveL) l = Math.min(snapX(p[0]), rr - MIN_SIZE);
  if (moveR) rr = Math.max(snapX(p[0]), l + MIN_SIZE);
  if (moveT) t = Math.min(snapY(p[1]), b - MIN_SIZE);
  if (moveB) b = Math.max(snapY(p[1]), t + MIN_SIZE);
  return { rect: [l, t, rr - l, b - t], markers };
}

/** A point snapped: to a point first, else to lines on each axis, else to the grid. */
export function snapPoint(p: P, guides: Guides, o: SnapOpts, opts: { pointsOnly?: boolean } = {}): { p: P; marker: Marker | null; point: SnapPoint | null } {
  let best: SnapPoint | null = null;
  let bd = o.tol;
  for (const s of guides.points) {
    const d = Math.hypot(s.p[0] - p[0], s.p[1] - p[1]);
    if (d <= bd) {
      bd = d;
      best = s;
    }
  }
  if (best) return { p: [best.p[0], best.p[1]], marker: { kind: "point", at: best.p, why: best.why }, point: best };
  if (opts.pointsOnly) return { p, marker: null, point: null };
  const nx = nearest([p[0]], guides.xs, o.tol);
  const ny = nearest([p[1]], guides.ys, o.tol);
  if (nx || ny) {
    const q: P = [nx ? nx.g.v : o.gridOn ? snapGrid(p[0], o.grid) : p[0], ny ? ny.g.v : o.gridOn ? snapGrid(p[1], o.grid) : p[1]];
    const marker: Marker = nx && ny ? { kind: "point", at: q, why: `${nx.g.why} and ${ny.g.why}` } : nx ? { kind: "x", v: nx.g.v, why: nx.g.why } : { kind: "y", v: ny!.g.v, why: ny!.g.why };
    return { p: q, marker, point: null };
  }
  if (o.gridOn) {
    const q: P = [snapGrid(p[0], o.grid), snapGrid(p[1], o.grid)];
    return { p: q, marker: { kind: "grid", at: q, why: "grid" }, point: null };
  }
  return { p, marker: null, point: null };
}

/** Keep a line horizontal, vertical or at 45° from `a` (Shift while drawing), the nearest of the eight. */
export function ortho(a: P, p: P): P {
  const dx = p[0] - a[0];
  const dy = p[1] - a[1];
  const len = Math.hypot(dx, dy);
  if (len < 1e-9) return p;
  const ang = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
  const along = dx * Math.cos(ang) + dy * Math.sin(ang);
  return [a[0] + along * Math.cos(ang), a[1] + along * Math.sin(ang)];
}

/** A frame moved by the arrow keys: 1 mm, 10 with Shift (the grid's spacing when the grid is on and Shift is held). */
export function nudge(r: Rect, key: string, shift: boolean, grid: number, gridOn: boolean): Rect | null {
  const step = shift ? (gridOn ? Math.max(grid, 1) : 10) : 1;
  const d: Record<string, P> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
  const m = d[key];
  if (!m) return null;
  return [round2(r[0] + m[0]), round2(r[1] + m[1]), r[2], r[3]];
}

/** Paper mm to 0.01 (what the display list is drawn to; it keeps saved numbers short). */
export const round2 = (v: number) => Math.round(v * 100) / 100;
export const roundRect = (r: Rect): Rect => [round2(r[0]), round2(r[1]), round2(r[2]), round2(r[3])];
export const roundP = (p: P): P => [round2(p[0]), round2(p[1])];

/** A frame from two corners (either order). */
export function rectOf(a: P, b: P): Rect {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1])];
}
