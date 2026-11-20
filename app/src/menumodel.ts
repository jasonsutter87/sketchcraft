// The menu bar as data: which menus there are, what is in them, and the id of the action each item runs.
// The actions themselves are in menuactions.ts; the menus are drawn by menubar.tsx. This file is pure (no React),
// so the Vitest suite checks that every item has an action and that its shortcut comes from the shortcuts list.
//
// An item's `id` is the id of its action. Where a shortcut exists it is the same id the shortcuts list uses
// ("file.save", "tool:line", "view.xray"), so the key shown beside the item is whatever the person has set.

import { WORKSPACES } from "./workspace";

/** one item of a menu */
export type MenuItem =
  | { sep: true }
  /** runs an action. `label` is a message id (locale-en.ts), `text` a name that is the same in every language. */
  | { id: string; label?: string; text?: string; kind?: "check" | "radio"; tool?: boolean }
  | { label: string; items: MenuItem[] }
  /** items that come and go: the commands of the plugins that are on */
  | { slot: "plugins" };

export interface Menu {
  id: string;
  /** message id of its name */
  label: string;
  items: MenuItem[];
}

const SEP: MenuItem = { sep: true };
const act = (id: string, label: string, kind?: "check" | "radio"): MenuItem => ({ id, label, ...(kind ? { kind } : {}) });
/** a tool; `label` where its short name would read the same as another's (two kinds of "Arc") */
const tool = (name: string, label = `tool.${name}`): MenuItem => ({ id: `tool:${name}`, label, tool: label.startsWith("tool.") });

/** the formats of File > Export: [engine format, name]; the same name in every language */
export const EXPORT_FORMATS: [string, string][] = [
  ["glb", "glTF (.glb)…"],
  ["fbx", "FBX (.fbx)…"],
  ["obj", "OBJ…"],
  ["stl", "STL (mm)…"],
  ["dae", "Collada (.dae)…"],
  ["dxf", "3D DXF (mm)…"],
  ["dwg", "DWG (AutoCAD)…"],
  ["ifc", "IFC (BIM)…"],
];

export const FACE_STYLES = ["textured", "physical", "shaded", "monochrome", "hiddenline", "wireframe"] as const;
export const BACKGROUNDS = ["default", "paper", "blueprint", "night"] as const;
export const LAYOUTS = ["floating", "classic", "side"] as const;
export const LANGUAGES = ["system", "en", "it"] as const;

