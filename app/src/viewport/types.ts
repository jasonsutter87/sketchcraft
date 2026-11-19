// The 3D view's shared types: tools, styles, the camera and the operation in progress.
import type * as THREE from "three";
import type { FaceInfo, Reply, Scene, Vec3 } from "../engine";
import type { Sel, SelMode } from "../selection";

export type Tool =
  | "select" | "axes" | "line" | "freehand" | "arc" | "arccenter" | "arc3" | "pie" | "rect" | "rrect" | "circle" | "polygon"
  | "push" | "offset" | "followme" | "move" | "rotate" | "scale"
  | "tape" | "dimension" | "text" | "paint" | "erase" | "orbit" | "pan" | "zoom" | "section" | "place"
  | "position" | "lookaround" | "walk" | "zoomwindow" | "protractor"
  | "wall" | "wallpull" | "room" | "stairs" | "roof" | "column"
  | "smoove" | "stamp" | "drape" | "flipedge"
  | "plugin";

/** Settings for the building-block tools (lengths in meters). */
export interface BuildSettings {
  /** Room mode: what a click on a wall adds, and whether the walls are shown low (a view setting only) */
  roomOpening: "door" | "window";
  wallsDown: boolean;
  wallThickness: number;
  wallHeight: number;
  stairWidth: number;
  stairRise: number;
  roofKind: "gable" | "hip" | "shed";
  roofPitch: number;
  roofOverhang: number;
  columnSize: number;
  columnHeight: number;
  columnRound: boolean;
  /** Sandbox */
  smooveRadius: number;
  smooveHeight: number;
  stampOffset: number;
  gridSize: number;
  gridSpacing: number;
  contourSpacing: number;
}

export type FaceStyle = "textured" | "shaded" | "monochrome" | "hiddenline" | "wireframe" | "physical";
export type Backdrop = "default" | "paper" | "blueprint" | "night";

export interface Styles {
  xray: boolean;
  /** while a group or component is open: everything else hidden */
  hideRest?: boolean;
  /** faces in their tag's color */
  byTag?: boolean;
  /** dimensions, labels and door swings (on unless turned off) */
  notes?: boolean;
  /** the tool coach, top-left: what to do next (on unless turned off) */
  help?: boolean;
  /** SketchUp's face styles */
  face?: FaceStyle;
  /** bold outlines around shapes */
  profiles?: boolean;
  /** hand-drawn, doubled edges */
  sketchy?: boolean;
  /** edges run a little past their ends */
  extensions?: boolean;
  /** ambient occlusion: soft shading where surfaces meet */
  ao?: boolean;
  background?: Backdrop;
  /** hidden edges shown dashed (SketchUp's Back Edges, K) */
  backEdges: boolean;
  /** draw tag and edge line weights and styles (default on) */
  lineWeights?: boolean;
  shadows: boolean;
  edges: boolean;
  parallel: boolean;
}

export interface Cam {
  eye: Vec3;
  target: Vec3;
  fov: number;
}

export interface ViewportApi {
  applyTyped: (text: string) => void;
  fit: () => void;
  fitSoon: () => void;
  /** frame this box (world corners), e.g. where most of an imported drawing is */
  fitBox: (lo: Vec3, hi: Vec3) => void;
  view: (name: "iso" | "top" | "front" | "right" | "back" | "left") => void;
  camera: () => Cam;
  goTo: (c: Cam & { photo?: string }) => void;
  clearSection: () => void;
  /** a section cut through `origin`, keeping the side `normal` points to */
  setSection: (origin: Vec3, normal: Vec3) => void;
  /** the active section plane (a point on it and its normal), if any */
  sectionPlane: () => { origin: Vec3; normal: Vec3 } | null;
  previousView: () => void;
  /** timeline playback: the camera exactly here */
  setCamera: (c: Cam) => void;
  /** a sun-study angle [azimuth, altitude] in degrees, or null for the usual light */
  setSun: (s: [number, number] | null) => void;
  /** render one frame at this size, as a JPEG data URL */
  captureFrame: (w: number, h: number, caption?: string) => string;
  nextView: () => void;
  /** SubD mode on a subdivision object (false when it isn't one) */
  editCage: (id: number) => boolean;
  exitCage: () => void;
  inCage: () => boolean;
  /** a floor-plan cut: everything above `height` (m) taken away */
  planCut: (height: number) => void;
  /** slide the cut along its direction (m; + moves it deeper into what's kept) */
  moveSection: (by: number) => void;
}

export interface Props {
  scene: Scene | null;
  tool: Tool;
  setTool: (t: Tool) => void;
  sel: Sel;
  setSel: (s: Sel) => void;
  color: [number, number, number];
  setColor: (c: [number, number, number]) => void;
  /** an image material to paint with (instead of the color) */
  paintMaterial: string | null;
  setPaintMaterial: (m: string | null) => void;
  styles: Styles;
  placeAsset: string | null;
  build: BuildSettings;
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  onMeasure: (label: string, value: string) => void;
  onContext: (x: number, y: number) => void;
  onToast: (t: string, bad?: boolean) => void;
  onSection: (on: boolean) => void;
  /** the coach's × */
  onHideHelp?: () => void;
  /** the view is put away (the Layout workspace covers it): it keeps everything but draws nothing and takes no keys */
  paused?: boolean;
  api: React.MutableRefObject<ViewportApi | null>;
  /** a plugin's tool in use (tool "plugin"): where its moves and clicks go,
   * what it draws, its hint, and the point its rubber band starts from */
  pluginTool?: {
    name: string;
    hint: string;
    preview: [number, number, number][][];
    anchor: [number, number, number] | null;
    move: (at: PluginPoint) => void;
    click: (at: PluginPoint) => void;
  } | null;
}

