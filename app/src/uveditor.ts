// The UV editor's 2D math: the view (UV space to the screen and back), picking (vertex, edge, island), marquee, the
// transforms the person applies (move, rotate, scale, flip) and how an edit goes back to the engine. No drawing and no
// React here, so it is all tested on its own (tests/uveditor.test.ts).
//
// Coordinates: (u, v) in tiles, v up (the engine's convention). The screen has y down; `View` does the flip.
// A "corner" is one corner of one face's outline (the engine keeps UVs per corner, so a seam has two values at one vertex).

export type V2 = [number, number];

/** One face as `uv_info` reports it. `uv` and `verts` run over the outer loop, then each hole; `loops` are their sizes. */
export interface UvFace {
  id: number;
  material: string | null;
  verts: number[];
  loops: number[];
  uv: V2[];
  island: number;
}

export interface UvInfo {
  faces: UvFace[];
  without: number[];
  islands: number;
  seams: number[];
}

/** The window onto UV space: (cx, cy) is at the middle of a w x h screen box, `zoom` pixels per tile. */
export interface View {
  cx: number;
  cy: number;
  zoom: number;
  w: number;
  h: number;
}

export const ZOOM_MIN = 8;
export const ZOOM_MAX = 20000;

export const toScreen = (v: View, p: V2): V2 => [v.w / 2 + (p[0] - v.cx) * v.zoom, v.h / 2 - (p[1] - v.cy) * v.zoom];
export const fromScreen = (v: View, s: V2): V2 => [v.cx + (s[0] - v.w / 2) / v.zoom, v.cy - (s[1] - v.h / 2) / v.zoom];

/** The whole 0..1 square in view with some air around it. */
export function fitView(w: number, h: number, pad = 28): View {
  const zoom = Math.max(ZOOM_MIN, Math.min(w - 2 * pad, h - 2 * pad));
  return { cx: 0.5, cy: 0.5, zoom, w, h };
}

/** Zoom by `factor` keeping the UV point under the screen point `at` where it is. */
export function zoomAt(v: View, at: V2, factor: number): View {
  const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v.zoom * factor));
  const p = fromScreen(v, at);
  return { ...v, zoom, cx: p[0] - (at[0] - v.w / 2) / zoom, cy: p[1] + (at[1] - v.h / 2) / zoom };
}

/** Slide the view by a screen drag. */
export function panBy(v: View, dx: number, dy: number): View {
  return { ...v, cx: v.cx - dx / v.zoom, cy: v.cy + dy / v.zoom };
}

// ---- corners -------------------------------------------------------------------------------------------------------------

/** "face:corner", the key of one corner. */
export const cornerKey = (face: number, corner: number) => `${face}:${corner}`;
export const parseKey = (k: string): [number, number] => {
  const [f, c] = k.split(":");
  return [Number(f), Number(c)];
};

const EPS = 1e-5;
const same = (a: V2, b: V2) => Math.abs(a[0] - b[0]) < EPS && Math.abs(a[1] - b[1]) < EPS;

export interface Index {
  faces: Map<number, UvFace>;
  /** model vertex id -> its corners, for finding the corners that sit together */
  byVertex: Map<number, { face: number; corner: number }[]>;
}

export function indexOf(info: UvInfo): Index {
  const faces = new Map<number, UvFace>();
  const byVertex = new Map<number, { face: number; corner: number }[]>();
  for (const f of info.faces) {
    faces.set(f.id, f);
    f.verts.forEach((v, corner) => {
      const l = byVertex.get(v) ?? [];
      l.push({ face: f.id, corner });
      byVertex.set(v, l);
    });
  }
  return { faces, byVertex };
}

const at = (ix: Index, k: string): V2 | null => {
  const [f, c] = parseKey(k);
  return ix.faces.get(f)?.uv[c] ?? null;
};

/** The corners that are the same point of the picture as this one: the same model vertex at the same place (moving a vertex
 *  moves all of them, so the island stays whole; across a seam they sit apart and stay apart). */
export function together(ix: Index, face: number, corner: number): string[] {
  const f = ix.faces.get(face);
  if (!f) return [];
  const p = f.uv[corner];
  return (ix.byVertex.get(f.verts[corner]) ?? []).filter((o) => same(ix.faces.get(o.face)!.uv[o.corner], p)).map((o) => cornerKey(o.face, o.corner));
}

