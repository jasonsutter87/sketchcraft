// 3D Text: type, pick a font (any font the system has), a height and a depth.
// The browser draws the text; its outlines are traced (marching squares, then
// simplified) and the engine makes faces with holes, pulled to the depth.

import { useEffect, useRef, useState } from "react";
import { type Reply, type Scene } from "./engine";
import type { Cam } from "./Viewport";
import { formatLength, parseLength } from "./units";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;
type Pt = [number, number];

const FONTS = ["Helvetica", "Arial Black", "Georgia", "Futura", "Avenir Next", "Courier New", "Times New Roman", "Impact"];

/** Trace the ink of a canvas into closed outlines (pixel units, y up). */
export function traceText(text: string, font: string, px: number): { rings: Pt[][]; cap: number } {
  const c = document.createElement("canvas");
  const ctx = c.getContext("2d")!;
  ctx.font = `${px}px "${font}"`;
  const lines = text.split("\n");
  const w = Math.ceil(Math.max(...lines.map((l) => ctx.measureText(l).width))) + 8;
  const lineH = Math.ceil(px * 1.25);
  const h = lineH * lines.length + 8;
  c.width = w;
  c.height = h;
  ctx.font = `${px}px "${font}"`;
  ctx.fillStyle = "#000";
  ctx.textBaseline = "alphabetic";
  lines.forEach((l, i) => ctx.fillText(l, 4, 4 + px + i * lineH));
  const cap = ctx.measureText("H").actualBoundingBoxAscent || px * 0.7;
  const data = ctx.getImageData(0, 0, w, h).data;
  const ink = (x: number, y: number) => x >= 0 && y >= 0 && x < w && y < h && data[(y * w + x) * 4 + 3] > 127;
  // marching squares over pixel corners: edges between inside and outside cells,
  // directed so ink is on the left; chained into loops
  const next = new Map<string, Pt[]>();
  const key = (p: Pt) => `${p[0]},${p[1]}`;
  const add = (a: Pt, b: Pt) => {
    const k = key(a);
    const l = next.get(k);
    if (l) l.push(b);
    else next.set(k, [b]);
  };
  for (let y = -1; y < h; y++)
    for (let x = -1; x < w; x++) {
      // cell between pixel centers (x,y) (x+1,y) (x+1,y+1) (x,y+1); walk its border pixels' edges instead:
      // each ink pixel contributes the sides that face a non-ink pixel
      if (!ink(x, y)) continue;
      if (!ink(x, y - 1)) add([x + 1, y], [x, y]); // top side, leftward
      if (!ink(x - 1, y)) add([x, y], [x, y + 1]); // left side, down
      if (!ink(x, y + 1)) add([x, y + 1], [x + 1, y + 1]); // bottom, rightward
      if (!ink(x + 1, y)) add([x + 1, y + 1], [x + 1, y]); // right, up
    }
  const rings: Pt[][] = [];
  for (const [k0, outs] of next) {
    while (outs.length) {
      const start = k0.split(",").map(Number) as Pt;
      const ring: Pt[] = [start];
      let cur = outs.pop()!;
      let prev = start;
      let guard = 0;
      while (key(cur) !== k0 && guard++ < 1e6) {
        ring.push(cur);
        const o = next.get(key(cur));
        if (!o?.length) break;
        // at a pinch (two ways on), turn the same way every time so loops stay simple
        let pickI = 0;
        if (o.length > 1) {
          const din = [cur[0] - prev[0], cur[1] - prev[1]];
          pickI = o.findIndex((q) => din[0] * (q[1] - cur[1]) - din[1] * (q[0] - cur[0]) > 0);
          if (pickI < 0) pickI = 0;
        }
        prev = cur;
        cur = o.splice(pickI, 1)[0];
      }
      if (ring.length >= 4) rings.push(ring);
    }
  }
  // pixel stairs → straight runs → simplified curves; flip y so up is up
  const out = rings.map((r) => simplify(corners(r), 0.7).map(([x, y]) => [x, h - y] as Pt)).filter((r) => r.length >= 3);
  return { rings: out, cap };
}

/** Drop points in the middle of straight runs. */
function corners(r: Pt[]): Pt[] {
  return r.filter((p, i) => {
    const a = r[(i - 1 + r.length) % r.length], b = r[(i + 1) % r.length];
    return (p[0] - a[0]) * (b[1] - p[1]) - (p[1] - a[1]) * (b[0] - p[0]) !== 0;
  });
}

