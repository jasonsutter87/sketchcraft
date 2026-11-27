import * as THREE from "three";
import { type Backdrop } from "./types";

/** A pleasant color for a tag that hasn't been given one (stable per name). */
export function autoTagColor(tag: string): [number, number, number] {
  let h = 0;
  for (const ch of tag) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  const hue = (h % 360) / 360;
  // soft, distinct: HSL with s .5, l .68
  const f = (n: number) => {
    const k = (n + hue * 12) % 12;
    return 0.68 - 0.5 * Math.min(0.68, 1 - 0.68) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  return [f(0), f(8), f(4)];
}

export const ACCENT = 0x6b5cff;
const EDGE_COLOR = 0x23252d;

/** Clear color, line color, and whether the ground grid shows, per background. */
export const BACKGROUNDS: Record<Backdrop, { clear: number; line: number; face: number; grid: boolean }> = {
  sky: { clear: 0xd6e6f4, line: 0x1c1d22, face: 0xffffff, grid: false },
  default: { clear: 0xf3f4f8, line: EDGE_COLOR, face: 0xffffff, grid: true },
  grey: { clear: 0xcbcac5, line: 0x000000, face: 0xffffff, grid: false },
  daylight: { clear: 0xc6e4f3, line: 0x000000, face: 0xffffff, grid: false },
  paper: { clear: 0xf6f1e4, line: 0x2b2a27, face: 0xfbf8f0, grid: false },
  blueprint: { clear: 0x1d4f8c, line: 0xeaf2ff, face: 0x1d4f8c, grid: false },
  night: { clear: 0x16181f, line: 0xd8dbe6, face: 0x2a2d38, grid: false },
};

/**
 * The Classic look's sky: a unit dome that follows the camera, pale blue at the horizon deepening
 * overhead, a grey-green below it. Drawn first, never tested for depth, never picked.
 */
/** the two skies the dome can wear: Classic's soft one, and Daylight's deeper blue over a flat grey ground */
const SKIES = {
  sky: { horizon: 0xeaf3fa, zenith: 0x8fbbe6, ground: 0xbcc3b1, haze: 6, curve: 0.7 },
  daylight: { horizon: 0xc6e4f3, zenith: 0x63b2de, ground: 0xbfbfc5, haze: 400, curve: 0.55 },
} as const;
export type SkyKind = keyof typeof SKIES;
export const hasSky = (b: string | undefined): b is SkyKind => b === "sky" || b === "daylight";

function skyColors(g: THREE.BufferGeometry, kind: SkyKind): number[] {
  const k = SKIES[kind];
  const pos = g.getAttribute("position");
  const col: number[] = [];
  const horizon = new THREE.Color(k.horizon);
  const zenith = new THREE.Color(k.zenith);
  const ground = new THREE.Color(k.ground);
  const c = new THREE.Color();
  for (let i = 0; i < pos.count; i++) {
    const z = pos.getZ(i);
    // a crisp horizon: a triangle that dips below it is all ground (the dome is unindexed, so each
    // triangle has its own corners); Classic's soft sky blends across it instead
    const tri = i - (i % 3);
    if (k.haze > 100 && i + 2 - (i % 3) < pos.count && Math.min(pos.getZ(tri), pos.getZ(tri + 1), pos.getZ(tri + 2)) < -1e-6) c.copy(ground);
    else if (z >= 0) c.copy(horizon).lerp(zenith, Math.pow(z, k.curve));
    else c.copy(ground).lerp(horizon, Math.max(0, 1 + z * k.haze)); // a haze at the horizon, then ground
    col.push(c.r, c.g, c.b);
  }
  return col;
}

/** Repaint the dome for a sky (no-op when it already wears that one). */
export function tintSky(m: THREE.Mesh, kind: SkyKind) {
  if (m.userData.sky === kind) return;
  m.userData.sky = kind;
  (m.geometry as THREE.BufferGeometry).setAttribute("color", new THREE.Float32BufferAttribute(skyColors(m.geometry as THREE.BufferGeometry, kind), 3));
}

export function skyDome(): THREE.Mesh {
  // unindexed, so a sky with a crisp horizon can paint whole triangles
  const g = new THREE.SphereGeometry(1, 32, 24).toNonIndexed();
  g.rotateX(Math.PI / 2); // poles on the Z axis: up is +Z
  const col = skyColors(g, "sky");
  g.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
  const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.BackSide, depthTest: false, depthWrite: false, fog: false }));
  m.renderOrder = -1000;
  m.frustumCulled = false;
  m.raycast = () => {};
  m.visible = false;
  m.userData.sky = "sky";
  return m;
}
