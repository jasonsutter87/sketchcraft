import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { isRotateState, orbitAbout, orbitPivotEnabled, pivotShift } from "../src/viewport/pivot";

const V = (x: number, y: number, z: number) => new THREE.Vector3(x, y, z);

/** What OrbitControls does: turn the eye about the target and look at it. */
function turnAboutTarget(cam: THREE.Camera, target: THREE.Vector3, q: THREE.Quaternion) {
  cam.position.sub(target).applyQuaternion(q).add(target);
  cam.quaternion.premultiply(q);
}

describe("orbit pivot", () => {
  const q = new THREE.Quaternion().setFromAxisAngle(V(0, 0, 1), 0.7);

  for (const [name, make] of [
    ["perspective", () => new THREE.PerspectiveCamera(40, 1.5, 0.1, 1000)],
    ["parallel", () => new THREE.OrthographicCamera(-5, 5, 5, -5, -1000, 1000)],
  ] as const) {
    it(`keeps the picked point still on screen (${name})`, () => {
      const cam: THREE.Camera = make();
      cam.up.set(0, 0, 1);
      cam.position.set(30, -40, 28);
      const target = V(0, 0, 0);
      cam.lookAt(target);
      cam.updateMatrixWorld(true);
      const pivot = V(4, 3, 2);
      const eye0 = cam.position.clone();
      const quat0 = cam.quaternion.clone();
      const screen0 = pivot.clone().project(cam);

      turnAboutTarget(cam, target, q);
      const d = pivotShift(pivot, target, quat0, cam.quaternion);
      cam.position.add(d);
      target.add(d);
      cam.updateMatrixWorld(true);

      expect(cam.position.distanceTo(eye0)).toBeGreaterThan(1); // it did orbit
      const screen1 = pivot.clone().project(cam);
      expect(screen1.x).toBeCloseTo(screen0.x, 9);
      expect(screen1.y).toBeCloseTo(screen0.y, 9);
      // the camera still looks at its target, and the distance to the pivot is unchanged
      const look = target.clone().sub(cam.position).normalize();
      expect(look.dot(cam.getWorldDirection(new THREE.Vector3()))).toBeCloseTo(1, 9);
      expect(cam.position.distanceTo(pivot)).toBeCloseTo(eye0.distanceTo(pivot), 9);
    });
  }

  it("makes no shift when nothing turned, or the pivot is the target", () => {
    const a = new THREE.Quaternion();
    expect(pivotShift(V(4, 3, 2), V(0, 0, 0), a, a).length()).toBeCloseTo(0, 12);
    expect(pivotShift(V(1, 2, 3), V(1, 2, 3), a, q).length()).toBeCloseTo(0, 12);
  });

  it("orbitAbout turns the camera and target about the point", () => {
    const cam = new THREE.PerspectiveCamera();
    cam.position.set(10, 0, 0);
    const target = V(0, 0, 0);
    orbitAbout(cam, target, V(10, 0, 0), new THREE.Quaternion().setFromAxisAngle(V(0, 0, 1), Math.PI));
    expect(cam.position.distanceTo(V(10, 0, 0))).toBeCloseTo(0, 9);
    expect(target.distanceTo(V(20, 0, 0))).toBeCloseTo(0, 9);
  });

  it("is on by default and off only when saved off", () => {
    expect(orbitPivotEnabled(() => null)).toBe(true);
    expect(orbitPivotEnabled(() => "on")).toBe(true);
    expect(orbitPivotEnabled(() => "off")).toBe(false);
    expect(
      orbitPivotEnabled(() => {
        throw new Error("blocked");
      }),
    ).toBe(true);
  });

  it("knows which OrbitControls states turn the camera", () => {
    expect([-1, 0, 1, 2, 3, 4, 5, 6].filter(isRotateState)).toEqual([0, 3, 6]);
  });
});
