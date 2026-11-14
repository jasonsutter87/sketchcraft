// Workspaces: the app arranged for a kind of work. Each picks the tools on
// the toolbar, which panels show, the strip of quick actions over the model,
// and a few view styles. Every one can be customized; your changes are kept.

import type { Styles, Tool } from "./Viewport";

export type WorkspaceId = "model" | "arch" | "design" | "previz";
export type Dock = "bottom" | "left";
export type LeftTab = "outliner" | "library" | "views";

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
export const TOOL_INFO: Record<Exclude<Tool, "place">, { name: string; key: string }> = {
  select: { name: "Select", key: "Space" },
  line: { name: "Line", key: "L" },
  arc: { name: "Arc", key: "A" },
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
    tools: [["select"], ["line", "arc", "rect", "circle", "polygon"], ["push", "offset", "followme"], ["move", "rotate", "scale"], ["tape", "dimension", "text", "paint", "erase"], NAV],
    leftTab: "outliner",
    showLeft: true,
    showRight: true,
    dock: "bottom",
  },
  {
    id: "arch",
    name: "Arch",
    blurb: "Buildings: walls, stairs, roofs, floor-by-floor plans, blueprints and sheets.",
    tools: [["select"], ["wall", "stairs", "roof", "column"], ["line", "rect", "push", "offset"], ["move", "rotate"], ["tape", "dimension", "text", "paint", "erase"], NAV],
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
];

/** your changes to a workspace (only what differs) */
export type WorkspaceEdits = Partial<Pick<Workspace, "tools" | "showLeft" | "showRight" | "dock" | "leftTab">>;

export function withEdits(w: Workspace, e: WorkspaceEdits | undefined): Workspace {
  return e ? { ...w, ...e } : w;
}
