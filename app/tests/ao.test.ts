import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { GTAOPass } from "three/examples/jsm/postprocessing/GTAOPass.js";
import { aoRadius, setupAO, skipAO, syncAO } from "../src/viewport/ao";

describe("ambient occlusion (#265)", () => {
  it("reaches about 4% of the view, within limits", () => {
    expect(aoRadius(10)).toBeCloseTo(0.4);
    expect(aoRadius(0.01)).toBe(0.02);
    expect(aoRadius(1e6)).toBe(20);
    expect(aoRadius(NaN)).toBe(0.25);
    expect(aoRadius(0)).toBe(0.25);
  });

  it("follows the camera between perspective and parallel, and the zoom", () => {
    const scene = new THREE.Scene();
    const persp = new THREE.PerspectiveCamera();
    const gt = new GTAOPass(scene, persp, 8, 8);
    setupAO(gt);
    expect(gt.gtaoMaterial.defines.PERSPECTIVE_CAMERA).toBe(1);
    syncAO(gt, new THREE.OrthographicCamera(), 50);
    expect(gt.gtaoMaterial.defines.PERSPECTIVE_CAMERA).toBe(0);
    expect(gt.gtaoMaterial.uniforms.radius.value).toBeCloseTo(2);
    syncAO(gt, persp, 50);
    expect(gt.gtaoMaterial.defines.PERSPECTIVE_CAMERA).toBe(1);
  });

  it("leaves marked objects out of the depth pass, and puts them back", () => {
    const scene = new THREE.Scene();
    const face = new THREE.Mesh();
    const outline = new THREE.Mesh();
    skipAO(outline);
    scene.add(face, outline);
    const gt = new GTAOPass(scene, new THREE.PerspectiveCamera(), 8, 8);
    setupAO(gt);
    expect(gt.normalMaterial.side).toBe(THREE.DoubleSide);
    const inner = gt as unknown as { _overrideVisibility(): void; _restoreVisibility(): void };
    inner._overrideVisibility();
    expect(face.visible).toBe(true);
    expect(outline.visible).toBe(false);
    inner._restoreVisibility();
    expect(outline.visible).toBe(true);
  });
});
