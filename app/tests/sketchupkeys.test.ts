import { describe, expect, it } from "vitest";
import { importSketchUp, mapSketchUpName, readSketchUpShortcuts } from "../src/sketchupkeys";
import { resolveKeymap } from "../src/keymap";

// Hand-written samples in the shapes sketchupkeys.ts documents (SketchUp does not publish its
// format; these follow the lines quoted on SketchUp's forums and a shared Shortcuts.plist).

/** SketchUp's settings file: one "Shortcut_N" line per key (flags, key, action) */
const SETTINGS = `{
  "Settings": {
    "Mac Only": {
      "Num_Shortcuts": 9,
      "Shortcut_1": "0 0 0   selectSelectionTool:",
      "Shortcut_2": "0 0 0 L selectLineTool:",
      "Shortcut_3": "0 0 0 R selectRectangleTool:",
      "Shortcut_4": "0 0 0 X selectPushPullTool:",
      "Shortcut_5": "0 0 0 Z selectZoomTool:",
      "Shortcut_6": "0 0 0 W renderWireframe:",
      "Shortcut_7": "0 0 5 Q selectRotateTool:",
      "Shortcut_8": "0 0 0 k toggleDisplayBackEdges:"
    }
  }
}`;

/** SketchUp for Mac's Shortcuts.plist: an array of (key, modifier flags, action) */
const PLIST = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
  <string> </string><integer>0</integer><string>selectSelectionTool:</string>
  <string>l</string><integer>0</integer><string>selectLineTool:</string>
  <string>q</string><integer>0</integer><string>selectRotateTool:</string>
  <string>Z</string><integer>131072</integer><string>viewZoomExtents:</string>
  <string>V</string><integer>393216</integer><string>pasteInPlace:</string>
  <string>g</string><integer>262144</integer><string>makeGroup:</string>
  <string>d</string><integer>0</integer><string>doSomethingUnknown:</string>
  <string>b</string><integer>12345</integer><string>selectPaintTool:</string>