/** Douglas–Peucker on a closed ring. */
function simplify(r: Pt[], tol: number): Pt[] {
  if (r.length < 4) return r;
  // midpoints of pixel steps make smoother curves than the stair corners
  const mids: Pt[] = r.map((p, i) => {
    const q = r[(i + 1) % r.length];
    return [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
  });
  const far = mids.reduce((best, p, i) => ((p[0] - mids[0][0]) ** 2 + (p[1] - mids[0][1]) ** 2 > (mids[best][0] - mids[0][0]) ** 2 + (mids[best][1] - mids[0][1]) ** 2 ? i : best), 0);
  const dp = (pts: Pt[]): Pt[] => {
    if (pts.length < 3) return pts;
    const [a, b] = [pts[0], pts[pts.length - 1]];
    const L = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1e-9;
    let worst = 0, at = 0;
    for (let i = 1; i < pts.length - 1; i++) {
      const d = Math.abs((b[0] - a[0]) * (a[1] - pts[i][1]) - (a[0] - pts[i][0]) * (b[1] - a[1])) / L;
      if (d > worst) (worst = d), (at = i);
    }
    if (worst <= tol) return [a, b];
    return [...dp(pts.slice(0, at + 1)).slice(0, -1), ...dp(pts.slice(at))];
  };
  const one = dp(mids.slice(0, far + 1));
  const two = dp([...mids.slice(far), mids[0]]);
  return [...one.slice(0, -1), ...two.slice(0, -1)];
}

export function Text3dDialog({ scene, run, camera, onClose, onToast }: { scene: Scene; run: Run; camera: () => Cam; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const units = scene.units || "m";
  const imperial = units === "ft" || units === "in";
  const [text, setText] = useState("Enter text");
  const [font, setFont] = useState(FONTS[0]);
  const [height, setHeight] = useState(imperial ? `1'` : "0.3");
  const [depth, setDepth] = useState(imperial ? `2"` : "0.05");
  const [flat, setFlat] = useState(false);
  const [busy, setBusy] = useState(false);
  const preview = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = preview.current;
    if (!c) return;
    const ctx = c.getContext("2d")!;
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.font = `44px "${font}"`;
    ctx.fillStyle = getComputedStyle(c).color;
    ctx.textBaseline = "middle";
    ctx.fillText(text.split("\n")[0], 8, c.height / 2, c.width - 16);
  }, [text, font]);
  const place = async () => {
    const h = parseLength(height, units);
    const d = depth.trim() ? parseLength(depth, units) : 0;
    if (!(h > 0) || !Number.isFinite(d)) return onToast("Give a height (and depth) in any units", true);
    if (!text.trim()) return;
    setBusy(true);
    try {
      const { rings, cap } = traceText(text, font, 160);
      const k = h / cap; // pixels → meters: the capital H is `height` tall
      const minX = Math.min(...rings.flat().map((p) => p[0]));
      const minY = Math.min(...rings.flat().map((p) => p[1]));
      const scaled = rings.map((r) => r.map(([x, y]) => [+((x - minX) * k).toFixed(5), +((y - minY) * k).toFixed(5)]));
      const width = Math.max(...scaled.flat().map((p) => p[0]));
      // in front of you, facing you: on the ground where you're looking
      const c = camera();
      const dir = [c.target[0] - c.eye[0], c.target[1] - c.eye[1]];
      const L = Math.hypot(dir[0], dir[1]) || 1;
      const fwd = [dir[0] / L, dir[1] / L];
      const right = [fwd[1], -fwd[0], 0];
      const up = flat ? [fwd[0], fwd[1], 0] : [0, 0, 1];
      const at = [c.target[0] - (right[0] * width) / 2, c.target[1] - (right[1] * width) / 2, flat ? 0 : Math.max(0, c.target[2] - h / 2)];
      const r = await run({ cmd: "text_3d", rings: scaled, at, right, up, depth: d, name: `3D Text: ${text.split("\n")[0].slice(0, 24)}` });
      if (r.ok) {
        onToast(`3D Text: ${formatLength(h, units)} tall${d ? `, ${formatLength(d, units)} deep` : ""}. It's a group: move it into place.`);
        onClose();
      } else onToast(String(r.error), true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" style={{ width: "min(480px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="3D Text">
        <div className="sheet-head">
          <b>3D Text</b>
          <span className="muted">Solid letters in any font on your computer.</span>
        </div>
        <label className="field">
          <span>Text</span>
          <textarea rows={2} value={text} onChange={(e) => setText(e.target.value)} autoFocus onFocus={(e) => e.target.select()} />
        </label>
        <canvas ref={preview} width={440} height={64} className="text3d-preview" aria-hidden />
        <label className="field">
          <span>Font</span>
          <select value={font} onChange={(e) => setFont(e.target.value)}>
            {FONTS.map((f) => (
              <option key={f} value={f} style={{ fontFamily: f }}>
                {f}
              </option>
            ))}
          </select>
        </label>
        <div className="row gap">
          <label className="field" style={{ flex: 1 }}>
            <span>Height (a capital letter)</span>
            <input className="num" value={height} onChange={(e) => setHeight(e.target.value)} />
          </label>
          <label className="field" style={{ flex: 1 }}>
            <span>Depth (blank or 0: flat letters)</span>
            <input className="num" value={depth} onChange={(e) => setDepth(e.target.value)} />
          </label>
        </div>
        <label className="check">
          <input type="checkbox" checked={flat} onChange={(e) => setFlat(e.target.checked)} /> Lying flat (on the ground, like a sign painted on a road)
        </label>
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="btn small" disabled={busy || !text.trim()} onClick={place}>
            {busy ? "Making…" : "Place"}
          </button>
          <span style={{ flex: 1 }} />
          <button className="btn small ghost" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
