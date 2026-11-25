// The "Classic Color" icon set: one friendly, colourful drawing per tool, in the spirit of a familiar desktop
// modeler's tool bar but drawn from scratch for SketchCraft (own shapes, own palette). Each icon is a 24 x 24
// SVG with a dark outline, a flat fill and one accent colour, so it stays legible at tool bar size. They reach
// the tool bar through the skin mechanism (skins.ts: a skin's `icons` map, tool name -> data URL).

const INK = "#2b3440";
const BLUE = "#3a86e0";
const SKY = "#a9d4f5";
const GREEN = "#47a75a";
const LEAF = "#a7d97b";
const RED = "#dc4a40";
const ORANGE = "#f0902a";
const SAND = "#f6d36a";
const PINK = "#f08aa4";
const GREY = "#aab4c0";
const PAPER = "#fbf8ef";

const o = (fill: string, w = 1.3) => `fill="${fill}" stroke="${INK}" stroke-width="${w}" stroke-linejoin="round" stroke-linecap="round"`;
const line = (color: string, w = 2) => `fill="none" stroke="${color}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"`;
const dot = (x: number, y: number, c = RED, r = 1.9) => `<circle cx="${x}" cy="${y}" r="${r}" ${o(c, 1)}/>`;
const arrowHead = (pts: string, c: string) => `<polygon points="${pts}" ${o(c, 1)}/>`;
/** a small cube, drawn at (x, y) with size s (a unit cube is 16 wide); faces take the three fills */
const cube = (x: number, y: number, s: number, top: string, left: string, right: string) => {
  const p = (a: number, b: number) => `${(x + a * s).toFixed(1)},${(y + b * s).toFixed(1)}`;
  return (
    `<polygon points="${p(8, 0)} ${p(16, 4.5)} ${p(8, 9)} ${p(0, 4.5)}" ${o(top)}/>` +
    `<polygon points="${p(0, 4.5)} ${p(8, 9)} ${p(8, 18)} ${p(0, 13.5)}" ${o(left)}/>` +
    `<polygon points="${p(8, 9)} ${p(16, 4.5)} ${p(16, 13.5)} ${p(8, 18)}" ${o(right)}/>`
  );
};

