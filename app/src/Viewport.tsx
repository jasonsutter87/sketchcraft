// The 3D view. Draws the engine's scene with three.js and turns mouse and
// keyboard input into engine commands. Z is up, units are meters.
import { useEffect, useRef, useState } from "react";
import { ask } from "./ask";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { GTAOPass } from "three/examples/jsm/postprocessing/GTAOPass.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { Line2 } from "three/examples/jsm/lines/Line2.js";
import { LineGeometry } from "three/examples/jsm/lines/LineGeometry.js";
import { LineMaterial } from "three/examples/jsm/lines/LineMaterial.js";
import { exec, type FaceInfo, type Reply, type Scene, type Vec3 } from "./engine";
import { guide, type Guide } from "./guide";
import { Icons } from "./icons";
import { AXIS_COLORS, closestOnLine, infer, rayFrom, V, type Axis, type InferEnv, type Pick, type Snap } from "./inference";
import { combine, EMPTY, selIds, selMode, toggle, type Sel, type SelMode } from "./selection";
import { formatLength, parseLengths } from "./units";

export type Tool =
  | "select" | "line" | "arc" | "rect" | "circle" | "polygon"
  | "push" | "offset" | "followme" | "move" | "rotate" | "scale"
  | "tape" | "dimension" | "text" | "paint" | "erase" | "orbit" | "pan" | "zoom" | "section" | "place"
  | "wall" | "stairs" | "roof" | "column"
  | "smoove" | "stamp" | "drape" | "flipedge";

/** Settings for the building-block tools (lengths in meters). */
export interface BuildSettings {
  wallThickness: number;
  wallHeight: number;
  stairWidth: number;
  stairRise: number;
  roofKind: "gable" | "hip" | "shed";
  roofPitch: number;
  roofOverhang: number;
  columnSize: number;
  columnHeight: number;
  columnRound: boolean;
  /** Sandbox */
  smooveRadius: number;
  smooveHeight: number;
  stampOffset: number;
  gridSize: number;
  gridSpacing: number;
  contourSpacing: number;
}

export type FaceStyle = "textured" | "shaded" | "monochrome" | "hiddenline" | "wireframe";
export type Backdrop = "default" | "paper" | "blueprint" | "night";

export interface Styles {
  xray: boolean;
  /** SketchUp's face styles */
  face?: FaceStyle;
  /** bold outlines around shapes */
  profiles?: boolean;
  /** hand-drawn, doubled edges */
  sketchy?: boolean;
  /** edges run a little past their ends */
  extensions?: boolean;
  /** ambient occlusion: soft shading where surfaces meet */
  ao?: boolean;
  background?: Backdrop;
  /** hidden edges shown dashed (SketchUp's Back Edges, K) */
  backEdges: boolean;
  shadows: boolean;
  edges: boolean;
  parallel: boolean;
}

export interface Cam {
  eye: Vec3;
  target: Vec3;
  fov: number;
}

export interface ViewportApi {
  applyTyped: (text: string) => void;
  fit: () => void;
  fitSoon: () => void;
  view: (name: "iso" | "top" | "front" | "right" | "back" | "left") => void;
  camera: () => Cam;
  goTo: (c: Cam & { photo?: string }) => void;
  clearSection: () => void;
  /** the active section plane (a point on it and its normal), if any */
  sectionPlane: () => { origin: Vec3; normal: Vec3 } | null;
  /** a floor-plan cut: everything above `height` (m) taken away */
  planCut: (height: number) => void;
  /** slide the cut along its direction (m; + moves it deeper into what's kept) */
  moveSection: (by: number) => void;
}

interface Props {
  scene: Scene | null;
  tool: Tool;
  setTool: (t: Tool) => void;
  sel: Sel;
  setSel: (s: Sel) => void;
  color: [number, number, number];
  setColor: (c: [number, number, number]) => void;
  /** an image material to paint with (instead of the color) */
  paintMaterial: string | null;
  setPaintMaterial: (m: string | null) => void;
  styles: Styles;
  placeAsset: string | null;
  build: BuildSettings;
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  onMeasure: (label: string, value: string) => void;
  onContext: (x: number, y: number) => void;
  onToast: (t: string, bad?: boolean) => void;
  onSection: (on: boolean) => void;
  api: React.MutableRefObject<ViewportApi | null>;
}

const ACCENT = 0x6b5cff;
const EDGE_COLOR = 0x23252d;

/** Clear color, line color, and whether the ground grid shows, per background. */
const BACKGROUNDS: Record<Backdrop, { clear: number; line: number; face: number; grid: boolean }> = {
  default: { clear: 0xf3f4f8, line: EDGE_COLOR, face: 0xffffff, grid: true },
  paper: { clear: 0xf6f1e4, line: 0x2b2a27, face: 0xfbf8f0, grid: false },
  blueprint: { clear: 0x1d4f8c, line: 0xeaf2ff, face: 0x1d4f8c, grid: false },
  night: { clear: 0x16181f, line: 0xd8dbe6, face: 0x2a2d38, grid: false },
};

/** Little deterministic noise for sketchy lines. */
function jitter(i: number): number {
  const x = Math.sin(i * 12.9898 + 78.233) * 43758.5453;
  return x - Math.floor(x) - 0.5;
}

/** Edge segments, lengthened past their ends and/or doubled and wobbled. */
function styledEdges(seg: number[], size: number, extend: boolean, sketchy: boolean): number[] {
  const out: number[] = [];
  const a = new THREE.Vector3(), b = new THREE.Vector3(), d = new THREE.Vector3(), side = new THREE.Vector3();
  const ext = extend ? size * 0.012 : 0;
  for (let i = 0; i < seg.length; i += 6) {
    a.fromArray(seg, i);
    b.fromArray(seg, i + 3);
    d.subVectors(b, a);
    const len = d.length();
    if (len < 1e-9) continue;
    d.divideScalar(len);
    const copies = sketchy ? 2 : 1;
    // run-ons and wobble scale with the edge itself, so small things stay tidy
    const cap = Math.min(ext, len * 0.18);
    const wob = Math.min(size * 0.003, len * 0.02);
    for (let c = 0; c < copies; c++) {
      const k = i / 6 + c * 7919;
      const e0 = cap + (sketchy ? Math.abs(jitter(k)) * Math.min(size * 0.01, len * 0.12) : 0);
      const e1 = cap + (sketchy ? Math.abs(jitter(k + 1)) * Math.min(size * 0.01, len * 0.12) : 0);
      const p = a.clone().addScaledVector(d, -e0);
      const q = b.clone().addScaledVector(d, e1);
      if (sketchy) {
        // a small sideways wobble, different at each end
        side.set(-d.y, d.x, d.z * 0.3).normalize();
        if (side.lengthSq() < 1e-6) side.set(1, 0, 0);
        p.addScaledVector(side, jitter(k + 2) * wob);
        q.addScaledVector(side, jitter(k + 3) * wob);
      }
      out.push(p.x, p.y, p.z, q.x, q.y, q.z);
    }
  }
  return out;
}

/** Normals averaged at every shared corner (for the profile outline). */
function averagedNormals(pos: number[]): Float32Array {
  const acc = new Map<string, THREE.Vector3>();
  const key = (i: number) => `${Math.round(pos[i] * 1e4)},${Math.round(pos[i + 1] * 1e4)},${Math.round(pos[i + 2] * 1e4)}`;
  const a = new THREE.Vector3(), b = new THREE.Vector3(), c = new THREE.Vector3();
  for (let t = 0; t < pos.length; t += 9) {
    a.fromArray(pos, t);
    b.fromArray(pos, t + 3);
    c.fromArray(pos, t + 6);
    const n = b.clone().sub(a).cross(c.clone().sub(a)).normalize();
    for (let k = 0; k < 3; k++) {
      const id = key(t + k * 3);
      const v = acc.get(id);
      if (v) v.add(n);
      else acc.set(id, n.clone());
    }
  }
  const out = new Float32Array(pos.length);
  for (let v = 0; v < pos.length; v += 3) {
    const n = acc.get(key(v))!.clone().normalize();
    out[v] = n.x;
    out[v + 1] = n.y;
    out[v + 2] = n.z;
  }
  return out;
}

/** Back faces pushed out by a few screen pixels: a silhouette outline. */
function profileMaterial(color: number, px: number, clip: THREE.Plane[]): THREE.MeshBasicMaterial {
  const m = new THREE.MeshBasicMaterial({ color, side: THREE.BackSide, clippingPlanes: clip });
  m.onBeforeCompile = (shader) => {
    shader.uniforms.profilePx = { value: px };
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", "#include <common>\nuniform float profilePx;")
      .replace(
        "#include <project_vertex>",
        `#include <project_vertex>
        vec4 clipN = projectionMatrix * modelViewMatrix * vec4(normal, 0.0);
        vec2 dir = length(clipN.xy) > 1e-6 ? normalize(clipN.xy) : vec2(0.0);
        // pixels -> clip space (approximate viewport of 1000 px tall)
        gl_Position.xy += dir * profilePx * gl_Position.w / 500.0;`
      );
  };
  return m;
}
const UP = new THREE.Vector3(0, 0, 1);
const r4 = (x: number) => Math.round(x * 1e4) / 1e4;
const arr = (v: THREE.Vector3): Vec3 => [r4(v.x), r4(v.y), r4(v.z)];
/**
 * Per-corner normals for a triangle soup: corners where neighboring
 * triangles meet at under 35 degrees share an averaged normal (smooth
 * ground, cylinders), sharper corners keep the triangle's own (crisp boxes).
 */
function smoothNormals(pos: number[]): Float32Array {
  const n = pos.length / 9;
  const face = new Float32Array(n * 3);
  const a = new THREE.Vector3(), b = new THREE.Vector3(), c = new THREE.Vector3();
  for (let t = 0; t < n; t++) {
    a.fromArray(pos, t * 9);
    b.fromArray(pos, t * 9 + 3);
    c.fromArray(pos, t * 9 + 6);
    // not normalized: bigger triangles count more in the average
    const f = b.sub(a).cross(c.sub(a));
    face[t * 3] = f.x;
    face[t * 3 + 1] = f.y;
    face[t * 3 + 2] = f.z;
  }
  const key = (i: number) => `${Math.round(pos[i] * 1e4)},${Math.round(pos[i + 1] * 1e4)},${Math.round(pos[i + 2] * 1e4)}`;
  const at = new Map<string, number[]>();
  for (let v = 0; v < n * 3; v++) {
    const k = key(v * 3);
    const l = at.get(k);
    if (l) l.push(v);
    else at.set(k, [v]);
  }
  const out = new Float32Array(n * 9);
  const cos = Math.cos((35 * Math.PI) / 180);
  const own = new THREE.Vector3(), other = new THREE.Vector3(), sum = new THREE.Vector3();
  for (const corners of at.values()) {
    for (const v of corners) {
      const t = Math.floor(v / 3);
      own.fromArray(face, t * 3);
      const ownN = own.clone().normalize();
      sum.set(0, 0, 0);
      for (const w of corners) {
        other.fromArray(face, Math.floor(w / 3) * 3);
        if (other.clone().normalize().dot(ownN) > cos) sum.add(other);
      }
      if (sum.lengthSq() < 1e-30) sum.copy(own);
      sum.normalize();
      out[v * 3] = sum.x;
      out[v * 3 + 1] = sum.y;
      out[v * 3 + 2] = sum.z;
    }
  }
  return out;
}

// images for textured faces, loaded once per material and image
const textureCache = new Map<string, THREE.Texture>();
function textureFor(name: string, data: string): THREE.Texture {
  const key = `${name}:${data.length}:${data.slice(-32)}`;
  let t = textureCache.get(key);
  if (!t) {
    t = new THREE.TextureLoader().load(data);
    t.wrapS = t.wrapT = THREE.RepeatWrapping;
    t.colorSpace = THREE.SRGBColorSpace;
    t.anisotropy = 8;
    textureCache.set(key, t);
  }
  return t;
}

// display units come from the model; set whenever a scene arrives
let units = "m";
const fmt = (x: number) => formatLength(x, units);

/** In-plane axes for a normal: u horizontal when possible, v = n x u. */
function basis(n: THREE.Vector3): [THREE.Vector3, THREE.Vector3] {
  const u = Math.abs(n.z) > 0.9 ? new THREE.Vector3(1, 0, 0) : UP.clone().cross(n).normalize();
  return [u, n.clone().cross(u).normalize()];
}

