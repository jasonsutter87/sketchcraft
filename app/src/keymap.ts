// Keyboard shortcuts: one list of everything that can have a key (tools, menu commands, plugin
// commands), each with a default; the presets; your changes, read back safely; and the rules that
// keep a key from being given to two commands or taken from the keys the app needs for itself.
//
// This file is pure (no React, no storage), so every rule here is covered by a Vitest test.
// The saved copy and the live hook are in keymapstore.ts.

import { TOOL_INFO } from "./workspace";

/** A key as text, always in one spelling: "L", "Shift+Z", "Mod+Shift+S", "Space", "Mod+1". Mod is ⌘ on a Mac, Ctrl elsewhere. */
export type Combo = string;

export type CommandGroup = "Tools" | "Camera" | "File" | "Edit" | "View" | "Window" | "Plugins";

export interface CommandDef {
  /** "tool:line", "edit.undo", "plugin:<plugin>:<command>" */
  id: string;
  label: string;
  group: CommandGroup;
  /** the key it has in the SketchCraft preset ("" for none) */
  key: Combo;
}

// ---- keys as text -------------------------------------------------------------------------

const SYMBOLS = "[]\\;',./-=`?";
const MODS = ["Mod", "Alt", "Shift"] as const;

/** Other spellings people (and other programs' files) use, read as ours. */
const ALIASES: Record<string, string> = {
  spacebar: "Space", space: "Space", " ": "Space",
  return: "Enter", enter: "Enter", esc: "Escape", escape: "Escape", tab: "Tab",
  bksp: "Backspace", backspace: "Backspace", del: "Delete", delete: "Delete", ins: "Insert", insert: "Insert",
  home: "Home", end: "End", pgup: "PageUp", pageup: "PageUp", pgdn: "PageDown", pagedown: "PageDown",
  up: "ArrowUp", down: "ArrowDown", left: "ArrowLeft", right: "ArrowRight",
  arrowup: "ArrowUp", arrowdown: "ArrowDown", arrowleft: "ArrowLeft", arrowright: "ArrowRight",
  plus: "=", minus: "-", comma: ",", period: ".", slash: "/", backslash: "\\", semicolon: ";",
};
const MOD_WORDS: Record<string, (typeof MODS)[number]> = {
  mod: "Mod", ctrl: "Mod", control: "Mod", cmd: "Mod", command: "Mod", meta: "Mod", "⌘": "Mod", "⌃": "Mod", win: "Mod",
  alt: "Alt", option: "Alt", opt: "Alt", "⌥": "Alt",
  shift: "Shift", "⇧": "Shift",
};

function keyName(k: string): string | null {
  if (k.length === 1) {
    if (/[a-z]/i.test(k)) return k.toUpperCase();
    if (/[0-9]/.test(k) || SYMBOLS.includes(k)) return k;
    if (k === " ") return "Space";
    return null;
  }
  const a = ALIASES[k.toLowerCase()];
  if (a) return a;
  const f = /^f([1-9]|1[0-2])$/i.exec(k);
  return f ? `F${f[1]}` : null;
}

/** Any spelling of a key ("ctrl+shift+z", "Cmd-Z", "Spacebar", "Shift + Z") as ours, or null if it isn't a key. */
export function normalizeCombo(text: unknown): Combo | null {
  if (typeof text !== "string") return null;
  let s = text.trim();
  if (!s || s.length > 40) return null;
  const mods = new Set<string>();
  // mac glyphs, glued to the key or to each other ("⇧⌘Z")
  while (s.length > 1 && "⌘⌃⌥⇧".includes(s[0])) {
    mods.add(MOD_WORDS[s[0]]);
    s = s.slice(1).trimStart();
  }
  // modifier words, each followed by + or - ("Ctrl+Shift+Z", "Cmd-Z", "Mod++")
  for (;;) {
    const m = /^([A-Za-z]+)\s*[+-]\s*(\S.*)$/.exec(s);
    if (!m) break;
    const word = MOD_WORDS[m[1].toLowerCase()];
    if (!word) return null;
    mods.add(word);
    s = m[2];
  }
  const key = s === "+" ? "=" : s;
  const name = keyName(key);
  if (!name) return null;
  // Shift is already part of a typed symbol ("?"): keep it out of the spelling
  if (name.length === 1 && !/[A-Z0-9]/.test(name)) mods.delete("Shift");
  return [...MODS.filter((m) => mods.has(m)), name].join("+");
}

