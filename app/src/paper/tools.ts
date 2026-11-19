// The sheet editor's tools (design §5): what each one is called, its key, its icon (shown on the button and as the
// cursor), the hint for each step, and its state machine. Pure: a tool takes events (a click on paper, already
// snapped, a move, Enter, Escape...) and answers its next state and, when a gesture is done, the one engine command
// it makes. PaperView feeds it and runs the command; app/tests/sheeteditor.test.ts drives it directly.

import { dimModeFor, dimOffsetAt, tableFromFrame, TEXT_HEIGHT } from "./items";
import { MIN_SIZE, rectOf, roundP, roundRect, type P, type Rect } from "./snap";

export type PaperTool = "select" | "viewport" | "line" | "polyline" | "rect" | "arc" | "text" | "table" | "dimension" | "trim" | "extend" | "offset" | "erase";

/** Icons, 24×24 strokes: the tool bar's buttons and the cursor's badge. */
const ICON: Record<PaperTool, string> = {
  select: "M5 3l13 7-6 1.5L9 18z",
  viewport: "M3 5h18v14H3zM3 9h18M8 13h5v3H8z",
  line: "M4 20L20 4M4 20m-1.5 0a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0M20 4m-1.5 0a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0-3 0",
  polyline: "M3 18l6-9 5 6 7-11",
  rect: "M4 6h16v12H4z",
  arc: "M4 18a9 9 0 0 1 16 0",
  text: "M5 5h14M12 5v14M9 19h6",
  table: "M3 5h18v14H3zM3 9.5h18M3 14h18M9.5 5v14M15.5 5v14",
  dimension: "M3 8v8M21 8v8M3 12h18M6 9l-3 3 3 3M18 9l3 3-3 3",
  trim: "M6 6a2.5 2.5 0 1 0 0.1 0M6 18a2.5 2.5 0 1 0 0.1 0M8 8l12 9M8 16l12-9",
  extend: "M3 12h11M14 12l-3-3M14 12l-3 3M20 4v16",
  offset: "M4 16L16 4M8 20L20 8",
  erase: "M15 4l5 5-9 9H6l-3-3zM10 9l5 5M6 18h14",
};

export interface ToolInfo {
  name: string;
  /** the shortcut (one key, as `KeyboardEvent.key` gives it, lower case) */
  key: string;
  /** what the button's tooltip says it does */
  tip: string;
  icon: string;
}

/** Every paper tool, in the tool bar's order (`PAPER_TOOL_INFO`, as the model has `TOOL_INFO`). */
export const PAPER_TOOL_INFO: Record<PaperTool, ToolInfo> = {
  select: { name: "Select", key: "v", tip: "Pick, move and resize what's on the page", icon: ICON.select },
  viewport: { name: "Viewport", key: "w", tip: "Drag a frame, then pick the view of the model it shows", icon: ICON.viewport },
  line: { name: "Line", key: "l", tip: "Click two points; it carries on from the end", icon: ICON.line },
  polyline: { name: "Polyline", key: "p", tip: "Click points; double-click or Enter to finish, click the first to close", icon: ICON.polyline },
  rect: { name: "Rectangle", key: "r", tip: "Drag (or click two corners)", icon: ICON.rect },
  arc: { name: "Arc", key: "a", tip: "Click its start, its end, then how far it bends", icon: ICON.arc },
  text: { name: "Text", key: "t", tip: "Click to place, then type; drag a width for a text block that wraps; double-click text to change it", icon: ICON.text },
  table: { name: "Table", key: "b", tip: "Click (or drag a frame) for a table, then type in its cells; paste from a spreadsheet", icon: ICON.table },
  dimension: { name: "Dimension", key: "d", tip: "Click points of the model; keep clicking to chain; Shift for baseline", icon: ICON.dimension },
  trim: { name: "Trim", key: "x", tip: "Click the piece of a line to cut away (Shift extends instead)", icon: ICON.trim },
  extend: { name: "Extend", key: "e", tip: "Click near the end of a line to run it on to the next line", icon: ICON.extend },
  offset: { name: "Offset", key: "o", tip: "Click a line, then where its copy goes (type a distance)", icon: ICON.offset },
  erase: { name: "Delete", key: "k", tip: "Click something on the page to delete it", icon: ICON.erase },
};
export const PAPER_TOOLS = Object.keys(PAPER_TOOL_INFO) as PaperTool[];
/** The tool bar's groups (a gap between each). */
export const TOOL_GROUPS: PaperTool[][] = [["select", "viewport"], ["line", "polyline", "rect", "arc"], ["text", "table", "dimension"], ["trim", "extend", "offset", "erase"]];

