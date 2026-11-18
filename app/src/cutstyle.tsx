// A tag's cut fill (Outliner > Tags): solid colour, hatch, or none, and the cut outline's weight, for its cut walls,
// slabs and furniture in plans and sections, on screen and on sheets. The button is a swatch of the fill as it
// prints; opening it shows the choices, each with a hint, and the outline weight.

import { useState } from "react";
import type { Reply, TagStyleInfo } from "./engine";
import { CUT_FILLS, CUT_OUTLINE_MM, cutColorOf, cutFillCommand, cutFillOf, cutLabel, cutOutlineOf, type CutFillKind } from "./cutfill";
import { PEN_WEIGHTS, penLabel } from "./linestyle";

/** A small box filled the way the cut is: solid, a few hatch strokes, or empty, with its outline. */
export function CutSwatch({ kind, color, outline, w, h }: { kind: CutFillKind; color: string; outline: number; w: number; h: number }) {
  const sw = Math.max(1, Math.min(4, outline * 4));
  const fill = kind === "solid" ? color : kind === "auto" ? "#292b33" : "#fff";
  const strokes: string[] = [];
  if (kind === "hatch") for (let x = -h; x < w; x += 4) strokes.push(`M${x} ${h}L${x + h} 0`);
  return (
    <svg className="cut-swatch" width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden="true" data-fill={kind}>
      <rect x={sw / 2} y={sw / 2} width={w - sw} height={h - sw} fill={fill} stroke="#1a1a1f" strokeWidth={sw} />
      {strokes.length > 0 && <path d={strokes.join("")} stroke="#1a1a1f" strokeWidth={0.8} />}
    </svg>
  );
}

export function CutStyleControls({ tag, style, run }: { tag: string; style: TagStyleInfo | undefined; run: (c: Record<string, unknown>) => Promise<Reply> }) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  const kind = cutFillOf(style);
  const color = cutColorOf(style);
  const outline = cutOutlineOf(style);
  const send = async (c: Record<string, unknown>) => {
    const r = await run(c);
    setNote(r.ok ? "" : String(r.error ?? "That didn't work"));
  };
  const id = `cut-${tag.replace(/[^a-z0-9]/gi, "_")}`;
  const own = style?.cut?.outline_mm && style.cut.outline_mm > 0 ? style.cut.outline_mm : 0;
  return (
    <span className="cutstyle">
      <button className="rowbtn cut-btn" aria-label={`Cut fill of tag ${tag}`} aria-expanded={open} aria-controls={id} title={`Cut fill: how ${tag} looks where a section cuts it. Now: ${cutLabel(style)}`} onClick={() => setOpen(!open)}>
        <CutSwatch kind={kind} color={color} outline={outline} w={22} h={14} />
      </button>
      {open && (
        <div className="hatch-editor cut-editor" id={id} role="group" aria-label={`Cut fill of ${tag}`}>
          <p className="hint">Where a section plane or plan cuts {tag}, its cut faces are filled like this, on screen and on sheets.</p>
          <div className="hatch-row" role="radiogroup" aria-label={`Cut fill kind of tag ${tag}`}>
            {CUT_FILLS.map((f) => (
              <button key={f.kind} role="radio" aria-checked={kind === f.kind} className={`btn small cut-kind ${kind === f.kind ? "on" : "ghost"}`} title={f.hint} data-kind={f.kind} onClick={() => void send(cutFillCommand(tag, f.kind, style))}>
                <CutSwatch kind={f.kind} color={color} outline={outline} w={16} h={12} />
                {f.label}
              </button>
            ))}
          </div>
          <p className="hint cut-kind-hint">{CUT_FILLS.find((f) => f.kind === kind)?.hint}</p>
          <div className="hatch-row">
            {kind === "solid" && (
              <label className="swatch-in" title="Cut fill colour">
                <span style={{ background: color }} />
                <input type="color" aria-label={`Cut fill colour of tag ${tag}`} value={color} onChange={(e) => void send({ cmd: "tag_style", tag, cut: { color: e.target.value } })} />
              </label>
            )}
            <select className="units-select" aria-label={`Cut outline weight of tag ${tag}`} title="Cut outline: the pen width of the line around the cut, on paper" value={String(own)} onChange={(e) => void send({ cmd: "tag_style", tag, cut: { outline_mm: Number(e.target.value) } })}>
              <option value="0">Outline: default ({penLabel(CUT_OUTLINE_MM)})</option>
              {own > 0 && !PEN_WEIGHTS.includes(own) && <option value={String(own)}>Outline: {penLabel(own)}</option>}
              {PEN_WEIGHTS.map((p) => (
                <option key={p} value={String(p)}>
                  Outline: {penLabel(p)}
                </option>
              ))}
            </select>
            {kind === "hatch" && <span className="hint">Pick the pattern with the Hatch button.</span>}
          </div>
          {note && (
            <p className="hint hatch-note" role="status">
              {note}
            </p>
          )}
        </div>
      )}
    </span>
  );
}
