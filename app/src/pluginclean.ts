// What a plugin hands the interface (a result to show, a form to fill in) is
// untrusted: it is checked and cut down here before React sees it, so a wrong
// shape or a huge table can't blank or freeze the app. Pure, so checks/run.mjs
// can test it on its own.

export interface PluginResult {
  title?: string;
  text?: string;
  table?: { columns: string[]; rows: (string | number)[][] };
}

/** A form a plugin asks you to fill in (ctx.ask). Lengths are in meters. */
export interface PluginForm {
  title?: string;
  text?: string;
  ok?: string;
  fields: PluginField[];
}
export interface PluginField {
  id: string;
  label: string;
  /** "length" (typed in the model's units, answered in meters), "number", "text", "choice" or "check" */
  type?: "length" | "number" | "text" | "choice" | "check";
  value?: string | number | boolean;
  options?: string[];
  min?: number;
  max?: number;
  hint?: string;
}

// ---- cleaning -------------------------------------------------------------------

/** Text a plugin shows in a body (results, form text): line breaks kept, hidden characters taken out, cut to `max`. */
const str = (v: unknown, max: number): string | undefined => {
  let s: string;
  if (typeof v === "string") s = v;
  else if (v == null) return undefined;
  else {
    try {
      s = String(v);
    } catch {
      return undefined; // an object with no usable text
    }
  }
  return s.replace(HIDDEN_IN_TEXT, "").slice(0, max);
};
export const cleanText = (v: unknown, max: number): string => str(v, max) ?? "";

/** A plugin's result, cleaned: strings and numbers only, within limits. */
export function cleanResult(r: unknown): PluginResult {
  const o = (r && typeof r === "object" ? r : {}) as Record<string, unknown>;
  const out: PluginResult = { title: o.title == null ? undefined : cleanLabel(o.title, 120), text: str(o.text, 20_000) };
  const t = o.table as { columns?: unknown; rows?: unknown } | undefined;
  if (t && Array.isArray(t.columns) && Array.isArray(t.rows)) {
    const columns = t.columns.slice(0, 40).map((c) => cleanLabel(c, 80));
    const rows = t.rows.slice(0, 2000).map((row) => (Array.isArray(row) ? row.slice(0, columns.length).map((c) => (typeof c === "number" && Number.isFinite(c) ? c : (str(c, 400) ?? ""))) : []));
    out.table = { columns, rows };
  }
  return out;
}

/** A plugin's form, cleaned the same way. */
export function cleanForm(f: unknown): PluginForm {
  const o = (f && typeof f === "object" ? f : {}) as Record<string, unknown>;
  const types = ["length", "number", "text", "choice", "check"];
  const fields: PluginField[] = (Array.isArray(o.fields) ? o.fields : [])
    .slice(0, 40)
    .filter((x): x is Record<string, unknown> => !!x && typeof x === "object" && typeof (x as Record<string, unknown>).id === "string")
    .map((x) => ({
      id: String(x.id).slice(0, 80),
      label: cleanLabel(x.label, 120) || cleanLabel(x.id, 80),
      type: types.includes(x.type as string) ? (x.type as PluginField["type"]) : "text",
      value: typeof x.value === "number" && Number.isFinite(x.value) ? x.value : typeof x.value === "boolean" ? x.value : (str(x.value, 400) ?? ""),
      options: Array.isArray(x.options) ? x.options.slice(0, 100).map((s) => cleanLabel(s, 80)) : undefined,
      min: typeof x.min === "number" && Number.isFinite(x.min) ? x.min : undefined,
      max: typeof x.max === "number" && Number.isFinite(x.max) ? x.max : undefined,
      hint: x.hint == null ? undefined : cleanLabel(x.hint, 200),
    }));
  return { title: o.title == null ? undefined : cleanLabel(o.title, 120), text: str(o.text, 2000), ok: o.ok == null ? undefined : cleanLabel(o.ok, 30), fields };
}

// ---- tool previews -----------------------------------------------------------------

export type Point3 = [number, number, number];
const COORD_MAX = 1e9;

