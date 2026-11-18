// The sheet editor (#9): snapping maths, the tools' state machines, the items' geometry and the pointer-to-paper
// transform the tools use.
import { describe, expect, it } from "vitest";
import { arcThrough, cutPath, dimLines, dimModeFor, dimOffsetAt, distToItem, itemAt, itemPoints, movedItem, paperItems, polylineOf, textBox, titleBlock, roughWidth, type PaperItem, type ShapeItem, type TextItem } from "../src/paper/items";
import type { SheetInfo } from "../src/paper/pages";
import { handlePoints, mergeGuides, MIN_SIZE, nudge, ortho, pageGuides, rectGuides, rectOf, snapGrid, snapMove, snapPoint, snapResize, type Guides, type SnapOpts } from "../src/paper/snap";
import { cursorFor, dimCommand, IDLE, PAPER_TOOL_INFO, PAPER_TOOLS, stepHint, toolForKey, toolStep, type PaperTool, type ToolEvent, type ToolState } from "../src/paper/tools";
import { fit, toPaper, toScreen, zoomAt } from "../src/paper/view";

const so: SnapOpts = { grid: 5, gridOn: true, tol: 2 };
const off: SnapOpts = { grid: 5, gridOn: false, tol: 2 };
const page = pageGuides(420, 297);

describe("snapping a frame being moved", () => {
  it("goes to the grid when nothing is near", () => {
    const r = snapMove([20, 20, 100, 60], 12.4, 7.1, { xs: [], ys: [], points: [] }, so);
    expect(r.rect).toEqual([30, 25, 100, 60]);
    expect(r.markers[0].kind).toBe("grid");
  });
  it("stays where it is with the grid off and nothing near", () => {
    expect(snapMove([20, 20, 100, 60], 12.4, 7.1, { xs: [], ys: [], points: [] }, off).rect).toEqual([32.4, 27.1, 100, 60]);
  });
  it("puts its left edge on the margin, its right edge on the page's edge, its centre on the middle", () => {
    expect(snapMove([20, 20, 100, 60], -9.2, 0, page, off).rect[0]).toBeCloseTo(10);
    expect(snapMove([20, 20, 100, 60], 299.1, 0, page, off).rect[0]).toBeCloseTo(320); // right edge at 420
    const c = snapMove([20, 20, 100, 60], 140.5, 0, page, off);
    expect(c.rect[0] + 50).toBeCloseTo(210);
    expect(c.markers).toContainEqual({ kind: "x", v: 210, why: "page middle" });
  });
  it("lines up with another frame's edge or centre, each axis on its own", () => {
    const g = rectGuides([[200, 40, 80, 50]]);
    const r = snapMove([20, 120, 60, 40], 0, -79, g, off); // top 41 -> the other's top, 40
    expect(r.rect[1]).toBeCloseTo(40);
    expect(r.rect[0]).toBe(20);
    const m = snapMove([20, 120, 60, 40], 189.5, 0, g, off); // centre 239.5 -> the other's centre 240
    expect(m.rect[0] + 30).toBeCloseTo(240);
  });
  it("prefers the nearest guide", () => {
    const g: Guides = { xs: [{ v: 10, why: "a" }, { v: 11.5, why: "b" }], ys: [], points: [] };
    expect(snapMove([0, 0, 50, 50], 11, 0, g, off).rect[0]).toBe(11.5);
  });
});

