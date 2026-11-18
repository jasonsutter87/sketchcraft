// Thin bridge to the Rust engine. The UI never edits geometry itself: every
// change is an engine command, exactly what scripts and Claude send.
//
// Inside the desktop app this calls Tauri; in a plain browser it talks to
// `sketchcraft bridge` on 127.0.0.1:5175.
import { invoke } from "@tauri-apps/api/core";
import type { HatchUseInfo } from "./hatchswatch";

export type Vec3 = [number, number, number];

export interface FaceInfo {
  id: number;
  normal: Vec3;
  outer: Vec3[];
  group: number | null;
  area: number;
  material: string | null;
  hidden: boolean;
}

export interface EdgeInfo {
  id: number;
  a: Vec3;
  b: Vec3;
  /** the two points' vertex ids (Move can grab one) */
  va?: number;
  vb?: number;
  soft: boolean;
  group: number | null;
  /** edges of one arc, circle or welded curve share this */
  curve?: number | null;
  /** this edge's own line weight and style on sheets and in the model view (beats its tag's) */
  draft?: EdgeDraft | null;
}

/** A line weight (paper mm), style (solid, dashed, hidden, center) and color ("#rrggbb"); each optional. */
export interface EdgeDraft {
  weight_mm?: number;
  line?: string;
  color?: string;
}

export interface TagStyleInfo {
  weight_mm?: number;
  line?: string;
  color?: string;
  /** how the tag's cut walls are filled in plans and sections: the drawing's own poché ("auto", also when unset), a
   * solid `color`, the hatch, or nothing ("none"); `outline_mm` is the cut outline's pen (0: the heavy default) */
  cut?: { fill?: "auto" | "none" | "solid" | "hatch"; outline_mm?: number; color?: string; hatch?: HatchUseInfo; /** intersection priority 0 to 999: the higher tag wins where cuts overlap (absent: 0) */ priority?: number };
}

export interface GroupInfo {
  id: number;
  name: string;
  faces: number;
  tag: string | null;
  hidden: boolean;
  /** the group it sits inside (null: top level) */
  parent: number | null;
  locked: boolean;
}

export interface InstanceInfo {
  id: number;
  name: string;
  component: number;
  tag: string | null;
  hidden: boolean;
  /** the group it sits inside (null: top level) */
  parent: number | null;
  transform: number[];
  /** Dynamic Component attributes (values or "=formulas") */
  attrs?: Record<string, string>;
  /** it always turns to face the camera (a flat scale figure) */
  faceMe?: boolean;
}

/** A face-me figure: drawn around its own point, turned toward the camera. */
export interface Figure {
  id: number;
  pivot: Vec3;
  /** which way it faces now (radians in the ground plane) */
  facing: number;
  positions: number[];
  /** one color per triangle */
  colors: number[];
  edges: number[];
}

export interface ComponentInfo {
  id: number;
  /** cuts an opening in the face it's placed on */
  cuts?: boolean;
  name: string;
  /** placements in the model */
  placed: number;
  /** placements inside other components (it's a part) */
  inside?: number;
  /** the library file it came from */
  source?: string | null;
}

export interface GuideInfo {
  id: number;
  p: Vec3;
  dir: Vec3 | null;
}

export interface SavedView {
  name: string;
  eye: Vec3;
  target: Vec3;
  fov: number;
  /** Match Photo: the photo behind this view (data URL) */
  photo?: string;
}

export type Annotation =
  | { kind: "dimension"; id: number; a: Vec3; b: Vec3; offset: Vec3 }
  | { kind: "text"; id: number; at: Vec3; text: string; leader: Vec3 | null }
  | { kind: "swing"; id: number; hinge: Vec3; from: Vec3; to: Vec3 };

export interface MaterialInfo {
  name: string;
  color: Vec3;
  texture: { data: string; width: number; height: number; project: Vec3 | null } | null;
  /** see-through in the view (reference images): 1 solid */
  opacity?: number;
  roughness?: number | null;
  metalness?: number | null;
  glass?: boolean | null;
  emission?: number | null;
}

