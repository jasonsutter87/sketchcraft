// Small stroke icons, drawn for SketchCraft (24px grid, 1.6 stroke).
import type { ReactNode } from "react";

const Svg = ({ children, size = 20 }: { children: ReactNode; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
    {children}
  </svg>
);

export const Logo = () => (
  <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
    <rect x="2" y="2" width="20" height="20" rx="6" fill="#6B5CFF" />
    <path d="M7 17.5h10" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" />
    <path d="M8.5 17.5V11h7v6.5" stroke="#fff" strokeWidth="1.8" strokeLinejoin="round" />
    <path d="M12 9V5.5m0 0-2 2m2-2 2 2" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
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
