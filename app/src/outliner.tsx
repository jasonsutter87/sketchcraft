// The Outliner: the model's hierarchy as a tree (groups inside groups,
// components placed in them), and its tags. Drag a row onto a group to put it
// inside; onto "Top level" to take it out. Every row can be hidden, locked and
// renamed in place; tags get a color, a solo and a rename.

import { useMemo, useRef, useState, type ReactElement } from "react";
import { ask } from "./ask";
import { type Reply, type Scene } from "./engine";
import { Icons } from "./icons";
import { EMPTY, type Sel } from "./selection";
import { autoTagColor } from "./Viewport";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

interface Node {
  id: number;
  kind: "groups" | "instances";
  name: string;
  hidden: boolean;
  locked: boolean;
  tag: string | null;
  faces: number;
  children: Node[];
}

const hex = (c: number[]) => "#" + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255).toString(16).padStart(2, "0")).join("");
const unhex = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);

/** groups and placements as a tree, by their parent links */
function buildTree(scene: Scene): Node[] {
  const nodes = new Map<number, Node>();
  for (const g of scene.groups) nodes.set(g.id, { id: g.id, kind: "groups", name: g.name, hidden: g.hidden, locked: g.locked, tag: g.tag, faces: g.faces, children: [] });
  for (const i of scene.instances) nodes.set(i.id, { id: i.id, kind: "instances", name: i.name, hidden: i.hidden, locked: false, tag: i.tag, faces: 0, children: [] });
  const roots: Node[] = [];
  const parentOf = (n: Node) => (n.kind === "groups" ? scene.groups.find((g) => g.id === n.id)?.parent : scene.instances.find((i) => i.id === n.id)?.parent) ?? null;
  for (const n of nodes.values()) {
    const p = parentOf(n);
    const parent = p != null ? nodes.get(p) : undefined;
    (parent ? parent.children : roots).push(n);
  }
  // groups first, then by name
  const sort = (list: Node[]) => {
    list.sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name, undefined, { numeric: true }) : a.kind === "groups" ? -1 : 1));
    list.forEach((n) => sort(n.children));
  };
  sort(roots);
  return roots;
}

