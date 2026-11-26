// A placed wall, stair, roof or column: its settings in Entity Info, changed in place (#263, from Blender 3D
// Architect's first look at 0.3.8: "we don't have any option to change the settings for the walls, for the stairs,
// roof"). The engine keeps each part's recipe (core/src/recipe.rs); changing a setting runs `rebuild`.
import { useEffect, useState } from "react";
import { exec, type Scene } from "./engine";
import { LengthInput, type Run } from "./panelkit";
import { parsePitch } from "./pitch";

type Info = { group: number; kind: "wall" | "stairs" | "roof" | "column" | null; settings?: Record<string, number | string | boolean>; edited?: boolean };

const NAMES: Record<string, string> = { wall: "Wall", stairs: "Stairs", roof: "Roof", column: "Column" };

export function PartSettings({ group, scene, run }: { group: number; scene: Scene; run: Run }) {
  const [info, setInfo] = useState<Info | null>(null);
  const [replace, setReplace] = useState(false);
  useEffect(() => {
    let live = true;
    // a read-only question: ask the engine directly (run() would reload the scene and loop, see EntityInfo)
    exec({ cmd: "part_info", group }).then((r) => live && setInfo(r.ok ? (r as unknown as Info) : null));
    return () => {
      live = false;
    };
  }, [group, scene]);
  useEffect(() => setReplace(false), [group]);
  if (!info?.kind || !info.settings) return null;
  const s = info.settings;
  const units = scene.units || "m";
  const set = (k: string) => (v: number | string | boolean) => {
    if (s[k] === v) return;
    void run({ cmd: "rebuild", group, set: { [k]: v }, force: replace });
  };
  const blocked = info.edited && !replace;
  const len = (k: string, label: string) => <LengthInput key={k} label={label} value={Number(s[k])} units={units} onChange={set(k)} />;
  return (
    <div className="part-settings">
      <div className="part-settings-title">{NAMES[info.kind]} settings</div>
      {info.edited && (
        // outside the fieldset below, which is disabled until this is ticked
        <label className="field check part-edited" title="Changing a setting remakes it from its settings, so the edits made by hand since are lost">
          <input type="checkbox" checked={replace} onChange={(e) => setReplace(e.target.checked)} />
          <span>Edited by hand since it was made. Replace those edits when I change a setting</span>
        </label>
      )}
      <fieldset disabled={blocked}>
      {info.kind === "wall" && [len("thickness", "Thickness"), len("height", "Height")]}
      {info.kind === "stairs" && [len("rise", "Total rise"), len("width", "Width"), len("tread", "Tread depth")]}
      {info.kind === "column" && (
        <>
          {len("size", "Size")}
          {len("height", "Height")}
          <label className="field check">
            <input type="checkbox" checked={Boolean(s.round)} onChange={(e) => set("round")(e.target.checked)} />
            <span>Round</span>
          </label>
        </>
      )}
      {info.kind === "roof" && (
        <>
          <label className="field">
            <span>Type</span>
            <select value={String(s.style)} onChange={(e) => set("style")(e.target.value)}>
              <option value="gable">Gable</option>
              <option value="hip">Hip</option>
              <option value="shed">Shed</option>
            </select>
          </label>
          <label className="field">
            <span>Pitch</span>
            <input
              key={`pitch-${s.pitch}`}
              defaultValue={`${s.pitch}°`}
              title="Degrees, or rise/run like 6/12"
              onBlur={(e) => {
                const deg = parsePitch(e.target.value);
                if (deg != null) set("pitch")(deg);
                else e.target.value = `${s.pitch}°`;
              }}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            />
          </label>
          {len("overhang", "Overhang")}
          {len("thickness", "Thickness")}
        </>
      )}
      </fieldset>
    </div>
  );
}