export const MENUS: Menu[] = [
  {
    id: "file",
    label: "menu.file",
    items: [
      act("file.new", "menu.new"),
      act("file.open", "menu.open"),
      SEP,
      act("file.save", "menu.save"),
      act("file.saveas", "menu.saveAs"),
      act("file.restore", "menu.restore"),
      SEP,
      act("file.import", "menu.import"),
      act("file.upload", "menu.upload"),
      {
        label: "menu.export",
        items: [
          ...EXPORT_FORMATS.map(([k, text]): MenuItem => ({ id: `file.export.${k}`, text })),
          act("file.export.pdf", "menu.pdf"),
          act("file.export.png", "menu.png"),
          act("file.export.tour", "menu.tour"),
          act("file.export.section.svg", "menu.svgSection"),
          act("file.export.section.dxf", "menu.dxfSection"),
        ],
      },
      act("file.render", "menu.render"),
      SEP,
      act("file.blueprint", "menu.blueprint"),
      act("file.layout", "menu.layout"),
      act("file.sheets", "menu.sheets"),
      act("file.timeline", "menu.timeline"),
      SEP,
      act("file.location", "menu.location"),
      act("file.text3d", "menu.text3d"),
      act("file.report", "menu.report"),
      SEP,
      { label: "menu.plugins", items: [{ slot: "plugins" }, act("win.plugins", "menu.manage")] },
    ],
  },
  {
    id: "edit",
    label: "menu.edit",
    items: [
      act("edit.undo", "menu.undo"),
      act("edit.redo", "menu.redo"),
      SEP,
      act("edit.cut", "menu.cut"),
      act("edit.copy", "menu.copy"),
      act("edit.paste", "menu.paste"),
      act("edit.pasteinplace", "menu.pasteInPlace"),
      act("edit.delete", "menu.delete"),
      SEP,
      act("edit.selectall", "menu.selectAll"),
      act("edit.selectnone", "menu.selectNone"),
      SEP,
      act("edit.group", "menu.group"),
      act("edit.makecomponent", "menu.component"),
      act("edit.explode", "menu.explode"),
      SEP,
      act("edit.hide", "menu.hide"),
      act("edit.unhide", "menu.unhide"),
      SEP,
      act("edit.clearguides", "menu.clearGuides"),
      act("edit.resetaxes", "menu.resetAxes"),
    ],
  },
  {
    id: "view",
    label: "menu.view",
    items: [
      { label: "menu.workspaces", items: [...WORKSPACES.map((w): MenuItem => ({ id: `ws.${w.id}`, text: w.name, kind: "radio" }))] },
      { label: "menu.faceStyle", items: FACE_STYLES.map((f) => act(`view.face.${f}`, `menu.face.${f}`, "radio")) },
      { label: "menu.background", items: BACKGROUNDS.map((b) => act(`view.bg.${b}`, `menu.bg.${b}`, "radio")) },
      SEP,
      act("view.shadows", "menu.shadows", "check"),
      act("view.xray", "menu.xray", "check"),
      act("view.backedges", "menu.backEdges", "check"),
      act("view.edges", "menu.edges", "check"),
      act("view.profiles", "menu.profiles", "check"),
      act("view.vertexcolors", "menu.vertexColors", "check"),
      act("view.sketchy", "menu.sketchy", "check"),
      act("view.ao", "menu.ao", "check"),
      SEP,
      act("view.notes", "menu.notes", "check"),
      act("view.coach", "menu.coach", "check"),
      act("view.hiderest", "menu.hideRest", "check"),
      SEP,
      act("view.plan", "menu.plan"),
      act("view.section.clear", "menu.clearSection"),
      SEP,
      act("view.panel.left", "menu.leftPanel", "check"),
      act("view.panel.right", "menu.rightPanel", "check"),
      { label: "menu.toolbarLayout", items: LAYOUTS.map((l) => act(`view.layout.${l}`, `menu.layout.${l}`, "radio")) },
      act("view.customize", "menu.customizeWs"),
      SEP,
      act("win.themes", "menu.themes"),
      { label: "language.label", items: LANGUAGES.map((l): MenuItem => (l === "system" ? act("lang.system", "language.system", "radio") : { id: `lang.${l}`, text: l === "en" ? "English" : "Italiano", kind: "radio" })) },
    ],
  },
  {
    id: "camera",
    label: "menu.camera",
    items: [
      { label: "menu.standardViews", items: ["iso", "top", "front", "right", "back", "left"].map((v) => act(`view.${v}`, `menu.view.${v}`)) },
      act("view.fit", "menu.fit"),
      act("view.prev", "menu.prevView"),
      act("view.next", "menu.nextView"),
      SEP,
      act("view.perspective", "menu.perspective", "radio"),
      act("view.parallel", "menu.parallel", "radio"),
      SEP,
      tool("orbit"),
      tool("pan"),
      tool("zoom"),
      tool("zoomwindow"),
      SEP,
      tool("position"),
      tool("lookaround"),
      tool("walk"),
    ],
  },
  {
    id: "draw",
    label: "menu.draw",
    items: [
      tool("line"),
      tool("freehand"),
      SEP,
      tool("arc", "menu.tool.arc"),
      tool("arccenter", "menu.tool.arccenter"),
      tool("arc3"),
      tool("pie"),
      SEP,
      tool("rect"),
      tool("rrect"),
      tool("circle"),
      tool("polygon"),
      SEP,
      tool("wall"),
      tool("room"),
    ],
  },
  {
    id: "tools",
    label: "menu.tools",
    items: [
      tool("select"),
      SEP,
      tool("move"),
      tool("rotate"),
      tool("scale"),
      SEP,
      tool("push"),
      tool("wallpull"),
      tool("followme"),
      tool("offset"),
      SEP,
      tool("tape"),
      tool("dimension"),
      tool("text"),
      SEP,
      tool("paint"),
      tool("vpaint"),
      act("tools.projecttexture", "menu.projectTexture"),
      act("tools.positiontexture", "menu.positionTexture"),
      act("tools.uveditor", "menu.uvEditor"),
      tool("erase"),
      tool("section"),
    ],
  },
  {
    id: "window",
    label: "menu.window",
    items: [
      act("win.outliner", "menu.outliner"),
      act("win.library", "menu.library"),
      act("win.views", "menu.scenes"),
      act("win.info", "menu.entityInfo"),
      act("win.uveditor", "menu.uvEditor"),
      SEP,
      act("win.themes", "menu.themes"),
      act("win.keyeditor", "menu.customize"),
      act("win.plugins", "menu.manage"),
    ],
  },
  {
    id: "help",
    label: "menu.help",
    items: [
      act("help.learn", "menu.learn"),
      act("help.shortcuts", "menu.shortcuts"),
      act("help.search", "menu.commands"),
      SEP,
      act("help.updates", "menu.updates"),
      act("help.bug", "menu.bug"),
      SEP,
      act("help.about", "menu.about"),
    ],
  },
];

/** every item that runs something, in menu order (submenus opened) */
export function leaves(items: MenuItem[] = MENUS.flatMap((m) => m.items)): Extract<MenuItem, { id: string }>[] {
  return items.flatMap((it) => ("sep" in it || "slot" in it ? [] : "items" in it ? leaves(it.items) : [it]));
}

/** "Move (Option/Ctrl to copy)" is "Move"; "Arc: ends, then bulge" is "Arc": the name a menu shows for a tool */
export const shortToolName = (name: string) => name.split(":")[0].split(" (")[0].trim();