/** Points on a circular arc from a to b, bowing `bulge` out from the chord. */
function arcPoints(a: THREE.Vector3, b: THREE.Vector3, bulge: number, n: THREE.Vector3, k = 12): THREE.Vector3[] {
  const chord = b.clone().sub(a);
  const l = chord.length();
  if (l < 1e-9 || Math.abs(bulge) < 1e-6) return [a, b];
  const side = n.clone().cross(chord.clone().normalize()).normalize();
  const s = Math.abs(bulge);
  const r = (l * l) / 4 / (2 * s) + s / 2;
  const mid = a.clone().add(b).multiplyScalar(0.5);
  const dir = side.clone().multiplyScalar(Math.sign(bulge));
  const c = mid.clone().addScaledVector(dir, s - r);
  const u = a.clone().sub(c).normalize();
  const w = n.clone().cross(u).normalize();
  const ang = (p: THREE.Vector3) => Math.atan2(p.clone().sub(c).dot(w), p.clone().sub(c).dot(u));
  const norm = (x: number) => ((x % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  const apex = norm(ang(mid.clone().addScaledVector(dir, s)));
  let end = norm(ang(b));
  if (apex > end) end -= 2 * Math.PI; // go the way that passes through the apex
  const pts: THREE.Vector3[] = [];
  for (let i = 0; i <= k; i++) {
    const t = (end * i) / k;
    pts.push(c.clone().addScaledVector(u, r * Math.cos(t)).addScaledVector(w, r * Math.sin(t)));
  }
  return pts;
}

function offsetOutline(outer: THREE.Vector3[], n: THREE.Vector3, d: number): THREE.Vector3[] {
  const [u, v] = basis(n);
  const depth = outer[0].dot(n);
  const p2 = outer.map((p) => new THREE.Vector2(p.dot(u), p.dot(v)));
  const k = p2.length;
  const left = (e: THREE.Vector2) => new THREE.Vector2(-e.y, e.x).normalize();
  return p2.map((b, i) => {
    const a = p2[(i + k - 1) % k];
    const c = p2[(i + 1) % k];
    const n1 = left(b.clone().sub(a));
    const n2 = left(c.clone().sub(b));
    const m = n1.clone().add(n2).multiplyScalar(d / Math.max(1 + n1.dot(n2), 0.2));
    const q = b.clone().add(m);
    return u.clone().multiplyScalar(q.x).addScaledVector(v, q.y).addScaledVector(n, depth);
  });
}

type Op =
  | { t: "line"; p0: THREE.Vector3; start: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "rect"; p0: THREE.Vector3; n: THREE.Vector3; u: THREE.Vector3; v: THREE.Vector3; a: number; b: number }
  | { t: "circle"; c: THREE.Vector3; n: THREE.Vector3; r: number; seg: number }
  | { t: "arc"; a: THREE.Vector3; b: THREE.Vector3 | null; n: THREE.Vector3; bulge: number; cur: THREE.Vector3 }
  | { t: "push"; face: FaceInfo; origin: THREE.Vector3; n: THREE.Vector3; d: number; dragged: boolean }
  | { t: "offset"; face: FaceInfo; start: THREE.Vector3; d: number }
  | { t: "move"; base: THREE.Vector3; ids: number[]; delta: THREE.Vector3; copy: boolean }
  | { t: "rotate"; center: THREE.Vector3; n: THREE.Vector3; ref: THREE.Vector3 | null; angle: number; ids: number[]; copy?: boolean }
  | { t: "scale"; center: THREE.Vector3; start: THREE.Vector3; factor: number; ids: number[] }
  | { t: "tape"; p0: THREE.Vector3; along: THREE.Vector3 | null; cur: THREE.Vector3 }
  | { t: "box"; x0: number; y0: number; x1: number; y1: number; mode: SelMode }
  | { t: "dim"; a: THREE.Vector3; b: THREE.Vector3 | null; off: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "text"; leader: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "wall"; pts: THREE.Vector3[]; cur: THREE.Vector3 }
  | { t: "stairs"; at: THREE.Vector3; cur: THREE.Vector3 };

interface LabelSpec {
  key: string;
  p: THREE.Vector3;
  text: string;
  live?: boolean;
  /** annotation id, for the delete button */
  id?: number;
}

type Last =
  | { t: "push"; face: number; d: number }
  | { t: "move"; ids: number[]; by: THREE.Vector3; copy: boolean }
  | { t: "rotate"; ids: number[]; center: THREE.Vector3; n: THREE.Vector3; angle: number }
  | { t: "scale"; ids: number[]; center: THREE.Vector3; factor: number }
  | null;

export default function Viewport(props: Props) {
  const { scene, tool, styles, placeAsset } = props;
  const host = useRef<HTMLDivElement>(null);
  const labelLayer = useRef<HTMLDivElement>(null);
  const [hint, setHint] = useState<{ x: number; y: number; label: string; kind: string } | null>(null);
  const [box, setBox] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);

  const live = useRef(props);
  live.current = props;

  const t = useRef<{
    renderer: THREE.WebGLRenderer;
    persp: THREE.PerspectiveCamera;
    ortho: THREE.OrthographicCamera;
    camera: THREE.PerspectiveCamera | THREE.OrthographicCamera;
    controls: OrbitControls;
    world: THREE.Scene;
    sun: THREE.DirectionalLight;
    model: THREE.Group;
    overlay: THREE.Group;
    preview: THREE.Group;
    ground: THREE.Group;
    catcher: THREE.Mesh;
    pickMesh: THREE.Mesh | null;
    editMesh: THREE.Mesh | null;
    groundZ: number;
  } | null>(null);

  /** newest section-fill request (older answers are dropped) */
  const sectionTicket = useRef(0);
  /** ambient occlusion post-processing, made the first time it's turned on */
  const ao = useRef<{ composer: EffectComposer; rp: RenderPass; gt: GTAOPass } | null>(null);
  const [backdrop, setBackdrop] = useState<string | null>(null);
  /** what the current tool wants next (shown top-left) */
  const [coach, setCoach] = useState<Guide | null>(null);
  const coachKey = useRef("");
  /** the tool's icon riding along with the cursor */
  const badge = useRef<HTMLDivElement>(null);
  const backdropOn = useRef(false);
  const st = useRef<{
    op: Op | null;
    last: Last;
    lock: Axis | null;
    hover: number | null;
    hoverEdge: number | null;
    /** last inference shown (Shift locks to it) */
    snap: Snap | null;
    /** the lock came from holding Shift (released with it) */
    shiftLock: boolean;
    /** Zoom tool drag */
    zoomY: number | null;
    space: boolean;
    spaceUsed: boolean;
    fitted: boolean;
    fitNext: boolean;
    section: THREE.Plane | null;
    segs: { circle: number; polygon: number };
    rightDown: { x: number; y: number } | null;
    selOverride: Sel | null;
    press: { x: number; y: number; fresh: boolean } | null;
    labels: LabelSpec[];
    liveLabels: LabelSpec[];
    labelEls: Map<string, HTMLDivElement>;
  }>({ press: null, labels: [], liveLabels: [], labelEls: new Map(), op: null, last: null, lock: null, hover: null, hoverEdge: null, snap: null, shiftLock: false, zoomY: null, space: false, spaceUsed: false, fitted: false, fitNext: false, section: null, segs: { circle: 24, polygon: 6 }, rightDown: null, selOverride: null });

  // dev builds: inspect the tool state from the console (window.__sketchcraft)
  if (import.meta.env.DEV) (window as unknown as { __sketchcraft: unknown }).__sketchcraft = st;

  // ---------------------------------------------------------------- setup
  useEffect(() => {
    const el = host.current!;
    // alpha: a matched photo can show through behind the model
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setClearColor(0xf3f4f8);
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    renderer.localClippingEnabled = true;
    el.appendChild(renderer.domElement);

    const persp = new THREE.PerspectiveCamera(40, 1, 0.05, 20000);
    persp.up.set(0, 0, 1);
    persp.position.set(30, -40, 28);
    const ortho = new THREE.OrthographicCamera(-10, 10, 10, -10, -10000, 10000);
    ortho.up.set(0, 0, 1);
    const controls = new OrbitControls(persp, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.14;
    controls.screenSpacePanning = true;
    controls.zoomToCursor = true;
    controls.enableZoom = false; // our own wheel handler: trackpads orbit, pinch zooms

    // Trackpad: two-finger swipe orbits, Shift+swipe pans, pinch zooms toward the cursor.
    // Mouse wheel: zooms toward the cursor.
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const T = t.current;
      if (!T) return;
      const cam = T.camera;
      const target = T.controls.target;
      const pinch = e.ctrlKey; // macOS reports pinch as ctrl+wheel
      const trackpad = !pinch && e.deltaMode === 0 && (Math.abs(e.deltaX) > 0 || Math.abs(e.deltaY) < 40);
      if (trackpad && e.shiftKey) {
        // pan
        const dist = cam.position.distanceTo(target);
        const k = dist * 0.0016;
        const right = new THREE.Vector3().setFromMatrixColumn(cam.matrix, 0);
        const up = new THREE.Vector3().setFromMatrixColumn(cam.matrix, 1);
        const d = right.multiplyScalar(e.deltaX * k).add(up.multiplyScalar(-e.deltaY * k));
        cam.position.add(d);
        target.add(d);
      } else if (trackpad) {
        // orbit around the target, Z up
        const off = cam.position.clone().sub(target);
        const r = off.length();
        const az = Math.atan2(off.y, off.x) - e.deltaX * 0.006;
        let el = Math.asin(THREE.MathUtils.clamp(off.z / r, -1, 1)) + e.deltaY * 0.006;
        el = THREE.MathUtils.clamp(el, -1.5, 1.5);
        cam.position.set(target.x + r * Math.cos(el) * Math.cos(az), target.y + r * Math.cos(el) * Math.sin(az), target.z + r * Math.sin(el));
        cam.lookAt(target);
      } else {
        // zoom toward what's under the cursor
        const f = Math.exp((pinch ? e.deltaY * 0.012 : Math.sign(e.deltaY) * 0.12));
        const rc = new THREE.Raycaster();
        const rect = renderer.domElement.getBoundingClientRect();
        rc.setFromCamera(new THREE.Vector2(((e.clientX - rect.left) / rect.width) * 2 - 1, -((e.clientY - rect.top) / rect.height) * 2 + 1), cam);
        const focus = rc.ray.intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(cam.getWorldDirection(new THREE.Vector3()).negate(), target), new THREE.Vector3()) ?? target.clone();
        if (cam === T.ortho) {
          T.ortho.zoom = THREE.MathUtils.clamp(T.ortho.zoom / f, 0.02, 200);
          T.ortho.updateProjectionMatrix();
        } else {
          const minR = 0.3;
          if (f < 1 && cam.position.distanceTo(target) * f < minR) return;
          cam.position.sub(focus).multiplyScalar(f).add(focus);
          target.sub(focus).multiplyScalar(f).add(focus);
        }
      }
      if (cam === T.ortho) T.persp.position.copy(cam.position);
    };
    renderer.domElement.addEventListener("wheel", wheel, { passive: false });
    // moving the camera leaves a matched photo's view (as in SketchUp)
    controls.addEventListener("start", () => backdropOn.current && showBackdrop(null));
    renderer.domElement.addEventListener("wheel", () => backdropOn.current && showBackdrop(null));

    const world = new THREE.Scene();
    world.add(new THREE.HemisphereLight(0xffffff, 0xe2e4ec, 2.7));
    const sun = new THREE.DirectionalLight(0xffffff, 1.3);
    sun.position.set(30, -50, 80);
    sun.shadow.mapSize.set(2048, 2048);
    sun.shadow.bias = -0.0004;
    sun.shadow.normalBias = 0.02; // no speckles on gently sloped ground
    world.add(sun, sun.target);

    const ground = new THREE.Group();
    const grid = new THREE.GridHelper(600, 600, 0xd5d8e2, 0xe4e6ee);
    grid.rotation.x = Math.PI / 2;
    (grid.material as THREE.Material).depthWrite = false;
    ground.add(grid);
    for (const [to, c] of [[[300, 0, 0], AXIS_COLORS[0]], [[0, 300, 0], AXIS_COLORS[1]], [[0, 0, 300], AXIS_COLORS[2]]] as [Vec3, number][]) {
      const g = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), V(to)]);
      ground.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: c, transparent: true, opacity: 0.55 })));
    }
    const catcher = new THREE.Mesh(new THREE.PlaneGeometry(2000, 2000), new THREE.ShadowMaterial({ opacity: 0.22 }));
    catcher.receiveShadow = true;
    catcher.visible = false;
    ground.add(catcher);
    world.add(ground);

    const model = new THREE.Group();
    const overlay = new THREE.Group();
    const preview = new THREE.Group();
    world.add(model, overlay, preview);

    t.current = { renderer, persp, ortho, camera: persp, controls, world, sun, model, overlay, preview, ground, catcher, pickMesh: null, editMesh: null, groundZ: 0 };

    const resize = () => {
      const { clientWidth: w, clientHeight: h } = el;
      renderer.setSize(w, h);
      ao.current?.composer.setSize(w, h);
      ao.current?.gt.setSize(w, h);
      persp.aspect = w / Math.max(h, 1);
      persp.updateProjectionMatrix();
      syncOrtho(false);
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();

    let raf = 0;
    const loop = () => {
      controls.update();
      const cam = t.current!.camera;
      if (live.current.styles.ao) {
        // ambient occlusion: render through a post-processing chain (built on first use)
        if (!ao.current) {
          const size = renderer.getSize(new THREE.Vector2());
          const composer = new EffectComposer(renderer);
          const rp = new RenderPass(world, cam);
          const gt = new GTAOPass(world, cam, size.x, size.y);
          gt.output = GTAOPass.OUTPUT.Default;
          composer.addPass(rp);
          composer.addPass(gt);
          composer.addPass(new OutputPass());
          composer.setPixelRatio(renderer.getPixelRatio());
          composer.setSize(size.x, size.y);
          ao.current = { composer, rp, gt };
        }
        ao.current.rp.camera = cam;
        ao.current.gt.camera = cam;
        ao.current.composer.render();
      } else renderer.render(world, cam);
      updateCoach();
      placeLabels();
      raf = requestAnimationFrame(loop);
    };
    loop();
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      renderer.dispose();
      el.removeChild(renderer.domElement);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Frame the parallel camera like the perspective one at the same distance. */
  function syncOrtho(copyPose: boolean) {
    const T = t.current;
    if (!T) return;
    const el = host.current!;
    const aspect = el.clientWidth / Math.max(el.clientHeight, 1);
    if (copyPose) {
      T.ortho.position.copy(T.persp.position);
      T.ortho.quaternion.copy(T.persp.quaternion);
      T.ortho.zoom = 1;
    }
    const dist = T.ortho.position.distanceTo(T.controls.target);
    const h = Math.max(dist, 1) * Math.tan(THREE.MathUtils.degToRad(T.persp.fov / 2));
    T.ortho.left = -h * aspect;
    T.ortho.right = h * aspect;
    T.ortho.top = h;
    T.ortho.bottom = -h;
    T.ortho.updateProjectionMatrix();
  }

  // ---------------------------------------------------------------- styles
  useEffect(() => {
    const T = t.current;
    if (!T) return;
    const want = styles.parallel ? T.ortho : T.persp;
    if (T.camera !== want) {
      if (styles.parallel) syncOrtho(true);
      else {
        T.persp.position.copy(T.ortho.position);
        T.persp.quaternion.copy(T.ortho.quaternion);
      }
      T.camera = want;
      T.controls.object = want;
      T.controls.update();
    }
    T.renderer.shadowMap.enabled = styles.shadows;
    T.sun.castShadow = styles.shadows;
    T.catcher.visible = styles.shadows;
    buildModel();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [styles.xray, styles.shadows, styles.edges, styles.parallel, styles.backEdges, styles.face, styles.profiles, styles.sketchy, styles.extensions, styles.background]);

  // ---------------------------------------------------------------- labels
  /** Keep one DOM label per spec; positions are updated every frame. */
  function syncLabels() {
    const layer = labelLayer.current;
    if (!layer) return;
    const specs = [...st.current.labels, ...st.current.liveLabels];
    const keep = new Set(specs.map((x) => x.key));
    for (const [k, el] of st.current.labelEls) {
      if (!keep.has(k)) {
        el.remove();
        st.current.labelEls.delete(k);
      }
    }
    for (const spec of specs) {
      let el = st.current.labelEls.get(spec.key);
      if (!el) {
        el = document.createElement("div");
        el.className = spec.live ? "label live" : "label";
        const span = document.createElement("span");
        el.appendChild(span);
        if (spec.id != null) {
          const x = document.createElement("button");
          x.textContent = "×";
          x.title = "Delete";
          const id = spec.id;
          x.onclick = (e) => {
            e.stopPropagation();
            live.current.run({ cmd: "delete", ids: [id] });
          };
          el.appendChild(x);
        }
        layer.appendChild(el);
        st.current.labelEls.set(spec.key, el);
      }
      (el.firstChild as HTMLSpanElement).textContent = spec.text;
    }
  }

  function placeLabels() {
    const T = t.current;
    if (!T || !st.current.labelEls.size) return;
    const r = T.renderer.domElement;
    const w = r.clientWidth;
    const h = r.clientHeight;
    for (const spec of [...st.current.labels, ...st.current.liveLabels]) {
      const el = st.current.labelEls.get(spec.key);
      if (!el) continue;
      const q = spec.p.clone().project(T.camera);
      const hidden = q.z > 1 || (st.current.section != null && !spec.live && st.current.section.distanceToPoint(spec.p) < -0.01);
      el.style.display = hidden ? "none" : "";
      el.style.transform = `translate(${(q.x * 0.5 + 0.5) * w}px, ${(-q.y * 0.5 + 0.5) * h}px) translate(-50%, -50%)`;
    }
  }

  function setLive(specs: { p: THREE.Vector3; text: string }[]) {
    st.current.liveLabels = specs.map((x, i) => ({ key: `live${i}`, p: x.p, text: x.text, live: true }));
    syncLabels();
  }

  /** Dimension lines (extension lines, dimension line, ticks) as line segments. */
  function dimSegments(a: THREE.Vector3, b: THREE.Vector3, off: THREE.Vector3): number[] {
    const a2 = a.clone().add(off);
    const b2 = b.clone().add(off);
    const out: number[] = [];
    const seg = (p: THREE.Vector3, q: THREE.Vector3) => out.push(p.x, p.y, p.z, q.x, q.y, q.z);
    if (off.length() > 1e-6) {
      const o = off.clone().normalize();
      seg(a.clone().addScaledVector(o, Math.min(0.1, off.length() * 0.2)), a2.clone().addScaledVector(o, 0.12));
      seg(b.clone().addScaledVector(o, Math.min(0.1, off.length() * 0.2)), b2.clone().addScaledVector(o, 0.12));
    }
    seg(a2, b2);
    // architectural ticks
    const u = b2.clone().sub(a2).normalize();
    const o = off.length() > 1e-6 ? off.clone().normalize() : new THREE.Vector3(0, 0, 1);
    const tick = u.clone().add(o).normalize().multiplyScalar(0.12);
    for (const p of [a2, b2]) seg(p.clone().sub(tick), p.clone().add(tick));
    return out;
  }

  function annotationSpecs(): LabelSpec[] {
    const s = live.current.scene;
    if (!s) return [];
    return s.annotations.flatMap((an) =>
      an.kind === "dimension"
        ? [{ key: `a${an.id}`, id: an.id, p: V(an.a).add(V(an.b)).multiplyScalar(0.5).add(V(an.offset)), text: fmt(V(an.a).distanceTo(V(an.b))) }]
        : an.kind === "text"
          ? [{ key: `a${an.id}`, id: an.id, p: V(an.at), text: an.text }]
          : [] // door swings are drawn, not labeled
    );
  }

  // ---------------------------------------------------------------- model
  function buildModel() {
    const T = t.current;
    const s = live.current.scene;
    if (!T || !s) return;
    T.model.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.model.clear();
    const xray = live.current.styles.xray;
    const clip = st.current.section ? [st.current.section] : [];

    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(s.positions, 3));
    // colors come per triangle; each corner gets its triangle's color
    const corners = (c: number[]) => {
      const out = new Float32Array(c.length * 3);
      for (let t = 0; t < c.length / 3; t++) for (let k = 0; k < 3; k++) out.set([c[t * 3], c[t * 3 + 1], c[t * 3 + 2]], t * 9 + k * 3);
      return new THREE.BufferAttribute(out, 3);
    };
    geo.setAttribute("color", corners(s.colors));
    geo.setAttribute("normal", new THREE.BufferAttribute(smoothNormals(s.positions), 3));
    const bgeo = geo.clone();
    bgeo.setAttribute("color", corners(s.backColors));
    const sty = live.current.styles;
    const faceStyle: FaceStyle = sty.face ?? "textured";
    const bg = BACKGROUNDS[sty.background ?? "default"];
    const common = { side: THREE.FrontSide as THREE.Side, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1, transparent: xray, opacity: xray ? 0.42 : 1, depthWrite: !xray, clippingPlanes: clip };
    const mat = (side: THREE.Side): THREE.Material => {
      if (faceStyle === "hiddenline" || faceStyle === "wireframe") {
        // flat background-colored faces hide what's behind (wireframe: invisible, still clickable)
        const m = new THREE.MeshBasicMaterial({ ...common, side, color: bg.face });
        if (faceStyle === "wireframe") m.visible = false;
        return m;
      }
      if (faceStyle === "monochrome") return new THREE.MeshLambertMaterial({ ...common, side, color: 0xeceef3 });
      return new THREE.MeshLambertMaterial({ ...common, side, vertexColors: true });
    };
    const front = new THREE.Mesh(geo, mat(THREE.FrontSide));
    const back = new THREE.Mesh(bgeo, mat(THREE.BackSide));
    front.castShadow = back.castShadow = live.current.styles.shadows;
    front.receiveShadow = live.current.styles.shadows;
    T.model.add(front, back);

    // profiles: an outline around every shape
    const box = new THREE.Box3().setFromBufferAttribute(geo.getAttribute("position") as THREE.BufferAttribute);
    const size = box.isEmpty() ? 10 : box.getSize(new THREE.Vector3()).length();
    if (sty.profiles && !xray && faceStyle !== "wireframe" && s.positions.length) {
      const pg = new THREE.BufferGeometry();
      pg.setAttribute("position", new THREE.Float32BufferAttribute(s.positions, 3));
      pg.setAttribute("normal", new THREE.BufferAttribute(averagedNormals(s.positions), 3));
      const outline = new THREE.Mesh(pg, profileMaterial(bg.line, 2.2, clip));
      outline.raycast = () => {}; // not pickable
      T.model.add(outline);
    }

    // textured faces: drawn over the base mesh with their images
    if (faceStyle === "textured" && s.triTex?.some((i) => i >= 0)) {
      const byTex = new Map<number, { pos: number[]; uv: number[] }>();
      s.triTex.forEach((ti, tri) => {
        if (ti < 0) return;
        const g = byTex.get(ti) ?? { pos: [], uv: [] };
        for (let k = 0; k < 9; k++) g.pos.push(s.positions[tri * 9 + k]);
        for (let k = 0; k < 6; k++) g.uv.push(s.uvs[tri * 6 + k]);
        byTex.set(ti, g);
      });
      for (const [ti, g] of byTex) {
        const mat = s.materials.find((m) => m.name === s.textures[ti]);
        if (!mat?.texture) continue;
        const geo2 = new THREE.BufferGeometry();
        geo2.setAttribute("position", new THREE.Float32BufferAttribute(g.pos, 3));
        geo2.setAttribute("uv", new THREE.Float32BufferAttribute(g.uv, 2));
        geo2.setAttribute("normal", new THREE.BufferAttribute(smoothNormals(g.pos), 3));
        const mesh = new THREE.Mesh(
          geo2,
          new THREE.MeshLambertMaterial({ map: textureFor(mat.name, mat.texture.data), side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 0, polygonOffsetUnits: 0, transparent: xray, opacity: xray ? 0.42 : 1, depthWrite: !xray, clippingPlanes: clip })
        );
        mesh.receiveShadow = live.current.styles.shadows;
        T.model.add(mesh);
      }
    }

    if (sty.edges || faceStyle === "wireframe" || faceStyle === "hiddenline") {
      const eg = new THREE.BufferGeometry();
      const segs = sty.sketchy || sty.extensions ? styledEdges(s.edges, size, !!sty.extensions || !!sty.sketchy, !!sty.sketchy) : s.edges;
      eg.setAttribute("position", new THREE.Float32BufferAttribute(segs, 3));
      T.model.add(new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color: bg.line, clippingPlanes: clip, transparent: !!sty.sketchy, opacity: sty.sketchy ? 0.8 : 1 })));
    }
    // background and ground for the style (a matched photo overrides)
    if (!backdropOn.current) {
      T.renderer.setClearColor(bg.clear, 1);
      T.ground.visible = bg.grid;
    }
    // section fill: the cut faces of walls and solids, dark, with a heavy outline
    if (st.current.section) {
      const plane = st.current.section;
      const o = plane.coplanarPoint(new THREE.Vector3());
      const ticket = ++sectionTicket.current;
      void exec({ cmd: "section", origin: [o.x, o.y, o.z], normal: [plane.normal.x, plane.normal.y, plane.normal.z] }).then((r) => {
        const T2 = t.current;
        if (!r.ok || !T2 || ticket !== sectionTicket.current) return;
        const fill = r.fill as number[];
        const lines = r.lines as number[];
        // nudge toward the viewer side so the cap isn't clipped away
        const lift = plane.normal.clone().multiplyScalar(0.002);
        const shift = (a: number[]) => a.map((v, i) => v + [lift.x, lift.y, lift.z][i % 3]);
        if (fill.length) {
          const g = new THREE.BufferGeometry();
          g.setAttribute("position", new THREE.Float32BufferAttribute(shift(fill), 3));
          const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: 0x3a3d4a, side: THREE.DoubleSide }));
          m.userData.sectionFill = true;
          T2.model.add(m);
        }
        if (lines.length) {
          const g = new THREE.BufferGeometry();
          g.setAttribute("position", new THREE.Float32BufferAttribute(shift(lines), 3));
          T2.model.add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: 0x0b0c10 })));
        }
      });
    }
    if (live.current.styles.backEdges && !xray && s.edges.length) {
      // the same edges again, dashed and drawn through faces
      const bg = new THREE.BufferGeometry();
      bg.setAttribute("position", new THREE.Float32BufferAttribute(s.edges, 3));
      const back = new THREE.LineSegments(bg, new THREE.LineDashedMaterial({ color: 0x7a7f94, dashSize: 0.12, gapSize: 0.09, depthTest: false, depthWrite: false, transparent: true, opacity: 0.55, clippingPlanes: clip }));
      back.computeLineDistances();
      back.renderOrder = 5; // after the faces, so they show through
      T.model.add(back);
    }
    if (xray && s.softEdges.length) {
      const sg = new THREE.BufferGeometry();
      sg.setAttribute("position", new THREE.Float32BufferAttribute(s.softEdges, 3));
      const soft = new THREE.LineSegments(sg, new THREE.LineDashedMaterial({ color: 0x8a8ea0, dashSize: 0.2, gapSize: 0.15 }));
      soft.computeLineDistances();
      T.model.add(soft);
    }
    // guides: long dashed lines and little crosses
    for (const g of s.guides) {
      const p = V(g.p);
      const pts = g.dir
        ? [p.clone().addScaledVector(V(g.dir), -500), p.clone().addScaledVector(V(g.dir), 500)]
        : [p.clone().add(new THREE.Vector3(-0.15, 0, 0)), p.clone().add(new THREE.Vector3(0.15, 0, 0)), p.clone().add(new THREE.Vector3(0, -0.15, 0)), p.clone().add(new THREE.Vector3(0, 0.15, 0))];
      const line = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineDashedMaterial({ color: 0x3e63dd, dashSize: 0.3, gapSize: 0.2 }));
      line.computeLineDistances();
      T.model.add(line);
    }
    // dimensions and text leaders, always on top
    const ann: number[] = [];
    for (const an of s.annotations) {
      if (an.kind === "dimension") ann.push(...dimSegments(V(an.a), V(an.b), V(an.offset)));
      else if (an.kind === "swing") {
        // the open leaf and the arc its edge sweeps
        const h = V(an.hinge), a = V(an.from).sub(h), b = V(an.to).sub(h);
        ann.push(...an.hinge, ...an.to);
        const ang = a.angleTo(b);
        const axis = a.clone().cross(b).normalize();
        let prev = V(an.from);
        for (let k = 1; k <= 16; k++) {
          const p = a.clone().applyAxisAngle(axis.lengthSq() ? axis : new THREE.Vector3(0, 0, 1), (ang * k) / 16).add(h);
          ann.push(prev.x, prev.y, prev.z, p.x, p.y, p.z);
          prev = p;
        }
      } else if (an.leader) ann.push(...an.leader, ...an.at);
    }
    if (ann.length) {
      const ag = new THREE.BufferGeometry();
      ag.setAttribute("position", new THREE.Float32BufferAttribute(ann, 3));
      const al = new THREE.LineSegments(ag, new THREE.LineBasicMaterial({ color: (live.current.styles.background ?? "default") === "default" ? 0x33384d : BACKGROUNDS[live.current.styles.background!].line, depthTest: false }));
      al.renderOrder = 10;
      T.model.add(al);
    }
    st.current.labels = annotationSpecs();
    syncLabels();
    T.pickMesh = front;

    T.editMesh = null;
    if (s.editing != null && s.editPositions.length) {
      const eg = new THREE.BufferGeometry();
      eg.setAttribute("position", new THREE.Float32BufferAttribute(s.editPositions, 3));
      T.editMesh = new THREE.Mesh(eg, new THREE.MeshBasicMaterial({ visible: false, side: THREE.DoubleSide }));
      T.model.add(T.editMesh);
    }

    let minZ = Infinity;
    const bb = new THREE.Box3();
    for (let i = 0; i < s.positions.length; i += 3) {
      minZ = Math.min(minZ, s.positions[i + 2]);
      bb.expandByPoint(new THREE.Vector3(s.positions[i], s.positions[i + 1], s.positions[i + 2]));
    }
    T.groundZ = Number.isFinite(minZ) ? Math.min(minZ, 0) : 0;
    T.ground.position.z = T.groundZ - 0.01;
    if (!bb.isEmpty()) {
      const c = bb.getCenter(new THREE.Vector3());
      const r = bb.getSize(new THREE.Vector3()).length() / 2 + 5;
      T.sun.position.copy(c.clone().add(new THREE.Vector3(0.35, -0.55, 0.76).multiplyScalar(r * 2)));
      T.sun.target.position.copy(c);
      const sc = T.sun.shadow.camera;
      sc.left = sc.bottom = -r;
      sc.right = sc.top = r;
      sc.near = 0.1;
      sc.far = r * 5;
      sc.updateProjectionMatrix();
    }
  }

  useEffect(() => {
    if (!t.current || !scene) return;
    units = scene.units || "m";
    buildModel();
    if (!st.current.fitted || st.current.fitNext) {
      st.current.fitted = true;
      st.current.fitNext = false;
      if (scene.positions.length) fit();
    }
    drawOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene]);

  useEffect(() => {
    st.current.selOverride = null;
    drawOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.sel]);

  useEffect(() => {
    if (!t.current) return;
    cancel();
    setOrbitButtons();
    drawOverlay(); // the hover preview depends on the tool (red for erase, paint color...)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tool]);

  function setOrbitButtons() {
    const T = t.current!;
    T.controls.mouseButtons = {
      LEFT: st.current.space || live.current.tool === "orbit" ? THREE.MOUSE.ROTATE : live.current.tool === "pan" ? THREE.MOUSE.PAN : (null as unknown as THREE.MOUSE),
      MIDDLE: THREE.MOUSE.ROTATE,
      RIGHT: THREE.MOUSE.PAN,
    };
  }

  // ---------------------------------------------------------------- helpers
  const sceneNow = () => live.current.scene!;
  const selNow = () => st.current.selOverride ?? live.current.sel;
  const faceById = (id: number) => sceneNow().faces.find((f) => f.id === id) ?? null;
  const groupOfFace = (id: number) => faceById(id)?.group ?? null;

  function rect() {
    return t.current!.renderer.domElement.getBoundingClientRect();
  }

  function env(): InferEnv {
    const T = t.current!;
    return { camera: T.camera, rect: rect(), scene: sceneNow(), pick, groundZ: T.groundZ };
  }

  function ray(e: { clientX: number; clientY: number }): THREE.Ray {
    return rayFrom({ camera: t.current!.camera, rect: rect() }, e);
  }

  function pick(e: { clientX: number; clientY: number }): Pick | null {
    const T = t.current!;
    const s = live.current.scene;
    if (!T.pickMesh || !s) return null;
    const rc = new THREE.Raycaster();
    rc.ray.copy(ray(e));
    const mat = T.pickMesh.material as THREE.Material;
    const side = mat.side;
    mat.side = THREE.DoubleSide;
    let hits = rc.intersectObjects(T.editMesh ? [T.editMesh, T.pickMesh] : [T.pickMesh], false);
    mat.side = side;
    // ignore what the section plane cut away
    if (st.current.section) hits = hits.filter((h) => st.current.section!.distanceToPoint(h.point) >= -1e-4);
    const hit = hits[0];
    if (!hit || hit.faceIndex == null) return null;
    const owner = hit.object === T.editMesh ? s.editTriFace[hit.faceIndex] : s.triFace[hit.faceIndex];
    return { face: faceById(owner), owner, point: hit.point.clone(), distance: hit.distance };
  }

  /** Editable edge under the cursor (within 5 px, not hidden behind a face). */
  function pickEdge(e: { clientX: number; clientY: number }): number | null {
    const s = sceneNow();
    const T = t.current!;
    const r = rect();
    const m = new THREE.Vector2(e.clientX - r.left, e.clientY - r.top);
    const hit = pick(e);
    const R = ray(e);
    const sc = (p: THREE.Vector3) => {
      const q = p.clone().project(T.camera);
      return new THREE.Vector2((q.x * 0.5 + 0.5) * r.width, (-q.y * 0.5 + 0.5) * r.height);
    };
    let best: { id: number; d: number } | null = null;
    for (const ed of s.edgeList) {
      if (ed.soft && !live.current.styles.xray && live.current.tool !== "flipedge") continue;
      const a = sc(V(ed.a));
      const b = sc(V(ed.b));
      const ab = b.clone().sub(a);
      const tt = Math.max(0, Math.min(1, m.clone().sub(a).dot(ab) / Math.max(ab.lengthSq(), 1e-9)));
      const d = a.clone().addScaledVector(ab, tt).distanceTo(m);
      if (d > 5 || (best && d >= best.d)) continue;
      const q = closestOnLine(R, V(ed.a), V(ed.b).sub(V(ed.a)).normalize());
      if (hit && q.clone().sub(R.origin).dot(R.direction) > hit.distance * 1.02 + 0.15) continue;
      best = { id: ed.id, d };
    }
    return best ? (best as { id: number }).id : null;
  }

  /** Triangles owned by these faces / instances (for highlights and ghosts). */
  function trianglesOf(owners: Set<number>): number[] {
    const s = sceneNow();
    const out: number[] = [];
    const scan = (pos: number[], tri: number[]) => {
      tri.forEach((o, i) => {
        if (owners.has(o)) for (let k = 0; k < 9; k++) out.push(pos[i * 9 + k]);
      });
    };
    scan(s.positions, s.triFace);
    if (s.editing != null) scan(s.editPositions, s.editTriFace);
    return out;
  }

  function groupFaces(g: number): Set<number> {
    return new Set(sceneNow().faces.filter((f) => f.group === g).map((f) => f.id));
  }

  function ownersOf(sel: Sel): Set<number> {
    const o = new Set<number>([...sel.faces, ...sel.instances]);
    for (const g of sel.groups) groupFaces(g).forEach((f) => o.add(f));
    return o;
  }

  function selectionLines(sel: Sel): number[] {
    const out: number[] = [];
    const ids = new Set(sel.edges);
    for (const e of sceneNow().edgeList) if (ids.has(e.id)) out.push(...e.a, ...e.b);
    return out;
  }

  function overlayMesh(pos: number[], opacity: number, color: number = ACCENT): THREE.Mesh {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    return new THREE.Mesh(
      g,
      new THREE.MeshBasicMaterial({ color, transparent: true, opacity, side: THREE.DoubleSide, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 })
    );
  }

  function lines(pos: number[], color = ACCENT, depthTest = true): THREE.LineSegments {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    return new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color, depthTest }));
  }

  /** A 2 px preview line that's easy to see (WebGL's own lines are 1 px). */
  function polyline(points: THREE.Vector3[], color = ACCENT, loop = false, dashed = false): Line2 {
    const pts = loop ? [...points, points[0]] : points;
    const g = new LineGeometry();
    g.setPositions(pts.flatMap((p) => [p.x, p.y, p.z]));
    const el = t.current!.renderer.domElement;
    const m = new LineMaterial({ color, linewidth: dashed ? 1.5 : 2.2, depthTest: false, dashed, dashSize: 0.25, gapSize: 0.18 });
    m.resolution.set(el.clientWidth, el.clientHeight);
    const l = new Line2(g, m);
    if (dashed) l.computeLineDistances();
    l.renderOrder = 20;
    return l;
  }

  function drawOverlay() {
    const T = t.current;
    const s = live.current.scene;
    if (!T || !s) return;
    T.overlay.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.overlay.clear();
    const sel = selNow();
    const owners = ownersOf(sel);
    if (owners.size) T.overlay.add(overlayMesh(trianglesOf(owners), 0.24));
    const sl = selectionLines(sel);
    if (sl.length) T.overlay.add(lines(sl, ACCENT, false));
    // a box around whole groups / components
    for (const id of [...sel.groups, ...sel.instances]) {
      const pos = trianglesOf(sel.groups.includes(id) ? groupFaces(id) : new Set([id]));
      if (!pos.length) continue;
      const b = new THREE.Box3().setFromBufferAttribute(new THREE.Float32BufferAttribute(pos, 3));
      T.overlay.add(new THREE.Box3Helper(b, new THREE.Color(ACCENT)));
    }
    // what a click would do, shown on what it would do it to
    const tool = live.current.tool;
    const RED = 0xe5484d;
    const h = st.current.hover;
    if (h != null && (!owners.has(h) || tool !== "select")) {
      const hp = trianglesOf(tool === "push" || tool === "offset" ? new Set([h]) : hoverOwners(h));
      if (hp.length) {
        if (tool === "erase") T.overlay.add(overlayMesh(hp, 0.35, RED));
        else if (tool === "paint") {
          const c = live.current.color;
          T.overlay.add(overlayMesh(hp, 0.7, new THREE.Color(c[0], c[1], c[2]).getHex()));
        } else if (tool === "push" || tool === "offset") T.overlay.add(overlayMesh(hp, 0.22));
        else T.overlay.add(overlayMesh(hp, 0.1));
      }
    }
    const he = st.current.hoverEdge;
    if (he != null) {
      const e = s.edgeList.find((x) => x.id === he);
      if (e) T.overlay.add(lines([...e.a, ...e.b], tool === "erase" ? RED : ACCENT, false));
    }
  }

  /** Work out the guidance for the tool and step in progress. */
  function updateCoach() {
    const L = live.current;
    const op = st.current.op;
    const stage = op?.t === "arc" ? (op.b ? 2 : 1) : op?.t === "rotate" ? (op.ref ? 2 : 1) : op?.t === "dim" ? (op.b ? 2 : 1) : undefined;
    const g = guide(L.tool, { op: op?.t ?? null, stage, selected: selIds(selNow()).length, placing: L.placeAsset });
    const key = `${g.title}|${g.step}|${g.keys.length}`;
    if (key !== coachKey.current) {
      coachKey.current = key;
      setCoach(g);
    }
  }

  /** What hovering this owner would select (a whole group, unless it's open). */
  function hoverOwners(owner: number): Set<number> {
    const g = groupOfFace(owner);
    if (g != null && g !== sceneNow().activeGroup) return groupFaces(g);
    return new Set([owner]);
  }

  /** The selectable thing for a pick: a component placement, a closed group, or a face. */
  function entityOf(p: Pick): { kind: keyof Sel; id: number } {
    const s = sceneNow();
    if (!p.face && s.instances.some((i) => i.id === p.owner)) return { kind: "instances", id: p.owner };
    const g = groupOfFace(p.owner);
    if (g != null && g !== s.activeGroup) return { kind: "groups", id: g };
    return { kind: "faces", id: p.owner };
  }

  function clearPreview() {
    const T = t.current;
    if (!T) return;
    T.preview.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.preview.clear();
    if (st.current.liveLabels.length) setLive([]);
  }

  function cancel() {
    st.current.op = null;
    st.current.lock = null;
    clearPreview();
    setHint(null);
    setBox(null);
    live.current.onMeasure("", "");
  }

  function toScreen2(p: THREE.Vector3) {
    const r = rect();
    const q = p.clone().project(t.current!.camera);
    return { x: (q.x * 0.5 + 0.5) * r.width, y: (-q.y * 0.5 + 0.5) * r.height };
  }

  /** Move the camera toward (f < 1) or away from the target. */
  function dolly(f: number) {
    const T = t.current;
    if (!T) return;
    if (T.camera === T.ortho) {
      T.ortho.zoom = THREE.MathUtils.clamp(T.ortho.zoom / f, 0.02, 200);
      T.ortho.updateProjectionMatrix();
    } else {
      const target = T.controls.target;
      const off = T.camera.position.clone().sub(target).multiplyScalar(f);
      if (off.length() < 0.3) return;
      T.camera.position.copy(target).add(off);
    }
    T.controls.update();
  }

  /** Match Photo backdrop: the photo behind a see-through canvas, no grid. */
  function showBackdrop(photo: string | null) {
    const T = t.current;
    if (!T) return;
    setBackdrop(photo);
    backdropOn.current = !!photo;
    T.renderer.setClearColor(0xf3f4f8, photo ? 0 : 1);
    T.ground.visible = !photo;
  }

  function showSnap(sn: Snap) {
    st.current.snap = sn;
    if (sn.from) t.current!.preview.add(polyline([sn.from, sn.p], sn.axis != null ? AXIS_COLORS[sn.axis] : 0x8a8ea0, false, true));
    const r = rect();
    const q = sn.p.clone().project(t.current!.camera);
    setHint({ x: (q.x * 0.5 + 0.5) * r.width, y: (-q.y * 0.5 + 0.5) * r.height, label: sn.label, kind: sn.kind });
  }

  function axisColor(sn: Snap): number {
    return sn.kind === "axis" && sn.axis != null ? AXIS_COLORS[sn.axis] : 0x1d1e24;
  }

  async function run(cmd: Record<string, unknown>): Promise<Reply> {
    return live.current.run(toLocal(cmd));
  }

  /** Inside a component, the engine works in the component's own coordinates. */
  function toLocal(cmd: Record<string, unknown>): Record<string, unknown> {
    const s = sceneNow();
    if (s.editing == null) return cmd;
    const inv = new THREE.Matrix4().fromArray(s.editTransform).invert();
    const isV = (a: unknown): a is number[] => Array.isArray(a) && a.length === 3 && typeof a[0] === "number";
    const pt = (a: unknown) => (isV(a) ? arr(V(a).applyMatrix4(inv)) : a);
    const dir = (a: unknown) => (isV(a) ? arr(V(a).transformDirection(inv)) : a);
    const vec = (a: unknown) => (isV(a) ? arr(V(a).applyMatrix4(inv).sub(new THREE.Vector3().applyMatrix4(inv))) : a);
    const out: Record<string, unknown> = { ...cmd };
    for (const k of ["from", "to", "center", "start", "end", "at", "point"]) if (k in out) out[k] = pt(out[k]);
    for (const k of ["normal", "axis", "direction"]) if (k in out) out[k] = dir(out[k]);
    if ("by" in out) out.by = vec(out.by);
    if (Array.isArray(out.points)) out.points = (out.points as unknown[]).map(pt);
    return out;
  }

  // ---------------------------------------------------------------- camera
  function fit(owners?: Set<number>) {
    const T = t.current;
    const s = live.current.scene;
    if (!T) return;
    const b = new THREE.Box3();
    const pos = owners ? trianglesOf(owners) : s?.positions ?? [];
    for (let i = 0; i < pos.length; i += 3) b.expandByPoint(new THREE.Vector3(pos[i], pos[i + 1], pos[i + 2]));
    if (b.isEmpty()) b.set(new THREE.Vector3(-5, -5, 0), new THREE.Vector3(5, 5, 3));
    const c = b.getCenter(new THREE.Vector3());
    const rad = Math.max(b.getSize(new THREE.Vector3()).length() / 2, 1.5);
    const dir = T.camera.position.clone().sub(T.controls.target).normalize();
    T.controls.target.copy(c);
    T.persp.position.copy(c.clone().addScaledVector(dir, (rad / Math.sin(THREE.MathUtils.degToRad(T.persp.fov / 2))) * 1.05));
    if (T.camera === T.ortho) syncOrtho(true);
  }

  function view(name: "iso" | "top" | "front" | "right" | "back" | "left") {
    const T = t.current!;
    setFov(40);
    const dirs: Record<string, Vec3> = { iso: [1, -1.35, 0.95], top: [0, -0.001, 1], front: [0, -1, 0.08], back: [0, 1, 0.08], right: [1, 0, 0.08], left: [-1, 0, 0.08] };
    const d = V(dirs[name]).normalize();
    const dist = T.camera.position.distanceTo(T.controls.target);
    T.persp.position.copy(T.controls.target.clone().addScaledVector(d, dist));
    T.persp.lookAt(T.controls.target);
    fit();
  }

  function camera(): Cam {
    const T = t.current!;
    return { eye: arr(T.camera.position), target: arr(T.controls.target), fov: T.persp.fov };
  }

  function setFov(fov: number) {
    const T = t.current!;
    if (Math.abs(T.persp.fov - fov) < 1e-6) return;
    T.persp.fov = fov;
    T.persp.updateProjectionMatrix();
  }

  function goTo(c: Cam & { photo?: string }) {
    const T = t.current!;
    showBackdrop(c.photo ?? null);
    setFov(c.fov || 40); // a saved view keeps its lens
    const from = { e: T.camera.position.clone(), t: T.controls.target.clone() };
    const to = { e: V(c.eye), t: V(c.target) };
    const t0 = performance.now();
    const step = () => {
      const k = Math.min(1, (performance.now() - t0) / 450);
      const ease = k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2;
      T.camera.position.lerpVectors(from.e, to.e, ease);
      T.controls.target.lerpVectors(from.t, to.t, ease);
      if (T.camera === T.ortho) {
        T.persp.position.copy(T.camera.position);
        T.ortho.lookAt(T.controls.target);
        syncOrtho(false); // frame the parallel view for the new distance
      }
      if (k < 1) requestAnimationFrame(step);
    };
    step();
  }

  // ---------------------------------------------------------------- typed values
  async function applyTyped(text: string) {
    const s = text.trim();
    if (!s) return;
    const op = st.current.op;
    const nums = s.split(/[ ,;]+/).map(Number).filter((x) => !Number.isNaN(x));
    // lengths understand units: 8, 8m, 250cm, 8', 10'6", 96"
    const lens = parseLengths(s, units);
    const L = live.current;

    // "24s": sides for circles and polygons
    const sides = /^(\d+)\s*s$/i.exec(s);
    if (sides) {
      const k = Math.max(3, Number(sides[1]));
      if (L.tool === "polygon") st.current.segs.polygon = k;
      else st.current.segs.circle = k;
      if (op?.t === "circle") op.seg = k;
      L.onToast(`${k} sides`);
      return;
    }
    // "x5" / "5x" / "/5" right after a copy: an array
    const mult = /^(?:x\s*(\d+)|(\d+)\s*x|\/\s*(\d+))$/i.exec(s);
    const last = st.current.last;
    if (mult && last?.t === "move" && last.copy) {
      const n = Number(mult[1] ?? mult[2] ?? mult[3]);
      const by = mult[3] ? last.by.clone().divideScalar(n) : last.by.clone();
      await L.run({ cmd: "undo" });
      await run({ cmd: "array", ids: last.ids, by: arr(by), count: n });
      return;
    }
    // plain numbers (angles, factors) or lengths with units (12', 8'6", 250cm)
    if (!nums.length && !lens.length) return;

    switch (op?.t) {
      case "wall": {
        const last = op.pts[op.pts.length - 1];
        const dir = op.cur.clone().sub(last);
        if (dir.length() > 1e-9 && lens.length) op.pts.push(last.clone().addScaledVector(dir.normalize(), lens[0]));
        return;
      }
      case "line": {
        const dir = op.cur.clone().sub(op.p0);
        if (dir.length() > 1e-9 && lens.length) await commitLine(op.p0.clone().addScaledVector(dir.normalize(), lens[0]));
        return;
      }
      case "rect":
        if (lens.length < 2) return L.onToast("Type width, height (e.g. 12',16' or 4,6)", true);
        op.a = Math.sign(op.a || 1) * Math.abs(lens[0]);
        op.b = Math.sign(op.b || 1) * Math.abs(lens[1]);
        return void (await commitRect());
      case "circle":
        if (!lens.length) return;
        op.r = Math.abs(lens[0]);
        return void (await commitCircle());
      case "arc":
        if (!op.b) return;
        if (!lens.length) return;
        op.bulge = Math.sign(op.bulge || 1) * Math.abs(lens[0]);
        return void (await commitArc());
      case "push":
        if (!lens.length) return;
        op.d = lens[0];
        return void (await commitPush());
      case "offset":
        if (!lens.length) return;
        op.d = lens[0];
        return void (await commitOffset());
      case "tape": {
        const dir = op.cur.clone().sub(op.p0);
        if (dir.length() > 1e-9 && lens.length) await commitTape(op.p0.clone().addScaledVector(dir.normalize(), lens[0]));
        return;
      }
      case "move":
        if (op.delta.length() < 1e-9 || !lens.length) return;
        op.delta = op.delta.clone().normalize().multiplyScalar(lens[0]);
        return void (await commitMove());
      case "rotate":
        if (!op.ref) return;
        op.angle = nums[0];
        return void (await commitRotate());
      case "scale":
        op.factor = nums[0];
        return void (await commitScale());
    }
    // nothing in progress: retype the last push / move / rotate / scale
    if (!last || (["push", "move"].includes(last.t) && !lens.length)) return;
    await L.run({ cmd: "undo" });
    if (last.t === "push") {
      if (!lens.length) return;
      await run({ cmd: "pushpull", face: last.face, distance: lens[0] });
      st.current.last = { ...last, d: lens[0] };
    } else if (last.t === "move") {
      if (!lens.length) return;
      const by = last.by.clone().normalize().multiplyScalar(lens[0]);
      await run({ cmd: last.copy ? "duplicate" : "move", ids: last.ids, by: arr(by) });
      st.current.last = { ...last, by };
    } else if (last.t === "rotate") {
      await run({ cmd: "rotate", ids: last.ids, center: arr(last.center), axis: arr(last.n), angle: nums[0] });
      st.current.last = { ...last, angle: nums[0] };
    } else if (last.t === "scale") {
      await run({ cmd: "scale", ids: last.ids, center: arr(last.center), factor: nums.length >= 3 ? nums.slice(0, 3) : nums[0] });
    }
  }

  props.api.current = {
    applyTyped,
    fit: () => fit(),
    fitSoon: () => void (st.current.fitNext = true),
    view,
    camera,
    goTo,
    clearSection: () => {
      st.current.section = null;
      buildModel();
    },
    sectionPlane: () => {
      const p = st.current.section;
      if (!p) return null;
      const o = p.coplanarPoint(new THREE.Vector3());
      return { origin: [o.x, o.y, o.z], normal: [p.normal.x, p.normal.y, p.normal.z] };
    },
    planCut: (height: number) => {
      // keep what's below: normal points down, plane at z = height
      st.current.section = new THREE.Plane(new THREE.Vector3(0, 0, -1), height);
      buildModel();
      live.current.onSection(true);
    },
    moveSection: (by: number) => {
      const p = st.current.section;
      if (!p) return;
      // normal points into what's kept; moving "by" along it
      p.constant -= by;
      buildModel();
      live.current.onSection(true);
    },
  };

  // ---------------------------------------------------------------- commits
  async function commitLine(p: THREE.Vector3) {
    const op = st.current.op;
    if (op?.t !== "line" || p.distanceTo(op.p0) < 1e-6) return;
    const r = await run({ cmd: "line", points: [arr(op.p0), arr(p)] });
    const closed = p.distanceTo(op.start) < 1e-6 || ((r.faces as unknown[]) ?? []).length > 0;
    st.current.lock = null;
    if (!r.ok || closed) {
      st.current.op = null;
      clearPreview();
    } else st.current.op = { t: "line", p0: p.clone(), start: op.start, cur: p.clone() };
  }

  function rectCorners(r: Extract<Op, { t: "rect" }>): THREE.Vector3[] {
    return [r.p0.clone(), r.p0.clone().addScaledVector(r.u, r.a), r.p0.clone().addScaledVector(r.u, r.a).addScaledVector(r.v, r.b), r.p0.clone().addScaledVector(r.v, r.b)];
  }

  async function commitRect() {
    const op = st.current.op;
    if (op?.t !== "rect" || Math.abs(op.a) < 1e-3 || Math.abs(op.b) < 1e-3) return;
    st.current.op = null;
    clearPreview();
    await run({ cmd: "polygon", points: rectCorners(op).map(arr) });
  }

  async function commitCircle() {
    const op = st.current.op;
    if (op?.t !== "circle" || op.r < 1e-3) return;
    st.current.op = null;
    clearPreview();
    await run({ cmd: "circle", center: arr(op.c), radius: r4(op.r), normal: arr(op.n), segments: op.seg });
  }

  async function commitArc() {
    const op = st.current.op;
    if (op?.t !== "arc" || !op.b) return;
    st.current.op = null;
    clearPreview();
    await run({ cmd: "arc", start: arr(op.a), end: arr(op.b), bulge: r4(op.bulge), normal: arr(op.n) });
  }

  async function commitPush() {
    const op = st.current.op;
    if (op?.t !== "push") return;
    st.current.op = null;
    clearPreview();
    if (Math.abs(op.d) < 1e-3) return;
    const d = r4(op.d);
    const r = await run({ cmd: "pushpull", face: op.face.id, distance: d });
    if (r.ok) st.current.last = { t: "push", face: op.face.id, d };
  }

  async function commitOffset() {
    const op = st.current.op;
    if (op?.t !== "offset") return;
    st.current.op = null;
    clearPreview();
    if (Math.abs(op.d) >= 1e-3) await run({ cmd: "offset", face: op.face.id, distance: r4(op.d) });
  }

  async function commitMove() {
    const op = st.current.op;
    if (op?.t !== "move") return;
    st.current.op = null;
    clearPreview();
    if (op.delta.length() < 1e-6) return;
    const r = await run({ cmd: op.copy ? "duplicate" : "move", ids: op.ids, by: arr(op.delta) });
    if (r.ok) {
      st.current.last = { t: "move", ids: op.ids, by: op.delta.clone(), copy: op.copy };
      if (op.copy) live.current.onToast("Copied. Type x5 for five copies, or /5 to fill the gap with five.");
    }
  }

  async function commitRotate() {
    const op = st.current.op;
    if (op?.t !== "rotate" || !op.ref) return;
    st.current.op = null;
    clearPreview();
    let ids = op.ids;
    if (op.copy) {
      // Option / Ctrl: rotate a copy, leave the original
      const d = await run({ cmd: "duplicate", ids, by: [0, 0, 0] });
      if (!d.ok) return;
      ids = d.ids as number[];
    }
    const r = await run({ cmd: "rotate", ids, center: arr(op.center), axis: arr(op.n), angle: r4(op.angle) });
    if (r.ok) st.current.last = { t: "rotate", ids, center: op.center, n: op.n, angle: op.angle };
  }

  async function commitScale() {
    const op = st.current.op;
    if (op?.t !== "scale") return;
    st.current.op = null;
    clearPreview();
    if (Math.abs(op.factor - 1) < 1e-4 || op.factor === 0) return;
    const r = await run({ cmd: "scale", ids: op.ids, center: arr(op.center), factor: r4(op.factor) });
    if (r.ok) st.current.last = { t: "scale", ids: op.ids, center: op.center, factor: op.factor };
  }

  async function commitWall(closed: boolean) {
    const op = st.current.op;
    if (op?.t !== "wall") return;
    st.current.op = null;
    clearPreview();
    if (op.pts.length < 2) return;
    const b = live.current.build;
    await run({ cmd: "wall", points: op.pts.map(arr), thickness: b.wallThickness, height: b.wallHeight, closed });
  }

  async function commitTape(p: THREE.Vector3) {
    const op = st.current.op;
    if (op?.t !== "tape") return;
    st.current.op = null;
    clearPreview();
    if (op.along) await run({ cmd: "guide", point: arr(p), direction: arr(op.along) });
    else if (p.distanceTo(op.p0) > 1e-4) await run({ cmd: "guide", point: arr(p) });
    live.current.onMeasure("Length", fmt(p.distanceTo(op.p0)));
  }

  /** A see-through copy of the selection under a transform (move/rotate/scale preview). */
  function ghost(m: THREE.Matrix4) {
    const T = t.current!;
    const sel = selNow();
    const pos = trianglesOf(ownersOf(sel));
    if (pos.length) {
      const g = overlayMesh(pos, 0.3);
      g.applyMatrix4(m);
      T.preview.add(g);
    }
    const sl = selectionLines(sel);
    if (sl.length) {
      const l = lines(sl, ACCENT, false);
      l.applyMatrix4(m);
      T.preview.add(l);
    }
  }

  // ---------------------------------------------------------------- input
  useEffect(() => {
    const T = t.current!;
    const el = T.renderer.domElement;

    const move = (e: PointerEvent) => {
      const b = badge.current;
      if (b) {
        const r = el.getBoundingClientRect();
        // below-left of the pointer, clear of the snap label on its right
        b.style.transform = `translate(${e.clientX - r.left - 30}px, ${e.clientY - r.top + 12}px)`;
        b.style.opacity = "1";
      }
      const L = live.current;
      if (st.current.space && e.buttons & 1) st.current.spaceUsed = true;
      if (L.tool === "zoom" && e.buttons & 1 && st.current.zoomY != null) {
        const dy = e.clientY - st.current.zoomY;
        st.current.zoomY = e.clientY;
        dolly(Math.exp(dy * 0.01));
        return;
      }
      if (!L.scene || st.current.space || L.tool === "orbit" || L.tool === "pan" || L.tool === "zoom" || (e.buttons & 6) !== 0) return;
      const op = st.current.op;
      const lock = st.current.lock;
      const E = env();
      clearPreview();

      switch (op?.t) {
        case "box": {
          op.x1 = e.clientX;
          op.y1 = e.clientY;
          setBox({ x0: op.x0 - E.rect.left, y0: op.y0 - E.rect.top, x1: op.x1 - E.rect.left, y1: op.y1 - E.rect.top });
          return;
        }
        case "line":
        case "tape": {
          const sn = infer(E, e, { anchor: op.p0, lock });
          op.cur = sn.p;
          T.preview.add(polyline([op.p0, sn.p], axisColor(sn), false, op.t === "tape"));
          showSnap(sn);
          setLive([{ p: op.p0.clone().add(sn.p).multiplyScalar(0.5), text: fmt(sn.p.distanceTo(op.p0)) }]);
          L.onMeasure("Length", fmt(sn.p.distanceTo(op.p0)));
          return;
        }
        case "dim": {
          if (!op.b) {
            const sn = infer(E, e, { anchor: op.a, lock });
            op.cur = sn.p;
            T.preview.add(polyline([op.a, sn.p], axisColor(sn)));
            showSnap(sn);
            setLive([{ p: op.a.clone().add(sn.p).multiplyScalar(0.5), text: fmt(sn.p.distanceTo(op.a)) }]);
            return;
          }
          // pull the dimension out, in the plane of the measured line that faces you
          const u = op.b.clone().sub(op.a).normalize();
          const view = new THREE.Vector3();
          T.camera.getWorldDirection(view);
          const n = u.clone().cross(view).cross(u).normalize();
          const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(n.lengthSq() > 0.5 ? n : UP, op.a), new THREE.Vector3());
          if (!q) return;
          const d = q.clone().sub(op.a);
          op.off = d.sub(u.clone().multiplyScalar(d.dot(u)));
          const segs = dimSegments(op.a, op.b, op.off);
          for (let i = 0; i < segs.length; i += 6) T.preview.add(polyline([V(segs.slice(i, i + 3)), V(segs.slice(i + 3, i + 6))], 0x33384d));
          setLive([{ p: op.a.clone().add(op.b).multiplyScalar(0.5).add(op.off), text: fmt(op.a.distanceTo(op.b)) }]);
          L.onMeasure("Dimension", fmt(op.a.distanceTo(op.b)));
          return;
        }
        case "wall": {
          const last = op.pts[op.pts.length - 1];
          const sn = infer(E, e, { anchor: last, lock, plane: { origin: op.pts[0], normal: UP } });
          op.cur = sn.p;
          T.preview.add(polyline([...op.pts, sn.p], ACCENT));
          if (op.pts.length > 2 && sn.p.distanceTo(op.pts[0]) < 1e-6) setHint({ ...toScreen2(sn.p), label: "Close room", kind: "endpoint" });
          else showSnap(sn);
          setLive([{ p: last.clone().add(sn.p).multiplyScalar(0.5), text: fmt(sn.p.distanceTo(last)) }]);
          L.onMeasure("Length", fmt(sn.p.distanceTo(last)));
          return;
        }
        case "stairs": {
          const sn = infer(E, e, { anchor: op.at, lock, plane: { origin: op.at, normal: UP } });
          op.cur = sn.p;
          T.preview.add(polyline([op.at, sn.p], ACCENT));
          showSnap(sn);
          L.onMeasure("Direction", "click which way it climbs");
          return;
        }
        case "text": {
          const sn = infer(E, e, {});
          op.cur = sn.p;
          T.preview.add(polyline([op.leader, sn.p], 0x33384d));
          setHint(null);
          return;
        }
        case "rect": {
          // no axis lock from the first corner: that would squash the rectangle flat
          const sn = infer(E, e, { plane: { origin: op.p0, normal: op.n } });
          const d = sn.p.clone().sub(op.p0);
          op.a = d.dot(op.u);
          op.b = d.dot(op.v);
          const pts = rectCorners(op);
          T.preview.add(polyline(pts, ACCENT, true));
          const fill = new THREE.BufferGeometry().setFromPoints([pts[0], pts[1], pts[2], pts[0], pts[2], pts[3]]);
          T.preview.add(new THREE.Mesh(fill, new THREE.MeshBasicMaterial({ color: ACCENT, transparent: true, opacity: 0.14, side: THREE.DoubleSide, depthWrite: false })));
          showSnap(sn);
          setLive([
            { p: pts[0].clone().add(pts[1]).multiplyScalar(0.5), text: fmt(Math.abs(op.a)) },
            { p: pts[1].clone().add(pts[2]).multiplyScalar(0.5), text: fmt(Math.abs(op.b)) },
          ]);
          L.onMeasure("Dimensions", `${Math.abs(op.a).toFixed(2)}, ${Math.abs(op.b).toFixed(2)}`);
          return;
        }
        case "circle": {
          const sn = infer(E, e, { anchor: op.c, plane: { origin: op.c, normal: op.n } });
          op.r = sn.p.distanceTo(op.c);
          const [u, v] = basis(op.n);
          const pts = Array.from({ length: op.seg }, (_, i) => {
            const a = (i / op.seg) * Math.PI * 2;
            return op.c.clone().addScaledVector(u, Math.cos(a) * op.r).addScaledVector(v, Math.sin(a) * op.r);
          });
          T.preview.add(polyline(pts, ACCENT, true));
          T.preview.add(polyline([op.c, sn.p], 0x8a8ea0));
          showSnap(sn);
          setLive([{ p: op.c.clone().add(sn.p).multiplyScalar(0.5), text: `r ${fmt(op.r)}` }]);
          L.onMeasure("Radius", fmt(op.r));
          return;
        }
        case "arc": {
          if (!op.b) {
            const sn = infer(E, e, { anchor: op.a, lock });
            op.cur = sn.p;
            T.preview.add(polyline([op.a, sn.p], axisColor(sn)));
            showSnap(sn);
            L.onMeasure("Length", fmt(sn.p.distanceTo(op.a)));
            return;
          }
          const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.a), new THREE.Vector3());
          if (!q) return;
          const side = op.n.clone().cross(op.b.clone().sub(op.a).normalize()).normalize();
          op.bulge = q.clone().sub(op.a.clone().add(op.b).multiplyScalar(0.5)).dot(side);
          T.preview.add(polyline(arcPoints(op.a, op.b, op.bulge, op.n)));
          L.onMeasure("Bulge", fmt(op.bulge));
          return;
        }
        case "push": {
          op.d = closestOnLine(ray(e), op.origin, op.n).sub(op.origin).dot(op.n);
          // match the height of a point the cursor hovers (line up with the neighbour's roof)
          const sn = infer(E, e, {});
          if (sn.kind === "endpoint" || sn.kind === "midpoint") {
            op.d = sn.p.clone().sub(op.origin).dot(op.n);
            showSnap(sn);
          } else setHint(null);
          op.dragged = true;
          const base = op.face.outer.map(V);
          const off = op.n.clone().multiplyScalar(op.d);
          const top = base.map((p) => p.clone().add(off));
          T.preview.add(polyline(top, ACCENT, true));
          const seg: number[] = [];
          base.forEach((p, i) => seg.push(p.x, p.y, p.z, top[i].x, top[i].y, top[i].z));
          T.preview.add(lines(seg, ACCENT, false));
          const cap = overlayMesh(trianglesOf(new Set([op.face.id])), 0.2);
          cap.position.copy(off);
          T.preview.add(cap);
          const mid = top.reduce((a, p) => a.add(p), new THREE.Vector3()).divideScalar(top.length);
          setLive([{ p: mid, text: fmt(op.d) }]);
          L.onMeasure("Distance", fmt(op.d));
          return;
        }
        case "offset": {
          const n = V(op.face.normal);
          const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(n, op.start), new THREE.Vector3());
          if (!q) return;
          const outer = op.face.outer.map(V);
          const c = outer.reduce((a, p) => a.add(p), new THREE.Vector3()).divideScalar(outer.length);
          op.d = q.clone().sub(op.start).dot(c.clone().sub(op.start).normalize());
          T.preview.add(polyline(offsetOutline(outer, n, op.d), ACCENT, true));
          L.onMeasure("Distance", fmt(op.d));
          return;
        }
        case "move": {
          const sn = infer(E, e, { anchor: op.base, lock });
          op.delta = sn.p.clone().sub(op.base);
          T.preview.add(polyline([op.base, sn.p], axisColor(sn)));
          ghost(new THREE.Matrix4().makeTranslation(op.delta.x, op.delta.y, op.delta.z));
          showSnap(sn);
          setLive([{ p: op.base.clone().add(sn.p).multiplyScalar(0.5), text: fmt(op.delta.length()) }]);
          L.onMeasure(op.copy ? "Copy" : "Distance", fmt(op.delta.length()));
          return;
        }
        case "rotate": {
          const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.center), new THREE.Vector3());
          if (!q) return;
          const [u, v] = basis(op.n);
          const R = Math.max(0.5, q.distanceTo(op.center));
          const ring = Array.from({ length: 48 }, (_, i) => op.center.clone().addScaledVector(u, Math.cos((i / 48) * Math.PI * 2) * R).addScaledVector(v, Math.sin((i / 48) * Math.PI * 2) * R));
          T.preview.add(polyline(ring, 0x3e63dd, true));
          if (!op.ref) {
            T.preview.add(polyline([op.center, q], 0x8a8ea0));
            L.onMeasure("Angle", "click a start direction");
            return;
          }
          const a0 = op.ref.clone().sub(op.center);
          const a1 = q.clone().sub(op.center);
          let ang = THREE.MathUtils.radToDeg(Math.atan2(a0.clone().cross(a1).dot(op.n), a0.dot(a1)));
          const s15 = Math.round(ang / 15) * 15;
          if (Math.abs(ang - s15) < 2.5) ang = s15;
          op.angle = ang;
          T.preview.add(polyline([op.center, op.ref], 0x8a8ea0));
          T.preview.add(polyline([op.center, q], ACCENT));
          const c = op.center;
          ghost(new THREE.Matrix4().makeTranslation(c.x, c.y, c.z).multiply(new THREE.Matrix4().makeRotationAxis(op.n, THREE.MathUtils.degToRad(ang))).multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z)));
          L.onMeasure("Angle", `${ang.toFixed(1)}°`);
          return;
        }
        case "scale": {
          const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(UP, op.center), new THREE.Vector3());
          if (!q) return;
          op.factor = Math.round((q.distanceTo(op.center) / Math.max(op.start.distanceTo(op.center), 1e-6)) * 100) / 100;
          const c = op.center;
          const f = op.factor;
          ghost(new THREE.Matrix4().makeTranslation(c.x, c.y, c.z).multiply(new THREE.Matrix4().makeScale(f, f, f)).multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z)));
          L.onMeasure("Scale", `${f.toFixed(2)}×`);
          return;
        }
      }

      // nothing in progress: show what a click would hit
      if (["line", "arc", "rect", "circle", "polygon", "tape", "dimension", "text", "move", "rotate", "scale", "place", "wall", "stairs", "column"].includes(L.tool)) showSnap(infer(E, e, {}));
      else setHint(null);
      if (["select", "erase", "paint", "push", "offset", "followme", "section", "move", "rotate", "scale", "roof"].includes(L.tool)) {
        const he = ["select", "erase", "move"].includes(L.tool) ? pickEdge(e) : null;
        const h = he == null ? pick(e) : null;
        const id = h ? h.owner : null;
        if (id !== st.current.hover || he !== st.current.hoverEdge) {
          st.current.hover = id;
          st.current.hoverEdge = he;
          drawOverlay();
        }
      }
    };

    const down = async (e: PointerEvent) => {
      const L = live.current;
      if (e.button === 2) {
        st.current.rightDown = { x: e.clientX, y: e.clientY };
        return;
      }
      if (L.tool === "zoom" && e.button === 0) {
        st.current.zoomY = e.clientY;
        el.setPointerCapture(e.pointerId);
        return;
      }
      if (e.button !== 0 || st.current.space || L.tool === "orbit" || L.tool === "pan" || !L.scene) return;
      const E = env();
      const op = st.current.op;
      const sel = selNow();
      // remember the press: dragging and releasing finishes the shape (click-drag drawing)
      st.current.press = { x: e.clientX, y: e.clientY, fresh: !op };
      if (["line", "rect", "circle", "polygon", "tape", "dimension", "move"].includes(L.tool)) el.setPointerCapture(e.pointerId);

      switch (L.tool) {
        case "select": {
          const he = pickEdge(e);
          const h = he == null ? pick(e) : null;
          if (e.detail >= 2) {
            // double-click: open a group or component; outside closes it
            if (h) {
              const ent = entityOf(h);
              if (ent.kind === "groups" || ent.kind === "instances") {
                await run({ cmd: "edit", id: ent.id });
                L.setSel(EMPTY);
                L.onToast(`Editing ${ent.kind === "groups" ? "group" : "component"}. Esc or double-click outside to close.`);
                return;
              }
            } else {
              await L.run({ cmd: "close" });
              return;
            }
          }
          if (he != null) return L.setSel(toggle(sel, "edges", he, selMode(e)));
          if (h) {
            const ent = entityOf(h);
            return L.setSel(toggle(sel, ent.kind, ent.id, selMode(e)));
          }
          st.current.op = { t: "box", x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY, mode: selMode(e) };
          el.setPointerCapture(e.pointerId);
          return;
        }
        case "line": {
          const sn = infer(E, e, op?.t === "line" ? { anchor: op.p0, lock: st.current.lock } : {});
          if (op?.t === "line") {
            if (e.detail >= 2) return cancel();
            return void (await commitLine(sn.p));
          }
          st.current.op = { t: "line", p0: sn.p, start: sn.p, cur: sn.p };
          return;
        }
        case "rect": {
          if (op?.t === "rect") return void (await commitRect());
          const sn = infer(E, e, {});
          const n = sn.face ? V(sn.face.normal) : UP.clone();
          const [u, v] = basis(n);
          st.current.op = { t: "rect", p0: sn.p, n, u, v, a: 0, b: 0 };
          return;
        }
        case "circle":
        case "polygon": {
          if (op?.t === "circle") return void (await commitCircle());
          const sn = infer(E, e, {});
          st.current.op = { t: "circle", c: sn.p, n: sn.face ? V(sn.face.normal) : UP.clone(), r: 0, seg: L.tool === "polygon" ? st.current.segs.polygon : st.current.segs.circle };
          return;
        }
        case "arc": {
          if (op?.t === "arc") {
            if (!op.b) {
              const sn = infer(E, e, { anchor: op.a, lock: st.current.lock });
              if (sn.p.distanceTo(op.a) > 1e-4) op.b = sn.p;
              return;
            }
            return void (await commitArc());
          }
          const sn = infer(E, e, {});
          st.current.op = { t: "arc", a: sn.p, b: null, n: sn.face ? V(sn.face.normal) : UP.clone(), bulge: 0, cur: sn.p };
          return;
        }
        case "push": {
          if (op?.t === "push") return void (await commitPush()); // click-move-click
          const h = pick(e);
          if (!h?.face) return;
          if (e.detail >= 2 && st.current.last?.t === "push") {
            // double-click: same distance as last time
            await run({ cmd: "pushpull", face: h.face.id, distance: st.current.last.d });
            return;
          }
          st.current.op = { t: "push", face: h.face, origin: h.point.clone(), n: V(h.face.normal), d: 0, dragged: false };
          el.setPointerCapture(e.pointerId);
          return;
        }
        case "offset": {
          if (op?.t === "offset") return void (await commitOffset());
          const h = pick(e);
          if (h?.face) st.current.op = { t: "offset", face: h.face, start: h.point.clone(), d: 0 };
          return;
        }
        case "followme": {
          const h = pick(e);
          if (!h?.face) return;
          if (sel.edges.length) await run({ cmd: "follow_me", face: h.face.id, edges: sel.edges });
          else if (sel.faces.length === 1 && sel.faces[0] !== h.face.id) await run({ cmd: "follow_me", face: h.face.id, around: sel.faces[0] });
          else return L.onToast("Select the path first (edges, or a face to go around), then click the profile.", true);
          L.setSel(EMPTY);
          return;
        }
        case "move":
        case "rotate":
        case "scale": {
          if (op?.t === "move") return void (await commitMove());
          if (op?.t === "scale") return void (await commitScale());
          if (op?.t === "rotate") {
            if (op.ref) return void (await commitRotate());
            const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.center), new THREE.Vector3());
            if (q && q.distanceTo(op.center) > 1e-4) op.ref = q;
            return;
          }
          // nothing selected: grab what's under the cursor
          let s2 = sel;
          if (selIds(sel).length === 0) {
            const he = pickEdge(e);
            const h = he == null ? pick(e) : null;
            if (he != null) s2 = toggle(EMPTY, "edges", he, false);
            else if (h) {
              const ent = entityOf(h);
              s2 = toggle(EMPTY, ent.kind, ent.id, false);
            } else return;
            st.current.selOverride = s2;
            L.setSel(s2);
          }
          const ids = selIds(s2);
          const sn = infer(E, e, {});
          if (L.tool === "move") st.current.op = { t: "move", base: sn.p, ids, delta: new THREE.Vector3(), copy: e.altKey || e.ctrlKey };
          else if (L.tool === "rotate") st.current.op = { t: "rotate", center: sn.p, n: sn.face ? V(sn.face.normal) : UP.clone(), ref: null, angle: 0, ids, copy: e.altKey || e.ctrlKey };
          else {
            const pos = trianglesOf(ownersOf(s2));
            const b = new THREE.Box3();
            for (let i = 0; i < pos.length; i += 3) b.expandByPoint(new THREE.Vector3(pos[i], pos[i + 1], pos[i + 2]));
            const c = b.isEmpty() ? sn.p : new THREE.Vector3((b.min.x + b.max.x) / 2, (b.min.y + b.max.y) / 2, b.min.z);
            const start = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(UP, c), new THREE.Vector3()) ?? sn.p;
            st.current.op = { t: "scale", center: c, start, factor: 1, ids };
          }
          return;
        }
        case "tape": {
          if (op?.t === "tape") return void (await commitTape(infer(E, e, { anchor: op.p0, lock: st.current.lock }).p));
          const sn = infer(E, e, {});
          st.current.op = { t: "tape", p0: sn.p, along: sn.kind === "on-edge" && sn.along ? sn.along : null, cur: sn.p };
          return;
        }
        case "dimension": {
          if (op?.t === "dim") {
            if (!op.b) {
              const sn = infer(E, e, { anchor: op.a, lock: st.current.lock });
              if (sn.p.distanceTo(op.a) > 1e-4) op.b = sn.p;
              return;
            }
            st.current.op = null;
            clearPreview();
            await run({ cmd: "dimension", from: arr(op.a), to: arr(op.b), offset: arr(op.off) });
            return;
          }
          const sn = infer(E, e, {});
          st.current.op = { t: "dim", a: sn.p, b: null, off: new THREE.Vector3(), cur: sn.p };
          return;
        }
        case "text": {
          if (op?.t === "text") {
            st.current.op = null;
            clearPreview();
            const text = await ask("Label text", "");
            if (text?.trim()) await run({ cmd: "text", at: arr(op.cur), text: text.trim(), leader: arr(op.leader) });
            return;
          }
          const sn = infer(E, e, {});
          st.current.op = { t: "text", leader: sn.p, cur: sn.p };
          return;
        }
        case "wall": {
          if (op?.t === "wall") {
            const last = op.pts[op.pts.length - 1];
            const sn = infer(E, e, { anchor: last, lock: st.current.lock, plane: { origin: op.pts[0], normal: UP } });
            if (op.pts.length > 2 && sn.p.distanceTo(op.pts[0]) < 1e-6) return void (await commitWall(true));
            if (e.detail >= 2) return void (await commitWall(false));
            if (sn.p.distanceTo(last) > 1e-4) op.pts.push(sn.p);
            st.current.lock = null;
            return;
          }
          const sn = infer(E, e, {});
          st.current.op = { t: "wall", pts: [sn.p], cur: sn.p };
          return;
        }
        case "stairs": {
          if (op?.t === "stairs") {
            const d = op.cur.clone().sub(op.at);
            if (d.length() < 1e-3) return;
            st.current.op = null;
            clearPreview();
            const b = L.build;
            const r = await run({ cmd: "stairs", at: arr(op.at), rise: b.stairRise, width: b.stairWidth, direction: r4(THREE.MathUtils.radToDeg(Math.atan2(d.y, d.x))) });
            if (r.ok) L.onToast(`${r.risers} risers of ${fmt(r.riser_height as number)}`);
            return;
          }
          const sn = infer(E, e, {});
          st.current.op = { t: "stairs", at: sn.p, cur: sn.p };
          return;
        }
        case "roof": {
          const h = pick(e);
          if (!h?.face) return L.onToast("Click a flat top face (a ceiling or the top of walls).", true);
          const b = L.build;
          await run({ cmd: "roof", face: h.face.id, kind: b.roofKind, pitch: b.roofPitch, overhang: b.roofOverhang });
          return;
        }
        case "smoove":
        case "stamp":
        case "drape": {
          const h = pick(e);
          const group = h?.face?.group;
          if (group == null) return L.onToast("Click on a terrain (a group of triangles).", true);
          const b = L.build;
          if (L.tool === "smoove") {
            // Shift digs instead of raising
            await run({ cmd: "smoove", group, at: arr(h!.point), radius: b.smooveRadius, height: e.shiftKey ? -b.smooveHeight : b.smooveHeight });
          } else if (L.tool === "stamp") {
            const fp = sel.faces.find((f) => faceById(f)?.group !== group);
            if (fp == null) return L.onToast("Select the footprint face first (a rectangle above the terrain), then click the terrain.", true);
            await run({ cmd: "stamp", group, face: fp, offset: b.stampOffset });
          } else {
            if (!sel.edges.length) return L.onToast("Select the lines to drape first, then click the terrain.", true);
            await run({ cmd: "drape", group, ids: sel.edges });
          }
          return;
        }
        case "flipedge": {
          const he = pickEdge(e);
          if (he == null) return L.onToast("Click an edge between two triangles.", true);
          await run({ cmd: "flip_edge", edge: he });
          return;
        }
        case "column": {
          const sn = infer(E, e, {});
          const b = L.build;
          await run({ cmd: "column", at: arr(sn.p), size: b.columnSize, height: b.columnHeight, round: b.columnRound });
          return;
        }
        case "paint": {
          const h = pick(e);
          if (!h) return;
          if (e.altKey || e.metaKey) {
            // eyedropper (Option on Windows-style keys, Cmd on Mac, as in SketchUp): a texture if the face has one, else its color
            const s = L.scene;
            const i = s.triFace.indexOf(h.owner);
            if (i >= 0 && s.triTex?.[i] >= 0) return L.setPaintMaterial(s.textures[s.triTex[i]]);
            if (i >= 0) {
              L.setPaintMaterial(null);
              L.setColor([s.colors[i * 3], s.colors[i * 3 + 1], s.colors[i * 3 + 2]]);
            }
            return;
          }
          const ent = entityOf(h);
          if (ent.kind === "instances") return L.onToast("Double-click a component with Select to open it, then paint inside.", true);
          let targets = [ent.id];
          if (e.shiftKey && ent.kind === "faces") {
            // Shift: every face here that has the same material (SketchUp's Replace)
            const s = L.scene;
            const f0 = s.faces.find((f) => f.id === ent.id);
            if (f0) targets = s.faces.filter((f) => f.group === f0.group && f.material === f0.material && !f.hidden).map((f) => f.id);
          }
          await run(L.paintMaterial ? { cmd: "paint", ids: targets, material: L.paintMaterial } : { cmd: "paint", ids: targets, color: L.color });
          return;
        }
        case "erase": {
          const he = pickEdge(e);
          const soften = e.altKey || e.ctrlKey;
          if (he != null) {
            if (soften) return void (await run({ cmd: "soften", ids: [he], soft: !e.shiftKey }));
            return void (await run({ cmd: e.shiftKey ? "hide" : "delete", ids: [he] }));
          }
          const h = pick(e);
          if (h && !soften) await run({ cmd: e.shiftKey ? "hide" : "delete", ids: [entityOf(h).id] });
          return;
        }
        case "section": {
          const h = pick(e);
          if (!h?.face) return;
          const n = V(h.face.normal).negate();
          // 1 cm into the solid behind the clicked face, keeping the far side
          st.current.section = new THREE.Plane().setFromNormalAndCoplanarPoint(n, h.point.clone().addScaledVector(n, 0.01));
          buildModel();
          L.onSection(true);
          L.setTool("select");
          return;
        }
        case "place": {
          if (!L.placeAsset) return;
          const sn = infer(E, e, {});
          const n = sn.face ? V(sn.face.normal) : UP;
          // doors and windows face out of the wall they're dropped on
          const wall = Math.abs(n.z) < 0.5;
          const rot = wall ? THREE.MathUtils.radToDeg(Math.atan2(n.y, n.x)) + 90 : 0;
          const at = sn.p.clone();
          // doors stand on the bottom of the wall they're dropped on
          if (wall && sn.face && (L.placeAsset === "door" || L.placeAsset === "garage_door")) at.z = Math.min(...sn.face.outer.map((q) => q[2]));
          const k = L.placeAsset;
          // a library file, a component already in the model, or a starter asset
          if (k.startsWith("file:")) await run({ cmd: "insert_file", path: k.slice(5), at: arr(at), rotate: r4(rot) });
          else if (k.startsWith("component:")) await run({ cmd: "place", component: Number(k.slice(10)), at: arr(at), rotate: r4(rot) });
          else await run({ cmd: "insert", asset: k, at: arr(at), rotate: r4(rot) });
          return;
        }
      }
    };

    const up = async (e: PointerEvent) => {
      const L = live.current;
      if (e.button === 2) {
        const rd = st.current.rightDown;
        st.current.rightDown = null;
        if (rd && Math.hypot(e.clientX - rd.x, e.clientY - rd.y) < 4) {
          // a right click (not a pan): select what's under the cursor if nothing is, then the menu
          if (selIds(selNow()).length === 0) {
            const h = pick(e);
            if (h) {
              const ent = entityOf(h);
              L.setSel(toggle(EMPTY, ent.kind, ent.id, false));
            }
          }
          const r = rect();
          L.onContext(e.clientX - r.left, e.clientY - r.top);
        }
        return;
      }
      const op = st.current.op;
      const press = st.current.press;
      st.current.press = null;
      if (el.hasPointerCapture(e.pointerId) && op?.t !== "push" && op?.t !== "box") el.releasePointerCapture(e.pointerId);
      if (press?.fresh && op && Math.hypot(e.clientX - press.x, e.clientY - press.y) > 6) {
        // a press-drag-release: finish where the mouse let go
        if (op.t === "line") return void (await commitLine(op.cur));
        if (op.t === "rect") return void (await commitRect());
        if (op.t === "circle") return void (await commitCircle());
        if (op.t === "tape") return void (await commitTape(op.cur));
        if (op.t === "move") return void (await commitMove());
        if (op.t === "dim" && !op.b && op.cur.distanceTo(op.a) > 1e-4) {
          op.b = op.cur;
          return;
        }
      }
      if (op?.t === "push" && !op.dragged && press?.fresh && Math.hypot(e.clientX - press.x, e.clientY - press.y) > 6) {
        // a quick flick with no move events in between: measure from where it let go
        op.d = closestOnLine(ray(e), op.origin, op.n).sub(op.origin).dot(op.n);
        op.dragged = true;
      }
      if (op?.t === "push" && op.dragged) {
        el.releasePointerCapture(e.pointerId);
        if (Math.abs(op.d) > 1e-3) await commitPush();
        else op.dragged = false;
        return;
      }
      if (st.current.zoomY != null) {
        st.current.zoomY = null;
        return;
      }
      if (op?.t === "box") {
        el.releasePointerCapture(e.pointerId);
        st.current.op = null;
        setBox(null);
        if (Math.hypot(op.x1 - op.x0, op.y1 - op.y0) < 4) {
          if (op.mode === "replace") L.setSel(EMPTY);
          return;
        }
        L.setSel(combine(selNow(), boxSelect(op), op.mode));
      }
    };

    /** Left-to-right drag: things fully inside. Right-to-left: anything it touches. */
    function boxSelect(op: Extract<Op, { t: "box" }>): Sel {
      const s = sceneNow();
      const cam = t.current!.camera;
      const r = rect();
      const [x0, x1] = [Math.min(op.x0, op.x1) - r.left, Math.max(op.x0, op.x1) - r.left];
      const [y0, y1] = [Math.min(op.y0, op.y1) - r.top, Math.max(op.y0, op.y1) - r.top];
      const crossing = op.x1 < op.x0;
      const inside = (p: number[] | THREE.Vector3) => {
        const v = (Array.isArray(p) ? V(p) : p.clone()).project(cam);
        const x = (v.x * 0.5 + 0.5) * r.width;
        const y = (-v.y * 0.5 + 0.5) * r.height;
        return v.z < 1 && x >= x0 && x <= x1 && y >= y0 && y <= y1;
      };
      const test = (pts: (number[] | THREE.Vector3)[]) => (crossing ? pts.some(inside) : pts.every(inside));
      const out: Sel = { faces: [], edges: [], groups: [], instances: [] };
      const groupsHit = new Map<number, boolean[]>();
      for (const f of s.faces) {
        const ok = test(f.outer);
        if (f.group != null && f.group !== s.activeGroup) groupsHit.set(f.group, [...(groupsHit.get(f.group) ?? []), ok]);
        else if (ok) out.faces.push(f.id);
      }
      for (const [g, hits] of groupsHit) if (crossing ? hits.some(Boolean) : hits.every(Boolean)) out.groups.push(g);
      for (const ed of s.edgeList) if (!ed.soft && (ed.group == null || ed.group === s.activeGroup) && test([ed.a, ed.b])) out.edges.push(ed.id);
      if (s.editing == null) {
        for (const inst of s.instances) {
          const pos = trianglesOf(new Set([inst.id]));
          const pts: THREE.Vector3[] = [];
          for (let i = 0; i < pos.length; i += 9) pts.push(new THREE.Vector3(pos[i], pos[i + 1], pos[i + 2]));
          if (pts.length && test(pts)) out.instances.push(inst.id);
        }
      }
      return out;
    }

    const key = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (e.code === "Space" && !typing) {
        const isDown = e.type === "keydown";
        e.preventDefault();
        if (st.current.space === isDown) return;
        st.current.space = isDown;
        if (isDown) st.current.spaceUsed = false;
        else if (!st.current.spaceUsed) live.current.setTool("select"); // tap Space = Select
        setOrbitButtons();
        return;
      }
      // hold Shift to lock the direction you're inferring (SketchUp)
      if (e.key === "Shift" && !typing) {
        if (e.type === "keydown" && !e.repeat && st.current.op && st.current.lock == null && st.current.snap?.axis != null) {
          st.current.lock = st.current.snap.axis;
          st.current.shiftLock = true;
        } else if (e.type === "keyup" && st.current.shiftLock) {
          st.current.lock = null;
          st.current.shiftLock = false;
        }
        return;
      }
      if (e.type !== "keydown" || typing) return;
      if (e.key === "Enter" && st.current.op?.t === "wall") {
        e.preventDefault();
        void commitWall(false);
        return;
      }
      if (e.key === "Escape") {
        if (st.current.op) cancel();
        else if (live.current.tool === "place") live.current.setTool("select");
        else if (live.current.scene?.editing != null || live.current.scene?.activeGroup != null) live.current.run({ cmd: "close" });
        else live.current.setSel(EMPTY);
        return;
      }
      // arrow keys lock the direction to an axis (press again to unlock)
      const lockKey: Record<string, Axis | null> = { ArrowRight: 0, ArrowLeft: 1, ArrowUp: 2, ArrowDown: null };
      if (e.key in lockKey && st.current.op) {
        e.preventDefault();
        const want = lockKey[e.key];
        st.current.lock = st.current.lock === want ? null : want;
        live.current.onToast(st.current.lock == null ? "Axis lock off" : `Locked to the ${["red", "green", "blue"][st.current.lock]} axis`);
      }
    };

    const noMenu = (e: MouseEvent) => e.preventDefault();
    const leave = () => {
      setHint(null);
      if (badge.current) badge.current.style.opacity = "0";
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerdown", down);
    el.addEventListener("pointerup", up);
    el.addEventListener("contextmenu", noMenu);
    el.addEventListener("pointerleave", leave);
    window.addEventListener("keydown", key);
    window.addEventListener("keyup", key);
    return () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerdown", down);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("contextmenu", noMenu);
      el.removeEventListener("pointerleave", leave);
      window.removeEventListener("keydown", key);
      window.removeEventListener("keyup", key);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cursor = tool === "orbit" ? "grab" : tool === "select" ? "default" : "crosshair";
  return (
    <div className="viewport" ref={host} style={{ cursor }}>
      {backdrop && <img className="backdrop" src={backdrop} alt="" />}
      {coach && coach.step && (
        <div className="coach" aria-live="polite">
          <div className="coach-head">
            <span className="coach-icon">{(Icons as unknown as Record<string, () => React.ReactNode>)[tool]?.()}</span>
            <b>{coach.title}</b>
          </div>
          <div className="coach-step">{coach.step}</div>
          {coach.keys.length > 0 && (
            <div className="coach-keys">
              {coach.keys.map(([k, what]) => (
                <span key={k}>
                  <kbd>{k}</kbd> {what}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
      {tool !== "select" && (
        <div className="cursor-badge" ref={badge}>
          {(Icons as unknown as Record<string, () => React.ReactNode>)[tool]?.()}
        </div>
      )}
      <div className="labels" ref={labelLayer} />
      {hint && (
        <div className="snap" style={{ left: hint.x, top: hint.y }}>
          <span className={`snap-dot ${hint.kind}`} />
          {hint.label && <span className="snap-label">{hint.label}</span>}
        </div>
      )}
      {box && (
        <div
          className={`select-box ${box.x1 < box.x0 ? "crossing" : ""}`}
          style={{ left: Math.min(box.x0, box.x1), top: Math.min(box.y0, box.y1), width: Math.abs(box.x1 - box.x0), height: Math.abs(box.y1 - box.y0) }}
        />
      )}
      {placeAsset && tool === "place" && <div className="mode-pill">Placing {placeAsset.replace("_", " ")}. Click to drop, Esc when done.</div>}
    </div>
  );
}
