import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { loadTrayOn, loadTrayOpen, readTrayOn, readTrayOpen, saveTrayOn, saveTrayOpen, toggleTrayPanel, TRAY_DEFAULT_OPEN, TRAY_ORDER, TRAY_TITLES, trayShown } from "../src/tray";

let data: Map<string, string>;
beforeEach(() => {
  data = new Map();
  (globalThis as { localStorage?: unknown }).localStorage = {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  };
});
afterEach(() => {
  delete (globalThis as { localStorage?: unknown }).localStorage;
});

describe("the Classic tray", () => {
  it("lists its panels in the order a SketchUp user knows (those SketchCraft has)", () => {
    expect(TRAY_ORDER.slice(0, 7).map((id) => TRAY_TITLES[id])).toEqual(["Entity Info", "Materials", "Components", "Styles", "Environments", "Tags", "Scenes"]);
  });

  it("is off until you turn it on, and only shows with the Classic layout", () => {
    expect(loadTrayOn()).toBe(false);
    saveTrayOn(true);
    expect(loadTrayOn()).toBe(true);
    expect(trayShown("classic", true)).toBe(true);
    expect(trayShown("floating", true)).toBe(false);
    expect(trayShown("side", true)).toBe(false);
    expect(trayShown("classic", false)).toBe(false);
    expect(readTrayOn("true")).toBe(false);
    expect(readTrayOn(null)).toBe(false);
  });

  it("opens Entity Info and Materials the first time", () => {
    expect(loadTrayOpen()).toEqual(TRAY_DEFAULT_OPEN);
    expect(TRAY_DEFAULT_OPEN.entity && TRAY_DEFAULT_OPEN.materials).toBe(true);
    expect(TRAY_ORDER.filter((id) => TRAY_DEFAULT_OPEN[id])).toEqual(["entity", "materials"]);
  });

  it("remembers which panels you opened and closed", () => {
    let open = loadTrayOpen();
    open = toggleTrayPanel(open, "styles");
    open = toggleTrayPanel(open, "entity");
    saveTrayOpen(open);
    const back = loadTrayOpen();
    expect(back.styles).toBe(true);
    expect(back.entity).toBe(false);
    expect(back.materials).toBe(true);
  });

  it("reads bad saved values back as the defaults", () => {
    expect(readTrayOpen("not json")).toEqual(TRAY_DEFAULT_OPEN);
    expect(readTrayOpen("[1,2]")).toEqual(TRAY_DEFAULT_OPEN);
    expect(readTrayOpen("null")).toEqual(TRAY_DEFAULT_OPEN);
    expect(readTrayOpen('{"styles":"yes","entity":0,"nope":true,"__proto__":{"tags":true}}')).toEqual(TRAY_DEFAULT_OPEN);
    expect(readTrayOpen('{"tags":true,"entity":false}')).toEqual({ ...TRAY_DEFAULT_OPEN, tags: true, entity: false });
  });
});
