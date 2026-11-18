// What is on a page besides viewports, as the editor sees it: paper line work, text and dimensions (from
// `sheets_info`), their shapes in paper mm, hit testing, the points they snap to, and the title block's place.
// Pure (unit tested in app/tests/sheeteditor.test.ts). The engine draws them (draft/sheet.rs, dims.rs); this only
// has to agree with it closely enough to pick and preview.

import type { SheetInfo, SheetItemInfo } from "./pages";
import { MARGIN, type P, type Rect, type SnapPoint } from "./snap";

export type ShapeKind = "line" | "polyline" | "arc" | "rect";
export interface ShapeItem {
  id: number;
  kind: ShapeKind;
  pts_mm: P[];
  pen?: { w_mm?: number; color?: string; dash_mm?: number[] };
  z?: number;
}
export interface TextItem {
  id: number;
  kind: "text";
  at_mm: P;
  paragraphs: string[];
  height_mm?: number;
  z?: number;
}
export interface DimItem {
  id: number;
  kind: "dimension";
  mode?: string;
  offset_mm?: number;
  paper_mm?: P[];
  viewport?: number;
  measured?: { spans?: { from: number; to: number; text: string }[]; attached?: boolean };
}
export type PaperItem = ShapeItem | TextItem | DimItem;

const SHAPES = new Set(["line", "polyline", "arc", "rect"]);
export const isShape = (i: SheetItemInfo | PaperItem): i is ShapeItem => SHAPES.has(i.kind) && Array.isArray((i as ShapeItem).pts_mm);
export const isText = (i: SheetItemInfo | PaperItem): i is TextItem => i.kind === "text" && Array.isArray((i as TextItem).at_mm);
export const isDim = (i: SheetItemInfo | PaperItem): i is DimItem => i.kind === "dimension";

/** The page's line work, text and dimensions, in drawing order. */
export function paperItems(s: SheetInfo | undefined): PaperItem[] {
  const all = (s?.items ?? []) as unknown as PaperItem[];
  const z = (i: PaperItem) => ("z" in i ? (i.z ?? 0) : 0);
  return all.filter((i) => isShape(i) || isText(i) || isDim(i)).sort((a, b) => z(a) - z(b) || a.id - b.id);
}

// ---- geometry ---------------------------------------------------------------------------------------------------

const sub = (a: P, b: P): P => [a[0] - b[0], a[1] - b[1]];
const dist = (a: P, b: P) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/** The circle through three points (an arc's start, a point on it, its end): centre, radius, start angle, signed sweep. */
export function arcThrough(a: P, m: P, b: P): { c: P; r: number; a0: number; sweep: number } | null {
  const d = 2 * (a[0] * (m[1] - b[1]) + m[0] * (b[1] - a[1]) + b[0] * (a[1] - m[1]));
  if (Math.abs(d) < 1e-9) return null;
  const sq = (p: P) => p[0] * p[0] + p[1] * p[1];
  const cx = (sq(a) * (m[1] - b[1]) + sq(m) * (b[1] - a[1]) + sq(b) * (a[1] - m[1])) / d;
  const cy = (sq(a) * (b[0] - m[0]) + sq(m) * (a[0] - b[0]) + sq(b) * (m[0] - a[0])) / d;
  const c: P = [cx, cy];
  const ang = (p: P) => Math.atan2(p[1] - cy, p[0] - cx);
  const a0 = ang(a);
  const norm = (t: number) => ((t % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  const toM = norm(ang(m) - a0);
  const toB = norm(ang(b) - a0);
  // the way round that passes the middle point
  const sweep = toM <= toB ? toB : toB - 2 * Math.PI;
  return { c, r: dist(a, c), a0, sweep };
}

/** A shape as points along it (arcs in short steps): what the overlay draws and hit tests. */
export function polylineOf(kind: ShapeKind, pts: P[], arcSteps = 48): P[] {
  if (kind === "rect" && pts.length >= 2) {
    const [a, b] = pts;
    return [a, [b[0], a[1]], b, [a[0], b[1]], a];
  }
  if (kind === "arc" && pts.length >= 3) {
    const k = arcThrough(pts[0], pts[1], pts[2]);
    if (!k) return [pts[0], pts[2]];
    const out: P[] = [];
    for (let i = 0; i <= arcSteps; i++) {
      const t = k.a0 + (k.sweep * i) / arcSteps;
      out.push([k.c[0] + k.r * Math.cos(t), k.c[1] + k.r * Math.sin(t)]);
    }
    return out;
  }
  return pts;
}

/** The nearest point on segment ab to p, and how far along (0..1). */
export function nearestOnSeg(p: P, a: P, b: P): { q: P; t: number; d: number } {
  const ab = sub(b, a);
  const l2 = ab[0] * ab[0] + ab[1] * ab[1];
  const t = l2 < 1e-12 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2));
  const q: P = [a[0] + ab[0] * t, a[1] + ab[1] * t];
  return { q, t, d: dist(p, q) };
}

