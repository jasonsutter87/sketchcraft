// The 3D view. Draws the engine's scene with three.js and turns mouse and
// keyboard input into engine commands. Z is up, units are meters.
import { useEffect, useRef, useState } from "react";
import { ask, confirm } from "./ask";
import { AxisGizmo } from "./gizmo";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { GTAOPass } from "three/examples/jsm/postprocessing/GTAOPass.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { Line2 } from "three/examples/jsm/lines/Line2.js";
import { LineGeometry } from "three/examples/jsm/lines/LineGeometry.js";
import { LineMaterial } from "three/examples/jsm/lines/LineMaterial.js";
import { LineSegments2 } from "three/examples/jsm/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/examples/jsm/lines/LineSegmentsGeometry.js";
import { edgePens } from "./viewport/linepens";
import { exec, type Annotation, type Reply, type Scene, type Vec3 } from "./engine";
import { CageEditor, type CageData, type CageUi } from "./cage";
import { CageBar } from "./cagebar";
import { guide, type Guide } from "./guide";
import { Icons } from "./icons";
import { AXES, AXIS_COLORS, closestOnLine, infer, rayFrom, setDrawingAxes, V, type Axis, type InferEnv, type Pick, type Snap } from "./inference";
import { combine, EMPTY, selIds, selMode, toggle, type Sel } from "./selection";
import { formatLength, parseLengths } from "./units";
import { autoTagColor, ACCENT, BACKGROUNDS } from "./viewport/colors";
import { styledEdges, averagedNormals, smoothNormals } from "./viewport/edges";
import { profileMaterial, textureFor } from "./viewport/materials";
import { UP, r4, arr, circleThrough, bulgeThrough, basis, arcPoints, offsetOutline } from "./viewport/geometry";
import { selectMarquee } from "./viewport/marquee";
import { isTypingTarget } from "./keymap";
import type { Tool, BuildSettings, FaceStyle, Backdrop, Styles, Cam, ViewportApi, PluginPoint, Props, Op, LabelSpec, Last } from "./viewport/types";
export { autoTagColor };
export type { Tool, BuildSettings, FaceStyle, Backdrop, Styles, Cam, ViewportApi, PluginPoint };








// display units come from the model; set whenever a scene arrives
let units = "m";
const fmt = (x: number) => formatLength(x, units);



/** The live props once the handlers have checked there is a scene. */
type Live = Props & { scene: Scene };

/** What the pointer handlers hand each tool's step: the event, the live props, the inference
 *  environment, the operation in progress and the current selection. */
interface DownArgs {
  e: PointerEvent;
  L: Live;
  E: InferEnv;
  op: Op | null;
  sel: Sel;
}
/** The same for a move, with the operation narrowed to the kind this step handles. */
interface MoveArgs<K extends Op["t"]> {
  e: PointerEvent;
  L: Live;
  E: InferEnv;
  op: Extract<Op, { t: K }>;
  lock: Axis | null;
}

