// The strip over the model with the quick actions for the workspace you're
// in: floors and drawings for Arch, solid and edge tools for Design, scenes
// and rendering for Previz. It says what to select when nothing fits yet.

import { useEffect, useState } from "react";
import { ask } from "./ask";
import { exec, type Reply, type Scene } from "./engine";
import { contextItems, ENVIRONMENTS } from "./panels";
import type { Sel } from "./selection";
import type { Cam, ViewportApi } from "./Viewport";
import type { WorkspaceId } from "./workspace";

interface Props {
  ws: WorkspaceId;
  scene: Scene;
  sel: Sel;
  setSel: (s: Sel) => void;
  run: (c: Record<string, unknown>) => Promise<Reply>;
  api: React.MutableRefObject<ViewportApi | null>;
  section: boolean;
  setSection: (on: boolean) => void;
  actions: { blueprint: () => void; sheets: () => void; location: () => void; render: () => void; animation: () => void };
}

export function WorkspaceStrip(p: Props) {
  if (p.ws === "arch") return <ArchStrip {...p} />;
  if (p.ws === "design") return <DesignStrip {...p} />;
  if (p.ws === "previz") return <PrevizStrip {...p} />;
  return null;
}

/** Pick the setting around the model, right from the strip. */
function SettingPicker({ scene, run }: { scene: Scene; run: Props["run"] }) {
  return (
    <label className="strip-select" title="A ready-made setting around your model">
      <span className="lbl">Setting</span>
      <select value={scene.environment ?? ""} onChange={(e) => run({ cmd: "environment", kind: e.target.value || "none" })}>
        <option value="">None</option>
        {ENVIRONMENTS.map((x) => (
          <option key={x.id} value={x.id}>
            {x.name}
          </option>
        ))}
      </select>
    </label>
  );
}

const FLOOR_NAMES = ["Main", "Upper", "Third", "Fourth", "Fifth"];

function ArchStrip({ scene, api, section, setSection, actions, run }: Props) {
  const [levels, setLevels] = useState<number[]>([]);
  const [at, setAt] = useState<number | null>(null);
  // floors come from the model; recount when it changes
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => exec({ cmd: "levels" }).then((r) => live && r.ok && setLevels(r.levels as number[])), 300);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [scene]);
  useEffect(() => {
    if (!section) setAt(null);
  }, [section]);
  return (
    <div className="ws-strip" role="toolbar" aria-label="Arch">
      <span className="lbl">Floors</span>
      {levels.length === 0 && <span className="hint">Draw walls and a floor to see plans by floor</span>}
      {levels.map((z, i) => (
        <button
          key={i}
          className={at === i ? "on" : ""}
          title={`Floor plan: cut 4 ft above this floor and look down`}
          onClick={() => {
            api.current?.planCut(z + 1.22);
            api.current?.view("top");
            setSection(true);
            setAt(i);
          }}
        >
          {FLOOR_NAMES[i] ?? `Floor ${i + 1}`}
        </button>
      ))}
      <button
        className={section ? "" : "on"}
        title="The whole building in 3D"
        onClick={() => {
          api.current?.clearSection();
          setSection(false);
          api.current?.view("iso");
        }}
      >
        3D
      </button>
      <span className="sep" />
      <span className="lbl">Drawings</span>
      <button onClick={actions.blueprint} title="Every floor plan and four elevations, on blueprint sheets">
        Blueprint…
      </button>
      <button onClick={actions.sheets} title="Lay out your own sheets: plans, elevations, views">
        Sheets…
      </button>
      <button onClick={actions.location} title="Real terrain and map around the site; drives the real sun">
        Site…
      </button>
      <span className="sep" />
      <SettingPicker scene={scene} run={run} />
    </div>
  );
}

const DESIGN_ACTIONS = /^(Solid: |Bevel|Round|Divide|Slice|Intersect Faces|Flip Along)/;

function DesignStrip({ scene, sel, setSel, run, api }: Props) {
  const items = contextItems(scene, sel, run, setSel, () => api.current?.fit(), api.current?.sectionPlane() ?? undefined).filter((i) => i.label && DESIGN_ACTIONS.test(i.label));
  const solids = sel.groups.length + sel.instances.length;
  const hint = sel.edges.length ? "" : solids === 1 ? "Select a second solid to combine them, or edges on it to bevel or round" : "Select two solids to combine them, or edges to bevel or round";
  return (
    <div className="ws-strip" role="toolbar" aria-label="Design">
      <span className="lbl">Shape</span>
      {items.length === 0 && <span className="hint">{hint}</span>}
      {items.map((i) => (
        <button key={i.label} onClick={() => i.run?.()} title={i.label}>
          {i.label!.replace(/^Solid: /, "").replace(/ \(first cuts second(, kept)?\)/, "")}
        </button>
      ))}
    </div>
  );
}

function PrevizStrip({ scene, run, api, actions }: Props) {
  const [on, setOn] = useState<string | null>(null);
  return (
    <div className="ws-strip" role="toolbar" aria-label="Previz">
      <span className="lbl">Scenes</span>
      {scene.views.length === 0 && <span className="hint">Frame a shot, then add it as a scene</span>}
      {scene.views.map((v) => (
        <button
          key={v.name}
          className={on === v.name ? "on" : ""}
          onClick={() => {
            api.current?.goTo(v as Cam);
            setOn(v.name);
          }}
        >
          {v.name}
        </button>
      ))}
      <button
        title="Save the current camera as a scene"
        onClick={async () => {
          const name = await ask("Name this scene", `Scene ${scene.views.length + 1}`);
          if (name && api.current) {
            const r = await run({ cmd: "save_view", name, ...api.current.camera() });
            if (r.ok) setOn(name);
          }
        }}
      >
        + Scene
      </button>
      <span className="sep" />
      <SettingPicker scene={scene} run={run} />
      <span className="sep" />
      <button onClick={actions.animation} title="Turntables, walkthroughs, scene glides and sun studies, exported as video">
        Timeline
      </button>
      <button className="on" onClick={actions.render} title="Photoreal: sun, sky, soft shadows, reflections">
        Render…
      </button>
    </div>
  );
}
