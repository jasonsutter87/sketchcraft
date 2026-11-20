// What each menu item does, and whether it is available or ticked right now. App.tsx fills in `MenuCtx` with the
// app's own handlers (the same ones the keyboard, the command search and the buttons use), so a menu item never
// has a second implementation of anything. Pure (no React): the Vitest suite builds this with stand-ins.

import type { Styles, Tool } from "./Viewport";
import { BACKGROUNDS, EXPORT_FORMATS, FACE_STYLES, LANGUAGES, LAYOUTS } from "./menumodel";
import { WORKSPACES, type Layout } from "./workspace";

export interface MenuAction {
  run: () => void;
  /** false: shown greyed out */
  enabled: boolean;
  /** a ticked check item or the chosen radio item */
  checked?: boolean;
  /** not shown at all (Check for updates, outside the desktop app) */
  hidden?: boolean;
}

/** everything the menus read from the app, and every handler they call */
export interface MenuCtx {
  // the state the menus look at
  selection: { total: number; faces: number; groups: number; instances: number };
  canPaste: boolean;
  /** inside a group or component that is open for editing */
  editing: boolean;
  /** a section cut is showing */
  section: boolean;
  /** the Layout workspace (paper) is up: the model's tools and views are put away */
  onPaper: boolean;
  workspace: string;
  styles: Styles;
  layout: Layout;
  language: string;
  showLeft: boolean;
  showRight: boolean;
  leftTabs: string[];
  desktop: boolean;
  // files
  file: {
    new: () => void;
    open: () => void;
    save: (as: boolean) => void;
    import: () => void;
    upload: () => void;
    export: (format: string) => void;
    section: (format: "svg" | "dxf") => void;
    png: () => void;
    tour: () => void;
    blueprint: () => void;
    report: () => void;
  };
  // the edit commands
  edit: {
    undo: () => void;
    redo: () => void;
    cut: () => void;
    copy: () => void;
    paste: (inPlace: boolean) => void;
    delete: () => void;
    selectAll: () => void;
    selectNone: () => void;
    group: () => void;
    component: () => void;
    explode: () => void;
    hide: () => void;
    unhide: () => void;
    clearGuides: () => void;
    resetAxes: () => void;
  };
  // the camera and the view
  tool: (t: Tool) => void;
  view: (name: "iso" | "top" | "front" | "right" | "back" | "left") => void;
  fit: () => void;
  previousView: () => void;
  nextView: () => void;
  plan: () => void;
  clearSection: () => void;
  toggleStyle: (key: "shadows" | "xray" | "backEdges" | "edges" | "profiles" | "vertexColors" | "sketchy" | "ao" | "notes" | "help" | "hideRest") => void;
  setStyles: (patch: Partial<Styles>) => void;
  // panels, layout, language
  workspaceTo: (id: string) => void;
  showTab: (tab: "outliner" | "library" | "views") => void;
  showRightPanel: () => void;
  togglePanel: (side: "left" | "right") => void;
  setLayout: (l: Layout) => void;
  setLanguage: (l: string) => void;
  // dialogs and links
  open: Record<"restore" | "render" | "sheets" | "layout" | "timeline" | "location" | "text3d" | "projection" | "plugins" | "themes" | "keys" | "keyEditor" | "palette" | "learn" | "about" | "customize", () => void>;
  checkForUpdates: () => void;
  openLink: (which: "issues") => void;
}

