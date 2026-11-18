import { describe, expect, it } from "vitest";
import { inOrder, moveIn, newPageName, opacityOf, paperId, parseScale, percentOf, reorderCommand, scaleText, viewArgs, viewName, viewportsOf, type SheetInfo, type ViewportItem } from "../src/paper/pages";
import { clampK, fit, hitRect, MAX_K, MIN_K, panBy, tilesFor, toPaper, toScreen, zoomAt, zoomPercent, zoomStep, zoomStepUp } from "../src/paper/view";
import { cleanEdits, leftTabs, toolAfterSwitch, WORKSPACES, withEdits } from "../src/workspace";

const sheet = (id: number, order: number, name = `P${id}`, items: SheetInfo["items"] = []): SheetInfo => ({ id, name, number: "", paper: { w_mm: 420, h_mm: 297, name: "a3" }, order, fields: {}, issued: 0, items });
const vp = (id: number, rect: [number, number, number, number], extra: Partial<ViewportItem> = {}) => ({ id, kind: "viewport", sheet: 1, rect_mm: rect, scale: 50, ...extra });

describe("page order", () => {
  it("sorts by order, then id, as the engine does", () => {
    expect(inOrder([sheet(5, 1), sheet(3, 0), sheet(4, 1)]).map((s) => s.id)).toEqual([3, 4, 5]);
  });
  it("moves one entry, clamped, and leaves bad moves alone", () => {
    expect(moveIn([1, 2, 3], 2, 0)).toEqual([3, 1, 2]);
    expect(moveIn([1, 2, 3], 0, 9)).toEqual([2, 3, 1]);
    expect(moveIn([1, 2, 3], 0, -4)).toEqual([1, 2, 3]);
    const same = [1, 2, 3];
    expect(moveIn(same, 5, 0)).toBe(same);
    expect(moveIn(same, 1, 1)).toBe(same);
  });
  it("reorders with one command (one undo step), only when the order changes", () => {
    const s = [sheet(1, 0), sheet(2, 1), sheet(3, 2)];
    expect(reorderCommand([1, 2, 3], s)).toBeNull();
    expect(reorderCommand([3, 1, 2], s)).toEqual({ cmd: "sheet_order", sheets: [3, 1, 2] });
    // the engine's order counts, not the list's: sheets listed out of order are sorted first
    expect(reorderCommand([1, 2, 3], [sheet(3, 2), sheet(1, 0), sheet(2, 1)])).toBeNull();
    // never a list the engine would refuse
    expect(reorderCommand([3, 1], s)).toBeNull();
    expect(reorderCommand([3, 1, 1], s)).toBeNull();
    expect(reorderCommand([3, 1, 9], s)).toBeNull();
  });
  it("names a new page with a number nobody has", () => {
    expect(newPageName([])).toBe("Page 1");
    expect(newPageName([sheet(1, 0, "Page 2")])).toBe("Page 3");
    expect(newPageName([sheet(1, 0, "Plans"), sheet(2, 1, "Elevations")])).toBe("Page 3");
  });
});

