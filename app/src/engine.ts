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
}

export interface GroupInfo {
  id: number;
  name: string;
  faces: number;
  tag: string | null;
  hidden: boolean;
}

export interface InstanceInfo {
  id: number;
  name: string;
  component: number;
  tag: string | null;
  hidden: boolean;
  transform: number[];
}

export interface ComponentInfo {
  id: number;
  name: string;
  placed: number;
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
}

export type Annotation =
  | { kind: "dimension"; id: number; a: Vec3; b: Vec3; offset: Vec3 }
  | { kind: "text"; id: number; at: Vec3; text: string; leader: Vec3 | null };

export interface Scene {
  positions: number[];
  colors: number[];
  backColors: number[];
  /** owner of each triangle: a face id, or a component instance id */
  triFace: number[];
  faces: FaceInfo[];
  edgeList: EdgeInfo[];
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
  guides: GuideInfo[];
  views: SavedView[];
  annotations: Annotation[];
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
