// The 3D view: draws the engine's scene with three.js and turns mouse input
// into engine commands. Z is up, units are meters, like the engine.
import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { FaceInfo, Reply, Scene, Vec3 } from "./engine";

export type Tool = "select" | "rect" | "push" | "paint" | "erase" | "orbit";

export interface ViewportApi {
  applyTyped: (text: string) => void;
  fit: () => void;
  view: (name: "iso" | "top" | "front" | "right") => void;
}

interface Props {
  scene: Scene | null;
  tool: Tool;
  selected: number[];
  color: [number, number, number];
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  onPick: (face: FaceInfo | null, opts: { additive: boolean; deep: boolean }) => void;
  onMeasure: (label: string, value: string) => void;
  api: React.MutableRefObject<ViewportApi | null>;
}

interface Snap {
  x: number;
  y: number;
  label: string;
}

const ACCENT = 0x6b5cff;
const V = (a: Vec3 | number[]) => new THREE.Vector3(a[0], a[1], a[2]);

/** In-plane axes for a face normal: u horizontal when possible, v = n x u. */
function basis(n: THREE.Vector3): [THREE.Vector3, THREE.Vector3] {
  const u = Math.abs(n.z) > 0.9 ? new THREE.Vector3(1, 0, 0) : new THREE.Vector3(0, 0, 1).cross(n).normalize();
  if (Math.abs(n.z) > 0.9) u.sub(n.clone().multiplyScalar(u.dot(n))).normalize();
  return [u, n.clone().cross(u).normalize()];
}

const round = (x: number, step: number) => Math.round(x / step) * step;

