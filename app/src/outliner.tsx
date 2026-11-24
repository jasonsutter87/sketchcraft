// The Outliner: the model's hierarchy as a tree (groups inside groups,
// components placed in them), and its tags. Drag a row onto a group to put it
// inside; onto "Top level" to take it out. Every row can be hidden, locked and
// renamed in place; tags get a color, a solo and a rename.

import { useMemo, useRef, useState, type ReactElement } from "react";
import { ask } from "./ask";
import { useLocale } from "./locale";
import { type Reply, type Scene } from "./engine";
import { Icons } from "./icons";
import { EMPTY, type Sel } from "./selection";
import { autoTagColor } from "./Viewport";
import { LineStyleControls } from "./linestyle";
import { HatchControls } from "./hatchstyle";
import { CutStyleControls } from "./cutstyle";
import { CutPriority } from "./cutpriorityeditor";
import { buildTree, flatten, matching, MAX_OUTLINE_DEPTH, type Row, type TreeNode as Node } from "./grouptree";

/** Rows deeper than this are drawn at this indent (a model can be nested far deeper than the panel is wide). */
const INDENT_LEVELS = 24;

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

const hex = (c: number[]) => "#" + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255).toString(16).padStart(2, "0")).join("");
const unhex = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);

export function Outliner({ scene, sel, setSel, run, byTag, setByTag }: { scene: Scene; sel: Sel; setSel: (s: Sel) => void; run: Run; byTag: boolean; setByTag: (on: boolean) => void }) {
  const { t } = useLocale();
  const [renaming, setRenaming] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [closed, setClosed] = useState<Set<number>>(new Set());
  const [dragging, setDragging] = useState<number[] | null>(null);
  const [over, setOver] = useState<number | "top" | null>(null);
  const [query, setQuery] = useState("");
  const full = useMemo(() => buildTree(scene), [scene]);
  // searching: rows whose name or tag matches, with the groups they sit in. The rows are one flat list worked out
  // without recursion (a model nested hundreds of levels deep must not overflow the stack), indented by their depth.
  const { rows, cut, more } = useMemo(() => {
    const searching = query.trim() !== "";
    const only = searching ? matching(full, query) : null;
    return flatten(full, (id) => searching || !closed.has(id), only);
  }, [full, query, closed]);
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

  const row = ({ node: n, depth, parentHidden, hasKids }: Row): ReactElement => {
    // hidden itself, or inside something hidden: the whole chain is off
    const off = n.hidden || parentHidden;
    const kids = hasKids;
    const open = query.trim() !== "" || !closed.has(n.id);
    return (
      <li key={n.id} role="treeitem" aria-level={depth + 1} aria-expanded={kids ? open : undefined} aria-selected={selected(n)}>
        <div
          className={`orow ${selected(n) ? "on" : ""} ${off ? "dim" : ""} ${over === n.id ? "drop" : ""}`}
          style={{ paddingLeft: 6 + Math.min(depth, INDENT_LEVELS) * 14 }}
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
            title={n.hidden ? "Show" : parentHidden ? "Hidden because a group it's in is hidden" : "Hide (and everything inside)"}
            aria-label={n.hidden ? "Show" : "Hide"}
            onClick={(e) => {
              e.stopPropagation();
              run({ cmd: n.hidden ? "unhide" : "hide", ids: [n.id] });
            }}
          >
            {n.hidden ? <Icons.eyeOff /> : <Icons.eye />}
          </button>
        </div>
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
      <div className="otools">
        <button
          className="btn small ghost"
          title={scene.activeGroup != null ? "An empty group inside the open group" : "An empty group to organize things into: drag rows onto it"}
          onClick={async () => {
            const n = await ask("Name the new group", "Upper floor", "An empty group to organize with. Drag things onto it in this list.");
            if (n?.trim()) {
              const r = await run({ cmd: "group", ids: [], name: n.trim() });
              if (r.ok) setSel({ ...EMPTY, groups: [r.group as number] });
            }
          }}
        >
          + New group
        </button>
      </div>
      {(scene.groups.length + scene.instances.length > 6 || query) && (
        <input className="otree-search" type="search" placeholder="Search names and tags" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setQuery("")} aria-label="Search the outliner" />
      )}
      <ul className="otree" role="tree" aria-label="Model hierarchy">
        {query.trim() && !rows.length && <li className="empty">Nothing named "{query.trim()}"</li>}
        {rows.map(row)}
        {cut > 0 && <li className="muted-row">{t(cut === 1 ? "outliner.deep.one" : "outliner.deep.many").replace("{n}", String(cut)).replace("{max}", String(MAX_OUTLINE_DEPTH))}</li>}
        {more > 0 && <li className="muted-row" data-more-rows>{t(more === 1 ? "outliner.more.one" : "outliner.more.many").replace("{n}", String(more))}</li>}
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
        {!rows.length && !scene.loose && !query.trim() && <li className="empty">Nothing yet. Press R and draw on the ground, or drop something from the Library.</li>}
      </ul>
      {rows.length > 0 && !dragging && <p className="hint-line">Right-click › Make group to group a selection; drag rows onto a group to nest them.</p>}
      {!!scene.clouds?.length && <Clouds scene={scene} run={run} />}
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
              <div className="tag-line">
                <LineStyleControls label={`tag ${t}`} none="Default" value={scene.tagStyles?.[t] ?? {}} onChange={(c) => run({ cmd: "tag_style", tag: t, ...c })} />
                <CutStyleControls tag={t} style={scene.tagStyles?.[t]} run={run} />
                <HatchControls tag={t} value={scene.tagStyles?.[t]?.cut?.fill === "hatch" ? scene.tagStyles?.[t]?.cut?.hatch : null} run={run} />
              </div>
            </li>
          );
        })}
        {!tags.length && <li className="empty">Tags are layers: give things a tag to show, hide or color them together.</li>}
      </ul>
      <CutPriority tags={tags.map(([t]) => t)} styles={scene.tagStyles} run={run} />
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

