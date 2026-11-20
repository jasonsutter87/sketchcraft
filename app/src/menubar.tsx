// The menu bar in the top bar: File, Edit, View, Camera, Draw, Tools, Window, Help.
// What is in each menu is menumodel.ts; what an item does is menuactions.ts; this file only draws them and handles the
// way desktop menus behave:
//   - click a name to open its menu; with one open, pointing at another name opens that one instead
//   - Up/Down move through the items, Right opens a submenu (or goes to the next menu), Left closes it (or goes to the
//     previous menu), Enter/Space runs the item, Esc closes; Home/End jump to the first and last item
//   - F10 focuses the menu bar (so does tapping Alt on Windows and Linux; on a Mac Option is the copy key, so not there)
// The roles are menubar > menuitem (with a menu) > menuitem / menuitemcheckbox / menuitemradio.

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { isMac, isTypingTarget } from "./keymap";
import { keyText, useKeymap } from "./keymapstore";
import { useLocale } from "./locale";
import { shortToolName, type Menu, type MenuItem } from "./menumodel";
import type { MenuAction } from "./menuactions";
import { useEscapeToClose } from "./sheet";

/** a command a plugin adds (shown under File > Plugins) */
export interface PluginEntry {
  key: string;
  label: string;
  title?: string;
  disabled?: boolean;
  run: () => void;
}

const ITEMS = "[role^='menuitem']:not([disabled])";
const unfocus = () => (document.activeElement as HTMLElement | null)?.blur?.();

