// Inference ("snapping") engine: figures out what the cursor means in 3D.
// Priority: endpoint > midpoint > guide point > axis from the last point >
// on an edge / guide > on a face > on the ground (or the drawing plane).
import * as THREE from "three";
import type { FaceInfo, Scene } from "./engine";

export type Axis = 0 | 1 | 2;

export interface Snap {
  p: THREE.Vector3;
  /** a hovered point this one lines up with (draw a dotted line from it) */
  from?: THREE.Vector3;
  kind: "endpoint" | "midpoint" | "guide-point" | "axis" | "on-edge" | "on-guide" | "face" | "ground" | "plane";
  label: string;
  axis?: Axis;
  /** direction of the edge or guide we're on */
  along?: THREE.Vector3;
  face?: FaceInfo;
  /** the plane this point lies in (face normal, or up for the ground) */
  normal: THREE.Vector3;
}

// Points the cursor recently rested on: you can line up with them later
// ("From Point" inference), like hovering a corner then drawing level with it.
const recent: THREE.Vector3[] = [];
function remember(p: THREE.Vector3) {
  if (recent.some((r) => r.distanceTo(p) < 1e-6)) return;
  recent.push(p.clone());
  if (recent.length > 4) recent.shift();
}
export function forgetRecent() {
  recent.length = 0;
}

export const AXIS_COLORS = [0xe5484d, 0x30a46c, 0x3e63dd];
const AXIS_NAMES = ["Red", "Green", "Blue"];
const AXES = [new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 1)];

export interface Pick {
  face: FaceInfo | null;
  owner: number;
  point: THREE.Vector3;
  distance: number;
}

export interface InferEnv {
  camera: THREE.Camera;
  rect: DOMRect;
  scene: Scene;
  pick: (e: { clientX: number; clientY: number }) => Pick | null;
  groundZ: number;
}

export interface InferOpts {
  anchor?: THREE.Vector3 | null;
  lock?: Axis | null;
  /** keep the result on this plane (second corner of a rectangle...) */
  plane?: { origin: THREE.Vector3; normal: THREE.Vector3 } | null;
}

function screen(env: InferEnv, p: THREE.Vector3) {
  const q = p.clone().project(env.camera);
  return { x: (q.x * 0.5 + 0.5) * env.rect.width, y: (-q.y * 0.5 + 0.5) * env.rect.height, behind: q.z > 1 };
}

export function rayFrom(env: { camera: THREE.Camera; rect: DOMRect }, e: { clientX: number; clientY: number }): THREE.Ray {
  const ndc = new THREE.Vector2(((e.clientX - env.rect.left) / env.rect.width) * 2 - 1, -((e.clientY - env.rect.top) / env.rect.height) * 2 + 1);
  const rc = new THREE.Raycaster();
  rc.setFromCamera(ndc, env.camera);
  return rc.ray;
}

/** Closest point on the infinite line (o, d) to the ray. */
export function closestOnLine(ray: THREE.Ray, o: THREE.Vector3, d: THREE.Vector3): THREE.Vector3 {
  const w0 = o.clone().sub(ray.origin);
  const b = d.dot(ray.direction);
  const den = d.lengthSq() - b * b;
  if (Math.abs(den) < 1e-9) return o.clone();
  const t = (b * ray.direction.dot(w0) - w0.dot(d)) / den;
  return o.clone().addScaledVector(d, t);
}

