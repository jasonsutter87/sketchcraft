// Position Texture, the app's side: the pin math (pure, so Vitest covers it). The overlay is positionpanel.tsx; the
// engine does the rewrite (core/src/texposition.rs, command `texture_position`).
//
// Two spaces. "Face space" (s) is plain metres on the face's plane: orthonormal axes e1, e2 picked on the face. "Picture
// space" is the engine's: the face's texture coordinates times the tile size, in metres. Each corner of the face has
// both, and the map between them is affine (L): picture = L(s). The picture's tile (one whole image) is the unit square
// in picture space, so in face space it is the square pulled back through L. The four pins sit on its corners.
//
// A drag changes the picture's lay in face space by an affine D (the picture's content moves by D). The engine wants the
// same change in picture space: A = L . D^-1 . L^-1, applied to every corner's coordinates.

export type V2 = [number, number];
export type V3 = [number, number, number];
/** p' = [a c; b d] p + [e f] */
export type Aff = [number, number, number, number, number, number];

export const IDENT: Aff = [1, 0, 0, 1, 0, 0];

export const apply = (m: Aff, p: V2): V2 => [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
export const det = (m: Aff) => m[0] * m[3] - m[1] * m[2];

/** `m` after `first` */
export function after(m: Aff, first: Aff): Aff {
  const [a, b, c, d, e, f] = m;
  const [a2, b2, c2, d2, e2, f2] = first;
  return [a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f];
}

export function invert(m: Aff): Aff | null {
  const d = det(m);
  if (!Number.isFinite(d) || Math.abs(d) < 1e-12) return null;
  const [a, b, c, e, f] = [m[0], m[1], m[2], m[4], m[5]];
  const dd = m[3];
  const ia = dd / d, ib = -b / d, ic = -c / d, id = a / d;
  return [ia, ib, ic, id, -(ia * e + ic * f), -(ib * e + id * f)];
}

/** the affine map that sends the three points `from` to the three points `to` (null: `from` is a line) */
export function fromTriangles(from: [V2, V2, V2], to: [V2, V2, V2]): Aff | null {
  const m = (t: [V2, V2, V2]): Aff => [t[1][0] - t[0][0], t[1][1] - t[0][1], t[2][0] - t[0][0], t[2][1] - t[0][1], t[0][0], t[0][1]];
  const inv = invert(m(from));
  return inv ? after(m(to), inv) : null;
}

/** turn by `deg` degrees about `c` */
export function turnAbout(c: V2, deg: number): Aff {
  const r = (deg * Math.PI) / 180;
  const [s, k] = [Math.sin(r), Math.cos(r)];
  return [k, s, -s, k, c[0] - (k * c[0] - s * c[1]), c[1] - (s * c[0] + k * c[1])];
}

const sub = (a: V2, b: V2): V2 => [a[0] - b[0], a[1] - b[1]];
const add = (a: V2, b: V2): V2 => [a[0] + b[0], a[1] + b[1]];
const len = (a: V2) => Math.hypot(a[0], a[1]);

export interface Frame {
  /** tile width and height in metres */
  tile: V2;
  normal: V3;
  /** per loop (outer first) the corners' points */
  points: V3[][];
  /** and their picture-space coordinates */
  picture: V2[][];
}

export interface Plane {
  origin: V3;
  e1: V3;
  e2: V3;
  /** picture = L(s) */
  L: Aff;
}

const dot3 = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross3 = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm3 = (a: V3): V3 => {
  const l = Math.hypot(...a) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

/** the face's plane axes and the map from them to picture space; null when the corners don't fix one (all on a line) */
export function planeOf(f: Frame): Plane | null {
  const pts = f.points[0] ?? [];
  const pic = f.picture[0] ?? [];
  if (pts.length < 3 || pic.length !== pts.length) return null;
  const n = norm3(f.normal);
  const up: V3 = Math.abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
  const e1 = norm3(cross3(up, n));
  const e2 = cross3(n, e1);
  const origin = pts[0];
  const s: V2[] = pts.map((p) => [dot3([p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]], e1), dot3([p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]], e2)]);
  // the three corners spanning the biggest triangle give the map
  let best: [number, number, number] | null = null;
  let area = 1e-12;
  for (let i = 0; i < s.length; i++)
    for (let j = i + 1; j < s.length; j++)
      for (let k = j + 1; k < s.length; k++) {
        const a = Math.abs((s[j][0] - s[i][0]) * (s[k][1] - s[i][1]) - (s[j][1] - s[i][1]) * (s[k][0] - s[i][0]));
        if (a > area) (area = a), (best = [i, j, k]);
      }
  if (!best) return null;
  const L = fromTriangles([s[best[0]], s[best[1]], s[best[2]]], [pic[best[0]], pic[best[1]], pic[best[2]]]);
  return L ? { origin, e1, e2, L } : null;
}