// ---- picking -------------------------------------------------------------------------------------------------------------

/** The corner (and the ones sitting with it) nearest to a screen point, within `radius` pixels. */
export function hitVertex(info: UvInfo, ix: Index, v: View, s: V2, radius = 8): string[] | null {
  let best: { d: number; face: number; corner: number } | null = null;
  for (const f of info.faces) {
    f.uv.forEach((p, corner) => {
      const q = toScreen(v, p);
      const d = Math.hypot(q[0] - s[0], q[1] - s[1]);
      if (d <= radius && (!best || d < best.d)) best = { d, face: f.id, corner };
    });
  }
  return best ? together(ix, (best as { face: number }).face, (best as { corner: number }).corner) : null;
}

/** Distance from point `p` to the segment a-b. */
export function segDistance(p: V2, a: V2, b: V2): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** The outline edges of a face as pairs of corner indices. */
export function loopEdges(f: UvFace): [number, number][] {
  const out: [number, number][] = [];
  let off = 0;
  for (const n of f.loops) {
    for (let k = 0; k < n; k++) out.push([off + k, off + ((k + 1) % n)]);
    off += n;
  }
  return out;
}

/** The edge nearest a screen point within `radius` pixels: the corners at both ends (and the ones sitting with them). */
export function hitEdge(info: UvInfo, ix: Index, v: View, s: V2, radius = 6): string[] | null {
  let best: { d: number; f: UvFace; a: number; b: number } | null = null;
  for (const f of info.faces) {
    for (const [a, b] of loopEdges(f)) {
      const d = segDistance(s, toScreen(v, f.uv[a]), toScreen(v, f.uv[b]));
      if (d <= radius && (!best || d < best.d)) best = { d, f, a, b };
    }
  }
  if (!best) return null;
  const e = best as { f: UvFace; a: number; b: number };
  return [...new Set([...together(ix, e.f.id, e.a), ...together(ix, e.f.id, e.b)])];
}

