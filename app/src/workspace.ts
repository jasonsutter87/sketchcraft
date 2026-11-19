// Workspaces: the app arranged for a kind of work. Each picks the tools on
// the toolbar, which panels show, the strip of quick actions over the model,
// and a few view styles. Every one can be customized; your changes are kept.

import type { Styles, Tool } from "./Viewport";

export type WorkspaceId = "model" | "arch" | "design" | "previz" | "layout";
/** where the tool bar sits: floating along the bottom, down the left, or
 * "classic": docked edge to edge above the model, like SketchUp */
export type Dock = "bottom" | "left" | "classic";
export type Layout = "classic" | "floating" | "side";
export type LeftTab = "outliner" | "library" | "views" | "pages";

export interface Workspace {
  id: WorkspaceId;
  name: string;
  /** what it's for, shown in the switcher's tooltip and the customize dialog */
  blurb: string;
  tools: Tool[][];
  leftTab: LeftTab;
  showLeft: boolean;
  showRight: boolean;
  dock: Dock;
  /** view styles it turns on when you switch to it */
  styles?: Partial<Styles>;
}

/** every tool that can sit on the toolbar, with its name and key */
export const TOOL_INFO: Record<Exclude<Tool, "place" | "plugin">, { name: string; key: string }> = {
  select: { name: "Select", key: "Space" },
  line: { name: "Line", key: "L" },
  arc: { name: "2-Point Arc: ends, then bulge", key: "A" },
  arccenter: { name: "Arc: center, start, then swing round", key: "" },
  arc3: { name: "3-Point Arc: start, a point on it, end", key: "" },
  pie: { name: "Pie: center, start, then swing round (a slice face)", key: "" },
  rrect: { name: "Rotated Rectangle: one side, then the other", key: "" },
  freehand: { name: "Freehand: drag to draw", key: "" },
  rect: { name: "Rectangle", key: "R" },
  circle: { name: "Circle", key: "C" },
  polygon: { name: "Polygon", key: "N" },
  push: { name: "Push / Pull", key: "P" },
  offset: { name: "Offset", key: "F" },
  followme: { name: "Follow Me", key: "J" },
  move: { name: "Move (Option/Ctrl to copy)", key: "M" },
  rotate: { name: "Rotate (Option/Ctrl to copy)", key: "Q" },
  scale: { name: "Scale", key: "S" },
  tape: { name: "Tape Measure (makes guides)", key: "T" },
  protractor: { name: "Protractor: an angled guide (center, base, angle)", key: "" },
  axes: { name: "Axes: move and turn the drawing axes (origin, red, green)", key: "" },
  dimension: { name: "Dimension: click, click, pull out", key: "D" },
  text: { name: "Text label: click a point, then where the label goes", key: "W" },
  paint: { name: "Paint Bucket (⌘/Option-click samples, Shift-click paints all matching)", key: "B" },
  erase: { name: "Eraser (Shift hides, Option/Ctrl softens, both unsoften)", key: "E" },
  orbit: { name: "Orbit (or middle-drag / hold Space)", key: "O" },
  pan: { name: "Pan (or Shift+middle-drag / right-drag)", key: "H" },
  zoom: { name: "Zoom (drag up/down; Shift+Z zooms extents)", key: "Z" },
  section: { name: "Section cut: click a face", key: "X" },
  zoomwindow: { name: "Zoom Window: drag a box to fill the view", key: "" },
  position: { name: "Position Camera: click where to stand", key: "" },
  lookaround: { name: "Look Around: drag to turn your head", key: "" },
  walk: { name: "Walk: drag, or arrow keys / WASD (Shift runs)", key: "" },
  wall: { name: "Wall: click corners, double-click to finish", key: "" },
  room: { name: "Room: drag out a room; walls and a floor appear", key: "" },
  wallpull: { name: "Pull wall: drag a wall sideways; the walls joined to it stretch", key: "" },
  stairs: { name: "Stairs: click the bottom, then the direction", key: "" },
  roof: { name: "Roof: click a flat top", key: "" },
  column: { name: "Column", key: "" },
  smoove: { name: "Smoove terrain (Shift digs)", key: "" },
  stamp: { name: "Stamp a pad into terrain", key: "" },
  drape: { name: "Drape lines on terrain", key: "" },
  flipedge: { name: "Flip a terrain edge", key: "" },
};

const NAV: Tool[] = ["orbit", "pan", "zoom", "zoomwindow", "section"];
const CAMERA: Tool[] = ["orbit", "pan", "zoom", "zoomwindow", "position", "lookaround", "walk"];

