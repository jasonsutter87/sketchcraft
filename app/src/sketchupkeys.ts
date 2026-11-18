// Reading SketchUp's exported shortcuts.
//
// SketchUp does not publish the format of its shortcuts file, so this reads every shape that
// is documented or quoted in public (SketchUp's help, its forums and a shared Shortcuts.plist),
// and reports what it could not understand instead of guessing:
//
//   1. SketchUp's own settings file (SharedPreferences.json, a "Mac Only" section), one line per key:
//        "Shortcut_3": "0 0 0 L selectLineTool:",
//      three flag numbers, the key, then the action's name. (What the flags mean is not
//      published; all zero means no modifier. The letter is spelled in capitals, whatever the key.)
//   2. SketchUp for Mac's Shortcuts.plist: an XML property list holding an array of
//      (key, modifier flags, action) triples. The flags are Cocoa's: 131072 Shift,
//      262144 Control, 524288 Option, 1048576 Command.
//   3. Windows' export (a .dat: SketchUp's help says it can be renamed .txt and read in a text
//      editor), which names each command by its menu path ("Tools/Push/Pull") beside its key
//      ("P", "Shift+Z", "Ctrl+Z"). Read as lines of `name = key`, `name : key`, `"name": "key"`
//      or `name<TAB>key`; JSON objects and registry-style lines fit too. UTF-16 text is accepted.
//
// Names are matched without case, spaces or punctuation, on the whole menu path or on its last
// part ("Tools/Select" and "Select" are the same).

import { assign, commandDef, formatCombo, normalizeCombo, reservedReason, resolveKeymap, type Combo, type KeymapState } from "./keymap";

/** our command → the names SketchUp calls it (menu paths, as on Windows, and the Mac's action names) */
const NAMES: Record<string, string[]> = {
  "tool:select": ["Select", "Selection", "selectSelectionTool:"],
  "tool:erase": ["Eraser", "Erase", "selectEraseTool:"],
  "tool:paint": ["Paint Bucket", "Paint", "selectPaintTool:"],
  "tool:line": ["Line", "selectLineTool:"],
  "tool:freehand": ["Freehand", "selectFreehandTool:"],
  "tool:rect": ["Rectangle", "selectRectangleTool:"],
  "tool:rrect": ["Rotated Rectangle", "selectRotatedRectangleTool:"],
  "tool:circle": ["Circle", "selectCircleTool:"],
  "tool:polygon": ["Polygon", "selectPolygonTool:"],
  "tool:arc": ["Arc", "2 Point Arc", "Two Point Arc", "selectArcTool:", "select2PointArcTool:"],
  "tool:arccenter": ["Arc by center"],
  "tool:arc3": ["3 Point Arc", "Three Point Arc", "select3PointArcTool:"],
  "tool:pie": ["Pie", "selectPieTool:"],
  "tool:move": ["Move", "selectMoveTool:"],
  "tool:push": ["Push/Pull", "PushPull", "selectPushPullTool:"],
  "tool:rotate": ["Rotate", "selectRotateTool:"],
  "tool:followme": ["Follow Me", "selectFollowMeTool:"],
  "tool:scale": ["Scale", "selectScaleTool:"],
  "tool:offset": ["Offset", "selectOffsetTool:"],
  "tool:tape": ["Tape Measure", "Tape Measure Tool", "selectMeasureTool:"],
  "tool:protractor": ["Protractor", "selectProtractorTool:"],
  "tool:axes": ["Axes", "selectAxesTool:", "selectAxisTool:"],
  "tool:dimension": ["Dimensions", "Dimension", "selectDimensionTool:"],
  "tool:text": ["Text", "selectTextTool:"],
  "tool:section": ["Section Plane", "selectSectionPlaneTool:"],
  "tool:orbit": ["Orbit", "selectOrbitTool:"],
  "tool:pan": ["Pan", "selectDollyTool:", "selectPanTool:"],
  "tool:zoom": ["Zoom", "selectZoomTool:"],
  "tool:zoomwindow": ["Zoom Window", "selectZoomWindowTool:"],
  "tool:position": ["Position Camera", "selectPositionCameraTool:"],
  "tool:lookaround": ["Look Around", "selectLookAroundTool:"],
  "tool:walk": ["Walk", "selectWalkTool:"],
  "file.new": ["File/New", "New", "newDocument:"],
  "file.open": ["File/Open", "Open", "openDocument:"],
  "file.save": ["File/Save", "Save", "saveDocument:"],
  "file.saveas": ["Save As", "saveDocumentAs:"],
  "file.import": ["File/Import", "Import", "importFile:"],
  "edit.undo": ["Edit/Undo", "Undo", "undo:"],
  "edit.redo": ["Edit/Redo", "Redo", "redo:"],
  "edit.cut": ["Edit/Cut", "Cut", "cut:"],
  "edit.copy": ["Edit/Copy", "Copy", "copy:"],
  "edit.paste": ["Edit/Paste", "Paste", "paste:"],
  "edit.pasteinplace": ["Paste In Place", "pasteInPlace:"],
  "edit.selectall": ["Edit/Select All", "Select All", "selectAll:"],
  "edit.selectnone": ["Edit/Select None", "Select None", "selectNone:", "editSelectNone:"],
  "edit.delete": ["Edit/Delete", "Delete", "Erase", "delete:", "deleteSelection:"],
  "edit.makecomponent": ["Make Component", "makeComponent:"],
  "edit.unhide": ["Unhide All", "Edit/Unhide/All", "editUnhideAll:"],
  "edit.clearguides": ["Delete Guides", "Clear Guides", "deleteGuides:"],
  "view.fit": ["Zoom Extents", "Camera/Zoom Extents", "viewZoomExtents:"],
  "view.iso": ["Iso", "Camera/Standard Views/Iso", "viewIso:"],
  "view.top": ["Top", "Camera/Standard Views/Top", "viewTop:"],
  "view.front": ["Front", "Camera/Standard Views/Front", "viewFront:"],
  "view.right": ["Right", "Camera/Standard Views/Right", "viewRight:"],
  "view.back": ["Back", "Camera/Standard Views/Back", "viewBack:"],
  "view.left": ["Left", "Camera/Standard Views/Left", "viewLeft:"],
  "view.backedges": ["Back Edges", "View/Face Style/Back Edges", "toggleDisplayBackEdges:"],
  "view.xray": ["X-Ray", "View/Face Style/X-Ray", "viewXRay:"],
  "view.shadows": ["Shadows", "View/Shadows", "toggleShadows:"],
  "view.edges": ["Edges", "View/Edge Style/Edges", "toggleDisplayEdges:"],
  "view.parallel": ["Parallel Projection", "Camera/Parallel Projection", "viewParallelProjection:"],
  "help.search": ["Search"],
};

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

