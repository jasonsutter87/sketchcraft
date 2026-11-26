// Inference priority (#231): hovering an edge should say "On Edge" (so the Tape Measure makes a
// parallel guide), not "From Point", and the drawing axes count as lines too.
import { describe, expect, it, beforeEach } from "vitest";
import * as THREE from "three";
import { infer, forgetRecent, type InferEnv } from "../src/inference";
import type { Scene } from "../src/engine";

const W = 1000, H = 800;
// a rectangle 4 x 3 m on the ground, away from the origin
const corners: [number, number, number][] = [[2, 2, 0], [6, 2, 0], [6, 5, 0], [2, 5, 0]];
const edges: number[] = [];
for (let i = 0; i < 4; i++) edges.push(...corners[i], ...corners[(i + 1) % 4]);

function env(): InferEnv {
  const camera = new THREE.PerspectiveCamera(40, W / H, 0.05, 2000);
  camera.up.set(0, 0, 1);
  camera.position.set(4, -8, 10);
  camera.lookAt(4, 3.5, 0);
  camera.updateMatrixWorld();
  const scene = { edges, edgeList: [], guides: [], axes: null } as unknown as Scene;
  return { camera, rect: { left: 0, top: 0, width: W, height: H } as DOMRect, scene, pick: () => null, groundZ: 0 };
}

/** the mouse position over a world point */
function at(E: InferEnv, x: number, y: number, z = 0) {
  const q = new THREE.Vector3(x, y, z).project(E.camera);
  return { clientX: (q.x * 0.5 + 0.5) * W, clientY: (-q.y * 0.5 + 0.5) * H };
}

describe("inference over edges (#231)", () => {
  beforeEach(() => forgetRecent());

  it("hovering a corner, then along a side, gives On Edge (not From Point)", () => {
    const E = env();
    expect(infer(E, at(E, 2, 2)).kind).toBe("endpoint"); // remembered for From Point
    expect(infer(E, at(E, 6, 2)).kind).toBe("endpoint");
    const s = infer(E, at(E, 3, 2));
    expect(s.label).toBe("On Edge");
    expect(s.kind).toBe("on-edge");
    expect(Math.abs(s.along!.y)).toBeLessThan(1e-9); // along the side, for a parallel guide
  });

  it("still lines up with a hovered corner away from any edge", () => {
    const E = env();
    infer(E, at(E, 6, 5));
    const s = infer(E, at(E, 6, 8)); // straight out from the corner along green, nothing there
    expect(s.label).toBe("From Point");
  });

  it("snaps onto the drawing axes", () => {
    const E = env();
    const s = infer(E, at(E, 1, 0));
    expect(s.label).toBe("On Red Axis");
    expect(s.kind).toBe("on-edge");
    expect(s.p.y).toBeCloseTo(0, 6);
    expect(Math.abs(s.along!.x)).toBeCloseTo(1, 6);
    const g = infer(E, at(E, 0, 1.5));
    expect(g.label).toBe("On Green Axis");
  });

  it("an edge nearer than an axis wins", () => {
    const E = env();
    expect(infer(E, at(E, 4, 2)).kind).not.toBe("face");
    expect(infer(E, at(E, 4.2, 2)).label).toBe("On Edge");
  });
});

describe("snapping while the direction is locked (#253)", () => {
  beforeEach(() => forgetRecent());

  it("hovering a corner sets the length along the locked axis", () => {
    const E = env();
    // drawing from (0, 3, 0) locked to red; hover the far corner (6, 5, 0)
    const s = infer(E, at(E, 6, 5), { anchor: new THREE.Vector3(0, 3, 0), lock: 0 });
    expect(s.kind).toBe("axis");
    expect(s.label).toBe("On Red Axis · Endpoint");
    expect(s.p.toArray().map((x) => +x.toFixed(6))).toEqual([6, 3, 0]);
    expect(s.from!.toArray()).toEqual([6, 5, 0]);
  });

  it("with nothing to snap to, it rides the axis under the cursor", () => {
    const E = env();
    const s = infer(E, at(E, 1, 3.4), { anchor: new THREE.Vector3(0, 3, 0), lock: 0 });
    expect(s.label).toBe("On Red Axis");
    expect(s.p.y).toBeCloseTo(3, 6);
    expect(s.from).toBeUndefined();
  });
});
