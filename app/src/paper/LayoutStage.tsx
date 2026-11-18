// The middle of the Layout workspace: the page you're on, drawn on paper, with the sheet editor's tool bar (#9) and
// its options (grid, snapping, how a dimension measures), or (with no pages yet) what Layout is for and a button to
// start. It covers the 3D view, which stays loaded underneath (camera, selection and all), so switching back is
// instant.

import { useEffect, useState } from "react";
import { PaperView } from "./PaperView";
import { paperItems } from "./items";
import { viewportsOf } from "./pages";
import { GRID_DEFAULT } from "./snap";
import { PAPER_TOOL_INFO, TOOL_GROUPS, type DimChoice, type PaperTool } from "./tools";
import type { LayoutState } from "./uselayout";
import type { EditorOpts } from "./editor";

const OPTS_KEY = "sketchcraft.paper.opts";
function loadOpts(): EditorOpts {
  const d: EditorOpts = { grid: GRID_DEFAULT, gridOn: true, snapOn: true, dimChoice: "auto" };
  try {
    const v = JSON.parse(localStorage.getItem(OPTS_KEY) ?? "null");
    if (v && typeof v === "object") return { ...d, ...v, grid: Number(v.grid) > 0 && Number(v.grid) <= 100 ? Number(v.grid) : d.grid };
  } catch {}
  return d;
}

/** The paper's tools, a button each (icon, name, shortcut), in groups. */
function PaperToolbar({ tool, setTool }: { tool: PaperTool; setTool: (t: PaperTool) => void }) {
  return (
    <nav className="paper-tools" role="toolbar" aria-label="Sheet tools" aria-orientation="vertical" onPointerDown={(e) => e.stopPropagation()}>
      {TOOL_GROUPS.map((g, gi) => (
        <div className="paper-tool-group" key={gi}>
          {g.map((t) => {
            const info = PAPER_TOOL_INFO[t];
            return (
              <button
                key={t}
                className={`paper-tool ${tool === t ? "on" : ""}`}
                aria-label={info.name}
                aria-pressed={tool === t}
                aria-keyshortcuts={info.key.toUpperCase()}
                title={`${info.name} (${info.key.toUpperCase()}): ${info.tip}`}
                onClick={() => setTool(t)}
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d={info.icon} />
                </svg>
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}

/** The tool's options: the grid (on, size), snapping, and for dimensions which way they measure. */
function PaperOptions({ tool, opts, setOpts }: { tool: PaperTool; opts: EditorOpts; setOpts: (o: EditorOpts) => void }) {
  const [gridText, setGridText] = useState(String(opts.grid));
  useEffect(() => setGridText(String(opts.grid)), [opts.grid]);
  const modes: [DimChoice, string, string][] = [
    ["auto", "Auto", "Horizontal or vertical, whichever way the points spread"],
    ["horizontal", "Horizontal", "Measure across the page"],
    ["vertical", "Vertical", "Measure up the page"],
    ["aligned", "Aligned", "Measure along the line between the points"],
  ];
  return (
    <div className="paper-opts" role="group" aria-label="Snapping and tool options" onPointerDown={(e) => e.stopPropagation()}>
      <label className="check" title="Things you place or drag snap to the page, its margins, other frames and points">
        <input type="checkbox" checked={opts.snapOn} onChange={(e) => setOpts({ ...opts, snapOn: e.target.checked })} />
        <span>Snap</span>
      </label>
      <label className="check" title="Show the paper grid and snap to it">
        <input type="checkbox" checked={opts.gridOn} onChange={(e) => setOpts({ ...opts, gridOn: e.target.checked })} />
        <span>Grid</span>
      </label>
      <input
        className="grid-size"
        aria-label="Grid spacing, millimetres"
        title="Grid spacing on the paper, mm"
        value={gridText}
        inputMode="decimal"
        onChange={(e) => setGridText(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
        onBlur={() => {
          const v = Number(gridText);
          if (v > 0 && v <= 100) setOpts({ ...opts, grid: v });
          else setGridText(String(opts.grid));
        }}
      />
      <span className="unit">mm</span>
      {tool === "dimension" && (
        <span className="dim-modes" role="radiogroup" aria-label="How the dimension measures">
          {modes.map(([id, label, tip]) => (
            <button key={id} role="radio" aria-checked={opts.dimChoice === id} className={opts.dimChoice === id ? "on" : ""} title={tip} onClick={() => setOpts({ ...opts, dimChoice: id })}>
              {label}
            </button>
          ))}
        </span>
      )}
    </div>
  );
}

export function LayoutStage({ L }: { L: LayoutState }) {
  const p = L.page;
  const [tool, setTool] = useState<PaperTool>("select");
  const [opts, setOptsRaw] = useState<EditorOpts>(loadOpts);
  const setOpts = (o: EditorOpts) => {
    setOptsRaw(o);
    try {
      localStorage.setItem(OPTS_KEY, JSON.stringify(o));
    } catch {}
  };
  if (!p)
    return (
      <div className="layout-stage">
        {L.ready && (
          <div className="layout-empty" role="region" aria-label="No pages yet">
            <svg width="120" height="86" viewBox="0 0 120 86" aria-hidden="true">
              <rect x="4" y="4" width="112" height="78" rx="2" className="sheet" />
              <rect x="14" y="14" width="56" height="46" className="vp" />
              <rect x="76" y="14" width="30" height="22" className="vp" />
              <rect x="76" y="62" width="30" height="12" className="tb" />
            </svg>
            <h2>Lay out drawing sheets</h2>
            <p>A page is a sheet of paper with views of your model on it: plans, elevations, 3D views, to scale. They stay live: change the model and the pages follow.</p>
            <button className="btn primary" onClick={() => void L.add()}>
              + Add page
            </button>
            <p className="hint">Your 3D model stays open; switch back to Model any time.</p>
          </div>
        )}
      </div>
    );
  return (
    <div className="layout-stage">
      <PaperView
        ir={L.ir}
        paper={{ w: p.paper.w_mm, h: p.paper.h_mm }}
        viewports={viewportsOf(p)}
        picked={L.viewport}
        onPick={L.setViewport}
        drawing={L.drawing}
        error={L.irError}
        title={`Page ${p.number ? `${p.number} ` : ""}${p.name}`}
        edit={{ page: p, items: paperItems(p), tool, setTool, opts, item: L.item, setItem: L.setItem, run: L.run, scene: L.scene, say: L.say }}
      />
      <PaperToolbar tool={tool} setTool={setTool} />
      <PaperOptions tool={tool} opts={opts} setOpts={setOpts} />
    </div>
  );
}
