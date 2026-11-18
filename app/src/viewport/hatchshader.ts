// The on-screen hatch of a section cap (design §3.3, "On screen: a fragment shader that computes distance to each
// line family"). The engine sends each hatched tag's cap triangles with its pattern placed on the cut plane (the
// `section` answer's `hatches`: families in plane millimetres, as on a 1:50 drawing), and this paints them: paper
// white, with every family's lines at the hatch's true weight in screen pixels (weight_mm x PX_PER_MM, as edges and
// outlines are), dashes and dots included, and no segment cap: the cost is per pixel, not per line.
//
// `hatchInk` is the same math in TypeScript (what the shader computes for one point), for tests.
import * as THREE from "three";
import { PX_PER_MM } from "./linepens";

/** A pattern family placed on the plane: angle in degrees, base point, step to the next line, dashes (mm). */
export type Family = { angle: number; base: [number, number]; offset: [number, number]; dash: number[] };
/** One hatched tag's cap, as the engine sends it. */
export type CapHatch = {
  tag: string;
  color: string;
  weight_mm: number;
  fill: number[];
  families: Family[];
  /** the plane's frame: its origin and axes (world metres); plane mm = (p - origin) . u * 1000, . v * 1000 */
  origin: [number, number, number];
  u: [number, number, number];
  v: [number, number, number];
};

/** The most families and dashes per family the shader draws (more: the engine's lines are drawn instead). */
export const MAX_FAMILIES = 16;
export const MAX_DASHES = 16;
export const PAPER = 0xf6f5f0;

/** Can the shader draw this pattern? */
export function shaderCan(fams: Family[]): boolean {
  return fams.length > 0 && fams.length <= MAX_FAMILIES && fams.every((f) => f.dash.length <= MAX_DASHES && [f.angle, ...f.base, ...f.offset, ...f.dash].every(Number.isFinite));
}

type Packed = { a: THREE.Vector4[]; b: THREE.Vector4[]; d: number[]; k: number[]; n: number };

/** Families as the shader's uniforms: (cos, sin, spacing, period), (base, step), the dashes and their count, and how many families. */
export function pack(fams: Family[]): Packed {
  const a: THREE.Vector4[] = [];
  const b: THREE.Vector4[] = [];
  const d: number[] = new Array(MAX_FAMILIES * MAX_DASHES).fill(0);
  const k: number[] = new Array(MAX_FAMILIES).fill(0);
  let n = 0;
  for (const f of fams.slice(0, MAX_FAMILIES)) {
    const r = (f.angle * Math.PI) / 180;
    const [c, s] = [Math.cos(r), Math.sin(r)];
    const spacing = f.offset[0] * -s + f.offset[1] * c;
    if (!(Math.abs(spacing) > 1e-6)) continue;
    const dashes = f.dash.slice(0, MAX_DASHES);
    const period = dashes.reduce((t, x) => t + Math.abs(x), 0);
    a.push(new THREE.Vector4(c, s, spacing, dashes.length ? period : 0));
    b.push(new THREE.Vector4(f.base[0], f.base[1], f.offset[0], f.offset[1]));
    // (a dot is a dash of 0: the count says where the list ends; a period of 0 means solid)
    dashes.forEach((x, i) => (d[n * MAX_DASHES + i] = x));
    k[n] = dashes.length;
    n++;
  }
  while (a.length < MAX_FAMILIES) {
    a.push(new THREE.Vector4());
    b.push(new THREE.Vector4());
  }
  return { a, b, d, k, n };
}

/** For one family: (distance across to the nearest line, distance along it to the nearest dash or dot) in plane mm. */
function familyDist(q: [number, number], f: Family): [number, number] | null {
  const r = (f.angle * Math.PI) / 180;
  const [c, s] = [Math.cos(r), Math.sin(r)];
  const spacing = f.offset[0] * -s + f.offset[1] * c;
  if (!(Math.abs(spacing) > 1e-6)) return null;
  const rel = [q[0] - f.base[0], q[1] - f.base[1]];
  const i = Math.round((rel[0] * -s + rel[1] * c) / spacing);
  const p = [f.base[0] + f.offset[0] * i, f.base[1] + f.offset[1] * i];
  const w = [q[0] - p[0], q[1] - p[1]];
  const across = Math.abs(w[0] * -s + w[1] * c);
  const t = w[0] * c + w[1] * s;
  const period = f.dash.reduce((x, y) => x + Math.abs(y), 0);
  if (!f.dash.length || period <= 0) return [across, 0];
  const tt = ((t % period) + period) % period;
  let along = Infinity;
  let pos = 0;
  for (const dl of f.dash) {
    const len = Math.abs(dl);
    if (dl >= 0) {
      // this dash (or dot), and its copies a period either side
      for (const shift of [-period, 0, period]) {
        const s0 = pos + shift;
        along = Math.min(along, Math.max(0, s0 - tt, tt - (s0 + len)));
      }
    }
    pos += len;
  }
  return [across, along];
}