describe("snapping a handle being dragged", () => {
  it("moves only the edges that handle owns", () => {
    const r = snapResize([20, 20, 100, 60], 3, [151.2, 999], { xs: [], ys: [], points: [] }, so); // the right edge
    expect(r.rect).toEqual([20, 20, 130, 60]);
    const c = snapResize([20, 20, 100, 60], 0, [12.4, 7.6], { xs: [], ys: [], points: [] }, so); // top left
    expect(c.rect).toEqual([10, 10, 110, 70]);
  });
  it("snaps an edge to the margin", () => {
    expect(snapResize([20, 20, 100, 60], 5, [0, 286.5], page, off).rect[3]).toBeCloseTo(267); // bottom to 287
  });
  it("never turns inside out or gets smaller than the least size", () => {
    const r = snapResize([20, 20, 100, 60], 3, [-50, 0], { xs: [], ys: [], points: [] }, off);
    expect(r.rect).toEqual([20, 20, MIN_SIZE, 60]);
    const t = snapResize([20, 20, 100, 60], 1, [0, 500], { xs: [], ys: [], points: [] }, off);
    expect(t.rect[1]).toBe(80 - MIN_SIZE);
    expect(t.rect[3]).toBe(MIN_SIZE);
  });
  it("has 8 handles in the order PaperView draws them", () => {
    expect(handlePoints([0, 0, 10, 20])).toEqual([
      [0, 0],
      [5, 0],
      [10, 0],
      [10, 10],
      [10, 20],
      [5, 20],
      [0, 20],
      [0, 10],
    ]);
  });
});

describe("snapping a point", () => {
  it("goes to a point first, then lines, then the grid", () => {
    const g = mergeGuides(page, { xs: [], ys: [], points: [{ p: [101.3, 52.7], why: "model corner", ref: { vertex: 7 } }] });
    const a = snapPoint([100.6, 52], g, so);
    expect(a.p).toEqual([101.3, 52.7]);
    expect(a.point?.ref).toEqual({ vertex: 7 });
    const b = snapPoint([11.2, 77.7], g, so);
    expect(b.p).toEqual([10, 80]);
    expect(b.marker).toEqual({ kind: "x", v: 10, why: "margin" });
    expect(snapPoint([61.2, 77.7], g, so).p).toEqual([60, 80]);
    expect(snapPoint([61.2, 77.7], g, off).p).toEqual([61.2, 77.7]);
  });
  it("points only: off a point it stays put (a dimension only attaches to the model)", () => {
    const g: Guides = { xs: [{ v: 10, why: "x" }], ys: [], points: [{ p: [50, 50], why: "c" }] };
    expect(snapPoint([10.5, 30], g, so, { pointsOnly: true })).toEqual({ p: [10.5, 30], marker: null, point: null });
  });
  it("keeps a line level, plumb or at 45° with Shift", () => {
    expect(ortho([0, 0], [10, 0.4])).toEqual([10, 0]);
    const d = ortho([0, 0], [10, 9]);
    expect(d[0]).toBeCloseTo(d[1]);
  });
  it("rounds to the grid", () => {
    expect(snapGrid(12.6, 5)).toBe(15);
    expect(snapGrid(12.6, 0)).toBe(12.6);
  });
});

describe("nudging", () => {
  it("moves 1 mm, 10 with Shift (the grid when it's on)", () => {
    expect(nudge([10, 10, 50, 50], "ArrowRight", false, 5, true)).toEqual([11, 10, 50, 50]);
    expect(nudge([10, 10, 50, 50], "ArrowUp", true, 5, true)).toEqual([10, 5, 50, 50]);
    expect(nudge([10, 10, 50, 50], "ArrowDown", true, 5, false)).toEqual([10, 20, 50, 50]);
    expect(nudge([10, 10, 50, 50], "a", false, 5, true)).toBeNull();
  });
  it("rects from two corners in either order", () => {
    expect(rectOf([50, 60], [10, 20])).toEqual([10, 20, 40, 40]);
  });
});

describe("the pointer on paper", () => {
  it("screen to paper and back, at any zoom", () => {
    const c = zoomAt(fit(420, 297, 1000, 700), [300, 200], 2.7);
    const p: [number, number] = [123.4, 56.7];
    const back = toPaper(c, toScreen(c, p));
    expect(back[0]).toBeCloseTo(p[0]);
    expect(back[1]).toBeCloseTo(p[1]);
    // the snap reach is 8 px whatever the zoom: the tolerance in mm shrinks as you zoom in
    expect(8 / c.k).toBeLessThan(8 / fit(420, 297, 1000, 700).k);
  });
});

// ---- items --------------------------------------------------------------------------------------------------------

