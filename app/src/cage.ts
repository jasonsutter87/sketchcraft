// SubD mode: shape a subdivision object by its cage, Maya-style. The cage
// (points, edges, faces) draws over a live smooth surface that's recomputed
// in the browser while you drag (Catmull-Clark, the same rules as the engine's
// subd.rs); letting go commits the change to the engine (cage_edit), which
// rebuilds the real surface and makes it one undo step.

import * as THREE from "three";
import type { Reply } from "./engine";

export interface CageData {
  points: number[][];
  faces: number[][];
  materials: (string | null)[];
  creases: number[][];
  levels: number;
  symmetry: { axis: number; center: number } | null;
}

export type CompMode = "vertex" | "edge" | "face";

export interface CageUi {
  group: number;
  mode: CompMode;
  selected: number;
  display: "both" | "cage";
  soft: number;
  symmetric: boolean;
  levels: number;
  tool: "select" | "loop";
  lock: number | null;
  points: number;
  faces: number;
}

type V = number[];
const ekey = (a: number, b: number) => (a < b ? `${a},${b}` : `${b},${a}`);

// ---- Catmull-Clark (the engine's subd::step, for the live preview) ------------------------------
interface Cage {
  p: V[];
  f: number[][];
  m: (string | null)[];
  c: Set<string>;
}

function step(cg: Cage): Cage {
  const n = cg.p.length;
  const fp = cg.f.map((f) => {
    const s = [0, 0, 0];
    for (const i of f) for (let k = 0; k < 3; k++) s[k] += cg.p[i][k];
    return s.map((x) => x / f.length);
  });
  const ef = new Map<string, number[]>();
  cg.f.forEach((f, fi) => {
    for (let k = 0; k < f.length; k++) {
      const key = ekey(f[k], f[(k + 1) % f.length]);
      const l = ef.get(key);
      if (l) l.push(fi);
      else ef.set(key, [fi]);
    }
  });
  const sharp = (key: string, fs: number[]) => fs.length !== 2 || cg.c.has(key);
  const sumF = Array.from({ length: n }, () => [0, 0, 0]);
  const cntF = new Array(n).fill(0);
  cg.f.forEach((f, fi) => {
    for (const i of f) {
      for (let k = 0; k < 3; k++) sumF[i][k] += fp[fi][k];
      cntF[i]++;
    }
  });
  const midSum = Array.from({ length: n }, () => [0, 0, 0]);
  const val = new Array(n).fill(0);
  const sharpNb: number[][] = Array.from({ length: n }, () => []);
  for (const [key, fs] of ef) {
    const [a, b] = key.split(",").map(Number);
    for (const v of [a, b]) {
      for (let k = 0; k < 3; k++) midSum[v][k] += (cg.p[a][k] + cg.p[b][k]) / 2;
      val[v]++;
    }
    if (sharp(key, fs)) {
      sharpNb[a].push(b);
      sharpNb[b].push(a);
    }
  }
  const p: V[] = [];
  for (let i = 0; i < n; i++) {
    const P = cg.p[i];
    const s = sharpNb[i];
    if (s.length > 2 || val[i] === 0 || cntF[i] === 0) p.push([...P]);
    else if (s.length === 2) p.push([0, 1, 2].map((k) => (P[k] * 6 + cg.p[s[0]][k] + cg.p[s[1]][k]) / 8));
    else {
      const nv = val[i];
      p.push([0, 1, 2].map((k) => (sumF[i][k] / cntF[i] + (midSum[i][k] / nv) * 2 + P[k] * (nv - 3)) / nv));
    }
  }
  const ep = new Map<string, number>();
  for (const [key, fs] of ef) {
    const [a, b] = key.split(",").map(Number);
    const q = sharp(key, fs) ? [0, 1, 2].map((k) => (cg.p[a][k] + cg.p[b][k]) / 2) : [0, 1, 2].map((k) => (cg.p[a][k] + cg.p[b][k] + fp[fs[0]][k] + fp[fs[1]][k]) / 4);
    ep.set(key, p.length);
    p.push(q);
  }
  const base = p.length;
  p.push(...fp);
  const f: number[][] = [];
  const m: (string | null)[] = [];
  cg.f.forEach((face, fi) => {
    const L = face.length;
    for (let k = 0; k < L; k++) {
      const prev = face[(k + L - 1) % L], cur = face[k], next = face[(k + 1) % L];
      f.push([cur, ep.get(ekey(cur, next))!, base + fi, ep.get(ekey(prev, cur))!]);
      m.push(cg.m[fi]);
    }
  });
  const c = new Set<string>();
  for (const key of cg.c) {
    const mid = ep.get(key);
    if (mid == null) continue;
    const [a, b] = key.split(",").map(Number);
    c.add(ekey(a, mid));
    c.add(ekey(mid, b));
  }
  return { p, f, m, c };
}

