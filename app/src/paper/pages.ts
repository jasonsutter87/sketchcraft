// The Layout workspace's pages: saved sheets as `sheets_info` lists them, and the commands the Pages panel sends.
// Pure (unit tested in app/tests/paper.test.ts); the panel is pagespanel.tsx.

/** One saved viewport as `sheets_info` lists it (only what the Layout workspace reads). */
export interface ViewportItem {
  id: number;
  kind: "viewport";
  sheet: number;
  rect_mm: [number, number, number, number];
  scale: number;
  /** 0 to 1; absent: 1 */
  opacity?: number;
  label?: string;
  z?: number;
  view?: { type?: string; [k: string]: unknown };
  scene?: string | null;
  render?: string;
}

export interface SheetItemInfo {
  id: number;
  kind: string;
  [k: string]: unknown;
}

export interface SheetInfo {
  id: number;
  name: string;
  number: string;
  paper: { w_mm: number; h_mm: number; name: string };
  order: number;
  fields: Record<string, string>;
  issued: number;
  items: SheetItemInfo[];
}

export const isViewport = (i: SheetItemInfo): i is SheetItemInfo & ViewportItem => i.kind === "viewport" && Array.isArray(i.rect_mm);

/** A sheet's viewports, bottom first (the order they are drawn in: z, then id). */
export const viewportsOf = (s: SheetInfo | undefined): ViewportItem[] =>
  (s?.items ?? [])
    .filter(isViewport)
    .map((v) => v as unknown as ViewportItem)
    .sort((a, b) => (a.z ?? 0) - (b.z ?? 0) || a.id - b.id);

/** The sheets in the set's order (order, then id: the engine's). */
export const inOrder = (sheets: SheetInfo[]): SheetInfo[] => [...sheets].sort((a, b) => a.order - b.order || a.id - b.id);

/** `ids` with the one at `from` moved to `to` (clamped; an out-of-range `from` changes nothing). */
export function moveIn<T>(list: T[], from: number, to: number): T[] {
  if (from < 0 || from >= list.length) return list;
  const at = Math.max(0, Math.min(list.length - 1, to));
  if (at === from) return list;
  const next = [...list];
  const [x] = next.splice(from, 1);
  next.splice(at, 0, x);
  return next;
}

/**
 * The command that puts the sheets in `order` (ids, first first): `sheet_order`, one command and one undo step (the
 * engine numbers them 0, 10, 20...). Null when that is already their order, or the list isn't every sheet once.
 */
export function reorderCommand(order: number[], sheets: SheetInfo[]): Record<string, unknown> | null {
  const now = inOrder(sheets).map((s) => s.id);
  if (order.length !== now.length || new Set(order).size !== order.length || !order.every((id) => now.includes(id))) return null;
  if (order.every((id, i) => now[i] === id)) return null;
  return { cmd: "sheet_order", sheets: order };
}

/** A name for a new page that no page has yet ("Page 3"). */
export function newPageName(sheets: SheetInfo[]): string {
  const names = new Set(sheets.map((s) => s.name));
  for (let n = sheets.length + 1; ; n++) if (!names.has(`Page ${n}`)) return `Page ${n}`;
}

/** Paper sizes the drawing knows (`draft/paper.rs`), for the inspector: value is what `sheet_edit paper` takes. */
export const PAPERS: { id: string; label: string; w: number; h: number }[] = [
  { id: "a4", label: "A4", w: 297, h: 210 },
  { id: "a3", label: "A3", w: 420, h: 297 },
  { id: "a2", label: "A2", w: 594, h: 420 },
  { id: "a1", label: "A1", w: 841, h: 594 },
  { id: "a0", label: "A0", w: 1189, h: 841 },
  { id: "letter", label: "Letter", w: 279.4, h: 215.9 },
  { id: "tabloid", label: "Tabloid (11×17)", w: 431.8, h: 279.4 },
  { id: "archc", label: "ARCH C (18×24)", w: 609.6, h: 457.2 },
  { id: "archd", label: "ARCH D (24×36)", w: 914.4, h: 609.6 },
  { id: "arche", label: "ARCH E (36×48)", w: 1219.2, h: 914.4 },
  { id: "ansic", label: "ANSI C (17×22)", w: 558.8, h: 431.8 },
  { id: "ansid", label: "ANSI D (22×34)", w: 863.6, h: 558.8 },
];

/** The paper id of a sheet's size (by its name, else by its size), or "" for a custom size. */
export function paperId(p: SheetInfo["paper"]): string {
  const key = (p.name ?? "").toLowerCase().replace(/[^a-z0-9]/g, "");
  const named = PAPERS.find((x) => x.id === key);
  if (named) return named.id;
  return PAPERS.find((x) => Math.abs(x.w - p.w_mm) < 0.5 && Math.abs(x.h - p.h_mm) < 0.5)?.id ?? "";
}

/** "1:50" for a viewport's scale N. */
export const scaleText = (n: number) => `1:${Number.isInteger(n) ? n : Number(n.toFixed(2))}`;

/** A typed scale as `viewport_edit scale` takes it: "50", "1:50", "1/4\"=1'-0\"" (the engine reads the imperial ones). */
export function parseScale(text: string): number | string | null {
  const t = text.trim();
  if (!t) return null;
  if (/^\d+(\.\d+)?$/.test(t)) return Number(t) > 0 ? Number(t) : null;
  if (/^1\s*:\s*\d+(\.\d+)?$/.test(t)) return t.replace(/\s+/g, "");
  if (/[="']/.test(t)) return t;
  return null;
}

/** An opacity percentage (the slider, 0 to 100) as `viewport_edit opacity` (0 to 1), and back. */
export const opacityOf = (pct: number) => Math.round(Math.min(100, Math.max(0, pct))) / 100;
export const percentOf = (v: ViewportItem | undefined) => Math.round((v?.opacity ?? 1) * 100);

/** The standard views a viewport can show (`viewport_edit from_view`), plus the model's saved views. */
export const STANDARD_VIEWS = ["iso", "top", "front", "back", "left", "right"];

/** A side's elevation (`dir`: from the model toward the viewer, as the engine takes it). */
export const ELEVATIONS: Record<string, [number, number, number]> = { front: [0, -1, 0], back: [0, 1, 0], left: [-1, 0, 0], right: [1, 0, 0] };

/**
 * What `viewport_add` / `viewport_edit` take to show `name`: "plan" (a floor plan cut 1.2 m up), a side (a true
 * elevation, hidden lines removed), or a standard or saved view's name (a 3D view from there).
 */
export function viewArgs(name: string): Record<string, unknown> {
  if (name === "plan") return { view: { type: "plan", cut_z: 1.2, floor_z: 0 } };
  if (ELEVATIONS[name]) return { view: { type: "elevation", dir: ELEVATIONS[name] } };
  return { from_view: name };
}

/** What a viewport shows, in words ("Plan", "Front elevation", "3D view"...). */
export function viewName(v: ViewportItem): string {
  const t = v.view?.type;
  if (t === "plan") return "Plan";
  if (t === "section") return "Section";
  if (t === "elevation") {
    const d = v.view?.dir as number[] | undefined;
    const name = d ? (d[1] < -0.5 ? "Front" : d[1] > 0.5 ? "Back" : d[0] > 0.5 ? "Right" : d[0] < -0.5 ? "Left" : "") : "";
    return `${name} elevation`.trim();
  }
  if (t === "view3d") return v.scene ? `3D view (${v.scene})` : "3D view";
  return "View";
}
