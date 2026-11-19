// File actions: New, Open, Save, the imports and the exports. Each runs engine commands and
// says what happened.
import type { RefObject } from "react";
import type { Reply, Scene } from "./engine";
import { exec } from "./engine";
import { isModelPath, pickOpen, pickSave } from "./files";
import type { Sel } from "./selection";
import { EMPTY } from "./selection";
import type { ViewportApi } from "./Viewport";

export type FigureChoice = "person" | "beaver" | "none";

export function readFigure(): FigureChoice {
  try {
    const v = localStorage.getItem("sketchcraft.figure");
    if (v === "person" || v === "beaver" || v === "none") return v;
  } catch {}
  return "person";
}

/** The figure for a new model today: on April 1 the beaver clocks in. */
function figureToday(choice: FigureChoice): FigureChoice {
  const d = new Date();
  return choice !== "none" && d.getMonth() === 3 && d.getDate() === 1 ? "beaver" : choice;
}

/** Pictures the engine left out of a render, an import or a projection (too big, unreadable), said plainly. */
export function skippedText(list: string[]): string {
  const shown = list.slice(0, 3).join("; ");
  return `${list.length} picture${list.length === 1 ? " was" : "s were"} left out: ${shown}${list.length > 3 ? `; and ${list.length - 3} more` : ""}.`;
}

/** What an import left out, to follow its "Imported ..." line (empty when nothing was). */
function importedSkips(r: Reply): string {
  return Array.isArray(r.skipped_pictures) && r.skipped_pictures.length ? ` ${skippedText(r.skipped_pictures as string[])}` : "";
}

/** The model's name without its folder or extension (what a saved file is called after). */
const modelBase = (file: string | null) => (file?.split("/").pop() ?? "model").replace(/\.sketchcraft$/, "");

