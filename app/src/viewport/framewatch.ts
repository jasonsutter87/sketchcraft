// Render on demand: draw the model view only when the picture would change.
//
// The view used to draw every animation frame. On a machine with a GPU that is nearly free; without one
// (a VM, an old laptop, a CI runner: WebGL drawn on the CPU by SwiftShader) each frame of a large canvas
// costs tens of milliseconds on every core, so an idle view kept the machine busy and everything else
// (engine round trips, plugin commands, timers) waited behind it: about 35 ms for a setTimeout(0) and
// 100-160 ms for a scene fetch, against 4 ms and 3 ms when nothing is drawn.
//
// Rather than ask every one of the view's many edits to say "draw again" (and miss some), FrameWatch reads
// what a frame is made of: the camera, the canvas size and clear colour, and for every object in the scene
// its place, visibility, geometry versions and material settings. If none of it changed since the last
// frame drawn, the frame is skipped (floating-point noise in the 16th digit does not count). A slow
// heartbeat still draws now and then, so anything it cannot see (a change made inside a shader's code)
// shows up within a second.
import * as THREE from "three";

/** What FrameWatch reads from the renderer (a WebGLRenderer has all of it). */
export interface RendererState {
  domElement: { width: number; height: number };
  getPixelRatio(): number;
  getClearColor(target: THREE.Color): THREE.Color;
  getClearAlpha(): number;
  shadowMap: { enabled: boolean };
  toneMapping: number;
  toneMappingExposure: number;
  localClippingEnabled: boolean;
  clippingPlanes: THREE.Plane[];
}

const MAPS = ["map", "alphaMap", "aoMap", "bumpMap", "normalMap", "emissiveMap", "envMap", "lightMap", "roughnessMap", "metalnessMap", "matcap", "specularMap", "displacementMap", "gradientMap"] as const;
const NUMS = ["opacity", "side", "alphaTest", "linewidth", "size", "roughness", "metalness", "envMapIntensity", "emissiveIntensity", "polygonOffsetFactor", "polygonOffsetUnits", "dashSize", "gapSize", "scale"] as const;
const FLAGS = ["visible", "transparent", "depthTest", "depthWrite", "wireframe", "vertexColors", "polygonOffset", "flatShading", "sizeAttenuation", "colorWrite"] as const;

/**
 * Two readings that draw the same picture. Floating-point noise does not count: orbit controls recompute the
 * camera every frame and, looking straight down (a plan view), it wanders in the 16th digit without moving.
 * Readings are compared with the last frame drawn, so a slow real drift still adds up and draws.
 */
function same(a: number, b: number): boolean {
  if (a === b) return true;
  if (a !== a) return b !== b; // NaN
  return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b));
}

export class FrameWatch {
  private prev: number[] = [];
  private cur: number[] = [];
  private force = true;
  private lastDrawn = -Infinity;
  private readonly tmp = new THREE.Color();
  /** frames drawn so far (dev builds show it to the UI tests) */
  drawn = 0;

  /** @param heartbeatMs draw at least this often even when nothing seems to change */
  constructor(private readonly heartbeatMs = 1000) {}

  /** The next frame is drawn whatever it holds (after drawing elsewhere, a resize to the same size, a lost context). */
  invalidate() {
    this.force = true;
  }

  /**
   * Whether the frame for `scene` seen by `camera` should be drawn now. `extra` is anything else the picture
   * depends on (the view's own switches). Counts the frame as drawn when it says yes.
   */
  shouldDraw(now: number, renderer: RendererState, scene: THREE.Scene, camera: THREE.Camera, extra: readonly number[] = []): boolean {
    this.read(renderer, scene, camera, extra);
    const v = this.cur;
    const changed = this.force || v.length !== this.prev.length || v.some((x, i) => !same(x, this.prev[i]));
    if (!changed && now - this.lastDrawn < this.heartbeatMs) return false;
    [this.prev, this.cur] = [this.cur, this.prev];
    this.force = false;
    this.lastDrawn = now;
    this.drawn++;
    return true;
  }

  /**
   * After drawing: read the frame again, as drawing left it. The renderer changes some of what it draws (a
   * see-through, two-sided material is drawn back then front, and each pass counts as a material change), and
   * that is not a reason to draw the next frame.
   */
  drew(renderer: RendererState, scene: THREE.Scene, camera: THREE.Camera, extra: readonly number[] = []) {
    this.read(renderer, scene, camera, extra);
    [this.prev, this.cur] = [this.cur, this.prev];
  }

