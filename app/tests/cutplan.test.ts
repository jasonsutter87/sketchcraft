// Tools > Cut plan: the stock form's numbers, the command it sends, and the shapes of the cutting diagram.
import { expect, it } from "vitest";
import { boardDiagram, formFromStock, lengthText, lengthWords, money, parsePrice, planCommand, sheetDiagram, stockCommand, stockTitle, stockUnits, type BoardPlan, type SheetPlan, type Stock } from "../src/cutplan";
import { buildMenuActions } from "../src/menuactions";
import { MENUS } from "../src/menumodel";
import { EMPTY } from "../src/selection";

const STOCK: Stock = {
  kerf: 0.003,
  trim: 0,
  sheet: [2.44, 1.22],
  board_lengths: [2.44],
  lengths_by_size: {},
  currency: "$",
  materials: { "Birch ply": { grain: false, sheet_price: 60, board_price: 0, board_foot_price: 0 } },
};

it("asks for the picked groups and components, or every part", () => {
  expect(planCommand(EMPTY)).toEqual({ cmd: "cut_plan" });
  expect(planCommand({ ...EMPTY, groups: [3], instances: [9], faces: [1] })).toEqual({ cmd: "cut_plan", ids: [3, 9] });
});

it("types lengths in millimetres, or inches for an imperial model", () => {
  expect(stockUnits("m")).toBe("mm");
  expect(stockUnits("mm")).toBe("mm");
  expect(stockUnits("ft")).toBe("in");
  expect(stockUnits("in")).toBe("in");
  expect(lengthText(2.44, "mm")).toBe("2440");
  expect(lengthText(2.4384, "in")).toBe("96");
  expect(lengthWords(0.018, "mm")).toBe("18 mm");
  expect(lengthWords(1.2192, "in")).toBe('48"');
  expect(money("$", 60)).toBe("$60.00");
});

it("shows the stock as a form and sends the form back as cut_stock", () => {
  const form = formFromStock(STOCK, "mm", ["Birch ply", "Maple"]);
  expect(form.sheet).toBe("2440 × 1220");
  expect(form.kerf).toBe("3");
  expect(Object.keys(form.materials)).toEqual(["Birch ply", "Maple"]);
  expect(form.materials["Birch ply"].sheetPrice).toBe("60");
  expect(form.materials["Maple"].sheetPrice).toBe("");
  form.materials["Maple"].boardFootPrice = "$9.5";
  form.materials["Maple"].grain = true;
  const made = stockCommand(form, "mm");
  expect(made).toEqual({
    cmd: {
      cmd: "cut_stock",
      kerf: 0.003,
      trim: 0,
      sheet: [2.44, 1.22],
      board_lengths: [2.44],
      currency: "$",
      materials: {
        "Birch ply": { sheet_price: 60, board_price: 0, board_foot_price: 0, grain: false },
        Maple: { sheet_price: 0, board_price: 0, board_foot_price: 9.5, grain: true },
      },
    },
  });
  // inches
  const inch = formFromStock({ ...STOCK, board_lengths: [2.4384, 3.048] }, "in", []);
  expect(inch.boardLengths).toBe("96, 120");
  const sent = stockCommand(inch, "in") as { cmd: { board_lengths: number[] } };
  expect(sent.cmd.board_lengths[0]).toBeCloseTo(2.4384, 6);
  expect(sent.cmd.board_lengths[1]).toBeCloseTo(3.048, 6);
});

