// Your shortcuts, live: read from storage when the app starts (a bad saved value falls back to the
// defaults one key at a time), kept in memory, and saved when they change. Anything that shows a key
// (the tool bar, the ? sheet, the Learn hints) reads it from here, so a change shows everywhere at once.

import { useSyncExternalStore } from "react";
import { store } from "./skins";
import { DEFAULT_STATE, commandDef, formatCombo, parseSaved, resolveKeymap, type CommandDef, type KeymapState, type Resolved } from "./keymap";

export const KEYMAP_KEY = "sketchcraft.shortcuts";

let state: KeymapState = parseSaved(store.get(KEYMAP_KEY));
let extra: CommandDef[] = [];
let resolved: Resolved = resolveKeymap(state, extra);
let version = 0;
const listeners = new Set<() => void>();

const publish = () => {
  resolved = resolveKeymap(state, extra);
  version++;
  for (const l of listeners) l();
};

export const keymap = {
  state: () => state,
  resolved: () => resolved,
  extra: () => extra,
  /** change your shortcuts (saved straight away) */
  set(next: KeymapState) {
    state = next;
    store.set(KEYMAP_KEY, JSON.stringify(state));
    publish();
  },
  /** the commands plugins add right now (they come and go as plugins are turned on and off) */
  setExtra(list: CommandDef[]) {
    if (list.length === extra.length && list.every((d, i) => d.id === extra[i].id && d.label === extra[i].label)) return;
    extra = list;
    publish();
  },
  subscribe(l: () => void) {
    listeners.add(l);
    return () => void listeners.delete(l);
  },
  version: () => version,
};

/** Re-render when shortcuts change; returns what each command has now. */
export function useKeymap(): Resolved {
  useSyncExternalStore(keymap.subscribe, keymap.version);
  return resolved;
}

/** The key a command has right now, as the person reads it ("" if none). */
export const keyText = (id: string): string => formatCombo(resolved.bindings[id] ?? "");

/** Reset the keymap to its defaults (used by Reset all). */
export const resetKeymap = () => keymap.set({ ...DEFAULT_STATE, overrides: {} });

/** A tool's name with its key: "Rectangle (R)". */
export const toolTitle = (name: string, id: string) => {
  const k = keyText(`tool:${id}`);
  return k ? `${name} (${k})` : name;
};

/**
 * Lesson text with the person's own keys: "Press {key:rect}" reads "Press R", or, if they have taken
 * the key away, "Pick Rectangle on the tool bar". Any other {key:id} gives the key, else the name.
 */
export function withKeys(text: string): string {
  const name = (id: string) => commandDef(`tool:${id}`)?.label ?? id;
  return text
    .replace(/Press \{key:(\w+)\}/g, (_, id: string) => keyText(`tool:${id}`) ? `Press ${keyText(`tool:${id}`)}` : `Pick ${name(id)} on the tool bar`)
    .replace(/\{key:(\w+)\}/g, (_, id: string) => keyText(`tool:${id}`) || name(id))
    .replace(/\{cmd:([\w.:]+)\}/g, (_, id: string) => keyText(id) || commandDef(id)?.label || id);
}
