// Position Texture: the picture on a face shown whole and see-through, with four pins on its corners. Drag a pin (or the
// picture itself) and the face follows live; Enter keeps it, Esc takes it back. The pin math is positiontexture.ts; the
// engine's `texture_position` does the rewrite. Each change takes the one before it back first (`undo`), so a whole
// session is one undo step however long it dragged.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ContextMenu, type MenuItem } from "./contextmenu";
import { exec, type Scene } from "./engine";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { after, dragDelta, hitPlane, homography, IDENT, mapPins, opDelta, pictureMatrix, pinsFor, planeOf, toWorld, type Aff, type Frame, type MenuOp, type PinKind, type Pins, type Plane, type V2, type V3 } from "./positiontexture";
import type { ViewportApi } from "./Viewport";

interface Base {
  frame: Frame;
  plane: Plane;
  pins: Pins;
}

const PIN_COLOR: Record<"red" | "green" | "blue" | "yellow", string> = { red: "#e5383b", green: "#2fa84f", blue: "#2f6fe5", yellow: "#f2c200" };

async function readBase(face: number, fresh: boolean): Promise<Base | string> {
  const r = await exec({ cmd: "texture_frame", face, default: fresh });
  if (!r.ok) return r.error ?? "no picture";
  const frame: Frame = { tile: r.tile as V2, normal: r.normal as V3, points: r.points as V3[][], picture: r.picture as V2[][] };
  const plane = planeOf(frame);
  const pins = plane && pinsFor(plane, frame);
  return plane && pins ? { frame, plane, pins } : "the face is too thin to position";
}

const isIdent = (d: Aff) => d.every((v, i) => Math.abs(v - IDENT[i]) < 1e-9);

