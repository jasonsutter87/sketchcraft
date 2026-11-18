import { describe, expect, it } from "vitest";
import { COMMANDS, DEFAULT_STATE, PRESETS, assign, comboFromEvent, commandForCombo, exportKeymap, formatCombo, holderOf, importKeymap, isTypingTarget, normalizeCombo, parseSaved, presetBindings, readKeymap, reservedReason, resetOne, resolveKeymap, withPreset } from "../src/keymap";
import { TOOL_INFO } from "../src/workspace";

const ev = (o: Partial<KeyboardEvent> & { key: string }) => ({ code: "", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...o });

describe("normalizeCombo", () => {
  it("reads many spellings as one", () => {
    expect(normalizeCombo("l")).toBe("L");
    expect(normalizeCombo("ctrl+shift+z")).toBe("Mod+Shift+Z");
    expect(normalizeCombo("Cmd-Z")).toBe("Mod+Z");
    expect(normalizeCombo("Shift + Z")).toBe("Shift+Z");
    expect(normalizeCombo("⇧⌘Z")).toBe("Mod+Shift+Z");
    expect(normalizeCombo("Spacebar")).toBe("Space");
    expect(normalizeCombo("Alt+Shift+Ctrl+K")).toBe("Mod+Alt+Shift+K");
    expect(normalizeCombo("f5")).toBe("F5");
    expect(normalizeCombo("Del")).toBe("Delete");
    expect(normalizeCombo("Ctrl+-")).toBe("Mod+-");
  });
  it("drops Shift from a typed symbol", () => {
    expect(normalizeCombo("Shift+?")).toBe("?");
  });
  it("refuses what isn't a key", () => {
    for (const bad of ["", "  ", "Ctrl+", "Banana", "Ctrl+Banana", "A+B", 5, null, undefined, {}, "x".repeat(80)]) expect(normalizeCombo(bad)).toBeNull();
  });
});

describe("comboFromEvent", () => {
  it("names a plain key", () => expect(comboFromEvent(ev({ key: "l" }))).toBe("L"));
  it("ignores caps lock and the case of the letter", () => expect(comboFromEvent(ev({ key: "L" }))).toBe("L"));
  it("adds modifiers in a fixed order", () => expect(comboFromEvent(ev({ key: "Z", metaKey: true, shiftKey: true }))).toBe("Mod+Shift+Z"));
  it("takes Ctrl as Mod too", () => expect(comboFromEvent(ev({ key: "z", ctrlKey: true }))).toBe("Mod+Z"));
  it("uses the physical key when Option changes the letter", () => expect(comboFromEvent(ev({ key: "¬", code: "KeyL", altKey: true }))).toBe("Alt+L"));
  it("reads Shift+digit as the digit", () => expect(comboFromEvent(ev({ key: "!", code: "Digit1", shiftKey: true }))).toBe("Shift+1"));
  it("keeps ASCII symbols on a French keyboard, where the comma key is the physical M key", () => {
    expect(comboFromEvent(ev({ key: ",", code: "KeyM" }))).toBe(",");
    expect(comboFromEvent(ev({ key: "?", code: "KeyM", shiftKey: true }))).toBe("?");
    expect(comboFromEvent(ev({ key: "m", code: "Semicolon" }))).toBe("M");
  });
  it("leaves Shift out of a typed symbol", () => expect(comboFromEvent(ev({ key: "?", code: "Slash", shiftKey: true }))).toBe("?"));
  it("names Space and the arrows", () => {
    expect(comboFromEvent(ev({ key: " ", code: "Space" }))).toBe("Space");
    expect(comboFromEvent(ev({ key: "ArrowUp" }))).toBe("ArrowUp");
  });
  it("gives nothing for a bare modifier", () => {
    for (const key of ["Shift", "Control", "Alt", "Meta", "Dead"]) expect(comboFromEvent(ev({ key }))).toBeNull();
  });
});

describe("formatCombo", () => {
  it("reads like a Mac", () => expect(formatCombo("Mod+Shift+Z", true)).toBe("⇧⌘Z"));
  it("reads like Windows", () => expect(formatCombo("Mod+Shift+Z", false)).toBe("Ctrl+Shift+Z"));
  it("is empty for no key", () => expect(formatCombo("")).toBe(""));
  it("uses arrows", () => expect(formatCombo("ArrowUp", false)).toBe("↑"));
});