export const toolForKey = (key: string): PaperTool | null => PAPER_TOOLS.find((t) => PAPER_TOOL_INFO[t].key === key.toLowerCase()) ?? null;

/**
 * The cursor for a tool: a crosshair with the tool's icon beside it (white under black, so it shows on paper and on
 * the desk). Select uses the normal arrow.
 */
export function cursorFor(tool: PaperTool): string {
  if (tool === "select") return "default";
  const icon = PAPER_TOOL_INFO[tool].icon;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><g stroke="#fff" stroke-width="3" stroke-linecap="round"><path d="M8 1v14M1 8h14"/></g><g stroke="#000" stroke-width="1.2" stroke-linecap="round"><path d="M8 1v14M1 8h14"/></g><g transform="translate(15 15) scale(0.68)" fill="none" stroke-linecap="round" stroke-linejoin="round"><path d="${icon}" stroke="#fff" stroke-width="4.5"/><path d="${icon}" stroke="#000" stroke-width="1.8"/></g></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") 8 8, crosshair`;
}

// ---- state machines -------------------------------------------------------------------------------------------------

export type DimChoice = "auto" | "aligned" | "horizontal" | "vertical";
export interface DimPick {
  p: P;
  anchor: unknown;
  viewport?: number;
}
export type ToolState =
  | { t: "idle" }
  /** a frame being dragged (viewport, rectangle); `click`: started with a click, finishes on the next */
  | { t: "drag"; from: P; to: P; click: boolean }
  /** points clicked so far (line, polyline, arc) and where the mouse is */
  | { t: "pts"; pts: P[]; at: P }
  /** dimension points picked; `baseline`: a Shift+click asked for a baseline set */
  | { t: "dim"; picks: DimPick[]; at: P; baseline?: boolean }
  /** the line to offset is picked; the mouse says where the copy goes */
  | { t: "offset"; item: number; at: P; typed: string }
  /** a viewport frame drawn: which view goes in it is being chosen */
  | { t: "choose"; rect: Rect }
  /** typing a text in place (new, or `id` being changed); `width`: a text block wrapped to it */
  | { t: "text"; at: P; id?: number; text: string; width?: number }
  /** typing in a table's cell */
  | { t: "cell"; id: number; row: number; col: number; text: string };

export const IDLE: ToolState = { t: "idle" };

export interface ToolEvent {
  type: "down" | "move" | "up" | "dbl" | "enter" | "esc" | "back";
  /** the paper point, already snapped */
  p?: P;
  shift?: boolean;
  /** the item under the mouse (line work, text, dimension), if any */
  hit?: number | null;
  /** a model point snapped to (`viewport_points`): what a dimension attaches to */
  anchor?: { anchor: unknown; viewport?: number } | null;
  /** offset: how far the mouse is from the picked line, mm */
  dist?: number;
  /** a typed character (offset distance) */
  ch?: string;
}
export interface ToolCtx {
  sheet: number;
  dimChoice: DimChoice;
  /** the snap reach, mm: how near the first point a click closes a polyline (half of it: the same place) */
  tol: number;
}
export interface ToolOut {
  s: ToolState;
  /** the engine command the finished gesture makes */
  cmd?: Record<string, unknown>;
  /** something to tell the person (a refused click) */
  say?: string;
}

/** The narrowest text block a drag makes (narrower: a click, plain text). */
export const MIN_BLOCK = 5;

const near = (a: P, b: P, tol: number) => Math.hypot(a[0] - b[0], a[1] - b[1]) <= tol;

