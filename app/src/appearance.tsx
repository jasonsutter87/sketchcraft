// The Appearance dialog: pick a skin (built-in or from your Skins folder),
// tweak its colors live, save it as a .sketchskin file to share. And the
// Customize Workspace dialog: which tools sit on the toolbar, where it docks,
// which panels show.

import { useEffect, useState } from "react";
import { ask } from "./ask";
import { exec } from "./engine";
import { Icons } from "./icons";
import { applySkin, isDark, readSkin, SKINS, TOKEN_LABELS, TOKENS, type Skin, type SkinToken } from "./skins";
import type { Tool } from "./Viewport";
import { TOOL_INFO, WORKSPACES, type Workspace, type WorkspaceEdits } from "./workspace";

function Mini({ s }: { s: Skin }) {
  const t = s.tokens;
  return (
    <div className="mini" style={{ background: t.bg, borderRadius: t.radius }}>
      <i className="top" style={{ background: t.panel, borderBottom: `1px solid ${t.line}` }} />
      <i style={{ background: t.panel, borderRight: `1px solid ${t.line}` }} />
      <i className="mstage" style={{ background: t.bg }}>
        <b style={{ background: t.accent }} />
      </i>
      <i style={{ background: t.panel, borderLeft: `1px solid ${t.line}` }} />
    </div>
  );
}

