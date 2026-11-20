// Skins: how the app looks. A skin is a set of color tokens (the CSS
// variables every panel uses), a corner radius, a font, and optionally a
// suggested model backdrop. Built-ins live here; your own are saved as
// .sketchskin files in ~/Documents/SketchCraft/Skins, so they can be shared.

import { skinValueOk } from "./skinvalue";

export interface Skin {
  id: string;
  name: string;
  /** one line about it, for the picker */
  blurb: string;
  /** dark skins get the night-friendly bits (inverted icons etc.) */
  dark: boolean;
  tokens: Record<SkinToken, string>;
  /** a backdrop for the model that suits the skin (applied when you pick it) */
  backdrop?: "sky" | "default" | "paper" | "blueprint" | "night";
  /** the layout it comes with when picked from the gallery: "classic" (tool
   * bars docked above the model, like SketchUp) or "floating" (SketchCraft's own) */
  layout?: "classic" | "floating";
  /** tool icons it swaps in: tool name -> an image (data: URL, SVG or bitmap) */
  icons?: Record<string, string>;
}

/** The icons the skin being worn swaps in (read by the tool bar). */
let wornIcons: Record<string, string> = {};
export const skinIcon = (tool: string): string | undefined => wornIcons[tool];

/** An image data URL a skin may carry as an icon: SVG, PNG, JPEG or WebP, up to 256 KB.
 * (Shown with <img>, so an SVG can't run anything.) */
