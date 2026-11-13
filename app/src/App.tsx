import { useCallback, useEffect, useRef, useState } from "react";
import { exec, loadScene, sync, type Reply, type Scene } from "./engine";
import { Icons, Logo } from "./icons";
import { ContextMenu, contextItems, EntityInfo, Library, Outliner, Views } from "./panels";
import { EMPTY, isEmpty, selIds, type Sel } from "./selection";
import Viewport, { type Styles, type Tool, type ViewportApi } from "./Viewport";

const TOOL_GROUPS: { id: Tool; name: string; key: string }[][] = [
  [{ id: "select", name: "Select", key: "Space" }],
  [
    { id: "line", name: "Line", key: "L" },
    { id: "arc", name: "Arc", key: "A" },
    { id: "rect", name: "Rectangle", key: "R" },
    { id: "circle", name: "Circle", key: "C" },
    { id: "polygon", name: "Polygon", key: "N" },
  ],
  [
    { id: "push", name: "Push / Pull", key: "P" },
    { id: "offset", name: "Offset", key: "F" },
    { id: "followme", name: "Follow Me", key: "J" },
  ],
  [
    { id: "move", name: "Move (Option to copy)", key: "M" },
    { id: "rotate", name: "Rotate", key: "Q" },
    { id: "scale", name: "Scale", key: "S" },
  ],
  [
    { id: "tape", name: "Tape Measure (makes guides)", key: "T" },
    { id: "paint", name: "Paint (Option-click to sample)", key: "B" },
    { id: "erase", name: "Erase (Shift to hide)", key: "E" },
  ],
  [
    { id: "orbit", name: "Orbit (or hold Space / middle-drag)", key: "O" },
    { id: "section", name: "Section cut: click a face", key: "X" },
  ],
];
const ALL_TOOLS = TOOL_GROUPS.flat();

const SWATCHES: [number, number, number][] = [
  [0.95, 0.94, 0.91], [0.93, 0.86, 0.72], [0.96, 0.8, 0.7], [0.82, 0.88, 0.8], [0.78, 0.85, 0.93], [0.87, 0.82, 0.92],
  [0.98, 0.93, 0.62], [0.78, 0.78, 0.76], [0.72, 0.16, 0.18], [0.45, 0.28, 0.18], [0.42, 0.55, 0.66], [0.42, 0.6, 0.36],
  [0.55, 0.57, 0.6], [0.32, 0.34, 0.38], [0.16, 0.17, 0.2], [0.97, 0.97, 0.95], [0.85, 0.55, 0.42], [0.6, 0.66, 0.5],
];

const hex = (c: number[]) => "#" + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255).toString(16).padStart(2, "0")).join("");
const unhex = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as [number, number, number];

type Tab = "outliner" | "library" | "views";