/** every menu action by id */
export function buildMenuActions(c: MenuCtx): Record<string, MenuAction> {
  const out: Record<string, MenuAction> = {};
  const add = (id: string, run: () => void, enabled = true, checked?: boolean) => {
    out[id] = { run, enabled, ...(checked === undefined ? {} : { checked }) };
  };
  const model = !c.onPaper; // the model's own tools and keys do nothing on paper
  const sel = c.selection;
  const has = sel.total > 0;
  const s = c.styles;

  // File
  add("file.new", c.file.new);
  add("file.open", c.file.open);
  add("file.save", () => c.file.save(false));
  add("file.saveas", () => c.file.save(true));
  add("file.restore", c.open.restore);
  add("file.import", c.file.import);
  add("file.upload", c.file.upload);
  for (const [k] of EXPORT_FORMATS) add(`file.export.${k}`, () => c.file.export(k));
  add("file.export.pdf", () => c.file.export("pdf"));
  add("file.export.png", c.file.png);
  add("file.export.tour", c.file.tour);
  add("file.export.section.svg", () => c.file.section("svg"), c.section);
  add("file.export.section.dxf", () => c.file.section("dxf"), c.section);
  add("file.render", c.open.render);
  add("file.blueprint", c.file.blueprint);
  add("file.layout", c.open.layout);
  add("file.sheets", c.open.sheets);
  add("file.timeline", c.open.timeline);
  add("file.location", c.open.location);
  add("file.text3d", c.open.text3d);
  add("file.report", c.file.report);
  add("win.plugins", c.open.plugins);

  // Edit
  add("edit.undo", c.edit.undo);
  add("edit.redo", c.edit.redo);
  add("edit.cut", c.edit.cut, model && has);
  add("edit.copy", c.edit.copy, model && has);
  add("edit.paste", () => c.edit.paste(false), model && c.canPaste);
  add("edit.pasteinplace", () => c.edit.paste(true), model && c.canPaste);
  add("edit.delete", c.edit.delete, model && has);
  add("edit.selectall", c.edit.selectAll, model);
  add("edit.selectnone", c.edit.selectNone, model && has);
  add("edit.group", c.edit.group, model && (sel.faces > 0 || sel.groups + sel.instances > 1));
  add("edit.makecomponent", c.edit.component, model && (sel.faces > 0 || sel.groups > 0));
  add("edit.explode", c.edit.explode, model && sel.instances > 0);
  add("edit.hide", c.edit.hide, model && has);
  add("edit.unhide", c.edit.unhide);
  add("edit.clearguides", c.edit.clearGuides, model);
  add("edit.resetaxes", c.edit.resetAxes, model);

  // View
  for (const w of WORKSPACES) add(`ws.${w.id}`, () => c.workspaceTo(w.id), true, c.workspace === w.id);
  for (const f of FACE_STYLES) add(`view.face.${f}`, () => c.setStyles({ face: f }), model, (s.face ?? "textured") === f);
  for (const b of BACKGROUNDS) add(`view.bg.${b}`, () => c.setStyles({ background: b }), model, (s.background ?? "default") === b);
  add("view.shadows", () => c.toggleStyle("shadows"), model, !!s.shadows);
  add("view.xray", () => c.toggleStyle("xray"), model, !!s.xray);
  add("view.backedges", () => c.toggleStyle("backEdges"), model, !!s.backEdges);
  add("view.edges", () => c.toggleStyle("edges"), model, !!s.edges);
  add("view.profiles", () => c.toggleStyle("profiles"), model, !!s.profiles);
  add("view.vertexcolors", () => c.toggleStyle("vertexColors"), model, s.vertexColors !== false);
  add("view.sketchy", () => c.toggleStyle("sketchy"), model, !!s.sketchy);
  add("view.ao", () => c.toggleStyle("ao"), model, !!s.ao);
  add("view.notes", () => c.toggleStyle("notes"), model, s.notes !== false);
  add("view.coach", () => c.toggleStyle("help"), model, s.help !== false);
  add("view.hiderest", () => c.toggleStyle("hideRest"), model && c.editing, !!s.hideRest);
  add("view.plan", c.plan, model && !c.section);
  add("view.section.clear", c.clearSection, model && c.section);
  add("view.panel.left", () => c.togglePanel("left"), true, c.showLeft);
  add("view.panel.right", () => c.togglePanel("right"), true, c.showRight);
  for (const l of LAYOUTS) add(`view.layout.${l}`, () => c.setLayout(l), true, c.layout === l);
  add("view.customize", c.open.customize);
  add("win.themes", c.open.themes);
  for (const l of LANGUAGES) add(`lang.${l}`, () => c.setLanguage(l), true, c.language === l);

  // Camera
  for (const v of ["iso", "top", "front", "right", "back", "left"] as const) add(`view.${v}`, () => c.view(v), model);
  add("view.fit", c.fit, model);
  add("view.prev", c.previousView, model);
  add("view.next", c.nextView, model);
  add("view.perspective", () => c.setStyles({ parallel: false }), model, !s.parallel);
  add("view.parallel", () => c.setStyles({ parallel: true }), model, !!s.parallel);

  // every tool a menu offers (Draw, Tools, Camera)
  for (const id of ["orbit", "pan", "zoom", "zoomwindow", "position", "lookaround", "walk", "line", "freehand", "arc", "arccenter", "arc3", "pie", "rect", "rrect", "circle", "polygon", "wall", "room", "select", "move", "rotate", "scale", "push", "wallpull", "followme", "offset", "tape", "dimension", "text", "paint", "vpaint", "erase", "section"] as const) {
    add(`tool:${id}`, () => c.tool(id), model);
  }

  // Project texture: a panel over a selection (the same action the command search and the context menu run)
  add("tools.projecttexture", c.open.projection, model && has);

  // Window
  add("win.outliner", () => c.showTab("outliner"), c.leftTabs.includes("outliner"));
  add("win.library", () => c.showTab("library"), c.leftTabs.includes("library"));
  add("win.views", () => c.showTab("views"), c.leftTabs.includes("views"));
  add("win.info", c.showRightPanel);
  add("win.keyeditor", c.open.keyEditor);

  // Help
  add("help.learn", c.open.learn);
  add("help.shortcuts", c.open.keys);
  add("help.search", c.open.palette);
  add("help.updates", c.checkForUpdates);
  out["help.updates"].hidden = !c.desktop;
  add("help.bug", () => c.openLink("issues"));
  add("help.about", c.open.about);
  return out;
}