export function AppearanceDialog({ current, onPick, onClose, onToast }: { current: Skin; onPick: (s: Skin, useBackdrop: boolean) => void; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const [mine, setMine] = useState<Skin[]>([]);
  const [dir, setDir] = useState("");
  const [editing, setEditing] = useState<Skin | null>(null);
  const load = () =>
    exec({ cmd: "skins" }).then((r) => {
      if (!r.ok) return;
      setDir(r.dir as string);
      const list = (r.skins as { name: string; skin: unknown }[]).map((f) => readSkin(f.skin, f.name)).filter((x): x is Skin => !!x);
      setMine(list);
    });
  useEffect(() => {
    load();
  }, []);
  // editing previews live; closing without saving puts the old skin back
  const edit = (k: SkinToken, v: string) => {
    if (!editing) return;
    const next = { ...editing, tokens: { ...editing.tokens, [k]: v } };
    next.dark = isDark(next.tokens.panel);
    setEditing(next);
    applySkin(next);
  };
  const save = async () => {
    if (!editing) return;
    const name = await ask("Name this skin", editing.id.startsWith("file:") ? editing.name : `My ${editing.name}`);
    if (!name) return;
    const skin = { name, blurb: "Made in SketchCraft", dark: editing.dark, backdrop: editing.backdrop, tokens: editing.tokens };
    const r = await exec({ cmd: "save_skin", name, skin });
    if (!r.ok) return onToast(r.error ?? "Couldn't save the skin", true);
    const saved = readSkin(skin, name)!;
    onPick(saved, false);
    setEditing(null);
    load();
    onToast(`Saved ${name}: share the .sketchskin file from your Skins folder`);
  };
  const all = [...SKINS, ...mine];
  return (
    <div
      className="sheet-backdrop"
      onClick={() => {
        if (editing) applySkin(current);
        onClose();
      }}
    >
      <div className="sheet" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Appearance">
        <div className="sheet-head">
          <b>Skins</b>
          <span className="muted">{editing ? `Editing ${editing.name}: changes show as you make them.` : "Click one to wear it. Edit any of them to make your own."}</span>
        </div>
        {!editing && (
          <div className="skin-grid">
            {all.map((s) => (
              <div key={s.id} className={`skin-card ${s.id === current.id ? "on" : ""}`} role="button" tabIndex={0} onClick={() => onPick(s, true)} onKeyDown={(e) => e.key === "Enter" && onPick(s, true)}>
                <Mini s={s} />
                <span className="name">{s.name}</span>
                <span className="blurb">{s.blurb}</span>
                <button
                  className="btn small ghost"
                  onClick={(e) => {
                    e.stopPropagation();
                    setEditing({ ...s, tokens: { ...s.tokens } });
                    applySkin(s);
                  }}
                >
                  Edit colors…
                </button>
              </div>
            ))}
          </div>
        )}
        {editing && (
          <div className="skin-edit">
            {TOKENS.map((k) =>
              k === "font" ? (
                <label key={k}>
                  {TOKEN_LABELS[k]}
                  <select value={editing.tokens.font} onChange={(e) => edit("font", e.target.value)}>
                    {[...new Set([editing.tokens.font, ...SKINS.map((s) => s.tokens.font)])].map((f) => (
                      <option key={f} value={f}>
                        {f.split(",")[0].replace(/"/g, "")}
                      </option>
                    ))}
                  </select>
                </label>
              ) : k === "radius" ? (
                <label key={k}>
                  {TOKEN_LABELS[k]} {parseInt(editing.tokens.radius, 10) || 0}px
                  <input type="range" min={0} max={16} value={parseInt(editing.tokens.radius, 10) || 0} onChange={(e) => edit("radius", `${e.target.value}px`)} />
                </label>
              ) : /^#[0-9a-f]{6}$/i.test(editing.tokens[k]) ? (
                <label key={k}>
                  {TOKEN_LABELS[k]}
                  <input type="color" value={editing.tokens[k]} onChange={(e) => edit(k, e.target.value)} />
                </label>
              ) : (
                <label key={k}>
                  {TOKEN_LABELS[k]}
                  <input type="text" value={editing.tokens[k]} onChange={(e) => edit(k, e.target.value)} />
                </label>
              )
            )}
          </div>
        )}
        <div className="row gap" style={{ marginTop: 14 }}>
          {editing ? (
            <>
              <button className="btn small" onClick={save}>
                Save as my skin…
              </button>
              <button
                className="btn small ghost"
                onClick={() => {
                  applySkin(current);
                  setEditing(null);
                }}
              >
                Cancel
              </button>
            </>
          ) : (
            <>
              <button className="btn small ghost" onClick={() => exec({ cmd: "reveal", path: dir })} disabled={!dir}>
                Open Skins folder
              </button>
              <button className="btn small ghost" onClick={load}>
                Reload
              </button>
              <span className="muted small">Got a .sketchskin from someone? Drop it in the Skins folder, then Reload.</span>
              <span style={{ flex: 1 }} />
              <button className="btn small" onClick={onClose}>
                Done
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export function WorkspaceDialog({ ws, onChange, onReset, onClose }: { ws: Workspace; onChange: (e: WorkspaceEdits) => void; onReset: () => void; onClose: () => void }) {
  const on = new Set(ws.tools.flat());
  const base = WORKSPACES.find((w) => w.id === ws.id)!;
  // toggling keeps each tool in its family's group, in the standard order
  const ORDER: Tool[][] = [["select"], ["wall", "stairs", "roof", "column"], ["line", "arc", "rect", "circle", "polygon"], ["push", "offset", "followme"], ["move", "rotate", "scale"], ["tape", "protractor", "dimension", "text", "paint", "erase"], ["smoove", "stamp", "drape", "flipedge"], ["orbit", "pan", "zoom", "zoomwindow", "position", "lookaround", "walk", "section"]];
  const toggle = (t: Tool) => {
    const next = new Set(on);
    if (next.has(t)) next.delete(t);
    else next.add(t);
    onChange({ tools: ORDER.map((g) => g.filter((x) => next.has(x))).filter((g) => g.length) });
  };
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" style={{ width: "min(560px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Customize workspace">
        <div className="sheet-head">
          <b>Customize {ws.name}</b>
          <span className="muted">{ws.blurb}</span>
        </div>
        <div className="section-title">Toolbar: click a tool to add or remove it</div>
        {ORDER.map((g, i) => (
          <div key={i} className="ws-tools">
            {g.map((t) => (
              <button key={t} className={on.has(t) ? "on" : ""} title={`${TOOL_INFO[t as keyof typeof TOOL_INFO].name}${TOOL_INFO[t as keyof typeof TOOL_INFO].key ? ` (${TOOL_INFO[t as keyof typeof TOOL_INFO].key})` : ""}`} onClick={() => toggle(t)}>
                {Icons[t as keyof typeof Icons]?.()}
              </button>
            ))}
          </div>
        ))}
        <p className="muted small">Every shortcut key still works, even for tools you hide.</p>
        <div className="kv" style={{ marginTop: 10 }}>
          <span>Toolbar</span>
          <select className="units-select" value={ws.dock} onChange={(e) => onChange({ dock: e.target.value as Workspace["dock"] })}>
            <option value="bottom">Along the bottom</option>
            <option value="left">Down the left side</option>
          </select>
          <span>Left panel</span>
          <select className="units-select" value={ws.showLeft ? ws.leftTab : "off"} onChange={(e) => (e.target.value === "off" ? onChange({ showLeft: false }) : onChange({ showLeft: true, leftTab: e.target.value as Workspace["leftTab"] }))}>
            <option value="outliner">Shown, on Outliner</option>
            <option value="library">Shown, on Library</option>
            <option value="views">Shown, on Views</option>
            <option value="off">Hidden</option>
          </select>
          <span>Right panel</span>
          <select className="units-select" value={ws.showRight ? "on" : "off"} onChange={(e) => onChange({ showRight: e.target.value === "on" })}>
            <option value="on">Shown</option>
            <option value="off">Hidden</option>
          </select>
        </div>
        <div className="row gap" style={{ marginTop: 16 }}>
          <button className="btn small ghost" onClick={onReset} disabled={JSON.stringify(ws) === JSON.stringify(base)}>
            Reset {ws.name} to how it came
          </button>
          <span style={{ flex: 1 }} />
          <button className="btn small" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
