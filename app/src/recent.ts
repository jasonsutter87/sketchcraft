// The models opened or saved lately (the welcome screen lists them). Plain storage, no browser globals, so it tests
// in Node.
import { isModelPath } from "./modelpath";

const RECENT = "sketchcraft.recent";
/** How many recent models the welcome screen lists. */
export const MAX_RECENT = 8;

export interface RecentFile {
  path: string;
  /** when it was last opened or saved (ms since 1970) */
  at: number;
}

/** The models opened or saved lately, newest first: only .sketchcraft paths survive (storage can hold anything). */
export function recentFiles(store: Pick<Storage, "getItem"> | undefined = globalThis.localStorage): RecentFile[] {
  try {
    const v = JSON.parse(store?.getItem(RECENT) ?? "[]");
    if (!Array.isArray(v)) return [];
    return v
      .filter((x): x is RecentFile => !!x && typeof x === "object" && isModelPath((x as RecentFile).path) && Number.isFinite((x as RecentFile).at))
      .slice(0, MAX_RECENT);
  } catch {
    return [];
  }
}

/** A model was opened or saved: it goes to the top of the list (once). */
export function addRecent(path: string, at = Date.now(), store: Pick<Storage, "getItem" | "setItem"> | undefined = globalThis.localStorage) {
  if (!isModelPath(path)) return;
  const list = [{ path, at }, ...recentFiles(store).filter((r) => r.path !== path)].slice(0, MAX_RECENT);
  try {
    store?.setItem(RECENT, JSON.stringify(list));
  } catch {
    /* storage unavailable: no list */
  }
}

/** Take one off the list (it couldn't be opened any more). */
export function forgetRecent(path: string, store: Pick<Storage, "getItem" | "setItem"> | undefined = globalThis.localStorage) {
  try {
    store?.setItem(RECENT, JSON.stringify(recentFiles(store).filter((r) => r.path !== path)));
  } catch {
    /* storage unavailable */
  }
}