export function infer(env: InferEnv, e: { clientX: number; clientY: number }, opts: InferOpts = {}): Snap {
  const s = env.scene;
  const mx = e.clientX - env.rect.left;
  const my = e.clientY - env.rect.top;
  const ray = rayFrom(env, e);
  const hit = env.pick(e);
  const plane = opts.plane;
  const onPlane = (q: THREE.Vector3) => !plane || Math.abs(q.clone().sub(plane.origin).dot(plane.normal)) < 1e-3;
  // hidden behind the surface under the cursor? (distance along the view ray, with slack
  // so points on the edges of that very surface still count as visible)
  const visible = (q: THREE.Vector3) => !hit || q.clone().sub(ray.origin).dot(ray.direction) <= hit.distance * 1.02 + 0.15;
  const up = new THREE.Vector3(0, 0, 1);
  const planeN = plane?.normal ?? (hit?.face ? V(hit.face.normal) : up);

  // axis lock (arrow keys): the cursor rides that axis from the anchor
  if (opts.anchor && opts.lock != null) {
    const q = closestOnLine(ray, opts.anchor, AXES[opts.lock]);
    return { p: q, kind: "axis", axis: opts.lock, label: `On ${AXIS_NAMES[opts.lock]} Axis`, normal: plane?.normal ?? up };
  }

  // points: endpoints, midpoints, guide points
  let best: { p: THREE.Vector3; d: number; kind: Snap["kind"]; label: string } | null = null;
  const consider = (q: THREE.Vector3, kind: Snap["kind"], label: string, radius: number) => {
    if (!onPlane(q) || !visible(q)) return;
    const sc = screen(env, q);
    if (sc.behind) return;
    const d = Math.hypot(sc.x - mx, sc.y - my);
    if (d < radius && (!best || d < best.d)) best = { p: q, d, kind, label };
  };
  const segs = s.edges;
  for (let i = 0; i < segs.length; i += 6) {
    const a = new THREE.Vector3(segs[i], segs[i + 1], segs[i + 2]);
    const b = new THREE.Vector3(segs[i + 3], segs[i + 4], segs[i + 5]);
    consider(a, "endpoint", "Endpoint", 12);
    consider(b, "endpoint", "Endpoint", 12);
  }
  if (!best) {
    for (let i = 0; i < segs.length; i += 6) {
      const a = new THREE.Vector3(segs[i], segs[i + 1], segs[i + 2]);
      const b = new THREE.Vector3(segs[i + 3], segs[i + 4], segs[i + 5]);
      consider(a.add(b).multiplyScalar(0.5), "midpoint", "Midpoint", 10);
    }
  }
  if (!best) for (const g of s.guides) if (!g.dir) consider(V(g.p), "guide-point", "Guide Point", 10);
  if (best) {
    const b = best as { p: THREE.Vector3; kind: Snap["kind"]; label: string };
    if (b.kind === "endpoint" || b.kind === "midpoint") remember(b.p);
    return { p: b.p.clone(), kind: b.kind, label: b.label, normal: planeN.clone(), face: hit?.face ?? undefined };
  }

  // axis inference from the last point
  if (opts.anchor) {
    let ax: { q: THREE.Vector3; d: number; k: Axis } | null = null;
    for (const k of [0, 1, 2] as Axis[]) {
      const q = closestOnLine(ray, opts.anchor, AXES[k]);
      if (!onPlane(q) || q.distanceTo(opts.anchor) < 1e-4) continue;
      const sc = screen(env, q);
      const d = Math.hypot(sc.x - mx, sc.y - my);
      if (d < 10 && (!ax || d < ax.d)) ax = { q, d, k };
    }
    if (ax) {
      // ...and level with a point hovered earlier?
      for (const r of recent) {
        const along = AXES[ax.k];
        const q = opts.anchor.clone().addScaledVector(along, r.clone().sub(opts.anchor).dot(along));
        if (q.distanceTo(r) < 1e-6 || !onPlane(q)) continue;
        const sc = screen(env, q);
        if (Math.hypot(sc.x - mx, sc.y - my) < 9) return { p: q, from: r.clone(), kind: "axis", axis: ax.k, label: `On ${AXIS_NAMES[ax.k]} Axis · From Point`, normal: plane?.normal ?? up };
      }
      return { p: ax.q, kind: "axis", axis: ax.k, label: `On ${AXIS_NAMES[ax.k]} Axis`, normal: plane?.normal ?? up };
    }
  }

  // in line with a point hovered earlier, along an axis
  {
    let fp: { q: THREE.Vector3; d: number; k: Axis; r: THREE.Vector3 } | null = null;
    for (const r of recent) {
      for (const k of [0, 1, 2] as Axis[]) {
        const q = closestOnLine(ray, r, AXES[k]);
        if (q.distanceTo(r) < 0.05 || !onPlane(q) || !visible(q)) continue;
        const sc = screen(env, q);
        if (sc.behind) continue;
        const d = Math.hypot(sc.x - mx, sc.y - my);
        if (d < 7 && (!fp || d < fp.d)) fp = { q, d, k, r };
      }
    }
    if (fp) return { p: fp.q, from: fp.r.clone(), kind: "axis", axis: fp.k, label: "From Point", normal: plane?.normal ?? (hit?.face ? V(hit.face.normal) : up) };
  }

  // on an edge or a guide line
  let line: { p: THREE.Vector3; d: number; along: THREE.Vector3; label: string; kind: Snap["kind"] } | null = null;
  const onSeg = (a: THREE.Vector3, b: THREE.Vector3, label: string, kind: Snap["kind"], infinite: boolean) => {
    const dir = b.clone().sub(a);
    const len = dir.length();
    if (len < 1e-9) return;
    const q = closestOnLine(ray, a, dir.clone().normalize());
    if (!infinite) {
      const t = q.clone().sub(a).dot(dir) / (len * len);
      if (t < 0 || t > 1) return;
    }
    if (!onPlane(q) || !visible(q)) return;
    const sc = screen(env, q);
    if (sc.behind) return;
    const d = Math.hypot(sc.x - mx, sc.y - my);
    if (d < 7 && (!line || d < line.d)) line = { p: q, d, along: dir.normalize(), label, kind };
  };
  for (let i = 0; i < segs.length; i += 6) {
    onSeg(new THREE.Vector3(segs[i], segs[i + 1], segs[i + 2]), new THREE.Vector3(segs[i + 3], segs[i + 4], segs[i + 5]), "On Edge", "on-edge", false);
  }
  for (const g of s.guides) if (g.dir) onSeg(V(g.p), V(g.p).add(V(g.dir)), "On Guide", "on-guide", true);
  if (line) {
    const l = line as { p: THREE.Vector3; along: THREE.Vector3; label: string; kind: Snap["kind"] };
    return { p: l.p, kind: l.kind, label: l.label, along: l.along, normal: planeN.clone(), face: hit?.face ?? undefined };
  }

  // drawing plane, face, ground
  if (plane) {
    const q = ray.intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(plane.normal, plane.origin), new THREE.Vector3());
    if (q) return { p: q, kind: "plane", label: "", normal: plane.normal.clone() };
  }
  if (hit && hit.face) return { p: hit.point.clone(), kind: "face", label: "On Face", face: hit.face, normal: V(hit.face.normal) };
  if (hit) return { p: hit.point.clone(), kind: "face", label: "On Face", normal: up };
  const g = ray.intersectPlane(new THREE.Plane(up, -env.groundZ), new THREE.Vector3());
  return { p: g ?? opts.anchor?.clone() ?? new THREE.Vector3(), kind: "ground", label: "On Ground", normal: up };
}

export const V = (a: ArrayLike<number>) => new THREE.Vector3(a[0], a[1], a[2]);
