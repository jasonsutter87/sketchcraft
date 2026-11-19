import { describe, expect, it } from "vitest";
import english from "../src/locale-en";
import italian from "../src/locale-it";
import { formatCombo, presetBindings } from "../src/keymap";
import { buildMenuActions, type MenuCtx } from "../src/menuactions";
import { leaves, MENUS, type MenuItem } from "../src/menumodel";
import { TOOL_INFO } from "../src/workspace";

/** a stand-in for the app: every handler records its own name */
function stub(over: Partial<MenuCtx> = {}): { ctx: MenuCtx; calls: string[] } {
  const calls: string[] = [];
  const rec = (path: string): unknown =>
    new Proxy(() => {}, {
      get: (_t, key) => rec(`${path}.${String(key)}`),
      apply: (_t, _this, args) => void calls.push(`${path}(${args.join(",")})`),
    });
  const group = (name: string) => rec(name) as never;
  const ctx = {
    selection: { total: 2, faces: 1, groups: 1, instances: 1 },
    canPaste: true,
    editing: true,
    section: true,
    onPaper: false,
    workspace: "model",
    styles: { xray: false, backEdges: false, shadows: false, edges: true, parallel: false },
    layout: "classic",
    language: "system",
    showLeft: true,
    showRight: true,
    leftTabs: ["outliner", "library", "views"],
    desktop: true,
    file: group("file"),
    edit: group("edit"),
    tool: rec("tool"),
    view: rec("view"),
    fit: rec("fit"),
    previousView: rec("previousView"),
    nextView: rec("nextView"),
    plan: rec("plan"),
    clearSection: rec("clearSection"),
    toggleStyle: rec("toggleStyle"),
    setStyles: rec("setStyles"),
    workspaceTo: rec("workspaceTo"),
    showTab: rec("showTab"),
    showRightPanel: rec("showRightPanel"),
    togglePanel: rec("togglePanel"),
    setLayout: rec("setLayout"),
    setLanguage: rec("setLanguage"),
    open: group("open"),
    checkForUpdates: rec("checkForUpdates"),
    openLink: rec("openLink"),
    ...over,
  } as unknown as MenuCtx;
  return { ctx, calls };
}

const ids = leaves().map((l) => l.id);

describe("the menus", () => {
  it("are File, Edit, View, Camera, Draw, Tools, Window and Help", () => {
    expect(MENUS.map((m) => m.id)).toEqual(["file", "edit", "view", "camera", "draw", "tools", "window", "help"]);
  });

  it("have an action for every item, and each action does exactly one thing", () => {
    const { ctx, calls } = stub();
    const actions = buildMenuActions(ctx);
    for (const id of ids) {
      expect(actions[id], id).toBeDefined();
      const before = calls.length;
      actions[id].run();
      expect(calls.length - before, id).toBe(1);
    }
  });

  it("have no action that no menu offers", () => {
    const offered = new Set(ids);
    for (const id of Object.keys(buildMenuActions(stub().ctx))) expect(offered.has(id), id).toBe(true);
  });

  it("send File > New and the tools to the same handlers the keys use", () => {
    const { ctx, calls } = stub();
    const a = buildMenuActions(ctx);
    a["file.new"].run();
    a["tool:room"].run();
    a["view.xray"].run();
    a["file.export.glb"].run();
    a["file.export.fbx"].run();
    expect(calls).toEqual(["file.new()", "tool(room)", "toggleStyle(xray)", "file.export(glb)", "file.export(fbx)"]);
  });

  it("offer only tools the app has", () => {
    for (const id of ids.filter((i) => i.startsWith("tool:"))) expect(Object.hasOwn(TOOL_INFO, id.slice(5)), id).toBe(true);
  });

  it("show the key each command has in the shortcuts list", () => {
    const keys = presetBindings("sketchcraft");
    const shown = (id: string) => formatCombo(keys[id] ?? "", true);
    for (const id of ["file.new", "file.open", "file.save", "file.saveas", "file.import", "edit.undo", "edit.redo", "edit.cut", "edit.copy", "edit.paste", "edit.pasteinplace", "edit.delete", "edit.selectall", "edit.makecomponent", "view.fit", "view.top", "view.backedges", "tool:line", "tool:rect", "tool:push", "tool:move", "tool:erase", "tool:orbit", "help.shortcuts", "help.search"]) {
      expect(ids, id).toContain(id);
      expect(shown(id), id).not.toBe("");
    }
    expect(shown("file.save")).toBe("⌘S");
    expect(shown("edit.redo")).toBe("⇧⌘Z");
    expect(shown("tool:line")).toBe("L");
  });

  it("have a name in English and in Italian for every label", () => {
    const labels: string[] = [];
    const walk = (items: MenuItem[]) =>
      items.forEach((it) => {
        if ("items" in it) (labels.push(it.label), walk(it.items));
        else if ("id" in it && it.label) labels.push(it.label);
      });
    MENUS.forEach((m) => (labels.push(m.label), walk(m.items)));
    for (const l of labels) {
      expect(Object.hasOwn(english, l), l).toBe(true);
      expect(Object.hasOwn(italian, l), l).toBe(true);
    }
    // an item names itself one way or the other
    for (const l of leaves()) expect(l.label ?? l.text, l.id).toBeTruthy();
  });
});

