import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { FrameWatch, type RendererState } from "../src/viewport/framewatch";

function fakeRenderer(): RendererState & { clear: number } {
  const r = {
    clear: 0xf3f4f8,
    domElement: { width: 800, height: 600 },
    getPixelRatio: () => 1,
    getClearColor: (c: THREE.Color) => c.setHex(r.clear),
    getClearAlpha: () => 1,
    shadowMap: { enabled: false },
    toneMapping: 0,
    toneMappingExposure: 1,
    localClippingEnabled: true,
    clippingPlanes: [] as THREE.Plane[],
  };
  return r;
}

function setup() {
  const scene = new THREE.Scene();
  const cam = new THREE.PerspectiveCamera(40, 4 / 3, 0.05, 1000);
  cam.position.set(10, -10, 10);
  cam.lookAt(0, 0, 0);
  const mat = new THREE.MeshBasicMaterial({ color: 0xffffff });
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(1, 1, 1), mat);
  scene.add(mesh);
  const r = fakeRenderer();
  const w = new FrameWatch(1000);
  let now = 0;
  /** one animation frame, 16 ms after the last: was it drawn? */
  const frame = () => w.shouldDraw((now += 16), r, scene, cam);
  return { scene, cam, mat, mesh, r, w, frame };
}

describe("render on demand (FrameWatch)", () => {
  it("draws the first frame, then nothing while the view is still", () => {
    const { frame, w } = setup();
    expect(frame()).toBe(true);
    let drawn = 0;
    for (let i = 0; i < 30; i++) drawn += +frame(); // about half a second at 60 frames a second
    expect(drawn).toBe(0);
    expect(w.drawn).toBe(1);
  });

  it("still draws now and then (a heartbeat), about once a second", () => {
    const { frame } = setup();
    frame();
    let drawn = 0;
    for (let i = 0; i < 60 * 5; i++) drawn += +frame(); // five seconds
    expect(drawn).toBeGreaterThanOrEqual(4);
    expect(drawn).toBeLessThanOrEqual(5);
  });

  it("draws when the camera moves or zooms", () => {
    const { frame, cam } = setup();
    frame();
    cam.position.x += 0.01;
    cam.lookAt(0, 0, 0);
    expect(frame()).toBe(true);
    expect(frame()).toBe(false);
    cam.fov = 60;
    cam.updateProjectionMatrix();
    expect(frame()).toBe(true);
  });

  it("ignores floating-point noise (a plan view's camera wanders in the 16th digit every frame), not a real drift", () => {
    const { frame, cam } = setup();
    cam.position.set(0, -0.0005, 60);
    cam.lookAt(0, 0, 0);
    frame();
    let drawn = 0;
    for (let i = 0; i < 30; i++) {
      cam.position.y = -0.0005 + (i % 2 ? 1e-18 : -1e-18);
      cam.position.z = 60 + (i % 3) * 1e-14;
      cam.lookAt(0, 0, 0);
      drawn += +frame();
    }
    expect(drawn).toBe(0);
    // a slow move that is real still adds up: readings compare with the last frame drawn, not the last frame
    for (let i = 0; i < 30 && !drawn; i++) {
      cam.position.x += 1e-4;
      drawn += +frame();
    }
    expect(drawn).toBe(1);
  });

  it("is not woken by what drawing itself changes (a see-through two-sided material is drawn in two passes)", () => {
    const { scene, cam, r, w } = setup();
    const m = new THREE.MeshBasicMaterial({ transparent: true, opacity: 0.3, side: THREE.DoubleSide });
    scene.add(new THREE.Mesh(new THREE.PlaneGeometry(1, 1), m));
    let now = 0;
    /** a frame, drawn the way three.js draws it: back, then front, each a material change */
    const frame = () => {
      const d = w.shouldDraw((now += 16), r, scene, cam);
      if (d) {
        m.side = THREE.BackSide;
        m.needsUpdate = true;
        m.side = THREE.DoubleSide;
        m.needsUpdate = true;
        w.drew(r, scene, cam);
      }
      return d;
    };
    expect(frame()).toBe(true);
    let drawn = 0;
    for (let i = 0; i < 30; i++) drawn += +frame();
    expect(drawn).toBe(0);
    m.opacity = 0.5;
    expect(frame()).toBe(true);
    expect(frame()).toBe(false);
  });

  it("draws when an object moves, hides, arrives or leaves", () => {
    const { frame, mesh, scene } = setup();
    frame();
    mesh.position.z = 1;
    expect(frame()).toBe(true);
    mesh.visible = false;
    expect(frame()).toBe(true);
    mesh.visible = true;
    frame();
    const extra = new THREE.Line(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3(1, 0, 0)]), new THREE.LineBasicMaterial());
    scene.add(extra);
    expect(frame()).toBe(true);
    scene.remove(extra);
    expect(frame()).toBe(true);
    expect(frame()).toBe(false);
  });

  it("draws when a material, a geometry or a texture changes", () => {
    const { frame, mat, mesh } = setup();
    frame();
    mat.color.setHex(0xff0000);
    expect(frame()).toBe(true);
    mat.opacity = 0.5;
    expect(frame()).toBe(true);
    (mesh.geometry.getAttribute("position") as THREE.BufferAttribute).needsUpdate = true;
    expect(frame()).toBe(true);
    const tex = new THREE.Texture();
    mat.map = tex;
    expect(frame()).toBe(true);
    tex.needsUpdate = true; // its picture arrived
    expect(frame()).toBe(true);
    mat.clippingPlanes = [new THREE.Plane(new THREE.Vector3(0, 0, -1), 1)];
    expect(frame()).toBe(true);
    mat.clippingPlanes[0].constant = 2; // the section moved
    expect(frame()).toBe(true);
    expect(frame()).toBe(false);
  });

  it("draws when a shader's uniform changes (fat lines' resolution)", () => {
    const { frame, scene } = setup();
    const sm = new THREE.ShaderMaterial({ uniforms: { resolution: { value: new THREE.Vector2(800, 600) }, w: { value: 2 } } });
    scene.add(new THREE.Mesh(new THREE.PlaneGeometry(1, 1), sm));
    frame();
    sm.uniforms.resolution.value.set(1000, 600);
    expect(frame()).toBe(true);
    sm.uniforms.w.value = 3;
    expect(frame()).toBe(true);
    expect(frame()).toBe(false);
  });

  it("draws when the canvas, clear colour or the view's own switches change, and when asked", () => {
    const { frame, r, w, scene, cam } = setup();
    frame();
    r.domElement.width = 1000;
    expect(frame()).toBe(true);
    r.clear = 0x000000;
    expect(frame()).toBe(true);
    r.shadowMap.enabled = true;
    expect(frame()).toBe(true);
    expect(w.shouldDraw(10_000, r, scene, cam, [1])).toBe(true); // ambient occlusion on
    expect(w.shouldDraw(10_016, r, scene, cam, [1])).toBe(false);
    w.invalidate();
    expect(w.shouldDraw(10_032, r, scene, cam, [1])).toBe(true);
  });
});
