// Thin bridge to the Rust engine. The UI never edits geometry itself: every
// change is an engine command, exactly what scripts and Claude send.
//
// Inside the desktop app this calls Tauri; in a plain browser it talks to
// `sketchcraft bridge` on 127.0.0.1:5175.
import { invoke } from "@tauri-apps/api/core";

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
  soft: boolean;
  group: number | null;
  /** edges of one arc, circle or welded curve share this */
  curve?: number | null;
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
}

export interface ComponentInfo {
  id: number;
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

export interface Scene {
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
  /** owner of each triangle: a face id, or a component instance id */
  triFace: number[];
  faces: FaceInfo[];
  edgeList: EdgeInfo[];
  /** video clips, played in order */
  timeline?: { clips: Clip[] };
  edges: number[];
  edgeIds: number[];
  softEdges: number[];
  groups: GroupInfo[];
  instances: InstanceInfo[];
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
  guides: GuideInfo[];
  views: SavedView[];
  annotations: Annotation[];
  units: string;
  doc: string;
}

export interface Reply {
  ok: boolean;
  error?: string;
  [k: string]: unknown;
}

const inTauri = "__TAURI_INTERNALS__" in window;
const BRIDGE = "http://127.0.0.1:5175";

async function http<T>(path: string, body?: unknown): Promise<T> {
  const r = await fetch(BRIDGE + path, body === undefined ? undefined : { method: "POST", body: JSON.stringify(body) });
  return r.json() as Promise<T>;
}

export const exec = (cmd: Record<string, unknown>) => (inTauri ? invoke<Reply>("exec", { cmd }) : http<Reply>("/exec", cmd));
export const loadScene = () => (inTauri ? invoke<Scene>("scene") : http<Scene>("/scene"));
export const sync = () => (inTauri ? invoke<boolean>("sync") : http<boolean>("/sync"));

export const ASSETS = [
  { key: "door", name: "Door" },
  { key: "window", name: "Window" },
  { key: "garage_door", name: "Garage" },
  { key: "tree", name: "Tree" },
  { key: "palm", name: "Palm" },
  { key: "car", name: "Car" },
  { key: "streetlight", name: "Streetlight" },
  { key: "bench", name: "Bench" },
  { key: "hydrant", name: "Hydrant" },
] as const;

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
