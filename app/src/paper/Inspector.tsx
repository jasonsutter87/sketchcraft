// Layout's inspector (the right panel): the page you're on (name, number, paper, title block) and, when you pick a
// viewport on it, that viewport (label, what it shows, scale, opacity). Each change is one engine command, one undo
// step; a slider sends its value when you let go.

import { useEffect, useState } from "react";
import type { Reply, Scene } from "../engine";
import { describe, isDim, isShape, isTable, isText, paperItems, type Align, type Border, type PaperItem, type TableItem } from "./items";
import { opacityOf, paperId, PAPERS, parseScale, percentOf, scaleText, STANDARD_VIEWS, viewArgs, viewName, viewportsOf, type SheetInfo, type ViewportItem } from "./pages";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

/** A text field that sends its value on Enter or when you leave it (Escape puts it back). */
function Field({ label, value, onCommit, hint, multiline, placeholder }: { label: string; value: string; onCommit: (v: string) => void; hint?: string; multiline?: boolean; placeholder?: string }) {
  const [text, setText] = useState<string | null>(null);
  const props = {
    value: text ?? value,
    title: hint,
    placeholder,
    "aria-label": label,
    onChange: (e: { target: { value: string } }) => setText(e.target.value),
    onBlur: () => {
      if (text != null && text !== value) onCommit(text);
      setText(null);
    },
    onKeyDown: (e: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      e.stopPropagation();
      if (e.key === "Enter" && !multiline) (e.target as HTMLInputElement).blur();
      if (e.key === "Escape") {
        setText(null);
        window.setTimeout(() => (e.target as HTMLInputElement).blur(), 0);
      }
    },
  };
  return (
    <>
      <span>{label}</span>
      {multiline ? <textarea rows={3} {...props} /> : <input {...props} />}
    </>
  );
}

const SCALES = ["1:1", "1:5", "1:10", "1:20", "1:25", "1:50", "1:100", "1:200", "1:500", "1/4\"=1'-0\"", "1/8\"=1'-0\"", "3/16\"=1'-0\"", "1/2\"=1'-0\""];

