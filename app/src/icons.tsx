// Small stroke icons, drawn for SketchCraft (24px grid, 1.6 stroke).
import type { ReactNode } from "react";

const Svg = ({ children, size = 20 }: { children: ReactNode; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
    {children}
  </svg>
);

/** The beaver (brand/beaver.py's small cut), on its violet tile. */
export const Logo = () => (
  <svg width="24" height="24" viewBox="0 0 32 32" role="img" aria-label="SketchCraft">
    <rect width="32" height="32" rx="7" fill="#6f4df0"/><path d="M4.2 14.2C4 10.2 7.6 7.2 13.6 6.4C20.6 5.6 26.4 8.6 28.2 15L30.5 32H8.2L8.2 26.6C6 25.2 4.6 22.6 4.4 19Z" fill="#f8f3e6"/><circle cx="21.6" cy="7.6" r="1.7" fill="#15131c"/><circle cx="12.4" cy="11" r="1.2" fill="#15131c"/><path d="M2.6 12.6C2.6 10.4 4.6 9.4 6.8 9.8C8.6 10.2 8.8 12.6 7.6 13.8C6 15.2 2.6 15 2.6 12.6Z" fill="#15131c"/><path d="M5.5 27.6L22.5 14.4L24.6 17L7.8 30Z" fill="#f2c230" stroke="#15131c" strokeWidth=".9" strokeLinejoin="round"/><path d="M3.4 30.6L5.5 27.6L7.8 30Z" fill="#f1dcb4" stroke="#15131c" strokeWidth=".7" strokeLinejoin="round"/><path d="M4.8 16H10.4L10.6 22.4H5.4Z" fill="#e8892b" stroke="#15131c" strokeWidth="1" strokeLinejoin="round"/><path d="M7.7 16.1V22.3" stroke="#15131c" strokeWidth=".9"/>
  </svg>
);

export const Icons = {
  select: () => (
    <Svg>
      <path d="M5 3.5 18 10l-6 1.6L9.6 18z" />
    </Svg>
  ),
  line: () => (
    <Svg>
      <path d="M5 19 19 5" />
      <circle cx="5" cy="19" r="1.6" />
      <circle cx="19" cy="5" r="1.6" />
    </Svg>
  ),
  arc: () => (
    <Svg>
      <path d="M4 18c0-7 4-12 8-12s8 5 8 12" />
      <circle cx="4" cy="18" r="1.4" />
      <circle cx="20" cy="18" r="1.4" />
    </Svg>
  ),
  rect: () => (
    <Svg>
      <rect x="4" y="6" width="16" height="12" rx="1.5" />
    </Svg>
  ),
  circle: () => (
    <Svg>
      <circle cx="12" cy="12" r="7.5" />
    </Svg>
  ),
  polygon: () => (
    <Svg>
      <path d="M12 4.5 19 9v6l-7 4.5L5 15V9z" />
    </Svg>
  ),
  push: () => (
    <Svg>
      <path d="M4 15.5 12 19l8-3.5L12 12z" />
      <path d="M12 12V4m0 0-2.5 2.5M12 4l2.5 2.5" />
    </Svg>
  ),
  offset: () => (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <rect x="7.5" y="9" width="9" height="6" rx="0.5" strokeDasharray="2 2" />
    </Svg>
  ),
  followme: () => (
    <Svg>
      <path d="M4 18h8a6 6 0 0 0 6-6V5" />
      <rect x="2.5" y="15" width="4" height="6" rx="0.8" />
    </Svg>
  ),
  move: () => (
    <Svg>
      <path d="M12 3v18M3 12h18" />
      <path d="m12 3-2.5 2.5M12 3l2.5 2.5M12 21l-2.5-2.5M12 21l2.5-2.5M3 12l2.5-2.5M3 12l2.5 2.5M21 12l-2.5-2.5M21 12l-2.5 2.5" />
    </Svg>
  ),
  rotate: () => (
    <Svg>
      <path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3" />
      <path d="M19.5 4.5v4h-4" />
    </Svg>
  ),
  scale: () => (
    <Svg>
      <rect x="4" y="10" width="10" height="10" rx="1" />
      <path d="M10 4h10v10" strokeDasharray="2 2" />
      <path d="m13 11 7-7m0 0h-4m4 0v4" />
    </Svg>
  ),
  tape: () => (
    <Svg>
      <circle cx="9" cy="12" r="5.5" />
      <circle cx="9" cy="12" r="1.5" />
      <path d="M14.5 12H21v3h-6.5" />
    </Svg>
  ),
  dimension: () => (
    <Svg>
      <path d="M4 7v10M20 7v10M4 12h16" />
      <path d="m2.8 13.2 2.4-2.4M18.8 13.2l2.4-2.4" />
    </Svg>
  ),
  text: () => (
    <Svg>
      <path d="M5 6.5V5h14v1.5M12 5v14M9.5 19h5" />
    </Svg>
  ),
  wall: () => (
    <Svg>
      <path d="M3 19V9l6-3v10M9 16l12-3V7L9 6" />
      <path d="M3 19l6-3M21 13v-0" />
    </Svg>
  ),
  stairs: () => (
    <Svg>
      <path d="M3 20h4v-4h4v-4h4V8h4V4" />
      <path d="M3 20V4" strokeDasharray="1.5 2" />
    </Svg>
  ),
  roof: () => (
    <Svg>
      <path d="M2.5 12 12 4.5l9.5 7.5" />
      <path d="M5.5 10v9.5h13V10" />
    </Svg>
  ),
  column: () => (
    <Svg>
      <path d="M7 4h10M7 20h10M9 4v16M15 4v16" />
    </Svg>
  ),
  paint: () => (
    <Svg>
      <path d="M14.5 4.5 19.5 9.5 11 18l-5-5z" />
      <path d="M6 13l-1.5 5.5L10 17" />
    </Svg>
  ),
  erase: () => (
    <Svg>
      <path d="M8 19h11" />
      <path d="M4.6 13.6 13.6 4.6a1.5 1.5 0 0 1 2.1 0l3.7 3.7a1.5 1.5 0 0 1 0 2.1L11 19H8.5l-3.9-3.9a1 1 0 0 1 0-1.5z" />
    </Svg>
  ),
  orbit: () => (
    <Svg>
      <circle cx="12" cy="12" r="3" />
      <path d="M3.5 12c0-2.5 3.8-4.5 8.5-4.5s8.5 2 8.5 4.5-3.8 4.5-8.5 4.5" />
      <path d="m10 14.5 2 2-2 2" />
    </Svg>
  ),
  smoove: () => (
    <Svg>
      <path d="M2.5 18c3-0.5 5-8 9.5-8s6.5 7.5 9.5 8" />
      <path d="M12 3v5m-2-2 2 2 2-2" />
    </Svg>
  ),
  stamp: () => (
    <Svg>
      <path d="M2.5 18c2-0.4 3.5-3 5.5-3h8c2 0 3.5 2.6 5.5 3" />
      <path d="M8 15V9h8v6" />
      <path d="M12 3v4" />
    </Svg>
  ),
  drape: () => (
    <Svg>
      <path d="M2.5 17c3-0.5 5-6 9.5-6s6.5 5.5 9.5 6" />
      <path d="M4 5h16" strokeDasharray="2 2" />
      <path d="M8 6v6m8-6v5" />
    </Svg>
  ),
  flipedge: () => (
    <Svg>
      <path d="M4 18 12 5l8 13z" />
      <path d="M4 18 20 5" strokeDasharray="2 2" />
    </Svg>
  ),
  terrain: () => (
    <Svg>
      <path d="M2.5 18 8 10l4 4 3-5 6.5 9z" />
      <path d="M8 10l2 8M15 9l-2 9" />
    </Svg>
  ),
  pan: () => (
    <Svg>
      <path d="M8 13V6.5a1.5 1.5 0 0 1 3 0V12" />
      <path d="M11 11V5a1.5 1.5 0 0 1 3 0v6" />
      <path d="M14 11V6.5a1.5 1.5 0 0 1 3 0V14a6 6 0 0 1-6 6h-.6a5 5 0 0 1-4-2l-2.8-3.7a1.5 1.5 0 0 1 2.3-1.9L8 14" />
    </Svg>
  ),
  zoomwindow: () => (
    <Svg>
      <rect x="3.5" y="3.5" width="11" height="9" rx="1.5" strokeDasharray="2.5 2" />
      <circle cx="14" cy="14" r="4" />
      <path d="m17 17 3.5 3.5" />
    </Svg>
  ),
  position: () => (
    <Svg>
      <circle cx="12" cy="5.5" r="2.2" />
      <path d="M12 8v7M8.5 11h7M12 15l-2.5 5M12 15l2.5 5" />
      <path d="M5 20.5h14" />
    </Svg>
  ),
  lookaround: () => (
    <Svg>
      <path d="M2.5 12S6 6 12 6s9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6z" />
      <circle cx="12" cy="12" r="2.6" />
      <path d="M4 4.5c2-1.6 4.6-2.5 8-2.5M20 4.5c-2-1.6-4.6-2.5-8-2.5" />
    </Svg>
  ),
  walk: () => (
    <Svg>
      <path d="M8 4.5c1.6 0 2.5 1.6 2.3 4-.2 2.3-1.3 3.5-2.6 3.3-1.3-.2-1.9-1.8-1.7-3.9C6.2 5.8 6.8 4.5 8 4.5z" />
      <path d="M15.8 9c1.4-.2 2.5 1.2 2.6 3.4.1 2.3-.8 3.7-2.1 3.8-1.3.1-2.2-1.3-2.3-3.4-.1-2.2.5-3.6 1.8-3.8z" />
      <path d="M6.6 14.2l2.9.4-.3 2.3c-.2 1-1 1.6-1.9 1.4-.9-.2-1.3-1-1.1-1.9zM14.6 18.8l2.9-.2.2 2.3c0 .9-.6 1.6-1.5 1.6-.9 0-1.5-.6-1.5-1.5z" />
    </Svg>
  ),
  zoom: () => (
    <Svg>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="m15 15 5 5" />
      <path d="M8 10.5h5M10.5 8v5" />
    </Svg>
  ),
  section: () => (
    <Svg>
      <path d="M4 7.5 12 4l8 3.5v9L12 20l-8-3.5z" />
      <path d="M2.5 13h19" strokeDasharray="2.5 2" />
    </Svg>
  ),
  place: () => (
    <Svg>
      <path d="M12 21s-6-5.6-6-10.5a6 6 0 0 1 12 0C18 15.4 12 21 12 21z" />
      <circle cx="12" cy="10.5" r="2" />
    </Svg>
  ),
  undo: () => (
    <Svg size={18}>
      <path d="M9 7 4.5 11.5 9 16" />
      <path d="M5 11.5h9a5 5 0 0 1 0 10h-2" />
    </Svg>
  ),
  redo: () => (
    <Svg size={18}>
      <path d="m15 7 4.5 4.5L15 16" />
      <path d="M19 11.5h-9a5 5 0 0 0 0 10h2" />
    </Svg>
  ),
  layer: () => (
    <Svg size={16}>
      <path d="M12 4 3.5 8.5 12 13l8.5-4.5z" />
      <path d="m3.5 13 8.5 4.5 8.5-4.5" />
    </Svg>
  ),
  cube: () => (
    <Svg size={16}>
      <path d="M12 3.5 19.5 7.5v9L12 20.5 4.5 16.5v-9z" />
      <path d="M4.5 7.5 12 11.5l7.5-4M12 11.5v9" />
    </Svg>
  ),
  component: () => (
    <Svg size={16}>
      <path d="M12 3.5 19.5 7.5v9L12 20.5 4.5 16.5v-9z" />
      <path d="m9 12 3 1.8 3-1.8" />
    </Svg>
  ),
  eye: () => (
    <Svg size={15}>
      <path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" />
      <circle cx="12" cy="12" r="2.8" />
    </Svg>
  ),
  eyeOff: () => (
    <Svg size={15}>
      <path d="M4 4l16 16" />
      <path d="M9.9 5.8A9.7 9.7 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-2.7 3.4M6.5 7.6C4 9.4 2.5 12 2.5 12S6 18.5 12 18.5c1.4 0 2.6-.3 3.7-.8" />
    </Svg>
  ),
  lock: () => (
    <Svg size={14}>
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" />
    </Svg>
  ),
  unlock: () => (
    <Svg size={14}>
      <rect x="5" y="11" width="14" height="9" rx="2" />
      <path d="M8 11V8a4 4 0 0 1 7.5-2" />
    </Svg>
  ),
  caret: () => (
    <Svg size={12}>
      <path d="M9 6l6 6-6 6" />
    </Svg>
  ),
  solo: () => (
    <Svg size={14}>
      <circle cx="12" cy="12" r="7.5" />
      <circle cx="12" cy="12" r="2.5" />
    </Svg>
  ),
  camera: () => (
    <Svg size={16}>
      <rect x="3.5" y="7" width="17" height="12" rx="2" />
      <circle cx="12" cy="13" r="3.2" />
      <path d="M9 7l1.2-2.2h3.6L15 7" />
    </Svg>
  ),
  export: () => (
    <Svg size={16}>
      <path d="M12 15V4m0 0L8 8m4-4 4 4" />
      <path d="M5 14v4.5A1.5 1.5 0 0 0 6.5 20h11a1.5 1.5 0 0 0 1.5-1.5V14" />
    </Svg>
  ),
  import: () => (
    <Svg size={16}>
      <path d="M12 4v11m0 0-4-4m4 4 4-4" />
      <path d="M5 14v4.5A1.5 1.5 0 0 0 6.5 20h11a1.5 1.5 0 0 0 1.5-1.5V14" />
    </Svg>
  ),
  plus: () => (
    <Svg size={15}>
      <path d="M12 5v14M5 12h14" />
    </Svg>
  ),
  trash: () => (
    <Svg size={14}>
      <path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5" />
    </Svg>
  ),
};