describe("viewports and their fields", () => {
  it("lists a page's viewports bottom first", () => {
    const s = sheet(1, 0, "x", [vp(9, [0, 0, 1, 1], { z: 2 }), { id: 4, kind: "dimension" }, vp(7, [0, 0, 1, 1]), vp(8, [0, 0, 1, 1])]);
    expect(viewportsOf(s).map((v) => v.id)).toEqual([7, 8, 9]);
    expect(viewportsOf(undefined)).toEqual([]);
  });
  it("reads paper sizes by name or size", () => {
    expect(paperId({ w_mm: 420, h_mm: 297, name: "a3" })).toBe("a3");
    expect(paperId({ w_mm: 914.4, h_mm: 609.6, name: "ARCH D" })).toBe("archd");
    expect(paperId({ w_mm: 297, h_mm: 210, name: "" })).toBe("a4");
    expect(paperId({ w_mm: 333, h_mm: 222, name: "" })).toBe("");
  });
  it("reads scales the engine takes", () => {
    expect(scaleText(50)).toBe("1:50");
    expect(scaleText(12.5)).toBe("1:12.5");
    expect(parseScale("50")).toBe(50);
    expect(parseScale(" 1 : 100 ")).toBe("1:100");
    expect(parseScale('1/4"=1\'-0"')).toBe('1/4"=1\'-0"');
    expect(parseScale("")).toBeNull();
    expect(parseScale("0")).toBeNull();
    expect(parseScale("big")).toBeNull();
  });
  it("turns the slider's percent into the engine's opacity and back", () => {
    expect(opacityOf(40)).toBe(0.4);
    expect(opacityOf(-5)).toBe(0);
    expect(opacityOf(140)).toBe(1);
    expect(opacityOf(33.4)).toBe(0.33);
    expect(percentOf(vp(1, [0, 0, 1, 1]) as ViewportItem)).toBe(100);
    expect(percentOf(vp(1, [0, 0, 1, 1], { opacity: 0.4 }) as ViewportItem)).toBe(40);
  });
  it("asks for true elevations for the sides, a plan, or a 3D view from a named view", () => {
    expect(viewArgs("front")).toEqual({ view: { type: "elevation", dir: [0, -1, 0] } });
    expect(viewArgs("left")).toEqual({ view: { type: "elevation", dir: [-1, 0, 0] } });
    expect(viewArgs("plan")).toEqual({ view: { type: "plan", cut_z: 1.2, floor_z: 0 } });
    expect(viewArgs("iso")).toEqual({ from_view: "iso" });
    expect(viewArgs("Porch")).toEqual({ from_view: "Porch" });
  });
  it("says what a viewport shows", () => {
    expect(viewName(vp(1, [0, 0, 1, 1], { view: { type: "plan" } }) as ViewportItem)).toBe("Plan");
    expect(viewName(vp(1, [0, 0, 1, 1], { view: { type: "elevation", dir: [0, -1, 0] } }) as ViewportItem)).toBe("Front elevation");
    expect(viewName(vp(1, [0, 0, 1, 1], { view: { type: "elevation", dir: [1, 0, 0] } }) as ViewportItem)).toBe("Right elevation");
    expect(viewName(vp(1, [0, 0, 1, 1], { view: { type: "view3d" }, scene: "Porch" }) as ViewportItem)).toBe("3D view (Porch)");
  });
});