export function subdivide(points: V[], faces: number[][], materials: (string | null)[], creases: number[][], levels: number): Cage {
  let cg: Cage = { p: points, f: faces, m: materials, c: new Set(creases.map(([a, b]) => ekey(a, b))) };
  for (let i = 0; i < levels; i++) cg = step(cg);
  return cg;
}

// ---- the editor ---------------------------------------------------------------------------------
export interface EditorOpts {
  group: number;
  data: CageData;
  root: THREE.Group;
  host: HTMLElement;
  camera: () => THREE.Camera;
  colorOf: (material: string | null) => [number, number, number];
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  onUi: (ui: CageUi) => void;
  toast: (t: string, bad?: boolean) => void;
}

const ACCENT = 0x6f4df0;
const SELECT = 0xf2c230;

export class CageEditor {
  o: EditorOpts;
  data: CageData;
  pts: V[];
  mode: CompMode = "vertex";
  sel = new Set<number>();
  display: "both" | "cage" = "both";
  soft = 0;
  tool: "select" | "loop" = "select";
  lock: number | null = null;
  hover: { kind: CompMode; id: number; t?: number } | null = null;
  mirror: (number | null)[] = [];
  edges: [number, number][] = [];
  drag: { orig: V[]; start: THREE.Vector3; plane: THREE.Plane; w: Float32Array; moving: number[] } | null = null;
  band: { x: number; y: number; el: HTMLDivElement } | null = null;
  pending = 0;

  constructor(o: EditorOpts) {
    this.o = o;
    this.data = o.data;
    this.pts = o.data.points.map((p) => [...p]);
    this.soft = 0;
    this.index();
    this.draw();
    this.ui();
  }

  /** New cage from the engine (after a commit, undo, or a script). */
  setData(d: CageData) {
    const sameShape = d.points.length === this.data.points.length && d.faces.length === this.data.faces.length;
    this.data = d;
    this.pts = d.points.map((p) => [...p]);
    if (!sameShape) this.sel.clear();
    this.index();
    this.draw();
    this.ui();
  }

  dispose() {
    this.o.root.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    this.o.root.clear();
    this.band?.el.remove();
  }

  ui() {
    this.o.onUi({ group: this.o.group, mode: this.mode, selected: this.sel.size, display: this.display, soft: this.soft, symmetric: !!this.data.symmetry, levels: this.data.levels, tool: this.tool, lock: this.lock, points: this.pts.length, faces: this.data.faces.length });
  }

