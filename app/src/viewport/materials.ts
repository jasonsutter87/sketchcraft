import * as THREE from "three";

/** Back faces pushed out by a few screen pixels: a silhouette outline. */
export function profileMaterial(color: number, px: number, clip: THREE.Plane[]): THREE.MeshBasicMaterial {
  const m = new THREE.MeshBasicMaterial({ color, side: THREE.BackSide, clippingPlanes: clip });
  m.onBeforeCompile = (shader) => {
    shader.uniforms.profilePx = { value: px };
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", "#include <common>\nuniform float profilePx;")
      .replace(
        "#include <project_vertex>",
        `#include <project_vertex>
        vec4 clipN = projectionMatrix * modelViewMatrix * vec4(normal, 0.0);
        vec2 dir = length(clipN.xy) > 1e-6 ? normalize(clipN.xy) : vec2(0.0);
        // pixels -> clip space (approximate viewport of 1000 px tall)
        gl_Position.xy += dir * profilePx * gl_Position.w / 500.0;`
      );
  };
  return m;
}

// images for textured faces, loaded once per material and image
const textureCache = new Map<string, THREE.Texture>();
const painted = new Map<string, { tex: THREE.Texture; baseKey: string; at: number }>();
const keyOf = (name: string, data: string) => `${name}:${data.length}:${data.slice(-32)}`;

/**
 * A Texture Paint stroke was sent: when the picture it made arrives (a different image for `name`), the canvas the stroke
 * was drawn on is used for it, so the surface does not blink while the real image loads.
 */
export function adoptPainted(name: string, oldData: string, tex: THREE.Texture) {
  painted.set(name, { tex, baseKey: keyOf(name, oldData), at: Date.now() });
}

/** The stroke changed nothing or failed: forget it. */
export function forgetPainted(name: string) {
  painted.delete(name);
}

export function textureFor(name: string, data: string): THREE.Texture {
  const key = keyOf(name, data);
  let t = textureCache.get(key);
  const p = painted.get(name);
  if (p && !t && key !== p.baseKey && Date.now() - p.at < 10000) {
    painted.delete(name);
    textureCache.set(key, p.tex);
    return p.tex;
  }
  if (!t) {
    t = new THREE.TextureLoader().load(data);
    t.wrapS = t.wrapT = THREE.RepeatWrapping;
    t.colorSpace = THREE.SRGBColorSpace;
    t.anisotropy = 8;
    textureCache.set(key, t);
  }
  return t;
}

let checker: THREE.Texture | null = null;
/** An 8 x 8 checker over 0..1 (the UV editor's Checker), repeating outside it. */
export function checkerTexture(): THREE.Texture {
  if (!checker) {
    const c = document.createElement("canvas");
    c.width = c.height = 256;
    const g = c.getContext("2d")!;
    for (let j = 0; j < 8; j++) for (let i = 0; i < 8; i++) ((g.fillStyle = (i + j) % 2 ? "#6f7385" : "#d9dce6"), g.fillRect(i * 32, j * 32, 32, 32));
    checker = new THREE.CanvasTexture(c);
    checker.wrapS = checker.wrapT = THREE.RepeatWrapping;
    checker.colorSpace = THREE.SRGBColorSpace;
    checker.anisotropy = 8;
  }
  return checker;
}