/** How far a point is from a polyline. */
export function distToPolyline(p: P, line: P[]): number {
  let best = Infinity;
  for (let i = 0; i + 1 < line.length; i++) best = Math.min(best, nearestOnSeg(p, line[i], line[i + 1]).d);
  if (line.length === 1) best = dist(p, line[0]);
  return best;
}

/** A paper text's box (mm): its first line's baseline starts at at_mm (draft/sheet.rs `text_item`). */
export const CAP_OF_SIZE = 0.73;
export const TEXT_LEADING = 1.25;
export const TEXT_HEIGHT = 3.5;
export function textBox(t: TextItem, width: (s: string, sizeMm: number) => number): Rect {
  const h = t.height_mm ?? TEXT_HEIGHT;
  const size = h / CAP_OF_SIZE;
  const pitch = size * TEXT_LEADING;
  const w = Math.max(size, ...t.paragraphs.map((l) => width(l, size)));
  const top = t.at_mm[1] - h * 1.05;
  const bottom = t.at_mm[1] + (t.paragraphs.length - 1) * pitch + size * 0.25;
  return [t.at_mm[0], top, w, bottom - top];
}
/** A rough text width (no canvas: tests, or before the font loads): Inter averages about 0.55 of its size. */
export const roughWidth = (s: string, sizeMm: number) => s.length * sizeMm * 0.55;

/** The dimension's measured line on paper, per span (as draft/dims.rs places it), and its extension lines. */
export function dimLines(pts: P[], mode: string | undefined, offset: number): { spans: [P, P][]; ext: [P, P][] } {
  const n = pts.length;
  if (n < 2) return { spans: [], ext: [] };
  let dir: P;
  if (mode === "horizontal") dir = [1, 0];
  else if (mode === "vertical") dir = [0, 1];
  else {
    const d = sub(pts[n - 1], pts[0]);
    const l = Math.hypot(d[0], d[1]);
    dir = l < 1e-9 ? [1, 0] : [d[0] / l, d[1] / l];
  }
  const nrm: P = [dir[1], -dir[0]];
  const dot = (p: P, v: P) => p[0] * v[0] + p[1] * v[1];
  const sign = offset >= 0 ? 1 : -1;
  const ns = pts.map((p) => dot(p, nrm));
  const nref = sign > 0 ? Math.max(...ns) : Math.min(...ns);
  const spans: [P, P][] = [];
  const ext: [P, P][] = [];
  const pairs: [number, number][] = mode === "baseline" ? pts.slice(1).map((_, k) => [0, k + 1]) : pts.slice(1).map((_, k) => [k, k + 1]);
  pairs.forEach(([i, j], k) => {
    const c = nref + offset + (mode === "baseline" ? sign * 7 * k : 0);
    const on = (p: P): P => [dir[0] * dot(p, dir) + nrm[0] * c, dir[1] * dot(p, dir) + nrm[1] * c];
    const a = on(pts[i]);
    const b = on(pts[j]);
    spans.push([a, b]);
    ext.push([pts[i], a], [pts[j], b]);
  });
  return { spans, ext };
}

