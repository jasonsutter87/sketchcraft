// Texture Paint: the brush's settings, the maths that mirror the engine's (core/src/texpaint.rs) so a stroke can be shown
// while it is dragged, and how a finished drag becomes ONE `texture_paint_stroke` command. The engine paints the real
// picture; `Painter` only keeps a copy of the pixels to draw the stroke as it grows.

import type { Scene } from "./engine";
import { clampRadius, extendStroke, thin, type P3 } from "./vertexpaint";

export { extendStroke, thin, type P3 };

/** RGBA bytes that can back an ImageData */
export type Pixels = Uint8ClampedArray<ArrayBuffer>;

export interface TPaintSettings {
  /** brush radius in metres on the model; 0 picks one from the size of the model */
  radius: number;
  /** brush radius on the picture (the UV editor), in tiles of the picture's width */
  uvRadius: number;
  /** 0..1: 1 is a hard disc, 0 a fully soft brush */
  hardness: number;
  /** 0..1: how far a texel under the middle of the brush moves toward the colour */
  opacity: number;
}

export const DEFAULT_TPAINT: TPaintSettings = { radius: 0, uvRadius: 0.03, hardness: 0.5, opacity: 1 };

export const MIN_UV_RADIUS = 0.002;
export const MAX_UV_RADIUS = 0.5;

export function clampUvRadius(r: number): number {
  if (!Number.isFinite(r)) return DEFAULT_TPAINT.uvRadius;
  return Math.min(MAX_UV_RADIUS, Math.max(MIN_UV_RADIUS, r));
}

export function clamp01(v: number, fallback: number): number {
  return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : fallback;
}

/** [ makes the picture brush smaller, ] bigger (a quarter each press). */
export function stepUvRadius(current: number, dir: -1 | 1): number {
  return clampUvRadius(dir > 0 ? current * 1.25 : current / 1.25);
}

// ----- the engine's maths, mirrored -------------------------------------------------------------------------------

/** Pixels (x right, y down) a texture coordinate (tiles, v up) lands on. */
export function uvToPixel(uv: [number, number], w: number, h: number): [number, number] {
  return [uv[0] * w, (1 - uv[1]) * h];
}

export function pixelToUv(p: [number, number], w: number, h: number): [number, number] {
  return [p[0] / w, 1 - p[1] / h];
}

/** Brush weight 0..1 at distance `d` from the centre: full inside hardness * r, then a smooth fall to nothing at r. */
export function falloff(d: number, r: number, hardness: number): number {
  if (r <= 0 || d >= r) return 0;
  const inner = Math.min(1, Math.max(0, hardness)) * r;
  if (d <= inner) return 1;
  const t = Math.min(1, Math.max(0, (d - inner) / (r - inner)));
  return 1 - t * t * (3 - 2 * t);
}

const BLEED = 0.75;

type Px3 = [[number, number], [number, number], [number, number]];

function bary(t: Px3, q: [number, number]): [number, number, number] | null {
  const d = (t[1][1] - t[2][1]) * (t[0][0] - t[2][0]) + (t[2][0] - t[1][0]) * (t[0][1] - t[2][1]);
  if (Math.abs(d) < 1e-12) return null;
  const l0 = ((t[1][1] - t[2][1]) * (q[0] - t[2][0]) + (t[2][0] - t[1][0]) * (q[1] - t[2][1])) / d;
  const l1 = ((t[2][1] - t[0][1]) * (q[0] - t[2][0]) + (t[0][0] - t[2][0]) * (q[1] - t[2][1])) / d;
  return [l0, l1, 1 - l0 - l1];
}