/** One event into a tool: its next state and, maybe, a command. Select is handled by PaperView (it needs the items). */
export function toolStep(tool: PaperTool, s: ToolState, e: ToolEvent, c: ToolCtx): ToolOut {
  if (e.type === "esc") return { s: IDLE };
  const p = e.p;
  switch (tool) {
    case "viewport":
    case "rect": {
      if (s.t === "choose") return { s };
      if (e.type === "down" && p) {
        if (s.t === "drag" && s.click) return finishFrame(tool, rectOf(s.from, p), c);
        return { s: { t: "drag", from: p, to: p, click: false } };
      }
      if (e.type === "move" && p && s.t === "drag") return { s: { ...s, to: p } };
      if (e.type === "up" && p && s.t === "drag" && !s.click) {
        const r = rectOf(s.from, p);
        // a click (no drag): the next click is the other corner
        if (r[2] < 1 && r[3] < 1) return { s: { ...s, to: p, click: true } };
        return finishFrame(tool, r, c);
      }
      return { s };
    }
    case "line": {
      if (e.type === "down" && p) {
        if (s.t !== "pts" || !s.pts.length) return { s: { t: "pts", pts: [p], at: p } };
        const a = s.pts[0];
        // (the second click of a double-click lands where the first did: nothing to draw)
        if (near(a, p, c.tol / 2)) return { s };
        // it carries on from where this one ended
        return { s: { t: "pts", pts: [p], at: p }, cmd: { cmd: "sheet_line", sheet: c.sheet, kind: "line", pts_mm: [roundP(a), roundP(p)] } };
      }
      if (e.type === "move" && p && s.t === "pts") return { s: { ...s, at: p } };
      if (e.type === "enter" || e.type === "dbl") return { s: IDLE };
      return { s };
    }
    case "polyline": {
      const finish = (pts: P[]): ToolOut => (pts.length >= 2 ? { s: IDLE, cmd: { cmd: "sheet_line", sheet: c.sheet, kind: "polyline", pts_mm: pts.map(roundP) } } : { s: IDLE });
      if (e.type === "down" && p) {
        if (s.t !== "pts") return { s: { t: "pts", pts: [p], at: p } };
        // clicking the first point closes it
        if (s.pts.length >= 3 && near(s.pts[0], p, c.tol)) return finish([...s.pts, s.pts[0]]);
        if (near(s.pts[s.pts.length - 1], p, c.tol / 2)) return { s };
        return { s: { ...s, pts: [...s.pts, p], at: p } };
      }
      if (e.type === "move" && p && s.t === "pts") return { s: { ...s, at: p } };
      // a double-click's own second click added nothing (same place), so the points are as they were
      if ((e.type === "enter" || e.type === "dbl") && s.t === "pts") return finish(s.pts);
      if (e.type === "back" && s.t === "pts") return s.pts.length > 1 ? { s: { ...s, pts: s.pts.slice(0, -1) } } : { s: IDLE };
      return { s };
    }
    case "arc": {
      if (e.type === "down" && p) {
        if (s.t !== "pts") return { s: { t: "pts", pts: [p], at: p } };
        if (near(s.pts[s.pts.length - 1], p, c.tol / 2)) return { s };
        if (s.pts.length === 1) return { s: { ...s, pts: [s.pts[0], p], at: p } };
        // start, end, then the bend: the engine takes start, a point on it, end
        const [a, b] = s.pts;
        const cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if (Math.abs(cross) < 1e-6) return { s, say: "That point is on the straight line: move off it to bend the arc" };
        return { s: IDLE, cmd: { cmd: "sheet_line", sheet: c.sheet, kind: "arc", pts_mm: [roundP(a), roundP(p), roundP(b)] } };
      }
      if (e.type === "move" && p && s.t === "pts") return { s: { ...s, at: p } };
      if (e.type === "back" && s.t === "pts") return s.pts.length > 1 ? { s: { ...s, pts: s.pts.slice(0, -1) } } : { s: IDLE };
      return { s };
    }
    case "dimension": {
      const picks = s.t === "dim" ? s.picks : [];
      if (e.type === "down" && p) {
        if (!e.anchor) return { s: s.t === "dim" ? s : { t: "dim", picks: [], at: p }, say: "Click a point of the model: a corner or a middle (a marker shows when you're on one)" };
        const last = picks[picks.length - 1];
        if (last && near(last.p, p, c.tol / 2)) return { s };
        return { s: { t: "dim", picks: [...picks, { p, anchor: e.anchor.anchor, viewport: e.anchor.viewport }], at: p, baseline: (s.t === "dim" && s.baseline) || !!e.shift } };
      }
      if (e.type === "move" && p && s.t === "dim") return { s: { ...s, at: p } };
      if ((e.type === "enter" || e.type === "dbl") && s.t === "dim") {
        if (picks.length < 2) return { s: IDLE, say: picks.length ? "A dimension needs two points at least" : undefined };
        return { s: IDLE, cmd: dimCommand(picks, e.p ?? s.at, !!e.shift || !!s.baseline, c) };
      }
      if (e.type === "back" && s.t === "dim") return picks.length > 1 ? { s: { ...s, picks: picks.slice(0, -1) } } : { s: IDLE };
      return { s };
    }
    case "trim":
    case "extend":
    case "erase": {
      if (e.type !== "down" || !p) return { s };
      if (e.hit == null) return { s, say: tool === "erase" ? "Nothing there to delete" : "Click on a piece of paper line work" };
      if (tool === "erase") return { s, cmd: { cmd: "sheet_item_delete", ids: [e.hit] } };
      const extend = tool === "extend" || !!e.shift;
      return { s, cmd: { cmd: extend ? "sheet_extend" : "sheet_trim", item: e.hit, at_mm: roundP(p) } };
    }
    case "offset": {
      if (s.t === "offset") {
        if (e.type === "move" && p) return { s: { ...s, at: p } };
        if (e.type === "back") return { s: { ...s, typed: s.typed.slice(0, -1) } };
        if (e.ch != null && /^[0-9.]$/.test(e.ch)) return { s: { ...s, typed: s.typed + e.ch } };
        const typed = Number(s.typed);
        const d = s.typed && typed > 0 ? typed : (e.dist ?? 0);
        if ((e.type === "down" || e.type === "enter") && p) {
          if (!(d > 0.05)) return { s, say: "Move away from the line to say how far the copy goes" };
          return { s: IDLE, cmd: { cmd: "sheet_offset", item: s.item, distance_mm: Math.round(d * 100) / 100, at_mm: roundP(p) } };
        }
        return { s };
      }
      if (e.type === "down" && p) {
        if (e.hit == null) return { s, say: "Click a piece of paper line work to copy" };
        return { s: { t: "offset", item: e.hit, at: p, typed: "" } };
      }
      return { s };
    }
    case "text": {
      // typing itself is in the text box PaperView shows; the tool only places it: a click for plain text, a drag
      // across for a text block (its width; the first line's baseline one letter below the top)
      if (s.t === "text" || s.t === "cell") return { s };
      if (e.type === "down" && p) return { s: { t: "drag", from: p, to: p, click: false } };
      if (e.type === "move" && p && s.t === "drag") return { s: { ...s, to: p } };
      if (e.type === "up" && p && s.t === "drag") {
        const r = rectOf(s.from, p);
        if (r[2] < MIN_BLOCK) return { s: { t: "text", at: s.from, text: "" } };
        return { s: { t: "text", at: [Math.round(r[0] * 100) / 100, Math.round((r[1] + TEXT_HEIGHT) * 100) / 100], text: "", width: Math.round(r[2] * 10) / 10 } };
      }
      return { s };
    }
    case "table": {
      if (s.t === "cell") return { s };
      if (e.type === "down" && p) return { s: { t: "drag", from: p, to: p, click: false } };
      if (e.type === "move" && p && s.t === "drag") return { s: { ...s, to: p } };
      if (e.type === "up" && p && s.t === "drag") {
        const r = rectOf(s.from, p);
        // a click: a 5 by 3 table there; a frame: as many 25 mm columns and 6 mm rows as fit it
        const made = r[2] < MIN_SIZE || r[3] < MIN_SIZE ? { size: [5, 3], cols_mm: [25, 25, 25] } : tableFromFrame(r);
        return { s: IDLE, cmd: { cmd: "sheet_table", sheet: c.sheet, at_mm: roundP([r[0], r[1]]), ...made } };
      }
      return { s };
    }
    default:
      return { s };
  }
}

