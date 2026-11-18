import type { Scene } from "../engine";

/** Screen pixels per paper millimetre of pen: 0.25 mm (the default pen) reads as 1.5 px, 0.7 mm as 4 px. */
export const PX_PER_MM = 6;
/** A dash in the model view: paper mm to world-ish screen length (the dashes are in line-width space, see LineMaterial). */
const DASH_K = 0.12;

export interface PenBucket {
  /** the pen's width on screen, px */
  px: number;
  dash: number[];
  color: [number, number, number];
  /** segment endpoints, 6 numbers each */
  pos: number[];
}

/**
 * Splits the scene's edges by pen: `plain` is the default 1 px line work, `buckets` is one list of segments per
 * distinct tag or edge pen (the engine worked out which edge uses which pen: tag style, then the edge's own over it).
 * null when nothing has a pen of its own.
 */
export function edgePens(s: Pick<Scene, "edges" | "edgeStyle" | "edgeStyles">): { plain: number[]; buckets: PenBucket[] } | null {
  const idx = s.edgeStyle;
  const table = s.edgeStyles;
  if (!idx || !idx.length || !table || !table.length) return null;
  const plain: number[] = [];
  const by = new Map<number, number[]>();
  for (let i = 0; i < idx.length; i++) {
    const seg = s.edges.slice(i * 6, i * 6 + 6);
    const k = idx[i];
    if (!k || !table[k - 1]) {
      plain.push(...seg);
    } else {
      let list = by.get(k);
      if (!list) by.set(k, (list = []));
      list.push(...seg);
    }
  }
  const buckets: PenBucket[] = [...by.entries()].map(([k, pos]) => {
    const p = table[k - 1];
    return { px: Math.max(1, p.w * PX_PER_MM), dash: p.dash.length >= 2 ? [p.dash[0] * DASH_K, p.dash[1] * DASH_K] : [], color: p.color, pos };
  });
  return { plain, buckets };
}
