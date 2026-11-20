// The UV editor: a floating (or docked) 2D view of the selected faces' texture coordinates. The picture lies under the
// UV wireframe; islands are outlined and tinted; clicking an island selects its faces in the 3D view and selecting faces
// in 3D shows them here. Every edit is an engine command (`uv_unwrap`, `uv_pack`, `uv_lightmap`, `uv_set`,
// `uv_transform`, `uv_seam`), one undo step each; while a drag is going the panel only draws the preview.
//
// Layers: the canvas under the wireframe is drawn by a stack of `UvLayer`s (the picture or the checker first). A texture
// paint layer plugs in with `addUvLayer` and draws between the picture and the wireframe, in UV space (docs/dev/uv-editor.md).
import { useEffect, useMemo, useRef, useState } from "react";
import { exec, type Scene } from "./engine";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { isEmpty, type Sel } from "./selection";
import {
  about, cornerKey, coverage, edgeIdsFor, fitView, flip, fromScreen, hitEdge, hitIsland, hitVertex, indexOf, islandCorners, islandFaces, islandPaths, islandsSelected, isFlat, livePins, loopEdges, marquee, marqueeIslands, nudge, outside,
  panBy, parseKey, pictureMaterial, pinList, pivotOf, selectedEdges, setCommand, toScreen, translation, transformed, wholeIslandsCommand, zoomAt,
  type Aff, type UvInfo, type V2, type View,
} from "./uveditor";
import { clampUvRadius, decodePixels, extendUvStroke, Painter, sampleRgba, type Pixels, stepUvRadius, stroke2DCommand, type TPaintSettings } from "./texturepaint";

/** A thing drawn under the wireframe, in UV space (the picture, a checker, later a paint layer). */
export interface UvLayer {
  id: string;
  draw: (g: CanvasRenderingContext2D, view: View) => void;
}
const extraLayers: UvLayer[] = [];
const layerListeners = new Set<() => void>();
/** Ask the panel to draw its layers again (a layer's own content changed). */
export function redrawUvLayers() {
  layerListeners.forEach((f) => f());
}
/** Add a layer between the picture and the wireframe; returns the function that removes it. */
export function addUvLayer(l: UvLayer): () => void {
  extraLayers.push(l);
  layerListeners.forEach((f) => f());
  return () => {
    extraLayers.splice(extraLayers.indexOf(l), 1);
    layerListeners.forEach((f) => f());
  };
}

type Mode = "vertex" | "edge" | "island" | "paint";
type Modal = { kind: "grab" | "rotate" | "scale"; from: V2; pivot: V2 };
type Drag = { kind: "move" | "marquee" | "pan" | "paint"; from: V2; px: V2; moved: boolean; view0: View; add: boolean };

const EMPTY_INFO: UvInfo = { faces: [], without: [], islands: 0, seams: [] };
const tint = (i: number) => `hsl(${(i * 67) % 360} 70% 55%)`;

