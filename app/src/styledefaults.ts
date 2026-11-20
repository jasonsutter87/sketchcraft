// The view style presets, and which style a session starts with.
//
// "Classic" is the genre look of a drafting-and-massing app: a light blue sky over a grey-green
// ground, a thick dark profile on silhouettes, thin dark edges elsewhere, no line weights, faces
// off-white in front and blue-grey behind (the engine already hands out those two face colours).
import type { Styles } from "./viewport/types";

export type StylePreset = { name: string; styles: Partial<Styles> };

/** The Classic look: the default for a first launch and for a new model. */
export const CLASSIC_STYLE: Partial<Styles> = { face: "textured", background: "sky", profiles: true, sketchy: false, extensions: false, ao: false, edges: true, lineWeights: false };

/** One-click looks, like a style collection. */
export const STYLE_PRESETS: StylePreset[] = [
  { name: "Classic", styles: CLASSIC_STYLE },
  { name: "Default", styles: { face: "textured", background: "default", profiles: false, sketchy: false, extensions: false, ao: false, edges: true, lineWeights: true } },
  { name: "Architectural", styles: { face: "shaded", background: "default", profiles: true, sketchy: false, extensions: false, ao: true, edges: true, lineWeights: true } },
  { name: "Sketchy on paper", styles: { face: "hiddenline", background: "paper", profiles: true, sketchy: true, extensions: true, ao: false, edges: true, lineWeights: true } },
  { name: "Hidden line", styles: { face: "hiddenline", background: "default", profiles: true, sketchy: false, extensions: false, ao: false, edges: true, lineWeights: true } },
  { name: "Blueprint", styles: { face: "hiddenline", background: "blueprint", profiles: false, sketchy: false, extensions: true, ao: false, edges: true, lineWeights: true } },
  { name: "Clay", styles: { face: "monochrome", background: "default", profiles: false, sketchy: false, extensions: false, ao: true, edges: false, lineWeights: true } },
  { name: "Night", styles: { face: "shaded", background: "night", profiles: true, sketchy: false, extensions: false, ao: false, edges: true, lineWeights: true } },
];

export const STYLE_KEY = "sketchcraft.style";

const FACES = ["textured", "shaded", "monochrome", "hiddenline", "wireframe", "physical"];
const BACKGROUNDS = ["sky", "default", "paper", "blueprint", "night"];
const FLAGS = ["profiles", "sketchy", "extensions", "ao", "edges", "lineWeights"] as const;

/** The look a user chose, read back from storage text: only known, well-formed keys survive. */
export function readChosenStyle(raw: string | null | undefined): Partial<Styles> | null {
  if (!raw) return null;
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  const out: Partial<Styles> = {};
  if (typeof o.face === "string" && FACES.includes(o.face)) out.face = o.face as Styles["face"];
  if (typeof o.background === "string" && BACKGROUNDS.includes(o.background)) out.background = o.background as Styles["background"];
  for (const k of FLAGS) if (typeof o[k] === "boolean") out[k] = o[k] as boolean;
  return Object.keys(out).length ? out : null;
}

/** The part of the style worth remembering between sessions. */
export function chosenStyle(s: Styles): Partial<Styles> {
  const out: Partial<Styles> = { face: s.face, background: s.background };
  for (const k of FLAGS) out[k] = k === "lineWeights" ? s[k] !== false : !!s[k];
  return out;
}

/** What a session starts with: the look the user last chose, else Classic. */
export function startingStyles(chosen: Partial<Styles> | null, help: boolean): Styles {
  return { xray: false, backEdges: false, shadows: false, parallel: false, ...CLASSIC_STYLE, ...chosen, edges: chosen?.edges ?? true, help } as Styles;
}

/**
 * The style once a model's saved scene is shown: a saved scene keeps its own style, a model that
 * saved none (a new model, or an older file) takes the current look as it is.
 */
export function styleForModel(current: Styles, saved: Partial<Styles> | null | undefined): Styles {
  return saved ? { ...current, ...saved } : current;
}
