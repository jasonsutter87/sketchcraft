// The Vertical layout's single right sidebar: Outliner | Library | Views | Properties, each taking the whole height.
// Properties is what the Classic layout shows in its lower-right panel (Document, Style, Paint, Finish, Entity info).
import { leftTabs, oneSidebar, type Layout, type LeftTab, type WorkspaceId } from "./workspace";

export type SideTab = LeftTab | "properties" | "ai";

const ALL: readonly SideTab[] = ["pages", "outliner", "library", "views", "properties", "ai"];
export const SIDE_TAB_KEY = "sketchcraft.sideTab";

/** The tabs a layout shows: Vertical adds Properties to the left panel's tabs; the others keep them as they were.
 *  With `ai` (the labs switch, labs.ts), SketchCraft AI is the last tab in every workspace and layout. */
export function panelTabs(ws: WorkspaceId, layout: Layout, ai = false): SideTab[] {
  const tabs: SideTab[] = oneSidebar(layout) ? [...leftTabs(ws), "properties"] : [...leftTabs(ws)];
  return ai ? [...tabs, "ai"] : tabs;
}

/** A remembered tab read back from storage: only a known tab name counts. */
export function readSideTab(raw: string | null): SideTab | null {
  return ALL.find((t) => t === raw) ?? null;
}

/** The tab to show: the chosen one if this layout and workspace have it, else the first. */
export function shownTab(tab: SideTab, tabs: readonly SideTab[]): SideTab {
  return tabs.includes(tab) ? tab : tabs[0];
}

/** The tab an arrow, Home or End key moves to from `cur` (wrapping), or null for any other key. */
export function stepTab(tabs: readonly SideTab[], cur: SideTab, key: string): SideTab | null {
  const i = tabs.indexOf(cur);
  if (i < 0 || tabs.length === 0) return null;
  if (key === "ArrowRight") return tabs[(i + 1) % tabs.length];
  if (key === "ArrowLeft") return tabs[(i + tabs.length - 1) % tabs.length];
  if (key === "Home") return tabs[0];
  if (key === "End") return tabs[tabs.length - 1];
  return null;
}
