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
  paper: { clear: 0xf6f1e4, line: 0x2b2a27, face: 0xfbf8f0, grid: false },
  blueprint: { clear: 0x1d4f8c, line: 0xeaf2ff, face: 0x1d4f8c, grid: false },
  night: { clear: 0x16181f, line: 0xd8dbe6, face: 0x2a2d38, grid: false },
};

/**
 * The Classic look's sky: a unit dome that follows the camera, pale blue at the horizon deepening
 * overhead, a grey-green below it. Drawn first, never tested for depth, never picked.
 */
export function skyDome(): THREE.Mesh {
  const g = new THREE.SphereGeometry(1, 32, 24);
  g.rotateX(Math.PI / 2); // poles on the Z axis: up is +Z
  const pos = g.getAttribute("position");
  const col: number[] = [];
  const horizon = new THREE.Color(0xeaf3fa);
  const zenith = new THREE.Color(0x8fbbe6);
  const ground = new THREE.Color(0xbcc3b1);
  const c = new THREE.Color();
  for (let i = 0; i < pos.count; i++) {
    const z = pos.getZ(i);
    if (z >= 0) c.copy(horizon).lerp(zenith, Math.pow(z, 0.7));
    else c.copy(ground).lerp(horizon, Math.max(0, 1 + z * 6)); // a soft haze at the horizon, then ground
    col.push(c.r, c.g, c.b);
  }
  g.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
  const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.BackSide, depthTest: false, depthWrite: false, fog: false }));
  m.renderOrder = -1000;
  m.frustumCulled = false;
  m.raycast = () => {};
  m.visible = false;
  return m;
}