/** Point clouds (scans): show / hide, rename, delete. */
function Clouds({ scene, run }: { scene: Scene; run: Run }) {
  const [renaming, setRenaming] = useState<number | null>(null);
  const [name, setName] = useState("");
  const commit = async () => {
    if (renaming != null && name.trim()) await run({ cmd: "cloud", id: renaming, name: name.trim() });
    setRenaming(null);
  };
  return (
    <>
      <div className="section-title sub">Point clouds</div>
      <ul className="otree" aria-label="Point clouds">
        {scene.clouds!.map((c) => (
          <li key={c.id}>
            <div className={`orow ${c.hidden ? "dim" : ""}`} style={{ paddingLeft: 6 }} onDoubleClick={() => (setRenaming(c.id), setName(c.name))} title="Double-click to rename. Drawing tools snap to its points.">
              {renaming === c.id ? (
                <input className="rename" autoFocus value={name} onChange={(e) => setName(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
              ) : (
                <span className="name">{c.name}</span>
              )}
              <span className="count" title={`${c.count.toLocaleString()} points`}>
                {c.count >= 1e6 ? `${(c.count / 1e6).toFixed(1)}M` : c.count >= 1e3 ? `${Math.round(c.count / 1e3)}k` : c.count}
              </span>
              <button className="rowbtn" title="Delete this point cloud" aria-label={`Delete ${c.name}`} onClick={() => run({ cmd: "cloud", id: c.id, delete: true })}>
                ×
              </button>
              <button className={`rowbtn eye ${c.hidden ? "keep" : ""}`} title={c.hidden ? "Show" : "Hide"} aria-label={c.hidden ? "Show" : "Hide"} onClick={() => run({ cmd: "cloud", id: c.id, hidden: !c.hidden })}>
                {c.hidden ? <Icons.eyeOff /> : <Icons.eye />}
              </button>
            </div>
          </li>
        ))}
      </ul>
    </>
  );
}
