// Drawing a sheet's display list (the engine's `sheet_ir`, core/src/draft/irjson.rs) on a Canvas2D: paths as
// `Path2D` from their SVG path strings (the engine has joined them per pen), text, pictures, clipped groups at
// their opacity, and hatches clipped to their rings. Paper millimetres, y down; the caller sets the transform.

export interface IrPen {
  w: number;
  c: string;
  dash: number[];
}
export type IrItem =
  | { k: "p"; d: string; s?: IrPen; f?: { c: string; eo: boolean } }
  | { k: "t"; x: number; y: number; size: number; bold: boolean; c: string; a: number; text: string }
  | { k: "i"; rect: [number, number, number, number]; png: string | null }
  | { k: "g"; clip?: [number, number, number, number]; o: number; alt?: string; items: IrItem[] }
  | { k: "h"; rings: string; lines: string; s: IrPen; tint: string | null; name?: string };

export interface Ir {
  sheet: number;
  w: number;
  h: number;
  bg: string | null;
  title: string;
  items: IrItem[];
  truncated: boolean;
  bytes: number;
}

/** The engine's drawing font (Inter, from core/assets/fonts; see _paper.scss), so text is where the PDF puts it. */
export const PAPER_FONT = '"SketchCraft Paper", system-ui, sans-serif';

/** Parsed paths and decoded pictures, kept per display list (a redraw doesn't parse the strings again). */
export class IrCache {
  private paths = new Map<string, Path2D>();
  private images = new Map<string, HTMLImageElement>();
  /** called when a picture has finished loading (draw again) */
  onImage: () => void = () => {};
  path(d: string): Path2D {
    let p = this.paths.get(d);
    if (!p) {
      p = new Path2D(d);
      this.paths.set(d, p);
    }
    return p;
  }
  image(png: string): HTMLImageElement | null {
    let i = this.images.get(png);
    if (!i) {
      i = new Image();
      i.onload = () => this.onImage();
      i.src = `data:image/png;base64,${png}`;
      this.images.set(png, i);
    }
    return i.complete && i.naturalWidth ? i : null;
  }
}

/** How thin a pen may get on screen: a hairline stays visible when zoomed out. `k` is pixels per mm. */
const minWidth = (w: number, k: number) => Math.max(w, 0.6 / k);

function stroke(g: CanvasRenderingContext2D, p: Path2D, pen: IrPen, k: number) {
  g.strokeStyle = pen.c;
  g.lineWidth = minWidth(pen.w, k);
  g.setLineDash(pen.dash.length ? pen.dash : []);
  g.lineCap = pen.dash.length ? "butt" : "round";
  g.lineJoin = "round";
  g.stroke(p);
}

/** Draw the items; `k` is the current zoom in pixels per mm (for hairlines). */
export function drawItems(g: CanvasRenderingContext2D, items: IrItem[], cache: IrCache, k: number) {
  for (const it of items) {
    switch (it.k) {
      case "p": {
        const p = cache.path(it.d);
        if (it.f) {
          g.fillStyle = it.f.c;
          g.fill(p, it.f.eo ? "evenodd" : "nonzero");
        }
        if (it.s) stroke(g, p, it.s, k);
        break;
      }
      case "t": {
        g.save();
        g.translate(it.x, it.y);
        if (it.a) g.rotate(-it.a);
        g.fillStyle = it.c;
        g.font = `${it.bold ? 600 : 400} ${it.size}px ${PAPER_FONT}`;
        g.textBaseline = "alphabetic";
        g.fillText(it.text, 0, 0);
        g.restore();
        break;
      }
      case "i": {
        const img = it.png ? cache.image(it.png) : null;
        const [x, y, w, h] = it.rect;
        if (img) g.drawImage(img, x, y, w, h);
        else {
          // a picture past the size limit (or still loading): a light box where it goes
          g.fillStyle = "rgba(128,128,128,0.15)";
          g.fillRect(x, y, w, h);
        }
        break;
      }
      case "g": {
        g.save();
        if (it.clip) {
          g.beginPath();
          g.rect(...it.clip);
          g.clip();
        }
        if (it.o < 1) {
          // one layer at the group's opacity, so lines overlapping inside it don't darken each other (as the PDF draws it)
          const m = g.getTransform();
          const cw = g.canvas.width;
          const ch = g.canvas.height;
          const layer = typeof OffscreenCanvas !== "undefined" ? new OffscreenCanvas(cw, ch) : Object.assign(document.createElement("canvas"), { width: cw, height: ch });
          const lg = layer.getContext("2d") as CanvasRenderingContext2D | null;
          if (lg) {
            lg.setTransform(m);
            drawItems(lg, it.items, cache, k);
            g.save();
            g.setTransform(1, 0, 0, 1, 0, 0);
            g.globalAlpha *= Math.max(0, it.o);
            g.drawImage(layer as CanvasImageSource, 0, 0);
            g.restore();
          }
        } else drawItems(g, it.items, cache, k);
        g.restore();
        break;
      }
      case "h": {
        const rings = cache.path(it.rings);
        if (it.tint) {
          g.fillStyle = it.tint;
          g.fill(rings, "evenodd");
        } else if (it.lines) {
          g.save();
          g.clip(rings, "evenodd");
          stroke(g, cache.path(it.lines), { ...it.s, dash: [] }, k);
          g.restore();
        }
        break;
      }
    }
  }
}

/** The whole page: paper, then its items. */
export function drawPage(g: CanvasRenderingContext2D, ir: Ir, cache: IrCache, k: number) {
  g.fillStyle = ir.bg ?? "#ffffff";
  g.fillRect(0, 0, ir.w, ir.h);
  drawItems(g, ir.items, cache, k);
}