function segDist(a: [number, number], b: [number, number], q: [number, number]): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 < 1e-18 ? 0 : Math.min(1, Math.max(0, ((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / l2));
  return Math.hypot(a[0] + dx * t - q[0], a[1] + dy * t - q[1]);
}

/** Texels of a pixel triangle inside the box lo..hi (clipped to the picture): centre inside or within 0.75 px. */
export function raster(t: Px3, w: number, h: number, lo: [number, number], hi: [number, number], f: (x: number, y: number, l: [number, number, number]) => void): void {
  const xs = [t[0][0], t[1][0], t[2][0]];
  const ys = [t[0][1], t[1][1], t[2][1]];
  const minx = Math.min(...xs);
  const maxx = Math.max(...xs);
  const miny = Math.min(...ys);
  const maxy = Math.max(...ys);
  if (![minx, maxx, miny, maxy].every(Number.isFinite)) return;
  const x0 = Math.max(0, Math.floor(Math.max(minx, lo[0]) - BLEED));
  const x1 = Math.min(w - 1, Math.ceil(Math.min(maxx, hi[0]) + BLEED));
  const y0 = Math.max(0, Math.floor(Math.max(miny, lo[1]) - BLEED));
  const y1 = Math.min(h - 1, Math.ceil(Math.min(maxy, hi[1]) + BLEED));
  for (let y = y0; y <= y1; y++) {
    for (let x = x0; x <= x1; x++) {
      const q: [number, number] = [x + 0.5, y + 0.5];
      let l = bary(t, q);
      if (!l) return;
      if (l.some((v) => v < 0)) {
        const d = Math.min(segDist(t[0], t[1], q), segDist(t[1], t[2], q), segDist(t[2], t[0], q));
        if (d > BLEED) continue;
        const c = l.map((v) => Math.max(0, v));
        const s = c[0] + c[1] + c[2];
        if (s <= 0) continue;
        l = [c[0] / s, c[1] / s, c[2] / s];
      }
      f(x, y, l);
    }
  }
}

/** A triangle of a textured face: corners in 3D, where they land on the picture, and what the dab maths need of it. */
export interface PaintTri {
  p: [number, number, number, number, number, number, number, number, number];
  px: Px3;
  lo: P3;
  hi: P3;
  n: P3;
  /** the most the map from the triangle's plane to the picture stretches (pixels per metre) */
  stretch: number;
}

function sub(a: P3, b: P3): P3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}
const dot = (a: P3, b: P3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: P3, b: P3): P3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = (a: P3) => Math.hypot(a[0], a[1], a[2]);

/** One triangle prepared for painting; null when it has no area. */
export function makeTri(p: PaintTri["p"], px: Px3): PaintTri | null {
  const a: P3 = [p[0], p[1], p[2]];
  const b: P3 = [p[3], p[4], p[5]];
  const c: P3 = [p[6], p[7], p[8]];
  const e1 = sub(b, a);
  const e2 = sub(c, a);
  const nn = cross(e1, e2);
  const nl = len(nn);
  if (nl < 1e-14) return null;
  const n: P3 = [nn[0] / nl, nn[1] / nl, nn[2] / nl];
  const l1 = len(e1);
  const ax: P3 = [e1[0] / l1, e1[1] / l1, e1[2] / l1];
  const bx = cross(n, ax);
  const m = [
    [l1, 0],
    [dot(e2, ax), dot(e2, bx)],
  ];
  const det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
  if (Math.abs(det) < 1e-18) return null;
  const dp = [
    [px[1][0] - px[0][0], px[1][1] - px[0][1]],
    [px[2][0] - px[0][0], px[2][1] - px[0][1]],
  ];
  const inv = [
    [m[1][1] / det, -m[0][1] / det],
    [-m[1][0] / det, m[0][0] / det],
  ];
  const j = [
    [0, 0],
    [0, 0],
  ];
  for (let r = 0; r < 2; r++) for (let k = 0; k < 2; k++) j[r][k] = dp[0][r] * inv[0][k] + dp[1][r] * inv[1][k];
  const pp = j[0][0] * j[0][0] + j[1][0] * j[1][0];
  const q = j[0][0] * j[0][1] + j[1][0] * j[1][1];
  const s = j[0][1] * j[0][1] + j[1][1] * j[1][1];
  const stretch = Math.sqrt(Math.max(0, (pp + s) / 2 + Math.sqrt(((pp - s) * (pp - s)) / 4 + q * q)));
  const lo: P3 = [Math.min(a[0], b[0], c[0]), Math.min(a[1], b[1], c[1]), Math.min(a[2], b[2], c[2])];
  const hi: P3 = [Math.max(a[0], b[0], c[0]), Math.max(a[1], b[1], c[1]), Math.max(a[2], b[2], c[2])];
  return { p, px, lo, hi, n, stretch };
}