function ViewportInspector({ v, scene, run, onDone }: { v: ViewportItem; scene: Scene; run: Run; onDone: () => void }) {
  const [pct, setPct] = useState(percentOf(v));
  const [scaleErr, setScaleErr] = useState("");
  // the saved value, when it changes (undo, an agent)
  useEffect(() => setPct(percentOf(v)), [v.opacity, v.id]); // eslint-disable-line react-hooks/exhaustive-deps
  const commitOpacity = (p: number) => {
    if (opacityOf(p) !== (v.opacity ?? 1)) void run({ cmd: "viewport_edit", id: v.id, opacity: opacityOf(p) });
  };
  const fromView = (name: string) => {
    if (name) void run({ cmd: "viewport_edit", id: v.id, ...viewArgs(name) });
  };
  return (
    <div className="section layout-inspector" aria-label="Viewport">
      <div className="section-title">Viewport</div>
      <p className="hint">{viewName(v)}. A window on the model: it shows the model as it is now.</p>
      <div className="kv">
        <Field label="Label" value={v.label ?? ""} placeholder="(no caption)" hint="The caption under the view" onCommit={(t) => void run({ cmd: "viewport_edit", id: v.id, label: t })} />
        <span>View</span>
        <select aria-label="What the viewport shows" value="" onChange={(e) => fromView(e.target.value)} title="Show the model from another side (a scaled view is parallel)">
          <option value="">{viewName(v)}…</option>
          <option value="plan">Floor plan (cut 1.2 m up)</option>
          {STANDARD_VIEWS.map((s) => (
            <option key={s} value={s}>
              {s === "iso" ? "3D view (iso)" : s === "top" ? "Top view" : `${s[0].toUpperCase()}${s.slice(1)} elevation`}
            </option>
          ))}
          {scene.views.map((sv) => (
            <option key={`s:${sv.name}`} value={sv.name}>
              Scene: {sv.name}
            </option>
          ))}
        </select>
        <span>Scale</span>
        <span className="scale-field">
          <input
            aria-label="Scale"
            list="paper-scales"
            defaultValue={scaleText(v.scale)}
            key={`${v.id}:${v.scale}`}
            title='1:N, or imperial like 1/4"=1&apos;-0"'
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            onBlur={async (e) => {
              const s = parseScale(e.target.value);
              if (s == null) return setScaleErr(`"${e.target.value}" isn't a scale: try 1:50`);
              setScaleErr("");
              if (s !== v.scale && s !== scaleText(v.scale)) await run({ cmd: "viewport_edit", id: v.id, scale: s });
            }}
          />
          <datalist id="paper-scales">
            {SCALES.map((s) => (
              <option key={s} value={s} />
            ))}
          </datalist>
        </span>
        <span>Opacity</span>
        <span className="opacity-field">
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={pct}
            aria-label="Viewport opacity"
            aria-valuetext={`${pct}%`}
            title="How strongly the view is drawn: lower it to trace over another view"
            onChange={(e) => setPct(Number(e.target.value))}
            onPointerUp={(e) => commitOpacity(Number((e.target as HTMLInputElement).value))}
            onKeyUp={(e) => commitOpacity(Number((e.target as HTMLInputElement).value))}
            onBlur={(e) => commitOpacity(Number(e.target.value))}
            onKeyDown={(e) => e.stopPropagation()}
          />
          <output aria-hidden="true">{pct}%</output>
        </span>
      </div>
      {scaleErr && (
        <p className="hint bad" role="alert">
          {scaleErr}
        </p>
      )}
      <div className="row gap">
        <button className="btn small ghost" onClick={onDone} title="Back to the page (Esc)">
          Done
        </button>
        <button className="btn small danger" title="Take this view off the page (Undo brings it back)" onClick={() => void run({ cmd: "sheet_item_delete", ids: [v.id] }).then(onDone)}>
          Delete view
        </button>
      </div>
    </div>
  );
}

const ALIGNS: [Align, string][] = [
  ["left", "Left"],
  ["center", "Centre"],
  ["right", "Right"],
];
function AlignPick({ value, label, onPick }: { value: Align; label: string; onPick: (a: Align) => void }) {
  return (
    <select aria-label={label} value={value} onChange={(e) => onPick(e.target.value as Align)}>
      {ALIGNS.map(([a, l]) => (
        <option key={a} value={a}>
          {l}
        </option>
      ))}
    </select>
  );
}
const BORDERS: [Border, string][] = [
  ["grid", "Grid (every cell boxed)"],
  ["rules", "Rules (a line under each row)"],
  ["outer", "Outline only"],
  ["none", "None"],
];