export const WORKSPACES: Workspace[] = [
  {
    id: "model",
    name: "Model",
    blurb: "Every modeling tool, the way SketchUp lays them out.",
    tools: [["select"], ["line", "freehand", "arc", "arccenter", "arc3", "pie", "rect", "rrect", "circle", "polygon"], ["push", "offset", "followme"], ["move", "rotate", "scale"], ["tape", "protractor", "axes", "dimension", "text", "paint", "erase"], NAV],
    leftTab: "outliner",
    showLeft: true,
    showRight: true,
    dock: "bottom",
  },
  {
    id: "arch",
    name: "Arch",
    blurb: "Buildings: walls, stairs, roofs, floor-by-floor plans, blueprints and sheets.",
    tools: [["select"], ["room", "wall", "wallpull", "stairs", "roof", "column"], ["line", "rect", "push", "offset"], ["move", "rotate"], ["tape", "dimension", "text", "paint", "erase"], NAV],
    leftTab: "library",
    showLeft: true,
    showRight: true,
    dock: "bottom",
  },
  {
    id: "design",
    name: "Design",
    blurb: "Products and furniture: solids, booleans, bevels and rounds, exact sizes.",
    tools: [["select"], ["line", "arc", "rect", "circle", "polygon"], ["push", "offset", "followme"], ["move", "rotate", "scale"], ["tape", "dimension", "paint", "erase"], NAV],
    leftTab: "outliner",
    showLeft: true,
    showRight: true,
    dock: "left",
    styles: { xray: false, edges: true },
  },
  {
    id: "previz",
    name: "Previz",
    blurb: "Presenting: scenes, camera moves, sun and shadows, photoreal renders.",
    tools: [["select"], ["move", "rotate", "scale"], ["paint"], CAMERA, ["section"]],
    leftTab: "views",
    showLeft: true,
    showRight: false,
    dock: "bottom",
    styles: { shadows: true },
  },
  {
    id: "layout",
    name: "Layout",
    blurb: "Drawing sheets: pages of plans, elevations and views, to scale, ready to print. Your 3D model stays open.",
    // the paper has its own tools (#9); the model's tool bar is put away while you're here
    tools: [["select"]],
    leftTab: "pages",
    showLeft: true,
    showRight: true,
    dock: "bottom",
  },
];

/** The left panel's tabs in a workspace: Layout leads with its pages (the model's Library isn't for paper). */
export function leftTabs(id: WorkspaceId): LeftTab[] {
  return id === "layout" ? ["pages", "outliner", "views"] : ["outliner", "library", "views"];
}

/**
 * The tool to have after switching to `w`: the one you had if this workspace's tool bar has it, else Select. Layout
 * keeps the model's tool as it is (its tool bar is put away, not changed), so going back finds it where you left it.
 */
export function toolAfterSwitch(w: Workspace, tool: Tool): Tool {
  if (w.id === "layout") return tool;
  return w.tools.flat().includes(tool) ? tool : "select";
}

/** your changes to a workspace (only what differs) */
export type WorkspaceEdits = Partial<Pick<Workspace, "tools" | "showLeft" | "showRight" | "dock" | "leftTab">>;

/** Edits read back from storage: only well-formed parts are kept (a bad value must never stop the app starting). */
export function cleanEdits(raw: unknown): Partial<Record<WorkspaceId, WorkspaceEdits>> {
  const out: Partial<Record<WorkspaceId, WorkspaceEdits>> = {};
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return out;
  for (const w of WORKSPACES) {
    const e = (raw as Record<string, unknown>)[w.id];
    if (!e || typeof e !== "object" || Array.isArray(e)) continue;
    const r = e as Record<string, unknown>;
    const c: WorkspaceEdits = {};
    if (Array.isArray(r.tools) && r.tools.length <= 40 && r.tools.every((g) => Array.isArray(g) && g.length <= 80 && g.every((t) => typeof t === "string" && Object.hasOwn(TOOL_INFO, t)))) c.tools = r.tools as Tool[][];
    if (typeof r.showLeft === "boolean") c.showLeft = r.showLeft;
    if (typeof r.showRight === "boolean") c.showRight = r.showRight;
    if (r.dock === "bottom" || r.dock === "left" || r.dock === "classic") c.dock = r.dock;
    if (r.leftTab === "outliner" || r.leftTab === "library" || r.leftTab === "views" || r.leftTab === "pages") c.leftTab = r.leftTab;
    if (Object.keys(c).length) out[w.id] = c;
  }
  return out;
}

/** A workspace with your changes. A per-workspace dock overrides the layout default. */
export function withEdits(w: Workspace, e: WorkspaceEdits | undefined, layout: Layout = "floating"): Workspace {
  const dock = layout === "classic" ? "classic" : layout === "side" ? "left" : w.dock;
  return { ...w, dock, ...e };
}
