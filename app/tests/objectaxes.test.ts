import { describe, expect, it } from "vitest";
import type { Scene } from "../src/engine";

// the menu's module reads `window` when it loads (the desktop app or the browser); a bare one is enough here
const g = globalThis as Record<string, unknown>;
g.window ??= globalThis;
const { contextItems } = await import("../src/contextmenu");

// #266: right-click a face to put the drawing axes on it; right-click a group or component with its own axes to reset them
const EMPTY = { faces: [], edges: [], groups: [], instances: [] };

function scene(over: Partial<Record<string, unknown>> = {}): Scene {
  return {
    faces: [{ id: 5, normal: [0, 0, 1], outer: [], group: 2, area: 1, material: null, hidden: false }],
    edgeList: [],
    groups: [
      { id: 2, name: "Roof", tag: null, hidden: false, parent: null, locked: false, faces: 1, ownAxes: true },
      { id: 3, name: "Wall", tag: null, hidden: false, parent: null, locked: false, faces: 0 },
    ],
    instances: [],
    views: [],
    materials: [],
    cages: {},
    editing: null,
    activeGroup: null,
    units: "m",
    ...over,
  } as unknown as Scene;
}

function menu(s: Scene, sel: Partial<typeof EMPTY>) {
  const ran: Record<string, unknown>[] = [];
  const run = (c: Record<string, unknown>) => (ran.push(c), Promise.resolve({ ok: true }));
  const items = contextItems(s, { ...EMPTY, ...sel } as never, run as never, () => {}, () => {});
  return { items, ran, labels: items.map((i) => i.label) };
}

describe("axes on groups and components", () => {
  it("offers Align axes on one face and runs align_axes", () => {
    const { items, ran } = menu(scene({ activeGroup: 2 }), { faces: [5] });
    items.find((i) => i.label === "Align axes to this face")!.run();
    expect(ran).toEqual([{ cmd: "align_axes", face: 5 }]);
  });
  it("leaves Align axes out for several faces", () => {
    expect(menu(scene(), { faces: [5, 6] }).labels).not.toContain("Align axes to this face");
  });
  it("offers Reset axes only on a group that has its own", () => {
    const { items, ran, labels } = menu(scene(), { groups: [2] });
    expect(labels).toContain("Reset axes");
    items.find((i) => i.label === "Reset axes")!.run();
    expect(ran).toEqual([{ cmd: "set_axes", id: 2, reset: true }]);
    expect(menu(scene(), { groups: [3] }).labels).not.toContain("Reset axes");
  });
});