const TABLE = new Map<string, string>();
for (const [id, names] of Object.entries(NAMES)) for (const n of names) if (!TABLE.has(norm(n))) TABLE.set(norm(n), id);

/** Our command for one of SketchUp's names, or null. */
export function mapSketchUpName(name: string): string | null {
  const direct = TABLE.get(norm(name));
  if (direct) return direct;
  // a menu path: try its last parts ("Tools/Push/Pull": "PushPull", then "Pull")
  const parts = name.split(/[\\/]+/).map((p) => p.trim()).filter(Boolean);
  for (let n = Math.min(3, parts.length - 1); n >= 1; n--) {
    const hit = TABLE.get(norm(parts.slice(-n).join("")));
    if (hit) return hit;
  }
  return null;
}

export interface SketchUpEntry {
  /** SketchUp's name for the command */
  name: string;
  /** the key as read (our spelling) */
  combo: Combo;
}

export interface SketchUpRead {
  /** which shape the file had */
  format: "settings" | "plist" | "names" | "none";
  entries: SketchUpEntry[];
  /** lines whose key could not be read: name and why */
  unreadable: { name: string; why: string }[];
}

const decode = (s: string) => s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, "&");

/** Cocoa's modifier flags (Shift, Control, Option, Command) as our modifiers. */
function cocoaMods(flags: number): string[] | null {
  const known = 131072 | 262144 | 524288 | 1048576;
  if (flags & ~known) return null;
  const m: string[] = [];
  if (flags & (262144 | 1048576)) m.push("Mod");
  if (flags & 524288) m.push("Alt");
  if (flags & 131072) m.push("Shift");
  return m;
}

/** one key character with modifiers as a combo (the letter's case means nothing: Shift comes from the flags) */
function charCombo(ch: string, mods: string[]): Combo | null {
  return normalizeCombo([...["Mod", "Alt", "Shift"].filter((m) => mods.includes(m)), ch === " " ? "Space" : ch].join("+"));
}