/**
 * The `offset_mm` that puts a dimension's line through the paper point `at` (a positive offset goes up the sheet for
 * a level line, right for a plumb one: draft/dims.rs). Past the points on either side it is that far beyond the
 * outermost point; between them it goes to the nearer side.
 */
export function dimOffsetAt(pts: P[], mode: string | undefined, at: P): number {
  const n = pts.length;
  if (n < 2) return 8;
  let dir: P;
  if (mode === "horizontal") dir = [1, 0];
  else if (mode === "vertical") dir = [0, 1];
  else {
    const d = sub(pts[n - 1], pts[0]);
    const l = Math.hypot(d[0], d[1]);
    dir = l < 1e-9 ? [1, 0] : [d[0] / l, d[1] / l];
  }
  const nrm: P = [dir[1], -dir[0]];
  const ns = pts.map((p) => p[0] * nrm[0] + p[1] * nrm[1]);
  const hi = Math.max(...ns);
  const lo = Math.min(...ns);
  const c = at[0] * nrm[0] + at[1] * nrm[1];
  const raw = c >= (hi + lo) / 2 ? Math.max(0, c - hi) : Math.min(0, c - lo);
  // never right on the points (a zero offset draws the line over the model): at least 2 mm out
  const off = Math.abs(raw) < 2 ? (raw < 0 || (raw === 0 && c < (hi + lo) / 2) ? -2 : 2) : raw;
  return Math.round(off * 10) / 10;
}

/** Horizontal or vertical, whichever the points spread along more (Auto); or what was chosen. */
export function dimModeFor(pts: P[], choice: "auto" | "aligned" | "horizontal" | "vertical"): "aligned" | "horizontal" | "vertical" {
  if (choice !== "auto") return choice;
  if (pts.length < 2) return "horizontal";
  const xs = pts.map((p) => p[0]);
  const ys = pts.map((p) => p[1]);
  return Math.max(...xs) - Math.min(...xs) >= Math.max(...ys) - Math.min(...ys) ? "horizontal" : "vertical";
}

/** Where an item's ends and middles are, to snap to. */
export function itemPoints(items: PaperItem[], width = roughWidth): SnapPoint[] {
  const out: SnapPoint[] = [];
  for (const i of items) {
    if (isShape(i)) {
      const line = i.kind === "arc" ? i.pts_mm : polylineOf(i.kind, i.pts_mm);
      const corners = i.kind === "arc" ? [i.pts_mm[0], i.pts_mm[2]].filter(Boolean) : line;
      for (const p of corners) out.push({ p, why: "end" });
      if (i.kind !== "arc")
        for (let k = 0; k + 1 < line.length; k++) out.push({ p: [(line[k][0] + line[k + 1][0]) / 2, (line[k][1] + line[k + 1][1]) / 2], why: "midpoint" });
      else {
        const k = arcThrough(i.pts_mm[0], i.pts_mm[1], i.pts_mm[2]);
        if (k) out.push({ p: k.c, why: "centre" }, { p: polylineOf("arc", i.pts_mm, 2)[1], why: "midpoint" });
      }
    } else if (isText(i)) {
      out.push({ p: i.at_mm, why: "text" });
      void width;
    }
  }
  return out;
}

/** How far the paper point is from the item (mm), for picking. */
export function distToItem(i: PaperItem, p: P, width = roughWidth): number {
  if (isShape(i)) return distToPolyline(p, polylineOf(i.kind, i.pts_mm));
  if (isText(i)) {
    const [x, y, w, h] = textBox(i, width);
    const dx = Math.max(x - p[0], 0, p[0] - (x + w));
    const dy = Math.max(y - p[1], 0, p[1] - (y + h));
    return Math.hypot(dx, dy);
  }
  if (isDim(i) && i.paper_mm && i.paper_mm.length >= 2) {
    const g = dimLines(i.paper_mm, i.mode, i.offset_mm ?? 8);
    return Math.min(...g.spans.map(([a, b]) => distToPolyline(p, [a, b])));
  }
  return Infinity;
}

