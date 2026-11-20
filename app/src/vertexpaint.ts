// Vertex Paint: the brush's settings and how a drag becomes one `vertex_paint` command.
// The engine does the painting (core/src/vertex_paint.rs); this file only turns pointer samples into a stroke.

import type { Scene } from "./engine";

export type Falloff = "smooth" | "linear" | "hard";
export type P3 = [number, number, number];

export interface VPaintSettings {
  /** brush radius in metres; 0 picks one from the size of the model */
  radius: number;
  /** 0..1: how far a corner under the middle of the brush moves toward the colour */
  strength: number;
  falloff: Falloff;
}

export const DEFAULT_VPAINT: VPaintSettings = { radius: 0, strength: 0.6, falloff: "smooth" };

export const MIN_RADIUS = 0.001;
export const MAX_RADIUS = 1000;
/** the most points one stroke sends (a long drag is thinned to this) */
export const MAX_STROKE_POINTS = 1500;
/** a new point is added once the brush has moved this far, as a share of its radius */
export const SPACING = 0.3;

const diagonals = new WeakMap<object, number>();

/** The diagonal of the scene's bounding box (metres), worked out once per scene. */
export function sceneDiagonal(s: Pick<Scene, "positions">): number {
  const hit = diagonals.get(s);
  if (hit != null) return hit;
  const p = s.positions;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i + 2 < p.length; i += 3) {
    for (let k = 0; k < 3; k++) {
      if (p[i + k] < lo[k]) lo[k] = p[i + k];
      if (p[i + k] > hi[k]) hi[k] = p[i + k];
    }
  }
  const d = p.length >= 3 ? Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) : 0;
  diagonals.set(s, d);
  return d;
}

/** A brush radius for a model whose bounding box has this diagonal (metres): about 6% of it, never absurd. */
export function autoRadius(diagonal: number): number {
  if (!(diagonal > 0) || !Number.isFinite(diagonal)) return 0.1;
  return clampRadius(diagonal * 0.06);
}

export function clampRadius(r: number): number {
  if (!Number.isFinite(r)) return 0.1;
  return Math.min(MAX_RADIUS, Math.max(MIN_RADIUS, r));
}

export function clampStrength(s: number): number {
  if (!Number.isFinite(s)) return DEFAULT_VPAINT.strength;
  return Math.min(1, Math.max(0.01, s));
}

/** The radius in use: the setting, or the automatic one while it is 0. */
export function effectiveRadius(s: VPaintSettings, diagonal: number): number {
  return s.radius > 0 ? clampRadius(s.radius) : autoRadius(diagonal);
}

/** [ makes the brush smaller, ] bigger (a quarter each press). */
export function stepRadius(current: number, dir: -1 | 1): number {
  return clampRadius(dir > 0 ? current * 1.25 : current / 1.25);
}

const dist = (a: P3, b: P3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/**
 * Add a sample to a stroke. The brush's centre moves in a straight line from the last point to the new one, so points
 * are laid along it every `radius * SPACING`: a fast drag still paints a continuous line. Returns the points added.
 */
export function extendStroke(points: P3[], to: P3, radius: number): P3[] {
  const step = Math.max(radius * SPACING, 1e-9);
  const last = points[points.length - 1];
  if (!last) {
    points.push(to);
    return [to];
  }
  const d = dist(last, to);
  if (d < step) return [];
  const n = Math.floor(d / step);
  const added: P3[] = [];
  for (let i = 1; i <= n; i++) {
    const t = (i * step) / d;
    added.push([last[0] + (to[0] - last[0]) * t, last[1] + (to[1] - last[1]) * t, last[2] + (to[2] - last[2]) * t]);
  }
  points.push(...added);
  return added;
}

/** At most `max` points, spread evenly along the stroke (the first and the last are always kept). */
export function thin(points: P3[], max = MAX_STROKE_POINTS): P3[] {
  if (points.length <= max) return points;
  const out: P3[] = [];
  for (let i = 0; i < max; i++) out.push(points[Math.round((i * (points.length - 1)) / (max - 1))]);
  return out;
}

const r6 = (n: number) => Math.round(n * 1e6) / 1e6;

/**
 * The command for a finished stroke. `targets` are the faces or groups or components the brush went over; Alt (`erase`)
 * paints back toward no tint. Returns null when there is nothing to send.
 */
export function strokeCommand(opts: {
  targets: number[];
  points: P3[];
  color: [number, number, number];
  radius: number;
  settings: VPaintSettings;
  erase: boolean;
}): Record<string, unknown> | null {
  const targets = [...new Set(opts.targets)];
  if (!targets.length || !opts.points.length) return null;
  const c: Record<string, unknown> = {
    cmd: "vertex_paint",
    faces: targets,
    points: thin(opts.points).map((p) => [r6(p[0]), r6(p[1]), r6(p[2])]),
    radius: r6(clampRadius(opts.radius)),
    strength: r6(clampStrength(opts.settings.strength)),
    falloff: opts.settings.falloff,
  };
  if (opts.erase) c.erase = true;
  else c.color = opts.color.map((x) => r6(Math.min(1, Math.max(0, x))));
  return c;
}

/** A circle of `n` points around `center`, lying in the plane with this normal (the brush cursor drawn on a surface). */
export function ringPoints(center: P3, normal: P3, radius: number, n = 40): P3[] {
  const len = Math.hypot(...normal) || 1;
  const nz: P3 = [normal[0] / len, normal[1] / len, normal[2] / len];
  // any axis not parallel to the normal gives the first in-plane direction
  const ref: P3 = Math.abs(nz[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
  const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const u0 = cross(nz, ref);
  const ul = Math.hypot(...u0) || 1;
  const u: P3 = [u0[0] / ul, u0[1] / ul, u0[2] / ul];
  const v = cross(nz, u);
  const out: P3[] = [];
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2;
    const [c, s] = [Math.cos(a) * radius, Math.sin(a) * radius];
    out.push([center[0] + u[0] * c + v[0] * s, center[1] + u[1] * c + v[1] * s, center[2] + u[2] * c + v[2] * s]);
  }
  return out;
}
