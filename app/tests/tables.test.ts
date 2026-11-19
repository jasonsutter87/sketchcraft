// Tables and text blocks on sheets (#7): the Table and Text tools' gestures, a table's cells under the mouse, moving
// between cells, what a paste makes, and how tables and text blocks are picked, moved and described.
import { describe, expect, it } from "vitest";
import { cellAt, cellRect, describe as describeItem, distToItem, isTable, itemAt, itemPoints, movedItem, nextCell, paperItems, pasteCommand, tableFromFrame, tableRect, textBox, roughWidth, type TableItem, type TextItem } from "../src/paper/items";
import type { SheetInfo } from "../src/paper/pages";
import { IDLE, PAPER_TOOL_INFO, stepHint, toolForKey, toolStep, MIN_BLOCK, type PaperTool, type ToolEvent, type ToolState } from "../src/paper/tools";

const ctx = { sheet: 7, dimChoice: "auto" as const, tol: 1 };
function play(tool: PaperTool, events: ToolEvent[], from: ToolState = IDLE) {
  let s = from;
  const cmds: Record<string, unknown>[] = [];
  for (const e of events) {
    const o = toolStep(tool, s, e, ctx);
    s = o.s;
    if (o.cmd) cmds.push(o.cmd);
  }
  return { s, cmds };
}
const down = (x: number, y: number): ToolEvent => ({ type: "down", p: [x, y] });
const mv = (x: number, y: number): ToolEvent => ({ type: "move", p: [x, y] });
const up = (x: number, y: number): ToolEvent => ({ type: "up", p: [x, y] });

/** A door schedule as sheets_info gives it: 3 rows by 3 columns at (20, 30). */
const schedule: TableItem = {
  id: 5,
  kind: "table",
  at_mm: [20, 30],
  rows: [
    ["Mark", "Size", "Type"],
    ["D1", "900 x 2100", "Single"],
    ["D2", "1800 x 2100", "Double"],
  ],
  layout: {
    xs: [20, 40, 70, 95],
    ys: [30, 35.5, 41, 46.5],
    title_at: null,
    rows: [
      ["Mark", "Size", "Type"],
      ["D1", "900 x 2100", "Single"],
      ["D2", "1800 x 2100", "Double"],
    ],
    text_mm: 2.5,
    header_rows: 1,
    pad_mm: 1.5,
  },
};

describe("the Table tool", () => {
  it("has a key and a tip, and B picks it", () => {
    expect(PAPER_TOOL_INFO.table.key).toBe("b");
    expect(toolForKey("B")).toBe("table");
    expect(PAPER_TOOL_INFO.table.tip).toMatch(/paste/i);
  });
  it("a click makes a 5 by 3 table there", () => {
    const t = play("table", [down(20, 30), up(20, 30)]);
    expect(t.cmds).toEqual([{ cmd: "sheet_table", sheet: 7, at_mm: [20, 30], size: [5, 3], cols_mm: [25, 25, 25] }]);
    expect(t.s).toEqual(IDLE);
  });
  it("a frame makes as many 25 mm columns and 6 mm rows as fit it", () => {
    const t = play("table", [down(20, 30), mv(60, 40), mv(95, 60), up(95, 60)]);
    expect(t.cmds[0]).toEqual({ cmd: "sheet_table", sheet: 7, at_mm: [20, 30], size: [5, 3], cols_mm: [25, 25, 25] });
    expect(tableFromFrame([0, 0, 200, 300])).toEqual({ size: [50, 8], cols_mm: Array(8).fill(25) });
    expect(tableFromFrame([0, 0, 6, 6]).size).toEqual([2, 1]);
  });
  it("Escape cancels a frame; typing in a cell ignores the tool", () => {
    expect(play("table", [down(20, 30), mv(60, 60), { type: "esc" }]).s).toEqual(IDLE);
    const cell: ToolState = { t: "cell", id: 5, row: 0, col: 0, text: "" };
    expect(play("table", [down(1, 1)], cell).s).toEqual(cell);
  });
  it("hints say what to do", () => {
    expect(stepHint("table", IDLE)).toMatch(/Click for a table/);
    expect(stepHint("table", { t: "cell", id: 1, row: 0, col: 0, text: "" })).toMatch(/Tab next cell/);
    expect(stepHint("select", { t: "cell", id: 1, row: 0, col: 0, text: "" })).toMatch(/Enter the cell below/);
  });
});