export const toFace = (pl: Plane, p: V3): V2 => {
  const q: V3 = [p[0] - pl.origin[0], p[1] - pl.origin[1], p[2] - pl.origin[2]];
  return [dot3(q, pl.e1), dot3(q, pl.e2)];
};

export const toWorld = (pl: Plane, s: V2): V3 => [
  pl.origin[0] + pl.e1[0] * s[0] + pl.e2[0] * s[1],
  pl.origin[1] + pl.e1[1] * s[0] + pl.e2[1] * s[1],
  pl.origin[2] + pl.e1[2] * s[0] + pl.e2[2] * s[1],
];

/** where a ray meets the face's plane, as a point on it (null: parallel or behind) */
export function hitPlane(pl: Plane, origin: V3, dir: V3): V2 | null {
  const n = cross3(pl.e1, pl.e2);
  const den = dot3(dir, n);
  if (Math.abs(den) < 1e-9) return null;
  const t = dot3([pl.origin[0] - origin[0], pl.origin[1] - origin[1], pl.origin[2] - origin[2]], n) / den;
  if (!(t > 0)) return null;
  return toFace(pl, [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t]);
}

/** the four pins, in face space: red (the picture's corner), green (along its width), blue (up its height), yellow (the far corner) */
export interface Pins {
  r: V2;
  g: V2;
  b: V2;
  y: V2;
}

/** the whole picture (one tile) that lies under the middle of the face, with the pins on its corners */
export function pinsFor(pl: Plane, f: Frame): Pins | null {
  const inv = invert(pl.L);
  if (!inv) return null;
  const flat = f.picture.flat();
  if (!flat.length) return null;
  const mid = flat.reduce<V2>((a, p) => [a[0] + p[0] / flat.length, a[1] + p[1] / flat.length], [0, 0]);
  const [w, h] = f.tile;
  const [x0, y0] = [Math.floor(mid[0] / w) * w, Math.floor(mid[1] / h) * h];
  const at = (x: number, y: number) => apply(inv, [x, y]);
  return { r: at(x0, y0), g: at(x0 + w, y0), b: at(x0, y0 + h), y: at(x0 + w, y0 + h) };
}

export const mapPins = (d: Aff, p: Pins): Pins => ({ r: apply(d, p.r), g: apply(d, p.g), b: apply(d, p.b), y: apply(d, p.y) });

export type PinKind = "red" | "green" | "blue" | "yellow" | "move";

/** linear map `lin` (2 x 2 as an Aff with no shift) about the point `c` */
function about(c: V2, lin: Aff): Aff {
  return after([1, 0, 0, 1, c[0], c[1]], after(lin, [1, 0, 0, 1, -c[0], -c[1]]));
}

/**
 * The change a drag makes, in face space, for a pin grabbed at `from` and now at `to`:
 * - red (and dragging the picture itself): move
 * - green: turn and scale evenly about the red pin, so the green pin follows the pointer
 * - blue: stretch or lean the picture's height about the red pin: the green pin stays, the blue pin follows
 * - yellow: skew along the far diagonal: the yellow pin follows, the other diagonal stays (an affine stand-in for the
 *   free "distort" pin; a true perspective distort is not offered)
 */