export default function App() {
  const [scene, setScene] = useState<Scene | null>(null);
  const [tool, setTool] = useState<Tool>("rect");
  const [sel, setSel] = useState<Sel>(EMPTY);
  const [color, setColor] = useState<[number, number, number]>(SWATCHES[1]);
  const [measure, setMeasure] = useState({ label: "", value: "" });
  const [typed, setTyped] = useState("");
  const [toast, setToast] = useState<{ text: string; bad?: boolean } | null>(null);
  const [tab, setTab] = useState<Tab>("outliner");
  const [styles, setStyles] = useState<Styles>({ xray: false, shadows: false, edges: true, parallel: false });
  const [section, setSection] = useState(false);
  const [placing, setPlacing] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const api = useRef<ViewportApi | null>(null);
  const typedRef = useRef<HTMLInputElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const say = useCallback((text: string, bad = false) => {
    setToast({ text, bad });
    window.setTimeout(() => setToast((t) => (t?.text === text ? null : t)), 3600);
  }, []);

  const refresh = useCallback(async () => setScene(await loadScene()), []);

  const run = useCallback(
    async (cmd: Record<string, unknown>): Promise<Reply> => {
      const r = await exec(cmd);
      if (!r.ok) say(r.error ?? "That didn't work", true);
      await refresh();
      return r;
    },
    [refresh, say]
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
  }, [refresh, say]);

  // drop selections that no longer exist
  useEffect(() => {
    if (!scene) return;
    const f = new Set(scene.faces.map((x) => x.id));
    const e = new Set(scene.edgeList.map((x) => x.id));
    const g = new Set(scene.groups.map((x) => x.id));
    const i = new Set(scene.instances.map((x) => x.id));
    setSel((s) => ({ faces: s.faces.filter((x) => f.has(x)), edges: s.edges.filter((x) => e.has(x)), groups: s.groups.filter((x) => g.has(x)), instances: s.instances.filter((x) => i.has(x)) }));
  }, [scene]);

  const chooseTool = useCallback((t: Tool) => {
    setTool(t);
    if (t !== "place") setPlacing(null);
  }, []);

  const deleteSelection = useCallback(async () => {
    if (isEmpty(sel)) return;
    await run({ cmd: "delete", ids: selIds(sel) });
    setSel(EMPTY);
  }, [sel, run]);

  // keyboard: tools, undo, delete, groups, typing into the measurement box
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      const mod = e.metaKey || e.ctrlKey;
      const k = e.key.toLowerCase();
      if (mod && k === "z") {
        e.preventDefault();
        await run({ cmd: e.shiftKey ? "redo" : "undo" });
        return;
      }
      if (mod && k === "a" && scene) {
        e.preventDefault();
        const inGroup = scene.activeGroup;
        setSel({
          faces: scene.faces.filter((f) => f.group === inGroup || (inGroup == null && f.group == null)).map((f) => f.id),
          edges: [],
          groups: inGroup == null ? scene.groups.map((g) => g.id) : [],
          instances: inGroup == null && scene.editing == null ? scene.instances.map((i) => i.id) : [],
        });
        return;
      }
      if (mod) return;
      if (e.key === "Delete" || e.key === "Backspace") return void deleteSelection();
      if (e.shiftKey && k === "z") return api.current?.fit();
      if (k === "g" && sel.faces.length) {
        await run({ cmd: "group", ids: sel.faces });
        setSel(EMPTY);
        return;
      }
      if (k === "h" && !isEmpty(sel)) {
        await run({ cmd: "hide", ids: selIds(sel) });
        setSel(EMPTY);
        return;
      }
      if (/^[0-9.\-x/]$/.test(e.key) && !(k === "x" && tool !== "move")) {
        setTyped(e.key);
        typedRef.current?.focus();
        e.preventDefault();
        return;
      }
      const t = ALL_TOOLS.find((x) => x.key.toLowerCase() === k);
      if (t) chooseTool(t.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [scene, sel, tool, run, deleteSelection, chooseTool]);

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

  const importObj = async (file: File) => {
    const text = await file.text();
    const r = await run({ cmd: "import_obj", text, up: "y" });
    if (r.ok) {
      say(`Imported ${file.name}: ${r.faces} faces`);
      api.current?.fitSoon();
      await refresh();
    }
  };

  const doExport = async (kind: "obj" | "glb" | "stl") => {
    setExportOpen(false);
    const path = scene!.doc.replace(/[^/]+$/, `exports/riseline-${Date.now()}.${kind}`);
    const cmd = kind === "obj" ? { cmd: "export_obj", path } : kind === "glb" ? { cmd: "export_glb", path } : { cmd: "export_stl", path, scale: 1000 };
    const r = await run(cmd);
    if (r.ok) say(`Exported ${path}`);
  };

  const docName = scene?.doc.split("/").pop()?.replace(/\.riseline$/, "") ?? "";
  const selCount = selIds(sel).length;

  return (
    <div className="app">
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <Logo />
          <span className="brand-name">Riseline</span>
          <span className="sep">/</span>
          <span className="doc">{docName}</span>
          <span className="live" title="Claude can edit this model over MCP; changes show up here">
            <i /> Live
          </span>
          {(scene?.editing != null || scene?.activeGroup != null) && (
            <button className="editing-pill" onClick={() => run({ cmd: "close" })}>
              Editing {scene?.editing != null ? "component" : "group"} · Done
            </button>
          )}
        </div>
        <div className="top-actions">
          <button className="icon-btn" title="Undo (⌘Z)" onClick={() => run({ cmd: "undo" })}>
            <Icons.undo />
          </button>
          <button className="icon-btn" title="Redo (⇧⌘Z)" onClick={() => run({ cmd: "redo" })}>
            <Icons.redo />
          </button>
          <button className="btn ghost" onClick={() => fileRef.current?.click()} title="Import an OBJ file">
            <Icons.import /> Import
          </button>
          <input ref={fileRef} type="file" accept=".obj" hidden onChange={(e) => e.target.files?.[0] && importObj(e.target.files[0])} />
          <div className="dropdown">
            <button className="btn ghost" onClick={() => setExportOpen((o) => !o)}>
              <Icons.export /> Export
            </button>
            {exportOpen && (
              <div className="dropdown-menu">
                <button onClick={() => doExport("glb")}>glTF (.glb) <small>Blender, web, game engines</small></button>
                <button onClick={() => doExport("obj")}>OBJ + MTL <small>everything opens it</small></button>
                <button onClick={() => doExport("stl")}>STL <small>3D printing, millimeters</small></button>
              </div>
            )}
          </div>
          <button
            className="btn primary"
            onClick={async () => {
              const path = scene!.doc.replace(/[^/]+$/, `renders/riseline-${Date.now()}.png`);
              const c = api.current!.camera();
              const r = await run({ cmd: "render", out: path, width: 1920, height: 1080, camera: c });
              if (r.ok) say(`Rendered ${path}`);
            }}
          >
            <Icons.camera /> Render
          </button>
        </div>
      </header>

      <aside className="panel left">
        <div className="tabs">
          {(["outliner", "library", "views"] as Tab[]).map((t) => (
            <button key={t} className={tab === t ? "on" : ""} onClick={() => setTab(t)}>
              {t === "outliner" ? "Outliner" : t === "library" ? "Library" : "Views"}
            </button>
          ))}
        </div>
        <div className="section grow">
          {scene && tab === "outliner" && <Outliner scene={scene} sel={sel} setSel={setSel} run={run} />}
          {scene && tab === "library" && (
            <Library
              scene={scene}
              placing={placing}
              onTemplate={runTemplate}
              place={(k) => {
                setPlacing(k);
                setTool(k ? "place" : "select");
              }}
            />
          )}
          {scene && tab === "views" && <Views scene={scene} run={run} camera={() => api.current!.camera()} goTo={(c) => api.current?.goTo(c)} />}
        </div>
      </aside>

      <main className="stage">
        <Viewport
          scene={scene}
          tool={tool}
          setTool={chooseTool}
          sel={sel}
          setSel={setSel}
          color={color}
          setColor={setColor}
          styles={styles}
          placeAsset={placing}
          run={run}
          onMeasure={(label, value) => setMeasure({ label, value })}
          onContext={(x, y) => setMenu({ x, y })}
          onToast={say}
          onSection={setSection}
          api={api}
        />

        <nav className="toolbar">
          {TOOL_GROUPS.map((g, i) => (
            <div key={i} className="tool-group">
              {g.map((t) => (
                <button key={t.id} className={`tool ${tool === t.id ? "on" : ""}`} onClick={() => chooseTool(t.id)} title={`${t.name} (${t.key})`}>
                  {Icons[t.id]()}
                </button>
              ))}
            </div>
          ))}
          <label className="chip" style={{ background: hex(color) }} title="Paint color">
            <input type="color" value={hex(color)} onChange={(e) => (setColor(unhex(e.target.value)), chooseTool("paint"))} />
          </label>
        </nav>

        <div className="views">
          {(["iso", "top", "front", "right"] as const).map((v) => (
            <button key={v} onClick={() => api.current?.view(v)}>
              {v}
            </button>
          ))}
          <button onClick={() => api.current?.fit()} title="Zoom extents (⇧Z)">
            fit
          </button>
          <span className="vsep" />
          {(
            [
              ["xray", "X-ray"],
              ["shadows", "Shadows"],
              ["edges", "Edges"],
              ["parallel", "Parallel"],
            ] as [keyof Styles, string][]
          ).map(([k, label]) => (
            <button key={k} className={styles[k] ? "on" : ""} onClick={() => setStyles((s) => ({ ...s, [k]: !s[k] }))}>
              {label}
            </button>
          ))}
          {section && (
            <button
              className="on"
              onClick={() => {
                api.current?.clearSection();
                setSection(false);
              }}
              title="Remove the section cut"
            >
              Section ✕
            </button>
          )}
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
          <label>{measure.label || "Value"}</label>
          <input ref={typedRef} value={typed} placeholder={measure.value || "type, then Enter"} onChange={(e) => setTyped(e.target.value)} onKeyDown={(e) => e.key === "Escape" && (setTyped(""), typedRef.current?.blur())} />
        </form>

        {toast && <div className={`toast ${toast.bad ? "bad" : ""}`}>{toast.text}</div>}
        {menu && scene && <ContextMenu x={menu.x} y={menu.y} close={() => setMenu(null)} items={contextItems(scene, sel, run, setSel, () => api.current?.fit())} />}
      </main>

      <aside className="panel right">
        {scene && selCount > 0 && <EntityInfo scene={scene} sel={sel} run={run} />}
        {selCount === 0 && (
          <div className="section">
            <div className="section-title">Document</div>
            <div className="kv">
              <span>Faces</span>
              <b>{scene?.faces.length ?? 0}</b>
              <span>Groups</span>
              <b>{scene?.groups.length ?? 0}</b>
              <span>Components</span>
              <b>{scene?.instances.length ?? 0}</b>
            </div>
          </div>
        )}
        <div className="section">
          <div className="section-title">{selCount ? "Fill" : "Paint"}</div>
          <div className="swatches">
            {SWATCHES.map((c) => (
              <button
                key={hex(c)}
                className={`swatch ${hex(c) === hex(color) ? "on" : ""}`}
                style={{ background: hex(c) }}
                onClick={() => {
                  setColor(c);
                  const ids = [...sel.faces, ...sel.groups];
                  if (ids.length) run({ cmd: "paint", ids, color: c });
                }}
              />
            ))}
          </div>
        </div>
        {selCount > 0 && (
          <div className="section">
            <div className="section-title">Arrange</div>
            <div className="btn-grid">
              {sel.faces.length > 0 && (
                <button className="btn small" onClick={() => run({ cmd: "group", ids: sel.faces }).then(() => setSel(EMPTY))}>
                  Make group
                </button>
              )}
              {(sel.faces.length > 0 || sel.groups.length > 0) && (
                <button
                  className="btn small"
                  onClick={() => {
                    const name = window.prompt("Component name", "Component");
                    if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
                  }}
                >
                  Make component
                </button>
              )}
              {sel.instances.length > 0 && (
                <button className="btn small" onClick={() => Promise.all(sel.instances.map((id) => run({ cmd: "explode", id }))).then(() => setSel(EMPTY))}>
                  Explode
                </button>
              )}
              <button className="btn small ghost" onClick={() => run({ cmd: "hide", ids: selIds(sel) }).then(() => setSel(EMPTY))}>
                Hide
              </button>
              <button className="btn small danger" onClick={deleteSelection}>
                Delete
              </button>
            </div>
          </div>
        )}
        <div className="section hint">
          <p>
            <b>R</b> rectangle, <b>L</b> line, <b>P</b> push/pull. Type a number and press Enter for exact sizes; <b>←</b> <b>→</b> <b>↑</b> lock to an axis.
          </p>
          <p>
            <b>M</b> with Option copies; then type <b>x5</b> for an array. Double-click a group to open it, Esc to close. Right-click for more.
          </p>
          <p>Middle-drag or hold Space to orbit, right-drag to pan, scroll to zoom.</p>
        </div>
      </aside>
    </div>
  );
}