/** The key combination an event stands for, in our spelling (null for a bare modifier press). */
export function comboFromEvent(e: Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">): Combo | null {
  if (["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock", "Dead", "Unidentified"].includes(e.key)) return null;
  let key = e.key;
  // letters and digits by the physical key when the character isn't one (Option+L types "¬" on a Mac; Shift+1 types "!")
  const code = /^Key([A-Z])$/.exec(e.code) ?? /^Digit([0-9])$/.exec(e.code);
  if (code) {
    if (!/^[\x21-\x7e]$/.test(key)) key = code[1]; // a character from the physical key (Option+L types "¬")
    else if (e.shiftKey && /^Digit/.test(e.code)) key = code[1]; // Shift+1 types "!", but is Shift+1
  }
  if (key === "/" && e.shiftKey) key = "?"; // some layouts type "/" with Shift
  const name = keyName(key);
  if (!name) return null;
  const mods: string[] = [];
  if (e.metaKey || e.ctrlKey) mods.push("Mod");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey && !(name.length === 1 && !/[A-Z0-9]/.test(name))) mods.push("Shift");
  return [...mods, name].join("+");
}

/** Typing in a box (text, a number, a drop-down, a rich-text area): shortcuts must not fire. */
export function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as { tagName?: string; isContentEditable?: boolean; getAttribute?: (n: string) => string | null } | null;
  if (!el || typeof el.tagName !== "string") return false;
  return ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName.toUpperCase()) || el.isContentEditable === true || el.getAttribute?.("role") === "textbox";
}

export const isMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

const GLYPH: Record<string, string> = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Backspace: "⌫", Enter: "↵", Escape: "Esc" };

/** A key as the person should read it: ⇧⌘Z on a Mac, Ctrl+Shift+Z elsewhere. */
export function formatCombo(combo: Combo, mac = isMac()): string {
  if (!combo) return "";
  const parts = combo.split("+");
  const key = combo.endsWith("++") ? "+" : parts[parts.length - 1];
  const mods = parts.slice(0, combo.endsWith("++") ? -2 : -1);
  const k = GLYPH[key] ?? key;
  if (mac) return [...mods.map((m) => (m === "Mod" ? "⌘" : m === "Alt" ? "⌥" : "⇧")).sort((a, b) => "⌥⇧⌘".indexOf(a) - "⌥⇧⌘".indexOf(b)), k].join("");
  return [...mods.map((m) => (m === "Mod" ? "Ctrl" : m)), k].join("+");
}

// ---- keys the app keeps for itself -------------------------------------------------------

/**
 * Why a key can't be given to a command (null when it can). `forId` lets the Select tool keep Space
 * (hold Space to orbit, tap it to select).
 */
export function reservedReason(combo: Combo, forId = ""): string | null {
  const parts = combo.split("+");
  const key = combo.endsWith("++") ? "+" : parts[parts.length - 1];
  const mod = parts.includes("Mod");
  const alt = parts.includes("Alt");
  if (key === "Escape") return "Esc cancels what you're doing, so it can't be a shortcut.";
  if (key === "Enter") return "Enter confirms a typed measurement (and leaves SubD mode), so it can't be a shortcut.";
  if (key === "Tab") return "Tab shapes a SubD object, so it can't be a shortcut.";
  if (key === "Backspace") return "Backspace is a second Delete key, so it can't be a shortcut.";
  if (key.startsWith("Arrow") && !mod && !alt) return "The arrow keys lock the direction to an axis (and steer Walk), so they can't be shortcuts.";
  if (!mod && !alt && !parts.includes("Shift") && /^[0-9.\-/]$/.test(key)) return "Typing a number starts an exact measurement, so these keys can't be shortcuts.";
  if (combo === "Space" && forId !== "tool:select") return "Space is the Select tool (and held, orbits), so only Select can have it.";
  if (combo === "Mod+Y") return "Ctrl+Y is kept as a second Redo.";
  if (mod && (key === "[" || key === "]")) return "These step through scenes, so they can't be shortcuts.";
  if (mod && !alt && ["Q", "W", "H", "M", "R"].includes(key) && !parts.includes("Shift")) return "Your computer uses this one (quit, close, hide, minimize, reload).";
  if (["F5", "F11", "F12"].includes(key)) return "Your computer uses this one (reload, full screen, developer tools).";
  return null;
}

// ---- the commands ------------------------------------------------------------------------