  private read(renderer: RendererState, scene: THREE.Scene, camera: THREE.Camera, extra: readonly number[]) {
    scene.updateMatrixWorld();
    if (camera.parent === null) camera.updateMatrixWorld();
    const v = this.cur;
    v.length = 0;
    for (const x of extra) v.push(x);
    const el = renderer.domElement;
    v.push(el.width, el.height, renderer.getPixelRatio(), renderer.getClearColor(this.tmp).getHex(), renderer.getClearAlpha());
    v.push(+renderer.shadowMap.enabled, renderer.toneMapping, renderer.toneMappingExposure, +renderer.localClippingEnabled);
    this.planes(renderer.clippingPlanes);
    v.push(camera.id, ...camera.matrixWorld.elements, ...camera.projectionMatrix.elements);
    const bg = scene.background;
    v.push(bg ? ((bg as THREE.Color).isColor ? (bg as THREE.Color).getHex() : this.texture(bg as THREE.Texture)) : -1);
    v.push(scene.environment ? this.texture(scene.environment) : -1, scene.fog ? scene.fog.color.getHex() : -1);
    scene.traverseVisible((o) => this.object(o));
  }

  private object(o: THREE.Object3D) {
    const v = this.cur;
    v.push(o.id, o.renderOrder, o.layers.mask, +o.castShadow, +o.receiveShadow, +o.frustumCulled, ...o.matrixWorld.elements);
    const light = o as THREE.Light;
    if (light.isLight) v.push(light.intensity, light.color.getHex());
    const g = (o as THREE.Mesh).geometry as THREE.BufferGeometry | undefined;
    if (g?.isBufferGeometry) {
      v.push(g.id, g.drawRange.start, g.drawRange.count, g.groups.length, g.index ? g.index.version : -1);
      for (const name in g.attributes) {
        const a = g.attributes[name] as THREE.BufferAttribute | THREE.InterleavedBufferAttribute;
        v.push((a as THREE.InterleavedBufferAttribute).isInterleavedBufferAttribute ? (a as THREE.InterleavedBufferAttribute).data.version : (a as THREE.BufferAttribute).version, a.count);
      }
      const im = o as THREE.InstancedMesh;
      if (im.isInstancedMesh) v.push(im.count, im.instanceMatrix.version, im.instanceColor ? im.instanceColor.version : -1);
    }
    const m = (o as THREE.Mesh).material as THREE.Material | THREE.Material[] | undefined;
    if (Array.isArray(m)) for (const x of m) this.material(x);
    else if (m) this.material(m);
  }

  private material(m: THREE.Material) {
    const v = this.cur;
    const r = m as unknown as Record<string, unknown>;
    v.push(m.id, m.version);
    for (const k of FLAGS) if (k in r) v.push(+!!r[k]);
    for (const k of NUMS) if (typeof r[k] === "number") v.push(r[k] as number);
    for (const k of ["color", "emissive", "specular"]) if ((r[k] as THREE.Color | undefined)?.isColor) v.push((r[k] as THREE.Color).getHex());
    for (const k of MAPS) if (r[k]) v.push(this.texture(r[k] as THREE.Texture));
    this.planes(m.clippingPlanes);
    const u = (m as THREE.ShaderMaterial).uniforms;
    if (u) for (const name in u) this.value(u[name]?.value);
  }

  private value(x: unknown) {
    const v = this.cur;
    if (typeof x === "number") v.push(x);
    else if (typeof x === "boolean") v.push(+x);
    else if (x && typeof x === "object") {
      const o = x as { isColor?: boolean; isTexture?: boolean; isVector2?: boolean; isVector3?: boolean; isVector4?: boolean; isMatrix4?: boolean; isMatrix3?: boolean };
      if (o.isColor) v.push((x as THREE.Color).getHex());
      else if (o.isTexture) v.push(this.texture(x as THREE.Texture));
      else if (o.isVector2 || o.isVector3 || o.isVector4) for (const c of (x as THREE.Vector4).toArray()) v.push(c);
      else if (o.isMatrix4 || o.isMatrix3) for (const c of (x as THREE.Matrix4).elements) v.push(c);
    }
  }

  /** a texture as one number: its id and how often its pixels changed */
  private texture(t: THREE.Texture): number {
    return t.id * 1e6 + t.version;
  }

  private planes(ps: THREE.Plane[] | null | undefined) {
    if (!ps) return void this.cur.push(-1);
    this.cur.push(ps.length);
    for (const p of ps) this.cur.push(p.normal.x, p.normal.y, p.normal.z, p.constant);
  }
}
