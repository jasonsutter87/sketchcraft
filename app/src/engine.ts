// Thin bridge to the Rust engine. The UI never edits geometry itself: every
// change is an engine command, exactly what scripts and Claude send.
//
// Inside the desktop app this calls Tauri; in a plain browser it talks to
// `riseline bridge` on 127.0.0.1:5175.
import { invoke } from "@tauri-apps/api/core";

export type Vec3 = [number, number, number];

export interface FaceInfo {
  id: number;
  normal: Vec3;
  outer: Vec3[];
  group: number | null;
  area: number;
  material: string | null;
}

export interface GroupInfo {
  id: number;
  name: string;
  faces: number;
}

export interface Scene {
  positions: number[];
  colors: number[];
  triFace: number[];
  faces: FaceInfo[];
  edges: number[];
  groups: GroupInfo[];
  loose: number;
  activeGroup: number | null;
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
