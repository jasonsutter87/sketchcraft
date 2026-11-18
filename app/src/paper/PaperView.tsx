// The paper: a saved sheet drawn from the engine's display list on a Canvas2D, with pan and zoom, and an SVG overlay
// on top for what you can pick (the viewports, their frame and handles). The sheet is drawn into tiles cached per zoom
// step, so panning only copies pictures and zooming redraws only every √2; a changed display list starts over.
//
// Mouse: click a viewport to pick it (the inspector shows it), click the paper to pick the page; drag to pan (or
// middle-drag, or hold Space); scroll to zoom at the cursor (pinch on a trackpad; two-finger scroll pans).
// Keys, with the paper focused: + and - zoom, 0 fits the page, arrows pan.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { drawPage, IrCache, PAPER_FONT, type Ir } from "./draw";
import { scaleText, viewName, type ViewportItem } from "./pages";
import { fit, hitRect, panBy, tilesFor, toPaper, toScreen, zoomAt, zoomPercent, zoomStepUp, type PaperCam } from "./view";

const TILE = 512;
const MAX_TILES = 40;
/** most tiles drawn in one frame (the rest come on the next frames, so the view never stalls) */
const TILES_PER_FRAME = 6;

interface Props {
  ir: Ir | null;
  /** the sheet's paper, for the frame before its drawing arrives */
  paper: { w: number; h: number };
  viewports: ViewportItem[];
  picked: number | null;
  onPick: (viewport: number | null) => void;
  drawing: boolean;
  error: string;
  /** names the sheet for screen readers */
  title: string;
}