function finishFrame(tool: PaperTool, r: Rect, c: ToolCtx): ToolOut {
  if (r[2] < MIN_SIZE || r[3] < MIN_SIZE) return { s: IDLE, say: `Too small: drag a frame at least ${MIN_SIZE} mm each way` };
  const rr = roundRect(r);
  if (tool === "viewport") return { s: { t: "choose", rect: rr } };
  return { s: IDLE, cmd: { cmd: "sheet_line", sheet: c.sheet, kind: "rect", pts_mm: [[rr[0], rr[1]], [rr[0] + rr[2], rr[1] + rr[3]]] } };
}

/** The `sheet_dim` for the points picked, with the line through where the mouse is (Shift: a baseline set). */
export function dimCommand(picks: DimPick[], at: P, baseline: boolean, c: { sheet: number; dimChoice: DimChoice }): Record<string, unknown> {
  const pts = picks.map((k) => k.p);
  const mode = baseline ? "baseline" : dimModeFor(pts, c.dimChoice);
  // a baseline set is laid out along the first and last points (aligned), so its offset is measured that way too
  const offset = dimOffsetAt(pts, mode === "baseline" ? "aligned" : mode, at);
  const viewport = picks.find((k) => k.viewport != null)?.viewport;
  return { cmd: "sheet_dim", sheet: c.sheet, anchors: picks.map((k) => k.anchor), mode, offset_mm: offset, ...(viewport != null ? { viewport } : {}) };
}