export function MenuBar({ menus, actions, plugins, onOpenPlugins }: {
  menus: Menu[];
  actions: Record<string, MenuAction>;
  plugins: PluginEntry[];
  /** File > Plugins was opened: look for new plugins */
  onOpenPlugins: () => void;
}) {
  const { t } = useLocale();
  useKeymap();
  const [open, setOpen] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  /** focus the first or last item once the menu that was just opened is on screen */
  const [focusIn, setFocusIn] = useState<"first" | "last" | null>(null);
  const tops = () => [...(bar.current?.querySelectorAll<HTMLElement>(":scope > .menubar-item > [data-top]") ?? [])];
  const focusTop = (i: number) => tops()[(i + menus.length) % menus.length]?.focus();
  const close = () => setOpen(null);
  const openMenu = (i: number, focus: "first" | "last" | null = null) => {
    if (menus[i].id === "file") onOpenPlugins();
    setOpen(i);
    setFocusIn(focus);
  };
  useEscapeToClose(() => {
    const i = open;
    close();
    if (i != null) focusTop(i); // the bar keeps the focus, so the arrow keys still work; Esc again leaves it
  }, open !== null);

  // F10 (and a tap on Alt, off the Mac) puts the focus in the menu bar
  useEffect(() => {
    let altAlone = false;
    const down = (e: KeyboardEvent) => {
      altAlone = e.key === "Alt" && !e.repeat && !e.ctrlKey && !e.metaKey && !e.shiftKey;
      if (e.key === "F10" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && !isTypingTarget(e.target)) {
        e.preventDefault();
        focusTop(0);
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Alt" && altAlone && !isMac() && !isTypingTarget(e.target)) {
        e.preventDefault();
        focusTop(0);
      }
      altAlone = false;
    };
    const cancel = () => (altAlone = false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("pointerdown", cancel, true);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("pointerdown", cancel, true);
    };
  });

  const onKeyDown = (e: ReactKeyboardEvent) => {
    const el = e.target as HTMLElement;
    const top = el.closest<HTMLElement>("[data-top]");
    const menu = el.closest<HTMLElement>("[role='menu']");
    const key = e.key;
    if (top && !menu) {
      const i = Number(top.dataset.top);
      if (key === "ArrowRight" || key === "ArrowLeft") {
        e.preventDefault();
        const j = (i + (key === "ArrowRight" ? 1 : -1) + menus.length) % menus.length;
        if (open !== null) openMenu(j, "first");
        focusTop(j);
      } else if (key === "ArrowDown" || key === "ArrowUp") {
        e.preventDefault();
        openMenu(i, key === "ArrowDown" ? "first" : "last");
      } else if (key === "Escape") unfocus();
      return;
    }
    if (!menu) return;
    const items = [...menu.querySelectorAll<HTMLElement>(ITEMS)].filter((x) => x.closest("[role='menu']") === menu);
    const at = items.indexOf(el);
    if (key === "ArrowDown" || key === "ArrowUp") {
      e.preventDefault();
      items[(at + (key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
    } else if (key === "Home" || key === "End") {
      e.preventDefault();
      items[key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (key === "ArrowRight") {
      e.preventDefault();
      if (el.getAttribute("aria-haspopup") === "menu") {
        if (el.getAttribute("aria-expanded") === "true") (el.nextElementSibling as HTMLElement | null)?.querySelector<HTMLElement>(ITEMS)?.focus();
        else el.click(); // opens its submenu and focuses its first item
      } else if (open !== null) {
        openMenu((open + 1) % menus.length, "first");
        focusTop((open + 1) % menus.length);
      }
    } else if (key === "ArrowLeft") {
      e.preventDefault();
      const trigger = menu.previousElementSibling as HTMLElement | null;
      if (menu.dataset.sub && trigger) {
        trigger.focus();
        trigger.click(); // closes it
      } else if (open !== null) {
        openMenu((open + menus.length - 1) % menus.length, "first");
        focusTop((open + menus.length - 1) % menus.length);
      }
    }
  };

  return (
    <nav className="menubar-wrap" aria-label="Menu bar">
      {open !== null && <div className="menu-scrim" onPointerDown={close} />}
      <div className="menubar" role="menubar" ref={bar} onKeyDown={onKeyDown}>
        {menus.map((m, i) => (
          <div key={m.id} className="menubar-item">
            <button
              type="button"
              role="menuitem"
              data-top={i}
              data-menu={m.id}
              aria-haspopup="menu"
              aria-expanded={open === i}
              tabIndex={i === 0 ? 0 : -1}
              className={open === i ? "on" : ""}
              onClick={() => (open === i ? close() : openMenu(i))}
              onPointerEnter={(e) => e.pointerType === "mouse" && open !== null && open !== i && openMenu(i)}
            >
              {t(m.label)}
            </button>
            {open === i && (
              <MenuList
                label={t(m.label)}
                items={m.items}
                actions={actions}
                plugins={plugins}
                done={() => (close(), unfocus())}
                focusFirst={focusIn}
                top
              />
            )}
          </div>
        ))}
      </div>
    </nav>
  );
}

/** one menu's list of items (the drop-down of a name, or a submenu) */
function MenuList({ label, items, actions, plugins, done, focusFirst, top, sub }: {
  label: string;
  items: MenuItem[];
  actions: Record<string, MenuAction>;
  plugins: PluginEntry[];
  done: () => void;
  focusFirst?: "first" | "last" | null;
  top?: boolean;
  sub?: boolean;
}) {
  const { t } = useLocale();
  const root = useRef<HTMLDivElement>(null);
  const [openSub, setOpenSub] = useState<string | null>(null);
  const [place, setPlace] = useState<CSSProperties | undefined>(undefined);

  useLayoutEffect(() => {
    const list = root.current;
    if (!list) return;
    if (sub) {
      // a submenu is fixed beside its name (a scrolling menu would clip it); on the left when there's no room on the right
      const trigger = list.previousElementSibling as HTMLElement;
      const r = trigger.getBoundingClientRect();
      const parent = (trigger.closest("[role='menu']") ?? trigger).getBoundingClientRect();
      const w = list.offsetWidth;
      const left = parent.right + w + 8 > window.innerWidth ? Math.max(4, parent.left - w + 2) : parent.right - 2;
      const maxHeight = window.innerHeight - 16;
      const top = Math.max(8, Math.min(r.top - 6, window.innerHeight - Math.min(list.scrollHeight, maxHeight) - 8));
      setPlace({ left, top, maxHeight });
    }
    if (focusFirst) {
      const found = [...list.querySelectorAll<HTMLElement>(ITEMS)].filter((x) => x.closest("[role='menu']") === list);
      (focusFirst === "first" ? found[0] : found[found.length - 1])?.focus();
    }
  }, [sub, focusFirst]);

  const visible = (items: MenuItem[]) => items.filter((it) => !("id" in it) || !actions[it.id]?.hidden);
  // no separator at the start or end, none twice in a row
  const shown = visible(items).filter((it, i, all) => !("sep" in it) || (i > 0 && i < all.length - 1 && !("sep" in all[i - 1])));

  return (
    <div
      ref={root}
      role="menu"
      aria-label={label}
      data-sub={sub ? "1" : undefined}
      className={`menubar-menu ${top ? "top" : "sub"}`}
      style={sub ? { ...(place ?? { visibility: "hidden" }) } : undefined}
    >
      {shown.map((it, n) => {
        if ("sep" in it) return <hr key={`s${n}`} role="separator" />;
        if ("slot" in it) {
          return plugins.map((p) => (
            <button key={p.key} type="button" role="menuitem" className="mi" disabled={p.disabled} title={p.title} onPointerEnter={() => setOpenSub(null)} onClick={() => (p.run(), done())}>
              <span className="mi-mark" />
              <span className="mi-label">{p.label}</span>
            </button>
          ));
        }
        if ("items" in it) {
          const name = t(it.label);
          const isOpen = openSub === it.label;
          return [
            <button
              key={`b${it.label}`}
              type="button"
              role="menuitem"
              className={`mi ${isOpen ? "on" : ""}`}
              aria-haspopup="menu"
              aria-expanded={isOpen}
              onClick={(e) => {
                // a mouse click keeps it open: hovering already opened it, and the click must not shut it again
                // (e.detail is 0 for Enter/Space, which does toggle)
                const close = isOpen && e.detail === 0;
                setOpenSub(close ? null : it.label);
                if (!isOpen) {
                  // the submenu mounts on the next render: focus its first item then
                  const btn = e.currentTarget;
                  requestAnimationFrame(() => (btn.nextElementSibling as HTMLElement | null)?.querySelector<HTMLElement>(ITEMS)?.focus());
                }
              }}
              onPointerEnter={(e) => e.pointerType === "mouse" && setOpenSub(it.label)}
            >
              <span className="mi-mark" />
              <span className="mi-label">{name}</span>
              <span className="mi-caret" aria-hidden="true">▸</span>
            </button>,
            isOpen && <MenuList key={`m${it.label}`} label={name} items={it.items} actions={actions} plugins={plugins} done={done} sub />,
          ];
        }
        const a = actions[it.id];
        const name = it.text ?? (it.tool ? shortToolName(t(it.label ?? it.id)) : t(it.label ?? it.id));
        const keys = keyText(it.id);
        const role = it.kind === "check" ? "menuitemcheckbox" : it.kind === "radio" ? "menuitemradio" : "menuitem";
        return (
          <button
            key={it.id + n}
            type="button"
            role={role}
            className="mi"
            data-action={it.id}
            disabled={!a || !a.enabled}
            aria-checked={it.kind ? !!a?.checked : undefined}
            onPointerEnter={() => setOpenSub(null)}
            onClick={() => (a?.run(), done())}
          >
            <span className="mi-mark" aria-hidden="true">{a?.checked ? (it.kind === "radio" ? "•" : "✓") : ""}</span>
            <span className="mi-label">{name}</span>
            {keys && <kbd>{keys}</kbd>}
          </button>
        );
      })}
    </div>
  );
}