/** The triangles of the scene that wear material `name`, ready for painting a `w` x `h` picture. */
export function trisOf(s: Pick<Scene, "positions" | "uvs" | "triTex" | "textures">, name: string, w: number, h: number): PaintTri[] {
  const out: PaintTri[] = [];
  const idx = s.textures.indexOf(name);
  if (idx < 0 || !s.triTex || !s.uvs?.length) return out;
  for (let t = 0; t < s.triTex.length; t++) {
    if (s.triTex[t] !== idx) continue;
    const p = Array.from(s.positions.slice(t * 9, t * 9 + 9)) as PaintTri["p"];
    const uv = (k: number): [number, number] => uvToPixel([s.uvs[t * 6 + k * 2], s.uvs[t * 6 + k * 2 + 1]], w, h);
    const tri = makeTri(p, [uv(0), uv(1), uv(2)]);
    if (tri) out.push(tri);
  }
  return out;
}

function distToBox(p: P3, lo: P3, hi: P3): number {
  const dx = Math.max(lo[0] - p[0], p[0] - hi[0], 0);
  const dy = Math.max(lo[1] - p[1], p[1] - hi[1], 0);
  const dz = Math.max(lo[2] - p[2], p[2] - hi[2], 0);
  return Math.hypot(dx, dy, dz);
}

function bary3(t: PaintTri, q: P3): [number, number, number] | null {
  const a: P3 = [t.p[0], t.p[1], t.p[2]];
  const v0 = sub([t.p[3], t.p[4], t.p[5]], a);
  const v1 = sub([t.p[6], t.p[7], t.p[8]], a);
  const v2 = sub(q, a);
  const d00 = dot(v0, v0);
  const d01 = dot(v0, v1);
  const d11 = dot(v1, v1);
  const d20 = dot(v2, v0);
  const d21 = dot(v2, v1);
  const den = d00 * d11 - d01 * d01;
  if (Math.abs(den) < 1e-18) return null;
  const b1 = (d11 * d20 - d01 * d21) / den;
  const b2 = (d00 * d21 - d01 * d20) / den;
  return [1 - b1 - b2, b1, b2];
}

/** What a stroke paints with. */
export interface TBrush {
  color: [number, number, number];
  opacity: number;
  hardness: number;
  erase: boolean;
  /** the colour erasing goes back to (0..1) */
  base: [number, number, number];
}

/**
 * A copy of a picture's pixels that a stroke is shown on. Dabs raise a per-texel weight (the strongest dab wins, never a
 * sum, as in the engine) and the shown pixel is the original moved toward the brush colour by that weight, so the
 * preview does not depend on the order the dabs came in.
 */
export class Painter {
  readonly w: number;
  readonly h: number;
  readonly orig: Pixels;
  readonly cur: Pixels;
  readonly weights: Float32Array;
  /** the box of texels changed since the last `takeDirty` */
  private dirty: [number, number, number, number] | null = null;
  readonly brush: TBrush;

  constructor(rgba: Pixels, w: number, h: number, brush: TBrush) {
    this.w = w;
    this.h = h;
    this.orig = rgba;
    this.cur = new Uint8ClampedArray(rgba);
    this.weights = new Float32Array(w * h);
    this.brush = brush;
  }

  private raise(x: number, y: number, wt: number) {
    const i = y * this.w + x;
    const f = Math.fround(wt);
    if (f <= this.weights[i]) return;
    this.weights[i] = f;
    const b = this.brush;
    const t = b.erase ? b.base : b.color;
    const k = Math.min(1, Math.max(0, f * b.opacity));
    for (let c = 0; c < 3; c++) {
      const o = this.orig[i * 4 + c];
      this.cur[i * 4 + c] = Math.round(o + (Math.min(1, Math.max(0, t[c])) * 255 - o) * k);
    }
    const d = this.dirty;
    if (!d) this.dirty = [x, y, x, y];
    else {
      if (x < d[0]) d[0] = x;
      if (y < d[1]) d[1] = y;
      if (x > d[2]) d[2] = x;
      if (y > d[3]) d[3] = y;
    }
  }