describe("the paper view's camera", () => {
  it("fits a page in the box, centred, with a margin", () => {
    const c = fit(420, 297, 1000, 800, 40);
    expect(c.k).toBeCloseTo(920 / 420);
    const [x0, y0] = toScreen(c, [0, 0]);
    const [x1, y1] = toScreen(c, [420, 297]);
    expect(x0).toBeCloseTo(40);
    expect(x1).toBeCloseTo(960);
    expect((y0 + y1) / 2).toBeCloseTo(400);
  });
  it("maps paper to screen and back", () => {
    const c = { k: 2.5, ox: 30, oy: -12 };
    const p: [number, number] = [123.4, 56.7];
    const back = toPaper(c, toScreen(c, p));
    expect(back[0]).toBeCloseTo(p[0]);
    expect(back[1]).toBeCloseTo(p[1]);
  });
  it("zooms at the cursor: the point under it stays put", () => {
    const c = { k: 2, ox: 50, oy: 40 };
    const at: [number, number] = [300, 220];
    const before = toPaper(c, at);
    const z = zoomAt(c, at, 1.7);
    expect(z.k).toBeCloseTo(3.4);
    const after = toPaper(z, at);
    expect(after[0]).toBeCloseTo(before[0]);
    expect(after[1]).toBeCloseTo(before[1]);
  });
  it("clamps the zoom", () => {
    expect(zoomAt({ k: 1, ox: 0, oy: 0 }, [0, 0], 1e9).k).toBe(MAX_K);
    expect(zoomAt({ k: 1, ox: 0, oy: 0 }, [0, 0], 1e-9).k).toBe(MIN_K);
    expect(clampK(NaN)).toBe(1);
  });
  it("pans", () => expect(panBy({ k: 1, ox: 1, oy: 2 }, 10, -5)).toEqual({ k: 1, ox: 11, oy: -3 }));
  it("caches at zoom steps of √2", () => {
    expect(zoomStep(1)).toBe(1);
    expect(zoomStep(1.3)).toBe(1);
    expect(zoomStep(1.45)).toBeCloseTo(Math.SQRT2);
    expect(zoomStep(2.1)).toBe(2);
    expect(zoomStep(3)).toBeCloseTo(2 * Math.SQRT2);
    // drawn at the step above, the tiles are only ever shrunk on screen, by at most √2
    expect(zoomStepUp(1)).toBe(1);
    expect(zoomStepUp(1.3)).toBeCloseTo(Math.SQRT2);
    for (const k of [0.07, 0.5, 1.93, 7, 60]) {
      expect(zoomStepUp(k)).toBeGreaterThanOrEqual(k - 1e-9);
      expect(zoomStepUp(k) / k).toBeLessThanOrEqual(Math.SQRT2 + 1e-9);
    }
  });
  it("lists the tiles that cover the screen, inside the page", () => {
    // the page 420 mm wide at 2 px/mm is 840 px: two 512 px tiles across, one down for 297 mm (594 px: two)
    const c = { k: 2, ox: 0, oy: 0 };
    expect(tilesFor(c, 2, 2000, 2000, 420, 297)).toEqual([
      [0, 0],
      [1, 0],
      [0, 1],
      [1, 1],
    ]);
    // a small window over the top left needs one
    expect(tilesFor(c, 2, 300, 300, 420, 297)).toEqual([[0, 0]]);
    // the page panned out of sight needs none
    expect(tilesFor({ k: 2, ox: -5000, oy: 0 }, 2, 300, 300, 420, 297)).toEqual([]);
    // tiles drawn at twice the screen's zoom (a retina screen) cover the same area with twice the pixels
    expect(tilesFor(c, 4, 300, 300, 420, 297)).toEqual([
      [0, 0],
      [1, 0],
      [0, 1],
      [1, 1],
    ]);
  });
  it("shows 100% at the paper's real size", () => expect(zoomPercent(96 / 25.4)).toBe(100));
  it("picks the topmost viewport under a point", () => {
    const r: [number, number, number, number][] = [
      [0, 0, 100, 100],
      [50, 50, 100, 100],
    ];
    expect(hitRect(r, [75, 75])).toBe(1);
    expect(hitRect(r, [10, 10])).toBe(0);
    expect(hitRect(r, [200, 10])).toBe(-1);
  });
});

describe("the Layout workspace", () => {
  const layout = WORKSPACES.find((w) => w.id === "layout")!;
  it("is the fifth workspace, opening on its pages", () => {
    expect(WORKSPACES.map((w) => w.id)).toEqual(["model", "arch", "design", "previz", "layout"]);
    expect(layout.leftTab).toBe("pages");
    expect(leftTabs("layout")[0]).toBe("pages");
    expect(leftTabs("model")).not.toContain("pages");
  });
  it("keeps the model's tool when you switch to it, and back", () => {
    expect(toolAfterSwitch(layout, "push")).toBe("push");
    const model = WORKSPACES.find((w) => w.id === "model")!;
    const previz = WORKSPACES.find((w) => w.id === "previz")!;
    expect(toolAfterSwitch(model, "push")).toBe("push");
    expect(toolAfterSwitch(previz, "push")).toBe("select");
  });
  it("remembers a Pages tab choice, and drops nonsense", () => {
    expect(cleanEdits({ layout: { leftTab: "pages", showRight: false } })).toEqual({ layout: { leftTab: "pages", showRight: false } });
    expect(cleanEdits({ layout: { leftTab: "paper" } })).toEqual({});
    expect(withEdits(layout, { showLeft: false }).showLeft).toBe(false);
  });
});