const line = (id: number, a: [number, number], b: [number, number]): ShapeItem => ({ id, kind: "line", pts_mm: [a, b] });

describe("items", () => {
  it("are read from sheets_info in drawing order, without viewports", () => {
    const s: SheetInfo = { id: 1, name: "P", number: "", paper: { w_mm: 420, h_mm: 297, name: "a3" }, order: 0, fields: {}, issued: 0, items: [{ id: 9, kind: "viewport", rect_mm: [0, 0, 1, 1] }, { id: 4, kind: "line", pts_mm: [[0, 0], [1, 1]], z: 2 }, { id: 3, kind: "text", at_mm: [0, 0], paragraphs: ["a"] }, { id: 5, kind: "dimension" }] };
    expect(paperItems(s).map((i) => i.id)).toEqual([3, 5, 4]);
  });
  it("arc through three points", () => {
    const k = arcThrough([0, 0], [10, -10], [20, 0])!;
    expect(k.c[0]).toBeCloseTo(10);
    expect(k.c[1]).toBeCloseTo(0);
    expect(k.r).toBeCloseTo(10);
    const pts = polylineOf("arc", [[0, 0], [10, -10], [20, 0]], 4);
    expect(pts[2][0]).toBeCloseTo(10);
    expect(pts[2][1]).toBeCloseTo(-10);
    expect(arcThrough([0, 0], [5, 0], [10, 0])).toBeNull();
  });
  it("hit tests line work, text and dimensions, topmost and nearest", () => {
    const items: PaperItem[] = [line(1, [0, 0], [100, 0]), line(2, [0, 3], [100, 3]), { id: 3, kind: "text", at_mm: [10, 50], paragraphs: ["HELLO"] }, { id: 4, kind: "dimension", mode: "horizontal", offset_mm: 10, paper_mm: [[0, 100], [50, 100]] }];
    expect(itemAt(items, [50, 0.5], 2)?.id).toBe(1);
    expect(itemAt(items, [50, 2.2], 2)?.id).toBe(2);
    expect(itemAt(items, [12, 49], 2)?.id).toBe(3);
    // a horizontal dimension with a positive offset is drawn up the sheet: at y = 90
    expect(itemAt(items, [25, 90.5], 2)?.id).toBe(4);
    expect(itemAt(items, [300, 300], 2)).toBeNull();
    expect(distToItem(items[0], [50, 4])).toBeCloseTo(4);
  });
  it("text boxes follow the engine's line pitch", () => {
    const t: TextItem = { id: 1, kind: "text", at_mm: [10, 20], paragraphs: ["a", "b"], height_mm: 3.5 };
    const [, y, , h] = textBox(t, roughWidth);
    expect(y).toBeLessThan(20 - 3.5);
    expect(y + h).toBeGreaterThan(20 + (3.5 / 0.73) * 1.25);
  });
  it("snap points are line ends and middles, arc centres", () => {
    const pts = itemPoints([line(1, [0, 0], [10, 0]), { id: 2, kind: "arc", pts_mm: [[0, 0], [10, -10], [20, 0]] }]);
    const has = (x: number, y: number, why: string) => pts.some((p) => Math.abs(p.p[0] - x) < 1e-6 && Math.abs(p.p[1] - y) < 1e-6 && p.why === why);
    expect(has(0, 0, "end") && has(10, 0, "end") && has(5, 0, "midpoint") && has(10, 0, "centre") && has(20, 0, "end")).toBe(true);
  });
  it("moving an item is one command: line work by its points, text by its place", () => {
    expect(movedItem(line(1, [0, 0], [10, 0]), [5, 2.5])).toEqual({ cmd: "sheet_line", id: 1, pts_mm: [[5, 2.5], [15, 2.5]] });
    expect(movedItem({ id: 2, kind: "text", at_mm: [1, 1], paragraphs: ["x"] }, [1, 1])).toEqual({ cmd: "sheet_text", id: 2, at_mm: [2, 2] });
    expect(movedItem({ id: 3, kind: "dimension" }, [1, 1])).toBeNull();
  });
  it("a trim's cut walks the outline", () => {
    const r: ShapeItem = { id: 1, kind: "polyline", pts_mm: [[0, 0], [10, 0], [10, 10]] };
    expect(cutPath(r, [5, 0], [10, 5])).toEqual([
      [5, 0],
      [10, 0],
      [10, 5],
    ]);
    expect(cutPath(line(1, [0, 0], [100, 0]), [60, 0], [20, 0])).toEqual([
      [20, 0],
      [60, 0],
    ]);
  });
  it("the title block sits in the bottom right, inside the border", () => {
    const tb = titleBlock(420, 297);
    expect(tb.rect).toEqual([300, 255, 110, 32]);
    expect(tb.project[3] + tb.title[3]).toBe(32);
  });
});

