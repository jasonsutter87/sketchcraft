// The Appearance dialog: pick a skin (built-in or from your Skins folder),
// tweak its colors live, save it as a .sketchskin file to share. And the
// Customize Workspace dialog: which tools sit on the toolbar, where it docks,
// which panels show.

import { useEffect, useRef, useState } from "react";
import { ask } from "./ask";
import { exec } from "./engine";
import { Icons } from "./icons";
import { iconOk, isDark, readSkin, SKINS, TOKEN_LABELS, TOKENS, type Skin, type SkinToken } from "./skins";
import type { Tool } from "./Viewport";
import { toolTitle } from "./keymapstore";
import { TOOL_INFO, WORKSPACES, type Layout, type Workspace, type WorkspaceEdits } from "./workspace";
import { Sheet, useEscapeToClose } from "./sheet";

/** the tools an icon set can cover (its files are named after these) */
const TOOL_NAMES = Object.keys(TOOL_INFO);

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

/** A tiny drawing of a layout: where the tool bars sit around the model. */
function LayoutMini({ kind }: { kind: Layout }) {
  if (kind === "side") {
    return (
      <svg className="layout-mini" viewBox="0 0 120 72" aria-hidden="true">
        <rect x="0.5" y="0.5" width="119" height="71" rx="5" className="lm-frame" />
        <rect x="1" y="1" width="91" height="70" className="lm-model" />
        <rect x="5" y="5" width="11" height="61" rx="2" className="lm-float" />
        <rect x="92" y="1" width="27" height="70" className="lm-panel" />
        <path d="M92 1v70M92 39h27" className="lm-bar2" />
      </svg>
    );
  }
  return (
    <svg className="layout-mini" viewBox="0 0 120 72" aria-hidden="true">
      <rect x="0.5" y="0.5" width="119" height="71" rx="5" className="lm-frame" />
      <rect x="1" y="1" width="20" height="70" className="lm-panel" />
      <rect x="99" y="1" width="20" height="70" className="lm-panel" />
      {kind === "classic" ? (
        <>
          <rect x="21" y="1" width="78" height="8" className="lm-bar" />
          <rect x="21" y="9" width="78" height="5" className="lm-bar2" />
          <rect x="21" y="14" width="78" height="57" className="lm-model" />
        </>
      ) : (
        <>
          <rect x="21" y="1" width="78" height="70" className="lm-model" />
          <rect x="33" y="5" width="54" height="6" rx="3" className="lm-float" />
          <rect x="35" y="60" width="50" height="7" rx="3.5" className="lm-float" />
        </>
      )}
    </svg>
  );
}

