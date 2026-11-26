// File pickers: native dialogs in the desktop app; in a browser, the path typed into the
// app's own text box (the browser's prompt() is unstyled and blocks the page).
import { open, save } from "@tauri-apps/plugin-dialog";
import { ask } from "./ask";

const inTauri = "__TAURI_INTERNALS__" in window;

/** A typed path, trimmed; nothing typed is the same as Cancel. */
const nonEmpty = (v: string | null) => v?.trim() || null;

export async function pickOpen(): Promise<string | null> {
  if (!inTauri) return nonEmpty(await ask("Open which file?", "", "The full path on this computer: .sketchcraft, .skp, .dwg, .dxf, .ifc, .fbx, .obj, or a .ply / .las / .xyz / .pts scan."));
  const r = await open({
    multiple: false,
    filters: [
      { name: "Models, drawings and scans", extensions: ["sketchcraft", "skp", "dwg", "dxf", "ifc", "fbx", "obj", "ply", "las", "xyz", "pts"] },
      { name: "SketchCraft", extensions: ["sketchcraft"] },
      { name: "SketchUp", extensions: ["skp"] },
      { name: "AutoCAD (DWG, DXF)", extensions: ["dwg", "dxf"] },
      { name: "IFC (Revit, ArchiCAD, BIM)", extensions: ["ifc"] },
      { name: "FBX (binary)", extensions: ["fbx"] },
      { name: "OBJ", extensions: ["obj"] },
      { name: "Point clouds (PLY, LAS, XYZ, PTS, TXT)", extensions: ["ply", "las", "laz", "xyz", "pts", "txt"] },
    ],
  });
  return typeof r === "string" ? r : null;
}

export async function pickSave(defaultName: string, ext: string, label: string): Promise<string | null> {
  if (!inTauri) return nonEmpty(await ask("Save to which path?", defaultName, `The full path on this computer, ending in .${ext}.`));
  const r = await save({ defaultPath: defaultName, filters: [{ name: label, extensions: [ext] }] });
  return r ?? null;
}

const KEY = "sketchcraft.currentFile";

/** A path that ⌘S may write to: a SketchCraft model file, never something else the storage was made to name. */
export function isModelPath(p: string | null | undefined): p is string {
  return !!p && p.length < 4096 && !p.includes("\0") && p.toLowerCase().endsWith(".sketchcraft");
}

export function rememberedFile(): string | null {
  try {
    const p = localStorage.getItem(KEY);
    return isModelPath(p) ? p : null;
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
