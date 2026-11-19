// The Build and Sandbox panels: settings for the building-block and terrain tools.
import { useEffect, useState } from "react";
import { exec, type Scene } from "./engine";
import { Icons } from "./icons";
import { useLocale } from "./locale";
import { areaText } from "./viewport/rooms";
import { type Sel } from "./selection";
import type { BuildSettings, Tool } from "./Viewport";
import { LengthInput, type Run } from "./panelkit";

const BUILD_TOOLS: { id: Tool; name: string; how: string }[] = [
  { id: "room", name: "Room", how: "Press and drag on the ground to draw a room: four walls and a floor. Rooms drawn next to each other share a wall." },
  { id: "wall", name: "Wall", how: "Click points along the wall. Double-click or Enter to finish; click the first point to close a room." },
  { id: "wallpull", name: "Pull wall", how: "Hover a wall, then drag it (or click, move, click). It slides sideways and the walls joined to it stretch. Type a distance, then Enter. Esc cancels." },
  { id: "stairs", name: "Stairs", how: "Click where the bottom step starts, then the direction it climbs." },
  { id: "roof", name: "Roof", how: "Click a flat top face: the top of your walls or a ceiling you drew." },
  { id: "column", name: "Column", how: "Click to drop a column." },
];

/** Room mode's panel: what a click on a wall adds, walls down, and the rooms by name (rename here). */
function RoomOptions({ build, set, units, scene, run }: { build: BuildSettings; set: (k: keyof BuildSettings) => (v: unknown) => void; units: string; scene: Scene | null; run: Run }) {
  const { t } = useLocale();
  const [rooms, setRooms] = useState<{ id: number; name: string; area: number }[]>([]);
  useEffect(() => {
    let current = true;
    void exec({ cmd: "rooms" }).then((r) => {
      if (current && r.ok) setRooms((r as unknown as { rooms: { id: number; name: string; area: number }[] }).rooms ?? []);
    });
    return () => {
      current = false;
    };
  }, [scene]);
  return (
    <>
      <label className="field">
        <span>{t("room.opening")}</span>
        <select value={build.roomOpening} onChange={(e) => set("roomOpening")(e.target.value)}>
          <option value="door">{t("room.opening.door")}</option>
          <option value="window">{t("room.opening.window")}</option>
        </select>
      </label>
      <label className="field">
        <span>{t("room.walls_down")}</span>
        <input type="checkbox" checked={build.wallsDown} onChange={(e) => set("wallsDown")(e.target.checked)} />
      </label>
      <div className="room-list" aria-label={t("room.list")}>
        {rooms.length === 0 && <p>{t("room.none")}</p>}
        {rooms.map((r) => (
          <label className="field" key={`${r.id}:${r.name}`}>
            <input
              defaultValue={r.name}
              aria-label={r.name}
              onBlur={(e) => {
                const name = e.target.value.trim();
                if (name && name !== r.name) void run({ cmd: "rename", id: r.id, name });
                else e.target.value = r.name;
              }}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            />
            <span>{areaText(r.area, units)}</span>
          </label>
        ))}
      </div>
    </>
  );
}

export function BuildTools({ tool, setTool, build, setBuild, units, scene, run }: { tool: Tool; setTool: (t: Tool) => void; build: BuildSettings; setBuild: (b: BuildSettings) => void; units: string; scene: Scene | null; run: Run }) {
  const { t } = useLocale();
  // the labels the app translates; the rest of this panel is still English
  const name = (b: (typeof BUILD_TOOLS)[number]) => (b.id === "wallpull" ? t("build.wallpull") : b.id === "room" ? t("build.room") : b.name);
  const how = (b: (typeof BUILD_TOOLS)[number]) => (b.id === "wallpull" ? t("build.wallpull.how") : b.id === "room" ? t("build.room.how") : b.how);
  const active = BUILD_TOOLS.find((b) => b.id === tool);
  const set = (k: keyof BuildSettings) => (v: unknown) => setBuild({ ...build, [k]: v });
  return (
    <>
      <div className="tiles build">
        {BUILD_TOOLS.map((b) => (
          <button key={b.id} className={`tile ${tool === b.id ? "on" : ""}`} onClick={() => setTool(tool === b.id ? "select" : b.id)} title={how(b)}>
            <span className="tile-art build-art">{Icons[b.id as "wall"]()}</span>
            <span>{name(b)}</span>
          </button>
        ))}
      </div>
      {active && (
        <div className="build-options">
          <p>{how(active)}</p>
          {tool === "room" && (
            <>
              <LengthInput label="Thickness" value={build.wallThickness} units={units} onChange={set("wallThickness")} />
              <LengthInput label="Height" value={build.wallHeight} units={units} onChange={set("wallHeight")} />
              <RoomOptions build={build} set={set} units={units} scene={scene} run={run} />
            </>
          )}
          {tool === "wall" && (
            <>
              <LengthInput label="Thickness" value={build.wallThickness} units={units} onChange={set("wallThickness")} />
              <LengthInput label="Height" value={build.wallHeight} units={units} onChange={set("wallHeight")} />
            </>
          )}
          {tool === "stairs" && (
            <>
              <LengthInput label="Total rise" value={build.stairRise} units={units} onChange={set("stairRise")} />
              <LengthInput label="Width" value={build.stairWidth} units={units} onChange={set("stairWidth")} />
            </>
          )}
          {tool === "roof" && (
            <>
              <label className="field">
                <span>Type</span>
                <select value={build.roofKind} onChange={(e) => set("roofKind")(e.target.value)}>
                  <option value="gable">Gable</option>
                  <option value="hip">Hip</option>
                  <option value="shed">Shed</option>
                </select>
              </label>
              <label className="field">
                <span>Pitch</span>
                <input
                  defaultValue={`${build.roofPitch}°`}
                  title="Degrees, or rise/run like 6/12"
                  onBlur={(e) => {
                    const t = e.target.value.trim();
                    const ratio = /^(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)$/.exec(t);
                    const deg = ratio ? (Math.atan(Number(ratio[1]) / Number(ratio[2])) * 180) / Math.PI : parseFloat(t);
                    if (deg > 0 && deg < 80) set("roofPitch")(Math.round(deg * 10) / 10);
                    e.target.value = `${build.roofPitch}°`;
                  }}
                />
              </label>
              <LengthInput label="Overhang" value={build.roofOverhang} units={units} onChange={set("roofOverhang")} />
            </>
          )}
          {tool === "column" && (
            <>
              <LengthInput label="Size" value={build.columnSize} units={units} onChange={set("columnSize")} />
              <LengthInput label="Height" value={build.columnHeight} units={units} onChange={set("columnHeight")} />
              <label className="field check">
                <input type="checkbox" checked={build.columnRound} onChange={(e) => set("columnRound")(e.target.checked)} />
                <span>Round</span>
              </label>
            </>
          )}
        </div>
      )}
    </>
  );
}