const CAMERA_TOOLS = new Set(["orbit", "pan", "zoom", "zoomwindow", "position", "lookaround", "walk"]);
/** The short name of a tool ("Move (Option/Ctrl to copy)" is "Move"; "Arc: ends, then bulge" is "Arc"). */
export const toolLabel = (name: string) => name.split(":")[0].split(" (")[0].trim();
/** names that would read the same in a list of commands */
const LABELS: Record<string, string> = { arc: "2-Point Arc", arccenter: "Arc (center)", text: "Text label", erase: "Eraser", section: "Section Plane" };

export const TOOL_COMMANDS: CommandDef[] = (Object.entries(TOOL_INFO) as [string, { name: string; key: string }][]).map(([id, t]) => ({
  id: `tool:${id}`,
  label: LABELS[id] ?? toolLabel(t.name),
  group: CAMERA_TOOLS.has(id) ? "Camera" : "Tools",
  key: t.key === "" ? "" : normalizeCombo(t.key) ?? "",
}));

const c = (id: string, label: string, group: CommandGroup, key = ""): CommandDef => ({ id, label, group, key });

export const MENU_COMMANDS: CommandDef[] = [
  c("file.new", "New model", "File", "Mod+N"),
  c("file.open", "Open…", "File", "Mod+O"),
  c("file.save", "Save", "File", "Mod+S"),
  c("file.saveas", "Save As…", "File", "Mod+Shift+S"),
  c("file.import", "Import…", "File", "Mod+I"),
  c("file.render", "Render…", "File"),
  c("file.timeline", "Timeline (video)…", "File"),
  c("edit.undo", "Undo", "Edit", "Mod+Z"),
  c("edit.redo", "Redo", "Edit", "Mod+Shift+Z"),
  c("edit.cut", "Cut", "Edit", "Mod+X"),
  c("edit.copy", "Copy", "Edit", "Mod+C"),
  c("edit.paste", "Paste", "Edit", "Mod+V"),
  c("edit.pasteinplace", "Paste in place", "Edit", "Mod+Shift+V"),
  c("edit.selectall", "Select All", "Edit", "Mod+A"),
  c("edit.selectnone", "Select None", "Edit", "Mod+T"),
  c("edit.delete", "Delete", "Edit", "Delete"),
  c("edit.makecomponent", "Make Component", "Edit", "G"),
  c("edit.unhide", "Unhide all", "Edit"),
  c("edit.clearguides", "Clear guides", "Edit"),
  c("view.fit", "Zoom Extents", "Camera", "Shift+Z"),
  c("view.iso", "Iso view", "Camera", "Mod+1"),
  c("view.top", "Top view", "Camera", "Mod+2"),
  c("view.front", "Front view", "Camera", "Mod+3"),
  c("view.right", "Right view", "Camera", "Mod+4"),
  c("view.back", "Back view", "Camera", "Mod+5"),
  c("view.left", "Left view", "Camera", "Mod+6"),
  c("view.backedges", "Back Edges", "View", "K"),
  c("view.xray", "X-Ray", "View"),
  c("view.shadows", "Shadows", "View"),
  c("view.edges", "Edges", "View"),
  c("view.parallel", "Parallel projection", "View"),
  c("view.hiderest", "Hide rest of model", "View"),
  c("ws.model", "Model workspace", "Window"),
  c("ws.arch", "Arch workspace", "Window"),
  c("ws.design", "Design workspace", "Window"),
  c("ws.previz", "Previz workspace", "Window"),
  c("win.themes", "Themes…", "Window"),
  c("win.plugins", "Manage plugins…", "Window"),
  c("help.shortcuts", "Keyboard shortcuts", "Window", "?"),
  c("help.search", "Search commands", "Window", "Shift+S"),
];

/** Every command that is always there (plugin commands are added as they load). */
export const COMMANDS: CommandDef[] = [...TOOL_COMMANDS, ...MENU_COMMANDS];
const BY_ID = new Map(COMMANDS.map((d) => [d.id, d]));
export const commandDef = (id: string) => BY_ID.get(id);

const PLUGIN_ID = /^plugin:[^:\s]{1,80}:[^\s]{1,80}$/;
/** an id the app knows now, or one a plugin command could have */
export const isKnownId = (id: string) => BY_ID.has(id) || PLUGIN_ID.test(id);

// ---- presets -----------------------------------------------------------------------------

export type PresetId = "sketchcraft" | "sketchup";

