import { describe, expect, it } from "vitest";
import { contextEventAction, isMenuKey, isRightClick } from "../src/viewport/rightclick";

describe("isRightClick", () => {
  it("a still press is a click, however long", () => {
    expect(isRightClick(0, 50)).toBe(true);
    expect(isRightClick(0, 5000)).toBe(true);
  });
  it("jitter of a few pixels is still a click (high-DPI mice)", () => {
    expect(isRightClick(5, 120)).toBe(true);
    expect(isRightClick(5.9, 900)).toBe(true);
  });
  it("a quick press may move a little more", () => {
    expect(isRightClick(10, 200)).toBe(true);
  });
  it("a slow press that moved is a pan", () => {
    expect(isRightClick(10, 600)).toBe(false);
    expect(isRightClick(6, 400)).toBe(false);
  });
  it("a real drag is a pan, quick or not", () => {
    expect(isRightClick(40, 100)).toBe(false);
    expect(isRightClick(12, 100)).toBe(false);
    expect(isRightClick(300, 2000)).toBe(false);
  });
});

describe("contextEventAction", () => {
  const idle = { pressed: false, sinceMenuMs: 1e9, sincePanMs: 1e9 };
  it("opens the menu for a contextmenu event nothing else handled", () => {
    expect(contextEventAction(idle)).toBe("open");
  });
  it("waits while the button is still down", () => {
    expect(contextEventAction({ ...idle, pressed: true })).toBe("wait");
  });
  it("skips when our menu just opened for this click (never two menus)", () => {
    expect(contextEventAction({ ...idle, sinceMenuMs: 20 })).toBe("skip");
  });
  it("skips the contextmenu that Windows raises after a right-drag pan", () => {
    expect(contextEventAction({ ...idle, sincePanMs: 30 })).toBe("skip");
  });
  it("opens again for a later, separate click", () => {
    expect(contextEventAction({ ...idle, sinceMenuMs: 5000, sincePanMs: 5000 })).toBe("open");
  });
});

describe("isMenuKey", () => {
  it("takes the Menu key and Shift+F10", () => {
    expect(isMenuKey({ key: "ContextMenu", shiftKey: false })).toBe(true);
    expect(isMenuKey({ key: "F10", shiftKey: true })).toBe(true);
  });
  it("leaves plain F10 and other chords alone", () => {
    expect(isMenuKey({ key: "F10", shiftKey: false })).toBe(false);
    expect(isMenuKey({ key: "F10", shiftKey: true, ctrlKey: true })).toBe(false);
    expect(isMenuKey({ key: "a", shiftKey: true })).toBe(false);
  });
});