const ART: Record<string, string> = {
  select: `<path d="M6 3 L6 18.5 L10.2 14.8 L13 20.8 L15.8 19.5 L13 13.6 L18.5 13.2 Z" ${o(BLUE)}/>`,
  line: `<path d="M4 20.5 L5.3 15.3 L16 4.6 L19.4 8 L8.7 18.7 Z" ${o(SAND)}/><path d="M4 20.5 L5.3 15.3 L8.7 18.7 Z" ${o("#f3d9b4")}/><path d="M16 4.6 L18 2.6 L21.4 6 L19.4 8 Z" ${o(PINK)}/>`,
  arc: `<path d="M4 18 Q12 1 20 18" ${line(BLUE, 2.4)}/>${dot(4, 18)}${dot(20, 18)}`,
  arccenter: `<path d="M6 18 V6" ${line(GREY, 1.4)} stroke-dasharray="2 2"/><path d="M6 6 A12 12 0 0 1 18 18" ${line(BLUE, 2.4)}/>${dot(6, 18, ORANGE)}${dot(6, 6)}${dot(18, 18)}`,
  arc3: `<path d="M3.5 18.5 Q12 -1 20.5 18.5" ${line(BLUE, 2.4)}/>${dot(3.5, 18.5)}${dot(20.5, 18.5)}${dot(12, 8.3, ORANGE)}`,
  pie: `<path d="M7 18 V5 A13 13 0 0 1 20 18 Z" ${o(ORANGE)}/>${dot(7, 18, RED, 1.6)}`,
  rrect: `<polygon points="3.5,12 11,4 20.5,10.5 13,19.5" ${o(LEAF)}/>${dot(3.5, 12, RED, 1.7)}${dot(13, 19.5, ORANGE, 1.7)}`,
  freehand: `<path d="M3 16 C5 5 8 21 11.5 12 S17 5 21 10" ${line(RED, 2.4)}/><circle cx="3" cy="16" r="1.4" fill="${INK}"/>`,
  spline: `<path d="M3 17 C8 17 7 6 12 8.5 S17 19 21 6" ${line(GREEN, 2.4)}/>${dot(3, 17, ORANGE, 1.6)}${dot(9.4, 9.6, ORANGE, 1.6)}${dot(15, 14.5, ORANGE, 1.6)}${dot(21, 6, ORANGE, 1.6)}`,
  rect: `<rect x="3.5" y="6" width="17" height="12" rx="1" ${o(SAND)}/>${dot(3.5, 6, RED, 1.5)}${dot(20.5, 18, RED, 1.5)}`,
  circle: `<circle cx="12" cy="12" r="8.5" ${o(SKY)}/><circle cx="12" cy="12" r="1.6" fill="${BLUE}"/><path d="M12 12 L18.5 7.5" ${line(BLUE, 1.4)}/>`,
  polygon: `<polygon points="12,3 19.8,7.5 19.8,16.5 12,21 4.2,16.5 4.2,7.5" ${o(LEAF)}/><circle cx="12" cy="12" r="1.5" fill="${GREEN}"/>`,
  push: cube(5.2, 8, 0.85, SAND, "#8ec0ee", "#5b9be0") + `<path d="M12 8 V4.2" ${line(RED, 2.2)}/>${arrowHead("8.6,4.6 12,1.2 15.4,4.6", RED)}`,
  offset: `<rect x="3" y="4" width="18" height="16" rx="2.5" ${o(SKY)}/><rect x="7.5" y="8" width="9" height="8" rx="1.2" fill="${PAPER}" stroke="${ORANGE}" stroke-width="1.6" stroke-dasharray="2.4 1.6"/>`,
  followme: `<path d="M5.5 7 H17 V17" ${line(INK, 5.6)}/><path d="M5.5 7 H17 V17" ${line(SKY, 3.6)}/><circle cx="5.2" cy="7" r="3" ${o(RED)}/>${arrowHead("13,16 21,16 17,22", BLUE)}`,
  move: `<path d="M12 3 V21 M3 12 H21" ${line(GREEN, 2.2)}/>${arrowHead("8.5,6 12,1.6 15.5,6", GREEN)}${arrowHead("8.5,18 12,22.4 15.5,18", GREEN)}${arrowHead("6,8.5 1.6,12 6,15.5", GREEN)}${arrowHead("18,8.5 22.4,12 18,15.5", GREEN)}<circle cx="12" cy="12" r="2.2" ${o(SAND, 1)}/>`,
  rotate: `<path d="M19 12 A7 7 0 1 1 12 5" ${line(ORANGE, 2.6)}/>${arrowHead("10.5,1 16.5,5 10.5,9", ORANGE)}${dot(12, 12, BLUE, 1.9)}`,
  scale: `<rect x="3" y="9" width="12" height="12" rx="1" ${o(SKY)}/><rect x="3" y="9" width="6" height="6" ${o(BLUE, 1)}/><path d="M10 14 L19.5 4.5" ${line(RED, 2.2)}/>${arrowHead("13.5,3 21,3 21,10.5", RED)}`,
  tape: `<rect x="2.5" y="7" width="12" height="12" rx="3" ${o(SAND)}/><circle cx="8.5" cy="13" r="2.4" ${o(PAPER, 1)}/><rect x="14.5" y="10" width="7" height="3.6" ${o(PAPER, 1)}/><path d="M17 10 V12 M19.2 10 V11.8" ${line(INK, 1)}/>`,
  protractor: `<path d="M2.5 18.5 A9.5 9.5 0 0 1 21.5 18.5 Z" ${o("#d5ecfb")}/><path d="M5.2 17 L6.2 16.4 M7 12.6 L8 13.2 M12 9.8 V11 M17 12.6 L16 13.2 M18.8 17 L17.8 16.4" ${line(INK, 1)}/><path d="M12 18.5 L17.5 8.5" ${line(RED, 2)}/>${dot(12, 18.5, ORANGE, 1.6)}`,
  axes: `<path d="M10 15 L20.5 19.5" ${line(RED, 2.2)}/><path d="M10 15 L20.5 8.5" ${line(GREEN, 2.2)}/><path d="M10 15 V2.5" ${line(BLUE, 2.2)}/>${dot(10, 15, SAND, 2)}`,
  dimension: `<path d="M3.5 5 V17 M20.5 5 V17" ${line(INK, 1.4)}/><path d="M4 12 H20" ${line(RED, 1.8)}/>${arrowHead("4,9.6 4,14.4 0.8,12", RED)}${arrowHead("20,9.6 20,14.4 23.2,12", RED)}<rect x="8.5" y="17" width="7" height="4.5" rx="1" ${o(PAPER, 1)}/>`,
  text: `<path d="M3 4 H19 L21 6 V17 H12 L8 21 V17 H3 Z" ${o(PAPER)}/><path d="M8.2 14.2 L11.5 6.6 L14.8 14.2 M9.4 11.6 H13.6" ${line(BLUE, 1.9)}/>`,
  paint: `<path d="M5 9 L8 21 H17 L20 9 Z" ${o(BLUE)}/><ellipse cx="12.5" cy="9" rx="7.5" ry="2.6" ${o("#d9ecfc")}/><path d="M5.5 9 C4 3 18 1 19.5 9" ${line(INK, 1.4)}/><path d="M20.5 15 Q22.5 18.5 20.5 20" ${line(RED, 2.4)}/><circle cx="20.5" cy="20.6" r="1.5" fill="${RED}"/>`,
  vpaint: `<polygon points="12,4 20.5,19 3.5,19" ${o("#eef3f8")}/>${dot(12, 4, RED, 2.3)}${dot(20.5, 19, GREEN, 2.3)}${dot(3.5, 19, BLUE, 2.3)}`,
  tpaint: `<rect x="3" y="7" width="14" height="14" ${o(PAPER)}/><path d="M3 7 H10 V14 H17 V21 H10 V14 H3Z" fill="${ORANGE}"/><rect x="3" y="7" width="14" height="14" fill="none" stroke="${INK}" stroke-width="1.3"/><path d="M12 13 L20.5 3.5" ${line(INK, 3.6)}/><path d="M12 13 L20.5 3.5" ${line(RED, 2)}/>`,
  erase: `<polygon points="3,15 11,6 20,12 12,21" ${o(PINK)}/><polygon points="3,15 7.4,10.1 15.8,16.5 12,21" ${o(SKY, 1.2)}/><path d="M2 21.5 H8" ${line(GREY, 1.6)}/>`,
  orbit: `<circle cx="12" cy="12" r="5.2" ${o(SKY)}/><path d="M4.5 9 A8.5 8.5 0 0 1 19.5 9" ${line(GREEN, 2.2)}/>${arrowHead("16,4.5 22,8 16.5,11.5", GREEN)}<path d="M19.5 15 A8.5 8.5 0 0 1 4.5 15" ${line(GREEN, 2.2)}/>${arrowHead("8,12.5 2,16 7.5,19.5", GREEN)}`,
  pan: `<path d="M7.5 21 C5 18 4 16 4 14 V11.5 Q4 10 5.5 10 Q7 10 7 11.5 V8 Q7 6.6 8.4 6.6 Q9.8 6.6 9.8 8 V5.5 Q9.8 4 11.2 4 Q12.6 4 12.6 5.5 V7 Q12.6 5.6 14 5.6 Q15.4 5.6 15.4 7 V8.4 Q15.4 7.4 16.7 7.4 Q18 7.4 18 8.8 V15 Q18 19 15 21 Z" ${o("#f7cfa3")}/>`,
  zoom: `<circle cx="10" cy="10" r="6.6" ${o("#d5ecfb", 1.6)}/><path d="M15 15 L21 21" ${line(INK, 4)}/><path d="M15 15 L21 21" ${line("#a8693a", 2.4)}/><path d="M7 10 H13 M10 7 V13" ${line(BLUE, 1.8)}/>`,
  section: cube(3, 3, 1, "#e6ecf2", "#cdd7e1", "#b8c5d2") + `<polygon points="1.5,12 15.5,4.5 22.5,10 8.5,17.5" fill="${RED}" fill-opacity="0.55" stroke="${RED}" stroke-width="1.4" stroke-linejoin="round"/>`,
  zoomwindow: `<rect x="2.5" y="3.5" width="12" height="10" fill="none" stroke="${ORANGE}" stroke-width="1.8" stroke-dasharray="2.6 1.8"/><circle cx="13.5" cy="13" r="5" ${o("#d5ecfb", 1.5)}/><path d="M17.2 16.7 L21.5 21" ${line(INK, 3.4)}/><path d="M17.2 16.7 L21.5 21" ${line("#a8693a", 2)}/>`,
  position: `<path d="M12 22 C6 15 5 12.5 5 10 A7 7 0 0 1 19 10 C19 12.5 18 15 12 22 Z" ${o(RED)}/><circle cx="12" cy="9.6" r="2.8" ${o(PAPER, 1.1)}/>`,
  lookaround: `<path d="M1.8 12 C5 6 19 6 22.2 12 C19 18 5 18 1.8 12 Z" ${o(PAPER)}/><circle cx="12" cy="12" r="3.9" ${o(BLUE, 1.2)}/><circle cx="12" cy="12" r="1.6" fill="${INK}"/><path d="M4 4.5 Q12 0 20 4.5" ${line(ORANGE, 1.8)}/>`,
  walk: `<ellipse cx="8.2" cy="9" rx="3" ry="4.8" ${o(BLUE)} transform="rotate(-12 8.2 9)"/><ellipse cx="7.6" cy="17" rx="2.1" ry="2.6" ${o(BLUE)}/><ellipse cx="16" cy="6" rx="3" ry="4.8" ${o(SKY)} transform="rotate(12 16 6)"/><ellipse cx="16.8" cy="14" rx="2.1" ry="2.6" ${o(SKY)}/>`,
  wall: `<rect x="2.5" y="5" width="19" height="14" ${o("#e8a27a")}/><path d="M2.5 9.7 H21.5 M2.5 14.3 H21.5 M9 5 V9.7 M16 9.7 V14.3 M9 14.3 V19" ${line("#9c5233", 1.2)}/>`,
  room: `<rect x="3" y="3.5" width="18" height="17" ${o("#fdeccb")}/><path d="M3 3.5 H21 V20.5 H11 M7 20.5 H3 V3.5" ${line("#6f7a86", 3.4)}/><path d="M8 18.4 Q10 15 12 16.5" ${line(BLUE, 1.5)}/>`,
  wallpull: `<rect x="3" y="4" width="5" height="16" ${o("#e8a27a")}/><path d="M9.5 12 H20" ${line(RED, 2.4)}/>${arrowHead("17,7.8 22.5,12 17,16.2", RED)}<path d="M8 4 H12 M8 20 H12" ${line(GREY, 1.3)} stroke-dasharray="1.6 1.6"/>`,
  stairs: `<polygon points="2.5,21 2.5,16.5 7.5,16.5 7.5,12 12.5,12 12.5,7.5 17.5,7.5 17.5,3.5 21.5,3.5 21.5,21" ${o("#e2c08a")}/><path d="M7.5 16.5 V21 M12.5 12 V21 M17.5 7.5 V21" ${line("#a8814b", 1.1)}/>`,
  roof: `<rect x="5" y="12" width="14" height="9" ${o(PAPER)}/><rect x="15" y="3.5" width="3" height="6" ${o("#b9795a", 1.1)}/><polygon points="1.8,13 12,4 22.2,13" ${o(RED)}/><path d="M1.8 13 H22.2" ${line(INK, 1)}/>`,
  column: `<rect x="5" y="3" width="14" height="3.4" rx="1" ${o("#dcd6c6")}/><rect x="8" y="6.4" width="8" height="11.2" ${o("#ece7d9")}/><path d="M10.7 6.4 V17.6 M13.3 6.4 V17.6" ${line("#bdb6a2", 1)}/><rect x="5" y="17.6" width="14" height="3.4" rx="1" ${o("#dcd6c6")}/>`,
  smoove: `<path d="M1.5 19 C5 19 7 7 12 7 S19 19 22.5 19 Z" ${o(LEAF)}/><path d="M12 6 V1.2" ${line(RED, 2)}/>${arrowHead("8.8,3 12,0 15.2,3", RED)}<path d="M8 15 C9 13 10.5 12 12 12" ${line(GREEN, 1.3)}/>`,
  stamp: `<circle cx="12" cy="4.6" r="2.6" ${o(RED)}/><rect x="10" y="6.8" width="4" height="6" ${o("#c63d34", 1.1)}/><rect x="4.5" y="12.8" width="15" height="4" rx="1.4" ${o(RED)}/><path d="M1.5 20.5 C5 17.5 8 21 12 20 S19 18.5 22.5 20.5" ${line(GREEN, 2.4)}/>`,
  drape: `<path d="M1.5 21 V18 C5 18 7 11 11 11 S17 18 22.5 17 V21 Z" ${o(LEAF)}/><path d="M2 5 H22" ${line(RED, 1.8)}/><path d="M6 6.5 V9.5 M12 6.5 V9 M18 6.5 V12" ${line(BLUE, 1.3)} stroke-dasharray="1.6 1.4"/><path d="M2 5 C6 5 7 12.4 11 11.8 S17 16.5 22 15.8" ${line(RED, 2.2)}/>`,
  flipedge: `<polygon points="3.5,5 20,3.5 21,19 4.5,20.5" ${o("#d5ecfb")}/><path d="M3.5 5 L21 19" ${line(RED, 2)}/><path d="M8 12.5 A4.5 4.5 0 0 1 13 7" ${line(ORANGE, 1.8)}/>${arrowHead("12,4 16,7.4 11,9.6", ORANGE)}`,
};

/** The tools this set draws (every tool on the tool bar). */
export const CLASSIC_ICON_TOOLS: string[] = Object.keys(ART);

/** The raw SVG for one tool (for the contact sheet and tests). */
export const classicIconSvg = (tool: string): string | undefined => (ART[tool] ? `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24">${ART[tool]}</svg>` : undefined);

/** tool name -> data URL, ready for a skin's `icons` */
export const CLASSIC_ICONS: Record<string, string> = Object.fromEntries(CLASSIC_ICON_TOOLS.map((k) => [k, `data:image/svg+xml;base64,${btoa(classicIconSvg(k)!)}`]));
