import { describe, expect, it } from "vitest";
import { anythingHidden, emptySpaceItems } from "../src/emptymenu";
import type { MenuAction } from "../src/menuactions";

const acts = (over: Record<string, Partial<MenuAction>> = {}) => {
  const ran: string[] = [];
  const ids = ["edit.selectall", "edit.paste", "edit.pasteinplace", "view.fit", "edit.unhide", "view.perspective", "view.parallel", "view.iso", "view.top", "view.front", "view.right", "view.back", "view.left", "view.xray", "view.shadows", "view.edges"];
  const out: Record<string, MenuAction> = {};
  for (const id of ids) out[id] = { run: () => void ran.push(id), enabled: true, ...over[id] };
  return { out, ran };
};

describe("the right-click menu on empty space", () => {
  it("offers select all, paste, zoom extents, unhide, the camera and the style", () => {
    const { out } = acts();
    const labels = emptySpaceItems(out, true).map((i) => i.label);
    for (const l of ["Select all", "Paste", "Zoom extents", "Unhide all", "Perspective", "Parallel projection", "Top", "Iso", "X-Ray", "Shadows", "Edges"]) expect(labels).toContain(l);
  });
  it("greys out Paste when the clipboard is empty, and Unhide all when nothing is hidden", () => {
    const { out } = acts({ "edit.paste": { enabled: false }, "edit.pasteinplace": { enabled: false } });
    const items = emptySpaceItems(out, false);
    const by = (l: string) => items.find((i) => i.label === l)!;
    expect(by("Paste").disabled).toBe(true);
    expect(by("Paste in place").disabled).toBe(true);
    expect(by("Unhide all").disabled).toBe(true);
    expect(by("Select all").disabled).toBe(false);
    expect(emptySpaceItems(out, true).find((i) => i.label === "Unhide all")!.disabled).toBe(false);
  });
  it("shows which projection and styles are on, and runs the menu's own action", () => {
    const { out, ran } = acts({ "view.parallel": { checked: true }, "view.perspective": { checked: false }, "view.xray": { checked: false } });
    const items = emptySpaceItems(out, true);
    expect(items.find((i) => i.label === "Parallel projection")!.checked).toBe(true);
    expect(items.find((i) => i.label === "Perspective")!.checked).toBe(false);
    items.find((i) => i.label === "Top")!.run();
    expect(ran).toEqual(["view.top"]);
  });
  it("leaves out an action that is not there", () => {
    const { out } = acts();
    delete out["edit.pasteinplace"];
    expect(emptySpaceItems(out, true).map((i) => i.label)).not.toContain("Paste in place");
  });
  it("finds hidden things", () => {
    const none = { faces: [{ hidden: false }], groups: [], instances: [] };
    expect(anythingHidden(none)).toBe(false);
    expect(anythingHidden({ ...none, groups: [{ hidden: true }] })).toBe(true);
  });
});
