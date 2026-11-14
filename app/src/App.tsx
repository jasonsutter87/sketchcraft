import { useCallback, useEffect, useRef, useState } from "react";
import { ask, AskHost } from "./ask";
import { exec, loadScene, sync, type Reply, type Scene } from "./engine";
import { Icons, Logo } from "./icons";
import { BuildTools, ContextMenu, contextItems, EntityInfo, Environments, Library, LocationDialog, Outliner, RenderDialog, Sandbox, SheetsDialog, ShortcutSheet, Templates, Textures, Views } from "./panels";
import { EMPTY, isEmpty, selIds, type Sel } from "./selection";
import { formatLength, parseLength, UNIT_OPTIONS } from "./units";
import { pickOpen, pickSave, rememberedFile, rememberFile } from "./files";
import Viewport, { type Backdrop, type BuildSettings, type FaceStyle, type Styles, type Tool, type ViewportApi } from "./Viewport";
import { MatchPhoto, type PhotoMatch } from "./matchphoto";
import { prepareImage } from "./textures";
import { applySkin, readSkin, SKINS, store, type Skin } from "./skins";
import { TOOL_INFO, WORKSPACES, withEdits, type WorkspaceEdits, type WorkspaceId } from "./workspace";
import { AppearanceDialog, WorkspaceDialog } from "./appearance";
import { WorkspaceStrip } from "./wsstrip";
import { PhotoModeler } from "./photomodel";
import { RefImageDialog } from "./refimage";
import { Text3dDialog } from "./text3d";
import { TimelinePanel } from "./timeline";
import { LearnCenter, LessonCard, lessonsDone, useLearnCtx } from "./learn";

const DEFAULT_BUILD: BuildSettings = {
  wallThickness: 0.2,
  wallHeight: 2.7,
  stairWidth: 1.0,
  stairRise: 2.7,
  roofKind: "gable",
  roofPitch: 30,
  roofOverhang: 0.4,
  columnSize: 0.3,
  columnHeight: 2.7,
  columnRound: false,
  smooveRadius: 5,
  smooveHeight: 1,
  stampOffset: 1,
  gridSize: 20,
  gridSpacing: 1,
  contourSpacing: 1,
};

function loadBuild(): BuildSettings {
  try {
    return { ...DEFAULT_BUILD, ...JSON.parse(localStorage.getItem("sketchcraft.build") ?? "{}") };
  } catch {
    return DEFAULT_BUILD;
  }
}

/** One-click looks, like SketchUp's style collections. */
const STYLE_PRESETS: { name: string; styles: Partial<Styles> }[] = [
  { name: "Default", styles: { face: "textured", background: "default", profiles: false, sketchy: false, extensions: false, ao: false, edges: true } },
  { name: "Architectural", styles: { face: "shaded", background: "default", profiles: true, sketchy: false, extensions: false, ao: true, edges: true } },
  { name: "Sketchy on paper", styles: { face: "hiddenline", background: "paper", profiles: true, sketchy: true, extensions: true, ao: false, edges: true } },
  { name: "Hidden line", styles: { face: "hiddenline", background: "default", profiles: true, sketchy: false, extensions: false, ao: false, edges: true } },
  { name: "Blueprint", styles: { face: "hiddenline", background: "blueprint", profiles: false, sketchy: false, extensions: true, ao: false, edges: true } },
  { name: "Clay", styles: { face: "monochrome", background: "default", profiles: false, sketchy: false, extensions: false, ao: true, edges: false } },
  { name: "Night", styles: { face: "shaded", background: "night", profiles: true, sketchy: false, extensions: false, ao: false, edges: true } },
];

/** The cut's height for a floor-plan cut: type one (4', 14', 1.2 m) to move it. */
function SectionHeight({ api, units, tick }: { api: React.RefObject<ViewportApi | null>; units: string; tick: number }) {
  const plane = api.current?.sectionPlane();
  const flat = plane && Math.abs(plane.normal[2]) > 0.99;
  const [text, setText] = useState<string | null>(null);
  void tick;
  if (!flat) return null;
  const z = plane!.origin[2];
  return (
    <input
      className="section-height"
      title="Cut height (type a height, Enter)"
      value={text ?? formatLength(z, units)}
      onFocus={(e) => (setText(formatLength(z, units)), e.target.select())}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => setText(null)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          const h = parseLength(text ?? "", units);
          if (!Number.isNaN(h)) api.current?.planCut(h);
          (e.target as HTMLInputElement).blur();
        }
        if (e.key === "Escape") (e.target as HTMLInputElement).blur();
      }}
    />
  );
}

const TOOL_GROUPS: { id: Tool; name: string; key: string }[][] = [
  [{ id: "select", name: "Select", key: "Space" }],
  [
    { id: "line", name: "Line", key: "L" },
    { id: "arc", name: "Arc", key: "A" },
    { id: "rect", name: "Rectangle", key: "R" },
    { id: "circle", name: "Circle", key: "C" },
    { id: "polygon", name: "Polygon", key: "N" },
  ],
  [
    { id: "push", name: "Push / Pull", key: "P" },
    { id: "offset", name: "Offset", key: "F" },
    { id: "followme", name: "Follow Me", key: "J" },
  ],
  [
    { id: "move", name: "Move (Option/Ctrl to copy)", key: "M" },
    { id: "rotate", name: "Rotate (Option/Ctrl to copy)", key: "Q" },
    { id: "scale", name: "Scale", key: "S" },
  ],
  [
    { id: "tape", name: "Tape Measure (makes guides)", key: "T" },
    { id: "dimension", name: "Dimension: click, click, pull out", key: "D" },
    { id: "text", name: "Text label: click a point, then where the label goes", key: "W" },
    { id: "paint", name: "Paint Bucket (⌘/Option-click samples, Shift-click paints all matching)", key: "B" },
    { id: "erase", name: "Eraser (Shift hides, Option/Ctrl softens, both unsoften)", key: "E" },
  ],
  [
    { id: "orbit", name: "Orbit (or middle-drag / hold Space)", key: "O" },
    { id: "pan", name: "Pan (or Shift+middle-drag / right-drag)", key: "H" },
    { id: "zoom", name: "Zoom (drag up/down; Shift+Z zooms extents)", key: "Z" },
    { id: "section", name: "Section cut: click a face", key: "X" },
  ],
];
const ALL_TOOLS = TOOL_GROUPS.flat();

