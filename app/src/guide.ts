// What to do next, for every tool and every step of it. The viewport shows
// this beside the model, so a tool explains itself while you use it.

import type { Tool } from "./Viewport";

export interface Guide {
  /** the tool's name */
  title: string;
  /** the one thing to do now */
  step: string;
  /** keys that matter right now: [key, what it does] */
  keys: [string, string][];
}

export interface GuideCtx {
  /** the step in progress (Op.t), and a detail for multi-click tools */
  op: string | null;
  stage?: number;
  selected: number;
  placing?: string | null;
}

const TYPE: [string, string] = ["Type", "an exact size, then Enter"];
const LOCK: [string, string] = ["→ ← ↑", "lock to red / green / blue"];
const SHIFT: [string, string] = ["Hold ⇧", "lock this direction"];
const ESC: [string, string] = ["Esc", "cancel"];

export function guide(tool: Tool, c: GuideCtx): Guide {
  const g = (title: string, step: string, keys: [string, string][] = []): Guide => ({ title, step, keys });
  const busy = c.op != null;
  switch (tool) {
    case "select":
      if (c.op === "box") return g("Select", "Release to pick. Left-to-right takes what's inside the box; right-to-left, what it touches.", [["⇧", "add or remove"]]);
      return c.selected
        ? g("Select", `${c.selected} selected. M moves, Q rotates, S scales, Delete erases; right-click for more.`, [["⇧ click", "add or remove"], ["Double-click", "open a group"], ["⌘T", "select none"]])
        : g("Select", "Click something to select it, or drag a box around things.", [["⇧ click", "add or remove"], ["Double-click", "open a group"], ["Space", "back here from any tool"]]);
    case "line":
      return busy ? g("Line", "Click the next point. Close the shape to make a face.", [TYPE, LOCK, SHIFT, ["Esc", "stop drawing"]]) : g("Line", "Click where the line starts.", [LOCK]);
    case "rect":
      return busy ? g("Rectangle", "Click the opposite corner.", [["Type", "width,depth (12',8') then Enter"], ESC]) : g("Rectangle", "Click the first corner, on the ground or on a face.", []);
    case "circle":
      return busy ? g("Circle", "Click to set the radius.", [["Type", "a radius, or 24s for 24 sides"], ESC]) : g("Circle", "Click the center, on the ground or on a face.", []);
    case "polygon":
      return busy ? g("Polygon", "Click to set its size.", [["Type", "a radius, or 8s for 8 sides"], ESC]) : g("Polygon", "Click the center.", []);
    case "arc":
      if (!busy) return g("Arc", "Click where the arc starts.", []);
      return c.stage === 1 ? g("Arc", "Click where it ends.", [TYPE, LOCK, ESC]) : g("Arc", "Pull out to bend it, then click.", [["Type", "how far it bows out"], ESC]);
    case "push":
      return busy ? g("Push/Pull", "Move to pull the face out or push it in, then click.", [["Type", "a distance, then Enter"], ESC]) : g("Push/Pull", "Click a face to pull it into 3D.", [["Double-click", "repeat the last distance"]]);
    case "offset":
      return busy ? g("Offset", "Move in or out, then click.", [TYPE, ESC]) : g("Offset", "Click a face to draw a copy of its outline inside (or outside) it.", []);
    case "followme":
      return g("Follow Me", "Select the path first (edges, or a face to go around), then click the profile face to sweep it along.", []);
    case "move":
      if (busy) return g("Move", "Click where it goes.", [["Type", "a distance, then Enter"], ["Option / Ctrl", "leave a copy (then type x5)"], LOCK, ESC]);
      return g("Move", c.selected ? "Click a point on the selection to pick it up there." : "Click something to grab it, or select first.", [["Option / Ctrl", "copy instead of move"]]);
    case "rotate":
      if (!busy) return g("Rotate", "Click the center of rotation (on a face, it turns in that face's plane).", [["Option / Ctrl", "rotate a copy"]]);
      return c.stage === 1 ? g("Rotate", "Click a point to start the angle from.", [ESC]) : g("Rotate", "Swing it around and click.", [["Type", "degrees, then Enter"], ESC]);
    case "scale":
      return busy ? g("Scale", "Move to resize, then click.", [["Type", "a factor (2 doubles it)"], ESC]) : g("Scale", c.selected ? "Click to start resizing the selection." : "Click something to resize it.", []);
    case "tape":
      return busy ? g("Tape Measure", "Click to drop a guide here (or read the distance).", [TYPE, LOCK, ESC]) : g("Tape Measure", "Click a point or an edge to measure from.", []);
    case "dimension":
      if (!busy) return g("Dimension", "Click the first point to measure from.", []);
      return c.stage === 1 ? g("Dimension", "Click the second point.", [LOCK, ESC]) : g("Dimension", "Pull the dimension out and click to place it.", [ESC]);
    case "text":
      return busy ? g("Text", "Click where the label goes.", [ESC]) : g("Text", "Click the point the label is about.", []);
    case "paint":
      return g("Paint Bucket", "Click a face or a group to paint it with the color (or texture) on the right.", [["⌘ / Option click", "pick up a color"], ["⇧ click", "paint every matching face"]]);
    case "erase":
      return g("Eraser", "Click an edge, face or group to delete it. Erasing an edge between two faces merges them.", [["⇧ click", "hide instead"], ["Option / Ctrl", "soften the edge"]]);
    case "orbit":
      return g("Orbit", "Drag to turn the view around the model.", [["Middle-drag", "orbit from any tool"], ["⇧Z", "see everything"]]);
    case "pan":
      return g("Pan", "Drag to slide the view.", [["Right-drag", "pan from any tool"]]);
    case "zoom":
      return g("Zoom", "Drag up to zoom in, down to zoom out.", [["Scroll", "zoom from any tool"], ["⇧Z", "see everything"]]);
    case "section":
      return g("Section", "Click a face: the model is cut just behind it so you can see inside. For floor plans, use Plan in the view bar.", []);
    case "place":
      return g("Place", `Click to drop ${c.placing ? c.placing.replace(/^file:.*\//, "").replace(/^component:\d+$/, "a copy").replace(/\.\w+$/, "").replace(/_/g, " ") : "it"}. On a wall, doors and windows face out of it.`, [["Esc", "done placing"]]);
    case "wall":
      return busy ? g("Wall", "Click the next corner. Click the first point to close a room.", [["Double-click / Enter", "finish"], TYPE, LOCK, ESC]) : g("Wall", "Click where the wall starts. Thickness and height are in the Library tab.", []);
    case "stairs":
      return busy ? g("Stairs", "Click the direction the stairs climb.", [ESC]) : g("Stairs", "Click where the bottom step starts.", []);
    case "roof":
      return g("Roof", "Click a flat top face, like the top of your walls.", []);
    case "column":
      return g("Column", "Click to drop a column.", []);
    case "smoove":
      return g("Smoove", "Click the terrain to raise it there.", [["⇧ click", "dig instead"]]);
    case "stamp":
      return g("Stamp", "Select a footprint face first, then click the terrain to flatten a pad under it.", []);
    case "drape":
      return g("Drape", "Select lines first, then click the terrain to lay them on it.", []);
    case "flipedge":
      return g("Flip Edge", "Click a diagonal between two triangles to swap it.", []);
  }
  return g(tool, "", []);
}
