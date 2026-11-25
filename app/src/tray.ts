// The Classic tray: a right-hand column of collapsible panels, in the order a SketchUp user expects. Each entry
// maps to a panel SketchCraft already has; the tray only stacks them. This file is the pure part: the order, the
// defaults, and reading and writing which panels are open (kept in saved settings).
import { store } from "./skins";

export type TrayPanel = "entity" | "materials" | "components" | "styles" | "environments" | "tags" | "scenes" | "outliner";

/** Top to bottom. Shadows and Soften Edges (also in a SketchUp tray) have no panel in SketchCraft yet:
 *  Shadows is a view bar button, softening is on the right-click menu and the Eraser's modifier keys. */
export const TRAY_ORDER: readonly TrayPanel[] = ["entity", "materials", "components", "styles", "environments", "tags", "scenes", "outliner"];

export const TRAY_TITLES: Record<TrayPanel, string> = {
  entity: "Entity Info",
  materials: "Materials",
  components: "Components",
  styles: "Styles",
  environments: "Environments",
  tags: "Tags",
  scenes: "Scenes",
  outliner: "Outliner",
};

export const TRAY_KEY = "sketchcraft.tray";
export const TRAY_OPEN_KEY = "sketchcraft.trayOpen";

/** open or closed before you have chosen: the two you reach for first */
export const TRAY_DEFAULT_OPEN: Record<TrayPanel, boolean> = { entity: true, materials: true, components: false, styles: false, environments: false, tags: false, scenes: false, outliner: false };

/** Is the tray switched on? Only "1" counts; the default is off (SketchCraft's own look stays as it was). */
export const readTrayOn = (raw: string | null): boolean => raw === "1";

/** Which panels are open, read back from saved settings: a panel counts only if it is a known one with a true or
 *  false; anything else (a hand edit, another version) keeps its default. */
export function readTrayOpen(raw: string | null): Record<TrayPanel, boolean> {
  const out = { ...TRAY_DEFAULT_OPEN };
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw ?? "null");
  } catch {
    return out;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return out;
  for (const id of TRAY_ORDER) {
    const v = (parsed as Record<string, unknown>)[id];
    if (typeof v === "boolean") out[id] = v;
  }
  return out;
}

export const toggleTrayPanel = (open: Record<TrayPanel, boolean>, id: TrayPanel): Record<TrayPanel, boolean> => ({ ...open, [id]: !open[id] });

export const loadTrayOn = (): boolean => readTrayOn(store.get(TRAY_KEY));
export const saveTrayOn = (on: boolean) => store.set(TRAY_KEY, on ? "1" : "0");
export const loadTrayOpen = (): Record<TrayPanel, boolean> => readTrayOpen(store.get(TRAY_OPEN_KEY));
export const saveTrayOpen = (open: Record<TrayPanel, boolean>) => store.set(TRAY_OPEN_KEY, JSON.stringify(open));

/** The tray shows with the Classic layout only (the other layouts have their own panels). */
export const trayShown = (layout: string, on: boolean): boolean => layout === "classic" && on;