describe("dimensions", () => {
  it("are placed as draft/dims.rs places them (positive: up the sheet, or right)", () => {
    const g = dimLines([[0, 100], [40, 100], [100, 110]], "horizontal", 10);
    // up from the highest point on the sheet (the smallest y): 100 - 10
    expect(g.spans).toHaveLength(2);
    expect(g.spans[0]).toEqual([
      [0, 90],
      [40, 90],
    ]);
    expect(g.spans[1][1]).toEqual([100, 90]);
    const v = dimLines([[0, 0], [0, 50]], "vertical", 5);
    expect(v.spans[0][0][0]).toBeCloseTo(5);
    const b = dimLines([[0, 0], [10, 0], [20, 0]], "baseline", 5);
    expect(b.spans[1][0][0]).toBe(0); // every span from the first point
  });
  it("the offset puts the line through the mouse, either side", () => {
    expect(dimOffsetAt([[0, 100], [50, 100]], "horizontal", [20, 80])).toBe(20);
    expect(dimOffsetAt([[0, 100], [50, 100]], "horizontal", [20, 115])).toBe(-15);
    // right on the points: 2 mm out, never zero
    expect(Math.abs(dimOffsetAt([[0, 100], [50, 100]], "horizontal", [20, 100]))).toBe(2);
    // and round trip: the line drawn with that offset passes through the mouse
    const off = dimOffsetAt([[0, 100], [50, 120]], "horizontal", [20, 70]);
    expect(dimLines([[0, 100], [50, 120]], "horizontal", off).spans[0][0][1]).toBeCloseTo(70);
  });
  it("Auto picks the way the points spread", () => {
    expect(dimModeFor([[0, 0], [100, 10]], "auto")).toBe("horizontal");
    expect(dimModeFor([[0, 0], [10, 100]], "auto")).toBe("vertical");
    expect(dimModeFor([[0, 0], [10, 100]], "aligned")).toBe("aligned");
  });
});

// ---- the tools ------------------------------------------------------------------------------------------------------

const ctx = { sheet: 7, dimChoice: "auto" as const, tol: 1 };
function play(tool: PaperTool, events: ToolEvent[], from: ToolState = IDLE) {
  let s = from;
  const cmds: Record<string, unknown>[] = [];
  const said: string[] = [];
  for (const e of events) {
    const o = toolStep(tool, s, e, ctx);
    s = o.s;
    if (o.cmd) cmds.push(o.cmd);
    if (o.say) said.push(o.say);
  }
  return { s, cmds, said };
}
const down = (x: number, y: number, extra: Partial<ToolEvent> = {}): ToolEvent => ({ type: "down", p: [x, y], ...extra });
const mv = (x: number, y: number): ToolEvent => ({ type: "move", p: [x, y] });
const up = (x: number, y: number): ToolEvent => ({ type: "up", p: [x, y] });