/** A snapped point for a plugin tool (see plugins.tsx ToolPoint). */
export interface PluginPoint {
  point: [number, number, number];
  snap: string;
  face: number | null;
  normal: [number, number, number];
  shift: boolean;
  alt: boolean;
}

/** What `wall_info` tells the app about a wall: its faces, which way a positive pull goes, and every moving point's travel per metre. */
export interface WallPlan {
  face: number;
  normal: [number, number, number];
  thickness: number;
  faces: number[];
  strips: [number, number, number][][];
  moves: { v: number; at: [number, number, number]; per_m: [number, number, number] }[];
}

/** What `room_preview` tells the app about a dragged room (or why it can't be there). */
export interface RoomPreview {
  ok: boolean;
  error?: string;
  rect?: [number, number, number, number];
  inside?: [number, number, number, number];
  width?: number;
  depth?: number;
  area?: number;
  thickness?: number;
  shared?: { room: number; name: string | null; side: string }[];
  snapped?: boolean;
  /** what really happens: a new room, or a combine / subtract on `target` */
  mode?: "new" | "combine" | "subtract";
  target?: { room: number; name: string } | null;
  /** the inside outline and the wall centre-line outline (counter-clockwise), and a point inside to label */
  outline?: [number, number][];
  centre_outline?: [number, number][];
  label?: [number, number];
}

/** A room as `rooms` lists it. */
export interface RoomRow {
  id: number;
  name: string;
  area: number;
  center: [number, number, number];
}

export type Op =
  | { t: "line"; p0: THREE.Vector3; start: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "rect"; p0: THREE.Vector3; n: THREE.Vector3; u: THREE.Vector3; v: THREE.Vector3; a: number; b: number }
  | { t: "circle"; c: THREE.Vector3; n: THREE.Vector3; r: number; seg: number }
  | { t: "arc"; a: THREE.Vector3; b: THREE.Vector3 | null; n: THREE.Vector3; bulge: number; cur: THREE.Vector3 }
  /** center Arc and Pie: center, then the start (radius), then swing round */
  | { t: "carc"; c: THREE.Vector3; n: THREE.Vector3; a: THREE.Vector3 | null; ang: number; last: number; pie: boolean }
  /** 3-Point Arc: start, a point on it, end */
  | { t: "arc3"; a: THREE.Vector3; m: THREE.Vector3 | null; n: THREE.Vector3; cur: THREE.Vector3 }
  /** Rotated Rectangle: one side (any direction), then how far the other goes */
  | { t: "rrect"; a: THREE.Vector3; b: THREE.Vector3 | null; h: THREE.Vector3; cur: THREE.Vector3 }
  /** Freehand: the points dragged through, on the plane it started on */
  | { t: "free"; pts: THREE.Vector3[]; n: THREE.Vector3; o: THREE.Vector3 }
  /** Axes: the new origin, then the red direction, then green */
  | { t: "axes"; o: THREE.Vector3; x: THREE.Vector3 | null; cur: THREE.Vector3 }
  | { t: "push"; face: FaceInfo; origin: THREE.Vector3; n: THREE.Vector3; d: number; dragged: boolean }
  | { t: "offset"; face: FaceInfo; start: THREE.Vector3; d: number }
  /** Wall pull: the wall the picked face belongs to, and what the engine says moves with it */
  /** Room mode: dragging a room out between two ground corners (a, b); what the engine says about it (null: not asked yet) */
  | { t: "room"; a: THREE.Vector3; b: THREE.Vector3; preview: RoomPreview | null; asked: string; pending: boolean; alt: boolean }
  | { t: "wallpull"; face: number; plan: WallPlan; origin: THREE.Vector3; n: THREE.Vector3; d: number; dragged: boolean }
  | { t: "move"; base: THREE.Vector3; ids: number[]; delta: THREE.Vector3; copy: boolean; vertex?: { far: THREE.Vector3[] } }
  | { t: "rotate"; center: THREE.Vector3; n: THREE.Vector3; ref: THREE.Vector3 | null; angle: number; ids: number[]; copy?: boolean }
  | { t: "scale"; center: THREE.Vector3; start: THREE.Vector3; factor: number; ids: number[] }
  | { t: "tape"; p0: THREE.Vector3; along: THREE.Vector3 | null; cur: THREE.Vector3 }
  | { t: "box"; x0: number; y0: number; x1: number; y1: number; mode: SelMode }
  | { t: "dim"; a: THREE.Vector3; b: THREE.Vector3 | null; off: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "text"; leader: THREE.Vector3; cur: THREE.Vector3 }
  | { t: "wall"; pts: THREE.Vector3[]; cur: THREE.Vector3 }
  | { t: "stairs"; at: THREE.Vector3; cur: THREE.Vector3 };

export interface LabelSpec {
  key: string;
  p: THREE.Vector3;
  text: string;
  live?: boolean;
  /** annotation id, for the delete button */
  id?: number;
}

export type Last =
  | { t: "push"; face: number; d: number }
  | { t: "move"; ids: number[]; by: THREE.Vector3; copy: boolean }
  | { t: "rotate"; ids: number[]; center: THREE.Vector3; n: THREE.Vector3; angle: number }
  | { t: "scale"; ids: number[]; center: THREE.Vector3; factor: number }
  /** a Tape Measure reading (typing its real length then resizes the model) */
  | { t: "tape"; d: number }
  | null;
