// Side panels: Outliner (groups, components, tags), Library (assets and the
// model's components), Views (saved cameras), Entity Info, and the
// right-click menu.
import { useEffect, useState } from "react";
import { ASSETS, type Reply, type Scene } from "./engine";
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

const ASSET_TINT: Record<string, string> = {
  door: "#F3E3D6", window: "#DDEBF7", garage_door: "#ECEDEF", tree: "#DDEFD8", palm: "#E7F1D9",
  car: "#F8DEDE", streetlight: "#E6E7EC", bench: "#F1E6D8", hydrant: "#FBF0C9",
};

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
      <div className="section-title sub">Build</div>
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
      <div className="section-title sub">Sandbox</div>
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

export function Library({ scene, place, placing, onTemplate }: { scene: Scene; place: (k: string | null) => void; placing: string | null; onTemplate: (lines: string[]) => void }) {
  return (
    <>
      <div className="section-title sub">Assets</div>
      <div className="tiles">
        {ASSETS.map((a) => (
          <button key={a.key} className={`tile ${placing === a.key ? "on" : ""}`} onClick={() => place(placing === a.key ? null : a.key)} title={`Place ${a.name}: click in the model`}>
            <span className="tile-art" style={{ background: ASSET_TINT[a.key] }}>
              <Icons.component />
            </span>
            <span>{a.name}</span>
          </button>
        ))}
      </div>
      {scene.components.length > 0 && (
        <>
          <div className="section-title sub">In this model</div>
          <ul className="layers">
            {scene.components.map((c) => (
              <li key={c.id}>
                <Icons.component />
                <span className="name">{c.name}</span>
                <span className="count">{c.placed}×</span>
              </li>
            ))}
          </ul>
        </>
      )}
      <div className="section-title sub">Templates</div>
      <TemplateTiles onTemplate={onTemplate} />
      <p className="hint-text">Assets are components: edit one (double-click it) and every copy changes.</p>
    </>
  );
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

export function contextItems(scene: Scene, sel: Sel, run: Run, setSel: (s: Sel) => void, fitSel: () => void): MenuItem[] {
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
      run: () => {
        const name = window.prompt("Component name", "Component");
        if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
      },
    });
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