describe("tools", () => {
  it("each has a name, one key of its own, a tip, an icon, a cursor and a hint", () => {
    const keys = PAPER_TOOLS.map((t) => PAPER_TOOL_INFO[t].key);
    expect(new Set(keys).size).toBe(keys.length);
    for (const t of PAPER_TOOLS) {
      expect(toolForKey(PAPER_TOOL_INFO[t].key.toUpperCase())).toBe(t);
      expect(stepHint(t, IDLE).length).toBeGreaterThan(10);
      if (t !== "select") expect(cursorFor(t)).toMatch(/^url\("data:image\/svg\+xml,.*crosshair$/);
    }
    // keys the paper already uses are not tools
    for (const k of ["+", "-", "0", " "]) expect(toolForKey(k)).toBeNull();
  });
  it("Escape cancels any step", () => {
    for (const t of PAPER_TOOLS) expect(toolStep(t, { t: "pts", pts: [[0, 0]], at: [1, 1] }, { type: "esc" }, ctx).s).toEqual(IDLE);
  });
  it("viewport: a drag makes a frame, then the view is chosen", () => {
    const r = play("viewport", [down(20, 20), mv(80, 60), up(120.004, 90)]);
    expect(r.s).toEqual({ t: "choose", rect: [20, 20, 100, 70] });
    expect(r.cmds).toEqual([]);
    expect(stepHint("viewport", r.s)).toMatch(/Pick the view/);
  });
  it("viewport: or two clicks; a tiny one is refused", () => {
    const r = play("viewport", [down(20, 20), up(20, 20), mv(50, 50), down(60, 70)]);
    expect(r.s).toEqual({ t: "choose", rect: [20, 20, 40, 50] });
    const t = play("viewport", [down(20, 20), up(22, 22)]);
    expect(t.s.t).toBe("idle");
    expect(t.said[0]).toMatch(/Too small/);
  });
  it("rectangle: a drag is one sheet_line rect", () => {
    expect(play("rect", [down(10, 10), up(60, 40)]).cmds).toEqual([{ cmd: "sheet_line", sheet: 7, kind: "rect", pts_mm: [[10, 10], [60, 40]] }]);
  });
  it("line: two clicks, and it carries on from the end; Enter stops", () => {
    const r = play("line", [down(0, 0), mv(5, 5), down(10, 0), down(10, 10), { type: "enter" }]);
    expect(r.cmds).toEqual([
      { cmd: "sheet_line", sheet: 7, kind: "line", pts_mm: [[0, 0], [10, 0]] },
      { cmd: "sheet_line", sheet: 7, kind: "line", pts_mm: [[10, 0], [10, 10]] },
    ]);
    expect(r.s).toEqual(IDLE);
    // a double-click's second press at the same place draws nothing
    expect(play("line", [down(0, 0), down(0.2, 0)]).cmds).toEqual([]);
  });
  it("polyline: clicks, Backspace takes one back, double-click finishes; clicking the first closes", () => {
    const r = play("polyline", [down(0, 0), down(10, 0), down(10, 10), down(20, 20), { type: "back" }, down(10, 10.1), { type: "dbl" }]);
    expect(r.cmds).toEqual([{ cmd: "sheet_line", sheet: 7, kind: "polyline", pts_mm: [[0, 0], [10, 0], [10, 10]] }]);
    const c = play("polyline", [down(0, 0), down(10, 0), down(10, 10), down(0.3, 0.2)]);
    expect(c.cmds[0].pts_mm).toEqual([[0, 0], [10, 0], [10, 10], [0, 0]]);
    expect(play("polyline", [down(0, 0), { type: "enter" }]).cmds).toEqual([]);
  });
  it("arc: start, end, then the bend (sent as start, a point on it, end)", () => {
    const r = play("arc", [down(0, 0), down(20, 0), mv(10, -8), down(10, -10)]);
    expect(r.cmds).toEqual([{ cmd: "sheet_line", sheet: 7, kind: "arc", pts_mm: [[0, 0], [10, -10], [20, 0]] }]);
    const flat = play("arc", [down(0, 0), down(20, 0), down(10, 0)]);
    expect(flat.cmds).toEqual([]);
    expect(flat.said[0]).toMatch(/straight/);
  });
  it("dimension: only model points; clicks chain; Enter finishes where the mouse is", () => {
    const a = (v: number) => ({ anchor: { vertex: v }, viewport: 3 });
    const r = play("dimension", [down(5, 5), down(0, 100, { anchor: a(1) }), down(40, 100, { anchor: a(2) }), down(100, 100, { anchor: a(3) }), mv(50, 80), { type: "enter", p: [50, 80] }]);
    expect(r.said[0]).toMatch(/point of the model/);
    expect(r.cmds).toEqual([{ cmd: "sheet_dim", sheet: 7, anchors: [{ vertex: 1 }, { vertex: 2 }, { vertex: 3 }], mode: "horizontal", offset_mm: 20, viewport: 3 }]);
    // Shift: a baseline set
    const b = play("dimension", [down(0, 100, { anchor: a(1) }), down(40, 100, { anchor: a(2) }), { type: "dbl", p: [20, 90], shift: true }]);
    expect(b.cmds[0].mode).toBe("baseline");
    const c = play("dimension", [down(0, 100, { anchor: a(1), shift: true }), down(40, 100, { anchor: a(2) }), { type: "enter", p: [20, 90] }]);
    expect(c.cmds[0].mode).toBe("baseline");
    // one point is not a dimension
    expect(play("dimension", [down(0, 100, { anchor: a(1) }), { type: "enter", p: [0, 0] }]).cmds).toEqual([]);
  });
  it("dimCommand measures the way chosen", () => {
    const picks = [
      { p: [0, 0] as [number, number], anchor: 1 },
      { p: [0, 50] as [number, number], anchor: 2 },
    ];
    expect(dimCommand(picks, [10, 25], false, { sheet: 1, dimChoice: "auto" }).mode).toBe("vertical");
    expect(dimCommand(picks, [10, 25], false, { sheet: 1, dimChoice: "aligned" }).mode).toBe("aligned");
  });
  it("trim, extend and delete act on the item clicked; Shift makes trim extend", () => {
    expect(play("trim", [down(5, 5, { hit: 4 })]).cmds).toEqual([{ cmd: "sheet_trim", item: 4, at_mm: [5, 5] }]);
    expect(play("trim", [down(5, 5, { hit: 4, shift: true })]).cmds[0].cmd).toBe("sheet_extend");
    expect(play("extend", [down(5, 5, { hit: 4 })]).cmds[0].cmd).toBe("sheet_extend");
    expect(play("erase", [down(5, 5, { hit: 4 })]).cmds).toEqual([{ cmd: "sheet_item_delete", ids: [4] }]);
    expect(play("trim", [down(5, 5, { hit: null })]).said[0]).toMatch(/line work/);
  });
  it("offset: pick a line, then the side; a typed distance wins", () => {
    const r = play("offset", [down(5, 0, { hit: 9 }), { type: "move", p: [5, 6], dist: 6 }, down(5, 6, { dist: 6 })]);
    expect(r.cmds).toEqual([{ cmd: "sheet_offset", item: 9, distance_mm: 6, at_mm: [5, 6] }]);
    const t = play("offset", [down(5, 0, { hit: 9 }), { type: "move", ch: "1" }, { type: "move", ch: "2" }, { type: "move", ch: "." }, { type: "move", ch: "5" }, { type: "enter", p: [5, 3], dist: 3 }]);
    expect(t.cmds[0].distance_mm).toBe(12.5);
    expect(play("offset", [down(5, 0, { hit: 9 }), down(5, 0, { dist: 0 })]).said[0]).toMatch(/Move away/);
  });
  it("text: a click places it (typing is in the text box)", () => {
    expect(play("text", [down(30, 40)]).s).toEqual({ t: "text", at: [30, 40], text: "" });
  });
  it("hints say what to do next", () => {
    expect(stepHint("line", { t: "pts", pts: [[0, 0]], at: [0, 0] })).toMatch(/Click the end/);
    expect(stepHint("dimension", { t: "dim", picks: [{ p: [0, 0], anchor: 1 }, { p: [1, 0], anchor: 2 }], at: [0, 0] })).toMatch(/chain/);
  });
});