  /** The box (x, y, w, h) of texels changed since the last call. */
  takeDirty(): [number, number, number, number] | null {
    const d = this.dirty;
    this.dirty = null;
    return d ? [d[0], d[1], d[2] - d[0] + 1, d[3] - d[1] + 1] : null;
  }

  /** A dab at a point on the model (world metres, radius in metres) over the prepared triangles. */
  dab3D(tris: PaintTri[], q: P3, radius: number): void {
    const hard = this.brush.hardness;
    for (const t of tris) {
      if (distToBox(q, t.lo, t.hi) >= radius) continue;
      const a: P3 = [t.p[0], t.p[1], t.p[2]];
      const height = Math.abs(dot(sub(q, a), t.n));
      if (height >= radius) continue;
      const rho = Math.sqrt(radius * radius - height * height);
      const k = dot(sub(q, a), t.n);
      const foot: P3 = [q[0] - t.n[0] * k, q[1] - t.n[1] * k, q[2] - t.n[2] * k];
      const bc = bary3(t, foot);
      if (!bc) continue;
      const cx = bc[0] * t.px[0][0] + bc[1] * t.px[1][0] + bc[2] * t.px[2][0];
      const cy = bc[0] * t.px[0][1] + bc[1] * t.px[1][1] + bc[2] * t.px[2][1];
      const reach = rho * t.stretch + 1.5;
      raster(t.px, this.w, this.h, [cx - reach, cy - reach], [cx + reach, cy + reach], (x, y, l) => {
        const px = t.p[0] * l[0] + t.p[3] * l[1] + t.p[6] * l[2];
        const py = t.p[1] * l[0] + t.p[4] * l[1] + t.p[7] * l[2];
        const pz = t.p[2] * l[0] + t.p[5] * l[1] + t.p[8] * l[2];
        const wt = falloff(Math.hypot(px - q[0], py - q[1], pz - q[2]), radius, hard);
        if (wt > 0) this.raise(x, y, wt);
      });
    }
  }

  /** A dab straight on the picture: `uv` in tiles (v up), radius in tiles of the picture's width. */
  dab2D(uv: [number, number], radius: number): void {
    const r = radius * this.w;
    if (r <= 0) return;
    const c = uvToPixel(uv, this.w, this.h);
    const x0 = Math.max(0, Math.floor(c[0] - r));
    const x1 = Math.min(this.w - 1, Math.ceil(c[0] + r));
    const y0 = Math.max(0, Math.floor(c[1] - r));
    const y1 = Math.min(this.h - 1, Math.ceil(c[1] + r));
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        const wt = falloff(Math.hypot(x + 0.5 - c[0], y + 0.5 - c[1]), r, this.brush.hardness);
        if (wt > 0) this.raise(x, y, wt);
      }
    }
  }
}

// ----- the commands ------------------------------------------------------------------------------------------------

const r6 = (n: number) => Math.round(n * 1e6) / 1e6;
const col = (c: [number, number, number]) => c.map((x) => r6(Math.min(1, Math.max(0, x))));

/** The command for a finished stroke on the model; null when there is nothing to send. */
export function stroke3DCommand(o: { material: string; points: P3[]; radius: number; color: [number, number, number]; settings: TPaintSettings; erase: boolean }): Record<string, unknown> | null {
  if (!o.points.length) return null;
  const c: Record<string, unknown> = {
    cmd: "texture_paint_stroke",
    material: o.material,
    points: thin(o.points).map((p) => [r6(p[0]), r6(p[1]), r6(p[2])]),
    radius: r6(clampRadius(o.radius)),
    hardness: r6(clamp01(o.settings.hardness, 0.5)),
    opacity: r6(clamp01(o.settings.opacity, 1)),
  };
  if (o.erase) c.erase = true;
  else c.color = col(o.color);
  return c;
}

