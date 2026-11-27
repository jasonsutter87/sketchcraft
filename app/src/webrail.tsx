// The Web layout's right edge: a rail of panel tiles, each with a pill label,
// like SketchUp for Web's tray. Picking a tile slides its panel out; picking
// the open one puts it away. The icons are our own drawings.

import type { ReactNode } from "react";
import type { SideTab } from "./sidetabs";

const svg = (body: ReactNode) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {body}
  </svg>
);

/** One drawing per panel: thin ink outlines on a 24px grid. */
export const RAIL_ICONS: Record<SideTab, () => ReactNode> = {
  properties: () =>
    svg(
      <>
        <path d="M12 3.5 19.5 7.8v8.4L12 20.5 4.5 16.2V7.8Z" />
        <circle cx="12" cy="12" r="3.2" />
        <path d="M12 10.9v2.6M12 9.6v.1" />
      </>,
    ),
  outliner: () =>
    svg(
      <>
        <rect x="4" y="4.5" width="5" height="4" rx=".6" />
        <rect x="10" y="10" width="5" height="4" rx=".6" />
        <rect x="10" y="15.5" width="5" height="4" rx=".6" />
        <path d="M6.5 8.5v9h3.5M6.5 12h3.5M11 6.5h9" />
      </>,
    ),
  library: () =>
    svg(
      <>
        <path d="M12 3.5 16.5 6v5L12 13.5 7.5 11V6Z" />
        <path d="M7.5 11 3.5 13.3v5L8 20.8l4-2.3v-5M16.5 11l4 2.3v5L16 20.8l-4-2.3" />
      </>,
    ),
  views: () =>
    svg(
      <>
        <rect x="3.5" y="6" width="17" height="12" rx="1.5" />
        <path d="m10 9.5 5 2.5-5 2.5Z" />
      </>,
    ),
  pages: () =>
    svg(
      <>
        <rect x="5.5" y="3.5" width="13" height="17" rx="1" />
        <path d="M8.5 8h7M8.5 11h7M8.5 14h4" />
      </>,
    ),
  ai: () =>
    svg(
      <>
        <path d="M11 4.5c.6 3.6 2.4 5.4 6 6-3.6.6-5.4 2.4-6 6-.6-3.6-2.4-5.4-6-6 3.6-.6 5.4-2.4 6-6Z" />
        <path d="M18 3.5v3M16.5 5h3" />
      </>,
    ),
};

export function WebRail({ tabs, cur, open, label, onPick }: { tabs: readonly SideTab[]; cur: SideTab; open: boolean; label: (t: SideTab) => string; onPick: (t: SideTab) => void }) {
  // the open panel's own tile moves into its header, so the rail leaves it out
  const shown = open ? tabs.filter((t) => t !== cur) : tabs;
  return (
    <nav className="web-rail" aria-label="Panels">
      {shown.map((t) => (
        <button key={t} className="web-tile" onClick={() => onPick(t)} aria-label={label(t)}>
          <span className="web-pill">{label(t)}</span>
          <span className="web-icon">{RAIL_ICONS[t]()}</span>
        </button>
      ))}
    </nav>
  );
}