export function PaperView({ ir, paper, viewports, picked, onPick, drawing, error, title }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [cam, setCam] = useState<PaperCam | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const [panning, setPanning] = useState(false);
  const [space, setSpace] = useState(false);
  const camRef = useRef(cam);
  camRef.current = cam;
  const tiles = useRef(new Map<string, HTMLCanvasElement>());
  const cache = useRef(new IrCache());
  const frame = useRef(0);
  const [fontTick, setFontTick] = useState(0);

  // the box's size
  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // the paper's own font, then draw the text again
  useEffect(() => {
    let live = true;
    document.fonts?.load(`400 12px ${PAPER_FONT}`).then(() => document.fonts.load(`600 12px ${PAPER_FONT}`)).then(() => {
      if (!live) return;
      tiles.current.clear();
      setFontTick((t) => t + 1);
    }, () => {});
    return () => {
      live = false;
    };
  }, []);

  // a new page size or a new box: fit the page
  const pw = paper.w;
  const ph = paper.h;
  useEffect(() => {
    if (size.w > 0 && size.h > 0) setCam(fit(pw, ph, size.w, size.h));
    // only when the page's size or the box changes (not on every redraw)
  }, [pw, ph, size.w > 0 && size.h > 0]); // eslint-disable-line react-hooks/exhaustive-deps

  // a new drawing: the cached tiles are stale
  useEffect(() => {
    tiles.current.clear();
    cache.current = new IrCache();
  }, [ir]);

  // draw: the desk, the page's shadow, then the tiles of the page at this zoom step
  useEffect(() => {
    const cv = canvas.current;
    if (!cv || !cam || !size.w) return;
    const dpr = window.devicePixelRatio || 1;
    const draw = () => {
      frame.current = 0;
      const W = Math.round(size.w * dpr);
      const H = Math.round(size.h * dpr);
      if (cv.width !== W || cv.height !== H) {
        cv.width = W;
        cv.height = H;
      }
      const g = cv.getContext("2d");
      if (!g) return;
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.clearRect(0, 0, W, H);
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      const [x0, y0] = toScreen(cam, [0, 0]);
      const pwPx = pw * cam.k;
      const phPx = ph * cam.k;
      g.save();
      g.shadowColor = "rgba(0,0,0,0.28)";
      g.shadowBlur = 14;
      g.shadowOffsetY = 3;
      g.fillStyle = ir?.bg ?? "#ffffff";
      g.fillRect(x0, y0, pwPx, phPx);
      g.restore();
      if (!ir) return;
      const kt = zoomStepUp(cam.k) * dpr;
      const need = tilesFor(cam, kt, size.w, size.h, ir.w, ir.h, TILE);
      let made = 0;
      let missing = false;
      cache.current.onImage = () => {
        tiles.current.clear();
        if (!frame.current) frame.current = requestAnimationFrame(draw);
      };
      for (const [tx, ty] of need) {
        const key = `${kt}:${tx}:${ty}`;
        let t = tiles.current.get(key);
        if (!t) {
          if (made >= TILES_PER_FRAME) {
            missing = true;
            continue;
          }
          t = document.createElement("canvas");
          t.width = TILE;
          t.height = TILE;
          const tg = t.getContext("2d");
          if (!tg) continue;
          tg.setTransform(kt, 0, 0, kt, -tx * TILE, -ty * TILE);
          drawPage(tg, ir, cache.current, kt);
          tiles.current.set(key, t);
          made++;
          // least recently made goes first
          while (tiles.current.size > MAX_TILES) tiles.current.delete(tiles.current.keys().next().value!);
        } else {
          // recently used: to the back of the queue
          tiles.current.delete(key);
          tiles.current.set(key, t);
        }
        // each tile's edges on whole device pixels, shared with its neighbours: no seams between tiles
        const [ax, ay] = toScreen(cam, [(tx * TILE) / kt, (ty * TILE) / kt]);
        const [bx, by] = toScreen(cam, [((tx + 1) * TILE) / kt, ((ty + 1) * TILE) / kt]);
        const snap = (v: number) => Math.round(v * dpr) / dpr;
        g.drawImage(t, snap(ax), snap(ay), snap(bx) - snap(ax), snap(by) - snap(ay));
      }
      if (missing) frame.current = requestAnimationFrame(draw);
    };
    if (frame.current) cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(draw);
    return () => {
      if (frame.current) cancelAnimationFrame(frame.current);
      frame.current = 0;
    };
  }, [cam, ir, size, pw, ph, fontTick]);

  // ---- pan and zoom -------------------------------------------------------------------------------------------------
  const drag = useRef<{ x: number; y: number; moved: boolean; button: number } | null>(null);
  const local = (e: { clientX: number; clientY: number }): [number, number] => {
    const r = host.current!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const rects = viewports.map((v) => v.rect_mm);
  const vpAt = (s: [number, number]) => {
    if (!cam) return null;
    const i = hitRect(rects, toPaper(cam, s));
    return i < 0 ? null : viewports[i].id;
  };
  const zoomBy = (f: number, at?: [number, number]) => setCam((c) => (c ? zoomAt(c, at ?? [size.w / 2, size.h / 2], f) : c));
  const fitPage = () => size.w && setCam(fit(pw, ph, size.w, size.h));

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const at = local(e);
      // a pinch (ctrl) or a mouse wheel's notch zooms; a trackpad's two-finger scroll pans
      const notch = e.deltaMode !== 0 || (e.deltaX === 0 && Math.abs(e.deltaY) >= 50 && Number.isInteger(e.deltaY));
      if (e.ctrlKey || notch) setCam((c) => (c ? zoomAt(c, at, Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015))) : c));
      else setCam((c) => (c ? panBy(c, -e.deltaX, -e.deltaY) : c));
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  const cursor = panning ? "grabbing" : space ? "grab" : hover != null ? "pointer" : "grab";
  const hint = space ? "Drag to pan" : hover != null ? "Click to select this viewport" : "Click the paper to select the page · drag to pan · scroll to zoom";

  return (
    <div
      ref={host}
      className="paper-view"
      tabIndex={0}
      role="application"
      aria-roledescription="sheet"
      aria-label={`${title}. Plus and minus zoom, 0 fits the page, arrow keys pan.`}
      style={{ cursor }}
      onPointerDown={(e) => {
        host.current?.focus({ preventScroll: true });
        drag.current = { ...{ x: e.clientX, y: e.clientY }, moved: false, button: e.button };
        (e.target as Element).setPointerCapture?.(e.pointerId);
        if (e.button === 1 || space) setPanning(true);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (d) {
          const dx = e.clientX - d.x;
          const dy = e.clientY - d.y;
          if (!d.moved && Math.hypot(dx, dy) > 3) {
            d.moved = true;
            setPanning(true);
          }
          if (d.moved) {
            setCam((c) => (c ? panBy(c, dx, dy) : c));
            d.x = e.clientX;
            d.y = e.clientY;
          }
          return;
        }
        setHover(vpAt(local(e)));
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        setPanning(false);
        if (d && !d.moved && d.button === 0 && !space) onPick(vpAt(local(e)));
      }}
      onPointerLeave={() => setHover(null)}
      onKeyDown={(e) => {
        const step = 60;
        const k = e.key;
        let used = true;
        if (k === "+" || k === "=") zoomBy(1.25);
        else if (k === "-" || k === "_") zoomBy(0.8);
        else if (k === "0") fitPage();
        else if (k === "ArrowLeft") setCam((c) => (c ? panBy(c, step, 0) : c));
        else if (k === "ArrowRight") setCam((c) => (c ? panBy(c, -step, 0) : c));
        else if (k === "ArrowUp") setCam((c) => (c ? panBy(c, 0, step) : c));
        else if (k === "ArrowDown") setCam((c) => (c ? panBy(c, 0, -step) : c));
        else if (k === " ") setSpace(true);
        else if (k === "Escape" && picked != null) onPick(null);
        else used = false;
        if (used) {
          e.preventDefault();
          e.stopPropagation();
        }
      }}
      onKeyUp={(e) => e.key === " " && setSpace(false)}
      onBlur={() => setSpace(false)}
    >
      <canvas ref={canvas} className="paper-canvas" aria-hidden="true" />
      {cam && (
        <svg className="paper-overlay" width={size.w} height={size.h} aria-label="Viewports on this page">
          {/* the page's edge */}
          <rect className="paper-edge" x={cam.ox} y={cam.oy} width={pw * cam.k} height={ph * cam.k} />
          {viewports.map((v) => {
            const [x, y] = toScreen(cam, [v.rect_mm[0], v.rect_mm[1]]);
            const w = v.rect_mm[2] * cam.k;
            const h = v.rect_mm[3] * cam.k;
            const on = v.id === picked;
            const name = `${v.label || viewName(v)}, ${scaleText(v.scale)}${(v.opacity ?? 1) < 1 ? `, ${Math.round((v.opacity ?? 1) * 100)}% opacity` : ""}`;
            return (
              <g key={v.id} className={`vp ${on ? "on" : ""} ${hover === v.id ? "hover" : ""}`} data-viewport={v.id}>
                <rect
                  className="vp-frame"
                  x={x}
                  y={y}
                  width={w}
                  height={h}
                  role="button"
                  tabIndex={0}
                  aria-pressed={on}
                  aria-label={`Viewport: ${name}`}
                  onFocus={() => setHover(v.id)}
                  onBlur={() => setHover(null)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      e.stopPropagation();
                      onPick(on ? null : v.id);
                    }
                  }}
                >
                  <title>{`${name}. Click to select it.`}</title>
                </rect>
                {on &&
                  [
                    [x, y],
                    [x + w / 2, y],
                    [x + w, y],
                    [x + w, y + h / 2],
                    [x + w, y + h],
                    [x + w / 2, y + h],
                    [x, y + h],
                    [x, y + h / 2],
                  ].map(([hx, hy], i) => <rect key={i} className="vp-handle" x={hx - 4} y={hy - 4} width={8} height={8} aria-hidden="true" />)}
                {on && (
                  <text className="vp-tag" x={x + 6} y={Math.max(14, y - 6)} aria-hidden="true">
                    {name}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      )}
      <div className="paper-bar" role="toolbar" aria-label="Paper view" onPointerDown={(e) => e.stopPropagation()}>
        <button title="Zoom out (−)" aria-label="Zoom out" onClick={() => zoomBy(0.8)}>
          −
        </button>
        <span className="paper-zoom" aria-live="polite" title="100% is the paper's real size">
          {cam ? `${zoomPercent(cam.k)}%` : ""}
        </span>
        <button title="Zoom in (+)" aria-label="Zoom in" onClick={() => zoomBy(1.25)}>
          +
        </button>
        <button title="Fit the page in the window (0)" onClick={fitPage}>
          Fit
        </button>
        <span className="paper-hint">{hint}</span>
      </div>
      {(drawing || error || ir?.truncated) && (
        <div className={`paper-status ${error ? "bad" : ""}`} role="status">
          {error ? error : drawing && !ir ? "Drawing the page…" : drawing ? "Updating…" : "This page is very large: only part of it is drawn here (the PDF has all of it)."}
        </div>
      )}
    </div>
  );
}