/** Is the point inside the polygon (even-odd)? */
export function inPolygon(p: V2, poly: V2[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > p[1] !== yj > p[1] && p[0] < ((xj - xi) * (p[1] - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** A face's outer outline in UV space. */
export const outerOf = (f: UvFace): V2[] => f.uv.slice(0, f.loops[0] ?? f.uv.length);

/** The island under a UV point (the topmost face wins, so the last drawn); null if none. */
export function hitIsland(info: UvInfo, p: V2): number | null {
  for (let i = info.faces.length - 1; i >= 0; i--) {
    const f = info.faces[i];
    if (inPolygon(p, outerOf(f))) return f.island;
  }
  return null;
}

/** Every corner of an island. */
export function islandCorners(info: UvInfo, island: number): string[] {
  const out: string[] = [];
  for (const f of info.faces) if (f.island === island) f.uv.forEach((_, c) => out.push(cornerKey(f.id, c)));
  return out;
}

export const islandFaces = (info: UvInfo, island: number): number[] => info.faces.filter((f) => f.island === island).map((f) => f.id);

/** The islands a set of corners touches (an island is selected when all of its corners are). */
export function islandsSelected(info: UvInfo, sel: Set<string>): number[] {
  const all = new Map<number, number>();
  const got = new Map<number, number>();
  for (const f of info.faces) {
    all.set(f.island, (all.get(f.island) ?? 0) + f.uv.length);
    f.uv.forEach((_, c) => sel.has(cornerKey(f.id, c)) && got.set(f.island, (got.get(f.island) ?? 0) + 1));
  }
  return [...all.keys()].filter((i) => got.get(i) === all.get(i));
}

/** Corners inside a screen rectangle (drag corners in any order). */
export function marquee(info: UvInfo, v: View, a: V2, b: V2): string[] {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  const out: string[] = [];
  for (const f of info.faces)
    f.uv.forEach((p, c) => {
      const q = toScreen(v, p);
      if (q[0] >= x0 && q[0] <= x1 && q[1] >= y0 && q[1] <= y1) out.push(cornerKey(f.id, c));
    });
  return out;
}

/** The islands whose outline a screen rectangle touches or holds (for a box-select in island mode). */
export function marqueeIslands(info: UvInfo, v: View, a: V2, b: V2): number[] {
  const got = new Set<number>();
  for (const k of marquee(info, v, a, b)) got.add(info.faces.find((f) => f.id === parseKey(k)[0])!.island);
  return [...got];
}

// ---- transforms ----------------------------------------------------------------------------------------------------------

/** p' = [a c; b d] p + [e f], the engine's matrix order. */
export type Aff = [number, number, number, number, number, number];
export const IDENT: Aff = [1, 0, 0, 1, 0, 0];

export const apply = (m: Aff, p: V2): V2 => [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];

/** `second` after `first`. */
export function after(second: Aff, first: Aff): Aff {
  const [a, b, c, d, e, f] = second;
  const [a2, b2, c2, d2, e2, f2] = first;
  return [a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f];
}

export const translation = (dx: number, dy: number): Aff => [1, 0, 0, 1, dx, dy];

/** Turn by `deg` (counter-clockwise in UV space, which is what the eye sees: v is up) and scale by (sx, sy), about `pivot`. */
export function about(pivot: V2, deg: number, sx = 1, sy = sx): Aff {
  const r = (deg * Math.PI) / 180;
  const [c, s] = [Math.cos(r), Math.sin(r)];
  const a = c * sx;
  const b = s * sx;
  const cc = -s * sy;
  const d = c * sy;
  return [a, b, cc, d, pivot[0] - (a * pivot[0] + cc * pivot[1]), pivot[1] - (b * pivot[0] + d * pivot[1])];
}

export type FlipAxis = "h" | "v";
export const flip = (pivot: V2, axis: FlipAxis): Aff => (axis === "h" ? about(pivot, 0, -1, 1) : about(pivot, 0, 1, -1));

export function isFlat(m: Aff): boolean {
  return !m.every(Number.isFinite) || Math.abs(m[0] * m[3] - m[1] * m[2]) < 1e-9;
}

/** The middle of the selected corners' bounding box (the pivot of a turn or a scale); null for none. */
export function pivotOf(info: UvInfo, sel: Set<string>): V2 | null {
  const ix = indexOf(info);
  const pts = [...sel].map((k) => at(ix, k)).filter((p): p is V2 => !!p);
  if (!pts.length) return null;
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
}

export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** The bounding box of the selected corners (or of every corner when `sel` is null). */
export function boxOf(info: UvInfo, sel: Set<string> | null): Box | null {
  let b: Box | null = null;
  for (const f of info.faces)
    f.uv.forEach((p, c) => {
      if (sel && !sel.has(cornerKey(f.id, c))) return;
      b = b ? { x0: Math.min(b.x0, p[0]), y0: Math.min(b.y0, p[1]), x1: Math.max(b.x1, p[0]), y1: Math.max(b.y1, p[1]) } : { x0: p[0], y0: p[1], x1: p[0], y1: p[1] };
    });
  return b;
}

/** A copy of the faces with the selected corners moved by `m`. */
export function transformed(info: UvInfo, sel: Set<string>, m: Aff): UvInfo {
  return {
    ...info,
    faces: info.faces.map((f) => (f.uv.some((_, c) => sel.has(cornerKey(f.id, c))) ? { ...f, uv: f.uv.map((p, c) => (sel.has(cornerKey(f.id, c)) ? apply(m, p) : p)) } : f)),
  };
}

/** What `uv_set` takes after an edit: the faces whose corners moved, each with all its corners. */
export function setCommand(before: UvInfo, after: UvInfo, set: 1 | 2): Record<string, unknown> | null {
  const old = new Map(before.faces.map((f) => [f.id, f]));
  const uv: Record<string, V2[]> = {};
  for (const f of after.faces) {
    const o = old.get(f.id);
    if (!o || f.uv.some((p, i) => !same(p, o.uv[i]))) uv[String(f.id)] = f.uv.map((p) => [round(p[0]), round(p[1])]);
  }
  return Object.keys(uv).length ? { cmd: "uv_set", uv, ...(set === 2 ? { set } : {}) } : null;
}

const round = (x: number) => Math.round(x * 1e7) / 1e7;

/** True when the selection is whole islands: they can move with one `uv_transform` per... one command for all of them. */
export function wholeIslandsCommand(info: UvInfo, sel: Set<string>, m: Aff, set: 1 | 2): Record<string, unknown> | null {
  const isl = islandsSelected(info, sel);
  if (!isl.length || sel.size !== isl.reduce((n, i) => n + info.faces.filter((f) => f.island === i).reduce((k, f) => k + f.uv.length, 0), 0)) return null;
  const faces = isl.flatMap((i) => islandFaces(info, i));
  return { cmd: "uv_transform", faces, matrix: m.map(round), ...(set === 2 ? { set } : {}) };
}

// ---- edges (seams) -------------------------------------------------------------------------------------------------------

/** The model edges between selected corners: pairs of adjacent corners of a face outline, both selected, as vertex pairs. */
export function selectedEdges(info: UvInfo, sel: Set<string>): [number, number][] {
  const out = new Map<string, [number, number]>();
  for (const f of info.faces) {
    for (const [a, b] of loopEdges(f)) {
      if (sel.has(cornerKey(f.id, a)) && sel.has(cornerKey(f.id, b))) {
        const [x, y] = [f.verts[a], f.verts[b]].sort((p, q) => p - q);
        out.set(`${x}-${y}`, [x, y]);
      }
    }
  }
  return [...out.values()];
}

/** Model edge ids for vertex pairs, from the scene's edge list (`va`, `vb`). */
export function edgeIdsFor(pairs: [number, number][], edges: { id: number; va?: number; vb?: number }[]): number[] {
  const want = new Set(pairs.map(([a, b]) => `${Math.min(a, b)}-${Math.max(a, b)}`));
  return edges.filter((e) => e.va != null && e.vb != null && want.has(`${Math.min(e.va, e.vb)}-${Math.max(e.va, e.vb)}`)).map((e) => e.id);
}

// ---- pins ----------------------------------------------------------------------------------------------------------------

/** `uv_unwrap`'s pins: [face, corner, u, v] for each pinned corner still there, at its place now. */
export function pinList(info: UvInfo, pins: Set<string>): [number, number, number, number][] {
  const ix = indexOf(info);
  const out: [number, number, number, number][] = [];
  for (const k of pins) {
    const p = at(ix, k);
    if (p) out.push([...parseKey(k), p[0], p[1]] as [number, number, number, number]);
  }
  return out;
}

/** Pins that still name a corner of the faces shown. */
export function livePins(info: UvInfo, pins: Set<string>): Set<string> {
  const ix = indexOf(info);
  return new Set([...pins].filter((k) => at(ix, k)));
}

// ---- drawing helpers -----------------------------------------------------------------------------------------------------

/** One SVG path (d) per island: every face's loops, closed. */
export function islandPaths(info: UvInfo, v: View): Map<number, string> {
  const out = new Map<number, string>();
  for (const f of info.faces) {
    let off = 0;
    let d = out.get(f.island) ?? "";
    for (const n of f.loops) {
      d += f.uv
        .slice(off, off + n)
        .map((p, i) => {
          const q = toScreen(v, p);
          return `${i ? "L" : "M"}${q[0].toFixed(1)} ${q[1].toFixed(1)}`;
        })
        .join("") + "Z";
      off += n;
    }
    out.set(f.island, d);
  }
  return out;
}

/** The area of a polygon (signed, v up: counter-clockwise is positive). */
export function area(poly: V2[]): number {
  let a = 0;
  for (let i = 0; i < poly.length; i++) {
    const [p, q] = [poly[i], poly[(i + 1) % poly.length]];
    a += p[0] * q[1] - q[0] * p[1];
  }
  return a / 2;
}

/** The share of the 0..1 sheet the faces cover (for the "used" figure in the footer). */
export const coverage = (info: UvInfo): number => info.faces.reduce((n, f) => n + Math.abs(area(outerOf(f))), 0);

/** Does any corner lie outside the 0..1 sheet? */
export function outside(info: UvInfo): boolean {
  return info.faces.some((f) => f.uv.some((p) => p[0] < -1e-6 || p[0] > 1 + 1e-6 || p[1] < -1e-6 || p[1] > 1 + 1e-6));
}

/** The material most of the shown faces wear that has a picture; null if none. */
export function pictureMaterial(info: UvInfo, textured: Set<string>): string | null {
  const n = new Map<string, number>();
  for (const f of info.faces) if (f.material && textured.has(f.material)) n.set(f.material, (n.get(f.material) ?? 0) + 1);
  return [...n.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

/** Arrow-key nudge: a screen pixel step as a UV step. */
export const nudge = (zoom: number, big: boolean): number => (big ? 10 : 1) / zoom;