export function iconOk(v: unknown): v is string {
  return typeof v === "string" && v.length <= 350_000 && /^data:image\/(svg\+xml|png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(v);
}

export const TOKENS = ["bg", "panel", "line", "line-strong", "text", "muted", "faint", "accent", "accent-soft", "accent-ink", "danger", "glass", "radius", "font"] as const;
export type SkinToken = (typeof TOKENS)[number];

/** what each token paints, for the skin editor */
export const TOKEN_LABELS: Record<SkinToken, string> = {
  bg: "Background",
  panel: "Panels",
  line: "Dividers",
  "line-strong": "Borders",
  text: "Text",
  muted: "Quiet text",
  faint: "Faint text",
  accent: "Accent",
  "accent-soft": "Accent wash",
  "accent-ink": "Accent text",
  danger: "Danger",
  glass: "Floating bars",
  radius: "Corner radius",
  font: "Font",
};

const SYSTEM = `-apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", system-ui, sans-serif`;
const MONO = `"SF Mono", Menlo, Monaco, Consolas, monospace`;

const BRICOLAGE = `"Bricolage Grotesque Variable", "Bricolage Grotesque", "Avenir Next", ${SYSTEM}`;

export const SKINS: Skin[] = [
  {
    id: "classic",
    name: "Classic",
    blurb: "Neutral greys, a blue accent and docked tool bars: at home if you know SketchUp. The default.",
    dark: false,
    tokens: { bg: "#ececec", panel: "#f7f7f7", line: "#dcdcdc", "line-strong": "#c2c2c2", text: "#1f1f1f", muted: "#575757", faint: "#999999", accent: "#1e6fd9", "accent-soft": "#e2edfb", "accent-ink": "#1557ad", danger: "#d23c3c", glass: "rgba(247,247,247,0.97)", radius: "4px", font: SYSTEM },
    backdrop: "sky",
    layout: "classic",
  },
  {
    id: "workshop",
    name: "Workshop",
    blurb: "SketchCraft's own: drafting paper, ink and beaver violet, with floating tool bars.",
    dark: false,
    tokens: { bg: "#f6f1e4", panel: "#fffdf7", line: "#e9e1cc", "line-strong": "#d8cfb8", text: "#15131c", muted: "#4b4658", faint: "#958d7a", accent: "#6f4df0", "accent-soft": "#ece6ff", "accent-ink": "#4a2fc4", danger: "#c8372d", glass: "rgba(255,253,247,0.95)", radius: "10px", font: BRICOLAGE },
    layout: "floating",
  },
  {
    id: "studio",
    name: "Studio",
    blurb: "Light, cool and roomy.",
    dark: false,
    tokens: { bg: "#f3f4f8", panel: "#ffffff", line: "#e7e8ef", "line-strong": "#d9dbe5", text: "#1d1e24", muted: "#6c6f7d", faint: "#a3a6b4", accent: "#6b5cff", "accent-soft": "#efedff", "accent-ink": "#4a3df0", danger: "#e5484d", glass: "rgba(255,255,255,0.94)", radius: "10px", font: SYSTEM },
    layout: "floating",
  },
  {
    id: "midnight",
    name: "Midnight",
    blurb: "Dark panels, easy on the eyes at night.",
    dark: true,
    tokens: { bg: "#14151a", panel: "#1c1d23", line: "#2a2c34", "line-strong": "#363945", text: "#e8e9ee", muted: "#9a9dab", faint: "#5f6270", accent: "#8b7fff", "accent-soft": "#2a2648", "accent-ink": "#b4abff", danger: "#ff6369", glass: "rgba(28,29,35,0.94)", radius: "10px", font: SYSTEM },
    layout: "floating",
    backdrop: "night",
  },
  {
    id: "blueprint",
    name: "Blueprint",
    blurb: "Drafting-room blue, white linework, mono type.",
    dark: true,
    tokens: { bg: "#173a6b", panel: "#1e4a85", line: "#2f5f9e", "line-strong": "#4a78b5", text: "#f2f6ff", muted: "#b9cbe8", faint: "#7f9cc8", accent: "#ffffff", "accent-soft": "#2b5a98", "accent-ink": "#ffffff", danger: "#ff8a8a", glass: "rgba(30,74,133,0.95)", radius: "2px", font: MONO },
    layout: "floating",
    backdrop: "blueprint",
  },
  {
    id: "drafting",
    name: "Drafting Table",
    blurb: "Warm paper and ink, square corners.",
    dark: false,
    tokens: { bg: "#ece6d8", panel: "#f8f4ea", line: "#ddd4c0", "line-strong": "#c9bea6", text: "#2b2721", muted: "#6f6656", faint: "#a89e8a", accent: "#b5532f", "accent-soft": "#f3e2d6", "accent-ink": "#8e3d1f", danger: "#b3261e", glass: "rgba(248,244,234,0.95)", radius: "3px", font: `Georgia, "Iowan Old Style", serif` },
    layout: "floating",
    backdrop: "paper",
  },
  {
    id: "cassette",
    name: "Cassette Deck",
    blurb: "Brushed metal, bevelled buttons and a green LCD. Turn it up.",
    dark: true,
    tokens: { bg: "#23262d", panel: "#2e323b", line: "#1a1c21", "line-strong": "#4a5060", text: "#d8dde6", muted: "#9aa3b2", faint: "#6a7180", accent: "#39ff6a", "accent-soft": "#1d3a25", "accent-ink": "#39ff6a", danger: "#ff5050", glass: "rgba(46,50,59,0.97)", radius: "0px", font: `"Tahoma", "Verdana", ${SYSTEM}` },
    layout: "floating",
    backdrop: "night",
  },
  {
    id: "synthwave",
    name: "Synthwave",
    blurb: "Midnight purple with neon pink and cyan.",
    dark: true,
    tokens: { bg: "#160b2e", panel: "#20123f", line: "#33205c", "line-strong": "#4b3080", text: "#f6e9ff", muted: "#c2a8e8", faint: "#7d64a8", accent: "#ff3fb4", "accent-soft": "#3d1550", "accent-ink": "#5ef2ff", danger: "#ff5c7a", glass: "rgba(32,18,63,0.95)", radius: "12px", font: SYSTEM },
    layout: "floating",
    backdrop: "night",
  },
  {
    id: "contrast",
    name: "High Contrast",
    blurb: "Black on white, strong borders, nothing faint.",
    dark: false,
    tokens: { bg: "#ffffff", panel: "#ffffff", line: "#000000", "line-strong": "#000000", text: "#000000", muted: "#1a1a1a", faint: "#333333", accent: "#0040ff", "accent-soft": "#dfe6ff", "accent-ink": "#0030c0", danger: "#c00000", glass: "rgba(255,255,255,1)", radius: "4px", font: SYSTEM },
    layout: "floating",
  },
];

/** Put a skin on the page. */
export function applySkin(s: Skin) {
  const root = document.documentElement;
  for (const k of TOKENS) {
    let v = s.tokens[k];
    // whatever the skin came from (a file, saved settings), only plain colors, lengths and fonts get on the page
    if (!skinValueOk(k, v)) v = SKINS[0].tokens[k];
    // skins saved before the font was bundled name it without "Variable"
    if (k === "font" && v.includes('"Bricolage Grotesque"') && !v.includes("Bricolage Grotesque Variable")) v = v.replace('"Bricolage Grotesque"', '"Bricolage Grotesque Variable", "Bricolage Grotesque"');
    root.style.setProperty(`--${k}`, v);
  }
  wornIcons = { ...(s.icons ?? {}) };
  root.dataset.skin = s.id;
  root.dataset.dark = s.dark ? "1" : "0";
  root.style.colorScheme = s.dark ? "dark" : "light";
}

/** Is this a usable skin (from a file someone shared)? Fills gaps from Studio. */
export function readSkin(raw: unknown, fallbackName: string): Skin | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Partial<Skin> & { tokens?: Record<string, unknown> };
  if (!r.tokens || typeof r.tokens !== "object") return null;
  const base = SKINS[0].tokens;
  const tokens = { ...base };
  for (const k of TOKENS) {
    const v = r.tokens[k];
    // each token takes a color, a length or a font list in plain form: nothing that could load anything
    if (skinValueOk(k, v)) tokens[k] = v;
  }
  const name = typeof r.name === "string" && r.name.trim() ? r.name.trim().slice(0, 40) : fallbackName;
  return {
    id: `file:${name}`,
    name,
    blurb: typeof r.blurb === "string" ? r.blurb.slice(0, 120) : "Your theme",
    dark: typeof r.dark === "boolean" ? r.dark : isDark(tokens.panel),
    tokens,
    backdrop: ["sky", "default", "paper", "blueprint", "night"].includes(r.backdrop as string) ? r.backdrop : undefined,
    icons: readIcons((r as { icons?: unknown }).icons),
  };
}