  private index() {
    const seen = new Set<string>();
    this.edges = [];
    for (const f of this.data.faces)
      for (let k = 0; k < f.length; k++) {
        const a = f[k], b = f[(k + 1) % f.length];
        const key = ekey(a, b);
        if (!seen.has(key)) (seen.add(key), this.edges.push([a, b]));
      }
    // mirror partners (same rule as the engine: position matching across the plane)
    const s = this.data.symmetry;
    if (!s) {
      this.mirror = this.pts.map(() => null);
      return;
    }
    let lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (const p of this.pts) for (let k = 0; k < 3; k++) (lo[k] = Math.min(lo[k], p[k])), (hi[k] = Math.max(hi[k], p[k]));
    const tol = Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) * 1e-5 + 1e-7;
    const cell = (p: V) => p.map((x) => Math.round(x / tol / 4)).join(",");
    const grid = new Map<string, number[]>();
    this.pts.forEach((p, i) => {
      const c = cell(p);
      grid.set(c, [...(grid.get(c) ?? []), i]);
    });
    this.mirror = this.pts.map((p, i) => {
      const q = [...p];
      q[s.axis] = 2 * s.center - p[s.axis];
      if (Math.abs(q[s.axis] - p[s.axis]) <= tol) return i;
      const c = q.map((x) => Math.round(x / tol / 4));
      for (let dx = -1; dx <= 1; dx++)
        for (let dy = -1; dy <= 1; dy++)
          for (let dz = -1; dz <= 1; dz++)
            for (const j of grid.get([c[0] + dx, c[1] + dy, c[2] + dz].join(",")) ?? [])
              if (Math.hypot(this.pts[j][0] - q[0], this.pts[j][1] - q[1], this.pts[j][2] - q[2]) <= tol) return j;
      return null;
    });
  }

  // ---- drawing ----
  draw() {
    const root = this.o.root;
    root.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    root.clear();
    if (this.display === "both") {
      const sm = subdivide(this.pts, this.data.faces, this.data.materials, this.data.creases, Math.max(1, Math.min(this.data.levels, 3)));
      const pos: number[] = [];
      const col: number[] = [];
      sm.f.forEach((f, fi) => {
        const c = this.o.colorOf(sm.m[fi]);
        const tri = (a: number, b: number, d: number) => {
          for (const i of [a, b, d]) pos.push(...sm.p[i]), col.push(...c);
        };
        tri(f[0], f[1], f[2]);
        tri(f[0], f[2], f[3]);
      });
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
      g.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
      g.computeVertexNormals();
      root.add(new THREE.Mesh(g, new THREE.MeshLambertMaterial({ vertexColors: true, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 })));
    }
    // the cage: edges (creases bolder), points
    const creased = new Set(this.data.creases.map(([a, b]) => ekey(a, b)));
    const plain: number[] = [], sharp: number[] = [];
    for (const [a, b] of this.edges) (creased.has(ekey(a, b)) ? sharp : plain).push(...this.pts[a], ...this.pts[b]);
    root.add(this.segs(plain, ACCENT, 0.75));
    if (sharp.length) root.add(this.segs(sharp, 0xe5484d, 1));
    const vp: number[] = [];
    this.pts.forEach((p) => vp.push(...p));
    const pg = new THREE.BufferGeometry();
    pg.setAttribute("position", new THREE.Float32BufferAttribute(vp, 3));
    if (this.mode === "vertex") root.add(new THREE.Points(pg, new THREE.PointsMaterial({ color: ACCENT, size: 6, sizeAttenuation: false, depthTest: false, transparent: true, opacity: 0.85 })));
    // selection and hover
    const hl = (ids: Iterable<number>, kind: CompMode, color: number, size = 9) => {
      const out: THREE.Object3D[] = [];
      if (kind === "vertex") {
        const p: number[] = [];
        for (const i of ids) if (this.pts[i]) p.push(...this.pts[i]);
        const g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.Float32BufferAttribute(p, 3));
        out.push(new THREE.Points(g, new THREE.PointsMaterial({ color, size, sizeAttenuation: false, depthTest: false })));
      } else if (kind === "edge") {
        const p: number[] = [];
        for (const i of ids) if (this.edges[i]) p.push(...this.pts[this.edges[i][0]], ...this.pts[this.edges[i][1]]);
        out.push(this.segs(p, color, 1));
      } else {
        const p: number[] = [];
        for (const i of ids) {
          const f = this.data.faces[i];
          if (!f) continue;
          for (let k = 1; k + 1 < f.length; k++) p.push(...this.pts[f[0]], ...this.pts[f[k]], ...this.pts[f[k + 1]]);
        }
        const g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.Float32BufferAttribute(p, 3));
        out.push(new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.35, side: THREE.DoubleSide, depthTest: false })));
      }
      out.forEach((o) => ((o.renderOrder = 20), root.add(o)));
    };
    if (this.sel.size) hl(this.sel, this.mode, SELECT);
    if (this.hover && !this.drag) {
      hl([this.hover.id], this.hover.kind, this.tool === "loop" ? 0xe5484d : 0xffffff, 8);
      if (this.tool === "loop" && this.hover.kind === "edge" && this.hover.t != null) {
        const [a, b] = this.edges[this.hover.id];
        const t = this.hover.t;
        const q = [0, 1, 2].map((k) => this.pts[a][k] + (this.pts[b][k] - this.pts[a][k]) * t);
        const g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.Float32BufferAttribute(q, 3));
        const dot = new THREE.Points(g, new THREE.PointsMaterial({ color: 0xe5484d, size: 11, sizeAttenuation: false, depthTest: false }));
        dot.renderOrder = 21;
        root.add(dot);
      }
    }
  }

  private segs(p: number[], color: number, opacity: number) {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(p, 3));
    const l = new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true, opacity }));
    l.renderOrder = 15;
    return l;
  }

  // ---- picking ----
  private screen(p: V) {
    const r = this.o.host.getBoundingClientRect();
    const v = new THREE.Vector3(p[0], p[1], p[2]).project(this.o.camera());
    return { x: r.left + (v.x * 0.5 + 0.5) * r.width, y: r.top + (-v.y * 0.5 + 0.5) * r.height, z: v.z };
  }

  pickAt(x: number, y: number, kind: CompMode): { id: number; t?: number } | null {
    const S = this.pts.map((p) => this.screen(p));
    if (kind === "vertex") {
      let best: { id: number; d: number; z: number } | null = null;
      S.forEach((s, i) => {
        const d = Math.hypot(s.x - x, s.y - y);
        if (s.z > 1 || d > 10) return;
        if (!best || d < best.d - 2 || (Math.abs(d - best.d) <= 2 && s.z < best.z)) best = { id: i, d, z: s.z };
      });
      return best ? { id: (best as { id: number }).id } : null;
    }
    if (kind === "edge") {
      let best: { id: number; d: number; z: number; t: number } | null = null;
      this.edges.forEach(([a, b], i) => {
        const A = S[a], B = S[b];
        const dx = B.x - A.x, dy = B.y - A.y;
        const L = dx * dx + dy * dy || 1e-9;
        const t = Math.max(0, Math.min(1, ((x - A.x) * dx + (y - A.y) * dy) / L));
        const d = Math.hypot(A.x + dx * t - x, A.y + dy * t - y);
        const z = A.z + (B.z - A.z) * t;
        if (z > 1 || d > 7) return;
        if (!best || d < best.d - 2 || (Math.abs(d - best.d) <= 2 && z < best.z)) best = { id: i, d, z, t };
      });
      return best ? { id: (best as { id: number }).id, t: (best as { t: number }).t } : null;
    }
    let best: { id: number; z: number } | null = null;
    this.data.faces.forEach((f, i) => {
      const poly = f.map((v) => S[v]);
      let inside = false;
      for (let a = 0, b = poly.length - 1; a < poly.length; b = a++) {
        if (poly[a].y > y !== poly[b].y > y && x < ((poly[b].x - poly[a].x) * (y - poly[a].y)) / (poly[b].y - poly[a].y) + poly[a].x) inside = !inside;
      }
      if (!inside) return;
      const z = poly.reduce((s, p) => s + p.z, 0) / poly.length;
      if (z <= 1 && (!best || z < best.z)) best = { id: i, z };
    });
    return best ? { id: (best as { id: number }).id } : null;
  }

  /** The points a selection moves. */
  private movingPoints(): number[] {
    const out = new Set<number>();
    for (const i of this.sel) {
      if (this.mode === "vertex") out.add(i);
      else if (this.mode === "edge" && this.edges[i]) this.edges[i].forEach((v) => out.add(v));
      else if (this.mode === "face" && this.data.faces[i]) this.data.faces[i].forEach((v) => out.add(v));
    }
    return [...out];
  }

  // ---- input (return true when handled) ----
  pointerDown(e: PointerEvent): boolean {
    if (e.button !== 0) return false;
    if (this.tool === "loop") {
      const h = this.pickAt(e.clientX, e.clientY, "edge");
      if (h) void this.insertLoop(h.id, h.t ?? 0.5);
      return true;
    }
    const h = this.pickAt(e.clientX, e.clientY, this.mode);
    if (!h) {
      // empty space: a selection box
      const el = document.createElement("div");
      el.className = "select-box";
      Object.assign(el.style, { position: "fixed", left: e.clientX + "px", top: e.clientY + "px", width: "0px", height: "0px", pointerEvents: "none", zIndex: "40" });
      document.body.appendChild(el);
      this.band = { x: e.clientX, y: e.clientY, el };
      if (!e.shiftKey) this.sel.clear();
      this.draw();
      this.ui();
      return true;
    }
    if (e.shiftKey) {
      if (this.sel.has(h.id)) this.sel.delete(h.id);
      else this.sel.add(h.id);
      this.draw();
      this.ui();
      return true;
    }
    if (!this.sel.has(h.id)) this.sel = new Set([h.id]);
    this.startDrag(e);
    this.ui();
    return true;
  }

  private startDrag(e: PointerEvent) {
    const moving = this.movingPoints();
    if (!moving.length) return;
    const c = new THREE.Vector3();
    for (const i of moving) c.add(new THREE.Vector3(...this.pts[i]));
    c.divideScalar(moving.length);
    const cam = this.o.camera();
    const normal = new THREE.Vector3();
    cam.getWorldDirection(normal);
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(normal, c);
    const start = this.hit(e, plane) ?? c.clone();
    // soft selection: nearby points follow, less the further they are
    const w = new Float32Array(this.pts.length);
    const set = new Set(moving);
    for (let i = 0; i < this.pts.length; i++) {
      if (set.has(i)) w[i] = 1;
      else if (this.soft > 0) {
        let d = Infinity;
        for (const j of moving) d = Math.min(d, Math.hypot(this.pts[i][0] - this.pts[j][0], this.pts[i][1] - this.pts[j][1], this.pts[i][2] - this.pts[j][2]));
        const u = d / this.soft;
        w[i] = u < 1 ? (1 - u * u) * (1 - u * u) : 0;
      }
    }
    this.drag = { orig: this.pts.map((p) => [...p]), start, plane, w, moving };
  }

  private hit(e: { clientX: number; clientY: number }, plane: THREE.Plane): THREE.Vector3 | null {
    const r = this.o.host.getBoundingClientRect();
    const ndc = new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    const rc = new THREE.Raycaster();
    rc.setFromCamera(ndc, this.o.camera());
    return rc.ray.intersectPlane(plane, new THREE.Vector3());
  }

  pointerMove(e: PointerEvent): boolean {
    if (this.band) {
      const b = this.band;
      Object.assign(b.el.style, { left: Math.min(b.x, e.clientX) + "px", top: Math.min(b.y, e.clientY) + "px", width: Math.abs(e.clientX - b.x) + "px", height: Math.abs(e.clientY - b.y) + "px" });
      return true;
    }
    if (this.drag) {
      const p = this.hit(e, this.drag.plane);
      if (!p) return true;
      const d = p.sub(this.drag.start);
      if (this.lock != null) {
        const ax = [0, 0, 0];
        ax[this.lock] = 1;
        const k = d.x * ax[0] + d.y * ax[1] + d.z * ax[2];
        d.set(ax[0] * k, ax[1] * k, ax[2] * k);
      }
      this.applyDelta([d.x, d.y, d.z]);
      if (!this.pending) this.pending = requestAnimationFrame(() => ((this.pending = 0), this.draw()));
      return true;
    }
    const kind = this.tool === "loop" ? "edge" : this.mode;
    const h = this.pickAt(e.clientX, e.clientY, kind);
    const same = h && this.hover && h.id === this.hover.id && this.hover.kind === kind && (kind !== "edge" || Math.abs((h.t ?? 0) - (this.hover.t ?? 0)) < 0.02);
    if (!same && (h || this.hover)) {
      this.hover = h ? { kind, id: h.id, t: h.t } : null;
      this.draw();
    }
    return true;
  }

  private applyDelta(d: number[]) {
    const D = this.drag!;
    const s = this.data.symmetry;
    const done = new Set<number>();
    for (let i = 0; i < this.pts.length; i++) {
      const w = D.w[i];
      if (!w) continue;
      const j = this.mirror[i];
      // a pair both moving: the first one decides, the other mirrors it
      if (j != null && j !== i && done.has(j)) continue;
      const p = [0, 1, 2].map((k) => D.orig[i][k] + d[k] * w);
      if (s && j === i) p[s.axis] = s.center;
      this.pts[i] = p;
      done.add(i);
      if (s && j != null && j !== i) {
        const q = [...p];
        q[s.axis] = 2 * s.center - p[s.axis];
        this.pts[j] = q;
        done.add(j);
      }
    }
  }

  async pointerUp(e: PointerEvent): Promise<boolean> {
    if (this.band) {
      const b = this.band;
      const [x0, x1] = [Math.min(b.x, e.clientX), Math.max(b.x, e.clientX)];
      const [y0, y1] = [Math.min(b.y, e.clientY), Math.max(b.y, e.clientY)];
      b.el.remove();
      this.band = null;
      if (x1 - x0 > 3 || y1 - y0 > 3) {
        const S = this.pts.map((p) => this.screen(p));
        const inside = (i: number) => S[i].x >= x0 && S[i].x <= x1 && S[i].y >= y0 && S[i].y <= y1 && S[i].z <= 1;
        if (this.mode === "vertex") this.pts.forEach((_, i) => inside(i) && this.sel.add(i));
        else if (this.mode === "edge") this.edges.forEach(([a, c], i) => inside(a) && inside(c) && this.sel.add(i));
        else this.data.faces.forEach((f, i) => f.every(inside) && this.sel.add(i));
      }
      this.draw();
      this.ui();
      return true;
    }
    if (this.drag) {
      const D = this.drag;
      this.drag = null;
      const moves: number[][] = [];
      this.pts.forEach((p, i) => {
        if (p.some((v, k) => Math.abs(v - D.orig[i][k]) > 1e-9)) moves.push([i, ...p]);
      });
      if (moves.length) {
        const r = await this.o.run({ cmd: "cage_edit", id: this.o.group, moves, mirror: false });
        if (!r.ok) {
          this.pts = D.orig;
          this.draw();
        }
      }
      return true;
    }
    return false;
  }

  // ---- commands ----
  setMode(m: CompMode) {
    if (m === this.mode) return;
    // carry the selection over as points
    const pts = this.movingPoints();
    this.mode = m;
    this.sel = new Set();
    const has = new Set(pts);
    if (m === "vertex") pts.forEach((i) => this.sel.add(i));
    else if (m === "edge") this.edges.forEach(([a, b], i) => has.has(a) && has.has(b) && this.sel.add(i));
    else this.data.faces.forEach((f, i) => f.every((v) => has.has(v)) && this.sel.add(i));
    this.tool = "select";
    this.draw();
    this.ui();
  }

  setDisplay(d: "both" | "cage") {
    this.display = d;
    this.draw();
    this.ui();
  }

  setSoft(r: number) {
    this.soft = Math.max(0, r);
    this.ui();
  }

  setTool(t: "select" | "loop") {
    this.tool = t;
    this.hover = null;
    this.draw();
    this.ui();
  }

  setLock(a: number | null) {
    this.lock = a;
    this.ui();
  }

  size() {
    let lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (const p of this.pts) for (let k = 0; k < 3; k++) (lo[k] = Math.min(lo[k], p[k])), (hi[k] = Math.max(hi[k], p[k]));
    return Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
  }

  async edit(cmd: Record<string, unknown>, ok?: string) {
    const r = await this.o.run({ cmd: "cage_edit", id: this.o.group, ...cmd });
    if (r.ok && ok) this.o.toast(ok);
    return r;
  }

  async insertLoop(edge: number, t: number) {
    const [a, b] = this.edges[edge];
    await this.edit({ op: { kind: "insert_loop", edge: [a, b], t } });
  }

  async crease() {
    if (this.mode !== "edge" || !this.sel.size) return this.o.toast("Pick edges (2) to crease", true);
    const creased = new Set(this.data.creases.map(([a, b]) => ekey(a, b)));
    const picked = [...this.sel].map((i) => this.edges[i]);
    const allSharp = picked.every(([a, b]) => creased.has(ekey(a, b)));
    await this.edit(allSharp ? { uncrease: picked } : { crease: picked }, allSharp ? "Uncreased" : "Creased: those edges stay sharp");
  }

  async extrude() {
    if (this.mode !== "face" || !this.sel.size) return this.o.toast("Pick faces (3) to extrude", true);
    const r = await this.edit({ op: { kind: "extrude", faces: [...this.sel], distance: this.size() * 0.04 } });
    if (r.ok) {
      this.sel = new Set(r.faces as number[]);
      this.o.toast("Extruded: drag the new faces to shape them");
    }
  }

  async remove() {
    if (this.mode !== "face" || !this.sel.size) return this.o.toast("Pick faces (3) to delete", true);
    const faces = [...this.sel];
    this.sel.clear();
    await this.edit({ op: { kind: "delete_faces", faces } });
  }

  async merge() {
    if (this.mode !== "vertex" || this.sel.size < 2) return this.o.toast("Pick two or more points (1) to merge", true);
    const points = [...this.sel];
    this.sel.clear();
    await this.edit({ op: { kind: "merge", points } });
  }

  async setLevels(l: number) {
    await this.edit({ levels: Math.max(0, Math.min(4, l)) });
  }

  async toggleSymmetry() {
    await this.edit({ symmetry: this.data.symmetry ? "off" : "auto" }, this.data.symmetry ? "Symmetry off" : "Symmetry on");
  }

  /** Keys while in SubD mode. Returns true when handled. */
  key(e: KeyboardEvent): boolean {
    const k = e.key.toLowerCase();
    if (k === "1") return this.setMode("vertex"), true;
    if (k === "2") return this.setMode("edge"), true;
    if (k === "3") return this.setMode("face"), true;
    if (e.key === "Tab") return e.preventDefault(), this.setDisplay(this.display === "both" ? "cage" : "both"), true;
    if (k === "l") return this.setTool(this.tool === "loop" ? "select" : "loop"), true;
    if (k === "c") return void this.crease(), true;
    if (k === "e") return void this.extrude(), true;
    if (k === "m") return void this.merge(), true;
    if (k === "b") return this.setSoft(this.soft ? 0 : this.size() * 0.12), true;
    if (e.key === "Delete" || e.key === "Backspace") return void this.remove(), true;
    if (e.key === "+" || e.key === "=") return void this.setLevels(this.data.levels + 1), true;
    if (e.key === "-") return void this.setLevels(this.data.levels - 1), true;
    const locks: Record<string, number | null> = { ArrowRight: 0, ArrowLeft: 1, ArrowUp: 2, ArrowDown: null };
    if (e.key in locks) {
      e.preventDefault();
      const want = locks[e.key];
      this.setLock(this.lock === want ? null : want);
      return true;
    }
    return false;
  }
}