describe("the Text tool's text blocks", () => {
  it("a click is plain text; a drag across makes a block of that width", () => {
    expect(play("text", [down(30, 40), up(30.5, 40)]).s).toEqual({ t: "text", at: [30, 40], text: "" });
    const b = play("text", [down(30, 40), mv(70, 45), up(110, 50)]).s;
    expect(b).toEqual({ t: "text", at: [30, 43.5], text: "", width: 80 });
    expect(MIN_BLOCK).toBeGreaterThan(1);
    expect(stepHint("text", b)).toMatch(/wrap/);
  });
  it("a block's box comes from the engine's layout", () => {
    const t: TextItem = { id: 1, kind: "text", at_mm: [10, 20], paragraphs: ["a long paragraph"], box_mm: [40, 0], layout: { rect: [10, 17, 40, 12], lines: 3 } };
    expect(textBox(t, roughWidth)).toEqual([10, 17, 40, 12]);
    expect(describeItem(t)).toMatch(/^Text block/);
  });
});

describe("a table on the page", () => {
  it("is listed, picked, moved and described", () => {
    const s: SheetInfo = { id: 1, name: "P", number: "", paper: { w_mm: 420, h_mm: 297, name: "a3" }, order: 0, fields: {}, issued: 0, items: [schedule as never] };
    expect(paperItems(s).map((i) => i.id)).toEqual([5]);
    expect(isTable(schedule)).toBe(true);
    expect(tableRect(schedule)).toEqual([20, 30, 75, 16.5]);
    expect(itemAt([schedule], [50, 40], 1)?.id).toBe(5);
    expect(distToItem(schedule, [100, 30])).toBeCloseTo(5);
    expect(movedItem(schedule, [1, 2])).toEqual({ cmd: "sheet_table", id: 5, at_mm: [21, 32] });
    expect(itemPoints([schedule]).map((p) => p.p)).toContainEqual([95, 46.5]);
    expect(describeItem(schedule)).toBe("Table, 3 rows by 3 columns");
    expect(describeItem({ ...schedule, title: "Doors", source: "sheets" })).toBe('Drawing index "Doors", 3 rows by 3 columns');
  });
  it("finds the cell under the mouse and its rectangle", () => {
    expect(cellAt(schedule, [21, 31])).toEqual([0, 0]);
    expect(cellAt(schedule, [80, 45])).toEqual([2, 2]);
    expect(cellAt(schedule, [10, 45])).toBeNull();
    expect(cellRect(schedule, 1, 1)).toEqual([40, 35.5, 30, 5.5]);
    expect(cellRect(schedule, 3, 0)).toBeNull();
  });
  it("Tab goes across then down, Shift+Tab back, Enter down; past the end is null", () => {
    expect(nextCell(schedule, 0, 2, "tab")).toEqual([1, 0]);
    expect(nextCell(schedule, 1, 0, "back")).toEqual([0, 2]);
    expect(nextCell(schedule, 0, 0, "back")).toBeNull();
    expect(nextCell(schedule, 2, 2, "tab")).toBeNull();
    expect(nextCell(schedule, 0, 1, "down")).toEqual([1, 1]);
    expect(nextCell(schedule, 2, 1, "down")).toBeNull();
  });
});

describe("pasting", () => {
  it("a spreadsheet's grid goes into the picked table at its cell, or makes a new table", () => {
    const tsv = "D3\t800 x 2100\tSingle\r\nD4\t900 x 2100\tSingle\r\n";
    expect(pasteCommand(tsv, { table: 5, cell: [3, 0] }, 7, [0, 0])).toEqual({ cmd: "sheet_table_paste", id: 5, at: [3, 0], text: tsv });
    expect(pasteCommand(tsv, null, 7, [12.345, 50])).toEqual({ cmd: "sheet_table_paste", sheet: 7, at_mm: [12.35, 50], text: tsv });
    // CSV: every line has a comma
    expect(pasteCommand("a,b\n1,2", null, 7, [0, 0])?.cmd).toBe("sheet_table_paste");
  });
  it("plain words are not a grid", () => {
    expect(pasteCommand("just words, typed", null, 7, [0, 0])).toBeNull();
    expect(pasteCommand("two\nlines", null, 7, [0, 0])).toBeNull();
    expect(pasteCommand("  ", null, 7, [0, 0])).toBeNull();
  });
});