describe("reserved keys", () => {
  it.each(["Escape", "Enter", "Tab", "Shift+Tab", "Backspace", "ArrowUp", "ArrowLeft", "5", ".", "-", "/", "Mod+Y", "Mod+[", "Mod+Q", "F5"])("%s can't be taken", (k) => {
    expect(reservedReason(k, "tool:line")).toBeTruthy();
  });
  it.each(["L", "Shift+Z", "Mod+1", "Mod+Alt+ArrowUp", "F2", "Shift+5", "?", "Delete", "Mod+Shift+Q"])("%s can be", (k) => {
    expect(reservedReason(k, "tool:line")).toBeNull();
  });
  it("keeps Space for Select", () => {
    expect(reservedReason("Space", "tool:select")).toBeNull();
    expect(reservedReason("Space", "tool:line")).toBeTruthy();
  });
});

describe("the command list", () => {
  it("has a command for every tool", () => {
    for (const id of Object.keys(TOOL_INFO)) expect(COMMANDS.some((d) => d.id === `tool:${id}`)).toBe(true);
  });
  it("keeps today's tool keys as the defaults", () => {
    const b = presetBindings("sketchcraft");
    for (const [id, t] of Object.entries(TOOL_INFO)) expect(b[`tool:${id}`]).toBe(t.key ? normalizeCombo(t.key) : "");
  });
  it("has unique ids and no key given twice, in any preset", () => {
    expect(new Set(COMMANDS.map((d) => d.id)).size).toBe(COMMANDS.length);
    for (const p of PRESETS) {
      const keys = Object.values(presetBindings(p.id)).filter(Boolean);
      expect(new Set(keys).size).toBe(keys.length);
      for (const [id, k] of Object.entries(presetBindings(p.id))) if (k) expect(reservedReason(k, id), `${p.id} ${id} ${k}`).toBeNull();
    }
  });
  it("gives the SketchCraft preset SketchUp's old keys", () => {
    const b = presetBindings("sketchcraft");
    expect(b["edit.undo"]).toBe("Mod+Z");
    expect(b["view.fit"]).toBe("Shift+Z");
    expect(b["tool:select"]).toBe("Space");
  });
});

describe("presets", () => {
  const su = presetBindings("sketchup");
  it("SketchUp's keys", () => {
    const want: Record<string, string> = { select: "Space", line: "L", rect: "R", push: "P", move: "M", rotate: "Q", scale: "S", erase: "E", paint: "B", tape: "T", orbit: "O", pan: "H", zoom: "Z", offset: "F", arc: "A", circle: "C" };
    for (const [t, k] of Object.entries(want)) expect(su[`tool:${t}`], t).toBe(k);
    expect(su["view.fit"]).toBe("Shift+Z");
    expect(su["edit.makecomponent"]).toBe("G");
    expect(su["view.backedges"]).toBe("K");
    expect(su["help.search"]).toBe("Shift+S");
  });
  it("leaves the tools SketchUp has no key for without one", () => {
    for (const t of ["polygon", "followme", "dimension", "text", "section"]) expect(su[`tool:${t}`]).toBe("");
    expect(presetBindings("sketchcraft")["tool:polygon"]).toBe("N");
  });
  it("applies cleanly: no changes of your own, no key lost", () => {
    const r = resolveKeymap(withPreset("sketchup"));
    expect(r.bindings).toEqual(su);
    expect(r.dropped).toEqual([]);
  });
});

describe("readKeymap (what is saved)", () => {
  it("gives the defaults for nothing, junk or the wrong shape", () => {
    for (const bad of [null, undefined, 5, "x", [], [1, 2], true, { preset: 7 }, { overrides: 5 }, { overrides: [] }]) expect(readKeymap(bad)).toEqual(DEFAULT_STATE);
    expect(parseSaved(null)).toEqual(DEFAULT_STATE);
    expect(parseSaved("")).toEqual(DEFAULT_STATE);
    expect(parseSaved("{not json")).toEqual(DEFAULT_STATE);
    expect(parseSaved("null")).toEqual(DEFAULT_STATE);
  });
  it("keeps what is good and drops each bad binding on its own", () => {
    const s = readKeymap({
      preset: "sketchup",
      overrides: {
        "tool:line": "j", // fine
        "tool:rect": 12, // wrong kind
        "tool:circle": "Escape", // reserved
        "tool:move": "Ctrl+Banana", // not a key
        "nope.such": "K", // no such command
        "tool:push": "", // taken away
        "plugin:acme:1": "Shift+F2", // a plugin command
      },
    });
    expect(s.preset).toBe("sketchup");
    expect(s.overrides).toEqual({ "tool:line": "J", "tool:push": "", "plugin:acme:1": "Shift+F2" });
  });
  it("drops an unknown preset but keeps the rest", () => {
    expect(readKeymap({ preset: "emacs", overrides: { "tool:line": "J" } })).toEqual({ preset: "sketchcraft", overrides: { "tool:line": "J" } });
  });
  it("ignores changes that only repeat the preset", () => {
    expect(readKeymap({ preset: "sketchcraft", overrides: { "tool:line": "L" } })).toEqual(DEFAULT_STATE);
  });
  it("survives a huge or hostile value", () => {
    const o: Record<string, string> = {};
    for (let i = 0; i < 5000; i++) o[`tool:x${i}`] = "K";
    expect(() => readKeymap({ overrides: o })).not.toThrow();
    expect(readKeymap(JSON.parse('{"__proto__":{"a":1},"overrides":{"__proto__":"K","constructor":"J"}}'))).toEqual(DEFAULT_STATE);
  });
});