describe("what is available", () => {
  it("greys out Cut, Copy, Delete, Group and Hide with nothing selected, and Paste with nothing copied", () => {
    const { ctx } = stub({ selection: { total: 0, faces: 0, groups: 0, instances: 0 }, canPaste: false });
    const a = buildMenuActions(ctx);
    for (const id of ["edit.cut", "edit.copy", "edit.delete", "edit.hide", "edit.group", "edit.makecomponent", "edit.explode", "edit.paste", "edit.pasteinplace", "edit.selectnone"]) expect(a[id].enabled, id).toBe(false);
    for (const id of ["edit.undo", "edit.selectall", "edit.unhide", "file.new"]) expect(a[id].enabled, id).toBe(true);
  });

  it("turns on Explode only for components, and Group for loose faces or several things", () => {
    const one = buildMenuActions(stub({ selection: { total: 1, faces: 0, groups: 1, instances: 0 } }).ctx);
    expect(one["edit.explode"].enabled).toBe(false);
    expect(one["edit.group"].enabled).toBe(false);
    expect(one["edit.makecomponent"].enabled).toBe(true);
    const inst = buildMenuActions(stub({ selection: { total: 1, faces: 0, groups: 0, instances: 1 } }).ctx);
    expect(inst["edit.explode"].enabled).toBe(true);
  });

  it("greys out the model's tools and views on paper, but not the file commands", () => {
    const a = buildMenuActions(stub({ onPaper: true, workspace: "layout" }).ctx);
    for (const id of ["tool:line", "tool:orbit", "view.top", "view.fit", "view.xray", "edit.copy"]) expect(a[id].enabled, id).toBe(false);
    for (const id of ["file.save", "help.about", "ws.model"]) expect(a[id].enabled, id).toBe(true);
  });

  it("offers Section export, Remove section and Hide rest only when they apply", () => {
    const off = buildMenuActions(stub({ section: false, editing: false }).ctx);
    expect(off["file.export.section.svg"].enabled).toBe(false);
    expect(off["view.section.clear"].enabled).toBe(false);
    expect(off["view.plan"].enabled).toBe(true);
    expect(off["view.hiderest"].enabled).toBe(false);
    const on = buildMenuActions(stub().ctx);
    expect(on["file.export.section.svg"].enabled).toBe(true);
    expect(on["view.section.clear"].enabled).toBe(true);
    expect(on["view.plan"].enabled).toBe(false);
  });

  it("hides Check for updates outside the desktop app, and the Library on paper", () => {
    expect(buildMenuActions(stub({ desktop: false }).ctx)["help.updates"].hidden).toBe(true);
    expect(buildMenuActions(stub({ desktop: true }).ctx)["help.updates"].hidden).toBe(false);
    expect(buildMenuActions(stub({ leftTabs: ["pages", "outliner", "views"] }).ctx)["win.library"].enabled).toBe(false);
  });

  it("ticks the current workspace, face style, projection, layout, language and panels", () => {
    const a = buildMenuActions(stub({ workspace: "arch", layout: "side", language: "it", showLeft: false, styles: { xray: true, backEdges: false, shadows: true, edges: true, parallel: true, face: "wireframe", background: "night", notes: false } }).ctx);
    expect(a["ws.arch"].checked).toBe(true);
    expect(a["ws.model"].checked).toBe(false);
    expect(a["view.face.wireframe"].checked).toBe(true);
    expect(a["view.face.textured"].checked).toBe(false);
    expect(a["view.bg.night"].checked).toBe(true);
    expect(a["view.parallel"].checked).toBe(true);
    expect(a["view.perspective"].checked).toBe(false);
    expect(a["view.layout.side"].checked).toBe(true);
    expect(a["lang.it"].checked).toBe(true);
    expect(a["view.panel.left"].checked).toBe(false);
    expect(a["view.panel.right"].checked).toBe(true);
    expect(a["view.xray"].checked).toBe(true);
    expect(a["view.shadows"].checked).toBe(true);
    expect(a["view.notes"].checked).toBe(false);
    expect(a["view.coach"].checked).toBe(true); // on unless turned off
  });
});
