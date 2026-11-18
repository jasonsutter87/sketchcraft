// Outliner > Tags > Intersection priority: which tag wins where two cuts overlap (a wall into a slab,
// insulation inside a wall). A list from the winner (top) to the loser, reordered by dragging, by the up and down
// buttons, or by Alt+Arrow keys on a row, plus a number field per tag. The engine trims the lower tag back with no seam.

import { useState } from "react";
import type { Reply, TagStyleInfo } from "./engine";
import { moveTag, parsePriority, priorityCommand, priorityOf, priorityOrder, renumber, tiedTags } from "./cutpriority";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

/** Two overlapping boxes: the first (the winner) whole, the second stopping at its edge. */
function Example() {
  return (
    <svg className="prio-example" width="64" height="22" viewBox="0 0 64 22" aria-hidden="true">
      <rect x="31" y="1" width="32" height="20" fill="#c9ccd6" stroke="#1a1a1f" />
      <rect x="1" y="1" width="40" height="20" fill="#8a8f9c" stroke="#1a1a1f" />
    </svg>
  );
}

export function CutPriority({ tags, styles, run }: { tags: string[]; styles: Record<string, TagStyleInfo> | undefined; run: Run }) {
  const [open, setOpen] = useState(false);
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [typed, setTyped] = useState<Record<string, string>>({});
  const order = priorityOrder(tags, styles);
  const tied = tiedTags(order, styles);
  if (tags.length < 2) return null;

  const apply = async (changes: Record<string, number>) => {
    const entries = Object.entries(changes);
    if (!entries.length) return;
    // one undo step for the whole reorder
    // (a batch the engine refused to open is not ended: the commands then run one by one)
    const opened = entries.length > 1 ? await run({ cmd: "begin_batch" }) : null;
    const b = opened && opened.ok !== false ? opened : null;
    let fail = "";
    for (const [t, p] of entries) {
      const r = await run(priorityCommand(t, p));
      if (!r.ok) fail = String(r.error ?? "That didn't work");
    }
    if (b) await run(typeof b.batch === "number" ? { cmd: "end_batch", id: b.batch } : { cmd: "end_batch" });
    setNote(fail);
  };
  const move = (from: number, to: number) => void apply(renumber(moveTag(order, from, to), styles));
  const commitNumber = (t: string) => {
    const text = typed[t];
    setTyped(({ [t]: _gone, ...rest }) => rest);
    if (text === undefined) return;
    const p = parsePriority(text);
    if (p === null) return setNote(`Priority is a whole number from 0 to 999, not "${text}"`);
    if (p !== priorityOf(styles?.[t])) void apply({ [t]: p });
    else setNote("");
  };

  return (
    <div className="cut-priority">
      <button className="btn small ghost" aria-expanded={open} aria-controls="cut-priority-list" title="Which tag wins where two cuts overlap in a plan or section" onClick={() => setOpen(!open)}>
        Intersection priority
      </button>
      {open && (
        <div id="cut-priority-list" className="hatch-editor prio-editor" role="group" aria-label="Intersection priority">
          <p className="hint">Where two tags' cuts overlap, the tag nearer the top keeps the overlap and the one below stops at its edge. Drag to reorder, or use the arrows.</p>
          <p className="hint prio-eg">
            <Example /> Wall above Slab: the wall stays whole where it runs into the slab.
          </p>
          <ol className="prio-list" aria-label="Tags, top wins">
            {order.map((t, i) => (
              <li
                key={t}
                className={`prio-row ${drag === i ? "dragging" : ""} ${over === i && drag !== null && drag !== i ? "over" : ""}`}
                draggable
                data-tag={t}
                onDragStart={(e) => {
                  setDrag(i);
                  e.dataTransfer.effectAllowed = "move";
                  e.dataTransfer.setData("text/plain", t);
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
                <span
                  className="grip"
                  role="button"
                  tabIndex={0}
                  aria-label={`Move tag ${t}: Alt+Up or Alt+Down`}
                  title="Drag to reorder (or Alt+Up / Alt+Down)"
                  onKeyDown={(e) => {
                    if (e.altKey && e.key === "ArrowUp") (e.preventDefault(), move(i, i - 1));
                    if (e.altKey && e.key === "ArrowDown") (e.preventDefault(), move(i, i + 1));
                  }}
                >
                  ⠿
                </span>
                <span className="name">{t}</span>
                {tied.has(t) && (
                  <span className="hint tie" title="Same number as another tag: the first by name wins an overlap">
                    =
                  </span>
                )}
                <input
                  className="prio-num"
                  inputMode="numeric"
                  aria-label={`Intersection priority of tag ${t}`}
                  title="0 to 999: higher wins"
                  value={typed[t] ?? String(priorityOf(styles?.[t]))}
                  onChange={(e) => setTyped({ ...typed, [t]: e.target.value })}
                  onBlur={() => commitNumber(t)}
                  onKeyDown={(e) => e.key === "Enter" && commitNumber(t)}
                />
                <button className="rowbtn" aria-label={`Move tag ${t} up`} title="Higher priority" disabled={i === 0} onClick={() => move(i, i - 1)}>
                  ▲
                </button>
                <button className="rowbtn" aria-label={`Move tag ${t} down`} title="Lower priority" disabled={i === order.length - 1} onClick={() => move(i, i + 1)}>
                  ▼
                </button>
              </li>
            ))}
          </ol>
          {note && (
            <p className="hint hatch-note" role="status">
              {note}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