/** What a shortcuts file says. Never throws; text that isn't a shortcuts file gives format "none". */
export function readSketchUpShortcuts(input: string): SketchUpRead {
  const out: SketchUpRead = { format: "none", entries: [], unreadable: [] };
  // UTF-16 text (a Windows export) arrives with a NUL between letters
  const text = input.replace(/^﻿/, "").replace(/\u0000/g, "").slice(0, 2_000_000);
  const add = (name: string, combo: Combo | null, raw: string) => {
    if (out.entries.length + out.unreadable.length >= 5000) return; // no real file has this many
    name = name.slice(0, 80);
    if (combo) out.entries.push({ name, combo });
    else out.unreadable.push({ name, why: `“${raw.slice(0, 20)}” isn't a key I know` });
  };

  // 2. a property list
  if (/<plist[\s>]/.test(text)) {
    out.format = "plist";
    const tokens: { t: "key" | "string" | "integer"; v: string }[] = [];
    for (const m of text.matchAll(/<(key|string|integer)>([\s\S]*?)<\/\1>|<string\s*\/>/g)) tokens.push(m[1] ? { t: m[1] as "key" | "string" | "integer", v: decode(m[2]) } : { t: "string", v: "" });
    for (let i = 0; i < tokens.length; ) {
      const [a, b, c] = [tokens[i], tokens[i + 1], tokens[i + 2]];
      if (a?.t === "string" && b?.t === "integer" && c?.t === "string") {
        const mods = cocoaMods(Number(b.v));
        if (a.v.length === 1 && c.v) {
          if (!mods) out.unreadable.push({ name: c.v, why: `modifier flags ${b.v} aren't ones I know` });
          else add(c.v, charCombo(a.v, mods), a.v);
        }
        i += 3;
      } else if (a?.t === "key" && b?.t === "string") {
        if (b.v) add(a.v, normalizeCombo(b.v), b.v);
        i += 2;
      } else i++;
    }
    return out;
  }

  // 1. the settings file: "Shortcut_3": "0 0 0 L selectLineTool:" (other lines there are other preferences)
  const lines = text.split(/\r?\n/).slice(0, 20000).map((line) => line.trim().replace(/,$/, ""));
  for (const l of lines) {
    const cocoa = l.length < 300 ? /^"?Shortcut_\d+"?\s*[:=]\s*"(\d+) (\d+) (\d+) (.) (\S+)"$/.exec(l) : null;
    if (!cocoa) continue;
    out.format = "settings";
    const [, m1, m2, m3, key, action] = cocoa;
    if (key === " " && !/Selection/.test(action)) continue; // no key on this command
    if (m1 !== "0" || m2 !== "0" || m3 !== "0") out.unreadable.push({ name: action, why: `modifier flags ${m1} ${m2} ${m3} aren't documented` });
    else add(action, charCombo(key, []), key);
  }
  if (out.format === "settings") return out;

  // 3. names and keys, a line each
  const seen = new Set<string>();
  for (const l of lines) {
    if (!l || l.length > 300) continue;
    const m = /^"?([^"=:\t]+?)"?\s*(?:=|:|\t+|\s{2,})\s*"?([^"]*?)"?$/.exec(l);
    if (!m) continue;
    const [, name, value] = m;
    if (!/[A-Za-z]/.test(name) || /^(\[|@|<|Windows Registry|REGEDIT)/i.test(name) || /^(kind|version|app|preset|Num_Shortcuts)$/i.test(name)) continue;
    if (!value.trim() || /^[{[]/.test(value.trim())) continue; // no key set, or the start of a block
    const key = `${norm(name)}|${value}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.format = "names";
    add(name.trim(), normalizeCombo(value), value);
  }
  return out;
}

export interface SketchUpImport {
  state: KeymapState;
  /** SketchUp commands now set on ours: name, our command, key */
  applied: { name: string; id: string; combo: Combo }[];
  /** names in the file that have no counterpart here (with the key they had) */
  unmapped: SketchUpEntry[];
  /** lines or entries left out, and why */
  skipped: { name: string; why: string }[];
}

/**
 * Take the keys in a SketchUp shortcuts file: start from the SketchUp preset (so a command the file
 * doesn't mention has SketchUp's key), put the file's keys over it, and say what didn't fit.
 * `parsed.format === "none"` means the text wasn't a shortcuts file.
 */
export function importSketchUp(parsed: SketchUpRead): SketchUpImport {
  let state: KeymapState = { preset: "sketchup", overrides: {} };
  const applied: SketchUpImport["applied"] = [];
  const unmapped: SketchUpEntry[] = [];
  const skipped = [...parsed.unreadable];
  const taken = new Map<Combo, string>();
  const done = new Set<string>();
  for (const e of parsed.entries) {
    const id = mapSketchUpName(e.name);
    if (!id) {
      unmapped.push(e);
      continue;
    }
    if (done.has(id)) continue; // the first line for a command wins
    const why = reservedReason(e.combo, id);
    if (why) {
      skipped.push({ name: e.name, why: `${formatCombo(e.combo)}: ${why}` });
      continue;
    }
    const other = taken.get(e.combo);
    if (other) {
      skipped.push({ name: e.name, why: `${formatCombo(e.combo)} is already ${commandDef(other)?.label ?? other}` });
      continue;
    }
    taken.set(e.combo, id);
    done.add(id);
    state = assign(state, resolveKeymap(state), id, e.combo, "clear");
    applied.push({ name: e.name, id, combo: e.combo });
  }
  return { state, applied, unmapped, skipped };
}
