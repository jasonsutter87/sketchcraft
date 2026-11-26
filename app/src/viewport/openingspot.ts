// Door and Window tools (#264): where the opening would go on the hovered wall face, for the preview. The engine
// places it the same way (core/src/doorwin.rs, Model::opening_frame): centred on the cursor along the wall, moved along
// to fit, a door on the wall's foot and a window at its sill.

type V3 = [number, number, number];

export type OpeningSpot = { ok: true; corners: [V3, V3, V3, V3] } | { ok: false; why: string };

const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/** Where a `w` x `h` door or window goes on an upright face `outer` (outward `normal`), near `point`. */
export function openingSpot(kind: "door" | "window", outer: V3[], normal: V3, point: V3, w: number, h: number, sill: number): OpeningSpot {
  if (Math.abs(normal[2]) > 1e-3 || outer.length < 3) return { ok: false, why: "Hover the side of a wall" };
  // along the wall: (-n) x Z
  const len = Math.hypot(normal[0], normal[1]);
  const along: V3 = [-normal[1] / len, normal[0] / len, 0];
  const a = outer.map((p) => dot(p, along));
  const zs = outer.map((p) => p[2]);
  const [a0, a1, z0, z1] = [Math.min(...a), Math.max(...a), Math.min(...zs), Math.max(...zs)];
  if (w > a1 - a0 - 1e-3) return { ok: false, why: `Wider than this wall (${(a1 - a0).toFixed(2)} m)` };
  const foot = z0 + (kind === "door" ? 0 : sill);
  if (foot + h > z1 - 1e-3) return { ok: false, why: "Doesn't fit under the top of the wall" };
  const left = Math.min(Math.max(dot(point, along) - w / 2, a0), a1 - w);
  // on the face's plane: start from a corner and move along and up within it
  const p0 = outer[0];
  const at = (s: number, z: number): V3 => {
    const k = s - dot(p0, along);
    return [p0[0] + along[0] * k, p0[1] + along[1] * k, z];
  };
  return { ok: true, corners: [at(left, foot), at(left + w, foot), at(left + w, foot + h), at(left, foot + h)] };
}