export interface CloudInfo {
  id: number;
  name: string;
  count: number;
  colored: boolean;
  hidden: boolean;
  /** changes when its points do */
  version: number;
  lo: Vec3;
  hi: Vec3;
}

export interface SectionInfo {
  id: number;
  name: string;
  origin: Vec3;
  normal: Vec3;
}

export interface Scene {
  /** who made the step Undo would take back / Redo would do again: "you", "plugin “X”", "an agent" */
  undo_origin?: string | null;
  redo_origin?: string | null;
  /** section planes, and the one cutting now */
  sections?: SectionInfo[];
  /** drawing axes moved with the Axes tool: origin, red, green */
  axes?: [Vec3, Vec3, Vec3] | null;
  activeSection?: number | null;
  /** point clouds (scans); their points come by cloud_data */
  clouds?: CloudInfo[];
  /** Classifier: IFC type by group / placement id */
  classes?: Record<string, string>;
  /** geo-location of the origin */
  location?: { lat: number; lon: number; elevation: number } | null;
  /** the ready-made setting around the model (suburban, city, park, desert, waterfront, studio) */
  environment?: string | null;
  /** Model from photos: corners marked across matched photos, and the edges between them */
  photoPoints?: { name: string; marks: Record<string, [number, number]> }[];
  photoEdges?: [number, number][];
  /** subdivision objects' editable cages, by group id */
  cages?: Record<string, { points: number[][]; faces: number[][]; materials: (string | null)[]; creases: number[][]; levels: number; symmetry: { axis: number; center: number } | null }>;
  /** texture coordinates, two per vertex */
  uvs: number[];
  /** per triangle: index into `textures`, or -1 */
  triTex: number[];
  /** material names of the images in use */
  textures: string[];
  materials: MaterialInfo[];
  positions: number[];
  /** per triangle: r, g, b */
  colors: number[];
  /** per triangle: r, g, b */
  backColors: number[];
  /** per triangle: roughness, metalness (the physically based style) */
  pbr?: number[];
  /** owner of each triangle: a face id, or a component instance id */
  triFace: number[];
  faces: FaceInfo[];
  edgeList: EdgeInfo[];
  /** video clips, played in order */
  timeline?: { clips: Clip[] };
  edges: number[];
  edgeIds: number[];
  /** per segment of `edges`: 0 for the default line, else a 1-based index into `edgeStyles` */
  edgeStyle?: number[];
  /** the distinct pens: width in paper mm, dash pattern in paper mm, rgb */
  edgeStyles?: { w: number; dash: number[]; color: [number, number, number] }[];
  softEdges: number[];
  groups: GroupInfo[];
  instances: InstanceInfo[];
  figures?: Figure[];
  components: ComponentInfo[];
  loose: number;
  activeGroup: number | null;
  editing: number | null;
  /** when editing a component: its faces' triangles (world space) and ids */
  editPositions: number[];
  editTriFace: number[];
  /** instance transform used to show the component being edited (column-major) */
  editTransform: number[];
  tags: Record<string, boolean>;
  /** a color per tag (Color by tag) */
  tagColors: Record<string, [number, number, number]>;
  /** line weight and style per tag (drawing sheets and the model view) */
  tagStyles?: Record<string, TagStyleInfo>;
  guides: GuideInfo[];
  views: SavedView[];
  annotations: Annotation[];
  units: string;
  doc: string;
  /** something to tell the person (a damaged file was set aside, a save failed) */
  notice?: string;
}

export interface Reply {
  ok: boolean;
  error?: string;
  /** the command worked but the model couldn't be saved (why) */
  save_error?: string;
  [k: string]: unknown;
}

export const inTauri = "__TAURI_INTERNALS__" in window;
const BRIDGE = `http://127.0.0.1:${import.meta.env.VITE_BRIDGE_PORT ?? 5175}`;

