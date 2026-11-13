// Thin bridge to the Rust engine. The UI never edits geometry itself: every
// change is an engine command, exactly what scripts and Claude send.
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

export const exec = (cmd: Record<string, unknown>) => invoke<Reply>("exec", { cmd });
export const loadScene = () => invoke<Scene>("scene");
export const sync = () => invoke<boolean>("sync");