export function AppearanceDialog({
  current,
  onPick,
  onClose,
  onToast,
  layout,
  onLayout,
}: {
  current: Skin;
  onPick: (s: Skin, useBackdrop: boolean) => void;
  onClose: () => void;
  onToast: (t: string, bad?: boolean) => void;
  layout: Layout;
  onLayout: (l: Layout) => void;
}) {
  const [mine, setMine] = useState<Skin[]>([]);
  const [dir, setDir] = useState("");
  const [editing, setEditing] = useState<Skin | null>(null);
  // the skin worn before editing began (Undo changes goes back to it), and
  // the edited skin's original id (to name it when saving)
  const [before, setBefore] = useState<Skin | null>(null);
  const [origin, setOrigin] = useState("");
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
  // edits are worn (and remembered) as you make them, saved file or not;
  // only Undo changes puts the old skin back
  const edit = (k: SkinToken, v: string) => {
    if (!editing) return;
    const next = { ...editing, id: "custom", tokens: { ...editing.tokens, [k]: v } };
    next.dark = isDark(next.tokens.panel);
    setEditing(next);
    onPick(next, false);
  };
  // an icon set: image files named after the tools (line.svg, push.png, ...)
  const iconInput = useRef<HTMLInputElement>(null);
  const [showNames, setShowNames] = useState(false);
  const loadIcons = async (files: FileList | null) => {
    const base = editing ?? current;
    if (!files?.length) return;
    const icons: Record<string, string> = { ...(base.icons ?? {}) };
    const skipped: string[] = [];
    for (const f of Array.from(files)) {
      const tool = f.name.replace(/\.[^.]+$/, "").toLowerCase();
      const url = await new Promise<string>((res) => {
        const fr = new FileReader();
        fr.onload = () => res(String(fr.result));
        fr.onerror = () => res("");
        fr.readAsDataURL(f);
      });
      if (TOOL_NAMES.includes(tool) && iconOk(url)) icons[tool] = url;
      else skipped.push(f.name);
    }
    const next = { ...base, id: "custom", icons };
    if (editing) setEditing(next);
    onPick(next, false);
    const n = Object.keys(icons).length;
    onToast(`${n} tool icon${n === 1 ? "" : "s"} in this theme${skipped.length ? `. Skipped ${skipped.slice(0, 4).join(", ")}${skipped.length > 4 ? "…" : ""}: name each file after a tool (SVG, PNG, JPEG or WebP, up to 256 KB)` : ""}.`, skipped.length > 0);
  };
  const clearIcons = () => {
    const base = editing ?? current;
    const next = { ...base, id: "custom", icons: undefined };
    if (editing) setEditing(next);
    onPick(next, false);
  };
  const save = async () => {
    if (!editing) return;
    const name = await ask("Name this theme", origin.startsWith("file:") ? editing.name : `My ${editing.name}`);
    if (!name) return;
    const skin = { name, blurb: "Made in SketchCraft", dark: editing.dark, backdrop: editing.backdrop, tokens: editing.tokens, ...(editing.icons ? { icons: editing.icons } : {}) };
    const r = await exec({ cmd: "save_skin", name, skin });
    if (!r.ok) return onToast(r.error ?? "Couldn't save the theme", true);
    const saved = readSkin(skin, name)!;
    onPick(saved, false);
    setEditing(null);
    load();
    onToast(`Saved ${name}: share the .sketchskin file from your themes folder`);
  };
  const all = [...SKINS, ...mine];
  useEscapeToClose(onClose);
  return (
    <div
      className="sheet-backdrop"
      onClick={onClose}
    >
      <div className="sheet" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Appearance">
        <div className="sheet-head">
          <b>Themes</b>
          <span className="muted">{editing ? `Editing ${editing.name}: changes show as you make them.` : "How SketchCraft looks: its colors, layout and icons."}</span>
        </div>
        {!editing && <div className="theme-h">Theme <span className="muted small">Click one to wear it (it brings its layout). Edit any to make your own.</span></div>}
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
                    setBefore(current);
                    setOrigin(s.id);
                    setEditing({ ...s, tokens: { ...s.tokens } });
                    onPick(s, s.id !== current.id);
                  }}
                >
                  Edit colors…
                </button>
              </div>
            ))}
          </div>
        )}
        {!editing && (
          <>
            <div className="theme-h">Layout</div>
            <div className="layout-pick" role="radiogroup" aria-label="Layout">
              {(
                [
                  ["classic", "Classic", "Tool bars docked above the model, like SketchUp"],
                  ["floating", "Floating", "SketchCraft's own: tool bars float over the model"],
                  ["side", "Vertical tools", "Tools on the left, Outliner and properties on the right"],
                ] as const
              ).map(([id, name, about]) => (
                <button key={id} role="radio" aria-checked={layout === id} className={`layout-card${layout === id ? " on" : ""}`} onClick={() => onLayout(id)}>
                  <LayoutMini kind={id} />
                  <b>{name}</b>
                  <span className="muted small">{about}</span>
                </button>
              ))}
            </div>
          </>
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
        <div className="theme-h">Icons</div>
        <div className="skin-icons">
              <span>
                Tool icons{(editing ?? current).icons ? `: ${Object.keys((editing ?? current).icons ?? {}).length} of ${TOOL_NAMES.length} swapped` : ": built in"}
              </span>
              <input ref={iconInput} type="file" multiple accept="image/svg+xml,image/png,image/jpeg,image/webp" hidden onChange={(e) => (loadIcons(e.target.files), (e.target.value = ""))} />
              <button className="btn small ghost" onClick={() => iconInput.current?.click()} title="Pick image files named after the tools: line.svg, push.png, ...">
                Load icons…
              </button>
              {(editing ?? current).icons && (
                <button className="btn small ghost" onClick={clearIcons}>
                  Clear icons
                </button>
              )}
              <button className="btn small ghost" onClick={() => setShowNames((v) => !v)}>
                {showNames ? "Hide" : "File names"}
              </button>
              {showNames && <p className="muted small skin-icon-names">Name each image after its tool: {TOOL_NAMES.map((t) => `${t}.svg`).join(", ")}. Any you leave out keep the built-in icon.</p>}
            </div>
        <div className="row gap" style={{ marginTop: 14 }}>
          {editing ? (
            <>
              <button className="btn small" onClick={save}>
                Save as my theme…
              </button>
              <button
                className="btn small ghost"
                onClick={() => {
                  if (before) onPick(before, false);
                  setEditing(null);
                }}
              >
                Undo changes
              </button>
              <span className="muted small">Your changes are kept as you make them; save to share them as a file.</span>
              <span style={{ flex: 1 }} />
              <button className="btn small" onClick={onClose}>
                Done
              </button>
            </>
          ) : (
            <>
              <button className="btn small ghost" onClick={() => exec({ cmd: "reveal", path: dir })} disabled={!dir}>
                Open themes folder
              </button>
              <button className="btn small ghost" onClick={load}>
                Reload
              </button>
              <span className="muted small">Got a theme file (.sketchskin) from someone? Drop it in the themes folder, then Reload.</span>
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
  const ORDER: Tool[][] = [["select"], ["wall", "stairs", "roof", "column"], ["line", "freehand", "arc", "arccenter", "arc3", "pie", "rect", "rrect", "circle", "polygon"], ["push", "offset", "followme"], ["move", "rotate", "scale"], ["tape", "protractor", "axes", "dimension", "text", "paint", "erase"], ["smoove", "stamp", "drape", "flipedge"], ["orbit", "pan", "zoom", "zoomwindow", "position", "lookaround", "walk", "section"]];
  const toggle = (t: Tool) => {
    const next = new Set(on);
    if (next.has(t)) next.delete(t);
    else next.add(t);
    onChange({ tools: ORDER.map((g) => g.filter((x) => next.has(x))).filter((g) => g.length) });
  };
  return (
    <Sheet onClose={onClose} label="Customize workspace" width={560}>
      <div className="sheet-head">
        <b>Customize {ws.name}</b>
        <span className="muted">{ws.blurb}</span>
      </div>
      <div className="section-title">Toolbar: click a tool to add or remove it</div>
      {ORDER.map((g, i) => (
        <div key={i} className="ws-tools">
          {g.map((t) => (
            <button key={t} className={on.has(t) ? "on" : ""} title={toolTitle(TOOL_INFO[t as keyof typeof TOOL_INFO].name, t)} onClick={() => toggle(t)}>
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
          <option value="classic">Classic: docked above the model</option>
        </select>
        <span>Navigation panel</span>
        <select className="units-select" value={ws.showLeft ? ws.leftTab : "off"} onChange={(e) => (e.target.value === "off" ? onChange({ showLeft: false }) : onChange({ showLeft: true, leftTab: e.target.value as Workspace["leftTab"] }))}>
          {ws.id === "layout" && <option value="pages">Shown, on Pages</option>}
          <option value="outliner">Shown, on Outliner</option>
          {ws.id !== "layout" && <option value="library">Shown, on Library</option>}
          <option value="views">Shown, on Views</option>
          <option value="off">Hidden</option>
        </select>
        <span>Properties panel</span>
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
    </Sheet>
  );
}
