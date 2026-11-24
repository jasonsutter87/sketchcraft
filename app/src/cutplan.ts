// Tools > Cut plan: what to ask the engine for, and what to draw from its answer.
// The engine's `cut_stock` / `cut_plan` commands do the packing (core/src/cutplan.rs, docs/dev/woodworker.md); this is
// the stock form's numbers, the cutting diagram's shapes and the money and size words.
import type { Sel } from "./selection";
import { parseLength, parseLengths } from "./units";

export interface Placed {
  number: number;
  id: number;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
  rotated: boolean;
}
export interface SheetPlan {
  index: number;
  material: string;
  thickness: number;
  size: [number, number];
  grain: boolean;
  parts: Placed[];
  waste_pct: number;
}
export interface BoardPart {
  number: number;
  id: number;
  name: string;
  x: number;
  length: number;
}
export interface BoardPlan {
  index: number;
  material: string;
  stock: string;
  length: number;
  cross: [number, number];
  parts: BoardPart[];
  waste_pct: number;
  board_feet: number;
}
export interface MaterialStock {
  sheet?: [number, number] | null;
  board_lengths?: number[] | null;
  grain: boolean;
  sheet_price: number;
  board_price: number;
  board_foot_price: number;
}
export interface Stock {
  kerf: number;
  trim: number;
  sheet: [number, number];
  board_lengths: number[];
  lengths_by_size: Record<string, number[]>;
  currency: string;
  materials: Record<string, MaterialStock>;
}
export interface MaterialCost {
  material: string;
  sheets: number;
  boards: number;
  board_feet: number;
  cost: number;
  priced: boolean;
}
export interface CutPlanReply {
  ok?: boolean;
  units: string;
  sheets: SheetPlan[];
  boards: BoardPlan[];
  unplaced: { number: number; name: string; reason: string }[];
  parts: { number: number; name: string; material: string; kind: string; on: number | null }[];
  summary: { parts: number; sheets: number; boards: number; unplaced: number; waste_pct: number };
  cost: { currency: string; total: number; by_material: MaterialCost[] };
  stock: Stock;
  [key: string]: unknown;
}

/** Sizes are typed in millimetres, or inches for an imperial model. */
export const stockUnits = (modelUnits: string): "mm" | "in" => (modelUnits === "ft" || modelUnits === "in" ? "in" : "mm");

const INCH = 0.0254;

/** A length in meters as the form shows it: whole millimetres, or inches to 1/16 as a decimal. */
export function lengthText(m: number, units: "mm" | "in"): string {
  if (units === "mm") return String(Math.round(m * 1000));
  return String(Math.round((m / INCH) * 16) / 16);
}

/** A length as words: 2440 mm, or 96" for an imperial model. */
export function lengthWords(m: number, units: "mm" | "in"): string {
  return units === "mm" ? `${Math.round(m * 1000)} mm` : `${lengthText(m, "in")}"`;
}

export function money(currency: string, v: number): string {
  return `${currency}${v.toFixed(2)}`;
}

/** The groups and components the person has picked; none: every part. */
export function planCommand(sel: Sel): Record<string, unknown> {
  const ids = [...sel.groups, ...sel.instances];
  return { cmd: "cut_plan", ...(ids.length ? { ids } : {}) };
}

/** The form's text for the stock: what each field shows. */
export interface StockForm {
  kerf: string;
  trim: string;
  sheet: string;
  boardLengths: string;
  currency: string;
  materials: Record<string, { sheetPrice: string; boardPrice: string; boardFootPrice: string; grain: boolean }>;
}

const price = (v: number | undefined) => (v ? String(v) : "");

export function formFromStock(s: Stock, units: "mm" | "in", used: string[]): StockForm {
  const names = [...new Set([...used, ...Object.keys(s.materials)])].sort();
  return {
    kerf: lengthText(s.kerf, units),
    trim: lengthText(s.trim, units),
    sheet: s.sheet.map((v) => lengthText(v, units)).join(" × "),
    boardLengths: s.board_lengths.map((v) => lengthText(v, units)).join(", "),
    currency: s.currency,
    materials: Object.fromEntries(names.map((n) => [n, { sheetPrice: price(s.materials[n]?.sheet_price), boardPrice: price(s.materials[n]?.board_price), boardFootPrice: price(s.materials[n]?.board_foot_price), grain: !!s.materials[n]?.grain }])),
  };
}

