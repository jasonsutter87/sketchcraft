// The middle of the Layout workspace: the page you're on, drawn on paper, or (with no pages yet) what Layout is for and
// a button to start. It covers the 3D view, which stays loaded underneath (camera, selection and all), so switching
// back is instant.

import { PaperView } from "./PaperView";
import { viewportsOf } from "./pages";
import type { LayoutState } from "./uselayout";

export function LayoutStage({ L }: { L: LayoutState }) {
  const p = L.page;
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
      />
    </div>
  );
}
