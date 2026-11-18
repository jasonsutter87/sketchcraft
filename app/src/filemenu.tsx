// The File menu in the top bar: files, exports, plugins, and the dialogs the rest of the app opens.
import { useState, type RefObject } from "react";
import { inTauri } from "./engine";
import { keyText } from "./keymapstore";
import { loadPlugins, type Plugin, type PluginList } from "./plugins";
import { checkForUpdatesNow } from "./updater";
import { useEscapeToClose } from "./sheet";
import type { useFileActions } from "./usefiles";

export function FileMenu({ file, section, plugins, livePlugins, setPlugins, runPluginCommand, reloadPlugins, fileRef, open }: {
  file: ReturnType<typeof useFileActions>["file"];
  /** a section cut is showing (it can be exported) */
  section: boolean;
  plugins: PluginList;
  livePlugins: Plugin[];
  setPlugins: (p: PluginList) => void;
  runPluginCommand: (plugin: Plugin, commandId: string) => Promise<void> | undefined;
  reloadPlugins: () => void;
  fileRef: RefObject<HTMLInputElement | null>;
  /** the dialogs the menu opens, by what they are */
  open: { restore: () => void; plugins: () => void; sheets: () => void; timeline: () => void; location: () => void; text3d: () => void; keys: () => void; keyEditor: () => void };
}) {
  const [fileMenu, setFileMenu] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [pluginsOpen, setPluginsOpen] = useState(false);
  useEscapeToClose(() => setFileMenu(false), fileMenu);
  return (
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
          <button onClick={() => open.restore()}>Restore from backup…</button>
          <hr />
          <button onClick={file.import}>Import SketchUp, CAD or OBJ… <kbd>⌘I</kbd></button>
          <button onClick={() => fileRef.current?.click()}>Import from browser upload…</button>
          <button
            className="submenu-btn"
            aria-expanded={exportOpen}
            onClick={(e) => {
              e.stopPropagation();
              setExportOpen((o) => !o);
            }}
          >
            Export <span className="caret">{exportOpen ? "▾" : "▸"}</span>
          </button>
          {exportOpen && (
            <div className="submenu">
              <button onClick={() => file.export("glb")}>glTF (.glb)…</button>
              <button onClick={() => file.export("obj")}>OBJ…</button>
              <button onClick={() => file.export("stl")}>STL (mm)…</button>
              <button onClick={() => file.export("dae")}>Collada (.dae)…</button>
              <button onClick={() => file.export("dxf")}>3D DXF (mm)…</button>
              <button onClick={() => file.export("dwg")}>DWG (AutoCAD)…</button>
              <button onClick={() => file.export("ifc")}>IFC (BIM)…</button>
              <button onClick={() => file.export("pdf")}>PDF (this view)…</button>
              <button onClick={file.render}>PNG (this view)…</button>
              <button onClick={file.animation}>Scene tour (MP4)…</button>
              {section && (
                <>
                  <button onClick={() => file.section("svg")}>Section cut (SVG)…</button>
                  <button onClick={() => file.section("dxf")}>Section cut (DXF)…</button>
                </>
              )}
            </div>
          )}
          <button
            className="submenu-btn"
            aria-expanded={pluginsOpen}
            onClick={(e) => {
              e.stopPropagation();
              if (!pluginsOpen) loadPlugins().then(setPlugins, () => {});
              setPluginsOpen((o) => !o);
            }}
          >
            Plugins <span className="caret">{pluginsOpen ? "▾" : "▸"}</span>
          </button>
          {pluginsOpen && (
            <div className="submenu">
              {livePlugins.flatMap((pl) =>
                pl.commands.map((c) => (
                  <button key={`${pl.id}:${c.id}`} title={c.hint ?? pl.description} onClick={() => void runPluginCommand(pl, c.id)}>
                    {c.label}
                  </button>
                )),
              )}
              {plugins.problems.map((p) => (
                <button key={`x${p.id}`} disabled title={p.error}>
                  {p.id}: {p.error}
                </button>
              ))}
              <button onClick={() => (reloadPlugins(), open.plugins())}>Manage plugins…</button>
            </div>
          )}
          <hr />
          <button onClick={file.blueprint}>Make blueprint (PDF)…</button>
          <button onClick={() => open.sheets()}>Drawing sheets (PDF)…</button>
          <button onClick={() => open.timeline()}>Timeline (video)…</button>
          <hr />
          <button onClick={() => open.location()}>Add location…</button>
          <button onClick={() => open.text3d()}>3D Text…</button>
          <button onClick={file.report}>Generate report (CSV)…</button>
          <hr />
          <button onClick={() => open.keys()}>
            Keyboard shortcuts <kbd>{keyText("help.shortcuts")}</kbd>
          </button>
          <button onClick={() => open.keyEditor()}>Customize shortcuts…</button>
          {inTauri && <button onClick={checkForUpdatesNow}>Check for updates…</button>}
          <div className="menu-version" title="The version of SketchCraft you're running">
            SketchCraft {__APP_VERSION__}
          </div>
        </div>
      </>
    )}
  </div>
  );
}
