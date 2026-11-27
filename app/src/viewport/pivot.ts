// Orbit about the point under the cursor (as SketchUp does). OrbitControls turns the camera about its target; to turn
// about another point P instead, after each step we slide the camera and its target together by the difference between
// "turn about the target" and "turn about P". The eye does not move when the orbit starts, so the view never jumps.
import * as THREE from "three";

/** Orbiting about the picked point is on unless the saved setting `sketchcraft.orbitPivot` is "off". */
export function orbitPivotEnabled(read: (key: string) => string | null): boolean {
  try {
    return read("sketchcraft.orbitPivot") !== "off";
  } catch {
    return true;
  }
}

/** OrbitControls states (its `state` field) in which a drag turns the camera: mouse rotate, touch rotate, touch dolly+rotate. */
export function isRotateState(state: number): boolean {
  return state === 0 || state === 3 || state === 6;
}

/**
 * How far to slide the camera and its target after the camera turned about `target` by `after * before^-1`, so the
 * turn is about `pivot` instead. Zero when nothing turned or when the pivot is the target.
 */
export function pivotShift(pivot: THREE.Vector3, target: THREE.Vector3, before: THREE.Quaternion, after: THREE.Quaternion): THREE.Vector3 {
  const r = after.clone().multiply(before.clone().invert());
  const rel = pivot.clone().sub(target);
  return rel.clone().sub(rel.clone().applyQuaternion(r));
}

/** Turn a camera (perspective or parallel) and its target about `pivot` by `q` (the trackpad swipe). */
export function orbitAbout(cam: THREE.Camera, target: THREE.Vector3, pivot: THREE.Vector3, q: THREE.Quaternion): void {
  cam.position.sub(pivot).applyQuaternion(q).add(pivot);
  target.sub(pivot).applyQuaternion(q).add(pivot);
  cam.quaternion.premultiply(q);
}