/**
 * How much ink a point gets (0 to 1): `q` in plane mm, `mmPerPx` how many plane mm one screen pixel spans there.
 * The shader's own rule: a round-capped line `max(1, weight_mm * PX_PER_MM)` pixels wide, half a pixel of blur.
 */
export function hatchInk(q: [number, number], fams: Family[], weightMm: number, mmPerPx: number): number {
  const half = Math.max(1, weightMm * PX_PER_MM) / 2;
  let ink = 0;
  for (const f of fams) {
    const d = familyDist(q, f);
    if (!d) continue;
    const px = Math.hypot(d[0], d[1]) / mmPerPx;
    ink = Math.max(ink, Math.min(1, Math.max(0, half + 0.5 - px)));
  }
  return ink;
}

const vertex = /* glsl */ `
  uniform vec3 uOrigin;
  uniform vec3 uU;
  uniform vec3 uV;
  varying vec2 vQ;
  #include <common>
  #include <clipping_planes_pars_vertex>
  void main() {
    vec4 world = modelMatrix * vec4(position, 1.0);
    vec3 rel = world.xyz - uOrigin;
    vQ = vec2(dot(rel, uU), dot(rel, uV)) * 1000.0;
    vec4 mvPosition = viewMatrix * world;
    gl_Position = projectionMatrix * mvPosition;
    #include <clipping_planes_vertex>
  }
`;

const fragment = /* glsl */ `
  #define MAX_FAMILIES ${MAX_FAMILIES}
  #define MAX_DASHES ${MAX_DASHES}
  uniform vec4 uA[MAX_FAMILIES];
  uniform vec4 uB[MAX_FAMILIES];
  uniform float uD[MAX_FAMILIES * MAX_DASHES];
  uniform float uK[MAX_FAMILIES];
  uniform int uN;
  uniform vec3 uInk;
  uniform vec3 uPaper;
  uniform float uHalf;
  varying vec2 vQ;
  #include <common>
  #include <clipping_planes_pars_fragment>
  void main() {
    #include <clipping_planes_fragment>
    // plane mm per screen pixel here (the cap may be seen at an angle: the larger of the two)
    vec2 dx = dFdx(vQ);
    vec2 dy = dFdy(vQ);
    float mmPerPx = max(max(length(dx), length(dy)), 1e-6);
    float ink = 0.0;
    for (int f = 0; f < MAX_FAMILIES; f++) {
      if (f >= uN) break;
      vec4 a = uA[f];
      vec4 b = uB[f];
      vec2 dir = a.xy;
      vec2 nrm = vec2(-a.y, a.x);
      vec2 rel = vQ - b.xy;
      float i = floor(dot(rel, nrm) / a.z + 0.5);
      vec2 w = vQ - (b.xy + b.zw * i);
      float across = abs(dot(w, nrm));
      float along = 0.0;
      if (a.w > 0.0) {
        float t = dot(w, dir);
        float tt = mod(t, a.w);
        along = 1e9;
        float pos = 0.0;
        for (int k = 0; k < MAX_DASHES; k++) {
          if (float(k) >= uK[f]) break;
          float dl = uD[f * MAX_DASHES + k];
          float len = abs(dl);
          if (dl >= 0.0) {
            for (int sh = -1; sh <= 1; sh++) {
              float s0 = pos + float(sh) * a.w;
              along = min(along, max(0.0, max(s0 - tt, tt - (s0 + len))));
            }
          }
          pos += len;
        }
      }
      float px = length(vec2(across, along)) / mmPerPx;
      ink = max(ink, clamp(uHalf + 0.5 - px, 0.0, 1.0));
    }
    gl_FragColor = vec4(mix(uPaper, uInk, ink), 1.0);
    #include <colorspace_fragment>
  }
`;

/** The material for one hatched tag's cap. `clip`: the view's clipping planes. */
export function hatchMaterial(h: CapHatch, clip: THREE.Plane[] | null): THREE.ShaderMaterial {
  const p = pack(h.families);
  const ink = new THREE.Color(h.color || "#1a1a1f");
  const m = new THREE.ShaderMaterial({
    uniforms: {
      uOrigin: { value: new THREE.Vector3(...h.origin) },
      uU: { value: new THREE.Vector3(...h.u) },
      uV: { value: new THREE.Vector3(...h.v) },
      uA: { value: p.a },
      uB: { value: p.b },
      uD: { value: p.d },
      uK: { value: p.k },
      uN: { value: p.n },
      uInk: { value: new THREE.Vector3(ink.r, ink.g, ink.b) },
      uPaper: { value: new THREE.Vector3().setFromColor(new THREE.Color(PAPER)) },
      uHalf: { value: Math.max(1, (Number.isFinite(h.weight_mm) ? h.weight_mm : 0.13) * PX_PER_MM) / 2 },
    },
    vertexShader: vertex,
    fragmentShader: fragment,
    side: THREE.DoubleSide,
    polygonOffset: true,
    polygonOffsetFactor: 1,
    polygonOffsetUnits: 4,
    clipping: true,
  });
  if (clip) m.clippingPlanes = clip;
  m.userData.color = "#" + ink.getHexString();
  m.userData.px = 2 * (m.uniforms.uHalf.value as number);
  return m;
}