</array>
</plist>`;

/** A Windows export read as text: menu path, then key */
const NAMES = `SketchUp shortcuts
Tools/Select = Space
Tools/Push/Pull = P
Draw/Line	L
"Draw/Rectangle": "R"
Edit/Undo = Ctrl+Z
Edit/Redo=Ctrl+Shift+Z
Camera/Zoom Extents = Shift+Z
Camera/Standard Views/Top = Ctrl+2
Tools/Dimensions = Banana
Tools/Section Plane =
Plugins/Some Extension/Do It = Alt+F9
Tools/Tape Measure = T
Draw/Arc = A
`;

describe("mapSketchUpName", () => {
  it("knows menu paths, last parts and Mac action names", () => {
    expect(mapSketchUpName("Tools/Push/Pull")).toBe("tool:push");
    expect(mapSketchUpName("Tools/Select")).toBe("tool:select");
    expect(mapSketchUpName("select")).toBe("tool:select");
    expect(mapSketchUpName("Camera/Zoom Extents")).toBe("view.fit");
    expect(mapSketchUpName("selectDollyTool:")).toBe("tool:pan");
    expect(mapSketchUpName("Camera/Standard Views/Top")).toBe("view.top");
    expect(mapSketchUpName("EDIT / UNDO")).toBe("edit.undo");
  });
  it("is null for what we don't have", () => {
    expect(mapSketchUpName("Window/Model Info")).toBeNull();
    expect(mapSketchUpName("makeGroup:")).toBeNull();
    expect(mapSketchUpName("")).toBeNull();
  });
});

describe("the settings file", () => {
  const read = readSketchUpShortcuts(SETTINGS);
  it("is recognized", () => expect(read.format).toBe("settings"));
  it("reads each Shortcut line", () => {
    const by = Object.fromEntries(read.entries.map((e) => [e.name, e.combo]));
    expect(by["selectSelectionTool:"]).toBe("Space");
    expect(by["selectLineTool:"]).toBe("L");
    expect(by["selectRectangleTool:"]).toBe("R");
    // the letter's case means nothing; Shift would be in the flags
    expect(by["selectPushPullTool:"]).toBe("X");
    expect(by["selectZoomTool:"]).toBe("Z");
    expect(by["toggleDisplayBackEdges:"]).toBe("K");
  });
  it("lists flags it doesn't know instead of guessing", () => {
    expect(read.unreadable.map((u) => u.name)).toEqual(["selectRotateTool:"]);
  });
  it("imports onto the SketchUp preset", () => {
    const imp = importSketchUp(read);
    const r = resolveKeymap(imp.state);
    expect(imp.state.preset).toBe("sketchup");
    expect(r.bindings["tool:push"]).toBe("X");
    expect(r.bindings["tool:line"]).toBe("L");
    // not in the file: SketchUp's own key stays
    expect(r.bindings["tool:move"]).toBe("M");
    expect(imp.unmapped.map((u) => u.name)).toEqual(["renderWireframe:"]);
    expect(imp.skipped.some((s) => s.name === "selectRotateTool:")).toBe(true);
  });
});

describe("the property list", () => {
  const read = readSketchUpShortcuts(PLIST);
  it("is recognized, with Cocoa's modifier flags", () => {
    expect(read.format).toBe("plist");
    const by = Object.fromEntries(read.entries.map((e) => [e.name, e.combo]));
    expect(by["selectSelectionTool:"]).toBe("Space");
    expect(by["selectLineTool:"]).toBe("L");
    expect(by["viewZoomExtents:"]).toBe("Shift+Z");
    expect(by["pasteInPlace:"]).toBe("Mod+Shift+V");
    expect(by["makeGroup:"]).toBe("Mod+G");
  });
  it("lists unknown flags", () => {
    expect(read.unreadable).toEqual([{ name: "selectPaintTool:", why: expect.stringContaining("12345") }]);
  });
  it("imports what maps and reports what doesn't", () => {
    const imp = importSketchUp(read);
    const r = resolveKeymap(imp.state);
    expect(r.bindings["edit.pasteinplace"]).toBe("Mod+Shift+V");
    expect(r.bindings["view.fit"]).toBe("Shift+Z");
    expect(imp.unmapped.map((u) => u.name).sort()).toEqual(["doSomethingUnknown:", "makeGroup:"]);
  });
  it("reads key and string pairs too", () => {
    const dict = `<plist><dict><key>Tools/Push/Pull</key><string>X</string><key>Edit/Undo</key><string>Ctrl+Z</string></dict></plist>`;
    expect(readSketchUpShortcuts(dict).entries).toEqual([
      { name: "Tools/Push/Pull", combo: "X" },
      { name: "Edit/Undo", combo: "Mod+Z" },
    ]);
  });
});

describe("a list of command names and keys", () => {
  const read = readSketchUpShortcuts(NAMES);
  it("is recognized", () => expect(read.format).toBe("names"));
  it("reads =, tab and JSON styles, and skips empty keys and the title line", () => {
    const by = Object.fromEntries(read.entries.map((e) => [e.name, e.combo]));
    expect(by["Tools/Select"]).toBe("Space");
    expect(by["Tools/Push/Pull"]).toBe("P");
    expect(by["Draw/Line"]).toBe("L");
    expect(by["Draw/Rectangle"]).toBe("R");
    expect(by["Edit/Undo"]).toBe("Mod+Z");
    expect(by["Edit/Redo"]).toBe("Mod+Shift+Z");
    expect(by["Camera/Standard Views/Top"]).toBe("Mod+2");
    expect(by["Tools/Section Plane"]).toBeUndefined();
    expect(read.unreadable.map((u) => u.name)).toEqual(["Tools/Dimensions"]);
  });
  it("maps names to ours and reports the rest", () => {
    const imp = importSketchUp(read);
    expect(imp.unmapped).toEqual([{ name: "Plugins/Some Extension/Do It", combo: "Alt+F9" }]);
    const r = resolveKeymap(imp.state);
    expect(r.bindings["view.top"]).toBe("Mod+2");
    expect(r.bindings["tool:select"]).toBe("Space");
    expect(imp.applied.length).toBeGreaterThan(8);
  });
});

describe("tolerance", () => {
  it("reads UTF-16 text with its NULs and a byte-order mark", () => {
    const utf16 = "﻿" + [...NAMES].map((c) => c + "\u0000").join("");
    expect(readSketchUpShortcuts(utf16).entries.length).toBe(readSketchUpShortcuts(NAMES).entries.length);
  });
  it("reads Windows line endings", () => {
    expect(readSketchUpShortcuts(NAMES.replace(/\n/g, "\r\n")).entries.length).toBe(readSketchUpShortcuts(NAMES).entries.length);
  });
  it("says 'none' for text that isn't a shortcuts file, without throwing", () => {
    for (const junk of ["", "   ", "hello world", "{{{{", "<html><body>nope</body></html>", "\u0000\u0001\u0002", "a".repeat(100000), JSON.stringify({ a: [1, 2, { b: 3 }] }), "<plist><array></array></plist>"]) {
      const r = readSketchUpShortcuts(junk);
      expect(r.entries.length, junk.slice(0, 20)).toBe(0);
    }
    expect(readSketchUpShortcuts("hello world").format).toBe("none");
  });
  it("lets the first line for a command win and keeps a key from being given twice", () => {
    const imp = importSketchUp(readSketchUpShortcuts("Tools/Line = J\nDraw/Line = K\nTools/Rectangle = J\n"));
    const r = resolveKeymap(imp.state);
    expect(r.bindings["tool:line"]).toBe("J");
    expect(r.bindings["tool:rect"]).toBe("R");
    expect(imp.skipped.some((s) => /already/.test(s.why))).toBe(true);
  });
  it("won't take a reserved key", () => {
    const imp = importSketchUp(readSketchUpShortcuts("Tools/Line = Escape\nTools/Move = Enter\n"));
    expect(imp.applied).toEqual([]);
    expect(imp.skipped.length).toBe(2);
    expect(resolveKeymap(imp.state).bindings["tool:line"]).toBe("L");
  });
  it("clears a default key the file gives to something else", () => {
    // SketchUp preset has L on Line; the file puts it on Move
    const r = resolveKeymap(importSketchUp(readSketchUpShortcuts("Tools/Move = L\n")).state);
    expect(r.bindings["tool:move"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("");
  });
});
