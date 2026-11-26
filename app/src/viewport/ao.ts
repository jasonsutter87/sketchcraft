import * as THREE from "three";
import type { GTAOPass } from "three/examples/jsm/postprocessing/GTAOPass.js";

/**
 * Ambient occlusion setup (#265: the Architectural style's AO "still needs some work", in Blender 3D Architect's
 * first look). GTAOPass out of the box was wrong for SketchCraft in four ways, fixed here and in Viewport.tsx:
 * - its depth/normal pass drew the profile outlines (inverted hulls of back faces at the faces' own depth), the sky,
 *   the shadow catcher and the preview/overlay groups, so AO speckled along every profile and darkened near the sky;
 * - it decides perspective vs parallel once, when built, so Parallel and the standard views shaded with the wrong maths;
 * - its 0.25 m radius ignored the zoom: blotchy up close, nothing at all on a whole building;
 * - Viewport resized it to CSS pixels after the composer had sized it to device pixels: half-resolution AO on HiDPI.
 */

/** Objects the AO depth pass skips besides points and lines (GTAOPass already skips those). */
export const NO_AO = "noAO";

/** Mark an object (and so everything under it) as left out of ambient occlusion. */
export function skipAO(o: THREE.Object3D): void {
  o.userData[NO_AO] = true;
}

/** How far AO reaches, in model units, for a camera `dist` from what it looks at: about 4% of the view. */
export function aoRadius(dist: number): number {
  if (!Number.isFinite(dist) || dist <= 0) return 0.25;
  return Math.min(Math.max(dist * 0.04, 0.02), 20);
}

type Internals = { _overrideVisibility(): void; _visibilityCache: THREE.Object3D[]; scene: THREE.Object3D };

/** Set up a freshly made pass: faces seen from behind still count, and marked objects are left out of its depth pass. */
export function setupAO(gt: GTAOPass): void {
  // a single-sided wall seen from behind still blocks what is behind it
  gt.normalMaterial.side = THREE.DoubleSide;
  const inner = gt as unknown as Internals;
  const base = inner._overrideVisibility.bind(gt);
  inner._overrideVisibility = () => {
    base();
    const cache = inner._visibilityCache;
    inner.scene.traverse((o) => {
      if (o.userData[NO_AO] && o.visible) {
        o.visible = false;
        cache.push(o);
      }
    });
  };
}

/** Before each frame: the camera to shade for (perspective or parallel) and the radius for its distance. */
export function syncAO(gt: GTAOPass, cam: THREE.Camera, dist: number): void {
  gt.camera = cam;
  const persp = (cam as THREE.PerspectiveCamera).isPerspectiveCamera ? 1 : 0;
  const m = gt.gtaoMaterial;
  if (m.defines.PERSPECTIVE_CAMERA !== persp) {
    m.defines.PERSPECTIVE_CAMERA = persp;
    m.needsUpdate = true;
  }
  const r = aoRadius(dist);
  if (Math.abs(m.uniforms.radius.value - r) > r * 0.02) gt.updateGtaoMaterial({ radius: r });
}
