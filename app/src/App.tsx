import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { exec, loadScene, sync, type FaceInfo, type Reply, type Scene } from "./engine";
import { Icons, Logo } from "./icons";
import { TEMPLATES } from "./templates";
import Viewport, { type Tool, type ViewportApi } from "./Viewport";

const TOOLS: { id: Tool; name: string; key: string }[] = [
  { id: "select", name: "Select", key: "V" },
  { id: "rect", name: "Rectangle", key: "R" },
  { id: "push", name: "Push / Pull", key: "P" },
  { id: "paint", name: "Paint", key: "B" },
  { id: "erase", name: "Erase", key: "E" },
  { id: "orbit", name: "Orbit (or hold Space)", key: "O" },
];

const SWATCHES: [number, number, number][] = [
  [0.95, 0.94, 0.91], [0.93, 0.86, 0.72], [0.96, 0.80, 0.70], [0.82, 0.88, 0.80],
  [0.78, 0.85, 0.93], [0.87, 0.82, 0.92], [0.98, 0.93, 0.62], [0.78, 0.78, 0.76],
  [0.45, 0.28, 0.18], [0.42, 0.55, 0.66], [0.32, 0.34, 0.38], [0.16, 0.17, 0.2],
];

const hex = (c: number[]) => "#" + c.map((x) => Math.round(x * 255).toString(16).padStart(2, "0")).join("");

interface Selection {
  faces: number[];
  group: number | null;
}