const SWATCHES: [number, number, number][] = [
  [0.95, 0.94, 0.91], [0.93, 0.86, 0.72], [0.96, 0.8, 0.7], [0.82, 0.88, 0.8], [0.78, 0.85, 0.93], [0.87, 0.82, 0.92],
  [0.98, 0.93, 0.62], [0.78, 0.78, 0.76], [0.72, 0.16, 0.18], [0.45, 0.28, 0.18], [0.42, 0.55, 0.66], [0.42, 0.6, 0.36],
  [0.55, 0.57, 0.6], [0.32, 0.34, 0.38], [0.16, 0.17, 0.2], [0.97, 0.97, 0.95], [0.85, 0.55, 0.42], [0.6, 0.66, 0.5],
];

const hex = (c: number[]) => "#" + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) * 255).toString(16).padStart(2, "0")).join("");
const unhex = (h: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as [number, number, number];

type Tab = "outliner" | "library" | "views";

/** the skin you wore last time (a built-in by id, or a saved one by its data) */
function rememberedSkin(): Skin {
  const id = store.get("sketchcraft.skin");
  const built = SKINS.find((s) => s.id === id);
  if (built) return built;
  try {
    const data = store.get("sketchcraft.skinData");
    const s = data ? readSkin(JSON.parse(data), "My skin") : null;
    if (s) return s;
  } catch {
    /* fall through */
  }
  return SKINS[0];
}
function rememberedEdits(): Partial<Record<WorkspaceId, WorkspaceEdits>> {
  try {
    return JSON.parse(store.get("sketchcraft.workspaceEdits") ?? "{}");
  } catch {
    return {};
  }
}

export default function App() {
  const [scene, setScene] = useState<Scene | null>(null);
  const [tool, setTool] = useState<Tool>("rect");
  const [sel, setSel] = useState<Sel>(EMPTY);
  const [color, setColorState] = useState<[number, number, number]>(SWATCHES[1]);
  const [paintMaterial, setPaintMaterial] = useState<string | null>(null);
  // picking a color stops painting with an image
  const setColor = (c: [number, number, number]) => {
    setColorState(c);
    setPaintMaterial(null);
  };
  const [measure, setMeasure] = useState({ label: "", value: "" });
  const [typed, setTyped] = useState("");
  const [toast, setToast] = useState<{ text: string; bad?: boolean } | null>(null);
  const [tab, setTab] = useState<Tab>("outliner");
  const [styles, setStyles] = useState<Styles>({ xray: false, backEdges: false, shadows: false, edges: true, parallel: false, face: "textured", background: "default" });
  const [section, setSection] = useState(false);
  // skin and workspace, remembered between launches
  const [skin, setSkinState] = useState<Skin>(rememberedSkin);
  useEffect(() => applySkin(skin), [skin]);
  const wearSkin = (s: Skin, useBackdrop: boolean) => {
    setSkinState(s);
    store.set("sketchcraft.skin", s.id);
    store.set("sketchcraft.skinData", JSON.stringify(s));
    if (useBackdrop) setStyles((st) => ({ ...st, background: s.backdrop ?? "default" }));
  };
  const [wsId, setWsId] = useState<WorkspaceId>(() => (WORKSPACES.some((w) => w.id === store.get("sketchcraft.workspace")) ? (store.get("sketchcraft.workspace") as WorkspaceId) : "model"));
  const [wsEdits, setWsEdits] = useState(rememberedEdits);
  const ws = withEdits(WORKSPACES.find((w) => w.id === wsId)!, wsEdits[wsId]);
  const editWs = (e: WorkspaceEdits | null) => {
    const next = { ...wsEdits };
    if (e) next[wsId] = { ...next[wsId], ...e };
    else delete next[wsId];
    setWsEdits(next);
    store.set("sketchcraft.workspaceEdits", JSON.stringify(next));
    if (e?.leftTab) setTab(e.leftTab);
  };
  const switchWs = (id: WorkspaceId) => {
    const w = withEdits(WORKSPACES.find((x) => x.id === id)!, wsEdits[id]);
    setWsId(id);
    store.set("sketchcraft.workspace", id);
    setTab(w.leftTab);
    if (w.styles) setStyles((s) => ({ ...s, ...w.styles }));
    // a tool that isn't on this workspace's toolbar: back to Select
    if (!w.tools.flat().includes(tool)) chooseTool("select");
  };
  const [showSkins, setShowSkins] = useState(false);
  const [showPhotoModel, setShowPhotoModel] = useState(false);
  const [showRef, setShowRef] = useState(false);
  const [showText3d, setShowText3d] = useState(false);
  const [showTimeline, setShowTimeline] = useState(false);
  const [showLearn, setShowLearn] = useState(false);
  const [lesson, setLesson] = useState<string | null>(null);
  const [showWs, setShowWs] = useState(false);
  const [placing, setPlacing] = useState<string | null>(null);
  const [build, setBuildState] = useState<BuildSettings>(loadBuild);
  const setBuild = (b: BuildSettings) => {
    setBuildState(b);
    try {
      localStorage.setItem("sketchcraft.build", JSON.stringify(b));
    } catch {
      /* settings just won't persist */
    }
  };
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [fileMenu, setFileMenu] = useState(false);
  const [currentFile, setCurrentFileState] = useState<string | null>(rememberedFile());
  const setCurrentFile = (p: string | null) => {
    setCurrentFileState(p);
    rememberFile(p);
  };
  const api = useRef<ViewportApi | null>(null);
  const typedRef = useRef<HTMLInputElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  /** ⌘C / ⌘X: what to paste, and whether it was cut */
  const clip = useRef<{ ids: number[]; cut: boolean } | null>(null);
  const [showKeys, setShowKeys] = useState(false);
  const [showSheets, setShowSheets] = useState(false);
  const [showLocation, setShowLocation] = useState(false);
  const [showRender, setShowRender] = useState(false);
  // bumps when the section moves, so its height box refreshes
  const [sectionTick, setSectionTick] = useState(0);
  /** Match Photo in progress: the photo being lined up */
  const [matching, setMatching] = useState<{ src: string; width: number; height: number } | null>(null);
  const startMatch = async (file: File) => {
    try {
      const img = await prepareImage(file, 1600);
      setMatching({ src: img.data, width: img.width, height: img.height });
    } catch (e) {
      say(String(e), true);
    }
  };
  const finishMatch = async (mp: PhotoMatch) => {
    const m = matching;
    if (!m) return;
    const r = await run({ cmd: "match_photo", name: mp.name, width: m.width, height: m.height, red: mp.red, green: mp.green, origin: mp.origin, distance: mp.distance, photo: m.src });
    if (!r.ok) return; // keep the editor open to fix the lines
    setMatching(null);
    api.current?.goTo({ eye: r.eye as [number, number, number], target: r.target as [number, number, number], fov: r.fov as number, photo: m.src });
    say(`Matched. Model over the photo; orbit to leave, click "${mp.name}" in Views to come back.`);
  };
  const showKeysRef = useRef(false);
  showKeysRef.current = showKeys;

  const say = useCallback((text: string, bad = false) => {
    setToast({ text, bad });
    window.setTimeout(() => setToast((t) => (t?.text === text ? null : t)), 3600);
  }, []);

  const refresh = useCallback(async () => setScene(await loadScene()), []);

  const run = useCallback(
    async (cmd: Record<string, unknown>): Promise<Reply> => {
      const r = await exec(cmd);
      if (!r.ok) say(r.error ?? "That didn't work", true);
      await refresh();
      return r;
    },
    [refresh, say]
  );

  useEffect(() => {
    refresh();
    // pick up edits Claude makes over MCP
    const id = window.setInterval(async () => {
      if (await sync()) {
        await refresh();
        say("Model updated by another session");
      }
    }, 700);
    return () => window.clearInterval(id);
  }, [refresh, say]);

  // drop selections that no longer exist
  useEffect(() => {
    if (!scene) return;
    const f = new Set(scene.faces.map((x) => x.id));
    const e = new Set(scene.edgeList.map((x) => x.id));
    const g = new Set(scene.groups.map((x) => x.id));
    const i = new Set(scene.instances.map((x) => x.id));
    setSel((s) => ({ faces: s.faces.filter((x) => f.has(x)), edges: s.edges.filter((x) => e.has(x)), groups: s.groups.filter((x) => g.has(x)), instances: s.instances.filter((x) => i.has(x)) }));
  }, [scene]);

  const chooseTool = useCallback((t: Tool) => {
    setTool(t);
    if (t !== "place") setPlacing(null);
  }, []);

  const deleteSelection = useCallback(async () => {
    if (isEmpty(sel)) return;
    await run({ cmd: "delete", ids: selIds(sel) });
    setSel(EMPTY);
  }, [sel, run]);

  // ⌘V: a copy of the clipboard, selected, with Move ready to place it
  const paste = async () => {
    const c = clip.current;
    if (!c) return say("Nothing to paste: select something and press ⌘C first.", true);
    const d = await run({ cmd: "duplicate", ids: c.ids, by: [0, 0, 0] });
    if (!d.ok) return;
    const ids = d.ids as number[];
    if (c.cut) {
      await run({ cmd: "unhide", ids });
      await run({ cmd: "delete", ids: c.ids });
      clip.current = { ids, cut: false };
    }
    const sc = await loadScene();
    setScene(sc);
    const has = (list: { id: number }[]) => ids.filter((i) => list.some((x) => x.id === i));
    setSel({ faces: has(sc.faces), edges: [], groups: has(sc.groups), instances: has(sc.instances) });
    chooseTool("move");
    say("Pasted in place. Click a point on it, then click where it goes.");
  };

  // keyboard: tools, undo, delete, groups, typing into the measurement box
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      // SubD mode has its own keys (the viewport handles them)
      if (api.current?.inCage()) {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
          e.preventDefault();
          run({ cmd: e.shiftKey ? "redo" : "undo" });
        }
        return;
      }
      // Tab on a selected subdivision object: shape it (SubD mode)
      if (e.key === "Tab" && sel.groups.length === 1 && scene?.cages?.[String(sel.groups[0])]) {
        e.preventDefault();
        api.current?.editCage(sel.groups[0]);
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      const k = e.key.toLowerCase();
      if (mod && ["n", "o", "s", "i"].includes(k)) {
        e.preventDefault();
        const f = fileRef2.current;
        if (k === "n") return void f.new();
        if (k === "o") return void f.open();
        if (k === "s") return void f.save(e.shiftKey);
        if (k === "i") return void f.import();
      }
      if (mod && k === "z") {
        e.preventDefault();
        await run({ cmd: e.shiftKey ? "redo" : "undo" });
        return;
      }
      if (mod && k === "y") {
        // Windows redo
        e.preventDefault();
        await run({ cmd: "redo" });
        return;
      }
      if (mod && k === "t") {
        // Select None
        e.preventDefault();
        setSel(EMPTY);
        return;
      }
      if (mod && ["c", "x", "v"].includes(k)) {
        e.preventDefault();
        if (k === "v") return void paste();
        if (isEmpty(sel)) return;
        clip.current = { ids: selIds(sel), cut: k === "x" };
        if (k === "x") {
          // cut: out of sight until pasted (undo brings it back)
          await run({ cmd: "hide", ids: clip.current.ids });
          setSel(EMPTY);
        }
        say(k === "x" ? "Cut. ⌘V pastes it where you click." : "Copied. ⌘V pastes, then click to place.");
        return;
      }
      if (mod && /^[1-6]$/.test(e.key)) {
        // standard views (SketchUp for Mac): Iso, Top, Front, Right, Back, Left
        e.preventDefault();
        api.current?.view((["iso", "top", "front", "right", "back", "left"] as const)[Number(e.key) - 1]);
        return;
      }
      if (mod && k === "a" && scene) {
        e.preventDefault();
        const inGroup = scene.activeGroup;
        setSel({
          faces: scene.faces.filter((f) => f.group === inGroup || (inGroup == null && f.group == null)).map((f) => f.id),
          edges: [],
          groups: inGroup == null ? scene.groups.map((g) => g.id) : [],
          instances: inGroup == null && scene.editing == null ? scene.instances.map((i) => i.id) : [],
        });
        return;
      }
      if (mod) return;
      if (e.key === "Escape" && showKeysRef.current) return setShowKeys(false);
      if (e.key === "?" || (e.shiftKey && e.key === "/")) {
        e.preventDefault();
        return setShowKeys((v) => !v);
      }
      if (e.key === "Delete" || e.key === "Backspace") return void deleteSelection();
      if (e.shiftKey && k === "z") return api.current?.fit();
      if (k === "g") {
        // Make Component (SketchUp's G)
        e.preventDefault();
        const ids = [...sel.faces, ...sel.groups];
        if (!ids.length) return say("Select faces or groups first, then G makes them a component.", true);
        const name = await ask("Component name", "Component");
        if (name) {
          await run({ cmd: "make_component", ids, name });
          setSel(EMPTY);
        }
        return;
      }
      if (k === "k") {
        // Back Edges
        setStyles((s) => ({ ...s, backEdges: !s.backEdges, xray: false }));
        return;
      }
      if (/^[0-9.\-x/]$/.test(e.key) && !(k === "x" && tool !== "move")) {
        setTyped(e.key);
        typedRef.current?.focus();
        e.preventDefault();
        return;
      }
      const t = ALL_TOOLS.find((x) => x.key.toLowerCase() === k);
      if (t) chooseTool(t.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [scene, sel, tool, run, deleteSelection, chooseTool]);

  const runTemplate = async (lines: string[]) => {
    for (const line of lines) {
      const r = await exec(JSON.parse(line));
      if (!r.ok) {
        say(r.error ?? "Template failed", true);
        break;
      }
    }
    api.current?.fitSoon();
    await refresh();
  };

  /** what a CAD import made, in a sentence */
  const cadSummary = (name: string, r: Reply) =>
    `Imported ${name}: ${r.layers} layer${r.layers === 1 ? "" : "s"} (as tags), ${r.faces} faces, ${r.edges} edges, read as ${r.units}${r.skipped ? `; ${r.skipped} text/hatch items skipped` : ""}`;

  const importFile = async (file: File) => {
    let r: Reply;
    const lower = file.name.toLowerCase();
    if (lower.endsWith(".skp") || lower.endsWith(".fbx")) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let bin = "";
      for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      const fbx = lower.endsWith(".fbx");
      r = await run({ cmd: fbx ? "import_fbx" : "import_skp", data: btoa(bin), name: file.name.replace(/\.(skp|fbx)$/i, "") });
    } else if (lower.endsWith(".dxf")) {
      r = await run({ cmd: "import_dxf", text: await file.text(), name: file.name.replace(/\.dxf$/i, "") });
    } else if (lower.endsWith(".dwg")) {
      return say("Open DWG files with File › Import (the desktop app converts them)", true);
    } else {
      r = await run({ cmd: "import_obj", text: await file.text(), up: "y" });
    }
    if (r.ok) {
      say(r.layers != null ? cadSummary(file.name, r) : `Imported ${file.name}: ${r.faces} faces`);
      api.current?.fitSoon();
      await refresh();
    }
  };

  const importPath = async (path: string) => {
    const lower = path.toLowerCase();
    const r = lower.endsWith(".skp")
      ? await run({ cmd: "import_skp", path })
      : lower.endsWith(".fbx")
        ? await run({ cmd: "import_fbx", path })
        : lower.endsWith(".dxf")
          ? await run({ cmd: "import_dxf", path })
          : lower.endsWith(".dwg")
            ? await run({ cmd: "import_dwg", path })
            : await run({ cmd: "import_obj", path, up: "y" });
    if (r.ok) {
      say(r.layers != null ? cadSummary(path.split("/").pop()!, r) : `Imported ${path.split("/").pop()}: ${r.faces} faces`);
      api.current?.fitSoon();
      await refresh();
    }
    return r.ok;
  };

  const file = {
    new: async () => {
      await run({ cmd: "new" });
      setCurrentFile(null);
      setSel(EMPTY);
      say("New model. ⌘Z brings the last one back.");
    },
    open: async () => {
      const path = await pickOpen();
      if (!path) return;
      if (path.toLowerCase().endsWith(".sketchcraft")) {
        const r = await run({ cmd: "open", path });
        if (!r.ok) return;
        setCurrentFile(path);
        api.current?.fitSoon();
        await refresh();
      } else {
        await run({ cmd: "new" });
        if (await importPath(path)) setCurrentFile(null);
      }
      setSel(EMPTY);
    },
    save: async (as = false) => {
      let path = currentFile;
      if (!path || as) {
        path = await pickSave(currentFile?.split("/").pop() ?? "Untitled.sketchcraft", "sketchcraft", "SketchCraft model");
        if (!path) return;
        if (!path.toLowerCase().endsWith(".sketchcraft")) path += ".sketchcraft";
      }
      const r = await run({ cmd: "save", path });
      if (r.ok) {
        setCurrentFile(path);
        say(`Saved ${path.split("/").pop()}`);
      }
    },
    import: async () => {
      const path = await pickOpen();
      if (path) await importPath(path);
    },
    export: async (kind: "obj" | "glb" | "stl" | "dae" | "dxf" | "dwg" | "pdf" | "ifc") => {
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base}.${kind}`, kind, kind.toUpperCase());
      if (!path) return;
      const cmd =
        kind === "stl"
          ? { cmd: "export_stl", path, scale: 1000 }
          : kind === "pdf"
            ? { cmd: "export_pdf", path, width: 2400, height: 1500, camera: api.current!.camera(), title: base }
            : { cmd: `export_${kind}`, path };
      const r = await run(cmd);
      if (r.ok) say(`Exported ${path.split("/").pop()}`);
    },
    section: async (kind: "svg" | "dxf") => {
      const plane = api.current?.sectionPlane();
      if (!plane) return say("Cut a section first: X, then click a face.", true);
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base} section.${kind}`, kind, kind.toUpperCase());
      if (!path) return;
      const r = await run({ cmd: "section", ...plane, path });
      if (r.ok) say(`Exported the section cut to ${path.split("/").pop()}`);
    },
    blueprint: async () => {
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base} blueprint.pdf`, "pdf", "PDF");
      if (!path) return;
      say("Drawing the blueprint: plans with dimensions and tags, elevations with levels…");
      const r = await run({ cmd: "blueprint", path, project: base });
      if (r.ok) say(`Blueprint ready: ${(r.levels as number[]).length} floor plan${(r.levels as number[]).length === 1 ? "" : "s"} + 4 elevations${(r.pages as number) > 2 ? " + door and window schedule" : ""} in ${path.split("/").pop()}`);
    },
    report: async () => {
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base} report.csv`, "csv", "CSV");
      if (!path) return;
      const r = await run({ cmd: "report", path });
      if (r.ok) say(`Report: ${(r.rows as unknown[]).length} groups and components in ${path.split("/").pop()}`);
    },
    animation: async () => {
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base}.mp4`, "mp4", "MP4 video");
      if (!path) return;
      say(scene?.views.length && scene.views.length > 1 ? "Rendering the tour through your saved views…" : "No saved views to visit: rendering a turntable…");
      const r = await run({ cmd: "animate", path, width: 1280, height: 720, fps: 24, transition: 2, delay: 1 });
      if (r.ok) say(`Exported ${r.frames} frames to ${path.split("/").pop()}`);
    },
    render: async () => {
      const base = (currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");
      const path = await pickSave(`${base}.png`, "png", "PNG image");
      if (!path) return;
      const r = await run({ cmd: "render", out: path, width: 1920, height: 1080, camera: api.current!.camera() });
      if (r.ok) say(`Rendered ${path.split("/").pop()}`);
    },
  };
  const fileRef2 = useRef(file);
  fileRef2.current = file;

  const learnCtx = useLearnCtx(scene, wsId, section, !!styles.byTag);
  const docName = currentFile ? currentFile.split("/").pop()!.replace(/\.sketchcraft$/, "") : "Untitled";
  const selCount = selIds(sel).length;

  return (
    <div className={`app ${ws.showLeft ? "" : "no-left"} ${ws.showRight ? "" : "no-right"} ${ws.dock === "left" ? "dock-left" : ""}`}>
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <Logo />
          <span className="brand-name">SketchCraft</span>
          <span className="sep">/</span>
          <div className="dropdown">
            <button className="file-btn" onClick={() => setFileMenu((o) => !o)}>
              File ▾
            </button>
            {fileMenu && (
              <>
                <div className="menu-scrim" onPointerDown={() => setFileMenu(false)} />
                <div className="dropdown-menu file-menu" onClick={() => setFileMenu(false)}>
                  <button onClick={file.new}>New <kbd>⌘N</kbd></button>
                  <button onClick={file.open}>Open… <kbd>⌘O</kbd></button>
                  <button onClick={() => file.save(false)}>Save <kbd>⌘S</kbd></button>
                  <button onClick={() => file.save(true)}>Save As… <kbd>⇧⌘S</kbd></button>
                  <hr />
                  <button onClick={file.import}>Import SketchUp, CAD or OBJ… <kbd>⌘I</kbd></button>
                  <button onClick={() => fileRef.current?.click()}>Import from browser upload…</button>
                  <hr />
                  <button onClick={() => file.export("glb")}>Export glTF (.glb)…</button>
                  <button onClick={() => file.export("obj")}>Export OBJ…</button>
                  <button onClick={() => file.export("stl")}>Export STL (mm)…</button>
                  <button onClick={() => file.export("dae")}>Export Collada (.dae)…</button>
                  <button onClick={() => file.export("dxf")}>Export 3D DXF (mm)…</button>
                  <button onClick={() => file.export("dwg")}>Export DWG (AutoCAD)…</button>
                  <button onClick={() => file.export("pdf")}>Export PDF (this view)…</button>
                  <button onClick={() => setShowTimeline(true)}>Timeline (video)…</button>
                  <button onClick={file.animation}>Export scene tour (MP4)…</button>
                  <button onClick={file.blueprint}>Make blueprint (PDF)…</button>
                  <button onClick={() => setShowSheets(true)}>Drawing sheets (PDF)…</button>
                  <button onClick={() => file.export("ifc")}>Export IFC (BIM)…</button>
                  <button onClick={() => setShowLocation(true)}>Add location…</button>
                  <button onClick={() => setShowText3d(true)}>3D Text…</button>
                  <button onClick={file.report}>Generate report (CSV)…</button>
                  <button onClick={file.render}>Render PNG…</button>
                  {section && (
                    <>
                      <button onClick={() => file.section("svg")}>Export section cut (SVG)…</button>
                      <button onClick={() => file.section("dxf")}>Export section cut (DXF)…</button>
                    </>
                  )}
                  <hr />
                  <button onClick={() => setShowKeys(true)}>
                    Keyboard shortcuts <kbd>?</kbd>
                  </button>
                </div>
              </>
            )}
          </div>
          <span className="doc">{docName}</span>
          <span className="live" title="Claude can edit this model over MCP; changes show up here">
            <i /> Live
          </span>
          {(scene?.editing != null || scene?.activeGroup != null) && (
            <button className="editing-pill" onClick={() => run({ cmd: "close" })}>
              Editing {scene?.editing != null ? "component" : "group"} · Done
            </button>
          )}
        </div>
        <div className="ws-switch" role="tablist" aria-label="Workspace">
          {WORKSPACES.map((w) => (
            <button key={w.id} role="tab" aria-selected={wsId === w.id} className={wsId === w.id ? "on" : ""} title={w.blurb} onClick={() => switchWs(w.id)}>
              {w.name}
            </button>
          ))}
          <button title={`Customize the ${ws.name} workspace: tools, toolbar, panels`} onClick={() => setShowWs(true)} aria-label="Customize workspace">
            ⋯
          </button>
        </div>
        <div className="top-actions">
          <button className="btn small ghost learn-btn" title="Lessons and demos: learn SketchCraft by doing" onClick={() => setShowLearn(true)}>
            Learn{lessonsDone().length === 0 && <i className="learn-dot" aria-label="(new)" />}
          </button>
          <button className="icon-btn skin-btn" title="Skins: change how SketchCraft looks" onClick={() => setShowSkins(true)} aria-label="Skins">
            <Icons.paint />
          </button>
          <button className="icon-btn" title="Undo (⌘Z)" onClick={() => run({ cmd: "undo" })}>
            <Icons.undo />
          </button>
          <button className="icon-btn" title="Redo (⇧⌘Z)" onClick={() => run({ cmd: "redo" })}>
            <Icons.redo />
          </button>
          <input ref={fileRef} type="file" accept=".obj,.skp,.fbx,.dxf" hidden onChange={(e) => e.target.files?.[0] && importFile(e.target.files[0])} />
          <button className="btn primary" onClick={() => setShowRender(true)} title="A photoreal picture of this view">
            <Icons.camera /> Render
          </button>
        </div>
      </header>

      <aside className="panel left">
        <div className="tabs">
          {(["outliner", "library", "views"] as Tab[]).map((t) => (
            <button key={t} className={tab === t ? "on" : ""} onClick={() => setTab(t)}>
              {t === "outliner" ? "Outliner" : t === "library" ? "Library" : "Views"}
            </button>
          ))}
        </div>
        <div className="section grow">
          {scene && tab === "outliner" && <Outliner scene={scene} sel={sel} setSel={setSel} run={run} byTag={!!styles.byTag} setByTag={(on) => setStyles((s) => ({ ...s, byTag: on }))} />}
          {scene && tab === "library" && (
            <>
              <Library
                scene={scene}
                placing={placing}
                run={run}
                onToast={say}
                place={(k) => {
                  setPlacing(k);
                  setTool(k ? "place" : "select");
                }}
              />
              <details className="panel-fold" open>
                <summary>Build</summary>
                <BuildTools tool={tool} setTool={chooseTool} build={build} setBuild={setBuild} units={scene.units || "m"} />
              </details>
              <details className="panel-fold">
                <summary>Sandbox (terrain)</summary>
                <Sandbox tool={tool} setTool={chooseTool} build={build} setBuild={setBuild} units={scene.units || "m"} run={run} sel={sel} onToast={say} />
              </details>
              <details className="panel-fold" open={!!scene.environment || undefined}>
                <summary>Environments</summary>
                <Environments scene={scene} run={run} onToast={say} />
              </details>
              <details className="panel-fold">
                <summary>Templates</summary>
                <Templates onTemplate={runTemplate} />
              </details>
            </>
          )}
          {scene && tab === "views" && <Views scene={scene} run={run} camera={() => api.current!.camera()} goTo={(c) => api.current?.goTo(c)} onMatchPhoto={startMatch} onPhotoModel={() => setShowPhotoModel(true)} onRefImage={() => setShowRef(true)} />}
        </div>
      </aside>

      <main className="stage">
        <Viewport
          scene={scene}
          tool={tool}
          setTool={chooseTool}
          sel={sel}
          setSel={setSel}
          color={color}
          setColor={setColor}
          paintMaterial={paintMaterial}
          setPaintMaterial={setPaintMaterial}
          styles={styles}
          placeAsset={placing}
          build={build}
          run={run}
          onMeasure={(label, value) => setMeasure({ label, value })}
          onContext={(x, y) => setMenu({ x, y })}
          onToast={say}
          onSection={(on) => {
            setSection(on);
            setSectionTick((t) => t + 1);
          }}
          api={api}
        />
        {matching && <MatchPhoto src={matching.src} width={matching.width} height={matching.height} defaultName={`Photo ${(scene?.views.filter((v) => v.photo).length ?? 0) + 1}`} onDone={finishMatch} onCancel={() => setMatching(null)} />}

        <nav className="toolbar">
          {ws.tools.map((g, i) => (
            <div key={i} className="tool-group">
              {g.map((id) => {
                const t = TOOL_INFO[id as keyof typeof TOOL_INFO];
                return (
                  <button key={id} className={`tool ${tool === id ? "on" : ""}`} onClick={() => chooseTool(id)} title={t.key ? `${t.name} (${t.key})` : t.name}>
                    {Icons[id as keyof typeof Icons]()}
                  </button>
                );
              })}
            </div>
          ))}
          <label className="chip" style={{ background: hex(color) }} title="Paint color">
            <input type="color" value={hex(color)} onChange={(e) => (setColor(unhex(e.target.value)), chooseTool("paint"))} />
          </label>
        </nav>

        <div className="views">
          {(["iso", "top", "front", "right"] as const).map((v) => (
            <button key={v} onClick={() => api.current?.view(v)}>
              {v}
            </button>
          ))}
          <button onClick={() => api.current?.fit()} title="Zoom extents (⇧Z)">
            fit
          </button>
          <button onClick={() => api.current?.previousView()} title="Previous view (⌘[)" aria-label="Previous view">
            ‹
          </button>
          <button onClick={() => api.current?.nextView()} title="Next view (⌘])" aria-label="Next view">
            ›
          </button>
          <span className="vsep" />
          {(
            [
              ["xray", "X-ray"],
              ["backEdges", "Back edges"],
              ["shadows", "Shadows"],
              ["edges", "Edges"],
              ["parallel", "Parallel"],
              ["notes", "Notes"],
            ] as [keyof Styles, string][]
          ).map(([k, label]) => (
            <button
              key={k}
              className={(k === "notes" ? styles.notes !== false : styles[k]) ? "on" : ""}
              title={k === "notes" ? "Dimensions, labels and door swings" : undefined}
              onClick={() => setStyles((s) => ({ ...s, [k]: k === "notes" ? s.notes === false : !s[k] }))}
            >
              {label}
            </button>
          ))}
          {!section && (
            <button
              onClick={() => {
                api.current?.planCut(1.22);
                api.current?.view("top");
              }}
              title="Floor plan: cut the model 4 ft up and look down. Change the height to see other floors."
            >
              Plan
            </button>
          )}
          {section && (
            <span className="section-ctl">
              <button title="Move the cut up 1 ft (or back, for a side cut)" onClick={() => api.current?.moveSection(-0.3048)}>
                ▲
              </button>
              <button title="Move the cut down 1 ft (or in, for a side cut)" onClick={() => api.current?.moveSection(0.3048)}>
                ▼
              </button>
              <SectionHeight api={api} units={scene?.units || "m"} tick={sectionTick} />
              <button
                className="on"
                onClick={() => {
                  api.current?.clearSection();
                  setSection(false);
                }}
                title="Remove the section cut"
              >
                Section ✕
              </button>
            </span>
          )}
        </div>

        <form
          className="measure"
          onSubmit={(e) => {
            e.preventDefault();
            api.current?.applyTyped(typed);
            setTyped("");
            typedRef.current?.blur();
          }}
        >
          <label>{measure.label || "Value"}</label>
          <input ref={typedRef} value={typed} placeholder={measure.value || "type, then Enter"} onChange={(e) => setTyped(e.target.value)} onKeyDown={(e) => e.key === "Escape" && (setTyped(""), typedRef.current?.blur())} />
        </form>

        {scene && !showTimeline && (
          <WorkspaceStrip
            ws={wsId}
            scene={scene}
            sel={sel}
            setSel={setSel}
            run={run}
            api={api}
            section={section}
            setSection={(on) => {
              setSection(on);
              setSectionTick((t) => t + 1);
            }}
            actions={{ blueprint: file.blueprint, sheets: () => setShowSheets(true), location: () => setShowLocation(true), render: () => setShowRender(true), animation: () => setShowTimeline((v) => !v) }}
          />
        )}
        {scene && showTimeline && <TimelinePanel scene={scene} run={run} api={api} setTool={chooseTool} onToast={say} onClose={() => setShowTimeline(false)} />}
        {showLearn && (
          <LearnCenter
            onStart={(id) => {
              setShowLearn(false);
              setLesson(null);
              window.setTimeout(() => setLesson(id), 0);
            }}
            onClose={() => setShowLearn(false)}
          />
        )}
        {lesson && learnCtx && <LessonCard key={lesson} id={lesson} ctx={learnCtx} onClose={() => setLesson(null)} onToast={say} />}
        {showText3d && scene && <Text3dDialog scene={scene} run={run} camera={() => api.current!.camera()} onClose={() => setShowText3d(false)} onToast={say} />}
        {showRef && scene && <RefImageDialog scene={scene} run={run} onClose={() => setShowRef(false)} onToast={say} />}
        {showPhotoModel && scene && <PhotoModeler scene={scene} run={run} onClose={() => setShowPhotoModel(false)} onToast={say} />}
        {showSkins && <AppearanceDialog current={skin} onPick={wearSkin} onClose={() => setShowSkins(false)} onToast={say} />}
        {showWs && <WorkspaceDialog ws={ws} onChange={editWs} onReset={() => editWs(null)} onClose={() => setShowWs(false)} />}
        {toast && <div className={`toast ${toast.bad ? "bad" : ""}`}>{toast.text}</div>}
        <AskHost />
        {showRender && api.current && (
          <RenderDialog
            camera={() => api.current!.camera()}
            aspect={(() => {
              const c = document.querySelector(".viewport canvas") as HTMLCanvasElement | null;
              return c && c.clientHeight ? c.clientWidth / c.clientHeight : 1.6;
            })()}
            name={(currentFile?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "")}
            onClose={() => setShowRender(false)}
            onToast={say}
          />
        )}
        {showKeys && <ShortcutSheet onClose={() => setShowKeys(false)} />}
        {showLocation && (
          <LocationDialog
            current={scene?.location}
            onClose={() => setShowLocation(false)}
            onAdd={async (p) => {
              setShowLocation(false);
              say("Fetching the ground and map…");
              const r = await run({ cmd: "add_location", ...p });
              if (r.ok) {
                say(`Location added: ground ${Math.round(r.elevation as number)} m above sea level`);
                api.current?.fitSoon();
              }
            }}
          />
        )}
        {showSheets && scene && (
          <SheetsDialog
            scene={scene}
            onClose={() => setShowSheets(false)}
            onMake={async (spec) => {
              const path = await pickSave(`${spec.project} sheets.pdf`, "pdf", "PDF");
              if (!path) return;
              setShowSheets(false);
              say("Drawing the sheets…");
              const r = await run({ cmd: "sheets", path, ...spec });
              if (r.ok) say(`Made ${r.pages} sheet${r.pages === 1 ? "" : "s"}: ${path.split("/").pop()}`);
            }}
          />
        )}
        {menu && scene && <ContextMenu x={menu.x} y={menu.y} close={() => setMenu(null)} items={contextItems(scene, sel, run, setSel, () => api.current?.fit(), api.current?.sectionPlane(), (id) => api.current?.editCage(id), say)} />}
      </main>

      <aside className="panel right">
        {scene && selCount > 0 && <EntityInfo scene={scene} sel={sel} run={run} />}
        {selCount === 0 && (
          <div className="section">
            <div className="section-title">Document</div>
            <div className="kv">
              <span>Faces</span>
              <b>{scene?.faces.length ?? 0}</b>
              <span>Groups</span>
              <b>{scene?.groups.length ?? 0}</b>
              <span>Components</span>
              <b>{scene?.instances.length ?? 0}</b>
              <span>Units</span>
              <select className="units-select" value={scene?.units || "m"} onChange={(e) => run({ cmd: "units", units: e.target.value })}>
                {UNIT_OPTIONS.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.label}
                  </option>
                ))}
              </select>
            </div>
          </div>
        )}
        {selCount === 0 && (
          <div className="section">
            <div className="section-title">Style</div>
            <div className="kv">
              <span>Preset</span>
              <select
                className="units-select"
                value=""
                onChange={(e) => {
                  const p = STYLE_PRESETS.find((x) => x.name === e.target.value);
                  if (p) setStyles((s) => ({ ...s, ...p.styles }));
                }}
              >
                <option value="">Choose…</option>
                {STYLE_PRESETS.map((p) => (
                  <option key={p.name}>{p.name}</option>
                ))}
              </select>
              <span>Faces</span>
              <select className="units-select" value={styles.face ?? "textured"} onChange={(e) => setStyles((s) => ({ ...s, face: e.target.value as FaceStyle }))}>
                <option value="textured">Shaded with textures</option>
                <option value="shaded">Shaded</option>
                <option value="monochrome">Monochrome</option>
                <option value="hiddenline">Hidden line</option>
                <option value="wireframe">Wireframe</option>
              </select>
              <span>Background</span>
              <select className="units-select" value={styles.background ?? "default"} onChange={(e) => setStyles((s) => ({ ...s, background: e.target.value as Backdrop }))}>
                <option value="default">Sky and ground</option>
                <option value="paper">Paper</option>
                <option value="blueprint">Blueprint</option>
                <option value="night">Night</option>
              </select>
            </div>
            <div className="style-checks">
              {(
                [
                  ["profiles", "Profiles"],
                  ["sketchy", "Sketchy edges"],
                  ["extensions", "Extensions"],
                  ["ao", "Ambient occlusion"],
                ] as [keyof Styles, string][]
              ).map(([k, label]) => (
                <label key={k} className="field check">
                  <input type="checkbox" checked={!!styles[k]} onChange={(e) => setStyles((s) => ({ ...s, [k]: e.target.checked }))} />
                  <span>{label}</span>
                </label>
              ))}
            </div>
          </div>
        )}
        <div className="section">
          <div className="section-title">{selCount ? "Fill" : "Paint"}</div>
          <div className="swatches">
            {SWATCHES.map((c) => (
              <button
                key={hex(c)}
                className={`swatch ${!paintMaterial && hex(c) === hex(color) ? "on" : ""}`}
                style={{ background: hex(c) }}
                onClick={() => {
                  setColor(c);
                  const ids = [...sel.faces, ...sel.groups];
                  if (ids.length) run({ cmd: "paint", ids, color: c });
                }}
              />
            ))}
          </div>
        </div>
        {scene && (
          <Textures
            scene={scene}
            paintMaterial={paintMaterial}
            choose={(n) => {
              setPaintMaterial(n);
              if (n) chooseTool(selCount ? tool : "paint");
            }}
            run={run}
            camera={() => api.current!.camera()}
            sel={sel}
            onToast={say}
          />
        )}
        {selCount > 0 && (
          <div className="section">
            <div className="section-title">Arrange</div>
            <div className="btn-grid">
              {(sel.faces.length > 0 || sel.groups.length + sel.instances.length > 1) && (
                <button className="btn small" title="Groups and components selected go inside the new group" onClick={() => run({ cmd: "group", ids: [...sel.faces, ...sel.groups, ...sel.instances] }).then((r) => r.ok && setSel({ ...EMPTY, groups: [r.group as number] }))}>
                  Make group
                </button>
              )}
              {(sel.faces.length > 0 || sel.groups.length > 0) && (
                <button
                  className="btn small"
                  onClick={async () => {
                    const name = await ask("Component name", "Component");
                    if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
                  }}
                >
                  Make component
                </button>
              )}
              {sel.instances.length > 0 && (
                <button className="btn small" onClick={() => Promise.all(sel.instances.map((id) => run({ cmd: "explode", id }))).then(() => setSel(EMPTY))}>
                  Explode
                </button>
              )}
              <button className="btn small ghost" onClick={() => run({ cmd: "hide", ids: selIds(sel) }).then(() => setSel(EMPTY))}>
                Hide
              </button>
              <button className="btn small danger" onClick={deleteSelection}>
                Delete
              </button>
            </div>
          </div>
        )}
        <div className="section hint">
          <p>
            Each tool tells you what to do next, top-left of the model. Press <b>?</b> for every shortcut.
          </p>
        </div>
      </aside>
    </div>
  );
}