export function dragDelta(kind: PinKind, pins: Pins, from: V2, to: V2): Aff {
  const d = sub(to, from);
  if (kind === "red" || kind === "move") return [1, 0, 0, 1, d[0], d[1]];
  const a = sub(pins.g, pins.r);
  const b = sub(pins.b, pins.r);
  if (kind === "green") {
    const na = add(a, d);
    if (len(a) < 1e-9 || len(na) < 1e-6) return IDENT;
    const k = len(na) / len(a);
    const turn = Math.atan2(na[1], na[0]) - Math.atan2(a[1], a[0]);
    const [s, c] = [Math.sin(turn) * k, Math.cos(turn) * k];
    return about(pins.r, [c, s, -s, c, 0, 0]);
  }
  if (kind === "blue") {
    const m = fromTriangles([[0, 0], a, b], [[0, 0], a, add(b, d)]);
    return m && Math.abs(det(m)) > 1e-6 ? about(pins.r, [m[0], m[1], m[2], m[3], 0, 0]) : IDENT;
  }
  // yellow: the diagonal a+b goes to the new far corner, the other diagonal b-a stays
  const diag = add(a, b);
  const other = sub(b, a);
  const m = fromTriangles([[0, 0], diag, other], [[0, 0], add(diag, d), other]);
  return m && Math.abs(det(m)) > 1e-6 ? about(pins.r, [m[0], m[1], m[2], m[3], 0, 0]) : IDENT;
}

/** what the engine needs: the same change in picture space */
export function pictureMatrix(pl: Plane, d: Aff): Aff | null {
  const di = invert(d);
  const li = invert(pl.L);
  if (!di || !li) return null;
  return after(pl.L, after(di, li));
}

export type MenuOp = "flipH" | "flipV" | "rot90" | "rot180" | "rot270";

/** the change (face space) a right-click item makes, about the middle of the picture */
export function opDelta(op: MenuOp, p: Pins): Aff | null {
  const mid: V2 = [(p.r[0] + p.y[0]) / 2, (p.r[1] + p.y[1]) / 2];
  switch (op) {
    case "flipH":
      return fromTriangles([p.r, p.g, p.b], [p.g, p.r, p.y]);
    case "flipV":
      return fromTriangles([p.r, p.g, p.b], [p.b, p.y, p.r]);
    case "rot90":
      return turnAbout(mid, 90);
    case "rot180":
      return turnAbout(mid, 180);
    case "rot270":
      return turnAbout(mid, 270);
  }
}

/** a 4-point perspective map from a w x h box (x right, y down) to four screen points, as a CSS matrix3d */
export function homography(w: number, h: number, dst: [V2, V2, V2, V2]): number[] | null {
  // dst[i] for the box corners (0,0), (w,0), (w,h), (0,h)
  const src: V2[] = [[0, 0], [w, 0], [w, h], [0, h]];
  const A: number[][] = [];
  const B: number[] = [];
  for (let i = 0; i < 4; i++) {
    const [x, y] = src[i];
    const [u, v] = dst[i];
    A.push([x, y, 1, 0, 0, 0, -u * x, -u * y]);
    B.push(u);
    A.push([0, 0, 0, x, y, 1, -v * x, -v * y]);
    B.push(v);
  }
  // Gauss-Jordan with partial pivoting
  const n = 8;
  for (let i = 0; i < n; i++) {
    let p = i;
    for (let r = i + 1; r < n; r++) if (Math.abs(A[r][i]) > Math.abs(A[p][i])) p = r;
    if (Math.abs(A[p][i]) < 1e-10) return null;
    [A[i], A[p]] = [A[p], A[i]];
    [B[i], B[p]] = [B[p], B[i]];
    for (let r = 0; r < n; r++) {
      if (r === i) continue;
      const k = A[r][i] / A[i][i];
      for (let c = i; c < n; c++) A[r][c] -= k * A[i][c];
      B[r] -= k * B[i];
    }
  }
  const [a, b, c, d, e, f, g, hh] = B.map((v, i) => v / A[i][i]);
  return [a, d, 0, g, b, e, 0, hh, 0, 0, 1, 0, c, f, 0, 1];
}
