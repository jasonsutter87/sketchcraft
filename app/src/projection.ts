// Projection mapping, the app's side: what the "Project texture" panel holds, and the engine command it turns into.
// Pure (no React), so the Vitest suite covers it. The math itself lives in the engine (core/src/projection.rs).

import type { MaterialInfo } from "./engine";
import type { Sel } from "./selection";

export type ProjMode = "planar" | "box" | "cylinder" | "sphere";
export const PROJ_MODES: ProjMode[] = ["planar", "box", "cylinder", "sphere"];

/** where the axis points: the world's up, its two horizontal axes, or (planar only) the way the camera looks */
export type AxisChoice = "z" | "x" | "y" | "view";

export interface ProjState {
  mode: ProjMode;
  material: string | null;
  /** size factor: 1 is the picture's own size (or once around), 2 is twice as big */
  scale: number;
  /** degrees */
  rotation: number;
  /** cover the selection once; null: the mode's own choice (cylinder and sphere fit, planar and box tile) */
  fit: boolean | null;
  axis: AxisChoice;
  /** the middle of the projection in metres; null: the middle of the selection */
  origin: [number, number, number] | null;
}

export const SCALE_MIN = 0.05;
export const SCALE_MAX = 50;

/** what a mode does unless told otherwise */
export const defaultFit = (m: ProjMode) => m === "cylinder" || m === "sphere";

export function initialState(mode: ProjMode, material: string | null): ProjState {
  // planar starts from the camera (the picture lands on what you are looking at); the others stand upright
  return { mode, material, scale: 1, rotation: 0, fit: null, axis: mode === "planar" ? "view" : "z", origin: null };
}

/** switching mode keeps what still means the same thing (scale, material, rotation) and resets the axis */
export function withMode(s: ProjState, mode: ProjMode): ProjState {
  return { ...s, mode, fit: null, axis: mode === "planar" ? "view" : s.axis === "view" ? "z" : s.axis };
}

export const clampScale = (v: number) => (Number.isFinite(v) ? Math.min(SCALE_MAX, Math.max(SCALE_MIN, v)) : 1);

/** any number of degrees, as -180 to 180 */
export function wrapDegrees(v: number): number {
  if (!Number.isFinite(v)) return 0;
  const w = ((((v + 180) % 360) + 360) % 360) - 180;
  return w === -180 && v > 0 ? 180 : w;
}

/** the faces, groups and placed components a projection can go on: a projection needs at least one */
export function targetIds(sel: Sel): number[] {
  return [...sel.faces, ...sel.groups, ...sel.instances];
}

/** the material the panel starts on: the first selected face's picture, else the first picture in the model */
export function pickMaterial(materials: MaterialInfo[], faceMaterials: (string | null)[], chosen: string | null): string | null {
  const textured = materials.filter((m) => m.texture).map((m) => m.name);
  if (chosen && textured.includes(chosen)) return chosen;
  const used = faceMaterials.find((m) => m && textured.includes(m));
  return used ?? textured[0] ?? null;
}

/** box, cylinder and sphere: the axis the shape stands on */
const AXES: Record<Exclude<AxisChoice, "view">, [number, number, number]> = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };
/** planar: the way the projector shines (from above, from the front, from the right) */
const PLANAR: Record<Exclude<AxisChoice, "view">, [number, number, number]> = { z: [0, 0, -1], y: [0, 1, 0], x: [-1, 0, 0] };

type Cam = { eye: number[]; target: number[] };

/** the unit direction the camera looks along */
export function viewDirection(cam: Cam): [number, number, number] | null {
  const d = [cam.target[0] - cam.eye[0], cam.target[1] - cam.eye[1], cam.target[2] - cam.eye[2]];
  const n = Math.hypot(d[0], d[1], d[2]);
  return n > 1e-9 ? [d[0] / n, d[1] / n, d[2] / n] : null;
}

const r6 = (v: number) => Math.round(v * 1e6) / 1e6;

/** the engine command for this panel state on these ids (null: nothing to do yet) */
export function projectCommand(s: ProjState, ids: number[], cam?: Cam): Record<string, unknown> | null {
  if (!ids.length) return null;
  const c: Record<string, unknown> = { cmd: "texture_project", faces: ids, mode: s.mode, scale: r6(clampScale(s.scale)), rotation: r6(wrapDegrees(s.rotation)) };
  if (s.material) c.material = s.material;
  if (s.fit != null) c.fit = s.fit;
  if (s.origin) c.origin = s.origin.map(r6);
  if (s.axis === "view") {
    // only planar has a view axis: the engine's default (the biggest face) stands in when the camera isn't known
    const v = cam && viewDirection(cam);
    if (v) c.axis = v.map(r6);
  } else {
    c.axis = (s.mode === "planar" ? PLANAR : AXES)[s.axis];
  }
  return c;
}

/** the locale id of the one-line help for a mode, and for the step the person is on */
export const modeHintId = (m: ProjMode) => `proj.hint.${m}`;
export const fitLabelId = (m: ProjMode) => (defaultFit(m) ? "proj.fit.around" : "proj.fit.selection");

/** parse a typed number; null when it isn't one */
export function parseNumber(text: string): number | null {
  const v = Number(text.trim().replace(",", "."));
  return text.trim() !== "" && Number.isFinite(v) ? v : null;
}