export function UvPanel({ scene, sel, setSel, run, color, setColor, tpaint, setTPaint, checker, setChecker, onClose, onToast }: { scene: Scene; sel: Sel; setSel: (s: Sel) => void; run: Run; color: [number, number, number]; setColor: (c: [number, number, number]) => void; tpaint: TPaintSettings; setTPaint: (v: TPaintSettings) => void; checker: boolean; setChecker: (on: boolean) => void; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const [targets, setTargets] = useState<number[]>(() => targetsOf(sel));
  const [set, setSet] = useState<1 | 2>(1);
  const [info, setInfo] = useState<UvInfo>(EMPTY_INFO);
  const [preview, setPreview] = useState<UvInfo | null>(null);
  const [view, setView] = useState<View>(() => fitView(520, 420));
  const [mode, setMode] = useState<Mode>("island");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [pins, setPins] = useState<Set<string>>(new Set());
  const [method, setMethod] = useState("conformal");
  const [seamsMode, setSeamsMode] = useState("marked");
  const [angle, setAngle] = useState(60);
  const [busy, setBusy] = useState(false);
  const [docked, setDocked] = useState(false);
  const [pos, setPos] = useState<V2>(() => [Math.max(16, window.innerWidth - 640), 90]);
  const [modal, setModal] = useState<Modal | null>(null);
  const [tick, bump] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const drag = useRef<Drag | null>(null);
  const [marq, setMarq] = useState<[V2, V2] | null>(null);
  const fromPanel = useRef<string>("");
  // painting on the picture: its pixels, the stroke being dragged, and the paint layer waiting for the new picture to arrive
  const paintPx = useRef<{ data: string; rgba: Pixels; w: number; h: number } | null>(null);
  const strokeRef = useRef<{ painter: Painter; canvas: HTMLCanvasElement; pts: [number, number][]; material: string; erase: boolean; remove: () => void } | null>(null);
  const pendingLayer = useRef<(() => void) | null>(null);
  const [ring, setRing] = useState<V2 | null>(null);
  const shown = preview ?? info;
  const ix = useMemo(() => indexOf(shown), [shown]);
  const textured = useMemo(() => new Set(scene.materials.filter((m) => m.texture).map((m) => m.name)), [scene.materials]);
  const material = pictureMaterial(info, textured);
  const picture = material ? scene.materials.find((m) => m.name === material)?.texture?.data ?? null : null;

  // 3D -> 2D: a selection made out there is the new scope (one this panel made itself is not). The app hands a new
  // selection object on every refresh, so this follows the ids, not the object.
  const selSig = JSON.stringify([sel.faces, sel.groups, sel.instances]);
  const targetsRef = useRef(targets);
  targetsRef.current = targets;
  useEffect(() => {
    if (selSig === fromPanel.current) {
      fromPanel.current = "";
      return;
    }
    const next = targetsOf(sel);
    if (next.length && JSON.stringify(next) !== JSON.stringify(targetsRef.current)) {
      setTargets(next);
      setChosen(new Set());
    }
  }, [selSig]); // eslint-disable-line react-hooks/exhaustive-deps

  // read what the 2D view shows whenever the model, the scope or the channel changes
  const key = JSON.stringify(targets);
  useEffect(() => {
    let live = true;
    if (!targets.length) {
      setInfo(EMPTY_INFO);
      return;
    }
    void exec({ cmd: "uv_info", faces: targets, set }).then((r) => {
      if (!live) return;
      if (r.ok) setInfo({ faces: r.faces as UvInfo["faces"], without: r.without as number[], islands: r.islands as number, seams: r.seams as number[] });
      else setInfo(EMPTY_INFO);
    });
    return () => {
      live = false;
    };
  }, [key, set, scene]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => setPins((p) => livePins(info, p)), [info]);
  useEffect(() => {
    // corners that no longer exist drop out of the selection
    setChosen((c) => {
      const n = new Set([...c].filter((k) => ix.faces.has(parseKey(k)[0])));
      return n.size === c.size ? c : n;
    });
  }, [ix]);

  // the stage's size
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      if (r.width > 20 && r.height > 20) setView((v) => (Math.abs(v.w - r.width) < 1 && Math.abs(v.h - r.height) < 1 ? v : { ...v, w: r.width, h: r.height }));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    const f = () => bump((n) => n + 1);
    layerListeners.add(f);
    return () => void layerListeners.delete(f);
  }, []);

  // the picture (or checker) under the wireframe
  const img = useRef<HTMLImageElement | null>(null);
  useEffect(() => {
    if (!picture) return void (img.current = null);
    const i = new Image();
    i.onload = () => {
      img.current = i;
      // the picture a stroke made has arrived: the layer that showed it meanwhile is not needed
      pendingLayer.current?.();
      pendingLayer.current = null;
      bump((n) => n + 1);
    };
    i.src = picture;
  }, [picture]);
  // the pixels the brush paints on (read once the Paint mode is on)
  useEffect(() => {
    if (mode !== "paint" || !picture) return;
    let live = true;
    void decodePixels(picture).then((p) => {
      if (live) paintPx.current = { data: picture, ...p };
    }, () => undefined);
    return () => {
      live = false;
    };
  }, [mode, picture]);
  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(view.w * dpr);
    c.height = Math.round(view.h * dpr);
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = "#2b2e3a";
    g.fillRect(0, 0, view.w, view.h);
    const [x0] = toScreen(view, [0, 0]);
    const [x1, y0] = toScreen(view, [1, 1]);
    const size = x1 - x0;
    const base: UvLayer = {
      id: "picture",
      draw: (g2) => {
        if (picture && img.current && !checker) g2.drawImage(img.current, x0, y0, size, size);
        else {
          // a checker: eight cells across, so stretching shows as squashed or skewed squares
          const cell = size / 8;
          for (let j = 0; j < 8; j++) for (let i = 0; i < 8; i++) (g2.fillStyle = (i + j) % 2 ? "#8f93a3" : "#cfd2dc"), g2.fillRect(x0 + i * cell, y0 + j * cell, cell, cell);
        }
      },
    };
    for (const l of [base, ...extraLayers]) l.draw(g, view);
    g.strokeStyle = "rgba(255,255,255,0.55)";
    g.lineWidth = 1;
    g.strokeRect(x0 + 0.5, y0 + 0.5, size, size);
  }, [view, picture, checker, tick, key]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- commands --------------------------------------------------------------------------------------------------------
  const go = async (cmd: Record<string, unknown>, ok?: (r: Record<string, unknown>) => string): Promise<boolean> => {
    setBusy(true);
    try {
      const r = await run(cmd);
      if (r.ok && ok) onToast(ok(r));
      return r.ok;
    } finally {
      setBusy(false);
    }
  };
  const haveTargets = targets.length > 0;
  const needFaces = () => (haveTargets ? true : (onToast(t("uv.need"), true), false));
  const seamEdges = (): number[] => {
    const fromUv = edgeIdsFor(selectedEdges(shown, chosen), scene.edgeList);
    return [...new Set([...sel.edges, ...fromUv])];
  };
  const seam = async (on: boolean) => {
    const edges = seamEdges();
    if (!edges.length) return onToast(t("uv.seam.none"), true);
    await go({ cmd: "uv_seam", edges, on }, (r) => t(on ? "uv.seam.marked" : "uv.seam.cleared").replace("{n}", String(r.changed)));
  };
  const autoSeams = async () => {
    if (!needFaces()) return;
    await go({ cmd: "uv_seam", faces: targets, auto: angle }, (r) => t("uv.seam.auto").replace("{n}", String((r.edges as number[]).length)).replace("{a}", String(angle)));
  };
  const unwrap = async () => {
    if (!needFaces()) return;
    const pl = pinList(info, pins);
    const ok = await go({ cmd: "uv_unwrap", faces: targets, method, seams: seamsMode, angle, ...(pl.length ? { pins: pl } : {}) }, (r) => t("uv.unwrapped").replace("{f}", String(r.faces)).replace("{i}", String(r.islands)) + ((r.ignored_pins as number) > 0 ? " " + t("uv.pins.ignored").replace("{n}", String(r.ignored_pins)) : ""));
    if (ok) {
      setChosen(new Set());
      setView(fitView(view.w, view.h));
    }
  };
  const pack = async () => {
    if (!needFaces()) return;
    await go({ cmd: "uv_pack", faces: targets, set, margin: set === 2 ? 0.02 : 0.01 }, (r) => t("uv.packed").replace("{i}", String(r.islands)));
  };
  const lightmap = async () => {
    if (!needFaces()) return;
    if (await go({ cmd: "uv_lightmap", faces: targets }, (r) => t("uv.lightmapped").replace("{i}", String(r.islands)))) setSet(2);
  };
  const reset = async () => {
    if (!info.faces.length) return;
    const uv: Record<string, null> = {};
    for (const f of info.faces) uv[String(f.id)] = null;
    await go({ cmd: "uv_set", uv, ...(set === 2 ? { set } : {}) }, () => t("uv.reset.done"));
    setChosen(new Set());
  };

  /** Apply a transform to the chosen corners (an island moves with one `uv_transform`) and keep it. */
  const commit = async (m: Aff, from: UvInfo = info) => {
    if (isFlat(m) || !chosen.size) return;
    const whole = wholeIslandsCommand(from, chosen, m, set);
    const cmd = whole ?? setCommand(from, transformed(from, chosen, m), set);
    setPreview(null);
    if (cmd) await go(cmd);
  };
  const pivot = () => pivotOf(shown, chosen);
  const apply = (mk: (p: V2) => Aff) => {
    const p = pivot();
    if (!p) return onToast(t("uv.choose"), true);
    void commit(mk(p));
  };

  const pinSelected = () => {
    if (!chosen.size) return onToast(t("uv.choose"), true);
    setPins((p) => {
      const n = new Set(p);
      const all = [...chosen].every((k) => n.has(k));
      chosen.forEach((k) => (all ? n.delete(k) : n.add(k)));
      return n;
    });
  };

  // ---- the pointer -----------------------------------------------------------------------------------------------------
  const local = (e: { clientX: number; clientY: number }): V2 => {
    const r = stage.current!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };

  const select = (keys: string[], add: boolean) => setChosen((c) => (add ? new Set([...c, ...keys]) : new Set(keys)));
  const syncTo3d = (faces: number[]) => {
    const inScene = faces.filter((f) => scene.faces.some((x) => x.id === f));
    if (!inScene.length) return;
    const next: Sel = { faces: inScene, edges: [], groups: [], instances: [] };
    fromPanel.current = JSON.stringify([next.faces, next.groups, next.instances]);
    setSel(next);
  };

  /** Paint mode: a press starts a stroke on the picture (Cmd/Ctrl-click picks up the colour). */
  const paintDown = (e: React.PointerEvent, s: V2) => {
    const px = paintPx.current;
    const m = material ? scene.materials.find((x) => x.name === material) : null;
    if (!m?.texture || !picture || !px || px.data !== picture) return onToast(t("uv.paint.nopicture"), true);
    const uv = fromScreen(view, s);
    if (e.metaKey || e.ctrlKey) return setColor(sampleRgba(px.rgba, px.w, px.h, uv));
    const painter = new Painter(px.rgba, px.w, px.h, { color, opacity: tpaint.opacity, hardness: tpaint.hardness, erase: e.altKey, base: m.color as [number, number, number] });
    const canvas = document.createElement("canvas");
    canvas.width = px.w;
    canvas.height = px.h;
    canvas.getContext("2d")!.putImageData(new ImageData(painter.cur, px.w, px.h), 0, 0);
    const layer: UvLayer = {
      id: "paint",
      draw: (g, v) => {
        const [x0] = toScreen(v, [0, 0]);
        const [x1, y0] = toScreen(v, [1, 1]);
        g.drawImage(canvas, x0, y0, x1 - x0, x1 - x0);
      },
    };
    const remove = addUvLayer(layer);
    const st = { painter, canvas, pts: [] as [number, number][], material: material!, erase: e.altKey, remove };
    strokeRef.current = st;
    for (const p of extendUvStroke(st.pts, uv, tpaint.uvRadius)) painter.dab2D(p, tpaint.uvRadius);
    flushPaint(st);
    drag.current = { kind: "paint", from: s, px: s, moved: false, view0: view, add: false };
  };
  const flushPaint = (st: NonNullable<typeof strokeRef.current>) => {
    const d = st.painter.takeDirty();
    if (!d) return;
    st.canvas.getContext("2d")!.putImageData(new ImageData(st.painter.cur, st.painter.w, st.painter.h), 0, 0, d[0], d[1], d[2], d[3]);
    redrawUvLayers();
  };
  const finishPaint = async () => {
    const st = strokeRef.current;
    strokeRef.current = null;
    if (!st) return;
    const cmd = stroke2DCommand({ material: st.material, points: st.pts, radius: tpaint.uvRadius, color, settings: tpaint, erase: st.erase });
    if (!cmd) return st.remove();
    // the layer keeps showing the stroke until the new picture arrives (or a moment has passed)
    pendingLayer.current = st.remove;
    const ok = await go(cmd);
    if (!ok) {
      pendingLayer.current = null;
      st.remove();
    } else setTimeout(() => (pendingLayer.current === st.remove ? (pendingLayer.current = null, st.remove()) : undefined), 2000);
  };

  const down = (e: React.PointerEvent) => {
    if (modal) {
      void finishModal(true);
      return;
    }
    stage.current?.focus();
    const s = local(e);
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    if (e.button === 1 || e.button === 2 || (e.altKey && mode !== "paint")) {
      drag.current = { kind: "pan", from: s, px: s, moved: false, view0: view, add: false };
      return;
    }
    if (mode === "paint") return paintDown(e, s);
    const add = e.shiftKey;
    let hit: string[] | null = null;
    if (mode === "vertex") hit = hitVertex(shown, ix, view, s);
    else if (mode === "edge") hit = hitEdge(shown, ix, view, s) ?? null;
    else {
      const i = hitIsland(shown, fromScreen(view, s));
      hit = i == null ? null : islandCorners(shown, i);
      if (i != null) syncTo3d(islandFaces(shown, i));
    }
    if (hit) {
      const already = hit.every((k) => chosen.has(k));
      if (!already || add) select(hit, add);
      drag.current = { kind: "move", from: s, px: s, moved: false, view0: view, add };
    } else {
      if (!add) setChosen(new Set());
      drag.current = { kind: "marquee", from: s, px: s, moved: false, view0: view, add };
    }
  };

  const movePointer = (e: React.PointerEvent) => {
    const s = local(e);
    lastPointer.current = s;
    if (modal) return modalMove(s);
    if (mode === "paint") setRing(s);
    const d = drag.current;
    if (!d) return;
    if (d.kind === "paint") {
      const st = strokeRef.current;
      if (!st) return;
      for (const p of extendUvStroke(st.pts, fromScreen(view, s), tpaint.uvRadius)) st.painter.dab2D(p, tpaint.uvRadius);
      flushPaint(st);
      return;
    }
    if (Math.hypot(s[0] - d.from[0], s[1] - d.from[1]) > 3) d.moved = true;
    if (!d.moved) return;
    if (d.kind === "pan") {
      setView(panBy(d.view0, s[0] - d.from[0], s[1] - d.from[1]));
    } else if (d.kind === "marquee") {
      setMarq([d.from, s]);
    } else if (chosen.size) {
      const [a, b] = [fromScreen(d.view0, d.from), fromScreen(d.view0, s)];
      setPreview(transformed(info, chosen, translation(b[0] - a[0], b[1] - a[1])));
    }
  };

  const up = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.kind === "paint") return void finishPaint();
    const s = local(e);
    if (d.kind === "marquee" && d.moved) {
      if (mode === "island") {
        const isl = marqueeIslands(info, view, d.from, s);
        select(isl.flatMap((i) => islandCorners(info, i)), d.add);
        syncTo3d(isl.flatMap((i) => islandFaces(info, i)));
      } else select(marquee(info, view, d.from, s), d.add);
      setMarq(null);
    } else if (d.kind === "move" && d.moved && chosen.size) {
      const [a, b] = [fromScreen(d.view0, d.from), fromScreen(d.view0, s)];
      void commit(translation(b[0] - a[0], b[1] - a[1]));
    }
    setMarq(null);
  };

  const wheel = (e: React.WheelEvent) => {
    const s = local(e);
    setView((v) => zoomAt(v, s, e.deltaY < 0 ? 1.15 : 1 / 1.15));
  };

  // ---- G R S, the modal transforms -------------------------------------------------------------------------------------
  const lastPointer = useRef<V2>([0, 0]);
  const startModal = (kind: Modal["kind"]) => {
    const p = pivot();
    if (!p || !chosen.size) return onToast(t("uv.choose"), true);
    setModal({ kind, from: fromScreen(view, lastPointer.current), pivot: p });
  };
  const modalMatrix = (m: Modal, s: V2): Aff => {
    const q = fromScreen(view, s);
    if (m.kind === "grab") return translation(q[0] - m.from[0], q[1] - m.from[1]);
    const [a, b] = [Math.atan2(m.from[1] - m.pivot[1], m.from[0] - m.pivot[0]), Math.atan2(q[1] - m.pivot[1], q[0] - m.pivot[0])];
    if (m.kind === "rotate") return about(m.pivot, ((b - a) * 180) / Math.PI);
    const [d0, d1] = [Math.hypot(m.from[0] - m.pivot[0], m.from[1] - m.pivot[1]), Math.hypot(q[0] - m.pivot[0], q[1] - m.pivot[1])];
    return about(m.pivot, 0, d0 > 1e-9 ? d1 / d0 : 1);
  };
  const modalMove = (s: V2) => {
    if (modal) setPreview(transformed(info, chosen, modalMatrix(modal, s)));
  };
  const finishModal = async (keep: boolean) => {
    const m = modal;
    setModal(null);
    if (!m) return;
    if (!keep) return setPreview(null);
    await commit(modalMatrix(m, lastPointer.current));
  };

  // ---- the keyboard ----------------------------------------------------------------------------------------------------
  const keyDown = (e: React.KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (/^(INPUT|SELECT|TEXTAREA)$/.test(el.tagName)) return e.stopPropagation();
    const k = e.key.toLowerCase();
    const mod = e.metaKey || e.ctrlKey;
    let handled = true;
    if (modal) {
      if (k === "escape") void finishModal(false);
      else if (k === "enter") void finishModal(true);
      else handled = false;
    } else if (k === "1") setMode("vertex");
    else if (k === "2") setMode("edge");
    else if (k === "3") setMode("island");
    else if (k === "4" || k === "b") setMode("paint");
    else if (mode === "paint" && (k === "[" || k === "]") && !mod) setTPaint({ ...tpaint, uvRadius: stepUvRadius(tpaint.uvRadius, k === "[" ? -1 : 1) });
    else if (k === "g") startModal("grab");
    else if (k === "r") startModal("rotate");
    else if (k === "s" && !mod) startModal("scale");
    else if (k === "p") pinSelected();
    else if (k === "f" || k === "home") setView(fitView(view.w, view.h));
    else if (k === "a" && mod) setChosen(new Set(shown.faces.flatMap((f) => f.uv.map((_, c) => cornerKey(f.id, c)))));
    else if (k === "escape") {
      if (chosen.size) setChosen(new Set());
      else onClose();
    } else if (k.startsWith("arrow")) {
      if (!chosen.size) return;
      const step = nudge(view.zoom, e.shiftKey);
      void commit(translation(k === "arrowleft" ? -step : k === "arrowright" ? step : 0, k === "arrowup" ? step : k === "arrowdown" ? -step : 0));
    } else if (k === "m") void seam(true);
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  };

  // ---- the window ------------------------------------------------------------------------------------------------------
  const dragWin = useRef<{ dx: number; dy: number } | null>(null);
  const headDown = (e: React.PointerEvent) => {
    if (docked || (e.target as HTMLElement).closest("button")) return;
    dragWin.current = { dx: e.clientX - pos[0], dy: e.clientY - pos[1] };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  };
  const headMove = (e: React.PointerEvent) => {
    const d = dragWin.current;
    if (d) setPos([Math.max(0, Math.min(window.innerWidth - 80, e.clientX - d.dx)), Math.max(0, Math.min(window.innerHeight - 40, e.clientY - d.dy))]);
  };

  const paths = useMemo(() => islandPaths(shown, view), [shown, view]);
  const selIslands = useMemo(() => new Set(islandsSelected(shown, chosen)), [shown, chosen]);
  const hasAny = shown.faces.length > 0;
  const cornerCount = shown.faces.reduce((n, f) => n + f.uv.length, 0);
  const step = !haveTargets ? t("uv.step.select") : !hasAny ? t(set === 2 ? "uv.step.nolightmap" : "uv.step.unwrap") : modal ? t(`uv.step.${modal.kind}`) : mode === "paint" ? t("uv.step.paint") : chosen.size ? t("uv.step.chosen") : t(`uv.step.${mode}`);
  const used = Math.round(coverage(shown) * 100);
  const msel = (m: Mode, label: string, tip: string, k: string) => (
    <button role="radio" aria-checked={mode === m} className={mode === m ? "on" : ""} onClick={() => setMode(m)} title={`${tip} (${k})`} data-testid={`uv-mode-${m}`}>
      {label}
    </button>
  );
  const sty = docked ? undefined : { left: pos[0], top: pos[1] };

  return (
    <div ref={root} className={`uv-panel${docked ? " docked" : ""}`} style={sty} role="dialog" aria-label={t("uv.title")} data-testid="uv-panel" data-islands={shown.islands} data-faces={shown.faces.length} data-set={set} data-sel-faces={sel.faces.length} data-sel-groups={sel.groups.length} onKeyDown={keyDown}>
      <div className="uv-head" onPointerDown={headDown} onPointerMove={headMove} onPointerUp={() => (dragWin.current = null)}>
        <b>{t("uv.title")}</b>
        <span className="muted small">{t("uv.count").replace("{f}", String(shown.faces.length)).replace("{i}", String(shown.islands))}</span>
        <span className="uv-spacer" />
        <button className="btn small ghost" onClick={() => setDocked((d) => !d)} aria-pressed={docked} title={t("uv.dock.tip")} data-testid="uv-dock">
          {t(docked ? "uv.float" : "uv.dock")}
        </button>
        <button className="btn small ghost" onClick={onClose} aria-label={t("uv.close")} title={t("uv.close")} data-testid="uv-close">
          ×
        </button>
      </div>
      <div className="uv-step" aria-live="polite" data-testid="uv-step">
        {step}
      </div>
      <div className="uv-bar">
        <div className="seg" role="radiogroup" aria-label={t("uv.channel")}>
          <button role="radio" aria-checked={set === 1} className={set === 1 ? "on" : ""} onClick={() => setSet(1)} title={t("uv.channel.1.tip")} data-testid="uv-set-1">
            {t("uv.channel.1")}
          </button>
          <button role="radio" aria-checked={set === 2} className={set === 2 ? "on" : ""} onClick={() => setSet(2)} title={t("uv.channel.2.tip")} data-testid="uv-set-2">
            {t("uv.channel.2")}
          </button>
        </div>
        <div className="seg" role="radiogroup" aria-label={t("uv.mode")}>
          {msel("vertex", t("uv.mode.vertex"), t("uv.mode.vertex.tip"), "1")}
          {msel("edge", t("uv.mode.edge"), t("uv.mode.edge.tip"), "2")}
          {msel("island", t("uv.mode.island"), t("uv.mode.island.tip"), "3")}
          {msel("paint", t("uv.mode.paint"), t("uv.mode.paint.tip"), "4")}
        </div>
        <label className="check small" title={t("uv.checker.tip")}>
          <input type="checkbox" checked={checker} onChange={(e) => setChecker(e.target.checked)} data-testid="uv-checker" />
          <span>{t("uv.checker")}</span>
        </label>
      </div>
      {mode === "paint" && (
        <div className="uv-paintbar" role="group" aria-label={t("uv.mode.paint")} data-testid="uv-paintbar">
          <span className="vp-chip" style={{ background: `rgb(${color.map((c) => Math.round(c * 255)).join(",")})` }} title={t("vp.colorFrom")} />
          <label className="small" title={t("uv.paint.size")}>
            <span>{t("uv.paint.size")}</span>
            <input type="range" min={0.005} max={0.25} step={0.005} value={clampUvRadius(tpaint.uvRadius)} onChange={(e) => setTPaint({ ...tpaint, uvRadius: clampUvRadius(Number(e.target.value)) })} aria-label={t("uv.paint.size")} data-testid="uv-paint-size" />
          </label>
          <label className="small">
            <span>{t("tp.hardness")}</span>
            <input type="range" min={0} max={1} step={0.05} value={tpaint.hardness} onChange={(e) => setTPaint({ ...tpaint, hardness: Number(e.target.value) })} aria-label={t("tp.hardness")} />
          </label>
          <label className="small">
            <span>{t("tp.opacity")}</span>
            <input type="range" min={0.05} max={1} step={0.05} value={tpaint.opacity} onChange={(e) => setTPaint({ ...tpaint, opacity: Number(e.target.value) })} aria-label={t("tp.opacity")} />
          </label>
        </div>
      )}
      <div className="uv-stage" ref={stage} tabIndex={0} aria-label={t("uv.canvas")} onPointerDown={down} onPointerMove={movePointer} onPointerUp={up} onWheel={wheel} onContextMenu={(e) => e.preventDefault()} data-testid="uv-stage">
        <canvas ref={canvas} className="uv-bg" style={{ width: view.w, height: view.h }} data-testid="uv-bg" />
        <svg className="uv-svg" width={view.w} height={view.h} data-testid="uv-svg">
          {[...paths].map(([i, d]) => (
            <path key={i} d={d} className={`uv-island${selIslands.has(i) ? " on" : ""}`} style={{ ["--tint" as string]: tint(i) }} data-island={i} data-testid="uv-island" fillRule="evenodd" />
          ))}
          {cornerCount <= 3000 &&
            mode !== "island" &&
            shown.faces.flatMap((f) =>
              f.uv.map((p, c) => {
                const q = toScreen(view, p);
                const k = cornerKey(f.id, c);
                return <circle key={k} cx={q[0]} cy={q[1]} r={chosen.has(k) ? 4 : 2.5} className={`uv-pt${chosen.has(k) ? " on" : ""}`} />;
              })
            )}
          {mode === "edge" &&
            shown.faces.flatMap((f) =>
              loopEdges(f).map(([a, b]) => {
                if (!chosen.has(cornerKey(f.id, a)) || !chosen.has(cornerKey(f.id, b))) return null;
                const [p, q] = [toScreen(view, f.uv[a]), toScreen(view, f.uv[b])];
                return <line key={`${f.id}-${a}`} x1={p[0]} y1={p[1]} x2={q[0]} y2={q[1]} className="uv-edge on" />;
              })
            )}
          {[...pins].map((k) => {
            const [f, c] = parseKey(k);
            const p = ix.faces.get(f)?.uv[c];
            if (!p) return null;
            const q = toScreen(view, p);
            return (
              <g key={k} className="uv-pin" transform={`translate(${q[0]} ${q[1]})`} data-testid="uv-pin">
                <circle r={6} />
                <path d="M-3 -3 L3 3 M3 -3 L-3 3" />
              </g>
            );
          })}
          {mode === "paint" && ring && <circle className="uv-brush" cx={ring[0]} cy={ring[1]} r={Math.max(2, tpaint.uvRadius * (toScreen(view, [1, 0])[0] - toScreen(view, [0, 0])[0]))} data-testid="uv-brush" />}
          {marq && <rect className="uv-marquee" x={Math.min(marq[0][0], marq[1][0])} y={Math.min(marq[0][1], marq[1][1])} width={Math.abs(marq[1][0] - marq[0][0])} height={Math.abs(marq[1][1] - marq[0][1])} />}
        </svg>
      </div>
      <div className="uv-actions">
        <div className="uv-group" role="group" aria-label={t("uv.group.seams")}>
          <button className="btn small" onClick={() => void seam(true)} disabled={busy} title={t("uv.seam.mark.tip")} data-testid="uv-mark-seam">
            {t("uv.seam.mark")}
          </button>
          <button className="btn small" onClick={() => void seam(false)} disabled={busy} title={t("uv.seam.clear.tip")} data-testid="uv-clear-seam">
            {t("uv.seam.clear")}
          </button>
          <button className="btn small" onClick={() => void autoSeams()} disabled={busy} title={t("uv.seam.auto.tip")} data-testid="uv-auto-seams">
            {t("uv.seam.autobtn")}
          </button>
        </div>
        <div className="uv-group" role="group" aria-label={t("uv.group.unwrap")}>
          <select value={method} onChange={(e) => setMethod(e.target.value)} aria-label={t("uv.method")} title={t("uv.method.tip")} data-testid="uv-method">
            <option value="conformal">{t("uv.method.conformal")}</option>
            <option value="angle">{t("uv.method.angle")}</option>
            <option value="planar_islands">{t("uv.method.planar")}</option>
          </select>
          <select value={seamsMode} onChange={(e) => setSeamsMode(e.target.value)} aria-label={t("uv.seams")} title={t("uv.seams.tip")} data-testid="uv-seams">
            <option value="marked">{t("uv.seams.marked")}</option>
            <option value="auto">{t("uv.seams.auto")}</option>
          </select>
          <label className="small uv-angle" title={t("uv.angle.tip")}>
            <span>{t("uv.angle")}</span>
            <input type="number" min={1} max={179} value={angle} onChange={(e) => setAngle(Math.max(1, Math.min(179, Number(e.target.value) || 60)))} aria-label={t("uv.angle")} data-testid="uv-angle" />
          </label>
          <button className="btn small primary" onClick={() => void unwrap()} disabled={busy || set === 2} title={t(pins.size ? "uv.unwrap.pins.tip" : "uv.unwrap.tip")} data-testid="uv-unwrap">
            {t(pins.size ? "uv.unwrap.pins" : "uv.unwrap").replace("{n}", String(pins.size))}
          </button>
          <button className="btn small" onClick={() => void pack()} disabled={busy || !hasAny} title={t("uv.pack.tip")} data-testid="uv-pack">
            {t("uv.pack")}
          </button>
          <button className="btn small" onClick={() => void lightmap()} disabled={busy} title={t("uv.lightmap.tip")} data-testid="uv-lightmap">
            {t("uv.lightmap")}
          </button>
          <button className="btn small ghost" onClick={() => void reset()} disabled={busy || !hasAny} title={t("uv.reset.tip")} data-testid="uv-reset">
            {t("uv.reset")}
          </button>
        </div>
        <div className="uv-group" role="group" aria-label={t("uv.group.edit")}>
          <button className="btn small ghost" onClick={() => startModal("grab")} title={t("uv.grab.tip")} data-testid="uv-grab">
            {t("uv.grab")}
          </button>
          <button className="btn small ghost" onClick={() => startModal("rotate")} title={t("uv.rotate.tip")} data-testid="uv-rotate">
            {t("uv.rotate")}
          </button>
          <button className="btn small ghost" onClick={() => startModal("scale")} title={t("uv.scale.tip")} data-testid="uv-scale">
            {t("uv.scale")}
          </button>
          <button className="btn small ghost" onClick={() => apply((p) => about(p, 90))} title={t("uv.turn90.tip")} data-testid="uv-turn90">
            {t("uv.turn90")}
          </button>
          <button className="btn small ghost" onClick={() => apply((p) => flip(p, "h"))} title={t("uv.flipH.tip")} data-testid="uv-flip-h">
            {t("uv.flipH")}
          </button>
          <button className="btn small ghost" onClick={() => apply((p) => flip(p, "v"))} title={t("uv.flipV.tip")} data-testid="uv-flip-v">
            {t("uv.flipV")}
          </button>
          <button className="btn small ghost" onClick={pinSelected} title={t("uv.pin.tip")} data-testid="uv-pin-btn">
            {t("uv.pin")}
          </button>
          <button className="btn small ghost" onClick={() => setView(fitView(view.w, view.h))} title={t("uv.fit.tip")} data-testid="uv-fit">
            {t("uv.fit")}
          </button>
        </div>
      </div>
      <div className="uv-foot muted small" data-testid="uv-stats" data-used={used} data-outside={outside(shown)}>
        {t("uv.foot").replace("{used}", String(used)).replace("{sel}", String(chosen.size))}
        {outside(shown) && <span> · {t("uv.outside")}</span>}
        {isEmpty(sel) && haveTargets && <span> · {t("uv.scope")}</span>}
      </div>
    </div>
  );
}

/** What the panel works on: the selected faces, groups and placed components. */
function targetsOf(sel: Sel): number[] {
  return [...sel.faces, ...sel.groups, ...sel.instances];
}
