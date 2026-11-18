// Layout > Pages: the saved sheets in their order, each with a small picture of where its views sit. Add, duplicate,
// delete and rename a page; reorder by dragging, by the arrows, or with Alt+Up / Alt+Down. Each is one undo step.
// Up and Down move between pages, F2 renames, Delete deletes (Undo brings it back).

import { useRef, useState } from "react";
import { moveIn, viewportsOf, type SheetInfo } from "./pages";
import type { LayoutState } from "./uselayout";

/** The page's outline and its viewports' frames, to scale: a thumbnail that costs nothing to draw. */
function Thumb({ s }: { s: SheetInfo }) {
  const W = 44;
  const H = 33;
  // a pixel clear all round, so the outline isn't cut
  const k = Math.min((W - 2) / s.paper.w_mm, (H - 2) / s.paper.h_mm);
  const w = s.paper.w_mm * k;
  const h = s.paper.h_mm * k;
  const blue = s.fields?.blueprint === "true";
  return (
    <svg className="page-thumb" width={W} height={H} viewBox={`${-(W - w) / 2} ${-(H - h) / 2} ${W} ${H}`} aria-hidden="true">
      <rect className={`sheet ${blue ? "blue" : ""}`} x={0} y={0} width={w} height={h} />
      {viewportsOf(s).map((v) => (
        <rect key={v.id} className="vp" x={v.rect_mm[0] * k} y={v.rect_mm[1] * k} width={v.rect_mm[2] * k} height={v.rect_mm[3] * k} opacity={v.opacity ?? 1} />
      ))}
    </svg>
  );
}

export function PagesPanel({ L }: { L: LayoutState }) {
  const { sheets, page } = L;
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [text, setText] = useState("");
  const list = useRef<HTMLOListElement>(null);
  const ids = sheets.map((s) => s.id);
  const move = (from: number, to: number) => {
    if (to < 0 || to >= ids.length || from === to) return;
    void L.reorder(moveIn(ids, from, to));
    // keep the keyboard on the page that moved
    window.setTimeout(() => (list.current?.querySelector(`[data-page="${ids[from]}"] .page-main`) as HTMLElement | null)?.focus(), 60);
  };
  const focusRow = (i: number) => {
    const s = sheets[Math.max(0, Math.min(sheets.length - 1, i))];
    if (!s) return;
    L.setPage(s.id);
    (list.current?.querySelector(`[data-page="${s.id}"] .page-main`) as HTMLElement | null)?.focus();
  };
  const startRename = (s: SheetInfo) => {
    setEditing(s.id);
    setText(s.name);
  };
  const commit = async () => {
    const id = editing;
    setEditing(null);
    if (id != null) await L.rename(id, text);
  };

  return (
    <div className="pages-panel">
      <div className="pages-head">
        <span className="section-title">Pages</span>
        <button className="btn small" onClick={() => void L.add()} title="A new empty page at the end, the same paper as the last one">
          + Add page
        </button>
      </div>
      {L.ready && sheets.length === 0 && <p className="hint">No pages yet. Add one, then put views of the model on it from the inspector on the right.</p>}
      {sheets.length > 0 && <p className="hint pages-how">Drag to reorder, or Alt+↑ / Alt+↓. Double-click a name to rename it.</p>}
      <ol ref={list} className="pages-list" aria-label="Pages, in order">
        {sheets.map((s, i) => {
          const on = s.id === page?.id;
          return (
            <li
              key={s.id}
              data-page={s.id}
              className={`page-row ${on ? "on" : ""} ${drag === i ? "dragging" : ""} ${over === i && drag !== null && drag !== i ? (drag < i ? "over-after" : "over-before") : ""}`}
              draggable={editing !== s.id}
              onDragStart={(e) => {
                setDrag(i);
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", s.name);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                setOver(i);
              }}
              onDrop={(e) => {
                e.preventDefault();
                if (drag !== null && drag !== i) move(drag, i);
                setDrag(null);
                setOver(null);
              }}
              onDragEnd={() => (setDrag(null), setOver(null))}
            >
              <span className="grip" aria-hidden="true" title="Drag to reorder">
                ⠿
              </span>
              {editing === s.id ? (
                <input
                  className="page-name-edit"
                  autoFocus
                  aria-label={`New name for page ${i + 1}`}
                  value={text}
                  maxLength={200}
                  onChange={(e) => setText(e.target.value)}
                  onFocus={(e) => e.target.select()}
                  onBlur={() => void commit()}
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter") void commit();
                    if (e.key === "Escape") setEditing(null);
                  }}
                />
              ) : (
                <button
                  className="page-main"
                  aria-current={on ? "page" : undefined}
                  aria-label={`Page ${i + 1} of ${sheets.length}: ${s.number ? `${s.number} ` : ""}${s.name}. Alt+Up or Alt+Down moves it, F2 renames, Delete deletes.`}
                  title={`${s.name}: ${s.paper.name ? s.paper.name.toUpperCase() : `${Math.round(s.paper.w_mm)} × ${Math.round(s.paper.h_mm)} mm`}, ${viewportsOf(s).length} view${viewportsOf(s).length === 1 ? "" : "s"}`}
                  onClick={() => L.setPage(s.id)}
                  onDoubleClick={() => startRename(s)}
                  onKeyDown={(e) => {
                    const k = e.key;
                    let used = true;
                    if (e.altKey && k === "ArrowUp") move(i, i - 1);
                    else if (e.altKey && k === "ArrowDown") move(i, i + 1);
                    else if (k === "ArrowUp") focusRow(i - 1);
                    else if (k === "ArrowDown") focusRow(i + 1);
                    else if (k === "Home") focusRow(0);
                    else if (k === "End") focusRow(sheets.length - 1);
                    else if (k === "F2") startRename(s);
                    else if (k === "Delete" || k === "Backspace") void L.remove(s.id);
                    else used = false;
                    if (used) {
                      e.preventDefault();
                      e.stopPropagation();
                    }
                  }}
                >
                  <Thumb s={s} />
                  <span className="page-text">
                    {s.number && <span className="page-number">{s.number}</span>}
                    <span className="page-name">{s.name}</span>
                  </span>
                </button>
              )}
              <span className="page-actions">
                <button className="rowbtn" aria-label={`Move ${s.name} up`} title="Earlier in the set (Alt+↑)" disabled={i === 0} onClick={() => move(i, i - 1)}>
                  ▲
                </button>
                <button className="rowbtn" aria-label={`Move ${s.name} down`} title="Later in the set (Alt+↓)" disabled={i === sheets.length - 1} onClick={() => move(i, i + 1)}>
                  ▼
                </button>
                <button className="rowbtn" aria-label={`Rename ${s.name}`} title="Rename (F2)" onClick={() => startRename(s)}>
                  ✎
                </button>
                <button className="rowbtn" aria-label={`Duplicate ${s.name}`} title="Duplicate: a copy with everything on it, right after it" onClick={() => void L.duplicate(s.id)}>
                  ⧉
                </button>
                <button className="rowbtn danger" aria-label={`Delete ${s.name}`} title="Delete this page and what's on it (Undo brings it back)" onClick={() => void L.remove(s.id)}>
                  ✕
                </button>
              </span>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
