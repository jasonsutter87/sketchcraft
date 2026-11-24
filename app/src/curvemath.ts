// The Curve tool's preview: the same smooth curve the engine's `spline` command draws (core/src/curves.rs, centripetal
// Catmull-Rom through the clicked points), so what you see while you click is what you get. Pure numbers, no three.js.

export type P3 = [number, number, number];

/** What the Curve tool uses until a number is typed (pieces between each two clicked points). */
export const DEFAULT_SPLINE_SEGMENTS = 12;
/** The engine's cap on pieces per span; typed numbers are held to it. */
export const MAX_SPAN_SEGMENTS = 256;

const SAME = 1e-9;
const dist = (a: P3, b: P3) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const mix = (a: P3, b: P3, ta: number, tb: number): P3 => [a[0] * ta + b[0] * tb, a[1] * ta + b[1] * tb, a[2] * ta + b[2] * tb];

/** A typed segment count, as the engine will take it (whole number, 1 to 256); null when it is not one. */
export function segmentCount(raw: number): number | null {
  if (!Number.isFinite(raw)) return null;
  return Math.min(MAX_SPAN_SEGMENTS, Math.max(1, Math.round(raw)));
}

/** Points along the curve through `points`: `segments` pieces a span, the clicked points exactly. `closed` ends on the first point. */
export function splinePoints(points: P3[], closed: boolean, segments: number): P3[] {
  const pts: P3[] = [];
  for (const p of points) if (p.every(Number.isFinite) && (!pts.length || dist(pts[pts.length - 1], p) > SAME)) pts.push(p);
  if (closed && pts.length > 1 && dist(pts[0], pts[pts.length - 1]) <= SAME) pts.pop();
  const n = pts.length;
  if (n < 2) return pts.slice();
  if (closed && n < 3) return pts.slice();
  const k = Math.min(MAX_SPAN_SEGMENTS, Math.max(1, Math.round(segments)));
  const spans = closed ? n : n - 1;
  const at = (i: number): P3 => {
    if (closed) return pts[((i % n) + n) % n];
    if (i < 0) return [pts[0][0] * 2 - pts[1][0], pts[0][1] * 2 - pts[1][1], pts[0][2] * 2 - pts[1][2]];
    if (i >= n) return [pts[n - 1][0] * 2 - pts[n - 2][0], pts[n - 1][1] * 2 - pts[n - 2][1], pts[n - 1][2] * 2 - pts[n - 2][2]];
    return pts[i];
  };
  const knot = (a: P3, b: P3, t: number) => t + Math.max(Math.sqrt(dist(a, b)), 1e-6);
  const out: P3[] = [pts[0]];
  for (let s = 0; s < spans; s++) {
    const [p0, p1, p2, p3] = [at(s - 1), at(s), at(s + 1), at(s + 2)];
    const t0 = 0;
    const t1 = knot(p0, p1, t0);
    const t2 = knot(p1, p2, t1);
    const t3 = knot(p2, p3, t2);
    for (let j = 1; j <= k; j++) {
      const t = t1 + ((t2 - t1) * j) / k;
      const a1 = mix(p0, p1, (t1 - t) / (t1 - t0), (t - t0) / (t1 - t0));
      const a2 = mix(p1, p2, (t2 - t) / (t2 - t1), (t - t1) / (t2 - t1));
      const a3 = mix(p2, p3, (t3 - t) / (t3 - t2), (t - t2) / (t3 - t2));
      const b1 = mix(a1, a2, (t2 - t) / (t2 - t0), (t - t0) / (t2 - t0));
      const b2 = mix(a2, a3, (t3 - t) / (t3 - t1), (t - t1) / (t3 - t1));
      out.push(mix(b1, b2, (t2 - t) / (t2 - t1), (t - t1) / (t2 - t1)));
    }
    out[out.length - 1] = p2;
  }
  return out;
}