export interface Preset {
  id: PresetId;
  name: string;
  blurb: string;
  /** keys that differ from the SketchCraft defaults ("" removes one) */
  changes: Record<string, Combo>;
}

export const PRESETS: Preset[] = [
  { id: "sketchcraft", name: "SketchCraft", blurb: "The default: SketchUp's keys, plus a few more (N, J, D, W, X).", changes: {} },
  {
    id: "sketchup",
    name: "SketchUp",
    blurb: "SketchUp's own defaults: Space select, L line, R rectangle, P push/pull, M move, Q rotate, S scale, E eraser, B paint, T tape, O orbit, H pan, Z zoom, F offset, G component, K back edges. Tools SketchUp has no key for have none.",
    // SketchUp leaves these without a key: Polygon, Follow Me, Dimension, Text, Section Plane
    changes: { "tool:polygon": "", "tool:followme": "", "tool:dimension": "", "tool:text": "", "tool:section": "" },
  },
];
export const presetDef = (id: string) => PRESETS.find((p) => p.id === id);

/** What a preset gives each command (every known command appears, "" meaning no key). */
export function presetBindings(id: PresetId): Record<string, Combo> {
  const out: Record<string, Combo> = {};
  const p = presetDef(id);
  for (const d of COMMANDS) out[d.id] = p && Object.hasOwn(p.changes, d.id) ? p.changes[d.id] : d.key;
  return out;
}

// ---- your keys: what is saved, read back, and worked out ----------------------------------

/** What is saved: a preset, and your changes to it ("" meaning you took the key away). */
export interface KeymapState {
  preset: PresetId;
  overrides: Record<string, Combo>;
}

export const DEFAULT_STATE: KeymapState = { preset: "sketchcraft", overrides: {} };

/**
 * A saved value read back. Anything that isn't right (junk, the wrong shape, a key that doesn't
 * exist, a reserved key) is dropped on its own; what is right is kept. Never throws.
 */
export function readKeymap(raw: unknown): KeymapState {
  const out: KeymapState = { preset: "sketchcraft", overrides: {} };
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return out;
  const r = raw as Record<string, unknown>;
  if (typeof r.preset === "string" && presetDef(r.preset)) out.preset = r.preset as PresetId;
  const o = r.overrides;
  if (o && typeof o === "object" && !Array.isArray(o)) {
    let n = 0;
    for (const [id, v] of Object.entries(o as Record<string, unknown>)) {
      if (++n > 600) break;
      if (!isKnownId(id)) continue;
      if (v === "") {
        out.overrides[id] = "";
        continue;
      }
      const combo = normalizeCombo(v);
      if (combo && !reservedReason(combo, id)) out.overrides[id] = combo;
    }
  }
  return compact(out);
}

/** Text from storage as a state (never throws). */
export function parseSaved(text: string | null): KeymapState {
  if (!text) return { ...DEFAULT_STATE, overrides: {} };
  try {
    return readKeymap(JSON.parse(text));
  } catch {
    return { ...DEFAULT_STATE, overrides: {} };
  }
}

/** Without changes that only repeat the preset. */
function compact(s: KeymapState): KeymapState {
  const base = presetBindings(s.preset);
  const overrides: Record<string, Combo> = {};
  for (const [id, v] of Object.entries(s.overrides)) if (!(id in base) || base[id] !== v) overrides[id] = v;
  return { preset: s.preset, overrides };
}

export interface Resolved {
  /** the key each command has ("" for none); no two commands share one */
  bindings: Record<string, Combo>;
  /** key → command */
  byCombo: Map<Combo, string>;
  /** commands that lost a key because another one had it (only possible from a hand-edited or imported file) */
  dropped: string[];
}

/**
 * The key every command has. Your own choices come first (in the order of the list), then the
 * preset's keys for what you haven't touched; a key already taken is not given twice.
 */
export function resolveKeymap(state: KeymapState, extra: CommandDef[] = []): Resolved {
  const base = presetBindings(state.preset);
  const ids = [...COMMANDS.map((d) => d.id), ...extra.map((d) => d.id)];
  const bindings: Record<string, Combo> = {};
  const byCombo = new Map<Combo, string>();
  const dropped: string[] = [];
  const give = (id: string, combo: Combo, explicit: boolean) => {
    if (!combo) return void (bindings[id] ??= "");
    if (byCombo.has(combo)) {
      bindings[id] ??= "";
      if (explicit) dropped.push(id);
      return;
    }
    byCombo.set(combo, id);
    bindings[id] = combo;
  };
  for (const id of ids) if (Object.hasOwn(state.overrides, id)) give(id, state.overrides[id], true);
  for (const id of ids) if (!Object.hasOwn(state.overrides, id)) give(id, base[id] ?? "", false);
  return { bindings, byCombo, dropped };
}