/** What the step hint says: what to do next with this tool, given where it is. */
export function stepHint(tool: PaperTool, s: ToolState): string {
  const esc = " · Esc cancels";
  switch (tool) {
    case "select":
      if (s.t === "cell") return "Type in the cell · Tab next cell, Enter the cell below · paste a grid from a spreadsheet · Esc stops";
      return "Click to select · drag a view to move it, its handles to resize · arrows nudge · double-click text, a table's cell or the title block to edit";
    case "viewport":
      if (s.t === "drag") return s.click ? `Click the opposite corner${esc}` : `Let go to finish the frame${esc}`;
      if (s.t === "choose") return `Pick the view this frame shows${esc}`;
      return "Drag a frame where the view goes (or click two corners)";
    case "rect":
      if (s.t === "drag") return s.click ? `Click the opposite corner${esc}` : `Let go to finish${esc}`;
      return "Drag a rectangle (or click two corners)";
    case "line":
      return s.t === "pts" ? `Click the end · Shift keeps it straight · Enter stops${esc}` : "Click where the line starts";
    case "polyline":
      if (s.t !== "pts") return "Click the first point";
      return s.pts.length >= 3 ? `Click the next point, the first to close · double-click or Enter finishes · Backspace undoes a point${esc}` : `Click the next point · double-click or Enter finishes${esc}`;
    case "arc":
      if (s.t !== "pts") return "Click where the arc starts";
      return s.pts.length === 1 ? `Click where it ends${esc}` : `Move to bend it, click to finish${esc}`;
    case "dimension":
      if (s.t !== "dim" || !s.picks.length) return "Click a point of the model (a marker shows on corners and middles)";
      return s.picks.length === 1 ? `Click the next point${esc}` : `Click more points to chain · Enter or double-click finishes where the mouse is · Shift+Enter for baseline${esc}`;
    case "text":
      if (s.t === "drag") return `Let go where the block's right edge goes (a click makes plain text)${esc}`;
      return s.t === "text" ? (s.width ? "Type · Enter finishes, Shift+Enter starts a new paragraph · lines wrap to the block · Esc cancels" : "Type · Enter finishes, Shift+Enter starts a new line · Esc cancels") : "Click where the text starts, or drag across for a text block that wraps · double-click text to change it";
    case "table":
      if (s.t === "drag") return `Let go to make the table (about 25 mm columns, 6 mm rows)${esc}`;
      if (s.t === "cell") return "Type in the cell · Tab next cell, Enter the cell below · paste a grid from a spreadsheet · Esc stops";
      return "Click for a table, or drag its size · double-click a cell to type · paste from a spreadsheet";
    case "trim":
      return "Click the piece of a line to cut away (red shows what goes) · Shift+click extends instead";
    case "extend":
      return "Click near the end of a line to run it on to the next line (the dashes show where)";
    case "offset":
      return s.t === "offset" ? `Click the side the copy goes, at the distance shown, or type a distance and Enter${esc}` : "Click a line, rectangle or arc to copy";
    case "erase":
      return "Click something on the page to delete it (Undo brings it back)";
  }
}