// In a plain browser the bridge needs its per-launch token. The dev server
// hands it over from the file the bridge wrote (see docs/dev/building.md);
// it is read again if the bridge was restarted.
let bridgeToken: Promise<string> | null = null;
const readToken = () => (bridgeToken ??= fetch("/__bridge-token", { cache: "no-store", headers: { "X-Sketchcraft-Dev": "1" } }).then((r) => (r.ok ? r.text() : "")).catch(() => ""));

async function http<T>(path: string, body?: unknown, retry = true): Promise<T> {
  const token = (await readToken()).trim();
  const r = await fetch(BRIDGE + path, { method: body === undefined ? "GET" : "POST", body: body === undefined ? undefined : JSON.stringify(body), headers: { "X-Sketchcraft-Token": token } });
  if (r.status === 403 && retry) {
    bridgeToken = null; // restarted bridge: new token
    return http<T>(path, body, false);
  }
  return r.json() as Promise<T>;
}

/** Run an engine command as the person using the app. */
export const exec = (cmd: Record<string, unknown>) => (inTauri ? invoke<Reply>("exec", { cmd }) : http<Reply>("/exec", cmd));
/**
 * Run an engine command that plugin code asked for (JavaScript or SketchUp
 * Ruby). The engine only lets these model: no files, programs, network or
 * plugin management (core/src/policy.rs). Anything a plugin triggers goes
 * through here, never through `exec`.
 */
export const execPlugin = (cmd: Record<string, unknown>, plugin?: string) =>
  inTauri ? invoke<Reply>("exec", { cmd, source: "plugin", plugin: plugin ?? null }) : http<Reply>(`/exec?as=plugin${plugin ? `&plugin=${encodeURIComponent(plugin.slice(0, 80))}` : ""}`, cmd);
export const loadScene = () => (inTauri ? invoke<Scene>("scene") : http<Scene>("/scene"));
export const sync = () => (inTauri ? invoke<boolean>("sync") : http<boolean>("/sync"));

/** The built-in library. `figure`: a scale figure (people, cyclists, flat cars), listed on its own. */
export const ASSETS: readonly { key: string; name: string; figure?: boolean }[] = [
  { key: "door", name: "Door" },
  { key: "window", name: "Window" },
  { key: "garage_door", name: "Garage" },
  { key: "tree", name: "Tree" },
  { key: "palm", name: "Palm" },
  { key: "car", name: "Car" },
  { key: "streetlight", name: "Streetlight" },
  { key: "bench", name: "Bench" },
  { key: "hydrant", name: "Hydrant" },
  { key: "person", name: "Person", figure: true },
  { key: "walker", name: "Walker", figure: true },
  { key: "shopper", name: "Shopper", figure: true },
  { key: "jogger", name: "Jogger", figure: true },
  { key: "child", name: "Child", figure: true },
  { key: "dog_walker", name: "Dog walker", figure: true },
  { key: "stroller", name: "Stroller", figure: true },
  { key: "wheelchair", name: "Wheelchair user", figure: true },
  { key: "cyclist", name: "Cyclist", figure: true },
  { key: "car_flat", name: "Car (flat)", figure: true },
  { key: "suv_flat", name: "SUV (flat)", figure: true },
  { key: "beaver", name: "Beaver", figure: true },
];

export interface CamSpec {
  eye: Vec3;
  target: Vec3;
  fov: number;
}

/** A timeline clip: scenes, turntable, walk or sun study. */
export type Clip = { name?: string | null; seconds: number } & (
  | { kind: "scenes"; views: string[]; hold?: number }
  | { kind: "turntable"; turns?: number; from?: CamSpec | null }
  | { kind: "walk"; keys?: CamSpec[]; path?: Vec3[]; eye_height?: number }
  | { kind: "sun"; date?: string; from?: string; to?: string; camera?: CamSpec | null }
);

export interface TimelineFrame extends CamSpec {
  sun?: [number, number];
  clock?: string;
  clip: number;
}