export default function Viewport(props: Props) {
  const { scene, tool, styles, placeAsset } = props;
  const host = useRef<HTMLDivElement>(null);
  const labelLayer = useRef<HTMLDivElement>(null);
  const [hint, setHint] = useState<{ x: number; y: number; label: string; kind: string } | null>(null);
  const [box, setBox] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);

  const live = useRef(props);
  /** How many quick presses in a row (1 single, 2 double, 3 triple) and the last press */
  const clicks = useRef(1);
  const lastDown = useRef<{ t: number; x: number; y: number; button: number } | null>(null);
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
    /** SubD mode's cage and live surface */
    cage: THREE.Group;
    ground: THREE.Group;
    catcher: THREE.Mesh;
    pickMesh: THREE.Mesh | null;
    editMesh: THREE.Mesh | null;
    groundZ: number;
    /** face-me figures, turned toward the camera every frame */
    figures?: { id: number; obj: THREE.Group; mesh: THREE.Mesh; lines: THREE.LineSegments; facing: number }[];
  } | null>(null);

  /** camera tools: eye height, the drag in progress, keys held, view history */
  const camTool = useRef<{ eyeH: number; drag: { x: number; y: number; at: number } | null; keys: Set<string>; loop: number; history: Cam[]; at: number; box: HTMLDivElement | null }>({ eyeH: 1.68, drag: null, keys: new Set(), loop: 0, history: [], at: -1, box: null });
  /** the studio reflection for the physically based style (made the first time) */
  const envMap = useRef<THREE.Texture | null>(null);
  /** the red / green / blue lines at the drawing axes' origin */
  const axesGroup = useRef(new THREE.Group());
  /** the section plane last taken from the scene (id, place), so the cut follows it */
  const secKey = useRef("");
  /** point clouds on show: their group, and each one's points by id (with the version they're from) */
  const cloudGroup = useRef(new THREE.Group());
  const cloudObjs = useRef(new Map<number, { version: number; points: THREE.Points }>());
  /** SubD mode: the cage editor while a subdivision object is being shaped */
  const cageEd = useRef<CageEditor | null>(null);
  const lastPointer = useRef({ x: 0, y: 0 });
  /** lights: the sky, where the sun sits around the model, and a sun-study angle */
  const rig = useRef<{ hemi: THREE.HemisphereLight | null; c: THREE.Vector3; r: number; sun: [number, number] | null }>({ hemi: null, c: new THREE.Vector3(), r: 20, sun: null });
  const [cageUi, setCageUi] = useState<CageUi | null>(null);
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
  // the axis gizmo reads the live camera every frame (a stable function)
  const gizmoCamera = useRef(() => t.current?.camera ?? null).current;
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
  if (import.meta.env.DEV) Object.assign(window as unknown as Record<string, unknown>, { __sketchcraft: st, __sketchcraftView: t });

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
    controls.addEventListener("start", () => {
      if (backdropOn.current) showBackdrop(null);
      rememberView();
    });
    controls.addEventListener("end", () => rememberView());
    renderer.domElement.addEventListener("wheel", () => backdropOn.current && showBackdrop(null));

    const world = new THREE.Scene();
    const hemi = new THREE.HemisphereLight(0xffffff, 0xe2e4ec, 2.7);
    rig.current.hemi = hemi;
    world.add(hemi);
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
    // the drawing axes (moved and turned by the Axes tool)
    axesGroup.current.matrixAutoUpdate = false;
    for (const [to, c] of [[[300, 0, 0], AXIS_COLORS[0]], [[0, 300, 0], AXIS_COLORS[1]], [[0, 0, 300], AXIS_COLORS[2]]] as [Vec3, number][]) {
      const g = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), V(to)]);
      axesGroup.current.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: c, transparent: true, opacity: 0.55 })));
    }
    world.add(axesGroup.current);
    const catcher = new THREE.Mesh(new THREE.PlaneGeometry(2000, 2000), new THREE.ShadowMaterial({ opacity: 0.22 }));
    catcher.receiveShadow = true;
    catcher.visible = false;
    ground.add(catcher);
    world.add(ground);

    const model = new THREE.Group();
    const overlay = new THREE.Group();
    const preview = new THREE.Group();
    const cage = new THREE.Group();
    world.add(model, overlay, preview, cage, cloudGroup.current);

    t.current = { renderer, persp, ortho, camera: persp, controls, world, sun, model, overlay, preview, cage, ground, catcher, pickMesh: null, editMesh: null, groundZ: 0 };

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
      turnFigures();
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
  }, [styles.xray, styles.shadows, styles.edges, styles.parallel, styles.backEdges, styles.face, styles.profiles, styles.sketchy, styles.extensions, styles.background, styles.byTag, styles.notes, styles.hideRest, styles.lineWeights]);

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

  /** Is a note shown? Not when notes are off; with a section, only what's on
   *  the kept side (a floor plan shows that floor's notes, not the ones above
   *  or a storey below). */
  function noteShown(an: Annotation): boolean {
    if (live.current.styles.notes === false) return false;
    const plane = st.current.section;
    if (!plane) return true;
    const p = V(an.kind === "dimension" ? an.a : an.kind === "text" ? an.at : an.hinge);
    if (plane.normal.z < -0.99) {
      const cut = plane.constant; // -z + c = 0
      return p.z <= cut + 0.05 && p.z >= cut - 2.6;
    }
    return plane.distanceToPoint(p) >= -0.05;
  }

  function annotationSpecs(): LabelSpec[] {
    const s = live.current.scene;
    if (!s) return [];
    return s.annotations.filter(noteShown).flatMap((an) =>
      an.kind === "dimension"
        ? [{ key: `a${an.id}`, id: an.id, p: V(an.a).add(V(an.b)).multiplyScalar(0.5).add(V(an.offset)), text: fmt(V(an.a).distanceTo(V(an.b))) }]
        : an.kind === "text"
          ? [{ key: `a${an.id}`, id: an.id, p: V(an.at), text: an.text }]
          : [] // door swings are drawn, not labeled
    );
  }

  /** Per-triangle colors from each thing's tag (its own, or the nearest
   *  group it sits in that has one); untagged stays a quiet gray. */
  function tagColors(s: Scene): number[] {
    const parentOf = new Map(s.groups.map((g) => [g.id, g]));
    const tagOfGroup = (g: number | null): string | null => {
      for (let c = g, n = 0; c != null && n < 256; n++) {
        const x = parentOf.get(c);
        if (!x) break;
        if (x.tag) return x.tag;
        c = x.parent;
      }
      return null;
    };
    const faceGroup = new Map(s.faces.map((f) => [f.id, f.group]));
    const inst = new Map(s.instances.map((i) => [i.id, i]));
    const out: number[] = [];
    for (const owner of s.triFace) {
      const i = inst.get(owner);
      const tag = i ? i.tag ?? tagOfGroup(i.parent) : tagOfGroup(faceGroup.get(owner) ?? null);
      const c = (tag && s.tagColors?.[tag]) || (tag ? autoTagColor(tag) : [0.86, 0.86, 0.84]);
      out.push(c[0], c[1], c[2]);
    }
    return out;
  }

  // ---------------------------------------------------------------- SubD mode
  function enterCage(group: number) {
    const T = t.current;
    const s = live.current.scene;
    const d = s?.cages?.[String(group)];
    if (!T || !s || !d) return false;
    cageEd.current?.dispose();
    const colors = new Map(s.materials.map((m) => [m.name, m.color as [number, number, number]]));
    cageEd.current = new CageEditor({
      group,
      data: d as CageData,
      root: T.cage,
      host: host.current!,
      camera: () => t.current!.camera,
      colorOf: (m) => (m && colors.get(m)) || [0.82, 0.83, 0.85],
      run: live.current.run,
      onUi: setCageUi,
      toast: live.current.onToast,
    });
    if (import.meta.env.DEV) (window as unknown as { __cage: unknown }).__cage = cageEd.current;
    live.current.setSel(EMPTY);
    live.current.setTool("select");
    buildModel();
    drawOverlay();
    return true;
  }
  function exitCage() {
    cageEd.current?.dispose();
    cageEd.current = null;
    setCageUi(null);
    buildModel();
  }

  // ---------------------------------------------------------------- model
  /** Guides: long dashed lines and little crosses. */
  function addGuides(T: NonNullable<typeof t.current>, s: Scene) {
    for (const g of s.guides) {
      const p = V(g.p);
      const pts = g.dir
        ? [p.clone().addScaledVector(V(g.dir), -500), p.clone().addScaledVector(V(g.dir), 500)]
        : [p.clone().add(new THREE.Vector3(-0.15, 0, 0)), p.clone().add(new THREE.Vector3(0.15, 0, 0)), p.clone().add(new THREE.Vector3(0, -0.15, 0)), p.clone().add(new THREE.Vector3(0, 0.15, 0))];
      const line = new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineDashedMaterial({ color: 0x3e63dd, dashSize: 0.3, gapSize: 0.2 }));
      line.computeLineDistances();
      T.model.add(line);
    }
  }

  /** Dimensions and text leaders, always on top. */
  function addAnnotationLines(T: NonNullable<typeof t.current>, s: Scene) {
    const ann: number[] = [];
    for (const an of s.annotations.filter(noteShown)) {
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
  }

  /** Face-me figures (scale people): each its own object at its point. */
  function addFigures(T: NonNullable<typeof t.current>, s: Scene, clip: THREE.Plane[]) {
    T.figures = [];
    for (const f of s.figures ?? []) {
      const obj = new THREE.Group();
      obj.position.set(f.pivot[0], f.pivot[1], f.pivot[2]);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(f.positions, 3));
      const col: number[] = [];
      // the colors are sRGB; the renderer works in linear light
      const lin = (c: number) => Math.pow(c, 2.2);
      for (let i = 0; i < f.colors.length; i += 3) for (let k = 0; k < 3; k++) col.push(lin(f.colors[i]), lin(f.colors[i + 1]), lin(f.colors[i + 2]));
      g.setAttribute("color", new THREE.Float32BufferAttribute(col, 3));
      g.computeVertexNormals();
      const mesh = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide, clippingPlanes: clip, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 }));
      mesh.userData.owner = f.id;
      const lg = new THREE.BufferGeometry();
      lg.setAttribute("position", new THREE.Float32BufferAttribute(f.edges, 3));
      const lines = new THREE.LineSegments(lg, new THREE.LineBasicMaterial({ color: 0x1d1e24, clippingPlanes: clip }));
      obj.add(mesh, lines);
      T.model.add(obj);
      T.figures.push({ id: f.id, obj, mesh, lines, facing: f.facing });
    }
  }

  /** The invisible copy of what is being edited, for picking. */
  function addEditMesh(T: NonNullable<typeof t.current>, s: Scene) {
    T.editMesh = null;
    if (s.editing != null && s.editPositions.length) {
      const eg = new THREE.BufferGeometry();
      eg.setAttribute("position", new THREE.Float32BufferAttribute(s.editPositions, 3));
      T.editMesh = new THREE.Mesh(eg, new THREE.MeshBasicMaterial({ visible: false, side: THREE.DoubleSide }));
      T.model.add(T.editMesh);
    }
  }

  /** Put the ground under the model and aim the camera rig and the sun at it. */
  function settleGround(T: NonNullable<typeof t.current>, s: Scene) {
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
      rig.current.c.copy(c);
      rig.current.r = r;
      placeSun();
      T.sun.target.position.copy(c);
      const sc = T.sun.shadow.camera;
      sc.left = sc.bottom = -r;
      sc.right = sc.top = r;
      sc.near = 0.1;
      sc.far = r * 5;
      sc.updateProjectionMatrix();
    }
  }

  function buildModel() {
    const T = t.current;
    const s = live.current.scene;
    if (!T || !s) return;
    syncClouds();
    // the drawing axes follow the model's
    {
      const a = s.axes;
      const x = a ? V(a[1]) : new THREE.Vector3(1, 0, 0);
      const y = a ? V(a[2]) : new THREE.Vector3(0, 1, 0);
      setDrawingAxes(x, y);
      const o = a ? V(a[0]) : new THREE.Vector3();
      axesGroup.current.matrix.makeBasis(AXES[0], AXES[1], AXES[2]).setPosition(o);
    }
    // the cut follows the scene's active section plane
    {
      const act = s.sections?.find((x) => x.id === s.activeSection);
      const key = act ? `${act.id}:${act.origin}:${act.normal}` : "";
      if (key !== secKey.current) {
        secKey.current = key;
        st.current.section = act ? new THREE.Plane().setFromNormalAndCoplanarPoint(V(act.normal).normalize(), V(act.origin)) : null;
        live.current.onSection(!!act);
      }
    }
    T.model.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.model.clear();
    const xray = live.current.styles.xray;
    const clip = st.current.section ? [st.current.section] : [];

    // in SubD mode the object being shaped is drawn by its cage editor instead
    const shaping = cageEd.current?.o.group ?? null;
    let positions = s.positions;
    // see-through reference images have no solid face under them
    const ghost = new Set(s.textures.map((name, i) => ((s.materials.find((m) => m.name === name)?.opacity ?? 1) < 1 ? i : -1)).filter((i) => i >= 0));
    if (ghost.size && s.triTex?.length) {
      positions = positions.slice();
      for (let t = 0; t < s.triTex.length; t++) if (ghost.has(s.triTex[t])) for (let k = 3; k < 9; k++) positions[t * 9 + k] = positions[t * 9 + (k % 3)];
    }
    let edgeSegs = s.edges;
    if (shaping != null) {
      const hide = new Set(s.faces.filter((f) => f.group === shaping).map((f) => f.id));
      positions = s.positions.slice();
      for (let t = 0; t < s.triFace.length; t++) if (hide.has(s.triFace[t])) for (let k = 3; k < 9; k++) positions[t * 9 + k] = positions[t * 9 + (k % 3)];
      edgeSegs = s.edgeList.filter((e) => !e.soft && e.group !== shaping).flatMap((e) => [...e.a, ...e.b]);
    }
    // Hide Rest of Model: while a group or component is open, only it shows
    if (live.current.styles.hideRest && (s.activeGroup != null || s.editing != null)) {
      const inside = new Set<number>();
      if (s.activeGroup != null) {
        inside.add(s.activeGroup);
        for (let grew = true; grew; ) {
          grew = false;
          for (const g of s.groups) if (g.parent != null && inside.has(g.parent) && !inside.has(g.id)) (inside.add(g.id), (grew = true));
        }
      }
      const faceIn = new Set(s.faces.filter((f) => f.group != null && inside.has(f.group)).map((f) => f.id));
      const instIn = new Set(s.instances.filter((i) => i.id === s.editing || (i.parent != null && inside.has(i.parent))).map((i) => i.id));
      positions = positions === s.positions ? s.positions.slice() : positions;
      for (let t = 0; t < s.triFace.length; t++) {
        const o = s.triFace[t];
        if (!faceIn.has(o) && !instIn.has(o)) for (let k = 3; k < 9; k++) positions[t * 9 + k] = positions[t * 9 + (k % 3)];
      }
      edgeSegs = s.edgeList.filter((e) => !e.soft && e.group != null && inside.has(e.group)).flatMap((e) => [...e.a, ...e.b]);
      if (s.editing != null) edgeSegs = s.edges; // a component's edges come with it
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    // colors come per triangle; each corner gets its triangle's color
    const corners = (c: number[]) => {
      const out = new Float32Array(c.length * 3);
      for (let t = 0; t < c.length / 3; t++) for (let k = 0; k < 3; k++) out.set([c[t * 3], c[t * 3 + 1], c[t * 3 + 2]], t * 9 + k * 3);
      return new THREE.BufferAttribute(out, 3);
    };
    geo.setAttribute("color", corners(live.current.styles.byTag ? tagColors(s) : s.colors));
    geo.setAttribute("normal", new THREE.BufferAttribute(smoothNormals(s.positions), 3));
    const bgeo = geo.clone();
    bgeo.setAttribute("color", corners(s.backColors));
    const sty = live.current.styles;
    const faceStyle: FaceStyle = sty.face ?? "textured";
    const bg = BACKGROUNDS[sty.background ?? "default"];
    const common = { side: THREE.FrontSide as THREE.Side, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1, transparent: xray, opacity: xray ? 0.42 : 1, depthWrite: !xray, clippingPlanes: clip };
    // physically based: a soft studio around the model to reflect, made once
    const physical = faceStyle === "physical";
    if (physical && !envMap.current) envMap.current = new THREE.PMREMGenerator(T.renderer).fromScene(new RoomEnvironment(), 0.04).texture;
    T.world.environment = physical ? envMap.current : null;
    placeSun();
    if (physical) {
      // roughness and metalness per triangle, to every corner
      const pb = s.pbr && s.pbr.length * 3 === s.triFace.length * 6 ? s.pbr : null;
      const out = new Float32Array((s.positions.length / 3) * 2);
      for (let t = 0; t < s.triFace.length; t++) for (let k = 0; k < 3; k++) out.set(pb ? [pb[t * 2], pb[t * 2 + 1]] : [0.7, 0], (t * 3 + k) * 2);
      geo.setAttribute("pbr", new THREE.BufferAttribute(out, 2));
      bgeo.setAttribute("pbr", new THREE.BufferAttribute(out, 2));
    }
    const mat = (side: THREE.Side): THREE.Material => {
      if (physical) {
        const m = new THREE.MeshStandardMaterial({ ...common, side, vertexColors: true, envMapIntensity: 1 });
        m.onBeforeCompile = (sh) => {
          sh.vertexShader = sh.vertexShader.replace("#include <common>", "#include <common>\nattribute vec2 pbr;\nvarying vec2 vPbr;").replace("#include <begin_vertex>", "#include <begin_vertex>\nvPbr = pbr;");
          sh.fragmentShader = sh.fragmentShader
            .replace("#include <common>", "#include <common>\nvarying vec2 vPbr;")
            .replace("#include <roughnessmap_fragment>", "float roughnessFactor = max(vPbr.x, 0.04);")
            .replace("#include <metalnessmap_fragment>", "float metalnessFactor = vPbr.y;");
        };
        return m;
      }
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
      pg.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
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
          new THREE.MeshLambertMaterial({ map: textureFor(mat.name, mat.texture.data), side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: 0, polygonOffsetUnits: 0, transparent: xray || (mat.opacity ?? 1) < 1, opacity: Math.min(xray ? 0.42 : 1, mat.opacity ?? 1), depthWrite: !xray && (mat.opacity ?? 1) >= 1, clippingPlanes: clip })
        );
        mesh.receiveShadow = live.current.styles.shadows;
        T.model.add(mesh);
      }
    }

    if (sty.edges || faceStyle === "wireframe" || faceStyle === "hiddenline") {
      const eg = new THREE.BufferGeometry();
      const segs = sty.sketchy || sty.extensions ? styledEdges(edgeSegs, size, !!sty.extensions || !!sty.sketchy, !!sty.sketchy) : edgeSegs;
      // line weights: segments with their own pen are drawn below, one thick line object per pen
      const pens = sty.lineWeights !== false && shaping == null ? edgePens(s) : null;
      const plain = pens ? pens.plain : segs;
      eg.setAttribute("position", new THREE.Float32BufferAttribute(plain, 3));
      T.model.add(new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color: bg.line, clippingPlanes: clip, transparent: !!sty.sketchy, opacity: sty.sketchy ? 0.8 : 1 })));
      if (pens) {
        const el = T.renderer.domElement;
        for (const b of pens.buckets) {
          const lg = new LineSegmentsGeometry();
          lg.setPositions(b.pos);
          const [r, g, bl] = b.color;
          const m = new LineMaterial({ color: new THREE.Color(r / 255, g / 255, bl / 255), linewidth: b.px, dashed: b.dash.length > 0, dashSize: b.dash[0] ?? 1, gapSize: b.dash[1] ?? 0, worldUnits: false });
          m.clipping = true;
          m.clippingPlanes = clip;
          m.resolution.set(el.clientWidth, el.clientHeight);
          const ls = new LineSegments2(lg, m);
          if (b.dash.length > 0) ls.computeLineDistances();
          ls.userData.pen = true;
          T.model.add(ls);
        }
      }
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
        // a tag's hatch: its cut walls on paper-white, with the pattern's lines in the hatch colour
        const paper = (r.hatchFill as number[] | undefined) ?? [];
        if (paper.length) {
          const g = new THREE.BufferGeometry();
          g.setAttribute("position", new THREE.Float32BufferAttribute(shift(paper), 3));
          const m = new THREE.Mesh(g, new THREE.MeshBasicMaterial({ color: 0xf6f5f0, side: THREE.DoubleSide }));
          m.userData.sectionFill = true;
          m.userData.sectionHatchFill = true;
          T2.model.add(m);
        }
        const lift2 = plane.normal.clone().multiplyScalar(0.003);
        for (const h of (r.hatch as { tag: string; color: string; lines: number[] }[] | undefined) ?? []) {
          if (!h.lines.length) continue;
          const g = new THREE.BufferGeometry();
          g.setAttribute("position", new THREE.Float32BufferAttribute(h.lines.map((v, i) => v + [lift2.x, lift2.y, lift2.z][i % 3]), 3));
          const ls = new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: new THREE.Color(h.color || "#1a1a1f") }));
          ls.userData.sectionHatch = h.tag;
          T2.model.add(ls);
        }
        if (lines.length) {
          const g = new THREE.BufferGeometry();
          g.setAttribute("position", new THREE.Float32BufferAttribute(shift(lines), 3));
          T2.model.add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: 0x0b0c10 })));
        }
      });
    }
    if (live.current.styles.backEdges && !xray && edgeSegs.length) {
      // the same edges again, dashed and drawn through faces
      const bg = new THREE.BufferGeometry();
      bg.setAttribute("position", new THREE.Float32BufferAttribute(edgeSegs, 3));
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
    addGuides(T, s);
    addAnnotationLines(T, s);
    st.current.labels = annotationSpecs();
    syncLabels();
    T.pickMesh = front;

    addFigures(T, s, clip);

    addEditMesh(T, s);

    settleGround(T, s);
  }

  useEffect(() => {
    if (!t.current || !scene) return;
    units = scene.units || "m";
    const ed = cageEd.current;
    if (ed) {
      const d = scene.cages?.[String(ed.o.group)];
      if (d) ed.setData(d);
      else exitCage();
    }
    buildModel();
    if (!st.current.fitted || st.current.fitNext) {
      st.current.fitted = true;
      st.current.fitNext = false;
      if (scene.positions.length) fit();
    }
    drawOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene]);

  // a plugin tool's preview arrives after the move that asked for it: draw it then
  useEffect(() => {
    const P = props.pluginTool;
    if (props.tool !== "plugin" || !P || !t.current) return;
    clearPreview();
    drawPluginPreview(P.preview);
    const at = st.current.snap?.p;
    if (P.anchor && at) t.current.preview.add(polyline([V(P.anchor), at], 0x1d1e24));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.pluginTool?.preview, props.pluginTool?.anchor, props.tool]);

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
    return { camera: T.camera, rect: rect(), scene: sceneNow(), pick, groundZ: T.groundZ, cloudPoint: cloudObjs.current.size ? cloudPoint : undefined };
  }

  /** the nearest shown scan point within a few pixels of the cursor */
  function cloudPoint(e: { clientX: number; clientY: number }): THREE.Vector3 | null {
    const T = t.current!;
    const shown = cloudGroup.current.children.filter((c) => c.visible);
    if (!shown.length) return null;
    const r = rect();
    const rc = new THREE.Raycaster();
    rc.setFromCamera(new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1), T.camera);
    // ~6 px at the distance being looked at
    const dist = T.camera.position.distanceTo(T.controls.target);
    const px = T.camera === T.persp ? (2 * dist * Math.tan(THREE.MathUtils.degToRad(T.persp.fov / 2))) / r.height : (T.ortho.top - T.ortho.bottom) / T.ortho.zoom / r.height;
    rc.params.Points = { threshold: px * 6 };
    const hits = rc.intersectObjects(shown, false);
    if (!hits.length) return null;
    // the one closest to the ray, among the nearest few
    const near = hits.slice(0, 64).sort((a, b) => (a.distanceToRay ?? 0) - (b.distanceToRay ?? 0) || a.distance - b.distance)[0];
    return near.point.clone();
  }

  /** show the scene's clouds: fetch points for new or changed ones, drop deleted ones */
  function syncClouds() {
    const s = live.current.scene;
    const list = s?.clouds ?? [];
    const keep = new Set(list.map((c) => c.id));
    for (const [id, o] of cloudObjs.current) {
      if (!keep.has(id)) {
        cloudGroup.current.remove(o.points);
        o.points.geometry.dispose();
        (o.points.material as THREE.Material).dispose();
        cloudObjs.current.delete(id);
      }
    }
    for (const c of list) {
      const have = cloudObjs.current.get(c.id);
      if (have && have.version === c.version) {
        have.points.visible = !c.hidden;
        continue;
      }
      const version = c.version;
      exec({ cmd: "cloud_data", id: c.id }).then((r) => {
        if (!r.ok) return;
        const now = live.current.scene?.clouds?.find((x) => x.id === c.id);
        if (!now || now.version !== version) return;
        const bytes = (b64: string) => {
          const bin = atob(b64);
          const out = new Uint8Array(bin.length);
          for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
          return out;
        };
        const pos = new Float32Array(bytes(r.points as string).buffer);
        const col = bytes((r.colors as string) ?? "");
        const g = new THREE.BufferGeometry();
        g.setAttribute("position", new THREE.BufferAttribute(pos, 3));
        const colored = col.length === pos.length;
        if (colored) g.setAttribute("color", new THREE.BufferAttribute(col, 3, true));
        g.computeBoundingSphere();
        const pts = new THREE.Points(g, new THREE.PointsMaterial({ size: 2, sizeAttenuation: false, vertexColors: colored, color: colored ? 0xffffff : 0x6e7f99 }));
        pts.userData.cloud = c.id;
        pts.visible = !now.hidden;
        const old = cloudObjs.current.get(c.id);
        if (old) {
          cloudGroup.current.remove(old.points);
          old.points.geometry.dispose();
          (old.points.material as THREE.Material).dispose();
        }
        cloudGroup.current.add(pts);
        cloudObjs.current.set(c.id, { version, points: pts });
        // a first cloud in an empty view: look at it
        if (!live.current.scene?.positions.length && cloudObjs.current.size === 1) fit();
      });
    }
  }

  function ray(e: { clientX: number; clientY: number }): THREE.Ray {
    return rayFrom({ camera: t.current!.camera, rect: rect() }, e);
  }

  /** Turn the face-me figures toward the camera; a selected one is outlined. */
  function turnFigures() {
    const T = t.current;
    if (!T?.figures?.length) return;
    const sel = live.current.sel;
    for (const f of T.figures) {
      const d = T.camera.position.clone().sub(f.obj.position);
      if (Math.hypot(d.x, d.y) > 1e-6) f.obj.rotation.z = Math.atan2(d.y, d.x) - f.facing;
      const on = !!sel?.instances.includes(f.id);
      (f.lines.material as THREE.LineBasicMaterial).color.setHex(on ? 0x6b5cff : 0x1d1e24);
    }
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
    const figs = (T.figures ?? []).map((f) => f.mesh);
    let hits = rc.intersectObjects([...(T.editMesh ? [T.editMesh] : []), T.pickMesh, ...figs], false);
    mat.side = side;
    // ignore what the section plane cut away
    if (st.current.section) hits = hits.filter((h) => st.current.section!.distanceToPoint(h.point) >= -1e-4);
    const hit = hits[0];
    if (!hit || hit.faceIndex == null) return null;
    const owner = hit.object.userData.owner != null ? (hit.object.userData.owner as number) : hit.object === T.editMesh ? s.editTriFace[hit.faceIndex] : s.triFace[hit.faceIndex];
    return { face: faceById(owner), owner, point: hit.point.clone(), distance: hit.distance };
  }

  /** A corner of loose geometry (or of the open group) under the cursor, for
   * Move: its vertex id, where it is, and the far ends of its edges. */
  function vertexUnder(e: { clientX: number; clientY: number }): { id: number; p: THREE.Vector3; far: THREE.Vector3[] } | null {
    const s = sceneNow();
    if (s.editing != null) return null;
    const sn = infer(env(), e, {});
    if (sn.kind !== "endpoint") return null;
    const open = s.activeGroup ?? null;
    let id: number | null = null;
    const far: THREE.Vector3[] = [];
    for (const ed of s.edgeList) {
      if ((ed.group ?? null) !== open || ed.va == null || ed.vb == null) continue;
      if (V(ed.a).distanceTo(sn.p) < 1e-6) {
        id = ed.va;
        far.push(V(ed.b));
      } else if (V(ed.b).distanceTo(sn.p) < 1e-6) {
        id = ed.vb;
        far.push(V(ed.a));
      }
    }
    return id == null ? null : { id, p: sn.p.clone(), far };
  }

  /** The whole curve an edge belongs to (an arc, a circle, a welded run). */
  function curveOf(id: number): number[] {
    const s = sceneNow();
    const c = s.edgeList.find((x) => x.id === id)?.curve;
    return c == null ? [id] : s.edgeList.filter((x) => x.curve === c).map((x) => x.id);
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
      // edges inside a closed group belong to the group: the click takes the group.
      // Flip edge is the exception: it works on terrain, which is always a group (like the other Sandbox tools)
      if (live.current.tool !== "flipedge" && selectableGroup(ed.group) != null) continue;
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

  // ---- the hierarchy: groups inside groups --------------------------------
  /** A group and everything nested in it. */
  function subtree(g: number): Set<number> {
    const s = sceneNow();
    const out = new Set([g]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const x of s.groups) if (x.parent != null && out.has(x.parent) && !out.has(x.id)) (out.add(x.id), (grew = true));
    }
    return out;
  }
  /** What a click on something in group `g` selects: the group one level
   *  inside the one that's open (the top group when none is), or nothing
   *  when `g` is the open group itself (then you pick its faces). */
  function selectableGroup(g: number | null): number | null {
    const s = sceneNow();
    if (g == null) return null;
    const parentOf = new Map(s.groups.map((x) => [x.id, x.parent]));
    const chain: number[] = [];
    for (let c: number | null | undefined = g; c != null && chain.length < 256; c = parentOf.get(c)) chain.push(c);
    if (s.activeGroup == null) return chain[chain.length - 1];
    const i = chain.indexOf(s.activeGroup);
    if (i === 0) return null;
    if (i > 0) return chain[i - 1];
    return chain[chain.length - 1];
  }
  function groupFaces(g: number): Set<number> {
    const tree = subtree(g);
    const s = sceneNow();
    const out = new Set(s.faces.filter((f) => f.group != null && tree.has(f.group)).map((f) => f.id));
    // components placed inside it light up with it
    for (const i of s.instances) if (i.parent != null && tree.has(i.parent)) out.add(i.id);
    return out;
  }

  function ownersOf(sel: Sel): Set<number> {
    const o = new Set<number>([...sel.faces, ...sel.instances]);
    for (const g of sel.groups) groupFaces(g).forEach((f) => o.add(f));
    return o;
  }

  function selectionLines(sel: Sel): number[] {
    const s = sceneNow();
    const out: number[] = [];
    const ids = new Set(sel.edges);
    for (const e of s.edgeList) if (ids.has(e.id)) out.push(...e.a, ...e.b);
    if (s.editing == null && (sel.groups.length || sel.instances.length)) {
      const triangleOwners = new Set(s.triFace);
      const closedOwners = new Set(sel.instances.filter((id) => !triangleOwners.has(id)));
      const wireGroups = new Set<number>();
      for (const group of sel.groups) {
        const descendants = subtree(group);
        const owners = groupFaces(group);
        if (![...owners].some((id) => triangleOwners.has(id))) descendants.forEach((id) => wireGroups.add(id));
        for (const inst of s.instances) {
          if (inst.parent != null && descendants.has(inst.parent) && !triangleOwners.has(inst.id)) closedOwners.add(inst.id);
        }
      }
      for (const edge of s.edgeList) if (edge.group != null && wireGroups.has(edge.group)) closedOwners.add(edge.id);
      s.edgeIds.forEach((id, i) => {
        if (closedOwners.has(id) && !ids.has(id)) out.push(...s.edges.slice(i * 6, i * 6 + 6));
      });
    }
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
      const seg: number[] = [];
      for (const id of curveOf(he)) {
        const e = s.edgeList.find((x) => x.id === id);
        if (e) seg.push(...e.a, ...e.b);
      }
      if (seg.length) T.overlay.add(lines(seg, tool === "erase" ? RED : ACCENT, false));
    }
  }

  /** Work out the guidance for the tool and step in progress. */
  function updateCoach() {
    const L = live.current;
    const op = st.current.op;
    const stage = op?.t === "axes" ? (op.x ? 2 : 1) : op?.t === "arc" ? (op.b ? 2 : 1) : op?.t === "carc" ? (op.a ? 2 : 1) : op?.t === "arc3" ? (op.m ? 2 : 1) : op?.t === "rrect" ? (op.b ? 2 : 1) : op?.t === "rotate" ? (op.ref ? 2 : 1) : op?.t === "dim" ? (op.b ? 2 : 1) : undefined;
    const g =
      L.tool === "plugin" && L.pluginTool
        ? { title: L.pluginTool.name, step: L.pluginTool.hint || "Click in the model.", keys: [["Esc", "leave the tool"]] as [string, string][] }
        : guide(L.tool, { op: op?.t ?? null, stage, selected: selIds(selNow()).length, placing: L.placeAsset });
    const key = `${g.title}|${g.step}|${g.keys.length}`;
    if (key !== coachKey.current) {
      coachKey.current = key;
      setCoach(g);
    }
  }

  /** What hovering this owner would select (a whole group, unless it's open). */
  function hoverOwners(owner: number): Set<number> {
    const inst = sceneNow().instances.find((i) => i.id === owner);
    const g = selectableGroup(inst ? inst.parent : groupOfFace(owner));
    if (g != null) return groupFaces(g);
    return new Set([owner]);
  }

  /** The selectable thing for a pick: a component placement, a closed group, or a face. */
  function entityOf(p: Pick): { kind: keyof Sel; id: number } {
    const s = sceneNow();
    const inst = !p.face ? s.instances.find((i) => i.id === p.owner) : undefined;
    if (inst) {
      // a component inside a closed group: the click takes the group
      const g = selectableGroup(inst.parent);
      return g != null ? { kind: "groups", id: g } : { kind: "instances", id: p.owner };
    }
    const g = selectableGroup(groupOfFace(p.owner));
    if (g != null) return { kind: "groups", id: g };
    return { kind: "faces", id: p.owner };
  }

  /** A snapped point, as a plugin tool gets it. */
  function pluginPoint(sn: Snap, e: { shiftKey: boolean; altKey: boolean }): PluginPoint {
    return { point: arr(sn.p) as [number, number, number], snap: sn.kind, face: sn.face?.id ?? null, normal: arr(sn.normal) as [number, number, number], shift: e.shiftKey, alt: e.altKey };
  }

  /** Lines a plugin tool asked to show. */
  function drawPluginPreview(segs: [number, number, number][][]) {
    const T = t.current;
    if (!T || !segs.length) return;
    const seg: number[] = [];
    for (const l of segs) if (Array.isArray(l) && l.length === 2) seg.push(...l[0], ...l[1]);
    if (seg.length) T.preview.add(lines(seg, ACCENT, false));
  }
  const T0 = () => t.current!;

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
    const s = live.current.scene;
    const b = new THREE.Box3();
    const pos = owners ? trianglesOf(owners) : s?.positions ?? [];
    for (let i = 0; i < pos.length; i += 3) b.expandByPoint(new THREE.Vector3(pos[i], pos[i + 1], pos[i + 2]));
    // scans count too (when not fitting a selection)
    if (!owners) for (const c of s?.clouds ?? []) if (!c.hidden) b.expandByPoint(V(c.lo)).expandByPoint(V(c.hi));
    if (b.isEmpty()) b.set(new THREE.Vector3(-5, -5, 0), new THREE.Vector3(5, 5, 3));
    fitTo(b);
  }

  function fitTo(b: THREE.Box3) {
    const T = t.current;
    if (!T) return;
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

  /** Orbit around the target by a drag of (dx, dy) pixels (the axis gizmo). */
  function orbitBy(dx: number, dy: number) {
    const T = t.current;
    if (!T) return;
    const target = T.controls.target;
    const off = T.camera.position.clone().sub(target);
    const r = off.length();
    if (r < 1e-9) return;
    let theta = Math.atan2(off.y, off.x) - dx * 0.009;
    let phi = Math.acos(THREE.MathUtils.clamp(off.z / r, -1, 1)) - dy * 0.009;
    phi = THREE.MathUtils.clamp(phi, 0.001, Math.PI - 0.001);
    off.set(r * Math.sin(phi) * Math.cos(theta), r * Math.sin(phi) * Math.sin(theta), r * Math.cos(phi));
    T.camera.position.copy(target).add(off);
    if (T.camera === T.ortho) T.persp.position.copy(T.camera.position);
    T.camera.lookAt(target);
    T.controls.update();
  }

  /** Look at the model along an axis: from +z is the top view, -y the front. */
  function lookAlong(dir: [number, number, number]) {
    const T = t.current;
    if (!T) return;
    if (backdropOn.current) showBackdrop(null);
    rememberView();
    // straight up or down: a hair off, so "up" on screen stays well defined
    const d = Math.abs(dir[2]) > 0.99 ? new THREE.Vector3(0, -0.001, dir[2]).normalize() : V(dir);
    const dist = T.camera.position.distanceTo(T.controls.target);
    goTo({ eye: arr(T.controls.target.clone().addScaledVector(d, dist)), target: arr(T.controls.target), fov: T.persp.fov });
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

  // ---------------------------------------------------------------- lights and frames
  /** The sun where a sun study puts it (compass azimuth, altitude), or the usual light. */
  function placeSun() {
    const T = t.current!;
    const { c, r, sun, hemi } = rig.current;
    let dir = new THREE.Vector3(0.35, -0.55, 0.76);
    if (sun) {
      const [az, al] = sun.map(THREE.MathUtils.degToRad);
      // +Y north, +X east
      dir = new THREE.Vector3(Math.sin(az) * Math.cos(al), Math.cos(az) * Math.cos(al), Math.sin(al));
      const up = Math.max(0, Math.sin(al));
      // shadows read best when the sun carries most of the light; dusk fades it
      T.sun.intensity = up > 0 ? 1.2 + 1.8 * Math.min(1, up * 2.5) : 0;
      T.sun.color.setHSL(0.09, Math.max(0, 0.7 - up * 1.6), 0.5 + Math.min(0.5, up * 1.5));
      if (hemi) hemi.intensity = 0.7 + 1.2 * Math.min(1, up * 3);
    } else {
      T.sun.intensity = 1.3;
      T.sun.color.set(0xffffff);
      if (hemi) hemi.intensity = 2.7;
    }
    // physically based: the studio reflection carries the ambient light, not the sky dome
    if (hemi && live.current.styles.face === "physical") hemi.intensity *= 0.25;
    T.sun.position.copy(c.clone().add(dir.normalize().multiplyScalar(r * 2)));
  }
  function setSun(s: [number, number] | null) {
    rig.current.sun = s;
    placeSun();
  }
  /** Put the camera exactly here (no glide): timeline playback. */
  function setCamera(c: Cam) {
    const T = t.current!;
    setFov(c.fov || 40);
    T.camera.position.set(...c.eye);
    T.controls.target.set(...c.target);
    if (T.camera === T.ortho) {
      T.persp.position.copy(T.camera.position);
      syncOrtho(false);
    }
    T.camera.lookAt(T.controls.target);
  }
  /** Render one frame at w × h and return it as a JPEG data URL (with a caption). */
  function captureFrame(w: number, h: number, caption?: string): string {
    const T = t.current!;
    const r = T.renderer;
    const prev = r.getSize(new THREE.Vector2());
    const ratio = r.getPixelRatio();
    r.setPixelRatio(1);
    r.setSize(w, h, false);
    const cam = T.camera as THREE.PerspectiveCamera;
    const aspect = cam.aspect;
    if (cam.isPerspectiveCamera) {
      cam.aspect = w / h;
      cam.updateProjectionMatrix();
    }
    T.controls.update();
    // a clean picture: no selection, snaps or SubD cage
    const hide = [T.overlay, T.preview, T.cage];
    const was = hide.map((o) => o.visible);
    hide.forEach((o) => (o.visible = false));
    r.render(T.world, T.camera);
    hide.forEach((o, i) => (o.visible = was[i]));
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = c.getContext("2d")!;
    ctx.drawImage(r.domElement, 0, 0, w, h);
    if (caption) {
      const s = Math.round(h / 22);
      ctx.font = `600 ${s}px system-ui, sans-serif`;
      const tw = ctx.measureText(caption).width;
      ctx.fillStyle = "rgba(0,0,0,0.45)";
      ctx.beginPath();
      ctx.roundRect(s * 0.8, h - s * 2.6, tw + s * 1.2, s * 1.7, s * 0.4);
      ctx.fill();
      ctx.fillStyle = "#fff";
      ctx.fillText(caption, s * 1.4, h - s * 1.35);
    }
    r.setPixelRatio(ratio);
    r.setSize(prev.x, prev.y, false);
    if (cam.isPerspectiveCamera) {
      cam.aspect = aspect;
      cam.updateProjectionMatrix();
    }
    return c.toDataURL("image/jpeg", 0.92);
  }

  // ---------------------------------------------------------------- camera tools
  const CAMERA_TOOLS = ["position", "lookaround", "walk", "zoomwindow"];
  /** Remember where the camera was (for Previous / Next view). */
  function rememberView() {
    const c = camTool.current;
    const now = camera();
    const last = c.history[c.at];
    if (last && V(last.eye).distanceTo(V(now.eye)) < 1e-3 && V(last.target).distanceTo(V(now.target)) < 1e-3) return;
    c.history = c.history.slice(0, c.at + 1);
    c.history.push(now);
    if (c.history.length > 60) c.history.shift();
    c.at = c.history.length - 1;
  }
  function stepView(by: number) {
    const c = camTool.current;
    rememberView();
    const i = c.at + by;
    if (i < 0 || i >= c.history.length) return live.current.onToast(by < 0 ? "No earlier view" : "No later view");
    c.at = i;
    goTo(c.history[i]);
  }
  /** Where the floor is under a point (the model's faces, else the ground). */
  function floorBelow(p: THREE.Vector3): number {
    const T = t.current!;
    const rc = new THREE.Raycaster(p.clone().add(new THREE.Vector3(0, 0, 0.3)), new THREE.Vector3(0, 0, -1), 0, 60);
    const hit = T.pickMesh ? rc.intersectObject(T.pickMesh, false)[0] : undefined;
    return hit ? hit.point.z : T.groundZ;
  }
  /** Is something solid within `d` meters ahead (walls stop a walk)? */
  function blocked(from: THREE.Vector3, dir: THREE.Vector3, d: number): boolean {
    const T = t.current!;
    if (!T.pickMesh) return false;
    const rc = new THREE.Raycaster(from, dir.clone().normalize(), 0, d);
    return rc.intersectObject(T.pickMesh, false).length > 0;
  }
  /** Turn the eye: yaw (around blue) and pitch (up/down), standing still. */
  function look(dyaw: number, dpitch: number) {
    const T = t.current!;
    const eye = T.camera.position.clone();
    const dir = T.controls.target.clone().sub(eye).normalize();
    let yaw = Math.atan2(dir.y, dir.x) - dyaw;
    let pitch = Math.asin(Math.max(-1, Math.min(1, dir.z))) - dpitch;
    pitch = Math.max(-1.45, Math.min(1.45, pitch));
    const nd = new THREE.Vector3(Math.cos(pitch) * Math.cos(yaw), Math.cos(pitch) * Math.sin(yaw), Math.sin(pitch));
    T.controls.target.copy(eye.clone().addScaledVector(nd, 2));
    T.camera.lookAt(T.controls.target);
  }
  /** One walking step: forward (m) and turn (radians), keeping eye height over the floor. */
  function walk(forward: number, turn: number, through: boolean) {
    const T = t.current!;
    if (turn) look(turn, 0);
    if (!forward) return;
    const eye = T.camera.position.clone();
    const dir = T.controls.target.clone().sub(eye);
    dir.z = 0;
    if (dir.lengthSq() < 1e-9) return;
    dir.normalize();
    const step = dir.clone().multiplyScalar(forward);
    // walls stop you (Option walks through), checked at knee and chest height
    if (!through) {
      const sign = Math.sign(forward);
      const probe = dir.clone().multiplyScalar(sign);
      const knee = eye.clone();
      knee.z -= camTool.current.eyeH * 0.7;
      if (blocked(eye, probe, Math.abs(forward) + 0.35) || blocked(knee, probe, Math.abs(forward) + 0.35)) return;
    }
    const next = eye.clone().add(step);
    // keep to eye height over what's underfoot (stairs climb, slopes follow)
    const floor = floorBelow(new THREE.Vector3(next.x, next.y, next.z - camTool.current.eyeH + 0.45));
    const want = floor + camTool.current.eyeH;
    next.z += (want - next.z) * 0.35;
    const d = next.clone().sub(T.camera.position);
    T.camera.position.copy(next);
    T.controls.target.add(d);
  }
  function walkLoop() {
    const c = camTool.current;
    if (c.loop) return;
    let last = performance.now();
    const tick = () => {
      const now = performance.now();
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const k = c.keys;
      const run = k.has("Shift") ? 3 : 1;
      let fwd = 0, turn = 0;
      if (k.has("ArrowUp") || k.has("w")) fwd += 1.4 * run * dt;
      if (k.has("ArrowDown") || k.has("s")) fwd -= 1.4 * run * dt;
      if (k.has("ArrowLeft") || k.has("a")) turn -= 1.6 * dt;
      if (k.has("ArrowRight") || k.has("d")) turn += 1.6 * dt;
      // dragging: up/down walks, left/right turns, further from the press = faster
      if (c.drag && live.current.tool === "walk") {
        const p = c.drag;
        // above the press walks forward, below walks back
        fwd += ((p.y - lastPointer.current.y) / 80) * 1.4 * run * dt;
        turn += ((lastPointer.current.x - p.x) / 120) * 1.4 * dt;
      }
      if (fwd || turn) walk(fwd, turn, k.has("Alt"));
      if (k.size || c.drag) c.loop = requestAnimationFrame(tick);
      else c.loop = 0;
    };
    c.loop = requestAnimationFrame(tick);
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
    // camera tools: eye height (Position, Walk, Look Around), field of view (Zoom)
    if (["position", "walk", "lookaround"].includes(L.tool) && lens.length) {
      const h = lens[0];
      if (h > 0.1 && h < 100) {
        const T = t.current!;
        const d = h - camTool.current.eyeH;
        camTool.current.eyeH = h;
        T.camera.position.z += d;
        T.controls.target.z += d;
        L.onToast(`Eye height ${formatLength(h, units)}`);
      }
      return;
    }
    if (L.tool === "zoom") {
      const mm = /^(\d+(?:\.\d+)?)\s*mm$/i.exec(s);
      const deg = mm ? (2 * Math.atan(12 / Number(mm[1])) * 180) / Math.PI : nums[0];
      if (deg > 1 && deg < 170) {
        setFov(deg);
        L.onToast(mm ? `${mm[1]} mm lens (${deg.toFixed(0)}° field of view)` : `Field of view ${deg.toFixed(0)}°`);
      }
      return;
    }
    // Tape Measure, just measured: typing the real length resizes the model
    if (!op && L.tool === "tape" && last?.t === "tape" && lens.length && lens[0] > 0) {
      const k = lens[0] / last.d;
      const scene = sceneNow();
      const what = scene.editing != null ? "this component" : scene.activeGroup != null ? "this group" : "the model";
      if (!(await confirm(`Resize ${what}?`, `It measured ${fmt(last.d)}; making that ${fmt(lens[0])} scales ${what} ×${k.toFixed(4)}.`, "Resize"))) return;
      await run({ cmd: "resize", factor: k });
      st.current.last = { t: "tape", d: lens[0] };
      L.onToast(`Resized ${what} ×${k.toFixed(4)}`);
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
      case "carc":
        if (!op.a || !lens.length) return;
        // typed: the angle in degrees
        op.ang = THREE.MathUtils.degToRad(lens[0]) * Math.sign(op.ang || 1);
        return void (await commitCarc());
      case "rrect":
        if (!op.b || !lens.length) return;
        op.h = (op.h.lengthSq() > 1e-12 ? op.h.clone().normalize() : basis(op.b.clone().sub(op.a).normalize())[0]).multiplyScalar(Math.abs(lens[0]));
        return void (await commitRrect());
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
    fitBox: (lo, hi) => {
      st.current.fitNext = false;
      st.current.fitted = true;
      fitTo(new THREE.Box3(V(lo), V(hi)));
    },
    view,
    camera,
    goTo,
    clearSection: () => void live.current.run({ cmd: "section_off" }),
    setSection: (origin: Vec3, normal: Vec3) => {
      // a scene's cut: the same plane if there is one, else a new one
      const s = live.current.scene;
      const n = V(normal).normalize();
      const same = s?.sections?.find((x) => V(x.normal).normalize().distanceTo(n) < 1e-6 && Math.abs(V(x.origin).sub(V(origin)).dot(n)) < 1e-6);
      void live.current.run(same ? { cmd: "section_edit", id: same.id, active: true } : { cmd: "section_plane", origin, normal: [n.x, n.y, n.z] });
    },
    sectionPlane: () => {
      const p = st.current.section;
      if (!p) return null;
      const o = p.coplanarPoint(new THREE.Vector3());
      return { origin: [o.x, o.y, o.z], normal: [p.normal.x, p.normal.y, p.normal.z] };
    },
    editCage: (id: number) => enterCage(id),
    previousView: () => stepView(-1),
    setCamera,
    setSun,
    captureFrame,
    nextView: () => stepView(1),
    exitCage: () => exitCage(),
    inCage: () => cageEd.current != null,
    planCut: (height: number) => {
      // keep what's below: normal points down, plane at z = height (one "Floor plan" plane, reused)
      const plan = live.current.scene?.sections?.find((x) => x.name === "Floor plan");
      const at = { origin: [0, 0, height], normal: [0, 0, -1] };
      void live.current.run(plan ? { cmd: "section_edit", id: plan.id, ...at, active: true } : { cmd: "section_plane", name: "Floor plan", ...at });
    },
    moveSection: (by: number) => {
      const p = st.current.section;
      if (!p) return;
      // normal points into what's kept; moving "by" along it
      p.constant -= by;
      buildModel();
      live.current.onSection(true);
      const id = live.current.scene?.activeSection;
      if (id != null) {
        const o = p.coplanarPoint(new THREE.Vector3());
        secKey.current = ""; // let the scene's answer settle it
        void live.current.run({ cmd: "section_edit", id, origin: [o.x, o.y, o.z] });
      }
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

  /** center Arc / Pie: the arc from the start, swung `ang` about the center */
  async function commitCarc() {
    const op = st.current.op;
    if (op?.t !== "carc" || !op.a || Math.abs(op.ang) < 1e-4) return;
    st.current.op = null;
    clearPreview();
    const r = op.a.distanceTo(op.c);
    const u = op.a.clone().sub(op.c).normalize();
    const v = op.n.clone().cross(u);
    const b = op.c.clone().addScaledVector(u, Math.cos(op.ang) * r).addScaledVector(v, Math.sin(op.ang) * r);
    const bulge = -Math.sign(op.ang) * r * (1 - Math.cos(op.ang / 2));
    await run({ cmd: "arc", start: arr(op.a), end: arr(b), bulge: r4(bulge), normal: arr(op.n) });
    // a pie closes back through the center into a face
    if (op.pie) await run({ cmd: "line", points: [arr(b), arr(op.c), arr(op.a)] });
  }

  async function commitArc3(end: THREE.Vector3) {
    const op = st.current.op;
    if (op?.t !== "arc3" || !op.m) return;
    const n = circleThrough(op.a, op.m, end)?.n;
    if (!n) return;
    if (n.dot(op.n) < 0) n.negate();
    const bulge = bulgeThrough(op.a, op.m, end, n);
    if (bulge == null) return;
    st.current.op = null;
    clearPreview();
    await run({ cmd: "arc", start: arr(op.a), end: arr(end), bulge: r4(bulge), normal: arr(n) });
  }

  async function commitRrect() {
    const op = st.current.op;
    if (op?.t !== "rrect" || !op.b || op.h.length() < 1e-4) return;
    st.current.op = null;
    clearPreview();
    await run({ cmd: "polygon", points: [op.a, op.b, op.b.clone().add(op.h), op.a.clone().add(op.h)].map(arr) });
  }

  async function commitFree() {
    const op = st.current.op;
    if (op?.t !== "free") return;
    st.current.op = null;
    clearPreview();
    const pts = op.pts;
    if (pts.length < 2) return;
    // ending near the start closes it (and makes a face)
    const s0 = toScreen2(pts[0]);
    const s1 = toScreen2(pts[pts.length - 1]);
    const closed = pts.length > 3 && Math.hypot(s0.x - s1.x, s0.y - s1.y) < 14;
    // closed: one face straight away (the line tool's loop search stops at 14 edges)
    if (closed) await run({ cmd: "polygon", points: pts.slice(0, -1).map(arr) });
    else await run({ cmd: "line", points: pts.map(arr) });
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
    if (live.current.tool === "protractor") {
      // an angled guide line through the center
      const dir = op.ref.clone().sub(op.center).normalize().applyAxisAngle(op.n, THREE.MathUtils.degToRad(op.angle));
      await run({ cmd: "guide", point: arr(op.center), direction: arr(dir) });
      return;
    }
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
    if (p.distanceTo(op.p0) > 1e-4) st.current.last = { t: "tape", d: p.distanceTo(op.p0) };
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

    const moveBox = ({ e, E, op }: MoveArgs<"box">) => {
      op.x1 = e.clientX;
      op.y1 = e.clientY;
      setBox({ x0: op.x0 - E.rect.left, y0: op.y0 - E.rect.top, x1: op.x1 - E.rect.left, y1: op.y1 - E.rect.top });
      return;
    };
    const moveLine = ({ e, L, E, op, lock }: MoveArgs<"line" | "tape">) => {
      const sn = infer(E, e, { anchor: op.p0, lock });
      op.cur = sn.p;
      T.preview.add(polyline([op.p0, sn.p], axisColor(sn), false, op.t === "tape"));
      showSnap(sn);
      setLive([{ p: op.p0.clone().add(sn.p).multiplyScalar(0.5), text: fmt(sn.p.distanceTo(op.p0)) }]);
      L.onMeasure("Length", fmt(sn.p.distanceTo(op.p0)));
      return;
    };
    const moveDim = ({ e, L, E, op, lock }: MoveArgs<"dim">) => {
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
    };
    const moveWall = ({ e, L, E, op, lock }: MoveArgs<"wall">) => {
      const last = op.pts[op.pts.length - 1];
      const sn = infer(E, e, { anchor: last, lock, plane: { origin: op.pts[0], normal: UP } });
      op.cur = sn.p;
      T.preview.add(polyline([...op.pts, sn.p], ACCENT));
      if (op.pts.length > 2 && sn.p.distanceTo(op.pts[0]) < 1e-6) setHint({ ...toScreen2(sn.p), label: "Close room", kind: "endpoint" });
      else showSnap(sn);
      setLive([{ p: last.clone().add(sn.p).multiplyScalar(0.5), text: fmt(sn.p.distanceTo(last)) }]);
      L.onMeasure("Length", fmt(sn.p.distanceTo(last)));
      return;
    };
    const moveStairs = ({ e, L, E, op, lock }: MoveArgs<"stairs">) => {
      const sn = infer(E, e, { anchor: op.at, lock, plane: { origin: op.at, normal: UP } });
      op.cur = sn.p;
      T.preview.add(polyline([op.at, sn.p], ACCENT));
      showSnap(sn);
      L.onMeasure("Direction", "click which way it climbs");
      return;
    };
    const moveText = ({ e, E, op }: MoveArgs<"text">) => {
      const sn = infer(E, e, {});
      op.cur = sn.p;
      T.preview.add(polyline([op.leader, sn.p], 0x33384d));
      setHint(null);
      return;
    };
    const moveCarc = ({ e, L, E, op }: MoveArgs<"carc">) => {
      if (!op.a) {
        const sn = infer(E, e, { anchor: op.c, plane: { origin: op.c, normal: op.n } });
        const r = sn.p.distanceTo(op.c);
        const [u, v] = basis(op.n);
        T.preview.add(polyline(Array.from({ length: 48 }, (_, i) => op.c.clone().addScaledVector(u, Math.cos((i / 48) * Math.PI * 2) * r).addScaledVector(v, Math.sin((i / 48) * Math.PI * 2) * r)), 0x8a8ea0, true));
        T.preview.add(polyline([op.c, sn.p], axisColor(sn)));
        showSnap(sn);
        L.onMeasure("Radius", fmt(r));
        return;
      }
      const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.c), new THREE.Vector3());
      if (!q) return;
      const r = op.a.distanceTo(op.c);
      const u = op.a.clone().sub(op.c).normalize();
      const v = op.n.clone().cross(u);
      const d = q.clone().sub(op.c);
      const phi = Math.atan2(d.dot(v), d.dot(u));
      // follow the swing past half a turn
      let step = phi - op.last;
      if (step > Math.PI) step -= Math.PI * 2;
      if (step < -Math.PI) step += Math.PI * 2;
      op.ang = Math.max(-Math.PI * 2 + 1e-3, Math.min(Math.PI * 2 - 1e-3, op.ang + step));
      op.last = phi;
      const k = Math.max(4, Math.ceil((Math.abs(op.ang) / (Math.PI * 2)) * 48));
      const pts = Array.from({ length: k + 1 }, (_, i) => op.c.clone().addScaledVector(u, Math.cos((op.ang * i) / k) * r).addScaledVector(v, Math.sin((op.ang * i) / k) * r));
      T.preview.add(polyline(op.pie ? [op.c, ...pts, op.c] : pts, ACCENT));
      if (!op.pie) T.preview.add(polyline([op.c, op.a], 0x8a8ea0));
      L.onMeasure("Angle", `${Math.abs(THREE.MathUtils.radToDeg(op.ang)).toFixed(1)}°`);
      return;
    };
    const moveArc3 = ({ e, L, E, op, lock }: MoveArgs<"arc3">) => {
      const sn = infer(E, e, { anchor: op.m ?? op.a, lock });
      op.cur = sn.p;
      showSnap(sn);
      if (!op.m) {
        T.preview.add(polyline([op.a, sn.p], axisColor(sn)));
        L.onMeasure("Length", fmt(sn.p.distanceTo(op.a)));
        return;
      }
      const k = circleThrough(op.a, op.m, sn.p);
      if (!k) {
        T.preview.add(polyline([op.a, op.m, sn.p], 0x8a8ea0));
        return;
      }
      const n = k.n.dot(op.n) < 0 ? k.n.clone().negate() : k.n;
      const b = bulgeThrough(op.a, op.m, sn.p, n) ?? 0;
      T.preview.add(polyline(arcPoints(op.a, sn.p, b, n, 32), ACCENT));
      L.onMeasure("Radius", fmt(k.r));
      return;
    };
    const moveRrect = ({ e, L, E, op, lock }: MoveArgs<"rrect">) => {
      if (!op.b) {
        const sn = infer(E, e, { anchor: op.a, lock });
        op.cur = sn.p;
        T.preview.add(polyline([op.a, sn.p], axisColor(sn)));
        showSnap(sn);
        L.onMeasure("Length", fmt(sn.p.distanceTo(op.a)));
        return;
      }
      const sn = infer(E, e, { anchor: op.b, lock });
      const along = op.b.clone().sub(op.a).normalize();
      const d = sn.p.clone().sub(op.b);
      op.h = d.sub(along.clone().multiplyScalar(d.dot(along)));
      const pts = [op.a, op.b, op.b.clone().add(op.h), op.a.clone().add(op.h)];
      T.preview.add(polyline(pts, ACCENT, true));
      showSnap(sn);
      setLive([
        { p: op.a.clone().add(op.b).multiplyScalar(0.5), text: fmt(op.a.distanceTo(op.b)) },
        { p: op.b.clone().add(pts[2]).multiplyScalar(0.5), text: fmt(op.h.length()) },
      ]);
      L.onMeasure("Height", fmt(op.h.length()));
      return;
    };
    const moveAxes = ({ e, L, E, op, lock }: MoveArgs<"axes">) => {
      const sn = infer(E, e, { anchor: op.x ?? op.o, lock });
      op.cur = sn.p;
      showSnap(sn);
      if (!op.x) {
        T.preview.add(polyline([op.o, sn.p], AXIS_COLORS[0]));
        L.onMeasure("Red axis", "click its direction");
        return;
      }
      const x = op.x.clone().sub(op.o).normalize();
      const d = sn.p.clone().sub(op.o);
      const y = d.sub(x.clone().multiplyScalar(d.dot(x)));
      const len = Math.max(1, op.x.distanceTo(op.o));
      T.preview.add(polyline([op.o, op.o.clone().addScaledVector(x, len)], AXIS_COLORS[0]));
      if (y.length() > 1e-6) T.preview.add(polyline([op.o, op.o.clone().addScaledVector(y.normalize(), len)], AXIS_COLORS[1]));
      return;
    };
    const moveFree = ({ e, op }: MoveArgs<"free">) => {
      const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.o), new THREE.Vector3());
      if (!q) return;
      const last = op.pts[op.pts.length - 1];
      const a = toScreen2(last);
      const b = toScreen2(q);
      if (Math.hypot(a.x - b.x, a.y - b.y) >= 5) op.pts.push(q);
      T.preview.add(polyline([...op.pts, q], ACCENT));
      return;
    };
    const moveRect = ({ e, L, E, op, lock }: MoveArgs<"rect">) => {
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
    };
    const moveCircle = ({ e, L, E, op }: MoveArgs<"circle">) => {
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
    };
    const moveArc = ({ e, L, E, op, lock }: MoveArgs<"arc">) => {
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
    };
    const movePush = ({ e, L, E, op }: MoveArgs<"push">) => {
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
    };
    const moveOffset = ({ e, L, op }: MoveArgs<"offset">) => {
      const n = V(op.face.normal);
      const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(n, op.start), new THREE.Vector3());
      if (!q) return;
      const outer = op.face.outer.map(V);
      const c = outer.reduce((a, p) => a.add(p), new THREE.Vector3()).divideScalar(outer.length);
      op.d = q.clone().sub(op.start).dot(c.clone().sub(op.start).normalize());
      T.preview.add(polyline(offsetOutline(outer, n, op.d), ACCENT, true));
      L.onMeasure("Distance", fmt(op.d));
      return;
    };
    const moveMove = ({ e, L, E, op, lock }: MoveArgs<"move">) => {
      const sn = infer(E, e, { anchor: op.base, lock });
      op.delta = sn.p.clone().sub(op.base);
      T.preview.add(polyline([op.base, sn.p], axisColor(sn)));
      if (op.vertex) {
        // the point's edges, stretched to where it's going
        for (const q of op.vertex.far) T.preview.add(polyline([q, sn.p], ACCENT));
      } else ghost(new THREE.Matrix4().makeTranslation(op.delta.x, op.delta.y, op.delta.z));
      showSnap(sn);
      setLive([{ p: op.base.clone().add(sn.p).multiplyScalar(0.5), text: fmt(op.delta.length()) }]);
      L.onMeasure(op.copy ? "Copy" : "Distance", fmt(op.delta.length()));
      return;
    };
    const moveRotate = ({ e, L, op }: MoveArgs<"rotate">) => {
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
      if (op.ids.length) ghost(new THREE.Matrix4().makeTranslation(c.x, c.y, c.z).multiply(new THREE.Matrix4().makeRotationAxis(op.n, THREE.MathUtils.degToRad(ang))).multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z)));
      // the protractor: the guide it will lay, long and dashed
      else {
        const d = q.clone().sub(c).normalize().multiplyScalar(200);
        T.preview.add(polyline([c.clone().sub(d), c.clone().add(d)], 0x8a8ea0));
      }
      L.onMeasure("Angle", `${ang.toFixed(1)}°`);
      return;
    };
    const moveScale = ({ e, L, op }: MoveArgs<"scale">) => {
      const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(UP, op.center), new THREE.Vector3());
      if (!q) return;
      op.factor = Math.round((q.distanceTo(op.center) / Math.max(op.start.distanceTo(op.center), 1e-6)) * 100) / 100;
      const c = op.center;
      const f = op.factor;
      ghost(new THREE.Matrix4().makeTranslation(c.x, c.y, c.z).multiply(new THREE.Matrix4().makeScale(f, f, f)).multiply(new THREE.Matrix4().makeTranslation(-c.x, -c.y, -c.z)));
      L.onMeasure("Scale", `${f.toFixed(2)}×`);
      return;
    };

    const move = (e: PointerEvent) => {
      lastPointer.current = { x: e.clientX, y: e.clientY };
      // camera tools
      {
        const c = camTool.current;
        const tool = live.current.tool;
        if (c.drag && tool === "lookaround") {
          look((e.clientX - c.drag.x) * 0.004, (e.clientY - c.drag.y) * 0.004);
          c.drag = { ...c.drag, x: e.clientX, y: e.clientY };
          return;
        }
        if (c.drag && tool === "zoomwindow" && c.box) {
          Object.assign(c.box.style, { left: Math.min(c.drag.x, e.clientX) + "px", top: Math.min(c.drag.y, e.clientY) + "px", width: Math.abs(e.clientX - c.drag.x) + "px", height: Math.abs(e.clientY - c.drag.y) + "px" });
          return;
        }
        if (c.drag && tool === "walk") return;
      }
      if (cageEd.current && !st.current.space && !(e.buttons & 6)) {
        cageEd.current.pointerMove(e);
        return;
      }
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

      // a plugin's tool: snap, draw what it asked for, and tell it where we are
      if (L.tool === "plugin" && L.pluginTool) {
        const P = L.pluginTool;
        const sn = infer(E, e, { anchor: P.anchor ? V(P.anchor) : null, lock });
        drawPluginPreview(P.preview);
        if (P.anchor) T0().preview.add(polyline([V(P.anchor), sn.p], axisColor(sn)));
        showSnap(sn);
        if (P.anchor) L.onMeasure("Length", fmt(sn.p.distanceTo(V(P.anchor))));
        P.move(pluginPoint(sn, e));
        return;
      }

      switch (op?.t) {
        case "box":
          return moveBox({ e, L: L as Live, E, op, lock });
        case "line":
        case "tape":
          return moveLine({ e, L: L as Live, E, op, lock });
        case "dim":
          return moveDim({ e, L: L as Live, E, op, lock });
        case "wall":
          return moveWall({ e, L: L as Live, E, op, lock });
        case "stairs":
          return moveStairs({ e, L: L as Live, E, op, lock });
        case "text":
          return moveText({ e, L: L as Live, E, op, lock });
        case "carc":
          return moveCarc({ e, L: L as Live, E, op, lock });
        case "arc3":
          return moveArc3({ e, L: L as Live, E, op, lock });
        case "rrect":
          return moveRrect({ e, L: L as Live, E, op, lock });
        case "axes":
          return moveAxes({ e, L: L as Live, E, op, lock });
        case "free":
          return moveFree({ e, L: L as Live, E, op, lock });
        case "rect":
          return moveRect({ e, L: L as Live, E, op, lock });
        case "circle":
          return moveCircle({ e, L: L as Live, E, op, lock });
        case "arc":
          return moveArc({ e, L: L as Live, E, op, lock });
        case "push":
          return movePush({ e, L: L as Live, E, op, lock });
        case "offset":
          return moveOffset({ e, L: L as Live, E, op, lock });
        case "move":
          return moveMove({ e, L: L as Live, E, op, lock });
        case "rotate":
          return moveRotate({ e, L: L as Live, E, op, lock });
        case "scale":
          return moveScale({ e, L: L as Live, E, op, lock });
      }

      // nothing in progress: show what a click would hit
      if (["axes", "line", "freehand", "arc", "arccenter", "arc3", "pie", "rect", "rrect", "circle", "polygon", "tape", "dimension", "text", "move", "rotate", "scale", "place", "wall", "stairs", "column", "protractor"].includes(L.tool)) showSnap(infer(E, e, {}));
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

    const downSelect = async ({ e, L, sel }: DownArgs) => {
      const he = pickEdge(e);
      const h = he == null ? pick(e) : null;
      if (clicks.current >= 2) {
        // double-click: open a group or component; outside closes it
        if (h) {
          const ent = entityOf(h);
          if (ent.kind === "groups" && L.scene.cages?.[String(ent.id)]) {
            enterCage(ent.id);
            L.onToast("SubD mode: 1 points, 2 edges, 3 faces; drag to shape. Enter when done.");
            return;
          }
          if (ent.kind === "groups" || ent.kind === "instances") {
            await run({ cmd: "edit", id: ent.id });
            L.setSel(EMPTY);
            L.onToast(`Editing ${ent.kind === "groups" ? "group" : "component"}. Esc or double-click outside to close.`);
            return;
          }
          // loose geometry: double-click takes the face and its edges,
          // triple-click everything connected (SketchUp's select)
          if (ent.kind === "faces") {
            const r = await exec({ cmd: "connected", id: ent.id, all: clicks.current >= 3 });
            if (r.ok) L.setSel(combine(sel, { ...EMPTY, faces: r.faces as number[], edges: r.edges as number[] }, selMode(e)));
            return;
          }
        } else if (he != null) {
          const r = await exec({ cmd: "connected", id: he, all: clicks.current >= 3 });
          if (r.ok) L.setSel(combine(sel, { ...EMPTY, faces: r.faces as number[], edges: r.edges as number[] }, selMode(e)));
          return;
        } else {
          await L.run({ cmd: "close" });
          return;
        }
      }
      if (he != null) return L.setSel(combine(sel, { ...EMPTY, edges: curveOf(he) }, selMode(e)));
      if (h) {
        const ent = entityOf(h);
        return L.setSel(toggle(sel, ent.kind, ent.id, selMode(e)));
      }
      st.current.op = { t: "box", x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY, mode: selMode(e) };
      el.setPointerCapture(e.pointerId);
      return;
    };
    const downLine = async ({ e, E, op }: DownArgs) => {
      const sn = infer(E, e, op?.t === "line" ? { anchor: op.p0, lock: st.current.lock } : {});
      if (op?.t === "line") {
        if (clicks.current >= 2) return cancel();
        return void (await commitLine(sn.p));
      }
      st.current.op = { t: "line", p0: sn.p, start: sn.p, cur: sn.p };
      return;
    };
    const downRect = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "rect") return void (await commitRect());
      const sn = infer(E, e, {});
      const n = sn.face ? V(sn.face.normal) : UP.clone();
      const [u, v] = basis(n);
      st.current.op = { t: "rect", p0: sn.p, n, u, v, a: 0, b: 0 };
      return;
    };
    const downArccenter = async ({ e, L, E, op }: DownArgs) => {
      if (op?.t === "carc") {
        if (!op.a) {
          const sn = infer(E, e, { anchor: op.c, plane: { origin: op.c, normal: op.n } });
          if (sn.p.distanceTo(op.c) > 1e-4) {
            op.a = sn.p;
            op.ang = 0;
            op.last = 0;
          }
          return;
        }
        return void (await commitCarc());
      }
      const sn = infer(E, e, {});
      st.current.op = { t: "carc", c: sn.p, n: sn.face ? V(sn.face.normal) : UP.clone(), a: null, ang: 0, last: 0, pie: L.tool === "pie" };
      return;
    };
    const downArc3 = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "arc3") {
        const sn = infer(E, e, { anchor: op.m ?? op.a, lock: st.current.lock });
        if (!op.m) {
          if (sn.p.distanceTo(op.a) > 1e-4) op.m = sn.p;
          return;
        }
        return void (await commitArc3(sn.p));
      }
      const sn = infer(E, e, {});
      st.current.op = { t: "arc3", a: sn.p, m: null, n: sn.face ? V(sn.face.normal) : UP.clone(), cur: sn.p };
      return;
    };
    const downRrect = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "rrect") {
        if (!op.b) {
          if (op.cur.distanceTo(op.a) > 1e-4) op.b = op.cur.clone();
          return;
        }
        return void (await commitRrect());
      }
      const sn = infer(E, e, {});
      st.current.op = { t: "rrect", a: sn.p, b: null, h: new THREE.Vector3(), cur: sn.p };
      return;
    };
    const downAxes = async ({ e, L, E, op }: DownArgs) => {
      const sn = infer(E, e, op?.t === "axes" ? { anchor: op.x ?? op.o, lock: st.current.lock } : {});
      if (op?.t !== "axes") {
        st.current.op = { t: "axes", o: sn.p, x: null, cur: sn.p };
        return;
      }
      if (!op.x) {
        if (sn.p.distanceTo(op.o) > 1e-4) op.x = sn.p;
        return;
      }
      const x = op.x.clone().sub(op.o).normalize();
      const y = sn.p.clone().sub(op.o);
      if (y.clone().sub(x.clone().multiplyScalar(y.dot(x))).length() < 1e-4) return;
      st.current.op = null;
      clearPreview();
      await run({ cmd: "set_axes", origin: arr(op.o), x: arr(x), y: arr(y) });
      L.onToast("Axes moved: drawing lines up with them now. Shift+S › Reset axes puts them back.");
      L.setTool("select");
      return;
    };
    const downFreehand = async ({ e, E }: DownArgs) => {
      const sn = infer(E, e, {});
      st.current.op = { t: "free", pts: [sn.p], n: sn.face ? V(sn.face.normal) : UP.clone(), o: sn.p };
      el.setPointerCapture(e.pointerId);
      return;
    };
    const downCircle = async ({ e, L, E, op }: DownArgs) => {
      if (op?.t === "circle") return void (await commitCircle());
      const sn = infer(E, e, {});
      st.current.op = { t: "circle", c: sn.p, n: sn.face ? V(sn.face.normal) : UP.clone(), r: 0, seg: L.tool === "polygon" ? st.current.segs.polygon : st.current.segs.circle };
      return;
    };
    const downArc = async ({ e, E, op }: DownArgs) => {
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
    };
    const downPush = async ({ e, op }: DownArgs) => {
      const again = st.current.last?.t === "push" ? st.current.last : null;
      // double-click: same distance as last time. The first press of it has already started a push (click-move-click),
      // so that one, still untouched, is dropped for the repeat.
      if (clicks.current >= 2 && again && (!op || (op.t === "push" && !op.dragged))) {
        const h = pick(e);
        if (h?.face) {
          cancel();
          await run({ cmd: "pushpull", face: h.face.id, distance: again.d });
          return;
        }
      }
      if (op?.t === "push") return void (await commitPush()); // click-move-click
      const h = pick(e);
      if (!h?.face) return;
      st.current.op = { t: "push", face: h.face, origin: h.point.clone(), n: V(h.face.normal), d: 0, dragged: false };
      el.setPointerCapture(e.pointerId);
      return;
    };
    const downOffset = async ({ e, op }: DownArgs) => {
      if (op?.t === "offset") return void (await commitOffset());
      const h = pick(e);
      if (h?.face) st.current.op = { t: "offset", face: h.face, start: h.point.clone(), d: 0 };
      return;
    };
    const downFollowme = async ({ e, L, sel }: DownArgs) => {
      const h = pick(e);
      if (!h?.face) return;
      if (sel.edges.length) await run({ cmd: "follow_me", face: h.face.id, edges: sel.edges });
      else if (sel.faces.length === 1 && sel.faces[0] !== h.face.id) await run({ cmd: "follow_me", face: h.face.id, around: sel.faces[0] });
      else return L.onToast("Select the path first (edges, or a face to go around), then click the profile.", true);
      L.setSel(EMPTY);
      return;
    };
    const downProtractor = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "rotate") {
        if (op.ref) return void (await commitRotate());
        const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.center), new THREE.Vector3());
        if (q && q.distanceTo(op.center) > 1e-4) op.ref = q;
        return;
      }
      const sn = infer(E, e, {});
      st.current.op = { t: "rotate", center: sn.p, n: sn.face ? V(sn.face.normal) : UP.clone(), ref: null, angle: 0, ids: [] };
      return;
    };
    const downMove = async ({ e, L, E, op, sel }: DownArgs) => {
      if (op?.t === "move") return void (await commitMove());
      if (op?.t === "scale") return void (await commitScale());
      if (op?.t === "rotate") {
        if (op.ref) return void (await commitRotate());
        const q = ray(e).intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(op.n, op.center), new THREE.Vector3());
        if (q && q.distanceTo(op.center) > 1e-4) op.ref = q;
        return;
      }
      // nothing selected, Move on a corner: grab just that point (the faces around it follow)
      if (L.tool === "move" && selIds(sel).length === 0 && !e.altKey && !e.ctrlKey) {
        const v = vertexUnder(e);
        if (v) {
          st.current.op = { t: "move", base: v.p, ids: [v.id], delta: new THREE.Vector3(), copy: false, vertex: { far: v.far } };
          return;
        }
      }
      // nothing selected: grab what's under the cursor
      let s2 = sel;
      if (selIds(sel).length === 0) {
        const he = pickEdge(e);
        const h = he == null ? pick(e) : null;
        if (he != null) s2 = { ...EMPTY, edges: curveOf(he) };
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
    };
    const downTape = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "tape") return void (await commitTape(infer(E, e, { anchor: op.p0, lock: st.current.lock }).p));
      const sn = infer(E, e, {});
      st.current.op = { t: "tape", p0: sn.p, along: sn.kind === "on-edge" && sn.along ? sn.along : null, cur: sn.p };
      return;
    };
    const downDimension = async ({ e, E, op }: DownArgs) => {
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
    };
    const downText = async ({ e, E, op }: DownArgs) => {
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
    };
    const downWall = async ({ e, E, op }: DownArgs) => {
      if (op?.t === "wall") {
        const last = op.pts[op.pts.length - 1];
        const sn = infer(E, e, { anchor: last, lock: st.current.lock, plane: { origin: op.pts[0], normal: UP } });
        if (op.pts.length > 2 && sn.p.distanceTo(op.pts[0]) < 1e-6) return void (await commitWall(true));
        if (clicks.current >= 2) return void (await commitWall(false));
        if (sn.p.distanceTo(last) > 1e-4) op.pts.push(sn.p);
        st.current.lock = null;
        return;
      }
      const sn = infer(E, e, {});
      st.current.op = { t: "wall", pts: [sn.p], cur: sn.p };
      return;
    };
    const downStairs = async ({ e, L, E, op }: DownArgs) => {
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
    };
    const downRoof = async ({ e, L }: DownArgs) => {
      const h = pick(e);
      if (!h?.face) return L.onToast("Click a flat top face (a ceiling or the top of walls).", true);
      const b = L.build;
      await run({ cmd: "roof", face: h.face.id, kind: b.roofKind, pitch: b.roofPitch, overhang: b.roofOverhang });
      return;
    };
    const downSmoove = async ({ e, L, sel }: DownArgs) => {
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
    };
    const downFlipedge = async ({ e, L }: DownArgs) => {
      const he = pickEdge(e);
      if (he == null) return L.onToast("Click an edge between two triangles.", true);
      await run({ cmd: "flip_edge", edge: he });
      return;
    };
    const downColumn = async ({ e, L, E }: DownArgs) => {
      const sn = infer(E, e, {});
      const b = L.build;
      await run({ cmd: "column", at: arr(sn.p), size: b.columnSize, height: b.columnHeight, round: b.columnRound });
      return;
    };
    const downPaint = async ({ e, L }: DownArgs) => {
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
    };
    const downErase = async ({ e }: DownArgs) => {
      const he = pickEdge(e);
      const soften = e.altKey || e.ctrlKey;
      if (he != null) {
        if (soften) return void (await run({ cmd: "soften", ids: curveOf(he), soft: !e.shiftKey }));
        return void (await run({ cmd: e.shiftKey ? "hide" : "delete", ids: curveOf(he) }));
      }
      const h = pick(e);
      if (h && !soften) await run({ cmd: e.shiftKey ? "hide" : "delete", ids: [entityOf(h).id] });
      return;
    };
    const downSection = async ({ e, L }: DownArgs) => {
      const h = pick(e);
      if (!h?.face) return;
      const n = V(h.face.normal).negate();
      // 1 cm into the solid behind the clicked face, keeping the far side; a named plane
      const o = h.point.clone().addScaledVector(n, 0.01);
      await L.run({ cmd: "section_plane", origin: arr(o), normal: arr(n) });
      L.setTool("select");
      return;
    };
    const downPlugin = async ({ e, L, E }: DownArgs) => {
      if (!L.pluginTool) return;
      const P = L.pluginTool;
      const sn = infer(E, e, { anchor: P.anchor ? V(P.anchor) : null, lock: st.current.lock });
      P.click(pluginPoint(sn, e));
      return;
    };
    const downPlace = async ({ e, L, E }: DownArgs) => {
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
      // dropped on a face: doors, windows (and components set to) cut their opening there
      const host = wall && sn.face ? sn.face.id : undefined;
      let r: Reply;
      if (k.startsWith("file:")) r = await run({ cmd: "insert_file", path: k.slice(5), at: arr(at), rotate: r4(rot) });
      else if (k.startsWith("component:")) r = await run({ cmd: "place", component: Number(k.slice(10)), at: arr(at), rotate: r4(rot), host });
      else r = await run({ cmd: "insert", asset: k, at: arr(at), rotate: r4(rot), host });
      // refused (for one, a component is open for editing): leave Place mode, as the message says what to do
      if (!r.ok) {
        L.setTool("select");
        return;
      }
      const op = r.opening as { error?: string } | null | undefined;
      if (op?.error) L.onToast(`Placed, but no opening: ${op.error}`, true);
      return;
    };

    const down = async (e: PointerEvent) => {
      const L = live.current;
      // PointerEvent.detail is 0 in Chromium (and WebView2), so count clicks here: the same place, quickly
      const now = e.timeStamp;
      const prev = lastDown.current;
      clicks.current = prev && e.button === prev.button && now - prev.t < 450 && Math.hypot(e.clientX - prev.x, e.clientY - prev.y) < 6 ? clicks.current + 1 : 1;
      lastDown.current = { t: now, x: e.clientX, y: e.clientY, button: e.button };
      if (e.button === 0 && !st.current.space && CAMERA_TOOLS.includes(L.tool)) {
        const c = camTool.current;
        el.setPointerCapture(e.pointerId);
        rememberView();
        if (L.tool === "position") {
          // stand where you click, at eye height, facing the way you were looking
          const h = pick(e);
          const T = t.current!;
          let p: THREE.Vector3 | null = h?.point ?? null;
          if (!p) p = ray(e).intersectPlane(new THREE.Plane(new THREE.Vector3(0, 0, 1), -T.groundZ), new THREE.Vector3());
          if (!p) return;
          const dir = T.controls.target.clone().sub(T.camera.position);
          dir.z = 0;
          if (dir.lengthSq() < 1e-9) dir.set(0, 1, 0);
          dir.normalize();
          const eye = p.clone().add(new THREE.Vector3(0, 0, c.eyeH));
          goTo({ eye: arr(eye), target: arr(eye.clone().addScaledVector(dir, 2)), fov: T.persp.fov });
          L.setTool("lookaround");
          L.onToast(`Standing at ${formatLength(c.eyeH, units)} eye height: drag to look around, or pick Walk`);
          return;
        }
        c.drag = { x: e.clientX, y: e.clientY, at: performance.now() };
        if (L.tool === "walk") walkLoop();
        if (L.tool === "zoomwindow") {
          const b = document.createElement("div");
          b.className = "select-box";
          Object.assign(b.style, { position: "fixed", left: e.clientX + "px", top: e.clientY + "px", width: "0px", height: "0px", pointerEvents: "none", zIndex: "40" });
          document.body.appendChild(b);
          c.box = b;
        }
        return;
      }
      if (cageEd.current && !st.current.space && e.button === 0) {
        el.setPointerCapture(e.pointerId);
        cageEd.current.pointerDown(e);
        return;
      }
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
        case "select":
          return downSelect({ e, L: L as Live, E, op, sel });
        case "line":
          return downLine({ e, L: L as Live, E, op, sel });
        case "rect":
          return downRect({ e, L: L as Live, E, op, sel });
        case "arccenter":
        case "pie":
          return downArccenter({ e, L: L as Live, E, op, sel });
        case "arc3":
          return downArc3({ e, L: L as Live, E, op, sel });
        case "rrect":
          return downRrect({ e, L: L as Live, E, op, sel });
        case "axes":
          return downAxes({ e, L: L as Live, E, op, sel });
        case "freehand":
          return downFreehand({ e, L: L as Live, E, op, sel });
        case "circle":
        case "polygon":
          return downCircle({ e, L: L as Live, E, op, sel });
        case "arc":
          return downArc({ e, L: L as Live, E, op, sel });
        case "push":
          return downPush({ e, L: L as Live, E, op, sel });
        case "offset":
          return downOffset({ e, L: L as Live, E, op, sel });
        case "followme":
          return downFollowme({ e, L: L as Live, E, op, sel });
        case "protractor":
          return downProtractor({ e, L: L as Live, E, op, sel });
        case "move":
        case "rotate":
        case "scale":
          return downMove({ e, L: L as Live, E, op, sel });
        case "tape":
          return downTape({ e, L: L as Live, E, op, sel });
        case "dimension":
          return downDimension({ e, L: L as Live, E, op, sel });
        case "text":
          return downText({ e, L: L as Live, E, op, sel });
        case "wall":
          return downWall({ e, L: L as Live, E, op, sel });
        case "stairs":
          return downStairs({ e, L: L as Live, E, op, sel });
        case "roof":
          return downRoof({ e, L: L as Live, E, op, sel });
        case "smoove":
        case "stamp":
        case "drape":
          return downSmoove({ e, L: L as Live, E, op, sel });
        case "flipedge":
          return downFlipedge({ e, L: L as Live, E, op, sel });
        case "column":
          return downColumn({ e, L: L as Live, E, op, sel });
        case "paint":
          return downPaint({ e, L: L as Live, E, op, sel });
        case "erase":
          return downErase({ e, L: L as Live, E, op, sel });
        case "section":
          return downSection({ e, L: L as Live, E, op, sel });
        case "plugin":
          return downPlugin({ e, L: L as Live, E, op, sel });
        case "place":
          return downPlace({ e, L: L as Live, E, op, sel });
      }
    };

    const up = async (e: PointerEvent) => {
      const L = live.current;
      {
        const c = camTool.current;
        if (c.drag && e.button === 0) {
          const d = c.drag;
          c.drag = null;
          if (L.tool === "zoomwindow" && c.box) {
            c.box.remove();
            c.box = null;
            const r = rect();
            const [x0, x1] = [Math.min(d.x, e.clientX), Math.max(d.x, e.clientX)];
            const [y0, y1] = [Math.min(d.y, e.clientY), Math.max(d.y, e.clientY)];
            if (x1 - x0 > 6 && y1 - y0 > 6) {
              // the box's middle comes to the middle, and the box fills the view
              const T = t.current!;
              const mid = { clientX: (x0 + x1) / 2, clientY: (y0 + y1) / 2 };
              const h = pick(mid);
              const dist = T.camera.position.distanceTo(T.controls.target);
              const rr = ray(mid);
              const target = h?.point ?? rr.at(dist, new THREE.Vector3());
              const frac = Math.max((x1 - x0) / r.width, (y1 - y0) / r.height) * 1.15; // a little margin
              const eyeDist = target.distanceTo(T.camera.position) * frac;
              const eye = target.clone().addScaledVector(rr.direction, -eyeDist);
              goTo({ eye: arr(eye), target: arr(target), fov: T.persp.fov });
              window.setTimeout(rememberView, 500);
            }
          }
          rememberView();
          return;
        }
      }
      if (cageEd.current && e.button === 0 && (await cageEd.current.pointerUp(e))) return;
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
      if (op?.t === "free") {
        if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
        return void (await commitFree());
      }
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
      if (!r.width || !r.height) return EMPTY;
      const x = (value: number) => 2 * (value - r.left) / r.width - 1;
      const y = (value: number) => 1 - 2 * (value - r.top) / r.height;
      const viewProjection = new THREE.Matrix4().multiplyMatrices(cam.projectionMatrix, cam.matrixWorldInverse);
      return selectMarquee(s, viewProjection, {
        left: x(Math.min(op.x0, op.x1)), right: x(Math.max(op.x0, op.x1)),
        bottom: y(Math.max(op.y0, op.y1)), top: y(Math.min(op.y0, op.y1)),
      }, op.x1 < op.x0);
    }

    const key = (e: KeyboardEvent) => {
      const typing = isTypingTarget(e.target);
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
      if (typing) return;
      // walking: arrow keys / WASD held (Shift runs, Option walks through walls)
      if (live.current.tool === "walk" && ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "w", "a", "s", "d", "W", "A", "S", "D", "Shift", "Alt"].includes(e.key)) {
        const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
        const c = camTool.current;
        if (e.type === "keydown") {
          if (!e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            e.stopImmediatePropagation();
            c.keys.add(k);
            walkLoop();
          }
        } else c.keys.delete(k);
        if (!["Shift", "Alt"].includes(k)) return;
      }
      if (e.type !== "keydown") return;
      if ((e.metaKey || e.ctrlKey) && (e.key === "[" || e.key === "]")) {
        e.preventDefault();
        stepView(e.key === "[" ? -1 : 1);
        return;
      }
      const ed = cageEd.current;
      if (ed) {
        if (e.key === "Escape") {
          if (ed.tool !== "select") ed.setTool("select");
          else if (ed.sel.size) (ed.sel.clear(), ed.draw(), ed.ui());
          else exitCage();
          return;
        }
        if (e.key === "Enter") return exitCage();
        if (!e.metaKey && !e.ctrlKey && ed.key(e)) return;
      }
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
      <AxisGizmo
        camera={gizmoCamera}
        onOrbit={orbitBy}
        onLook={lookAlong}
        onDrag={(on) => {
          if (on && backdropOn.current) showBackdrop(null);
          rememberView();
        }}
      />
      {coach && coach.step && !cageUi && props.styles.help !== false && (
        <div className="coach" aria-live="polite">
          <div className="coach-head">
            <span className="coach-icon">{(Icons as unknown as Record<string, () => React.ReactNode>)[tool]?.()}</span>
            <b>{coach.title}</b>
            {props.onHideHelp && (
              <button className="coach-hide" onClick={props.onHideHelp} aria-label="Hide help" title="Hide help (turn it back on with Help in the view bar)">
                ×
              </button>
            )}
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
      {cageUi && cageEd.current && <CageBar ui={cageUi} ed={cageEd.current} onDone={exitCage} />}
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