/** A price typed by hand: empty is no price, anything else must be a number of zero or more. NaN if it is not one. */
export function parsePrice(text: string): number {
  const t = text.trim().replace(/^[^\w.-]+/, "");
  if (!t) return 0;
  return /^\d+(\.\d+)?$/.test(t) ? Number(t) : NaN;
}

/** The `cut_stock` command for a form, or why it can not be sent. */
export function stockCommand(f: StockForm, units: "mm" | "in"): { cmd: Record<string, unknown> } | { error: string } {
  const len = (t: string) => parseLength(t, units);
  const kerf = t0(f.kerf, len);
  const trim = t0(f.trim, len);
  if (Number.isNaN(kerf) || kerf < 0) return { error: "kerf" };
  if (Number.isNaN(trim) || trim < 0) return { error: "trim" };
  const sheet = f.sheet.split(/[x×*]/i).map((p) => len(p));
  if (sheet.length !== 2 || sheet.some((v) => !(v > 0))) return { error: "sheet" };
  const lengths = parseLengths(f.boardLengths.replace(/\s+/g, " "), units);
  if (!lengths.length || lengths.some((v) => !(v > 0))) return { error: "boards" };
  const materials: Record<string, unknown> = {};
  for (const [name, m] of Object.entries(f.materials)) {
    const [sp, bp, bf] = [parsePrice(m.sheetPrice), parsePrice(m.boardPrice), parsePrice(m.boardFootPrice)];
    if ([sp, bp, bf].some(Number.isNaN)) return { error: "price" };
    materials[name] = { sheet_price: sp, board_price: bp, board_foot_price: bf, grain: m.grain };
  }
  return { cmd: { cmd: "cut_stock", kerf, trim, sheet, board_lengths: lengths, currency: f.currency.trim().slice(0, 8), materials } };
}

function t0(text: string, len: (t: string) => number): number {
  return text.trim() === "" ? 0 : len(text);
}

/** One rectangle of a cutting diagram, in the picture's own units. */
export interface Shape {
  key: string;
  x: number;
  y: number;
  w: number;
  h: number;
  number: number;
  name: string;
  size: string;
  tint: number;
}
export interface Diagram {
  width: number;
  height: number;
  /** the stock itself */
  stock: { x: number; y: number; w: number; h: number };
  shapes: Shape[];
}

const PAD = 4;

/** The shapes of one sheet drawn to fit `width` pixels. */
export function sheetDiagram(s: SheetPlan, width: number, units: "mm" | "in"): Diagram {
  const k = (width - 2 * PAD) / s.size[0];
  const shapes = s.parts.map((p, i) => ({ key: `p${p.number}`, x: PAD + p.x * k, y: PAD + p.y * k, w: p.w * k, h: p.h * k, number: p.number, name: p.name, size: `${lengthWords(p.w, units)} × ${lengthWords(p.h, units)}`, tint: i % 4 }));
  return { width, height: s.size[1] * k + 2 * PAD, stock: { x: PAD, y: PAD, w: s.size[0] * k, h: s.size[1] * k }, shapes };
}

/** The shapes of one board drawn to fit `width` pixels, 36 high. */
export function boardDiagram(b: BoardPlan, width: number, units: "mm" | "in"): Diagram {
  const k = (width - 2 * PAD) / b.length;
  const h = 36;
  const shapes = b.parts.map((p, i) => ({ key: `p${p.number}`, x: PAD + p.x * k, y: PAD, w: p.length * k, h, number: p.number, name: p.name, size: lengthWords(p.length, units), tint: i % 4 }));
  return { width, height: h + 2 * PAD, stock: { x: PAD, y: PAD, w: b.length * k, h }, shapes };
}

/** What to call a sheet or board in a list: "Sheet 2 of 5 · Birch ply". */
export function stockTitle(kind: "sheet" | "board", index: number, of: number, material: string, t: (id: string) => string, extra = ""): string {
  const base = `${t(kind === "sheet" ? "cut.sheet" : "cut.board")} ${index} / ${of}`;
  return [base, material || t("cut.noMaterial"), extra].filter(Boolean).join(" · ");
}

/** The materials the plan uses, in order. */
export const usedMaterials = (r: CutPlanReply): string[] => [...new Set([...r.sheets.map((s) => s.material), ...r.boards.map((b) => b.material), ...r.unplaced.map((u) => (r.parts.find((p) => p.number === u.number)?.material ?? ""))])];
