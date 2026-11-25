// The Classic tray: collapsible panels stacked down the right side. The panels themselves are SketchCraft's own
// (passed in by App); this only draws the headers and keeps what is open.
import type { ReactNode } from "react";
import { TRAY_ORDER, TRAY_TITLES, type TrayPanel } from "./tray";

export function ClassicTray({ panels, open, onToggle }: { panels: Partial<Record<TrayPanel, ReactNode>>; open: Record<TrayPanel, boolean>; onToggle: (id: TrayPanel) => void }) {
  return (
    <div className="tray" role="region" aria-label="Tray">
      {TRAY_ORDER.filter((id) => panels[id] != null).map((id) => (
        <section key={id} className={`tray-panel${open[id] ? " open" : ""}`} data-panel={id}>
          <h3>
            <button id={`tray-h-${id}`} className="tray-head" aria-expanded={open[id]} aria-controls={`tray-b-${id}`} onClick={() => onToggle(id)}>
              <i className="tray-caret" aria-hidden="true" />
              {TRAY_TITLES[id]}
            </button>
          </h3>
          {open[id] && (
            <div id={`tray-b-${id}`} role="region" aria-labelledby={`tray-h-${id}`} className="tray-body">
              {panels[id]}
            </div>
          )}
        </section>
      ))}
    </div>
  );
}