export function PositionPanel({ scene, face, run, api, onClose, onToast }: { scene: Scene; face: number; run: Run; api: ViewportApi; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const [base, setBase] = useState<Base | null>(null);
  const [d, setD] = useState<Aff>(IDENT);
  const [, setTick] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const resetFirst = useRef(false);
  const dRef = useRef<Aff>(IDENT);
  const baseRef = useRef<Base | null>(null);
  baseRef.current = base;
  const apiRef = useRef(api);
  apiRef.current = api;
  const drag = useRef<{ kind: PinKind; from: V2; pins: Pins; d0: Aff } | null>(null);
  const finished = useRef(false);
  const menuOpen = useRef(false);

  const tex = scene.materials.find((m) => m.name === scene.faces.find((f) => f.id === face)?.material)?.texture?.data;

  // live preview: the newest change wins; each try takes the one before back
  const applied = useRef(false);
  const pending = useRef<Record<string, unknown> | "none" | null>(null);
  const working = useRef<Promise<void> | null>(null);
  const pump = useCallback(() => {
    if (working.current) return working.current;
    working.current = (async () => {
      try {
        while (pending.current) {
          const c = pending.current;
          pending.current = null;
          if (applied.current) {
            await run({ cmd: "undo" });
            applied.current = false;
          }
          if (c !== "none") {
            const r = await run(c);
            applied.current = r.ok;
            if (!r.ok) onToast(t("pos.cantpin").replace("{err}", r.error ?? ""), true);
          }
        }
      } finally {
        working.current = null;
      }
    })();
    return working.current;
  }, [run, onToast, t]);

  const push = useCallback(
    (next: Aff) => {
      dRef.current = next;
      setD(next);
      const b = baseRef.current;
      if (!b) return;
      if (isIdent(next)) {
        pending.current = resetFirst.current ? { cmd: "texture_position", face, reset: true } : "none";
      } else {
        const m = pictureMatrix(b.plane, next);
        if (!m) return;
        pending.current = { cmd: "texture_position", face, matrix: m, ...(resetFirst.current ? { reset: true } : {}) };
      }
      void pump();
    },
    [face, pump]
  );

  useEffect(() => {
    let live = true;
    void readBase(face, false).then((b) => {
      if (!live) return;
      if (typeof b === "string") {
        onToast(t("pos.cantpin").replace("{err}", b), true);
        onClose();
      } else setBase(b);
    });
    return () => {
      live = false;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const redraw = () => setTick((n) => n + 1);
    const stop = api.watchCamera(redraw);
    window.addEventListener("resize", redraw);
    return () => {
      stop();
      window.removeEventListener("resize", redraw);
    };
  }, [api]);

  const finish = useCallback(
    async (keep: boolean) => {
      if (finished.current) return;
      finished.current = true;
      await working.current;
      if (!keep && applied.current) {
        await run({ cmd: "undo" });
        applied.current = false;
      }
      onClose();
    },
    [run, onClose]
  );

  // Enter keeps it, Esc takes it back; a click on the empty model keeps it too
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return;
      if ((e.key !== "Enter" && e.key !== "Escape") || menuOpen.current) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      void finish(e.key === "Enter");
    };
    let down: [number, number] | null = null;
    const pd = (e: PointerEvent) => (down = (e.target as HTMLElement).tagName === "CANVAS" && e.button === 0 ? [e.clientX, e.clientY] : null);
    const pu = (e: PointerEvent) => {
      if (down && Math.hypot(e.clientX - down[0], e.clientY - down[1]) < 4 && (e.target as HTMLElement).tagName === "CANVAS") void finish(true);
      down = null;
    };
    window.addEventListener("keydown", key, true);
    window.addEventListener("pointerdown", pd, true);
    window.addEventListener("pointerup", pu, true);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("pointerdown", pd, true);
      window.removeEventListener("pointerup", pu, true);
    };
  }, [finish]);

  menuOpen.current = menu != null;
  const pins = useMemo(() => (base ? mapPins(d, base.pins) : null), [base, d]);
  if (!base || !pins) return null;
  const toScreen = (s: V2) => api.toClient(toWorld(base.plane, s));
  const sc = { r: toScreen(pins.r), g: toScreen(pins.g), b: toScreen(pins.b), y: toScreen(pins.y) };
  const all = sc.r && sc.g && sc.b && sc.y ? { r: sc.r, g: sc.g, b: sc.b, y: sc.y } : null;
  const H = all ? homography(100, 100, [all.b, all.y, all.g, all.r]) : null;

  const at = (e: { clientX: number; clientY: number }): V2 | null => {
    const ray = api.rayAt(e.clientX, e.clientY);
    return hitPlane(base.plane, ray.origin, ray.dir);
  };
  const start = (kind: PinKind) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const s = at(e);
    if (!s) return;
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag.current = { kind, from: s, pins, d0: dRef.current };
  };
  const move = (e: React.PointerEvent) => {
    const g = drag.current;
    if (!g) return;
    const s = at(e);
    if (s) push(after(dragDelta(g.kind, g.pins, g.from, s), g.d0));
  };
  const end = () => (drag.current = null);
  const context = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY });
  };

  const op = (o: MenuOp) => () => {
    const m = opDelta(o, pins);
    if (m) push(after(m, dRef.current));
  };
  const reset = async () => {
    const b = await readBase(face, true);
    if (typeof b === "string") return onToast(t("pos.cantpin").replace("{err}", b), true);
    resetFirst.current = true;
    setBase(b);
    baseRef.current = b;
    push(IDENT);
  };
  const items: MenuItem[] = [
    { label: t("pos.reset"), run: () => void reset() },
    { label: t("pos.flipH"), run: op("flipH") },
    { label: t("pos.flipV"), run: op("flipV") },
    { label: t("pos.rot90"), run: op("rot90") },
    { label: t("pos.rot180"), run: op("rot180") },
    { label: t("pos.rot270"), run: op("rot270") },
    { label: t("pos.done"), key: "Enter", run: () => void finish(true) },
  ];

  const pinOf = (k: "red" | "green" | "blue" | "yellow", p: [number, number] | null) =>
    p && (
      <g key={k} className={`pos-pin pos-${k}`} transform={`translate(${p[0]} ${p[1]})`} onPointerDown={start(k)} onPointerMove={move} onPointerUp={end} onContextMenu={context}>
        <title>{t(`pos.pin.${k}`)}</title>
        <circle r={11} fill={PIN_COLOR[k]} stroke="#fff" strokeWidth={2.5} data-testid={`pos-pin-${k}`} />
        <text x={16} y={4} className="pos-label">
          {t(`pos.label.${k}`)}
        </text>
      </g>
    );
  const outline = all ? `${all.r.join(",")} ${all.g.join(",")} ${all.y.join(",")} ${all.b.join(",")}` : "";

  return (
    <div className="pos-overlay" data-testid="pos-overlay">
      {H && tex && (
        <div className="pos-image" style={{ transform: `matrix3d(${H.join(",")})` }}>
          <img src={tex} alt="" draggable={false} />
        </div>
      )}
      <svg className="pos-svg">
        {all && <polygon className="pos-quad" points={outline} onPointerDown={start("move")} onPointerMove={move} onPointerUp={end} onContextMenu={context} data-testid="pos-quad" />}
        {pinOf("red", sc.r)}
        {pinOf("green", sc.g)}
        {pinOf("blue", sc.b)}
        {pinOf("yellow", sc.y)}
      </svg>
      <div className="pos-bar" role="status">
        <strong>{t("pos.title")}</strong>
        <span>{t("pos.hint")}</span>
        <button onClick={() => void finish(true)} data-testid="pos-done">
          {t("pos.done")}
        </button>
        <button onClick={() => void finish(false)} data-testid="pos-cancel">
          {t("pos.cancel")}
        </button>
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} close={() => setMenu(null)} items={items} />}
    </div>
  );
}
