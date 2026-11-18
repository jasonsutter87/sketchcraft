// Layout's inspector (the right panel): the page you're on (name, number, paper, title block) and, when you pick a
// viewport on it, that viewport (label, what it shows, scale, opacity). Each change is one engine command, one undo
// step; a slider sends its value when you let go.

import { useEffect, useState } from "react";
import type { Reply, Scene } from "../engine";
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

export function LayoutInspector({ page, viewport, scene, run, setViewport }: { page: SheetInfo | undefined; viewport: number | null; scene: Scene; run: Run; setViewport: (id: number | null) => void }) {
  if (!page)
    return (
      <div className="section hint">
        <p>Pages are sheets of paper with views of your model on them, ready to print or send as PDF. Add one on the left.</p>
      </div>
    );
  const v = viewportsOf(page).find((x) => x.id === viewport);
  return v ? <ViewportInspector key={v.id} v={v} scene={scene} run={run} onDone={() => setViewport(null)} /> : <PageInspector s={page} scene={scene} run={run} />;
}
