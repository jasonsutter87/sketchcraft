// The paper view's camera: where a sheet sits on the screen. Pure, so it is unit tested (app/tests/paper.test.ts).
//
// A view is a zoom `k` (screen pixels per paper millimetre) and an offset: a point at paper millimetres (x, y),
// y down from the sheet's top left, is drawn at screen (ox + x·k, oy + y·k) inside the paper view's box.

export interface PaperCam {
  k: number;
  ox: number;
  oy: number;
}

/** How far in and out you can zoom: about a whole A0 in a small window, up to a 1:1 detail of 0.1 mm. */
export const MIN_K = 0.05;
export const MAX_K = 80;

export const clampK = (k: number) => Math.min(MAX_K, Math.max(MIN_K, Number.isFinite(k) ? k : 1));

export const toScreen = (c: PaperCam, p: [number, number]): [number, number] => [c.ox + p[0] * c.k, c.oy + p[1] * c.k];
export const toPaper = (c: PaperCam, s: [number, number]): [number, number] => [(s[0] - c.ox) / c.k, (s[1] - c.oy) / c.k];

/** The whole sheet (w x h mm) in a box of bw x bh pixels, centred, with `margin` pixels clear all round. */
export function fit(w: number, h: number, bw: number, bh: number, margin = 32): PaperCam {
  const aw = Math.max(1, bw - 2 * margin);
  const ah = Math.max(1, bh - 2 * margin);
  const k = clampK(Math.min(aw / Math.max(w, 1e-6), ah / Math.max(h, 1e-6)));
  return { k, ox: (bw - w * k) / 2, oy: (bh - h * k) / 2 };
}

/** Zoom by `factor` keeping the paper point under the screen point `at` where it is (a scroll at the cursor). */
export function zoomAt(c: PaperCam, at: [number, number], factor: number): PaperCam {
  const k = clampK(c.k * factor);
  const f = k / c.k;
  return { k, ox: at[0] - (at[0] - c.ox) * f, oy: at[1] - (at[1] - c.oy) * f };
}

export const panBy = (c: PaperCam, dx: number, dy: number): PaperCam => ({ ...c, ox: c.ox + dx, oy: c.oy + dy });

/**
 * The zoom a cached picture is drawn at: k rounded down to a step of √2, so zooming draws the sheet again only every
 * so often (in between, the cached tiles are scaled), and every tile of one step lines up.
 */
export function zoomStep(k: number): number {
  const s = Math.floor(Math.log2(clampK(k)) * 2 + 1e-9) / 2;
  return 2 ** s;
}

/** The zoom step at or just above k: tiles drawn there are only ever shrunk on screen (by at most √2), so lines stay crisp. */
export function zoomStepUp(k: number): number {
  const s = Math.ceil(Math.log2(clampK(k)) * 2 - 1e-9) / 2;
  return 2 ** s;
}

/** The tiles (`size` pixels square at zoom `k`) that cover the screen box, as [column, row] of the paper at that zoom. */
export function tilesFor(c: PaperCam, k: number, bw: number, bh: number, w: number, h: number, size = 512): [number, number][] {
  const s = k / c.k; // tile pixels per screen pixel
  // the screen box in tile-space pixels, clipped to the sheet
  const x0 = Math.max(0, -c.ox * s);
  const y0 = Math.max(0, -c.oy * s);
  const x1 = Math.min(w * k, (bw - c.ox) * s);
  const y1 = Math.min(h * k, (bh - c.oy) * s);
  const out: [number, number][] = [];
  if (x1 <= x0 || y1 <= y0) return out;
  for (let ty = Math.floor(y0 / size); ty * size < y1; ty++) for (let tx = Math.floor(x0 / size); tx * size < x1; tx++) out.push([tx, ty]);
  return out;
}

/** The percentage shown for a zoom: 100% is the paper at its real size on a 96 dpi screen. */
export const zoomPercent = (k: number) => Math.round((k / (96 / 25.4)) * 100);

/** Which rect (paper mm, [x, y, w, h]) is under a paper point, topmost (last) first; -1 when none. */
export function hitRect(rects: [number, number, number, number][], p: [number, number]): number {
  for (let i = rects.length - 1; i >= 0; i--) {
    const [x, y, w, h] = rects[i];
    if (p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h) return i;
  }
  return -1;
}
