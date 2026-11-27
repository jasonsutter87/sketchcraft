// The Desktop layout on a Mac: the menus live in the system menu bar, like SketchUp's, instead of
// in the window. Built from the same menu model (menumodel.ts) and actions (menuactions.ts) as the
// in-window menu bar. Items carry no key equivalents: the app's own key map still handles every
// shortcut, as it does with the in-window menus.
//
// Edit also keeps the system's Cut, Copy, Paste and Select All ("… Text"): on a Mac, text fields
// only get those keys through the app's Edit menu.

import { useEffect, useRef } from "react";
import { inTauri } from "./engine";
import { isMac } from "./keymap";
import type { MenuAction } from "./menuactions";
import { shortToolName, type Menu as MenuModel, type MenuItem as MenuModelItem } from "./menumodel";
import type { PluginEntry } from "./menubar";

type Translate = (id: string) => string;

/** Is there a system menu bar to use? (the desktop app on a Mac) */
export const hasNativeMenus = () => inTauri && isMac();

/** what decides how the menus look: rebuilt only when this changes */
function signature(menus: MenuModel[], actions: Record<string, MenuAction>, plugins: PluginEntry[], t: Translate) {
  const ids: string[] = [];
  const walk = (items: MenuModelItem[]) => {
    for (const it of items) {
      if ("id" in it) {
        const a = actions[it.id];
        ids.push(`${it.id}:${a ? `${+a.enabled}${+!!a.checked}${+!!a.hidden}` : "-"}`);
      } else if ("items" in it) walk(it.items);
    }
  };
  for (const m of menus) walk(m.items);
  return `${t("menu.file")}|${ids.join(",")}|${plugins.map((p) => `${p.key}:${+!!p.disabled}`).join(",")}`;
}

export function useNativeMenus(on: boolean, menus: MenuModel[], actions: Record<string, MenuAction>, plugins: PluginEntry[], t: Translate) {
  // the latest actions, so an item built a while ago still runs today's version
  const live = useRef({ actions, plugins });
  live.current = { actions, plugins };
  const sig = on ? signature(menus, actions, plugins, t) : "";
  const wasOn = useRef(false);

  useEffect(() => {
    if (!on) {
      // leaving the Desktop layout: give the window the standard menu back
      if (wasOn.current) {
        wasOn.current = false;
        void import("@tauri-apps/api/menu").then(({ Menu }) => Menu.default()).then((m) => m.setAsAppMenu()).catch(() => {});
      }
      return;
    }
    wasOn.current = true;
    let stale = false;
    void (async () => {
      const { Menu, Submenu, MenuItem, CheckMenuItem, PredefinedMenuItem } = await import("@tauri-apps/api/menu");
      const sep = () => PredefinedMenuItem.new({ item: "Separator" });
      const run = (id: string) => () => live.current.actions[id]?.run();

      const build = async (items: MenuModelItem[]): Promise<Awaited<ReturnType<typeof sep>>[]> => {
        const out = [];
        for (const it of items) {
          if ("sep" in it) {
            // no separator first, last or twice in a row (hidden items can leave them stranded)
            if (out.length && !(out[out.length - 1] instanceof PredefinedMenuItem)) out.push(await sep());
          } else if ("slot" in it) {
            for (const p of live.current.plugins) out.push(await MenuItem.new({ id: `plugin:${p.key}`, text: p.label, enabled: !p.disabled, action: () => live.current.plugins.find((q) => q.key === p.key)?.run() }));
          } else if ("items" in it) {
            const sub = await build(it.items);
            if (sub.length) out.push(await Submenu.new({ text: t(it.label), items: sub }));
          } else {
            const a = live.current.actions[it.id];
            if (!a || a.hidden) continue;
            const text = it.text ?? (it.tool ? shortToolName(t(it.label ?? it.id)) : t(it.label ?? it.id));
            out.push(it.kind ? await CheckMenuItem.new({ id: it.id, text, enabled: a.enabled, checked: !!a.checked, action: run(it.id) }) : await MenuItem.new({ id: it.id, text, enabled: a.enabled, action: run(it.id) }));
          }
        }
        while (out.length && out[out.length - 1] instanceof PredefinedMenuItem) out.pop();
        return out as Awaited<ReturnType<typeof sep>>[];
      };

      const app = await Submenu.new({
        text: "SketchCraft",
        items: [
          await PredefinedMenuItem.new({ item: { About: null } }),
          await sep(),
          await PredefinedMenuItem.new({ item: "Services" }),
          await sep(),
          await PredefinedMenuItem.new({ item: "Hide" }),
          await PredefinedMenuItem.new({ item: "HideOthers" }),
          await PredefinedMenuItem.new({ item: "ShowAll" }),
          await sep(),
          await PredefinedMenuItem.new({ item: "Quit" }),
        ],
      });
      const top = [app];
      for (const m of menus) {
        const items = await build(m.items);
        if (m.id === "edit") {
          items.push(
            await sep(),
            await PredefinedMenuItem.new({ item: "Cut", text: "Cut Text" }),
            await PredefinedMenuItem.new({ item: "Copy", text: "Copy Text" }),
            await PredefinedMenuItem.new({ item: "Paste", text: "Paste Text" }),
            await PredefinedMenuItem.new({ item: "SelectAll", text: "Select All Text" }),
          );
        }
        if (m.id === "window") items.push(await sep(), await PredefinedMenuItem.new({ item: "Minimize" }), await PredefinedMenuItem.new({ item: "Fullscreen" }));
        if (items.length) top.push(await Submenu.new({ text: t(m.label), items }));
      }
      if (stale) return;
      const bar = await Menu.new({ items: top });
      if (!stale) await bar.setAsAppMenu();
    })().catch(() => {});
    return () => {
      stale = true;
    };
    // `sig` stands for menus, actions, plugins and language
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, sig]);
}