/** The command for a finished stroke on the picture (the UV editor). */
export function stroke2DCommand(o: { material: string; points: [number, number][]; radius: number; color: [number, number, number]; settings: TPaintSettings; erase: boolean }): Record<string, unknown> | null {
  if (!o.points.length) return null;
  const c: Record<string, unknown> = {
    cmd: "texture_paint_stroke",
    material: o.material,
    uv_points: thinUv(o.points).map((p) => [r6(p[0]), r6(p[1])]),
    uv_radius: r6(clampUvRadius(o.radius)),
    hardness: r6(clamp01(o.settings.hardness, 0.5)),
    opacity: r6(clamp01(o.settings.opacity, 1)),
  };
  if (o.erase) c.erase = true;
  else c.color = col(o.color);
  return c;
}

function thinUv(points: [number, number][], max = 1500): [number, number][] {
  if (points.length <= max) return points;
  return Array.from({ length: max }, (_, i) => points[Math.round((i * (points.length - 1)) / (max - 1))]);
}

/** Add a sample to a stroke drawn on the picture: points are laid along the line every `radius * 0.3` (tiles). */
export function extendUvStroke(points: [number, number][], to: [number, number], radius: number): [number, number][] {
  const step = Math.max(radius * 0.3, 1e-9);
  const last = points[points.length - 1];
  if (!last) {
    points.push(to);
    return [to];
  }
  const d = Math.hypot(to[0] - last[0], to[1] - last[1]);
  if (d < step) return [];
  const n = Math.floor(d / step);
  const added: [number, number][] = [];
  for (let i = 1; i <= n; i++) {
    const t = (i * step) / d;
    added.push([last[0] + (to[0] - last[0]) * t, last[1] + (to[1] - last[1]) * t]);
  }
  points.push(...added);
  return added;
}

// ----- picking up a colour ----------------------------------------------------------------------------------------

/** The texture coordinate (tiles, v up) under point `q` of scene triangle `tri`. */
export function uvAt(s: Pick<Scene, "positions" | "uvs">, tri: number, q: P3): [number, number] | null {
  if (!s.uvs?.length) return null;
  const p = Array.from(s.positions.slice(tri * 9, tri * 9 + 9)) as PaintTri["p"];
  const t = makeTri(p, [
    [0, 0],
    [1, 0],
    [0, 1],
  ]);
  if (!t) return null;
  const bc = bary3(t, q);
  if (!bc) return null;
  const u = (k: number) => bc[0] * s.uvs[tri * 6 + k] + bc[1] * s.uvs[tri * 6 + 2 + k] + bc[2] * s.uvs[tri * 6 + 4 + k];
  return [u(0), u(1)];
}

/** The colour (0..1) of a picture's pixels at a texture coordinate (tiles repeat). */
export function sampleRgba(rgba: ArrayLike<number>, w: number, h: number, uv: [number, number]): [number, number, number] {
  const fu = uv[0] - Math.floor(uv[0]);
  const fv = uv[1] - Math.floor(uv[1]);
  const x = Math.min(w - 1, Math.max(0, Math.floor(fu * w)));
  const y = Math.min(h - 1, Math.max(0, Math.floor((1 - fv) * h)));
  const i = (y * w + x) * 4;
  return [rgba[i] / 255, rgba[i + 1] / 255, rgba[i + 2] / 255];
}

/** Decode a data URL into pixels (browser only). */
export function decodePixels(data: string): Promise<{ rgba: Pixels; w: number; h: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      const g = c.getContext("2d", { willReadFrequently: true });
      if (!g) return reject(new Error("no canvas"));
      g.drawImage(img, 0, 0);
      resolve({ rgba: g.getImageData(0, 0, c.width, c.height).data, w: c.width, h: c.height });
    };
    img.onerror = () => reject(new Error("the picture could not be read"));
    img.src = data;
  });
}

/** The picture material the selection wears: the first selected face's (or a selected group's face's) that has a picture. */
export function paintMaterialOf(scene: Pick<Scene, "faces" | "materials">, sel: { faces: number[]; groups: number[] }): string | null {
  const textured = new Set(scene.materials.filter((m) => m.texture).map((m) => m.name));
  const groups = new Set(sel.groups);
  const faces = new Set(sel.faces);
  for (const f of scene.faces) {
    if (!f.material || !textured.has(f.material)) continue;
    if (faces.has(f.id) || (f.group != null && groups.has(f.group))) return f.material;
  }
  return null;
}