describe("resolveKeymap and conflicts", () => {
  it("gives defaults when nothing is changed", () => {
    const r = resolveKeymap(DEFAULT_STATE);
    expect(r.bindings["tool:line"]).toBe("L");
    expect(r.byCombo.get("Mod+Z")).toBe("edit.undo");
    expect(r.dropped).toEqual([]);
  });
  it("never gives one key to two commands (your choice beats a default)", () => {
    // a hand-edited file puts Select on L, which Line has by default
    const r = resolveKeymap({ preset: "sketchcraft", overrides: { "tool:select": "L" } });
    expect(r.bindings["tool:select"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("");
    const keys = Object.values(r.bindings).filter(Boolean);
    expect(new Set(keys).size).toBe(keys.length);
  });
  it("when two of your own choices clash, the earlier command in the list keeps it", () => {
    const r = resolveKeymap({ preset: "sketchcraft", overrides: { "tool:rect": "J", "tool:circle": "J" } });
    expect(r.bindings["tool:rect"]).toBe("J");
    expect(r.bindings["tool:circle"]).toBe("");
    expect(r.dropped).toEqual(["tool:circle"]);
  });
  it("finds who has a key", () => {
    const r = resolveKeymap(DEFAULT_STATE);
    expect(holderOf(r, "L", "tool:rect")).toBe("tool:line");
    expect(holderOf(r, "L", "tool:line")).toBeNull();
    expect(holderOf(r, "Y", "tool:line")).toBeNull();
  });
  it("includes plugin commands", () => {
    const extra = [{ id: "plugin:acme:1", label: "Do it", group: "Plugins" as const, key: "" }];
    const s = assign(DEFAULT_STATE, resolveKeymap(DEFAULT_STATE, extra), "plugin:acme:1", "Shift+F2");
    expect(resolveKeymap(s, extra).byCombo.get("Shift+F2")).toBe("plugin:acme:1");
  });
});

describe("assign and reset", () => {
  const r0 = resolveKeymap(DEFAULT_STATE);
  it("sets a free key", () => {
    const s = assign(DEFAULT_STATE, r0, "tool:line", "Y");
    expect(resolveKeymap(s).bindings["tool:line"]).toBe("Y");
  });
  it("clearing the other command's key takes it", () => {
    const s = assign(DEFAULT_STATE, r0, "tool:rect", "L", "clear");
    const r = resolveKeymap(s);
    expect(r.bindings["tool:rect"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("");
  });
  it("a swap trades the two keys", () => {
    const s = assign(DEFAULT_STATE, r0, "tool:rect", "L", "swap");
    const r = resolveKeymap(s);
    expect(r.bindings["tool:rect"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("R");
  });
  it("a swap never hands Space to a command that may not have it", () => {
    // Select takes L from Line; Line would get Space, which only Select may have: it is left without one
    const s = assign(DEFAULT_STATE, r0, "tool:select", "L", "swap");
    const r = resolveKeymap(s);
    expect(r.bindings["tool:select"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("");
    expect(readKeymap(JSON.parse(JSON.stringify(s)))).toEqual(s);
  });
  it("a swap with a command that has no key leaves the other without one", () => {
    const s = assign(DEFAULT_STATE, r0, "tool:walk", "L", "swap");
    const r = resolveKeymap(s);
    expect(r.bindings["tool:walk"]).toBe("L");
    expect(r.bindings["tool:line"]).toBe("");
  });
  it("no key at all", () => {
    expect(resolveKeymap(assign(DEFAULT_STATE, r0, "tool:line", "")).bindings["tool:line"]).toBe("");
  });
  it("resets one command", () => {
    const s = assign(DEFAULT_STATE, r0, "tool:line", "Y");
    expect(resolveKeymap(resetOne(s, "tool:line")).bindings["tool:line"]).toBe("L");
  });
  it("giving the preset's own key back leaves nothing saved", () => {
    const s = assign(assign(DEFAULT_STATE, r0, "tool:line", "Y"), resolveKeymap(assign(DEFAULT_STATE, r0, "tool:line", "Y")), "tool:line", "L");
    expect(s).toEqual(DEFAULT_STATE);
  });
  it("Reset all is the preset again", () => {
    expect(withPreset("sketchup")).toEqual({ preset: "sketchup", overrides: {} });
  });
});

describe("export and import of our own file", () => {
  it("round-trips every binding", () => {
    let s = withPreset("sketchup");
    s = assign(s, resolveKeymap(s), "tool:line", "Y");
    s = assign(s, resolveKeymap(s), "tool:rect", "Shift+R");
    s = assign(s, resolveKeymap(s), "tool:erase", "");
    const before = resolveKeymap(s);
    const file = exportKeymap(before, s.preset);
    const got = importKeymap(file)!;
    expect(got.skipped).toEqual([]);
    expect(got.state).toEqual(s);
    expect(resolveKeymap(got.state).bindings).toEqual(before.bindings);
  });
  it("round-trips the defaults to no changes", () => {
    const got = importKeymap(exportKeymap(resolveKeymap(DEFAULT_STATE), "sketchcraft"))!;
    expect(got.state).toEqual(DEFAULT_STATE);
  });
  it("says it isn't ours for other JSON or text", () => {
    expect(importKeymap("hello")).toBeNull();
    expect(importKeymap("[]")).toBeNull();
    expect(importKeymap('{"bindings":{"tool:line":"J"}}')).toBeNull();
    expect(importKeymap('{"kind":"sketchcraft-shortcuts","bindings":[]}')).toBeNull();
  });
  it("takes what is good and lists the rest", () => {
    const file = JSON.stringify({
      kind: "sketchcraft-shortcuts",
      version: 1,
      preset: "sketchcraft",
      bindings: { "tool:line": "J", "tool:rect": "Escape", "tool:circle": 4, "ghost.cmd": "K", "tool:move": "J", "tool:push": "" },
    });
    const got = importKeymap(file)!;
    expect(got.applied).toBe(2);
    expect(got.state.overrides).toEqual({ "tool:line": "J", "tool:push": "" });
    expect(got.skipped.map((s) => s.name).sort()).toEqual(["ghost.cmd", "tool:circle", "tool:move", "tool:rect"]);
  });
});

describe("commandForCombo", () => {
  const { byCombo } = resolveKeymap(DEFAULT_STATE);
  it("looks a key up", () => {
    expect(commandForCombo("L", byCombo)).toBe("tool:line");
    expect(commandForCombo("Mod+Z", byCombo)).toBe("edit.undo");
    expect(commandForCombo("Delete", byCombo)).toBe("edit.delete");
    expect(commandForCombo("Q+Q", byCombo)).toBeNull();
  });
  it("leaves Space to the 3D view, so holding it doesn't drop the tool in use", () => {
    expect(byCombo.get("Space")).toBe("tool:select");
    expect(commandForCombo("Space", byCombo)).toBeNull();
  });
  it("always takes plain Backspace as Delete, even if Delete is rebound", () => {
    expect(commandForCombo("Backspace", byCombo)).toBe("edit.delete");
    const moved = resolveKeymap(assign(DEFAULT_STATE, resolveKeymap(DEFAULT_STATE), "edit.delete", "Shift+Delete"));
    expect(commandForCombo("Backspace", moved.byCombo)).toBe("edit.delete");
    expect(commandForCombo("Shift+Delete", moved.byCombo)).toBe("edit.delete");
  });
});

describe("isTypingTarget", () => {
  it("is true in a field", () => {
    expect(isTypingTarget({ tagName: "INPUT" } as unknown as EventTarget)).toBe(true);
    expect(isTypingTarget({ tagName: "textarea" } as unknown as EventTarget)).toBe(true);
    expect(isTypingTarget({ tagName: "SELECT" } as unknown as EventTarget)).toBe(true);
    expect(isTypingTarget({ tagName: "DIV", isContentEditable: true } as unknown as EventTarget)).toBe(true);
    expect(isTypingTarget({ tagName: "DIV", getAttribute: (n: string) => (n === "role" ? "textbox" : null) } as unknown as EventTarget)).toBe(true);
  });
  it("is false elsewhere", () => {
    expect(isTypingTarget(null)).toBe(false);
    expect(isTypingTarget({ tagName: "BUTTON" } as unknown as EventTarget)).toBe(false);
    expect(isTypingTarget({} as unknown as EventTarget)).toBe(false);
  });
});