export function useFileActions({ run, say, refresh, api, scene, figure, currentFile, setCurrentFile, setSel }: {
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  say: (text: string, bad?: boolean) => void;
  refresh: () => Promise<void>;
  api: RefObject<ViewportApi | null>;
  scene: Scene | null;
  figure: FigureChoice;
  currentFile: string | null;
  setCurrentFile: (p: string | null) => void;
  setSel: (s: Sel) => void;
}) {
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
    `Imported ${name}: ${r.layers} layer${r.layers === 1 ? "" : "s"} (as tags), ${r.faces} faces, ${r.labels ? `${r.labels} labels, ` : ""}${r.dimensions ? `${r.dimensions} dimensions, ` : ""}read as ${r.units}${r.skipped ? `; ${r.skipped} hatch/other items skipped` : ""}. A floor plan? Right-click it › Raise walls.`;

  /** after a CAD import: land on where most of the drawing is (not a scaled-up sheet border) */
  const landOn = (r: Reply) => {
    const f = r.focus as [number, number, number][] | undefined;
    if (f && f.length === 2) {
      const pad = Math.max(f[1][0] - f[0][0], f[1][1] - f[0][1]) * 0.05;
      api.current?.fitBox([f[0][0] - pad, f[0][1] - pad, f[0][2]], [f[1][0] + pad, f[1][1] + pad, f[1][2]]);
    } else api.current?.fitSoon();
  };

  /** say what an import made, then show it */
  const reportImport = async (r: Reply, name: string) => {
    say(`${r.layers != null ? cadSummary(name, r) : `Imported ${name}: ${r.faces} faces`}${importedSkips(r)}`, importedSkips(r) !== "");
    if (r.focus) {
      await refresh();
      landOn(r);
    } else {
      api.current?.fitSoon();
      await refresh();
    }
  };

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
    if (r.ok) await reportImport(r, file.name);
  };

  const importPath = async (path: string) => {
    const lower = path.toLowerCase();
    if (/\.(ply|las|laz|xyz|pts|txt)$/.test(lower)) {
      const r = await run({ cmd: "import_cloud", path });
      if (r.ok) {
        const shift = r.shift as number[] | undefined;
        const moved = shift && shift.some((v) => Math.abs(v) > 0) ? `; moved ${shift.map((v) => v.toFixed(1)).join(", ")} m to the origin` : "";
        say(`Imported ${path.split("/").pop()}: ${(r.points as number).toLocaleString()} points${r.colored ? " in color" : ""}${moved}. Drawing tools snap to them.`);
        await refresh();
        api.current?.fit();
      }
      return r.ok;
    }
    if (lower.endsWith(".ifc")) {
      say("Reading the IFC (IfcOpenShell)…");
      const r = await run({ cmd: "import_ifc", path });
      if (r.ok) {
        say(`Imported ${path.split("/").pop()} (${r.schema}): ${r.elements} elements on ${r.storeys} storey${r.storeys === 1 ? "" : "s"}, typed for the Classifier, tagged by storey`);
        api.current?.fitSoon();
        await refresh();
      }
      return r.ok;
    }
    const r = lower.endsWith(".skp")
      ? await run({ cmd: "import_skp", path })
      : lower.endsWith(".fbx")
        ? await run({ cmd: "import_fbx", path })
        : lower.endsWith(".dxf")
          ? await run({ cmd: "import_dxf", path })
          : lower.endsWith(".dwg")
            ? await run({ cmd: "import_dwg", path })
            : await run({ cmd: "import_obj", path, up: "y" });
    if (r.ok) await reportImport(r, path.split("/").pop()!);
    return r.ok;
  };

  const file = {
    new: async () => {
      await run({ cmd: "new", figure: figureToday(figure) });
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
      // a remembered path is checked again here: it only ever names a .sketchcraft file
      let path = isModelPath(currentFile) ? currentFile : null;
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
    export: async (kind: "obj" | "glb" | "fbx" | "stl" | "dae" | "dxf" | "dwg" | "pdf" | "ifc") => {
      const base = modelBase(currentFile);
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
      const base = modelBase(currentFile);
      const path = await pickSave(`${base} section.${kind}`, kind, kind.toUpperCase());
      if (!path) return;
      const r = await run({ cmd: "section", ...plane, path });
      if (r.ok) say(`Exported the section cut to ${path.split("/").pop()}`);
    },
    blueprint: async () => {
      const base = modelBase(currentFile);
      const path = await pickSave(`${base} blueprint.pdf`, "pdf", "PDF");
      if (!path) return;
      say("Drawing the blueprint: plans with dimensions and tags, elevations with levels…");
      const r = await run({ cmd: "blueprint", path, project: base });
      if (r.ok) say(`Blueprint ready: ${(r.levels as number[]).length} floor plan${(r.levels as number[]).length === 1 ? "" : "s"} + 4 elevations${(r.pages as number) > 2 ? " + door and window schedule" : ""} in ${path.split("/").pop()}`);
    },
    report: async () => {
      const base = modelBase(currentFile);
      const path = await pickSave(`${base} report.csv`, "csv", "CSV");
      if (!path) return;
      const r = await run({ cmd: "report", path });
      if (r.ok) say(`Report: ${(r.rows as unknown[]).length} groups and components in ${path.split("/").pop()}`);
    },
    animation: async () => {
      const base = modelBase(currentFile);
      const path = await pickSave(`${base}.mp4`, "mp4", "MP4 video");
      if (!path) return;
      say(scene?.views.length && scene.views.length > 1 ? "Rendering the tour through your saved views…" : "No saved views to visit: rendering a turntable…");
      const r = await run({ cmd: "animate", path, width: 1280, height: 720, fps: 24, transition: 2, delay: 1 });
      if (r.ok) say(`Exported ${r.frames} frames to ${path.split("/").pop()}`);
    },
    render: async () => {
      const base = modelBase(currentFile);
      const path = await pickSave(`${base}.png`, "png", "PNG image");
      if (!path) return;
      const r = await run({ cmd: "render", out: path, width: 1920, height: 1080, camera: api.current!.camera() });
      if (r.ok) say(`Rendered ${path.split("/").pop()}`);
    },
  };
  return { file, importFile, importPath, runTemplate };
}