/**
 * The command a key press runs (null for none). Two keys are not looked up: Space belongs to the 3D view
 * (held, it orbits; tapped, it picks Select, so a tool in use isn't dropped the moment Space goes down),
 * and plain Backspace is always a second Delete.
 */
export function commandForCombo(combo: Combo, byCombo: Map<Combo, string>): string | null {
  if (combo === "Space") return null;
  if (combo === "Backspace") return "edit.delete";
  return byCombo.get(combo) ?? null;
}

/** The command that already has this key (other than `id`), if any. */
export function holderOf(res: Resolved, combo: Combo, id: string): string | null {
  const h = res.byCombo.get(combo);
  return h && h !== id ? h : null;
}

/** Give a command a key (or "" for none). `holder` says what to do if another command has it. */
export function assign(state: KeymapState, res: Resolved, id: string, combo: Combo, holder: "swap" | "clear" = "clear"): KeymapState {
  const overrides = { ...state.overrides, [id]: combo };
  const other = combo ? holderOf(res, combo, id) : null;
  // a swap gives the other command this one's key, unless that key is one it may not have (Space belongs to Select)
  const mine = res.bindings[id] ?? "";
  if (other) overrides[other] = holder === "swap" && mine && !reservedReason(mine, other) ? mine : "";
  return compact({ preset: state.preset, overrides });
}

/** Back to the preset's key for one command. */
export function resetOne(state: KeymapState, id: string): KeymapState {
  const overrides = { ...state.overrides };
  delete overrides[id];
  return { preset: state.preset, overrides };
}

/** Switch to a preset, dropping your changes. */
export const withPreset = (id: PresetId): KeymapState => ({ preset: id, overrides: {} });

// ---- export and import of our own file ----------------------------------------------------

export const FILE_KIND = "sketchcraft-shortcuts";

/** Everything as a JSON file: every command's key. */
export function exportKeymap(res: Resolved, preset: PresetId): string {
  const bindings: Record<string, string> = {};
  for (const [id, k] of Object.entries(res.bindings)) bindings[id] = k;
  return JSON.stringify({ kind: FILE_KIND, version: 1, preset, bindings }, null, 2) + "\n";
}

export interface ImportReport {
  state: KeymapState;
  /** commands set */
  applied: number;
  /** what in the file was left out, and why */
  skipped: { name: string; why: string }[];
}

/** Our file, read tolerantly: what is good is used, the rest is listed. Returns null if it isn't our file. */
export function importKeymap(text: string): ImportReport | null {
  let raw: unknown;
  try {
    raw = JSON.parse(text.replace(/^﻿/, ""));
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const r = raw as Record<string, unknown>;
  if (r.kind !== FILE_KIND || !r.bindings || typeof r.bindings !== "object" || Array.isArray(r.bindings)) return null;
  const preset: PresetId = typeof r.preset === "string" && presetDef(r.preset) ? (r.preset as PresetId) : "sketchcraft";
  const skipped: ImportReport["skipped"] = [];
  const overrides: Record<string, Combo> = {};
  let applied = 0;
  const seen = new Map<Combo, string>();
  for (const [id, v] of Object.entries(r.bindings as Record<string, unknown>).slice(0, 600)) {
    if (!isKnownId(id)) {
      skipped.push({ name: id.slice(0, 60), why: "not a command in this version" });
      continue;
    }
    if (v === "") {
      overrides[id] = "";
      applied++;
      continue;
    }
    const combo = normalizeCombo(v);
    if (!combo) {
      skipped.push({ name: id, why: `“${String(v).slice(0, 20)}” isn't a key` });
      continue;
    }
    const why = reservedReason(combo, id);
    if (why) {
      skipped.push({ name: id, why });
      continue;
    }
    const other = seen.get(combo);
    if (other) {
      skipped.push({ name: id, why: `${formatCombo(combo)} is already ${commandDef(other)?.label ?? other}` });
      continue;
    }
    seen.set(combo, id);
    overrides[id] = combo;
    applied++;
  }
  return { state: compact({ preset, overrides }), applied, skipped };
}
