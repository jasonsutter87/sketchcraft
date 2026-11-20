// Entity Info: what is selected, and the options, attributes and classification to edit.
import { useEffect, useState } from "react";
import { exec, type Scene } from "./engine";
import { selIds, type Sel } from "./selection";
import { formatLength } from "./units";
import { LineStyleControls } from "./linestyle";
import { formatArea, formatVolume, type Run } from "./panelkit";

export function EntityInfo({ scene, sel, run }: { scene: Scene; sel: Sel; run: Run }) {
  const [tag, setTag] = useState("");
  const faces = scene.faces.filter((f) => sel.faces.includes(f.id));
  const edges = scene.edgeList.filter((e) => sel.edges.includes(e.id));
  const groups = scene.groups.filter((g) => sel.groups.includes(g.id));
  const insts = scene.instances.filter((i) => sel.instances.includes(i.id));
  const len = edges.reduce((a, e) => a + Math.hypot(e.b[0] - e.a[0], e.b[1] - e.a[1], e.b[2] - e.a[2]), 0);
  const title =
    groups.length + insts.length + faces.length + edges.length === 1
      ? groups[0]?.name ?? insts[0]?.name ?? (faces[0] ? `Face ${faces[0].id}` : `Edge ${edges[0].id}`)
      : `${selIds(sel).length} selected`;
  const taggable = [...sel.groups, ...sel.instances];
  // one group or component: is it a solid, and its volume (SketchUp's "Solid Group")
  const [solid, setSolid] = useState<{ id: number; volume: number | null } | null>(null);
  const one = taggable.length === 1 && selIds(sel).length === 1 ? taggable[0] : null;
  useEffect(() => {
    if (one == null) return setSolid(null);
    let live = true;
    // a read-only question: ask the engine directly. Going through run() reloads the scene, which
    // re-runs this effect, which asks again: a loop that rebuilt the scene many times a second.
    exec({ cmd: "volume", id: one }).then((r) => live && setSolid({ id: one, volume: r.ok && r.solid ? (r.volume as number) : null }));
    return () => {
      live = false;
    };
    // recheck whenever the model changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [one, scene]);
  return (
    <div className="section">
      <div className="section-title">{title}</div>
      <div className="kv">
        {faces.length > 0 && (
          <>
            <span>Area</span>
            <b>{formatArea(faces.reduce((a, f) => a + f.area, 0), scene.units)}</b>
          </>
        )}
        {edges.length > 0 && (
          <>
            <span>Length</span>
            <b>{formatLength(len, scene.units)}</b>
          </>
        )}
        {solid && solid.id === one && (
          <>
            <span>{solid.volume != null ? "Solid" : "Not solid"}</span>
            <b>{solid.volume != null ? formatVolume(solid.volume, scene.units) : "has open edges"}</b>
          </>
        )}
        {insts.length === 1 && (
          <>
            <span>Component</span>
            <b>{scene.components.find((c) => c.id === insts[0].component)?.name}</b>
          </>
        )}
        {(groups[0]?.tag ?? insts[0]?.tag) && (
          <>
            <span>Tag</span>
            <b>{groups[0]?.tag ?? insts[0]?.tag}</b>
          </>
        )}
      </div>
      {edges.length > 0 && (
        <div className="field">
          <span>Line (on sheets)</span>
          <LineStyleControls
            label="selected edges"
            none="From tag"
            mixed={edges.some((e) => e.draft?.weight_mm !== edges[0].draft?.weight_mm || e.draft?.line !== edges[0].draft?.line)}
            value={edges[0].draft ?? {}}
            onChange={(c) => run({ cmd: "edge_style", ids: edges.map((e) => e.id), ...c })}
          />
        </div>
      )}
      {taggable.length === 1 && selIds(sel).length === 1 && (
        <label className="field">
          <span>IFC type</span>
          <select value={scene.classes?.[String(taggable[0])] ?? ""} onChange={(e) => run({ cmd: "classify", ids: taggable, class: e.target.value })}>
            <option value="">{`(guess: ${guessIfc(groups[0]?.name ?? insts[0]?.name ?? "")})`}</option>
            {IFC_TYPES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
      )}
      {insts.length === 1 &&
        (() => {
          const comp = scene.components.find((c) => c.id === insts[0].component);
          if (!comp) return null;
          return (
            <>
            <label className="field check" title="It turns around its blue axis to face you, like a flat person or tree">
              <input type="checkbox" checked={!!insts[0].faceMe} onChange={(e) => run({ cmd: "component_face_me", id: comp.id, on: e.target.checked })} />
              <span>Always faces the camera</span>
            </label>
            <label className="field check" title="Placed on a wall (or any face), it cuts a hole there, through the wall when it's thick">
              <input type="checkbox" checked={!!comp.cuts} onChange={(e) => run({ cmd: "component_cuts", id: comp.id, cuts: e.target.checked })} />
              <span>Cuts an opening where placed</span>
            </label>
            </>
          );
        })()}
      {insts.length === 1 && <ComponentOptions key={`o${insts[0].id}:${JSON.stringify(insts[0].attrs ?? {})}`} id={insts[0].id} run={run} />}
      {insts.length === 1 && <AttrEditor key={`a${insts[0].id}:${JSON.stringify(insts[0].attrs ?? {})}`} id={insts[0].id} attrs={insts[0].attrs ?? {}} run={run} />}
      {taggable.length > 0 && (
        <div className="row gap">
          <input className="num wide-input" placeholder="Tag (e.g. Trees)" value={tag} onChange={(e) => setTag(e.target.value)} list="tag-list" />
          <datalist id="tag-list">
            {Object.keys(scene.tags).map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
          <button className="btn small" onClick={() => run({ cmd: "tag", ids: taggable, tag: tag.trim() || null })}>
            Set
          </button>
        </div>
      )}
    </div>
  );
}

interface Opt {
  name: string;
  label: string;
  access: "textbox" | "list" | "view";
  choices: [string, string][];
  value: string;
  number: number | null;
}

/** SketchUp's Component Options: the attributes a component offers its user
 * (a text box, a list of choices, or a value to read). */
function ComponentOptions({ id, run }: { id: number; run: Run }) {
  const [opts, setOpts] = useState<Opt[]>([]);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    exec({ cmd: "options", id }).then((r) => r.ok && setOpts((r.options as Opt[]) ?? []));
  }, [id]);
  if (!opts.length) return null;
  const set = async (name: string, value: string) => {
    const r = await run({ cmd: "set_attrs", id, attrs: { [name]: value } });
    setErr(r.ok ? null : (r.error ?? "That didn't work"));
  };
  const shown = (o: Opt) => (o.number != null ? String(Math.round(o.number * 1000) / 1000) : o.value);
  return (
    <div className="attrs">
      <div className="section-title sub">Component options</div>
      {opts.map((o) => (
        <label key={o.name} className="field">
          <span>{o.label}</span>
          {o.access === "list" && o.choices.length ? (
            <select value={o.choices.find(([, v]) => v === o.value)?.[1] ?? ""} onChange={(e) => set(o.name, e.target.value)}>
              {!o.choices.some(([, v]) => v === o.value) && <option value="">{shown(o)}</option>}
              {o.choices.map(([l, v]) => (
                <option key={l} value={v}>
                  {l}
                </option>
              ))}
            </select>
          ) : o.access === "view" ? (
            <b>{shown(o)}</b>
          ) : (
            <input
              value={draft[o.name] ?? o.value}
              onChange={(e) => setDraft((d) => ({ ...d, [o.name]: e.target.value }))}
              onBlur={() => draft[o.name] != null && draft[o.name] !== o.value && set(o.name, draft[o.name])}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            />
          )}
        </label>
      ))}
      {err && <p className="attr-err">{err}</p>}
    </div>
  );
}

/** Dynamic Component attributes of one placement: values or =formulas. */
function AttrEditor({ id, attrs, run }: { id: number; attrs: Record<string, string>; run: Run }) {
  const [rows, setRows] = useState<[string, string][]>(() => [...Object.entries(attrs), ["", ""]]);
  const [err, setErr] = useState<string | null>(null);
  const [vals, setVals] = useState<Record<string, number>>({});
  const apply = async () => {
    const next: Record<string, string> = {};
    for (const [k, v] of rows) if (k.trim()) next[k.trim()] = v;
    // names that were removed
    for (const k of Object.keys(attrs)) if (!(k in next)) next[k] = "";
    const r = await run({ cmd: "set_attrs", id, attrs: next });
    if (r.ok) {
      setErr(null);
      setVals((r.values as Record<string, number>) ?? {});
      setRows([...Object.entries(r.attrs as Record<string, string>), ["", ""]]);
    } else setErr(r.error ?? "That didn't work");
  };
  return (
    <div className="attrs">
      <div className="section-title sub">Attributes</div>
      {rows.map(([k, v], i) => (
        <div key={i} className="attr-row">
          <input placeholder="name" value={k} onChange={(e) => setRows((r) => r.map((x, j) => (j === i ? [e.target.value, x[1]] : x)))} />
          <input placeholder="value or =formula" value={v} onChange={(e) => setRows((r) => r.map((x, j) => (j === i ? [x[0], e.target.value] : x)))} onKeyDown={(e) => e.key === "Enter" && apply()} title={k && vals[k.toLowerCase()] != null ? `= ${vals[k.toLowerCase()]}` : undefined} />
        </div>
      ))}
      {err && <p className="attr-err">{err}</p>}
      <div className="row gap">
        <button className="btn small" onClick={apply}>
          Apply
        </button>
        <button className="btn small ghost" onClick={() => setRows((r) => [...r, ["", ""]])}>
          + Row
        </button>
      </div>
      <p className="muted small">lenx / leny / lenz (m), x / y / z (m), rotx / roty / rotz (°), copies, spacing, material, hidden. Formulas: =lenx*2, =if(a&gt;1, 2, 3), =parent!lenx. Offer one in Component options: _name_access = textbox, list or view; _name_label; _name_options = Low=0.9&amp;Tall=1.8</p>
    </div>
  );
}


const IFC_TYPES = ["IfcWall", "IfcSlab", "IfcRoof", "IfcDoor", "IfcWindow", "IfcColumn", "IfcBeam", "IfcStair", "IfcRailing", "IfcCovering", "IfcFurniture", "IfcPlate", "IfcMember", "IfcBuildingElementProxy"];

/** The same name-based guess the IFC export makes. */
function guessIfc(name: string): string {
  const n = name.toLowerCase();
  const has = (w: string) => n.includes(w);
  if (has("wall")) return "IfcWall";
  if (has("roof")) return "IfcRoof";
  if (has("stair") || has("step")) return "IfcStair";
  if (has("column") || has("post") || has("pillar")) return "IfcColumn";
  if (has("door") || has("garage")) return "IfcDoor";
  if (has("window")) return "IfcWindow";
  if (has("slab") || has("floor") || has("foundation") || has("deck")) return "IfcSlab";
  if (has("beam")) return "IfcBeam";
  if (has("rail") || has("fence")) return "IfcRailing";
  return "IfcBuildingElementProxy";
}