const SANDBOX_TOOLS: { id: Tool; name: string; how: string }[] = [
  { id: "smoove", name: "Smoove", how: "Click a terrain to raise it there (Shift-click digs). Radius and height below." },
  { id: "stamp", name: "Stamp", how: "Select a footprint face (a rectangle above the terrain), then click the terrain: a flat pad with sloped edges." },
  { id: "drape", name: "Drape", how: "Select lines (a path, a property line), then click the terrain: they're laid onto it." },
  { id: "flipedge", name: "Flip edge", how: "Click a diagonal between two triangles to swap it." },
];

/** SketchUp's Sandbox: terrain from contours or a grid, then sculpt it. */
export function Sandbox({ tool, setTool, build, setBuild, units, run, sel, onToast }: { tool: Tool; setTool: (t: Tool) => void; build: BuildSettings; setBuild: (b: BuildSettings) => void; units: string; run: Run; sel: Sel; onToast: (t: string, bad?: boolean) => void }) {
  const active = SANDBOX_TOOLS.find((b) => b.id === tool);
  const set = (k: keyof BuildSettings) => (v: unknown) => setBuild({ ...build, [k]: v });
  const contours = async () => {
    const ids = [...sel.edges, ...sel.groups];
    if (!ids.length) return onToast("Select the contour lines (or a group of them) first.", true);
    const r = await run({ cmd: "terrain_from_contours", ids, spacing: build.contourSpacing });
    if (r.ok) onToast("Terrain made from the contours. Smoove, Stamp and Drape work on it.");
  };
  const grid = async () => {
    const s = build.gridSize;
    const r = await run({ cmd: "terrain_grid", at: [-s / 2, -s / 2, 0], width: s, depth: s, spacing: build.gridSpacing });
    if (r.ok) onToast("A flat terrain grid. Use Smoove to shape it.");
  };
  return (
    <>
      <div className="tiles build">
        <button className="tile" onClick={contours} title="Select contour lines, then click: a terrain through them">
          <span className="tile-art build-art">{Icons.terrain()}</span>
          <span>From contours</span>
        </button>
        <button className="tile" onClick={grid} title="A flat grid to sculpt">
          <span className="tile-art build-art">{Icons.terrain()}</span>
          <span>From scratch</span>
        </button>
        {SANDBOX_TOOLS.map((b) => (
          <button key={b.id} className={`tile ${tool === b.id ? "on" : ""}`} onClick={() => setTool(tool === b.id ? "select" : b.id)} title={b.how}>
            <span className="tile-art build-art">{Icons[b.id as "smoove"]()}</span>
            <span>{b.name}</span>
          </button>
        ))}
      </div>
      <div className="build-options">
        {active && <p>{active.how}</p>}
        {tool === "smoove" && (
          <>
            <LengthInput label="Radius" value={build.smooveRadius} units={units} onChange={set("smooveRadius")} />
            <LengthInput label="Height" value={build.smooveHeight} units={units} onChange={set("smooveHeight")} />
          </>
        )}
        {tool === "stamp" && <LengthInput label="Slope out" value={build.stampOffset} units={units} onChange={set("stampOffset")} />}
        {!active && (
          <>
            <LengthInput label="Grid size" value={build.gridSize} units={units} onChange={set("gridSize")} />
            <LengthInput label="Grid spacing" value={build.gridSpacing} units={units} onChange={set("gridSpacing")} />
            <LengthInput label="Contour spacing" value={build.contourSpacing} units={units} onChange={set("contourSpacing")} />
          </>
        )}
      </div>
    </>
  );
}

// ----- Components (SketchUp's Components panel) --------------------------------