it("refuses what is not a length or a price, and says which", () => {
  const ok = formFromStock(STOCK, "mm", []);
  expect(stockCommand({ ...ok, kerf: "wide" }, "mm")).toEqual({ error: "kerf" });
  expect(stockCommand({ ...ok, kerf: "-2" }, "mm")).toEqual({ error: "kerf" });
  expect(stockCommand({ ...ok, trim: "x" }, "mm")).toEqual({ error: "trim" });
  expect(stockCommand({ ...ok, sheet: "2440" }, "mm")).toEqual({ error: "sheet" });
  expect(stockCommand({ ...ok, sheet: "2440 x 0" }, "mm")).toEqual({ error: "sheet" });
  expect(stockCommand({ ...ok, boardLengths: "" }, "mm")).toEqual({ error: "boards" });
  expect(stockCommand({ ...ok, materials: { A: { sheetPrice: "free", boardPrice: "", boardFootPrice: "", grain: false } } }, "mm")).toEqual({ error: "price" });
  // an empty kerf or trim is none
  expect((stockCommand({ ...ok, trim: "" }, "mm") as { cmd: { trim: number } }).cmd.trim).toBe(0);
  expect(parsePrice("")).toBe(0);
  expect(parsePrice("€12.5")).toBe(12.5);
  expect(parsePrice("1,5")).toBeNaN();
});

const SHEET: SheetPlan = {
  index: 1,
  material: "Birch ply",
  thickness: 0.018,
  size: [2.44, 1.22],
  grain: false,
  waste_pct: 40,
  parts: [
    { number: 1, id: 5, name: "Side", x: 0, y: 0, w: 0.72, h: 0.56, rotated: false },
    { number: 2, id: 6, name: "Shelf", x: 0.723, y: 0, w: 0.56, h: 0.3, rotated: true },
  ],
};

it("draws a sheet to scale inside the picture", () => {
  const d = sheetDiagram(SHEET, 724, "mm");
  // 724 wide with 4 px of border each side: 716 px for 2.44 m
  const k = 716 / 2.44;
  expect(d.width).toBe(724);
  expect(d.stock.w).toBeCloseTo(2.44 * k, 6);
  expect(d.height).toBeCloseTo(1.22 * k + 8, 6);
  expect(d.shapes).toHaveLength(2);
  expect(d.shapes[0]).toMatchObject({ number: 1, name: "Side", size: "720 mm × 560 mm", tint: 0 });
  expect(d.shapes[1].x).toBeCloseTo(4 + 0.723 * k, 6);
  // every part stays inside the sheet
  for (const s of d.shapes) expect(s.x + s.w).toBeLessThanOrEqual(d.stock.x + d.stock.w + 1e-9);
});

it("draws a board as a bar with its parts end to end", () => {
  const b: BoardPlan = { index: 2, material: "Maple", stock: "1×2", length: 2.44, cross: [0.019, 0.038], waste_pct: 10, board_feet: 1.33, parts: [{ number: 4, id: 1, name: "Stile", x: 0, length: 0.72 }, { number: 5, id: 2, name: "Rail", x: 0.723, length: 0.52 }] };
  const d = boardDiagram(b, 724, "in");
  expect(d.height).toBe(44);
  expect(d.shapes.map((s) => s.size)).toEqual(['28.375"', '20.5"']);
  expect(d.shapes[1].x).toBeGreaterThan(d.shapes[0].x + d.shapes[0].w);
});

it("names sheets and boards in a list", () => {
  const t = (id: string) => ({ "cut.sheet": "Sheet", "cut.board": "Board", "cut.noMaterial": "No material" })[id] ?? id;
  expect(stockTitle("sheet", 2, 5, "Birch ply", t)).toBe("Sheet 2 / 5 · Birch ply");
  expect(stockTitle("board", 1, 2, "", t, "2×4")).toBe("Board 1 / 2 · No material · 2×4");
});

it("is in the Tools menu and the menu actions", () => {
  const ids: string[] = [];
  const walk = (items: unknown[]) => {
    for (const i of items as { id?: string; items?: unknown[] }[]) {
      if (i.id) ids.push(i.id);
      if (i.items) walk(i.items);
    }
  };
  walk(MENUS as unknown as unknown[]);
  expect(ids).toContain("tools.cutplan");
  expect(typeof buildMenuActions).toBe("function");
});