/** The topmost item within `tol` mm of p (nearest when several are), or null. */
export function itemAt(items: PaperItem[], p: P, tol: number, only?: (i: PaperItem) => boolean, width = roughWidth): PaperItem | null {
  let best: PaperItem | null = null;
  let bd = tol;
  for (let k = items.length - 1; k >= 0; k--) {
    const i = items[k];
    if (only && !only(i)) continue;
    const d = distToItem(i, p, width);
    if (d < bd - 1e-9) {
      bd = d;
      best = i;
    }
  }
  return best;
}

/** The item moved by (dx, dy): its points (line work), or its place (text). */
export function movedItem(i: PaperItem, d: P): Record<string, unknown> | null {
  const mv = (p: P): P => [Math.round((p[0] + d[0]) * 100) / 100, Math.round((p[1] + d[1]) * 100) / 100];
  if (isShape(i)) return { cmd: "sheet_line", id: i.id, pts_mm: i.pts_mm.map(mv) };
  if (isText(i)) return { cmd: "sheet_text", id: i.id, at_mm: mv(i.at_mm) };
  return null;
}

/** The part of a shape between two points on it (a trim's `cut`), for the preview: walked along its outline. */
export function cutPath(i: ShapeItem, from: P, to: P): P[] {
  const line = polylineOf(i.kind, i.pts_mm, 96);
  // where each point falls along the outline
  const at = (p: P) => {
    let best = { s: 0, d: Infinity };
    let run = 0;
    for (let k = 0; k + 1 < line.length; k++) {
      const n = nearestOnSeg(p, line[k], line[k + 1]);
      const len = dist(line[k], line[k + 1]);
      if (n.d < best.d) best = { s: run + n.t * len, d: n.d };
      run += len;
    }
    return best.s;
  };
  let s0 = at(from);
  let s1 = at(to);
  if (s1 < s0) [s0, s1] = [s1, s0];
  const out: P[] = [];
  let run = 0;
  for (let k = 0; k + 1 < line.length; k++) {
    const a = line[k];
    const b = line[k + 1];
    const len = dist(a, b);
    const lerp = (s: number): P => {
      const t = len < 1e-12 ? 0 : (s - run) / len;
      return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    };
    if (run + len >= s0 && run <= s1) {
      if (!out.length) out.push(lerp(Math.max(s0, run)));
      out.push(lerp(Math.min(s1, run + len)));
    }
    run += len;
  }
  return out.length >= 2 ? out : [from, to];
}

// ---- the title block ------------------------------------------------------------------------------------------------

/** Where the title block is (draft/sheet.rs `title_block`): its frame, and the project and page-name rows. */
export function titleBlock(w: number, h: number): { rect: Rect; project: Rect; title: Rect } {
  const tw = Math.min(110, w * 0.4);
  const th = 32;
  const tx = w - MARGIN - tw;
  const ty = h - MARGIN - th;
  return { rect: [tx, ty, tw, th], project: [tx, ty, tw, 14], title: [tx, ty + 14, tw, 18] };
}

/** A shape's kind and size in words, for screen readers. */
export function describe(i: PaperItem): string {
  const mm = (v: number) => `${Math.round(v)} mm`;
  if (isShape(i)) {
    const line = polylineOf(i.kind, i.pts_mm);
    let len = 0;
    for (let k = 0; k + 1 < line.length; k++) len += dist(line[k], line[k + 1]);
    if (i.kind === "rect") {
      const [a, b] = i.pts_mm;
      return `Rectangle, ${mm(Math.abs(b[0] - a[0]))} by ${mm(Math.abs(b[1] - a[1]))}`;
    }
    return `${i.kind === "line" ? "Line" : i.kind === "arc" ? "Arc" : `Polyline of ${i.pts_mm.length} points`}, ${mm(len)} long`;
  }
  if (isText(i)) return `Text: ${i.paragraphs.join(" / ").slice(0, 80)}`;
  if (isDim(i)) {
    const spans = i.measured?.spans ?? [];
    return `${spans.length > 1 ? `Chain dimension of ${spans.length}` : "Dimension"}: ${spans.map((s) => s.text).join(", ")}${i.measured?.attached === false ? " (broken: its geometry is gone)" : ""}`;
  }
  return "Item";
}