/** One point: exactly three finite numbers, clamped to the model's range. Anything else is dropped (null). */
export function cleanPoint(p: unknown): Point3 | null {
  if (!Array.isArray(p) || p.length !== 3) return null;
  const out: number[] = [];
  for (let i = 0; i < 3; i++) {
    const v = p[i];
    if (typeof v !== "number" || !Number.isFinite(v)) return null;
    out.push(Math.max(-COORD_MAX, Math.min(COORD_MAX, v)));
  }
  return out as Point3;
}

/** A tool's preview lines: at most `max` segments, each exactly two good points; the rest are skipped. */
export function cleanPreview(lines: unknown, max = 5000): Point3[][] {
  if (!Array.isArray(lines)) return [];
  const out: Point3[][] = [];
  const n = Math.min(lines.length, max * 2);
  for (let i = 0; i < n && out.length < max; i++) {
    const l = lines[i];
    if (!Array.isArray(l) || l.length !== 2) continue;
    const a = cleanPoint(l[0]);
    const b = cleanPoint(l[1]);
    if (a && b) out.push([a, b]);
  }
  return out;
}

// ---- names and menus --------------------------------------------------------------

/** Control, bidirectional and other invisible format characters: they can make a name read as something else. */
// (the same set as the engine's is_format_char and is_invisible_filler: soft hyphen, grapheme joiner, Hangul and
// Khmer fillers, Mongolian selectors, zero-width and bidi marks, Braille blank, variation selectors, specials, tags)
const INVISIBLE = "\\u00ad\\u034f\\u061c\\u115f\\u1160\\u17b4\\u17b5\\u180b-\\u180e\\u200b-\\u200f\\u2028-\\u202e\\u2060-\\u206f\\u2800\\u3164\\ufe00-\\ufe0f\\ufeff\\uffa0\\ufff9-\\ufffc\\u{1d173}-\\u{1d17a}\\u{e0000}-\\u{e007f}\\u{e0100}-\\u{e01ef}";
const HIDDEN = new RegExp(`[\\u0000-\\u001f\\u007f-\\u009f${INVISIBLE}]`, "gu");
/** the same, less line breaks and tabs: for body text that may run over several lines */
const HIDDEN_IN_TEXT = new RegExp(`[\\u0000-\\u0008\\u000b-\\u001f\\u007f-\\u009f${INVISIBLE}]`, "gu");

/** A name or label as it may be shown: no control or bidi characters, one line, cut to `max`. */
export function cleanLabel(v: unknown, max = 80): string {
  let s: string;
  try {
    s = typeof v === "string" ? v : v == null ? "" : String(v);
  } catch {
    return "";
  }
  return s.replace(/[\t\n\r]/g, " ").replace(HIDDEN, "").replace(/\s+/g, " ").trim().slice(0, max);
}

/** Most menu items one extension may add, and the longest label. */
export const MAX_MENU_ITEMS = 50;
export const MAX_LABEL = 80;

/**
 * A menu a Ruby extension built, cut down: only the first MAX_MENU_ITEMS text
 * entries, each a clean label. `dropped` is how many were left out.
 */
export function cleanMenu(m: unknown): { items: string[]; dropped: number; indexes: number[] } {
  if (!Array.isArray(m)) return { items: [], dropped: 0, indexes: [] };
  const items: string[] = [];
  const indexes: number[] = [];
  for (let i = 0; i < m.length && items.length < MAX_MENU_ITEMS; i++) {
    items.push(cleanLabel(m[i], MAX_LABEL) || "(unnamed)");
    indexes.push(i);
  }
  return { items, dropped: Math.max(0, m.length - items.length), indexes };
}

const SHOWN_AT = new Map<string, number[]>();
/** A plugin may show at most `max` messages (toasts and windows) in `windowMs`; the rest are dropped. */
export function displayAllowed(id: string, now = Date.now(), max = 20, windowMs = 5000): boolean {
  const recent = (SHOWN_AT.get(id) ?? []).filter((t) => now - t < windowMs);
  const ok = recent.length < max;
  if (ok) recent.push(now);
  SHOWN_AT.set(id, recent);
  if (SHOWN_AT.size > 200) SHOWN_AT.clear();
  return ok;
}