/** A table's settings: title, rows and columns, header, borders, text, widths, alignment; a drawing index link. */
function TableFields({ t, run }: { t: TableItem; run: Run }) {
  const rows = t.layout?.rows ?? t.rows;
  const nr = rows.length;
  const nc = Math.max(1, ...rows.map((r) => r.length));
  const linked = !!t.source;
  const widths = (t.layout ? t.layout.xs.slice(1).map((x, i) => x - t.layout!.xs[i]) : (t.cols_mm ?? [])).map((w) => Math.round(w * 10) / 10);
  const allAlign = (t.align ?? []).length && (t.align ?? []).every((a) => a === t.align![0]) && (t.align ?? []).length >= nc ? t.align![0] : "left";
  return (
    <>
      <Field label="Title" value={t.title ?? ""} placeholder="(no title)" hint="A caption above the table, like DOOR SCHEDULE" onCommit={(v) => void run({ cmd: "sheet_table", id: t.id, title: v })} />
      <span>Rows</span>
      <span className="row gap">
        <span aria-live="polite">{nr}</span>
        <button className="btn small ghost" disabled={linked} title="Add a row at the bottom" aria-label="Add a row" onClick={() => void run({ cmd: "sheet_table", id: t.id, insert_rows: { at: nr, n: 1 } })}>
          +
        </button>
        <button className="btn small ghost" disabled={linked || nr <= 1} title="Remove the bottom row" aria-label="Remove the last row" onClick={() => void run({ cmd: "sheet_table", id: t.id, delete_rows: { at: nr - 1, n: 1 } })}>
          −
        </button>
      </span>
      <span>Columns</span>
      <span className="row gap">
        <span aria-live="polite">{nc}</span>
        <button className="btn small ghost" disabled={linked} title="Add a column at the right" aria-label="Add a column" onClick={() => void run({ cmd: "sheet_table", id: t.id, insert_cols: { at: nc, n: 1 } })}>
          +
        </button>
        <button className="btn small ghost" disabled={linked || nc <= 1} title="Remove the right-hand column" aria-label="Remove the last column" onClick={() => void run({ cmd: "sheet_table", id: t.id, delete_cols: { at: nc - 1, n: 1 } })}>
          −
        </button>
      </span>
      <span>Header</span>
      <select aria-label="Header rows" value={String(t.header_rows ?? 1)} onChange={(e) => void run({ cmd: "sheet_table", id: t.id, header_rows: Number(e.target.value) })} title="Rows at the top printed bold, with a heavy rule under them">
        <option value="0">No header row</option>
        <option value="1">1 header row</option>
        <option value="2">2 header rows</option>
      </select>
      <span>First column</span>
      <label className="field check">
        <input type="checkbox" checked={(t.header_cols ?? 0) > 0} onChange={(e) => void run({ cmd: "sheet_table", id: t.id, header_cols: e.target.checked ? 1 : 0 })} />
        <span>Bold (heads its row)</span>
      </label>
      <span>Borders</span>
      <select aria-label="Table borders" value={t.border ?? "grid"} onChange={(e) => void run({ cmd: "sheet_table", id: t.id, border: e.target.value })}>
        {BORDERS.map(([b, l]) => (
          <option key={b} value={b}>
            {l}
          </option>
        ))}
      </select>
      <Field label="Text (mm)" value={String(t.text_mm ?? 2.5)} hint="The cells' capital letters' height on paper, 1 to 50 mm" onCommit={(v) => Number(v) > 0 && void run({ cmd: "sheet_table", id: t.id, text_mm: Number(v) })} />
      <Field
        label="Widths (mm)"
        value={widths.join(", ")}
        hint="Each column's width, comma separated; 0 or 'auto' fits the column to its text"
        onCommit={(v) => {
          const ws = v.split(/[,\s]+/).filter(Boolean).map((x) => (/^auto$/i.test(x) ? 0 : Number(x)));
          if (ws.every((x) => Number.isFinite(x) && x >= 0)) void run({ cmd: "sheet_table", id: t.id, cols_mm: ws });
        }}
      />
      <span>Align</span>
      <AlignPick value={allAlign as Align} label="Column alignment (all columns)" onPick={(a) => void run({ cmd: "sheet_table", id: t.id, align: Array(nc).fill(a) })} />
      <span>Fill</span>
      {linked ? (
        <button className="btn small ghost" title="Stop filling it from the sheets; it keeps what it shows now, and you can type in it" onClick={() => void run({ cmd: "sheet_table", id: t.id, source: "" })}>
          Unlink drawing index
        </button>
      ) : (
        <button className="btn small ghost" title="Fill it with every sheet's number and name, kept up to date (a drawing index)" onClick={() => void run({ cmd: "sheet_table", id: t.id, source: "sheets" })}>
          Drawing index
        </button>
      )}
      <span />
      <span className="hint">Double-click a cell to type in it. Paste a grid copied from a spreadsheet to fill it from the cell you clicked.</span>
    </>
  );
}

const WEIGHTS: [number, string][] = [
  [0.13, "Hair 0.13 mm"],
  [0.18, "Thin 0.18 mm"],
  [0.25, "Medium 0.25 mm"],
  [0.35, "Heavy 0.35 mm"],
  [0.5, "Border 0.5 mm"],
  [0.7, "Bold 0.7 mm"],
];

