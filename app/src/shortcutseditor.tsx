// The Shortcuts editor: search a command, click its key and press a new one, see what a key
// is already used for and swap or clear it, reset one or all, choose SketchCraft's or SketchUp's
// keys, and import (SketchUp's exported shortcuts, or our own file) and export.

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { Sheet, useEscapeToClose } from "./sheet";
import { COMMANDS, PRESETS, assign, comboFromEvent, commandDef, formatCombo, holderOf, importKeymap, exportKeymap, presetBindings, reservedReason, resetOne, withPreset, type CommandDef, type CommandGroup, type Combo } from "./keymap";
import { importSketchUp, readSketchUpShortcuts, type SketchUpEntry } from "./sketchupkeys";
import type { KeymapState } from "./keymap";
import { keymap, useKeymap } from "./keymapstore";

const GROUPS: CommandGroup[] = ["Tools", "Camera", "Edit", "File", "View", "Window", "Plugins"];
const MAX_FILE = 2_000_000;

interface Clash {
  id: string;
  combo: Combo;
  holder: string;
}
interface Report {
  title: string;
  /** names in a SketchUp file that have no counterpart here */
  unmapped: SketchUpEntry[];
  skipped: { name: string; why: string }[];
  bad?: boolean;
}

const nameOf = (id: string) => commandDef(id)?.label ?? keymap.extra().find((d) => d.id === id)?.label ?? id;