export default function Viewport({ scene, tool, selected, color, run, onPick, onMeasure, api }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [snap, setSnap] = useState<Snap | null>(null);

  // three.js objects live in a ref so React re-renders don't rebuild them
  const t = useRef<{
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    controls: OrbitControls;
    world: THREE.Scene;
    model: THREE.Group;
    overlay: THREE.Group;
    preview: THREE.Group;
    pickMesh: THREE.Mesh | null;
  } | null>(null);

  // latest props for event handlers
  const live = useRef({ scene, tool, selected, color, run, onPick, onMeasure });
  live.current = { scene, tool, selected, color, run, onPick, onMeasure };

  // interaction state
  const st = useRef<{
    rect: null | { p0: THREE.Vector3; n: THREE.Vector3; u: THREE.Vector3; v: THREE.Vector3; a: number; b: number };
    push: null | { face: FaceInfo; origin: THREE.Vector3; n: THREE.Vector3; d: number; dragged: boolean };
    lastPush: null | { face: number; d: number };
    hover: number | null;
    space: boolean;
    fitted: boolean;
  }>({ rect: null, push: null, lastPush: null, hover: null, space: false, fitted: false });

  // ---------- setup ----------
  useEffect(() => {
    const el = host.current!;
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setClearColor(0xf3f4f8);
    el.appendChild(renderer.domElement);

    const camera = new THREE.PerspectiveCamera(40, 1, 0.05, 20000);
    camera.up.set(0, 0, 1);
    camera.position.set(30, -40, 28);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.12;
    controls.screenSpacePanning = true;

    const world = new THREE.Scene();
    world.add(new THREE.HemisphereLight(0xffffff, 0xd8dbe6, 2.2));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(30, -50, 80);
    world.add(sun);

    const grid = new THREE.GridHelper(400, 400, 0xd5d8e2, 0xe4e6ee);
    grid.rotation.x = Math.PI / 2;
    (grid.material as THREE.Material).depthWrite = false;
    world.add(grid);
    const axes = new THREE.Group();
    const axis = (to: Vec3, c: number) => {
      const g = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), V(to)]);
      axes.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: c, transparent: true, opacity: 0.55 })));
    };
    axis([200, 0, 0], 0xe5484d);
    axis([0, 200, 0], 0x30a46c);
    axis([0, 0, 200], 0x3e63dd);
    world.add(axes);

    const model = new THREE.Group();
    const overlay = new THREE.Group();
    const preview = new THREE.Group();
    world.add(model, overlay, preview);

    t.current = { renderer, camera, controls, world, model, overlay, preview, pickMesh: null };

    const resize = () => {
      const { clientWidth: w, clientHeight: h } = el;
      renderer.setSize(w, h);
      camera.aspect = w / Math.max(h, 1);
      camera.updateProjectionMatrix();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();

    let raf = 0;
    const loop = () => {
      controls.update();
      renderer.render(world, camera);
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
  }, []);

  // ---------- model geometry ----------
  useEffect(() => {
    const T = t.current;
    if (!T || !scene) return;
    T.model.children.forEach((c) => {
      const m = c as THREE.Mesh;
      m.geometry?.dispose();
    });
    T.model.clear();

    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(scene.positions, 3));
    geo.setAttribute("color", new THREE.Float32BufferAttribute(scene.colors, 3));
    geo.computeVertexNormals();
    const front = new THREE.Mesh(
      geo,
      new THREE.MeshLambertMaterial({ vertexColors: true, side: THREE.FrontSide, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 })
    );
    const back = new THREE.Mesh(
      geo,
      new THREE.MeshLambertMaterial({ color: 0xa6aed0, side: THREE.BackSide, polygonOffset: true, polygonOffsetFactor: 1, polygonOffsetUnits: 1 })
    );
    const eg = new THREE.BufferGeometry();
    eg.setAttribute("position", new THREE.Float32BufferAttribute(scene.edges, 3));
    const edges = new THREE.LineSegments(eg, new THREE.LineBasicMaterial({ color: 0x23252d }));
    T.model.add(front, back, edges);
    T.pickMesh = front; // hitFace() picks it from either side

    if (!st.current.fitted && scene.positions.length) {
      st.current.fitted = true;
      fit();
    }
    drawOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene]);

  useEffect(() => {
    drawOverlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected]);

  useEffect(() => {
    const T = t.current;
    if (!T) return;
    cancelTool();
    T.controls.mouseButtons = {
      LEFT: tool === "orbit" ? THREE.MOUSE.ROTATE : (null as unknown as THREE.MOUSE),
      MIDDLE: THREE.MOUSE.ROTATE,
      RIGHT: THREE.MOUSE.PAN,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tool]);

  // ---------- helpers ----------
  const faceById = (id: number) => live.current.scene?.faces.find((f) => f.id === id) ?? null;

  function faceTris(id: number, lift: number): THREE.BufferGeometry {
    const s = live.current.scene!;
    const f = faceById(id)!;
    const n = V(f.normal).multiplyScalar(lift);
    const pts: number[] = [];
    s.triFace.forEach((fid, i) => {
      if (fid !== id) return;
      for (let k = 0; k < 3; k++) {
        const o = (i * 3 + k) * 3;
        pts.push(s.positions[o] + n.x, s.positions[o + 1] + n.y, s.positions[o + 2] + n.z);
      }
    });
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    return g;
  }

  function outline(points: THREE.Vector3[], c = ACCENT): THREE.LineLoop {
    return new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(points), new THREE.LineBasicMaterial({ color: c, depthTest: false }));
  }

  function drawOverlay() {
    const T = t.current;
    const s = live.current.scene;
    if (!T || !s) return;
    T.overlay.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.overlay.clear();
    const ids = new Set(live.current.selected);
    const hover = st.current.hover;
    if (hover !== null && !ids.has(hover) && faceById(hover)) {
      T.overlay.add(new THREE.Mesh(faceTris(hover, 0.004), new THREE.MeshBasicMaterial({ color: ACCENT, transparent: true, opacity: 0.12, side: THREE.DoubleSide, depthWrite: false })));
    }
    for (const id of ids) {
      const f = faceById(id);
      if (!f) continue;
      T.overlay.add(new THREE.Mesh(faceTris(id, 0.004), new THREE.MeshBasicMaterial({ color: ACCENT, transparent: true, opacity: 0.26, side: THREE.DoubleSide, depthWrite: false })));
      T.overlay.add(outline(f.outer.map(V)));
    }
  }

  function ndc(e: { clientX: number; clientY: number }): THREE.Vector2 {
    const r = t.current!.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
  }

  function toScreen(p: THREE.Vector3): { x: number; y: number } {
    const T = t.current!;
    const q = p.clone().project(T.camera);
    const r = T.renderer.domElement.getBoundingClientRect();
    return { x: (q.x * 0.5 + 0.5) * r.width, y: (-q.y * 0.5 + 0.5) * r.height };
  }

  function ray(e: { clientX: number; clientY: number }): THREE.Raycaster {
    const rc = new THREE.Raycaster();
    rc.setFromCamera(ndc(e), t.current!.camera);
    return rc;
  }

  function hitFace(e: { clientX: number; clientY: number }): { face: FaceInfo; point: THREE.Vector3 } | null {
    const T = t.current!;
    const s = live.current.scene;
    if (!T.pickMesh || !s) return null;
    const mat = T.pickMesh.material as THREE.Material;
    const side = mat.side;
    mat.side = THREE.DoubleSide;
    const hit = ray(e).intersectObject(T.pickMesh, false)[0];
    mat.side = side;
    if (!hit || hit.faceIndex == null) return null;
    const face = faceById(s.triFace[hit.faceIndex]);
    return face ? { face, point: hit.point } : null;
  }

  /** Snap a point near the cursor: vertices, then edge midpoints, then a 5 cm grid. */
  function snapPoint(p: THREE.Vector3, n: THREE.Vector3, e: { clientX: number; clientY: number }, onLabel: string): { p: THREE.Vector3; label: string } {
    const s = live.current.scene!;
    const r = t.current!.renderer.domElement.getBoundingClientRect();
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    let best: { p: THREE.Vector3; label: string; d: number } | null = null;
    const consider = (q: THREE.Vector3, label: string) => {
      if (Math.abs(q.clone().sub(p).dot(n)) > 1e-3) return; // stay on the drawing plane
      const sc = toScreen(q);
      const d = Math.hypot(sc.x - mx, sc.y - my);
      if (d < 12 && (!best || d < best.d)) best = { p: q, label, d };
    };
    for (let i = 0; i < s.edges.length; i += 6) {
      const a = new THREE.Vector3(s.edges[i], s.edges[i + 1], s.edges[i + 2]);
      const b = new THREE.Vector3(s.edges[i + 3], s.edges[i + 4], s.edges[i + 5]);
      consider(a, "Endpoint");
      consider(b, "Endpoint");
      consider(a.clone().add(b).multiplyScalar(0.5), "Midpoint");
    }
    if (best) return { p: (best as { p: THREE.Vector3 }).p.clone(), label: (best as { label: string }).label };
    const q = p.clone();
    const axisAligned = [0, 1, 2].find((k) => Math.abs(n.getComponent(k)) > 0.999);
    if (axisAligned !== undefined) {
      for (let k = 0; k < 3; k++) if (k !== axisAligned) q.setComponent(k, round(q.getComponent(k), 0.05));
    }
    return { p: q, label: onLabel };
  }

  function planeHit(e: { clientX: number; clientY: number }, origin: THREE.Vector3, n: THREE.Vector3): THREE.Vector3 | null {
    const out = new THREE.Vector3();
    return ray(e).ray.intersectPlane(new THREE.Plane().setFromNormalAndCoplanarPoint(n, origin), out);
  }

  function clearPreview() {
    const T = t.current;
    if (!T) return;
    T.preview.children.forEach((c) => (c as THREE.Mesh).geometry?.dispose());
    T.preview.clear();
  }

  function cancelTool() {
    st.current.rect = null;
    st.current.push = null;
    clearPreview();
    setSnap(null);
    live.current.onMeasure("", "");
  }

  function rectCorners(r: NonNullable<typeof st.current.rect>): THREE.Vector3[] {
    const { p0, u, v, a, b } = r;
    return [p0.clone(), p0.clone().addScaledVector(u, a), p0.clone().addScaledVector(u, a).addScaledVector(v, b), p0.clone().addScaledVector(v, b)];
  }

  function drawRectPreview() {
    const r = st.current.rect;
    clearPreview();
    if (!r) return;
    const pts = rectCorners(r);
    const T = t.current!;
    T.preview.add(outline(pts));
    const fill = new THREE.BufferGeometry().setFromPoints([pts[0], pts[1], pts[2], pts[0], pts[2], pts[3]]);
    T.preview.add(new THREE.Mesh(fill, new THREE.MeshBasicMaterial({ color: ACCENT, transparent: true, opacity: 0.15, side: THREE.DoubleSide, depthWrite: false })));
    live.current.onMeasure("Dimensions", `${Math.abs(r.a).toFixed(2)} × ${Math.abs(r.b).toFixed(2)} m`);
  }

  function drawPushPreview() {
    const p = st.current.push;
    clearPreview();
    if (!p) return;
    const base = p.face.outer.map(V);
    const off = p.n.clone().multiplyScalar(p.d);
    const top = base.map((q) => q.clone().add(off));
    const T = t.current!;
    T.preview.add(outline(top));
    const seg: THREE.Vector3[] = [];
    base.forEach((q, i) => seg.push(q, top[i]));
    T.preview.add(new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(seg), new THREE.LineBasicMaterial({ color: ACCENT, depthTest: false })));
    const cap = faceTris(p.face.id, 0);
    cap.translate(off.x, off.y, off.z);
    T.preview.add(new THREE.Mesh(cap, new THREE.MeshBasicMaterial({ color: ACCENT, transparent: true, opacity: 0.2, side: THREE.DoubleSide, depthWrite: false })));
    live.current.onMeasure("Distance", `${p.d.toFixed(2)} m`);
  }

  async function commitRect() {
    const r = st.current.rect;
    if (!r || Math.abs(r.a) < 0.01 || Math.abs(r.b) < 0.01) return;
    const pts = rectCorners(r).map((q) => [q.x, q.y, q.z].map((x) => Math.round(x * 1e4) / 1e4));
    st.current.rect = null;
    clearPreview();
    await live.current.run({ cmd: "polygon", points: pts });
  }

  async function commitPush() {
    const p = st.current.push;
    st.current.push = null;
    clearPreview();
    if (!p || Math.abs(p.d) < 0.005) return;
    const d = Math.round(p.d * 1000) / 1000;
    const r = await live.current.run({ cmd: "pushpull", face: p.face.id, distance: d });
    if (r.ok) st.current.lastPush = { face: p.face.id, d };
  }

  function fit() {
    const T = t.current;
    const s = live.current.scene;
    if (!T) return;
    const box = new THREE.Box3();
    if (s && s.positions.length) {
      for (let i = 0; i < s.positions.length; i += 3) box.expandByPoint(new THREE.Vector3(s.positions[i], s.positions[i + 1], s.positions[i + 2]));
    } else {
      box.set(new THREE.Vector3(-5, -5, 0), new THREE.Vector3(5, 5, 3));
    }
    const c = box.getCenter(new THREE.Vector3());
    const rad = Math.max(box.getSize(new THREE.Vector3()).length() / 2, 2);
    const dir = T.camera.position.clone().sub(T.controls.target).normalize();
    T.controls.target.copy(c);
    T.camera.position.copy(c.clone().addScaledVector(dir, rad / Math.sin(THREE.MathUtils.degToRad(T.camera.fov / 2)) * 1.1));
  }

  function view(name: "iso" | "top" | "front" | "right") {
    const T = t.current!;
    const dirs = { iso: [1, -1.35, 0.95], top: [0, -0.001, 1], front: [0, -1, 0.1], right: [1, 0, 0.1] } as const;
    const d = V(dirs[name] as unknown as Vec3).normalize();
    const dist = T.camera.position.distanceTo(T.controls.target);
    T.camera.position.copy(T.controls.target.clone().addScaledVector(d, dist));
    fit();
  }

  async function applyTyped(text: string) {
    const nums = text.split(/[ ,x×]+/).map(Number).filter((x) => !Number.isNaN(x));
    if (!nums.length) return;
    const tool = live.current.tool;
    if (tool === "rect" && st.current.rect && nums.length >= 2) {
      const r = st.current.rect;
      r.a = Math.sign(r.a || 1) * Math.abs(nums[0]);
      r.b = Math.sign(r.b || 1) * Math.abs(nums[1]);
      await commitRect();
    } else if (tool === "push") {
      if (st.current.push) {
        st.current.push.d = nums[0];
        await commitPush();
      } else if (st.current.lastPush) {
        const lp = st.current.lastPush;
        await live.current.run({ cmd: "undo" });
        const r = await live.current.run({ cmd: "pushpull", face: lp.face, distance: nums[0] });
        if (r.ok) st.current.lastPush = { face: lp.face, d: nums[0] };
      }
    }
    live.current.onMeasure("", "");
  }

  api.current = { applyTyped, fit, view };

  // ---------- input ----------
  useEffect(() => {
    const el = t.current!.renderer.domElement;
    const T = t.current!;

    const move = (e: PointerEvent) => {
      const { tool } = live.current;
      if (!live.current.scene || st.current.space || tool === "orbit") return;
      if (tool === "rect") {
        const r = st.current.rect;
        if (r) {
          const hit = planeHit(e, r.p0, r.n);
          if (!hit) return;
          const sp = snapPoint(hit, r.n, e, "On plane");
          const d = sp.p.clone().sub(r.p0);
          r.a = d.dot(r.u);
          r.b = d.dot(r.v);
          drawRectPreview();
          const sc = toScreen(sp.p);
          setSnap({ ...sc, label: sp.label });
        } else {
          const h = hitFace(e);
          const n = h ? V(h.face.normal) : new THREE.Vector3(0, 0, 1);
          const p = h ? h.point : planeHit(e, new THREE.Vector3(), n);
          if (!p) return setSnap(null);
          const sp = snapPoint(p, n, e, h ? "On face" : "On ground");
          setSnap({ ...toScreen(sp.p), label: sp.label });
        }
        return;
      }
      if (tool === "push" && st.current.push) {
        const p = st.current.push;
        // closest point on the push line to the mouse ray
        const rc = ray(e).ray;
        const w0 = p.origin.clone().sub(rc.origin);
        const b = p.n.dot(rc.direction);
        const dd = p.n.dot(w0);
        const ee = rc.direction.dot(w0);
        const den = 1 - b * b;
        if (Math.abs(den) < 1e-6) return;
        p.d = round((b * ee - dd) / den, 0.05);
        p.dragged = true;
        drawPushPreview();
        return;
      }
      const h = hitFace(e);
      const id = h ? h.face.id : null;
      if (id !== st.current.hover) {
        st.current.hover = id;
        drawOverlay();
      }
    };

    const down = async (e: PointerEvent) => {
      if (e.button !== 0 || st.current.space) return;
      const { tool, color } = live.current;
      if (tool === "orbit") return;
      if (tool === "rect") {
        if (st.current.rect) return void (await commitRect());
        const h = hitFace(e);
        const n = h ? V(h.face.normal) : new THREE.Vector3(0, 0, 1);
        const p = h ? h.point : planeHit(e, new THREE.Vector3(), n);
        if (!p) return;
        const sp = snapPoint(p, n, e, "");
        const [u, v] = basis(n);
        st.current.rect = { p0: sp.p, n, u, v, a: 0, b: 0 };
        return;
      }
      if (tool === "push") {
        if (st.current.push) return void (await commitPush()); // click-move-click
        const h = hitFace(e);
        if (!h) return;
        st.current.push = { face: h.face, origin: h.point.clone(), n: V(h.face.normal), d: 0, dragged: false };
        el.setPointerCapture(e.pointerId);
        return;
      }
      const h = hitFace(e);
      if (tool === "select") return live.current.onPick(h?.face ?? null, { additive: e.shiftKey, deep: e.detail >= 2 });
      if (!h) return;
      if (tool === "paint") await live.current.run({ cmd: "paint", ids: [h.face.id], color });
      if (tool === "erase") await live.current.run({ cmd: "delete", ids: [h.face.id] });
    };

    const up = async (e: PointerEvent) => {
      if (live.current.tool === "push" && st.current.push?.dragged) {
        el.releasePointerCapture(e.pointerId);
        await commitPush();
      }
    };

    const key = (e: KeyboardEvent) => {
      if (e.code === "Space" && !(e.target instanceof HTMLInputElement)) {
        const down = e.type === "keydown";
        if (down) e.preventDefault();
        if (st.current.space === down) return;
        st.current.space = down;
        T.controls.mouseButtons = {
          LEFT: down || live.current.tool === "orbit" ? THREE.MOUSE.ROTATE : (null as unknown as THREE.MOUSE),
          MIDDLE: THREE.MOUSE.ROTATE,
          RIGHT: THREE.MOUSE.PAN,
        };
      }
      if (e.type === "keydown" && e.key === "Escape") cancelTool();
    };

    el.addEventListener("pointermove", move);
    el.addEventListener("pointerdown", down);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointerleave", () => setSnap(null));
    window.addEventListener("keydown", key);
    window.addEventListener("keyup", key);
    return () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerdown", down);
      el.removeEventListener("pointerup", up);
      window.removeEventListener("keydown", key);
      window.removeEventListener("keyup", key);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cursor = tool === "orbit" ? "grab" : tool === "select" ? "default" : "crosshair";
  return (
    <div className="viewport" ref={host} style={{ cursor }}>
      {snap && (
        <div className="snap" style={{ left: snap.x, top: snap.y }}>
          <span className={`snap-dot ${snap.label === "Endpoint" ? "end" : snap.label === "Midpoint" ? "mid" : ""}`} />
          {snap.label && <span className="snap-label">{snap.label}</span>}
        </div>
      )}
    </div>
  );
}