export default function App() {
  const [scene, setScene] = useState<Scene | null>(null);
  const [tool, setTool] = useState<Tool>("rect");
  const [sel, setSel] = useState<Selection>({ faces: [], group: null });
  const [color, setColor] = useState<[number, number, number]>(SWATCHES[1]);
  const [measure, setMeasure] = useState({ label: "", value: "" });
  const [typed, setTyped] = useState("");
  const [toast, setToast] = useState<{ text: string; bad?: boolean } | null>(null);
  const [pushBy, setPushBy] = useState("1");
  const [copyBy, setCopyBy] = useState("8");
  const api = useRef<ViewportApi | null>(null);
  const typedRef = useRef<HTMLInputElement>(null);

  const say = (text: string, bad = false) => {
    setToast({ text, bad });
    window.setTimeout(() => setToast((t) => (t?.text === text ? null : t)), 3200);
  };

  const refresh = useCallback(async () => setScene(await loadScene()), []);

  const run = useCallback(
    async (cmd: Record<string, unknown>, quiet = false): Promise<Reply> => {
      const r = await exec(cmd);
      if (!r.ok && !quiet) say(r.error ?? "That didn't work", true);
      if (!quiet) await refresh();
      return r;
    },
    [refresh]
  );

  useEffect(() => {
    refresh();
    // pick up edits Claude makes over MCP
    const id = window.setInterval(async () => {
      if (await sync()) {
        await refresh();
        say("Model updated by another session");
      }
    }, 700);
    return () => window.clearInterval(id);
  }, [refresh]);

  // drop selections that no longer exist
  useEffect(() => {
    if (!scene) return;
    const ids = new Set(scene.faces.map((f) => f.id));
    setSel((s) => ({ faces: s.faces.filter((f) => ids.has(f)), group: s.group && scene.groups.some((g) => g.id === s.group) ? s.group : null }));
  }, [scene]);

  const onPick = useCallback(
    (face: FaceInfo | null, { additive, deep }: { additive: boolean; deep: boolean }) => {
      if (!face) return setSel({ faces: [], group: null });
      if (face.group !== null && !deep && !additive) {
        const faces = scene?.faces.filter((f) => f.group === face.group).map((f) => f.id) ?? [];
        return setSel({ faces, group: face.group });
      }
      setSel((s) => {
        if (!additive) return { faces: [face.id], group: null };
        const has = s.faces.includes(face.id);
        return { faces: has ? s.faces.filter((f) => f !== face.id) : [...s.faces, face.id], group: null };
      });
    },
    [scene]
  );

  const selectGroup = (gid: number | null) => {
    const faces = scene?.faces.filter((f) => f.group === gid).map((f) => f.id) ?? [];
    setSel({ faces, group: gid });
  };

  const deleteSelection = async () => {
    if (sel.group !== null) await run({ cmd: "delete", ids: [sel.group] });
    else if (sel.faces.length) await run({ cmd: "delete", ids: sel.faces });
    setSel({ faces: [], group: null });
  };

  // keyboard: tools, undo, delete, and typing a value into the measurement box
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        await run({ cmd: e.shiftKey ? "redo" : "undo" });
        return;
      }
      if (mod) return;
      if (e.key === "Delete" || e.key === "Backspace") return void deleteSelection();
      if (e.shiftKey && e.key.toLowerCase() === "z") return api.current?.fit();
      if (/^[0-9.\-]$/.test(e.key)) {
        setTyped(e.key);
        typedRef.current?.focus();
        e.preventDefault();
        return;
      }
      const t = TOOLS.find((x) => x.key.toLowerCase() === e.key.toLowerCase());
      if (t) setTool(t.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const runTemplate = async (lines: string[]) => {
    for (const line of lines) {
      const r = await exec(JSON.parse(line));
      if (!r.ok) {
        say(r.error ?? "Template failed", true);
        break;
      }
    }
    api.current?.fitSoon();
    await refresh();
  };

  const selFaces = useMemo(() => scene?.faces.filter((f) => sel.faces.includes(f.id)) ?? [], [scene, sel]);
  const group = scene?.groups.find((g) => g.id === sel.group);
  const docName = scene?.doc.split("/").pop() ?? "";

  return (
    <div className="app">
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <Logo />
          <span className="brand-name">Riseline</span>
          <span className="sep">/</span>
          <span className="doc">{docName.replace(/\.riseline$/, "")}</span>
          <span className="live" title="Claude can edit this model over MCP; changes show up here">
            <i /> Live
          </span>
        </div>
        <div className="top-actions">
          <button className="icon-btn" title="Undo (⌘Z)" onClick={() => run({ cmd: "undo" })}>
            <Icons.undo />
          </button>
          <button className="icon-btn" title="Redo (⇧⌘Z)" onClick={() => run({ cmd: "redo" })}>
            <Icons.redo />
          </button>
          <button
            className="btn ghost"
            onClick={async () => {
              const path = scene!.doc.replace(/[^/]+$/, `exports/riseline-${Date.now()}.obj`);
              const r = await run({ cmd: "export_obj", path }, true);
              r.ok ? say(`Exported ${path}`) : say(r.error ?? "Export failed", true);
            }}
          >
            <Icons.export /> Export
          </button>
          <button
            className="btn primary"
            onClick={async () => {
              const path = scene!.doc.replace(/[^/]+$/, `renders/riseline-${Date.now()}.png`);
              const r = await run({ cmd: "render", out: path, width: 1920, height: 1080 }, true);
              r.ok ? say(`Rendered ${path}`) : say(r.error ?? "Render failed", true);
            }}
          >
            <Icons.camera /> Render
          </button>
        </div>
      </header>

      <aside className="panel left">
        <div className="section">
          <div className="section-title">Layers</div>
          <ul className="layers">
            {scene?.groups.map((g) => (
              <li key={g.id} className={sel.group === g.id ? "on" : ""} onClick={() => selectGroup(g.id)}>
                <Icons.cube />
                <span className="name">{g.name}</span>
                <span className="count">{g.faces}</span>
              </li>
            ))}
            {!!scene?.loose && (
              <li className={sel.group === null && sel.faces.length > 0 && selFaces.every((f) => f.group === null) ? "on" : ""} onClick={() => selectGroup(null)}>
                <Icons.layer />
                <span className="name">Loose geometry</span>
                <span className="count">{scene.loose}</span>
              </li>
            )}
            {!scene?.faces.length && <li className="empty">Nothing yet. Press R and draw on the ground.</li>}
          </ul>
        </div>
        <div className="section">
          <div className="section-title">Templates</div>
          <div className="tiles">
            {TEMPLATES.map((t) => (
              <button key={t.name} className="tile" onClick={() => runTemplate(t.lines)} title={t.about}>
                <span className="tile-art" style={{ background: t.tint }}>
                  <Icons.cube />
                </span>
                <span>{t.name}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="section grow">
          <div className="section-title">
            Assets <span className="soon">soon</span>
          </div>
          <div className="tiles muted">
            {["Door", "Window", "Bay", "Stoop", "Tree", "Car"].map((a) => (
              <div key={a} className="tile">
                <span className="tile-art" />
                <span>{a}</span>
              </div>
            ))}
          </div>
        </div>
      </aside>

      <main className="stage">
        <Viewport
          scene={scene}
          tool={tool}
          selected={sel.faces}
          color={color}
          run={run}
          onPick={onPick}
          onMeasure={(label, value) => setMeasure({ label, value })}
          api={api}
        />

        <nav className="toolbar">
          {TOOLS.map((t) => (
            <button key={t.id} className={`tool ${tool === t.id ? "on" : ""}`} onClick={() => setTool(t.id)} title={`${t.name} (${t.key})`}>
              {Icons[t.id]()}
            </button>
          ))}
          <span className="divider" />
          <button className="chip" style={{ background: hex(color) }} title="Paint color" onClick={() => setTool("paint")} />
        </nav>

        <div className="views">
          {(["iso", "top", "front", "right"] as const).map((v) => (
            <button key={v} onClick={() => api.current?.view(v)}>
              {v}
            </button>
          ))}
          <button onClick={() => api.current?.fit()} title="Zoom to fit (⇧Z)">
            fit
          </button>
        </div>

        <form
          className="measure"
          onSubmit={(e) => {
            e.preventDefault();
            api.current?.applyTyped(typed);
            setTyped("");
            typedRef.current?.blur();
          }}
        >
          <label>{measure.label || (tool === "rect" ? "Dimensions" : tool === "push" ? "Distance" : "Value")}</label>
          <input ref={typedRef} value={typed} placeholder={measure.value || "type a value"} onChange={(e) => setTyped(e.target.value)} onKeyDown={(e) => e.key === "Escape" && (setTyped(""), typedRef.current?.blur())} />
        </form>

        {toast && <div className={`toast ${toast.bad ? "bad" : ""}`}>{toast.text}</div>}
      </main>

      <aside className="panel right">
        {selFaces.length === 0 ? (
          <>
            <div className="section">
              <div className="section-title">Document</div>
              <div className="kv">
                <span>Faces</span>
                <b>{scene?.faces.length ?? 0}</b>
                <span>Groups</span>
                <b>{scene?.groups.length ?? 0}</b>
              </div>
            </div>
            <div className="section">
              <div className="section-title">Paint</div>
              <Swatches color={color} setColor={setColor} />
            </div>
            <div className="section hint">
              <p>
                <b>R</b> draw a rectangle on the ground or on any face. <b>P</b> drag a face to pull it up or push it in, or type an exact distance and press Enter.
              </p>
              <p>Middle-drag or hold Space to orbit, right-drag to pan, scroll to zoom.</p>
            </div>
          </>
        ) : (
          <>
            <div className="section">
              <div className="section-title">{group ? group.name : selFaces.length === 1 ? `Face ${selFaces[0].id}` : `${selFaces.length} faces`}</div>
              <div className="kv">
                <span>Area</span>
                <b>{selFaces.reduce((a, f) => a + f.area, 0).toFixed(2)} m²</b>
                {selFaces.length === 1 && (
                  <>
                    <span>Facing</span>
                    <b>{facing(selFaces[0].normal)}</b>
                  </>
                )}
              </div>
            </div>
            {selFaces.length === 1 && !group && (
              <div className="section">
                <div className="section-title">Push / Pull</div>
                <div className="row">
                  <input className="num" value={pushBy} onChange={(e) => setPushBy(e.target.value)} />
                  <span className="unit">m</span>
                  <button className="btn small" onClick={() => run({ cmd: "pushpull", face: selFaces[0].id, distance: Number(pushBy) })}>
                    Apply
                  </button>
                </div>
              </div>
            )}
            <div className="section">
              <div className="section-title">Fill</div>
              <Swatches
                color={color}
                setColor={(c) => {
                  setColor(c);
                  run({ cmd: "paint", ids: group ? [group.id] : sel.faces, color: c });
                }}
              />
            </div>
            <div className="section">
              <div className="section-title">Arrange</div>
              {group ? (
                <div className="row">
                  <span className="unit">Copy along X</span>
                  <input className="num" value={copyBy} onChange={(e) => setCopyBy(e.target.value)} />
                  <button className="btn small" onClick={() => run({ cmd: "copy", group: group.id, by: [Number(copyBy), 0, 0] })}>
                    Copy
                  </button>
                </div>
              ) : (
                <button className="btn small wide" onClick={() => run({ cmd: "group", ids: sel.faces })}>
                  Make group
                </button>
              )}
              <div className="row gap">
                <button className="btn small ghost" onClick={() => run({ cmd: "reverse", ids: group ? [group.id] : sel.faces })}>
                  Flip faces
                </button>
                <button className="btn small danger" onClick={deleteSelection}>
                  Delete
                </button>
              </div>
            </div>
          </>
        )}
      </aside>
    </div>
  );
}

function Swatches({ color, setColor }: { color: number[]; setColor: (c: [number, number, number]) => void }) {
  return (
    <div className="swatches">
      {SWATCHES.map((c) => (
        <button key={hex(c)} className={`swatch ${hex(c) === hex(color) ? "on" : ""}`} style={{ background: hex(c) }} onClick={() => setColor(c)} />
      ))}
    </div>
  );
}

function facing(n: number[]): string {
  const names: [number[], string][] = [
    [[0, 0, 1], "Up"],
    [[0, 0, -1], "Down"],
    [[0, -1, 0], "Front (−Y)"],
    [[0, 1, 0], "Back (+Y)"],
    [[1, 0, 0], "Right (+X)"],
    [[-1, 0, 0], "Left (−X)"],
  ];
  const best = names.find(([d]) => d[0] * n[0] + d[1] * n[1] + d[2] * n[2] > 0.94);
  return best ? best[1] : n.map((x) => x.toFixed(2)).join(", ");
}
