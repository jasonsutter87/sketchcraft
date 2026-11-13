// File pickers: native dialogs in the desktop app, typed paths in a browser.
import { open, save } from "@tauri-apps/plugin-dialog";

const inTauri = "__TAURI_INTERNALS__" in window;

export async function pickOpen(): Promise<string | null> {
  if (!inTauri) return window.prompt("Open which file? (full path to .sketchcraft, .skp or .obj)");
  const r = await open({
    multiple: false,
    filters: [
      { name: "Models", extensions: ["sketchcraft", "skp", "obj"] },
      { name: "SketchCraft", extensions: ["sketchcraft"] },
      { name: "SketchUp (6-8)", extensions: ["skp"] },
      { name: "OBJ", extensions: ["obj"] },
    ],
  });
  return typeof r === "string" ? r : null;
}

export async function pickSave(defaultName: string, ext: string, label: string): Promise<string | null> {
  if (!inTauri) return window.prompt(`Save to which path? (.${ext})`, defaultName);
  const r = await save({ defaultPath: defaultName, filters: [{ name: label, extensions: [ext] }] });
  return r ?? null;
}

const KEY = "sketchcraft.currentFile";

export function rememberedFile(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function rememberFile(path: string | null) {
  try {
    if (path) localStorage.setItem(KEY, path);
    else localStorage.removeItem(KEY);
  } catch {
    /* storage unavailable: the title just won't persist */
  }
}