export function Outliner({ scene, sel, setSel, run, byTag, setByTag }: { scene: Scene; sel: Sel; setSel: (s: Sel) => void; run: Run; byTag: boolean; setByTag: (on: boolean) => void }) {
  const [renaming, setRenaming] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [closed, setClosed] = useState<Set<number>>(new Set());
  const [dragging, setDragging] = useState<number[] | null>(null);
  const [over, setOver] = useState<number | "top" | null>(null);
  const tree = useMemo(() => buildTree(scene), [scene]);
  // pointer dragging (HTML5 drag-and-drop is taken over by the desktop
  // webview for file drops, so rows move with plain pointer events)
  const press = useRef<{ x: number; y: number; ids: number[] } | null>(null);
  const overAt = (x: number, y: number): number | "top" | null => {
    const el = document.elementFromPoint(x, y)?.closest("[data-drop]") as HTMLElement | null;
    if (!el) return null;
    return el.dataset.drop === "top" ? "top" : Number(el.dataset.drop);
  };
  const startPress = (e: React.PointerEvent, n: Node) => {
    if (e.button !== 0 || renaming === n.id) return;
    press.current = { x: e.clientX, y: e.clientY, ids: dragIds(n) };
    const move = (ev: PointerEvent) => {
      const p = press.current;
      if (!p) return;
      if (!draggingRef.current && Math.hypot(ev.clientX - p.x, ev.clientY - p.y) < 6) return;
      if (!draggingRef.current) {
        draggingRef.current = p.ids;
        setDragging(p.ids);
      }
      const o = overAt(ev.clientX, ev.clientY);
      setOver(o != null && o !== "top" && p.ids.includes(o) ? null : o);
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const ids = draggingRef.current;
      press.current = null;
      draggingRef.current = null;
      if (ids) {
        // a drag isn't a click
        suppressClick.current = true;
        const o = overAt(ev.clientX, ev.clientY);
        if (o === "top") drop(ids, null);
        else if (o != null && !ids.includes(o)) drop(ids, o);
        else (setDragging(null), setOver(null));
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const draggingRef = useRef<number[] | null>(null);
  const suppressClick = useRef(false);

  const commitRename = async () => {
    if (renaming != null && name.trim()) await run({ cmd: "rename", id: renaming, name: name.trim() });
    setRenaming(null);
  };
  const selected = (n: Node) => sel[n.kind].includes(n.id);
  const pick = (n: Node, add: boolean) => setSel(add ? { ...sel, [n.kind]: selected(n) ? sel[n.kind].filter((x) => x !== n.id) : [...sel[n.kind], n.id] } : { ...EMPTY, [n.kind]: [n.id] });
  // dragging a selected row drags the whole selection
  const dragIds = (n: Node) => (selected(n) ? [...sel.groups, ...sel.instances] : [n.id]);
  const drop = async (ids: number[] | null, parent: number | null) => {
    setDragging(null);
    setOver(null);
    if (!ids?.length || (parent != null && ids.includes(parent))) return;
    await run({ cmd: "set_parent", ids, parent });
  };

  // where you are: House › Room when you've opened groups
  const crumbs: string[] = [];
  for (let g = scene.activeGroup, n = 0; g != null && n < 64; n++) {
    const x = scene.groups.find((y) => y.id === g);
    if (!x) break;
    crumbs.unshift(x.name);
    g = x.parent;
  }

  const row = (n: Node, depth: number): ReactElement => {
    const kids = n.children.length > 0;
    const open = !closed.has(n.id);
    return (
      <li key={n.id} role="treeitem" aria-expanded={kids ? open : undefined} aria-selected={selected(n)}>
        <div
          className={`orow ${selected(n) ? "on" : ""} ${n.hidden ? "dim" : ""} ${over === n.id ? "drop" : ""}`}
          style={{ paddingLeft: 6 + depth * 14 }}
          data-drop={n.kind === "groups" ? n.id : undefined}
          onPointerDown={(e) => startPress(e, n)}
          onClick={(e) => {
            if (suppressClick.current) return void (suppressClick.current = false);
            pick(n, e.shiftKey || e.metaKey);
          }}
          onDoubleClick={() => (setRenaming(n.id), setName(n.name))}
          title={n.kind === "groups" ? "Drag onto a group to put it inside. Double-click to rename." : "A component. Drag onto a group to put it inside."}
        >
          <button
            className={`caret ${kids ? "" : "none"} ${open ? "open" : ""}`}
            aria-label={open ? "Collapse" : "Expand"}
            tabIndex={kids ? 0 : -1}
            onClick={(e) => {
              e.stopPropagation();
              const next = new Set(closed);
              if (open) next.add(n.id);
              else next.delete(n.id);
              setClosed(next);
            }}
          >
            {kids && <Icons.caret />}
          </button>
          {n.kind === "groups" ? <Icons.cube /> : <Icons.component />}
          {renaming === n.id ? (
            <input className="rename" autoFocus value={name} onChange={(e) => setName(e.target.value)} onBlur={commitRename} onKeyDown={(e) => e.key === "Enter" && commitRename()} onClick={(e) => e.stopPropagation()} />
          ) : (
            <span className="name">{n.name}</span>
          )}
          {n.tag && <span className="tag-dot" style={{ background: hex(scene.tagColors?.[n.tag] ?? autoTagColor(n.tag)) }} title={`Tag: ${n.tag}`} />}
          {n.kind === "groups" && n.faces > 0 && <span className="count">{n.faces}</span>}
          {n.kind === "groups" && (
            <button
              className={`rowbtn ${n.locked ? "keep" : ""}`}
              title={n.locked ? "Locked: click to unlock" : "Lock: no moving, editing or deleting"}
              aria-label={n.locked ? "Unlock" : "Lock"}
              onClick={(e) => {
                e.stopPropagation();
                run({ cmd: "lock", ids: [n.id], locked: !n.locked });
              }}
            >
              {n.locked ? <Icons.lock /> : <Icons.unlock />}
            </button>
          )}
          <button
            className={`rowbtn eye ${n.hidden ? "keep" : ""}`}
            title={n.hidden ? "Show" : "Hide"}
            aria-label={n.hidden ? "Show" : "Hide"}
            onClick={(e) => {
              e.stopPropagation();
              run({ cmd: n.hidden ? "unhide" : "hide", ids: [n.id] });
            }}
          >
            {n.hidden ? <Icons.eyeOff /> : <Icons.eye />}
          </button>
        </div>
        {kids && open && <ul role="group">{n.children.map((c) => row(c, depth + 1))}</ul>}
      </li>
    );
  };

  return (
    <>
      {(scene.editing != null || scene.activeGroup != null) && (
        <div className="editing-banner">
          <span className="crumbs">{scene.editing != null ? "Editing component" : crumbs.join(" › ")}</span>
          <button onClick={() => run({ cmd: "close" })} title="Back out one level (Esc)">
            {crumbs.length > 1 ? "Back" : "Done"}
          </button>
        </div>
      )}
      {dragging && (
        <div className={`top-drop ${over === "top" ? "drop" : ""}`} data-drop="top">
          Drop here to take it out to the top level
        </div>
      )}
      <ul className="otree" role="tree" aria-label="Model hierarchy">
        {tree.map((n) => row(n, 0))}
        {scene.loose > 0 && (
          <li className="muted-row">
            <div className="orow" style={{ paddingLeft: 6 }}>
              <span className="caret none" />
              <Icons.layer />
              <span className="name">Loose geometry</span>
              <span className="count">{scene.loose}</span>
            </div>
          </li>
        )}
        {!tree.length && !scene.loose && <li className="empty">Nothing yet. Press R and draw on the ground, or drop something from the Library.</li>}
      </ul>
      {tree.length > 0 && !dragging && <p className="hint-line">Right-click › Make group to group a selection; drag rows onto a group to nest them.</p>}
      <Tags scene={scene} sel={sel} run={run} byTag={byTag} setByTag={setByTag} />
    </>
  );
}

function Tags({ scene, sel, run, byTag, setByTag }: { scene: Scene; sel: Sel; run: Run; byTag: boolean; setByTag: (on: boolean) => void }) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [name, setName] = useState("");
  const tags = Object.entries(scene.tags).sort((a, b) => a[0].localeCompare(b[0], undefined, { numeric: true }));
  const used = (t: string) => scene.groups.filter((g) => g.tag === t).length + scene.instances.filter((i) => i.tag === t).length;
  const taggable = [...sel.groups, ...sel.instances];
  const solo = async (t: string) => {
    const soloed = tags.every(([x, v]) => (x === t ? v : !v));
    // solo again: everything back on
    for (const [x] of tags) await run({ cmd: "tag_visible", tag: x, visible: soloed ? true : x === t });
  };
  const commit = async () => {
    if (renaming && name.trim() && name.trim() !== renaming) await run({ cmd: "rename_tag", tag: renaming, to: name.trim() });
    setRenaming(null);
  };
  return (
    <>
      <div className="section-title sub tags-head">
        <span>Tags</span>
        <label className="bytag" title="Color every face by its tag (or the tag of the group it's in)">
          <input type="checkbox" checked={byTag} onChange={(e) => setByTag(e.target.checked)} /> Color by tag
        </label>
      </div>
      <ul className="otree tags" aria-label="Tags">
        {tags.map(([t, vis]) => {
          const color = hex(scene.tagColors?.[t] ?? autoTagColor(t));
          return (
            <li key={t}>
              <div className={`orow ${vis ? "" : "dim"}`} style={{ paddingLeft: 6 }} onDoubleClick={() => (setRenaming(t), setName(t))} title="Double-click to rename">
                <label className="swatch-in" title="Tag color">
                  <span style={{ background: color }} />
                  <input type="color" value={color} onChange={(e) => run({ cmd: "tag_color", tag: t, color: unhex(e.target.value) })} />
                </label>
                {renaming === t ? (
                  <input className="rename" autoFocus value={name} onChange={(e) => setName(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
                ) : (
                  <span className="name">{t}</span>
                )}
                <span className="count">{used(t)}</span>
                <button className="rowbtn" title="Solo: show only this tag (again: show all)" aria-label={`Solo ${t}`} onClick={() => solo(t)}>
                  <Icons.solo />
                </button>
                <button className={`rowbtn eye ${vis ? "" : "keep"}`} title={vis ? "Hide this tag" : "Show this tag"} aria-label={vis ? "Hide" : "Show"} onClick={() => run({ cmd: "tag_visible", tag: t, visible: !vis })}>
                  {vis ? <Icons.eye /> : <Icons.eyeOff />}
                </button>
              </div>
            </li>
          );
        })}
        {!tags.length && <li className="empty">Tags are layers: give things a tag to show, hide or color them together.</li>}
      </ul>
      <button
        className="btn small ghost new-tag"
        disabled={!taggable.length}
        title={taggable.length ? "Tag what's selected" : "Select groups or components first"}
        onClick={async () => {
          const t = await ask(`Tag ${taggable.length} selected`, tags[0]?.[0] ?? "Walls", "A new tag name, or one you already have.");
          if (t?.trim()) {
            await run({ cmd: "tag", ids: taggable, tag: t.trim() });
            if (!(t.trim() in scene.tags)) await run({ cmd: "tag_visible", tag: t.trim(), visible: true });
          }
        }}
      >
        {taggable.length ? `Tag ${taggable.length} selected…` : "New tag (select something first)"}
      </button>
    </>
  );
}
