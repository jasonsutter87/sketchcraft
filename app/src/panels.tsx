// Side panels: Outliner (groups, components, tags), Library (assets and the
// model's components), Views (saved cameras), Entity Info, and the
// right-click menu.
import { useEffect, useState } from "react";
import { ask } from "./ask";
import { ASSETS, exec, type Reply, type Scene } from "./engine";
import { pickOpen, pickSave } from "./files";
import { Icons } from "./icons";
import { EMPTY, isEmpty, selIds, type Sel } from "./selection";
import { TEMPLATES } from "./templates";
import { formatLength, parseLength } from "./units";
import { prepareImage } from "./textures";
import { useRef } from "react";
import type { BuildSettings, Tool } from "./Viewport";
import type { Cam } from "./Viewport";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

export function Outliner({ scene, sel, setSel, run }: { scene: Scene; sel: Sel; setSel: (s: Sel) => void; run: Run }) {
  const [renaming, setRenaming] = useState<number | null>(null);
  const [name, setName] = useState("");
  const tags = Object.entries(scene.tags);
  const rows = [
    ...scene.groups.map((g) => ({ id: g.id, name: g.name, kind: "groups" as const, hidden: g.hidden, tag: g.tag, count: `${g.faces}` })),
    ...scene.instances.map((i) => ({ id: i.id, name: i.name, kind: "instances" as const, hidden: i.hidden, tag: i.tag, count: "" })),
  ];
  const commitRename = async () => {
    if (renaming != null && name.trim()) await run({ cmd: "rename", id: renaming, name: name.trim() });
    setRenaming(null);
  };
  return (
    <>
      {(scene.editing != null || scene.activeGroup != null) && (
        <div className="editing-banner">
          Editing {scene.editing != null ? "component" : "group"}
          <button onClick={() => run({ cmd: "close" })}>Done</button>
        </div>
      )}
      <ul className="layers">
        {rows.map((r) => (
          <li key={r.id} className={`${sel[r.kind].includes(r.id) ? "on" : ""} ${r.hidden ? "dim" : ""}`} onClick={(e) => setSel(e.shiftKey ? { ...sel, [r.kind]: [...sel[r.kind], r.id] } : { ...EMPTY, [r.kind]: [r.id] })} onDoubleClick={() => (setRenaming(r.id), setName(r.name))}>
            {r.kind === "groups" ? <Icons.cube /> : <Icons.component />}
            {renaming === r.id ? (
              <input className="rename" autoFocus value={name} onChange={(e) => setName(e.target.value)} onBlur={commitRename} onKeyDown={(e) => e.key === "Enter" && commitRename()} onClick={(e) => e.stopPropagation()} />
            ) : (
              <span className="name">{r.name}</span>
            )}
            {r.tag && <span className="chip-tag">{r.tag}</span>}
            <span className="count">{r.count}</span>
            <button
              className="eye"
              title={r.hidden ? "Show" : "Hide"}
              onClick={(e) => {
                e.stopPropagation();
                run({ cmd: r.hidden ? "unhide" : "hide", ids: [r.id] });
              }}
            >
              {r.hidden ? <Icons.eyeOff /> : <Icons.eye />}
            </button>
          </li>
        ))}
        {scene.loose > 0 && (
          <li className="muted-row">
            <Icons.layer />
            <span className="name">Loose geometry</span>
            <span className="count">{scene.loose}</span>
          </li>
        )}
        {!rows.length && !scene.loose && <li className="empty">Nothing yet. Press R and draw on the ground, or drop something from the Library.</li>}
      </ul>
      {tags.length > 0 && (
        <>
          <div className="section-title sub">Tags</div>
          <ul className="layers">
            {tags.map(([t, vis]) => (
              <li key={t} className={vis ? "" : "dim"}>
                <span className="tag-dot" />
                <span className="name">{t}</span>
                <button className="eye" onClick={() => run({ cmd: "tag_visible", tag: t, visible: !vis })}>
                  {vis ? <Icons.eye /> : <Icons.eyeOff />}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}

function LengthInput({ value, units, onChange, label }: { value: number; units: string; onChange: (m: number) => void; label: string }) {
  const [text, setText] = useState<string | null>(null);
  return (
    <label className="field">
      <span>{label}</span>
      <input
        value={text ?? formatLength(value, units)}
        onFocus={(e) => (setText(formatLength(value, units)), e.target.select())}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          const v = parseLength(text ?? "", units);
          if (!Number.isNaN(v) && v > 0) onChange(v);
          setText(null);
        }}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
    </label>
  );
}

const BUILD_TOOLS: { id: Tool; name: string; how: string }[] = [
  { id: "wall", name: "Wall", how: "Click points along the wall. Double-click or Enter to finish; click the first point to close a room." },
  { id: "stairs", name: "Stairs", how: "Click where the bottom step starts, then the direction it climbs." },
  { id: "roof", name: "Roof", how: "Click a flat top face: the top of your walls or a ceiling you drew." },
  { id: "column", name: "Column", how: "Click to drop a column." },
];

export function BuildTools({ tool, setTool, build, setBuild, units }: { tool: Tool; setTool: (t: Tool) => void; build: BuildSettings; setBuild: (b: BuildSettings) => void; units: string }) {
  const active = BUILD_TOOLS.find((b) => b.id === tool);
  const set = (k: keyof BuildSettings) => (v: unknown) => setBuild({ ...build, [k]: v });
  return (
    <>
      <div className="tiles build">
        {BUILD_TOOLS.map((b) => (
          <button key={b.id} className={`tile ${tool === b.id ? "on" : ""}`} onClick={() => setTool(tool === b.id ? "select" : b.id)} title={b.how}>
            <span className="tile-art build-art">{Icons[b.id as "wall"]()}</span>
            <span>{b.name}</span>
          </button>
        ))}
      </div>
      {active && (
        <div className="build-options">
          <p>{active.how}</p>
          {tool === "wall" && (
            <>
              <LengthInput label="Thickness" value={build.wallThickness} units={units} onChange={set("wallThickness")} />
              <LengthInput label="Height" value={build.wallHeight} units={units} onChange={set("wallHeight")} />
            </>
          )}
          {tool === "stairs" && (
            <>
              <LengthInput label="Total rise" value={build.stairRise} units={units} onChange={set("stairRise")} />
              <LengthInput label="Width" value={build.stairWidth} units={units} onChange={set("stairWidth")} />
            </>
          )}
          {tool === "roof" && (
            <>
              <label className="field">
                <span>Type</span>
                <select value={build.roofKind} onChange={(e) => set("roofKind")(e.target.value)}>
                  <option value="gable">Gable</option>
                  <option value="hip">Hip</option>
                  <option value="shed">Shed</option>
                </select>
              </label>
              <label className="field">
                <span>Pitch</span>
                <input
                  defaultValue={`${build.roofPitch}°`}
                  title="Degrees, or rise/run like 6/12"
                  onBlur={(e) => {
                    const t = e.target.value.trim();
                    const ratio = /^(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)$/.exec(t);
                    const deg = ratio ? (Math.atan(Number(ratio[1]) / Number(ratio[2])) * 180) / Math.PI : parseFloat(t);
                    if (deg > 0 && deg < 80) set("roofPitch")(Math.round(deg * 10) / 10);
                    e.target.value = `${build.roofPitch}°`;
                  }}
                />
              </label>
              <LengthInput label="Overhang" value={build.roofOverhang} units={units} onChange={set("roofOverhang")} />
            </>
          )}
          {tool === "column" && (
            <>
              <LengthInput label="Size" value={build.columnSize} units={units} onChange={set("columnSize")} />
              <LengthInput label="Height" value={build.columnHeight} units={units} onChange={set("columnHeight")} />
              <label className="field check">
                <input type="checkbox" checked={build.columnRound} onChange={(e) => set("columnRound")(e.target.checked)} />
                <span>Round</span>
              </label>
            </>
          )}
        </div>
      )}
    </>
  );
}

const SANDBOX_TOOLS: { id: Tool; name: string; how: string }[] = [
  { id: "smoove", name: "Smoove", how: "Click a terrain to raise it there (Shift-click digs). Radius and height below." },
  { id: "stamp", name: "Stamp", how: "Select a footprint face (a rectangle above the terrain), then click the terrain: a flat pad with sloped edges." },
  { id: "drape", name: "Drape", how: "Select lines (a path, a property line), then click the terrain: they're laid onto it." },
  { id: "flipedge", name: "Flip edge", how: "Click a diagonal between two triangles to swap it." },
];

/** SketchUp's Sandbox: terrain from contours or a grid, then sculpt it. */
export function Sandbox({ tool, setTool, build, setBuild, units, run, sel, onToast }: { tool: Tool; setTool: (t: Tool) => void; build: BuildSettings; setBuild: (b: BuildSettings) => void; units: string; run: Run; sel: Sel; onToast: (t: string, bad?: boolean) => void }) {
  const active = SANDBOX_TOOLS.find((b) => b.id === tool);
  const set = (k: keyof BuildSettings) => (v: unknown) => setBuild({ ...build, [k]: v });
  const contours = async () => {
    const ids = [...sel.edges, ...sel.groups];
    if (!ids.length) return onToast("Select the contour lines (or a group of them) first.", true);
    const r = await run({ cmd: "terrain_from_contours", ids, spacing: build.contourSpacing });
    if (r.ok) onToast("Terrain made from the contours. Smoove, Stamp and Drape work on it.");
  };
  const grid = async () => {
    const s = build.gridSize;
    const r = await run({ cmd: "terrain_grid", at: [-s / 2, -s / 2, 0], width: s, depth: s, spacing: build.gridSpacing });
    if (r.ok) onToast("A flat terrain grid. Use Smoove to shape it.");
  };
  return (
    <>
      <div className="tiles build">
        <button className="tile" onClick={contours} title="Select contour lines, then click: a terrain through them">
          <span className="tile-art build-art">{Icons.terrain()}</span>
          <span>From contours</span>
        </button>
        <button className="tile" onClick={grid} title="A flat grid to sculpt">
          <span className="tile-art build-art">{Icons.terrain()}</span>
          <span>From scratch</span>
        </button>
        {SANDBOX_TOOLS.map((b) => (
          <button key={b.id} className={`tile ${tool === b.id ? "on" : ""}`} onClick={() => setTool(tool === b.id ? "select" : b.id)} title={b.how}>
            <span className="tile-art build-art">{Icons[b.id as "smoove"]()}</span>
            <span>{b.name}</span>
          </button>
        ))}
      </div>
      <div className="build-options">
        {active && <p>{active.how}</p>}
        {tool === "smoove" && (
          <>
            <LengthInput label="Radius" value={build.smooveRadius} units={units} onChange={set("smooveRadius")} />
            <LengthInput label="Height" value={build.smooveHeight} units={units} onChange={set("smooveHeight")} />
          </>
        )}
        {tool === "stamp" && <LengthInput label="Slope out" value={build.stampOffset} units={units} onChange={set("stampOffset")} />}
        {!active && (
          <>
            <LengthInput label="Grid size" value={build.gridSize} units={units} onChange={set("gridSize")} />
            <LengthInput label="Grid spacing" value={build.gridSpacing} units={units} onChange={set("gridSpacing")} />
            <LengthInput label="Contour spacing" value={build.contourSpacing} units={units} onChange={set("contourSpacing")} />
          </>
        )}
      </div>
    </>
  );
}

// ----- Components (SketchUp's Components panel) --------------------------------

/** Thumbnails, fetched one at a time and kept for the session. */
const thumbs = new Map<string, string>();
const waiting = new Map<string, ((d: string) => void)[]>();
const queue: { key: string; req: Record<string, unknown> }[] = [];
let busy = false;
async function pump() {
  if (busy) return;
  busy = true;
  while (queue.length) {
    const { key, req } = queue.shift()!;
    const r = await exec({ cmd: "thumbnail", size: 160, ...req }).catch(() => null);
    const data = r?.ok ? (r.data as string) : "";
    thumbs.set(key, data);
    for (const f of waiting.get(key) ?? []) f(data);
    waiting.delete(key);
  }
  busy = false;
}

function Thumb({ k, req }: { k: string; req: Record<string, unknown> }) {
  const [src, setSrc] = useState(() => thumbs.get(k) ?? null);
  useEffect(() => {
    if (thumbs.has(k)) return setSrc(thumbs.get(k)!);
    let live = true;
    waiting.set(k, [...(waiting.get(k) ?? []), (d) => live && setSrc(d)]);
    if (!queue.some((q) => q.key === k)) queue.push({ key: k, req });
    void pump();
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [k]);
  return src ? <img className="thumb" src={src} alt="" draggable={false} /> : <span className="thumb loading" />;
}

interface LibEntry {
  path: string;
  name: string;
  collection: string;
}

export function Library({ scene, place, placing, run, onToast }: { scene: Scene; place: (k: string | null) => void; placing: string | null; run: Run; onToast: (t: string, bad?: boolean) => void }) {
  const [q, setQ] = useState("");
  const [lib, setLib] = useState<{ dir: string; entries: LibEntry[] } | null>(null);
  const reload = async () => {
    const r = await exec({ cmd: "library" });
    if (r.ok) setLib({ dir: r.dir as string, entries: r.entries as LibEntry[] });
  };
  useEffect(() => {
    void reload();
  }, []);
  const match = (name: string) => !q.trim() || name.toLowerCase().includes(q.trim().toLowerCase());
  // in the model: placed things first; parts of other components only when searching
  const used = scene.components.filter((c) => match(c.name) && (c.placed > 0 || (q.trim() !== "" && (c.inside ?? 0) > 0)));
  const unused = scene.components.filter((c) => c.placed === 0 && (c.inside ?? 0) === 0);
  const parts = scene.components.filter((c) => c.placed === 0 && (c.inside ?? 0) > 0).length;
  const collections = new Map<string, LibEntry[]>();
  for (const e of lib?.entries ?? []) if (match(e.name)) collections.set(e.collection, [...(collections.get(e.collection) ?? []), e]);
  const starter = ASSETS.filter((a) => match(a.name));

  const tile = (key: string, name: string, thumbKey: string, req: Record<string, unknown>, title: string, extra?: React.ReactNode) => (
    <div key={key} className={`tile comp ${placing === key ? "on" : ""}`} title={title}>
      <button className="tile-hit" onClick={() => place(placing === key ? null : key)}>
        <span className="tile-art thumb-art">
          <Thumb k={thumbKey} req={req} />
        </span>
        <span className="tile-name">{name}</span>
      </button>
      {extra}
    </div>
  );

  const saveToLibrary = async (id: number, name: string) => {
    const collection = await ask(`Save "${name}" to my library`, "", "Collection (a folder in your library), or leave blank.");
    if (collection === null) return;
    const r = await run({ cmd: "save_to_library", component: id, collection });
    if (r.ok) {
      onToast(`Saved to your library: ${String(r.path).split("/").slice(-2).join("/")}`);
      void reload();
    }
  };
  const addFiles = async () => {
    const path = await pickOpen();
    if (!path) return;
    const r = await run({ cmd: "library_add", paths: [path] });
    if (r.ok) {
      onToast("Added to your library");
      void reload();
    }
  };

  return (
    <>
      <input className="lib-search" placeholder="Search components" value={q} onChange={(e) => setQ(e.target.value)} />

      <div className="section-title sub row-between">
        <span>In this model</span>
        {unused.length > 0 && (
          <button className="link" onClick={() => run({ cmd: "purge" }).then((r) => r.ok && onToast(`Removed ${r.removed} unused component${r.removed === 1 ? "" : "s"}`))} title="Remove components nothing places">
            Purge {unused.length} unused
          </button>
        )}
      </div>
      {used.length ? (
        <div className="tiles">
          {used.map((c) =>
            tile(
              `component:${c.id}`,
              c.name,
              `c${c.id}:${c.placed}:${scene.faces.length}`,
              { component: c.id },
              `${c.name}: ${c.placed} placed${c.inside ? `, ${c.inside} inside other components` : ""}. Click, then click in the model to place another.`,
              <>
                <span className="tile-count">{c.placed}×</span>
                <button className="tile-act" title="Save to my library" onClick={() => saveToLibrary(c.id, c.name)}>
                  <Icons.plus />
                </button>
              </>
            )
          )}
        </div>
      ) : (
        <p className="hint-text">{q ? "Nothing here matches." : "Components you make (G) or place show up here."}</p>
      )}
      {parts > 0 && !q && <p className="hint-text">{parts} more are parts inside these. Search to find them.</p>}

      <div className="section-title sub row-between">
        <span>My library</span>
        <span className="row gap-s">
          <button className="link" onClick={addFiles} title="Copy a .skp, .fbx, .obj or .sketchcraft file into your library">
            Add file…
          </button>
          <button className="link" onClick={() => run({ cmd: "reveal", path: lib?.dir })} title={lib?.dir}>
            Open folder
          </button>
          <button className="link" onClick={reload} title="Look for new files">
            ↻
          </button>
        </span>
      </div>
      {collections.size === 0 && (
        <p className="hint-text">
          {q ? "Nothing in your library matches." : "Your own components: save one from the model (+ on its tile), or drop .skp, .fbx, .obj or .sketchcraft files into the library folder. Subfolders become collections."}
        </p>
      )}
      {[...collections.entries()].map(([col, items]) => (
        <details key={col || "_top"} className="collection" open>
          <summary>{col || "Unsorted"}</summary>
          <div className="tiles">{items.map((e) => tile(`file:${e.path}`, e.name, `f:${e.path}`, { path: e.path }, `${e.name}. Click, then click in the model to place it.`))}</div>
        </details>
      ))}

      {starter.length > 0 && (
        <details className="collection" open={!lib?.entries.length}>
          <summary>Starter</summary>
          <div className="tiles">{starter.map((a) => tile(a.key, a.name, `a:${a.key}`, { asset: a.key }, `Place ${a.name}: click in the model`))}</div>
        </details>
      )}
      <p className="hint-text">Components share their geometry: edit one (double-click it) and every copy changes.</p>
    </>
  );
}

export function Templates({ onTemplate }: { onTemplate: (lines: string[]) => void }) {
  return <TemplateTiles onTemplate={onTemplate} />;
}

function TemplateTiles({ onTemplate }: { onTemplate: (lines: string[]) => void }) {
  return (
    <div className="tiles">
      {TEMPLATES.map((t) => (
        <button key={t.name} className="tile" onClick={() => onTemplate(t.lines)} title={t.about}>
          <span className="tile-art" style={{ background: t.tint }}>
            <Icons.cube />
          </span>
          <span>{t.name}</span>
        </button>
      ))}
    </div>
  );
}

export function Views({ scene, run, camera, goTo, onMatchPhoto }: { scene: Scene; run: Run; camera: () => Cam; goTo: (c: Cam & { photo?: string }) => void; onMatchPhoto: (f: File) => void }) {
  const [name, setName] = useState("");
  const photoInput = useRef<HTMLInputElement>(null);
  const save = async () => {
    const n = name.trim() || `View ${scene.views.length + 1}`;
    await run({ cmd: "save_view", name: n, ...camera() });
    setName("");
  };
  return (
    <>
      <div className="row">
        <input className="num wide-input" placeholder="Name this view" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
        <button className="btn small" onClick={save}>
          <Icons.plus /> Save
        </button>
      </div>
      <div className="row" style={{ marginTop: 8 }}>
        <button className="btn small ghost" onClick={() => photoInput.current?.click()} title="Line a photo up with the axes and model over it">
          <Icons.camera /> Match photo…
        </button>
        <input ref={photoInput} type="file" accept="image/*" hidden onChange={(e) => e.target.files?.[0] && (onMatchPhoto(e.target.files[0]), (e.target.value = ""))} />
      </div>
      <ul className="layers views-list">
        {scene.views.map((v) => (
          <li key={v.name} onClick={() => goTo(v)}>
            {v.photo ? <img className="view-thumb" src={v.photo} alt="" /> : <Icons.camera />}
            <span className="name">{v.name}</span>
            <button
              className="eye"
              title="Delete view"
              onClick={(e) => {
                e.stopPropagation();
                run({ cmd: "delete_view", name: v.name });
              }}
            >
              <Icons.trash />
            </button>
          </li>
        ))}
        {!scene.views.length && <li className="empty">Save camera angles here and click to fly back. Renders can use them by name.</li>}
      </ul>
    </>
  );
}

export function EntityInfo({ scene, sel, run }: { scene: Scene; sel: Sel; run: Run }) {
  const [tag, setTag] = useState("");
  const faces = scene.faces.filter((f) => sel.faces.includes(f.id));
  const edges = scene.edgeList.filter((e) => sel.edges.includes(e.id));
  const groups = scene.groups.filter((g) => sel.groups.includes(g.id));
  const insts = scene.instances.filter((i) => sel.instances.includes(i.id));
  const len = edges.reduce((a, e) => a + Math.hypot(e.b[0] - e.a[0], e.b[1] - e.a[1], e.b[2] - e.a[2]), 0);
  const title =
    groups.length + insts.length + faces.length + edges.length === 1
      ? groups[0]?.name ?? insts[0]?.name ?? (faces[0] ? `Face ${faces[0].id}` : `Edge ${edges[0].id}`)
      : `${selIds(sel).length} selected`;
  const taggable = [...sel.groups, ...sel.instances];
  // one group or component: is it a solid, and its volume (SketchUp's "Solid Group")
  const [solid, setSolid] = useState<{ id: number; volume: number | null } | null>(null);
  const one = taggable.length === 1 && selIds(sel).length === 1 ? taggable[0] : null;
  useEffect(() => {
    if (one == null) return setSolid(null);
    let live = true;
    run({ cmd: "volume", id: one }).then((r) => live && setSolid({ id: one, volume: r.ok && r.solid ? (r.volume as number) : null }));
    return () => {
      live = false;
    };
    // recheck whenever the model changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [one, scene]);
  return (
    <div className="section">
      <div className="section-title">{title}</div>
      <div className="kv">
        {faces.length > 0 && (
          <>
            <span>Area</span>
            <b>{formatArea(faces.reduce((a, f) => a + f.area, 0), scene.units)}</b>
          </>
        )}
        {edges.length > 0 && (
          <>
            <span>Length</span>
            <b>{formatLength(len, scene.units)}</b>
          </>
        )}
        {solid && solid.id === one && (
          <>
            <span>{solid.volume != null ? "Solid" : "Not solid"}</span>
            <b>{solid.volume != null ? formatVolume(solid.volume, scene.units) : "has open edges"}</b>
          </>
        )}
        {insts.length === 1 && (
          <>
            <span>Component</span>
            <b>{scene.components.find((c) => c.id === insts[0].component)?.name}</b>
          </>
        )}
        {(groups[0]?.tag ?? insts[0]?.tag) && (
          <>
            <span>Tag</span>
            <b>{groups[0]?.tag ?? insts[0]?.tag}</b>
          </>
        )}
      </div>
      {taggable.length === 1 && selIds(sel).length === 1 && (
        <label className="field">
          <span>IFC type</span>
          <select value={scene.classes?.[String(taggable[0])] ?? ""} onChange={(e) => run({ cmd: "classify", ids: taggable, class: e.target.value })}>
            <option value="">{`(guess: ${guessIfc(groups[0]?.name ?? insts[0]?.name ?? "")})`}</option>
            {IFC_TYPES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
      )}
      {insts.length === 1 && <AttrEditor key={insts[0].id} id={insts[0].id} attrs={insts[0].attrs ?? {}} run={run} />}
      {taggable.length > 0 && (
        <div className="row gap">
          <input className="num wide-input" placeholder="Tag (e.g. Trees)" value={tag} onChange={(e) => setTag(e.target.value)} list="tag-list" />
          <datalist id="tag-list">
            {Object.keys(scene.tags).map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
          <button className="btn small" onClick={() => run({ cmd: "tag", ids: taggable, tag: tag.trim() || null })}>
            Set
          </button>
        </div>
      )}
    </div>
  );
}

export interface MenuItem {
  label: string;
  key?: string;
  danger?: boolean;
  run: () => void;
}

export function contextItems(scene: Scene, sel: Sel, run: Run, setSel: (s: Sel) => void, fitSel: () => void, section?: { origin: number[]; normal: number[] } | null): MenuItem[] {
  const ids = selIds(sel);
  const items: MenuItem[] = [];
  if (isEmpty(sel)) {
    items.push({ label: "Unhide all", run: () => run({ cmd: "unhide" }) });
    items.push({ label: "Clear guides", run: () => run({ cmd: "clear_guides" }) });
    items.push({ label: "Soften flat seams", run: () => run({ cmd: "soften", coplanar: true }) });
    if (scene.editing != null || scene.activeGroup != null) items.push({ label: "Close editing", key: "Esc", run: () => run({ cmd: "close" }) });
    return items;
  }
  const one = ids.length === 1;
  if (one && (sel.groups.length || sel.instances.length)) items.push({ label: `Edit ${sel.groups.length ? "group" : "component"}`, run: () => (run({ cmd: "edit", id: ids[0] }), setSel(EMPTY)) });
  if (sel.faces.length) {
    items.push({ label: "Make group", run: () => run({ cmd: "group", ids: sel.faces }).then(() => setSel(EMPTY)) });
  }
  if (sel.faces.length || sel.groups.length) {
    items.push({
      label: "Make component…",
      key: "G",
      run: async () => {
        const name = await ask("Component name", "Component");
        if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
      },
    });
  }
  // edge tools: bevel / round outside edges of solids, divide an edge
  if (sel.edges.length) {
    const units = scene.units || "m";
    const askSize = async (title: string) => {
      const t = await ask(title, units === "ft" || units === "in" ? `2"` : "0.05", "A length, in any units (2\", 5cm, 0.1).");
      if (t == null) return null;
      const v = parseLength(t, units);
      return Number.isFinite(v) && v > 0 ? v : null;
    };
    items.push({
      label: "Bevel (chamfer)…",
      run: async () => {
        const size = await askSize("Bevel these edges by");
        if (size) run({ cmd: "bevel", edges: sel.edges, size, segments: 1 }).then((r) => r.ok && setSel(EMPTY));
      },
    });
    items.push({
      label: "Round…",
      run: async () => {
        const size = await askSize("Round these edges with a radius of");
        if (size) run({ cmd: "bevel", edges: sel.edges, size, segments: 8 }).then((r) => r.ok && setSel(EMPTY));
      },
    });
    if (sel.edges.length === 1)
      items.push({
        label: "Divide…",
        run: async () => {
          const t = await ask("Divide this edge into how many parts?", "4");
          const n = t ? parseInt(t, 10) : NaN;
          if (n >= 2) run({ cmd: "divide", edge: sel.edges[0], count: n });
        },
      });
  }
  // slice a solid along the section cut you've placed
  if (section && sel.groups.length + sel.instances.length === 1 && !sel.faces.length) {
    items.push({ label: "Slice along the section cut", run: () => run({ cmd: "slice", id: [...sel.groups, ...sel.instances][0], origin: section.origin, normal: section.normal }).then((r) => r.ok && setSel(EMPTY)) });
  }
  // Intersect Faces and Flip Along (SketchUp's Edit / context menu)
  const shapes = [...sel.faces, ...sel.groups, ...sel.instances];
  if (shapes.length) {
    items.push({ label: "Intersect Faces: with model", run: () => run({ cmd: "intersect", ids: shapes, with: "model" }) });
    if (shapes.length > 1) items.push({ label: "Intersect Faces: with selection", run: () => run({ cmd: "intersect", ids: shapes, with: "selection" }) });
    for (const [axis, name] of [["red", "Red"], ["green", "Green"], ["blue", "Blue"]]) items.push({ label: `Flip Along: ${name}`, run: () => run({ cmd: "flip", ids: shapes, axis }) });
  }
  // Solid Tools: in pick order (for Subtract and Trim the first one cuts the second)
  const solids = [...sel.groups, ...sel.instances];
  if (solids.length >= 2 && !sel.faces.length && !sel.edges.length) {
    const tool = (label: string, op: string, two = false) =>
      (!two || solids.length === 2) && items.push({ label: `Solid: ${label}`, run: () => run({ cmd: "solid", op, ids: solids }).then((r) => r.ok && setSel({ ...EMPTY, groups: (r.groups as number[]) ?? [] })) });
    tool("Outer Shell", "outer_shell");
    tool("Union", "union");
    tool("Intersect", "intersect");
    tool("Subtract (first cuts second)", "subtract", true);
    tool("Trim (first cuts second, kept)", "trim", true);
    tool("Split", "split", true);
  }
  if (sel.instances.length) items.push({ label: "Explode", run: () => Promise.all(sel.instances.map((id) => run({ cmd: "explode", id }))).then(() => setSel(EMPTY)) });
  if (sel.faces.length || sel.groups.length) items.push({ label: "Reverse faces", run: () => run({ cmd: "reverse", ids: [...sel.faces, ...sel.groups] }) });
  if (sel.edges.length) items.push({ label: "Soften edges", run: () => run({ cmd: "soften", ids: sel.edges }) });
  items.push({ label: "Zoom to selection", key: "⇧Z", run: fitSel });
  items.push({ label: "Hide", run: () => run({ cmd: "hide", ids }).then(() => setSel(EMPTY)) });
  items.push({ label: "Delete", key: "⌫", danger: true, run: () => run({ cmd: "delete", ids }).then(() => setSel(EMPTY)) });
  return items;
}

export function ContextMenu({ x, y, items, close }: { x: number; y: number; items: MenuItem[]; close: () => void }) {
  return (
    <div className="menu-scrim" onPointerDown={close} onContextMenu={(e) => (e.preventDefault(), close())}>
      <div className="menu" style={{ left: x, top: y }} onPointerDown={(e) => e.stopPropagation()}>
        {items.map((it) => (
          <button
            key={it.label}
            className={it.danger ? "danger" : ""}
            onClick={() => {
              close();
              it.run();
            }}
          >
            <span>{it.label}</span>
            {it.key && <kbd>{it.key}</kbd>}
          </button>
        ))}
      </div>
    </div>
  );
}

export function formatVolume(m3: number, units: string): string {
  if (units === "ft" || units === "in") return `${(m3 / 0.028316846592).toFixed(2)} ft³`;
  if (units === "mm" || units === "cm") return `${(m3 * 1e6).toFixed(0)} cm³`;
  return `${m3.toFixed(3)} m³`;
}

export function formatArea(m2: number, units: string): string {
  if (units === "ft" || units === "in") return `${(m2 / 0.09290304).toFixed(1)} ft²`;
  if (units === "mm" || units === "cm") return `${(m2 * 10000).toFixed(0)} cm²`;
  return `${m2.toFixed(2)} m²`;
}


/** Image materials: add one from a photo, paint with it, size it, project it. */
export function Textures({
  scene,
  paintMaterial,
  choose,
  run,
  camera,
  sel,
  onToast,
}: {
  scene: Scene;
  paintMaterial: string | null;
  choose: (name: string | null) => void;
  run: Run;
  camera: () => Cam;
  sel: Sel;
  onToast: (t: string, bad?: boolean) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const textures = scene.materials.filter((m) => m.texture);
  const active = textures.find((m) => m.name === paintMaterial);
  const add = async (file: File) => {
    try {
      const img = await prepareImage(file);
      let name = file.name.replace(/\.[^.]+$/, "") || "Image";
      while (scene.materials.some((m) => m.name === name)) name += " 2";
      const width = 1;
      const r = await run({ cmd: "texture", name, data: img.data, width, height: width * img.aspect, color: img.color });
      if (r.ok) {
        choose(name);
        onToast(`Added ${name}. Click faces with Paint, or set its size below.`);
      }
    } catch (e) {
      onToast(String(e), true);
    }
  };
  const project = async () => {
    if (!active) return;
    const c = camera();
    const dir = [c.target[0] - c.eye[0], c.target[1] - c.eye[1], c.target[2] - c.eye[2]];
    await run({ cmd: "texture", name: active.name, project: dir, origin: c.target });
    onToast("Projected from this view: paint a whole group and the image runs across it.");
  };
  const ids = [...sel.faces, ...sel.groups];
  return (
    <div className="section">
      <div className="section-title">Textures</div>
      <div className="swatches">
        {textures.map((m) => (
          <button
            key={m.name}
            className={`swatch texture ${paintMaterial === m.name ? "on" : ""}`}
            style={{ backgroundImage: `url(${m.texture!.data})` }}
            title={m.name}
            onClick={() => {
              choose(m.name);
              if (ids.length) run({ cmd: "paint", ids, material: m.name });
            }}
          />
        ))}
        <button className="swatch add" title="Add an image (photo of brick, wood, a facade...)" onClick={() => input.current?.click()}>
          <Icons.plus />
        </button>
        <input ref={input} type="file" accept="image/*" hidden onChange={(e) => e.target.files?.[0] && add(e.target.files[0])} />
      </div>
      {active?.texture && (
        <div className="build-options">
          <p>
            <b>{active.name}</b>: Paint (B) clicks apply it. {active.texture.project ? "Projected: it runs straight across every face it's painted on." : "Laid along each face it's painted on."}
          </p>
          <LengthInput
            label="Tile width"
            value={active.texture.width}
            units={scene.units || "m"}
            onChange={(w) => run({ cmd: "texture", name: active.name, width: w })}
          />
          <div className="row gap">
            <button className="btn small" onClick={project}>
              Project from this view
            </button>
            {active.texture.project && (
              <button className="btn small ghost" onClick={() => run({ cmd: "texture", name: active.name, project: null })}>
                Lay on faces
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}


/** Every shortcut, laid out like SketchUp's defaults (Mac keys; Windows uses Ctrl for ⌘ and Option). */
export const SHORTCUTS: [string, [string, string][]][] = [
  [
    "Tools",
    [
      ["Space", "Select"],
      ["L", "Line"],
      ["A", "Arc"],
      ["R", "Rectangle"],
      ["C", "Circle"],
      ["N", "Polygon"],
      ["P", "Push/Pull"],
      ["F", "Offset"],
      ["J", "Follow Me"],
      ["M", "Move"],
      ["Q", "Rotate"],
      ["S", "Scale"],
      ["T", "Tape Measure"],
      ["D", "Dimension"],
      ["W", "Text"],
      ["B", "Paint Bucket"],
      ["E", "Eraser"],
      ["X", "Section Plane"],
    ],
  ],
  [
    "Camera",
    [
      ["O", "Orbit (or middle-drag)"],
      ["H", "Pan (or Shift + middle-drag)"],
      ["Z", "Zoom"],
      ["⇧Z", "Zoom Extents"],
      ["⌘1 – ⌘6", "Iso, Top, Front, Right, Back, Left"],
      ["K", "Back Edges"],
    ],
  ],
  [
    "Edit",
    [
      ["⌘Z", "Undo"],
      ["⇧⌘Z  /  Ctrl+Y", "Redo"],
      ["⌘X  ⌘C  ⌘V", "Cut, Copy, Paste"],
      ["⌘A", "Select All"],
      ["⌘T", "Select None"],
      ["⌫", "Delete"],
      ["G", "Make Component"],
      ["⌘N  ⌘O  ⌘S  ⇧⌘S", "New, Open, Save, Save As"],
      ["⌘I", "Import"],
    ],
  ],
  [
    "While using a tool",
    [
      ["Type + Enter", "Exact length, size, angle, or 24s for sides"],
      ["→  ←  ↑", "Lock to the red, green, blue axis"],
      ["Hold ⇧", "Lock the direction you're inferring"],
      ["Esc", "Cancel, or close the group you're in"],
      ["Option / Ctrl", "Move and Rotate: copy"],
      ["x5 after a copy", "Make 5 copies in a row"],
      ["Double-click", "Push/Pull: repeat the last distance; Select: open a group"],
    ],
  ],
  [
    "Selecting",
    [
      ["⇧ click", "Add or remove"],
      ["Option / Ctrl click", "Add"],
      ["⇧ Option click", "Remove"],
      ["⌘ / Option click", "Paint Bucket: sample a material"],
      ["⇧ click", "Paint Bucket: paint every matching face"],
      ["Option / Ctrl click", "Eraser: soften (⇧ too: unsoften)"],
      ["⇧ click", "Eraser: hide"],
    ],
  ],
];

export function ShortcutSheet({ onClose }: { onClose: () => void }) {
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Keyboard shortcuts">
        <div className="sheet-head">
          <b>Keyboard shortcuts</b>
          <span className="muted">SketchUp's defaults. On Windows, Ctrl stands in for ⌘.</span>
          <button className="btn small ghost" onClick={onClose}>
            Close
          </button>
        </div>
        <div className="sheet-cols">
          {SHORTCUTS.map(([title, rows]) => (
            <div key={title} className="sheet-sec">
              <div className="section-title">{title}</div>
              {rows.map(([k, what]) => (
                <div key={k + what} className="sheet-row">
                  <kbd>{k}</kbd>
                  <span>{what}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/** LayOut-lite: pick views (and scales) for a PDF set of drawing sheets. */
export function SheetsDialog({ scene, onMake, onClose }: { scene: Scene; onMake: (spec: { sheet: string; project: string; blueprint: boolean; pages: { title: string; viewports: { view: string; scale?: string; label?: string; cut?: number }[] }[] }) => void; onClose: () => void }) {
  const standard = ["iso", "top", "front", "right", "back", "left"];
  // floor plans, one per floor the engine finds
  const [levels, setLevels] = useState<number[]>([]);
  useEffect(() => {
    exec({ cmd: "levels" }).then((r) => {
      if (!r.ok) return;
      const lv = r.levels as number[];
      setLevels(lv);
      setPicked((p) => ({ ...p, ...Object.fromEntries(lv.map((_, i) => [`plan:${i}`, { on: true, scale: "1:100" }])) }));
    });
  }, []);
  const [blueprint, setBlueprint] = useState(false);
  const all = [...levels.map((_, i) => `plan:${i}`), ...scene.views.map((v) => v.name), ...standard];
  const [sheet, setSheet] = useState("a3");
  const [project, setProject] = useState("Project");
  const [perPage, setPerPage] = useState(false);
  const [picked, setPicked] = useState<Record<string, { on: boolean; scale: string }>>(() =>
    Object.fromEntries(all.map((n, i) => [n, { on: scene.views.length ? i < scene.views.length && i < 4 : ["top", "front", "right", "iso"].includes(n), scale: ["top", "front", "right", "back", "left"].includes(n) ? "1:100" : "" }]))
  );
  const label = (n: string) => (n.startsWith("plan:") ? `${["Main floor", "Upper floor"][Number(n.slice(5))] ?? `Floor ${Number(n.slice(5)) + 1}`} plan` : standard.includes(n) ? { iso: "Iso view", top: "Plan (top)", front: "Front elevation", right: "Right elevation", back: "Back elevation", left: "Left elevation" }[n] ?? n : n);
  const make = () => {
    const chosen = all
      .filter((n) => picked[n]?.on)
      .map((n) => (n.startsWith("plan:") ? { view: "top", cut: levels[Number(n.slice(5))] + 1.22, scale: picked[n].scale.trim() || undefined, label: label(n) } : { view: n, scale: picked[n].scale.trim() || undefined, label: label(n) }));
    if (!chosen.length) return;
    const pages = [];
    const per = perPage ? 1 : 4;
    for (let i = 0; i < chosen.length; i += per) {
      const vs = chosen.slice(i, i + per);
      pages.push({ title: perPage ? vs[0].label : `Sheet ${pages.length + 1}`, viewports: vs });
    }
    onMake({ sheet, project: project.trim() || "Project", pages, blueprint });
  };
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" style={{ width: "min(520px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Drawing sheets">
        <div className="sheet-head">
          <b>Drawing sheets</b>
          <span className="muted">Views on paper, to scale, with a title block.</span>
        </div>
        <label className="field">
          <span>Project</span>
          <input value={project} onChange={(e) => setProject(e.target.value)} />
        </label>
        <label className="field">
          <span>Sheet</span>
          <select value={sheet} onChange={(e) => setSheet(e.target.value)}>
            <option value="a4">A4</option>
            <option value="a3">A3</option>
            <option value="a2">A2</option>
            <option value="letter">Letter</option>
            <option value="tabloid">Tabloid (11×17)</option>
          </select>
        </label>
        <label className="field check">
          <input type="checkbox" checked={perPage} onChange={(e) => setPerPage(e.target.checked)} />
          <span>One view per sheet</span>
        </label>
        <label className="field check">
          <input type="checkbox" checked={blueprint} onChange={(e) => setBlueprint(e.target.checked)} />
          <span>Blueprint (white lines on blue)</span>
        </label>
        <div className="section-title" style={{ marginTop: 10 }}>
          Views <span className="muted">(scale like 1:100 for a true-scale drawing; blank to fit)</span>
        </div>
        {all.map((n) => (
          <div key={n} className="sheet-row">
            <input type="checkbox" checked={!!picked[n]?.on} onChange={(e) => setPicked((p) => ({ ...p, [n]: { ...p[n], on: e.target.checked } }))} />
            <span>{label(n)}</span>
            <input className="num" style={{ width: 70 }} placeholder="fit" value={picked[n]?.scale ?? ""} onChange={(e) => setPicked((p) => ({ ...p, [n]: { ...p[n], scale: e.target.value } }))} />
          </div>
        ))}
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="btn small" onClick={make}>
            Make PDF…
          </button>
          <button className="btn small ghost" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}


/** Dynamic Component attributes of one placement: values or =formulas. */
function AttrEditor({ id, attrs, run }: { id: number; attrs: Record<string, string>; run: Run }) {
  const [rows, setRows] = useState<[string, string][]>(() => [...Object.entries(attrs), ["", ""]]);
  const [err, setErr] = useState<string | null>(null);
  const [vals, setVals] = useState<Record<string, number>>({});
  const apply = async () => {
    const next: Record<string, string> = {};
    for (const [k, v] of rows) if (k.trim()) next[k.trim()] = v;
    // names that were removed
    for (const k of Object.keys(attrs)) if (!(k in next)) next[k] = "";
    const r = await run({ cmd: "set_attrs", id, attrs: next });
    if (r.ok) {
      setErr(null);
      setVals((r.values as Record<string, number>) ?? {});
      setRows([...Object.entries(r.attrs as Record<string, string>), ["", ""]]);
    } else setErr(r.error ?? "That didn't work");
  };
  return (
    <div className="attrs">
      <div className="section-title sub">Attributes</div>
      {rows.map(([k, v], i) => (
        <div key={i} className="attr-row">
          <input placeholder="name" value={k} onChange={(e) => setRows((r) => r.map((x, j) => (j === i ? [e.target.value, x[1]] : x)))} />
          <input placeholder="value or =formula" value={v} onChange={(e) => setRows((r) => r.map((x, j) => (j === i ? [x[0], e.target.value] : x)))} onKeyDown={(e) => e.key === "Enter" && apply()} title={k && vals[k.toLowerCase()] != null ? `= ${vals[k.toLowerCase()]}` : undefined} />
        </div>
      ))}
      {err && <p className="attr-err">{err}</p>}
      <div className="row gap">
        <button className="btn small" onClick={apply}>
          Apply
        </button>
        <button className="btn small ghost" onClick={() => setRows((r) => [...r, ["", ""]])}>
          + Row
        </button>
      </div>
      <p className="muted small">lenx / leny / lenz (m), copies, spacing, material, hidden. Formulas: =lenx*2, =if(a&gt;1, 2, 3), =parent!lenx</p>
    </div>
  );
}


const IFC_TYPES = ["IfcWall", "IfcSlab", "IfcRoof", "IfcDoor", "IfcWindow", "IfcColumn", "IfcBeam", "IfcStair", "IfcRailing", "IfcCovering", "IfcFurniture", "IfcPlate", "IfcMember", "IfcBuildingElementProxy"];

/** The same name-based guess the IFC export makes. */
function guessIfc(name: string): string {
  const n = name.toLowerCase();
  const has = (w: string) => n.includes(w);
  if (has("wall")) return "IfcWall";
  if (has("roof")) return "IfcRoof";
  if (has("stair") || has("step")) return "IfcStair";
  if (has("column") || has("post") || has("pillar")) return "IfcColumn";
  if (has("door") || has("garage")) return "IfcDoor";
  if (has("window")) return "IfcWindow";
  if (has("slab") || has("floor") || has("foundation") || has("deck")) return "IfcSlab";
  if (has("beam")) return "IfcBeam";
  if (has("rail") || has("fence")) return "IfcRailing";
  return "IfcBuildingElementProxy";
}

/** SketchUp's Add Location: real terrain (and map) around a place. */
export function LocationDialog({ onAdd, onClose, current }: { onAdd: (p: { lat: number; lon: number; size: number; imagery: boolean }) => void; onClose: () => void; current?: { lat: number; lon: number } | null }) {
  const [lat, setLat] = useState(current ? String(current.lat) : "");
  const [lon, setLon] = useState(current ? String(current.lon) : "");
  const [size, setSize] = useState("300");
  const [imagery, setImagery] = useState(true);
  const paste = (v: string) => {
    // "37.8199, -122.4783" pasted into either box
    const m = /(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)/.exec(v);
    if (m) {
      setLat(m[1]);
      setLon(m[2]);
      return true;
    }
    return false;
  };
  const ok = () => {
    const [a, o, s] = [parseFloat(lat), parseFloat(lon), parseFloat(size)];
    if (Number.isFinite(a) && Number.isFinite(o) && s > 0) onAdd({ lat: a, lon: o, size: s, imagery });
  };
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" style={{ width: "min(420px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Add location">
        <div className="sheet-head">
          <b>Add location</b>
          <span className="muted">Ground and map around a place.</span>
        </div>
        <label className="field">
          <span>Latitude</span>
          <input value={lat} placeholder="37.8199" onChange={(e) => paste(e.target.value) || setLat(e.target.value)} />
        </label>
        <label className="field">
          <span>Longitude</span>
          <input value={lon} placeholder="-122.4783" onChange={(e) => paste(e.target.value) || setLon(e.target.value)} />
        </label>
        <label className="field">
          <span>Area (m across)</span>
          <input value={size} onChange={(e) => setSize(e.target.value)} />
        </label>
        <label className="field check">
          <input type="checkbox" checked={imagery} onChange={(e) => setImagery(e.target.checked)} />
          <span>Map imagery (OpenStreetMap)</span>
        </label>
        <p className="muted small">Paste "lat, lon" from a map. The model's origin sits on the ground there, north up (+Y). Elevation: AWS Terrain Tiles; map © OpenStreetMap contributors.</p>
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="btn small" onClick={ok}>
            Add
          </button>
          <button className="btn small ghost" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}


/** Photoreal render of the current view: quality, sun, then save. */
export function RenderDialog({ camera, aspect, name, onClose, onToast }: { camera: () => Cam; aspect: number; name: string; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const QUALITY = [
    { id: "preview", label: "Preview", samples: 16, long: 960, about: "a few seconds" },
    { id: "good", label: "Good", samples: 64, long: 1600, about: "under a minute" },
    { id: "final", label: "Final", samples: 256, long: 2400, about: "a few minutes" },
  ];
  const [q, setQ] = useState("preview");
  const [autoSun, setAutoSun] = useState(true);
  const [az, setAz] = useState(200);
  const [alt, setAlt] = useState(35);
  const [busy, setBusy] = useState(false);
  const [img, setImg] = useState<string | null>(null);
  const [secs, setSecs] = useState<number | null>(null);
  const cam = useRef(camera());
  const go = async () => {
    const preset = QUALITY.find((x) => x.id === q)!;
    const w = aspect >= 1 ? preset.long : Math.round(preset.long * aspect);
    const h = aspect >= 1 ? Math.round(preset.long / aspect) : preset.long;
    setBusy(true);
    const r = await exec({ cmd: "photoreal", width: w, height: h, samples: preset.samples, camera: cam.current, ...(autoSun ? {} : { sun_azimuth: az }), sun_altitude: alt });
    setBusy(false);
    if (r.ok) {
      setImg(r.data as string);
      setSecs(r.seconds as number);
    } else onToast(r.error ?? "The render didn't work", true);
  };
  const save = async () => {
    if (!img) return;
    const path = await pickSave(`${name} render.png`, "png", "PNG image");
    if (!path) return;
    const r = await exec({ cmd: "save_image", path, data: img });
    if (r.ok) onToast(`Saved ${path.split("/").pop()}`);
  };
  const compass = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][Math.round(((az % 360) + 360) % 360 / 45) % 8];
  return (
    <div className="sheet-backdrop" onClick={() => !busy && onClose()}>
      <div className="sheet render-dlg" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Render">
        <div className="sheet-head">
          <b>Render</b>
          <span className="muted">Photoreal: sunlight, sky, soft shadows, reflections.</span>
        </div>
        <div className="render-view">{img ? <img src={img} alt="Render" /> : <div className="render-empty">{busy ? "Rendering…" : "Pick a quality and press Render."}</div>}</div>
        <div className="render-controls">
          <div className="seg">
            {QUALITY.map((x) => (
              <button key={x.id} className={q === x.id ? "on" : ""} onClick={() => setQ(x.id)} title={x.about} disabled={busy}>
                {x.label}
              </button>
            ))}
          </div>
          <label className="field check">
            <input type="checkbox" checked={autoSun} onChange={(e) => setAutoSun(e.target.checked)} />
            <span>Sun placed for this view</span>
          </label>
          {!autoSun && (
            <label className="field">
              <span>Sun from {compass}</span>
              <input type="range" min={0} max={359} value={az} onChange={(e) => setAz(Number(e.target.value))} />
            </label>
          )}
          <label className="field">
            <span>Sun height {alt}°</span>
            <input type="range" min={5} max={85} value={alt} onChange={(e) => setAlt(Number(e.target.value))} />
          </label>
          <div className="row gap">
            <button className="btn small" onClick={go} disabled={busy}>
              {busy ? "Rendering…" : img ? "Render again" : "Render"}
            </button>
            <button className="btn small ghost" onClick={save} disabled={!img || busy}>
              Save PNG…
            </button>
            <button className="btn small ghost" onClick={onClose} disabled={busy}>
              Close
            </button>
            {secs != null && !busy && <span className="muted small">{secs.toFixed(1)} s</span>}
          </div>
        </div>
      </div>
    </div>
  );
}