export function ShortcutEditor({ onClose }: { onClose: () => void }) {
  const res = useKeymap();
  const state = keymap.state();
  const [q, setQ] = useState("");
  /** the command whose key is being picked */
  const [rec, setRec] = useState<string | null>(null);
  const [problem, setProblem] = useState("");
  const [clash, setClash] = useState<Clash | null>(null);
  const [tools, setTools] = useState(false);
  const [text, setText] = useState("");
  const [report, setReport] = useState<Report | null>(null);
  /** your keys from before the last import, so it can be undone */
  const [undo, setUndo] = useState<KeymapState | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const all: CommandDef[] = useMemo(() => [...COMMANDS, ...keymap.extra()], [res]);
  const base = presetBindings(state.preset);
  const defaultOf = (id: string): Combo => base[id] ?? "";

  const set = (id: string, combo: Combo, how: "swap" | "clear" = "clear") => keymap.set(assign(keymap.state(), keymap.resolved(), id, combo, how));
  /** Give a command a key; if another has it, ask what to do */
  const give = (id: string, combo: Combo) => {
    setProblem("");
    const holder = combo ? holderOf(keymap.resolved(), combo, id) : null;
    if (holder) return setClash({ id, combo, holder });
    setClash(null);
    set(id, combo);
  };

  // pressing the new key (in the capture phase, so no tool or command hears it)
  useEscapeToClose(() => (setRec(null), setProblem("")), rec != null);
  useLayoutEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") return;
      // Tab moves on as usual (and ends the pick), so the keyboard is never trapped
      if (e.key === "Tab") return setRec(null);
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Backspace") {
        setRec(null);
        return give(rec, "");
      }
      const combo = comboFromEvent(e);
      if (!combo) return; // only a modifier so far
      const why = reservedReason(combo, rec);
      if (why) return setProblem(why);
      setRec(null);
      give(rec, combo);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rec]);

  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = (d: CommandDef) => {
    const hay = `${d.label} ${d.group} ${formatCombo(res.bindings[d.id] ?? "")} ${res.bindings[d.id] ?? ""}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  };

  const applyText = (raw: string) => {
    if (raw.length > MAX_FILE) return setReport({ title: "That file is too big to be a shortcuts file.", unmapped: [], skipped: [], bad: true });
    setClash(null);
    const ours = importKeymap(raw);
    if (ours) {
      setUndo(keymap.state());
      keymap.set(ours.state);
      return setReport({ title: `Imported ${ours.applied} shortcuts from a SketchCraft file.`, unmapped: [], skipped: ours.skipped });
    }
    const read = readSketchUpShortcuts(raw);
    if (read.format === "none" || (!read.entries.length && !read.unreadable.length)) return setReport({ title: "That doesn't look like a SketchCraft or SketchUp shortcuts file. Nothing was changed.", unmapped: [], skipped: [], bad: true });
    const imp = importSketchUp(read);
    if (!imp.applied.length) return setReport({ title: "Nothing in that file matched a SketchCraft command. Nothing was changed.", unmapped: imp.unmapped, skipped: imp.skipped, bad: true });
    setUndo(keymap.state());
    keymap.set(imp.state);
    setReport({ title: `Imported ${imp.applied.length} of SketchUp's shortcuts (${read.format === "plist" ? "a Mac property list" : read.format === "settings" ? "SketchUp's settings file" : "a list of commands and keys"}).`, unmapped: imp.unmapped, skipped: imp.skipped });
  };
  const readFile = async (f: File | undefined) => {
    if (!f) return;
    if (f.size > MAX_FILE) return setReport({ title: "That file is too big to be a shortcuts file.", unmapped: [], skipped: [], bad: true });
    const buf = new Uint8Array(await f.arrayBuffer());
    // UTF-16 (a Windows export) or plain text
    const utf16 = (buf[0] === 0xff && buf[1] === 0xfe) || (buf[0] === 0xfe && buf[1] === 0xff);
    const t = new TextDecoder(utf16 ? (buf[0] === 0xff ? "utf-16le" : "utf-16be") : "utf-8").decode(buf);
    setText(t.slice(0, 20000));
    applyText(t);
  };
  const doExport = () => setText(exportKeymap(keymap.resolved(), keymap.state().preset));
  const download = () => {
    try {
      const url = URL.createObjectURL(new Blob([text || exportKeymap(keymap.resolved(), keymap.state().preset)], { type: "application/json" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = "sketchcraft-shortcuts.json";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch {
      /* the text box still has it to copy */
    }
  };
  const copy = () => void navigator.clipboard?.writeText(text || exportKeymap(keymap.resolved(), keymap.state().preset)).catch(() => {});

  const changed = Object.keys(state.overrides).length;
  const preset = PRESETS.find((p) => p.id === state.preset)!;

  return (
    <Sheet onClose={onClose} label="Customize shortcuts" className="key-editor" width={760}>
      <div className="sheet-head">
        <b>Shortcuts</b>
        <span className="muted">Click a key, then press the one you want. Esc cancels, Backspace takes it away.</span>
        <button className="btn small ghost" onClick={onClose}>
          Close
        </button>
      </div>
      <div className="key-bar">
        <div className="seg" role="group" aria-label="Preset">
          {PRESETS.map((p) => (
            <button key={p.id} className={state.preset === p.id ? "on" : ""} aria-pressed={state.preset === p.id} title={p.blurb} onClick={() => (setClash(null), setReport(null), keymap.set(withPreset(p.id)))}>
              {p.name}
            </button>
          ))}
        </div>
        <input className="key-search" type="search" placeholder="Search a command or key" aria-label="Search shortcuts" value={q} onChange={(e) => setQ(e.target.value)} />
        <button className="btn small" disabled={!changed} onClick={() => (setClash(null), keymap.set(withPreset(state.preset)))} title={`Back to ${preset.name}'s keys`}>
          Reset all{changed ? ` (${changed})` : ""}
        </button>
        <button className="btn small" aria-expanded={tools} onClick={() => setTools((v) => !v)}>
          Import / export
        </button>
      </div>
      <p className="muted small key-blurb">{preset.blurb} Choosing a preset replaces your changes.</p>

      {tools && (
        <div className="key-tools">
          <p className="small">
            Import a SketchUp shortcuts file (Preferences › Shortcuts › Export; the .dat, or its settings file or Shortcuts.plist) or one of ours. Pick the file, or paste its text.
          </p>
          <textarea aria-label="Shortcuts file text" value={text} onChange={(e) => setText(e.target.value)} rows={6} spellCheck={false} placeholder="Paste a shortcuts file here, or press Export to see yours" />
          <div className="row gap">
            <button className="btn small" onClick={() => fileInput.current?.click()}>
              Choose file…
            </button>
            <input ref={fileInput} type="file" accept=".dat,.txt,.json,.plist,.reg,text/*,application/json" hidden onChange={(e) => (void readFile(e.target.files?.[0]), (e.target.value = ""))} />
            <button className="btn small primary" disabled={!text.trim()} onClick={() => applyText(text)}>
              Import
            </button>
            <span className="grow" />
            <button className="btn small" onClick={doExport}>
              Export
            </button>
            <button className="btn small" onClick={copy}>
              Copy
            </button>
            <button className="btn small" onClick={download}>
              Download .json
            </button>
          </div>
          <div className={`key-report ${report?.bad ? "bad" : ""}`} role="status">
            {report && (
              <>
              <b>{report.title}</b>
              {undo && !report.bad && (
                <button className="btn small" onClick={() => (keymap.set(undo), setUndo(null), setReport({ title: "Import undone: your keys are back as they were.", unmapped: [], skipped: [] }))}>
                  Undo import
                </button>
              )}
              {report.unmapped.length > 0 && (
                <details open>
                  <summary>{report.unmapped.length} SketchUp command{report.unmapped.length === 1 ? " has" : "s have"} no match here</summary>
                  <ul>
                    {report.unmapped.slice(0, 60).map((u, i) => (
                      <li key={i}>
                        {u.name} <kbd>{formatCombo(u.combo)}</kbd>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              {report.skipped.length > 0 && (
                <details open>
                  <summary>{report.skipped.length} left out</summary>
                  <ul>
                    {report.skipped.slice(0, 60).map((u, i) => (
                      <li key={i}>
                        {u.name}: {u.why}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              </>
            )}
          </div>
        </div>
      )}

      {(problem || clash || res.dropped.length > 0) && (
        <div className="key-notice" role="alert">
          {problem && <span>{problem}</span>}
          {clash && (
            <>
              <span>
                <kbd>{formatCombo(clash.combo)}</kbd> is already <b>{nameOf(clash.holder)}</b>.
              </span>
              <button className="btn small" onClick={() => (set(clash.id, clash.combo, "swap"), setClash(null))} title={`${nameOf(clash.holder)} gets ${res.bindings[clash.id] ? formatCombo(res.bindings[clash.id]) : "no key"}`}>
                Swap
              </button>
              <button className="btn small" onClick={() => (set(clash.id, clash.combo, "clear"), setClash(null))}>
                Clear {nameOf(clash.holder)}'s key
              </button>
              <button className="btn small ghost" onClick={() => setClash(null)}>
                Cancel
              </button>
            </>
          )}
          {!problem && !clash && res.dropped.length > 0 && <span>{res.dropped.map(nameOf).join(", ")} lost a key that another command had; set it again below.</span>}
        </div>
      )}

      <div className="key-list">
        {GROUPS.map((g) => {
          const rows = all.filter((d) => d.group === g && shown(d));
          if (!rows.length) return null;
          return (
            <section key={g}>
              <div className="section-title">{g}</div>
              {rows.map((d) => {
                const k = res.bindings[d.id] ?? "";
                const isRec = rec === d.id;
                const differs = k !== defaultOf(d.id);
                return (
                  <div key={d.id} className={`key-row ${clash && (clash.id === d.id || clash.holder === d.id) ? "clash" : ""}`} data-cmd={d.id}>
                    <span className="key-name">{d.label}</span>
                    <button
                      className={`key-btn ${isRec ? "rec" : ""} ${k ? "" : "none"}`}
                      aria-label={isRec ? `${d.label}: press the new key, Esc to cancel, Backspace to clear` : `${d.label} shortcut: ${k ? formatCombo(k) : "none"}. Click to change.`}
                      aria-pressed={isRec}
                      onClick={() => (setClash(null), setProblem(""), setRec(isRec ? null : d.id))}
                    >
                      {isRec ? "Press a key…" : k ? <kbd>{formatCombo(k)}</kbd> : "none"}
                    </button>
                    <button className="btn small ghost" disabled={!differs} onClick={() => (setClash(null), defaultOf(d.id) && holderOf(res, defaultOf(d.id), d.id) ? give(d.id, defaultOf(d.id)) : keymap.set(resetOne(keymap.state(), d.id)))} title={differs ? `Back to ${defaultOf(d.id) ? formatCombo(defaultOf(d.id)) : "no key"}` : "Already the preset's key"} aria-label={`Reset ${d.label}`}>
                      Reset
                    </button>
                    <button className="btn small ghost" disabled={!k} onClick={() => (setClash(null), set(d.id, ""))} aria-label={`Clear ${d.label}'s key`} title="Take the key away">
                      Clear
                    </button>
                  </div>
                );
              })}
            </section>
          );
        })}
        {!all.some(shown) && <p className="muted">Nothing matches “{q}”.</p>}
      </div>
    </Sheet>
  );
}