/** A picked line, text or dimension: what it is, its few settings, and Delete. */
function ItemInspector({ it, run, onDone }: { it: PaperItem; run: Run; onDone: () => void }) {
  const title = isShape(it) ? (it.kind === "rect" ? "Rectangle" : it.kind === "arc" ? "Arc" : it.kind === "polyline" ? "Polyline" : "Line") : isText(it) ? (it.box_mm ? "Text block" : "Text") : isTable(it) ? "Table" : "Dimension";
  return (
    <div className="section layout-inspector" aria-label={title}>
      <div className="section-title">{title}</div>
      <p className="hint">{describe(it)}. Drag it on the paper to move it; arrow keys nudge it.</p>
      <div className="kv">
        {isShape(it) && (
          <>
            <span>Weight</span>
            <select aria-label="Line weight" value={String(it.pen?.w_mm ?? 0.25)} onChange={(e) => void run({ cmd: "sheet_line", id: it.id, w_mm: Number(e.target.value) })} title="The pen's width on paper">
              {!WEIGHTS.some(([w]) => w === (it.pen?.w_mm ?? 0.25)) && <option value={String(it.pen?.w_mm)}>{`${it.pen?.w_mm} mm`}</option>}
              {WEIGHTS.map(([w, l]) => (
                <option key={w} value={String(w)}>
                  {l}
                </option>
              ))}
            </select>
            <span>Line</span>
            <select aria-label="Line style" value={it.pen?.dash_mm?.length ? "dashed" : "solid"} onChange={(e) => void run({ cmd: "sheet_line", id: it.id, dash_mm: e.target.value === "dashed" ? [3, 1.5] : [] })}>
              <option value="solid">Solid</option>
              <option value="dashed">Dashed</option>
            </select>
          </>
        )}
        {isText(it) && (
          <>
            <Field label="Text" multiline value={it.paragraphs.join("\n")} hint="The words, one line per line" onCommit={(t) => t.trim() && void run({ cmd: "sheet_text", id: it.id, text: t })} />
            <Field label="Height (mm)" value={String(it.height_mm ?? 3.5)} hint="Capital letters' height on paper, 1 to 50 mm" onCommit={(t) => Number(t) > 0 && void run({ cmd: "sheet_text", id: it.id, height_mm: Number(t) })} />
            <Field label="Width (mm)" value={it.box_mm ? String(it.box_mm[0]) : ""} placeholder="(one line per line)" hint="A text block's width: its paragraphs wrap to it. Empty or 0: plain text" onCommit={(t) => void run({ cmd: "sheet_text", id: it.id, width_mm: Number(t) || 0 })} />
            <span>Align</span>
            <AlignPick value={it.align ?? "left"} label="Text alignment" onPick={(a) => void run({ cmd: "sheet_text", id: it.id, align: a })} />
          </>
        )}
        {isTable(it) && <TableFields t={it} run={run} />}
        {isDim(it) && (
          <>
            <span>Measures</span>
            <span>{(it.measured?.spans ?? []).map((x) => x.text).join(" + ") || "?"}</span>
            <span>Mode</span>
            <span>{it.mode ?? "aligned"}</span>
          </>
        )}
      </div>
      <div className="row gap">
        <button className="btn small ghost" onClick={onDone} title="Back to the page (Esc)">
          Done
        </button>
        <button className="btn small danger" title="Take it off the page (Undo brings it back)" onClick={() => void run({ cmd: "sheet_item_delete", ids: [it.id] }).then(onDone)}>
          Delete
        </button>
      </div>
    </div>
  );
}

