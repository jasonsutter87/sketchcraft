// Side panels: Outliner (groups, components, tags), Library (assets and the
// model's components), Views (saved cameras), Entity Info, and the
// right-click menu.
import { useState } from "react";
import { ASSETS, type Reply, type Scene } from "./engine";
import { Icons } from "./icons";
import { EMPTY, isEmpty, selIds, type Sel } from "./selection";
import { TEMPLATES } from "./templates";
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

export function Views({ scene, run, camera, goTo }: { scene: Scene; run: Run; camera: () => Cam; goTo: (c: Cam) => void }) {
  const [name, setName] = useState("");
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
      <ul className="layers views-list">
        {scene.views.map((v) => (
          <li key={v.name} onClick={() => goTo(v)}>
            <Icons.camera />
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
  return (
    <div className="section">
      <div className="section-title">{title}</div>
      <div className="kv">
        {faces.length > 0 && (
          <>
            <span>Area</span>
            <b>{faces.reduce((a, f) => a + f.area, 0).toFixed(2)} m²</b>
          </>
        )}
        {edges.length > 0 && (
          <>
            <span>Length</span>
            <b>{len.toFixed(2)} m</b>
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
    items.push({ label: "Make group", key: "G", run: () => run({ cmd: "group", ids: sel.faces }).then(() => setSel(EMPTY)) });
  }
  if (sel.faces.length || sel.groups.length) {
    items.push({
      label: "Make component…",
      run: () => {
        const name = window.prompt("Component name", "Component");
        if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
      },
    });
  }
  if (sel.instances.length) items.push({ label: "Explode", run: () => Promise.all(sel.instances.map((id) => run({ cmd: "explode", id }))).then(() => setSel(EMPTY)) });
  if (sel.faces.length || sel.groups.length) items.push({ label: "Reverse faces", run: () => run({ cmd: "reverse", ids: [...sel.faces, ...sel.groups] }) });
  if (sel.edges.length) items.push({ label: "Soften edges", run: () => run({ cmd: "soften", ids: sel.edges }) });
  items.push({ label: "Zoom to selection", key: "⇧Z", run: fitSel });
  items.push({ label: "Hide", key: "H", run: () => run({ cmd: "hide", ids }).then(() => setSel(EMPTY)) });
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
