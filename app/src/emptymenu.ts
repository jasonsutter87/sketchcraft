// The right-click menu on empty space in the view: the same actions the Edit, Camera and View menus run
// (menuactions.ts), so nothing here has a second implementation. Pure: the Vitest suite builds it with stand-ins.

import type { MenuAction } from "./menuactions";
import type { MenuItem } from "./contextmenu";

/** anything in the model that is hidden (so "Unhide all" has something to do) */
export function anythingHidden(scene: { faces: { hidden: boolean }[]; groups: { hidden: boolean }[]; instances: { hidden: boolean }[] }): boolean {
  return scene.faces.some((f) => f.hidden) || scene.groups.some((g) => g.hidden) || scene.instances.some((i) => i.hidden);
}

/** [action id, label id, English label] for each entry that runs a menu action */
type Entry = [string, string, string];

const VIEWS = ["iso", "top", "front", "right", "back", "left"];

export function emptySpaceItems(actions: Record<string, MenuAction>, hidden: boolean): MenuItem[] {
  const items: MenuItem[] = [];
  const add = ([id, labelId, label]: Entry, over: Partial<MenuItem> = {}) => {
    const a = actions[id];
    if (!a || a.hidden) return;
    items.push({ label, labelId, run: a.run, disabled: !a.enabled, ...(a.checked === undefined ? {} : { checked: a.checked }), ...over });
  };
  const heading = (label: string, labelId?: string) => items.push({ label, labelId, heading: true, run: () => {} });
  add(["edit.selectall", "menu.selectAll", "Select all"]);
  add(["edit.paste", "menu.paste", "Paste"]);
  add(["edit.pasteinplace", "menu.pasteInPlace", "Paste in place"]);
  add(["view.fit", "menu.fit", "Zoom extents"]);
  add(["edit.unhide", "menu.unhide", "Unhide all"], { disabled: !hidden || !actions["edit.unhide"]?.enabled });
  heading("Camera", "menu.camera");
  add(["view.perspective", "menu.perspective", "Perspective"]);
  add(["view.parallel", "menu.parallel", "Parallel projection"]);
  heading("Standard views", "menu.standardViews");
  for (const v of VIEWS) add([`view.${v}`, `menu.view.${v}`, v[0].toUpperCase() + v.slice(1)]);
  heading("Style");
  add(["view.xray", "menu.xray", "X-Ray"]);
  add(["view.shadows", "menu.shadows", "Shadows"]);
  add(["view.edges", "menu.edges", "Edges"]);
  return items;
}