function readIcons(raw: unknown): Record<string, string> | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) if (/^[a-z][a-z0-9_]{0,30}$/i.test(k) && iconOk(v)) out[k] = v;
  return Object.keys(out).length ? out : undefined;
}

/** Rough lightness of a #rrggbb color (dark panels → dark skin). */
export function isDark(color: string) {
  const m = /^#([0-9a-f]{6})$/i.exec(color.trim());
  if (!m) return false;
  const n = parseInt(m[1], 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b < 128;
}

export const store = {
  get(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(key: string, v: string) {
    try {
      localStorage.setItem(key, v);
    } catch {
      /* private window: just not remembered */
    }
  },
};

/**
 * Saved settings read back over their defaults: a stored value counts only if it has the same
 * kind as the default (a finite number, a string, a boolean) and, where `allowed` lists the
 * choices, is one of them. Anything else (a hand edit, another version) falls back to the default.
 */
export function savedSettings<T extends object>(key: string, defaults: T, allowed: Partial<Record<keyof T, readonly unknown[]>> = {}): T {
  const out = { ...defaults };
  let raw: unknown;
  try {
    raw = JSON.parse(store.get(key) ?? "null");
  } catch {
    return out;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return out;
  for (const k of Object.keys(defaults) as (keyof T)[]) {
    const v = (raw as Record<string, unknown>)[k as string];
    const ok = typeof v === typeof defaults[k] && (typeof v !== "number" || Number.isFinite(v)) && (allowed[k] ?? [v]).includes(v);
    if (ok) out[k] = v as T[keyof T];
  }
  return out;
}