function PageInspector({ s, scene, run }: { s: SheetInfo; scene: Scene; run: Run }) {
  const [adding, setAdding] = useState("plan");
  const fields = s.fields ?? {};
  const setField = (k: string, v: string) => void run({ cmd: "sheet_edit", sheet: s.id, fields: { [k]: v.trim() ? v : null } });
  const addView = async () => {
    // a perspective 3D view has no scale; everything else is drawn to 1:50
    const label = adding === "plan" ? "Plan" : adding === "iso" ? "3D view" : adding === "top" ? "Top" : STANDARD_VIEWS.includes(adding) ? `${adding[0].toUpperCase()}${adding.slice(1)} elevation` : adding;
    await run({ cmd: "viewport_add", sheet: s.id, ...viewArgs(adding), ...(adding === "iso" || !STANDARD_VIEWS.includes(adding) && adding !== "plan" ? {} : { scale: 50 }), label });
  };
  const n = viewportsOf(s).length;
  return (
    <div className="section layout-inspector" aria-label="Page">
      <div className="section-title">Page</div>
      <div className="kv">
        <Field label="Name" value={s.name} onCommit={(t) => t.trim() && void run({ cmd: "sheet_edit", sheet: s.id, name: t.trim() })} />
        <Field label="Number" value={s.number} placeholder="A-101" hint="The sheet number in the title block" onCommit={(t) => void run({ cmd: "sheet_edit", sheet: s.id, number: t.trim() })} />
        <span>Paper</span>
        <select aria-label="Paper size" value={paperId(s.paper)} onChange={(e) => e.target.value && void run({ cmd: "sheet_edit", sheet: s.id, paper: e.target.value })} title="The sheet's size (landscape); views keep their place and scale">
          {!paperId(s.paper) && <option value="">{`${Math.round(s.paper.w_mm)} × ${Math.round(s.paper.h_mm)} mm`}</option>}
          {PAPERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </div>
      <div className="section-title sub">Title block</div>
      <div className="kv">
        <Field label="Project" value={fields.project ?? ""} placeholder="Project name" hint="Printed large in the title block" onCommit={(t) => setField("project", t)} />
        <Field label="Notes" value={fields.notes ?? ""} multiline placeholder="General notes, one per line" hint="Printed in the title block's notes" onCommit={(t) => setField("notes", t)} />
        <span>Blueprint</span>
        <label className="field check">
          <input type="checkbox" checked={fields.blueprint === "true"} onChange={(e) => setField("blueprint", e.target.checked ? "true" : "")} />
          <span>White on blue</span>
        </label>
      </div>
      <div className="section-title sub">Views on this page ({n})</div>
      {n === 0 && <p className="hint">Nothing on this page yet. Add a view of the model, then click it on the paper to change it.</p>}
      <div className="row gap add-view">
        <select aria-label="View to add" value={adding} onChange={(e) => setAdding(e.target.value)}>
          <option value="plan">Floor plan, 1:50</option>
          {STANDARD_VIEWS.map((v) => (
            <option key={v} value={v}>
              {v === "iso" ? "3D view (iso)" : `${v[0].toUpperCase()}${v.slice(1)}${v === "top" ? " view" : " elevation"}, 1:50`}
            </option>
          ))}
          {scene.views.map((v) => (
            <option key={`s:${v.name}`} value={v.name}>
              Scene: {v.name}
            </option>
          ))}
        </select>
        <button className="btn small" onClick={() => void addView()} title="Put this view of the model on the page">
          Add view
        </button>
      </div>
    </div>
  );
}

export function LayoutInspector({ page, viewport, item, scene, run, setViewport, setItem }: { page: SheetInfo | undefined; viewport: number | null; item?: number | null; scene: Scene; run: Run; setViewport: (id: number | null) => void; setItem?: (id: number | null) => void }) {
  if (!page)
    return (
      <div className="section hint">
        <p>Pages are sheets of paper with views of your model on them, ready to print or send as PDF. Add one on the left.</p>
      </div>
    );
  const v = viewportsOf(page).find((x) => x.id === viewport);
  const it = item != null ? paperItems(page).find((x) => x.id === item) : undefined;
  if (!v && it) return <ItemInspector key={it.id} it={it} run={run} onDone={() => setItem?.(null)} />;
  return v ? <ViewportInspector key={v.id} v={v} scene={scene} run={run} onDone={() => setViewport(null)} /> : <PageInspector s={page} scene={scene} run={run} />;
}
