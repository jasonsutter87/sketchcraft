// Smaller dialogs: drawing sheets and the model's location.
import { useEffect, useState } from "react";
import { exec, type Scene } from "./engine";
import { Sheet, SheetHead } from "./sheet";

/** LayOut-lite: pick views (and scales) for a PDF set of drawing sheets. */
export function SheetsDialog({ scene, onMake, onClose, onLayout }: { scene: Scene; onMake: (spec: { sheet: string; project: string; blueprint: boolean; pages: { title: string; viewports: { view: string; scale?: string; label?: string; cut?: number }[] }[] }) => void; onClose: () => void; /** go to the Layout workspace (saved, editable pages) */ onLayout?: () => void }) {
  const standard = ["iso", "top", "front", "right", "back", "left"];
  // floor plans, one per floor the engine finds
  const [levels, setLevels] = useState<number[]>([]);
  useEffect(() => {
    exec({ cmd: "levels" }).then((r) => {
      if (!r.ok) return;
      const lv = r.levels as number[];
      setLevels(lv);
      setPicked((p) => ({ ...p, ...Object.fromEntries(lv.map((_, i) => [`plan:${i}`, { on: true, scale: "1:100" }])) }));
    });
  }, []);
  const [blueprint, setBlueprint] = useState(false);
  const all = [...levels.map((_, i) => `plan:${i}`), ...scene.views.map((v) => v.name), ...standard];
  const [sheet, setSheet] = useState("a3");
  const [project, setProject] = useState("Project");
  const [perPage, setPerPage] = useState(false);
  const [picked, setPicked] = useState<Record<string, { on: boolean; scale: string }>>(() =>
    Object.fromEntries(all.map((n, i) => [n, { on: scene.views.length ? i < scene.views.length && i < 4 : ["top", "front", "right", "iso"].includes(n), scale: ["top", "front", "right", "back", "left"].includes(n) ? "1:100" : "" }]))
  );
  const label = (n: string) => (n.startsWith("plan:") ? `${["Main floor", "Upper floor"][Number(n.slice(5))] ?? `Floor ${Number(n.slice(5)) + 1}`} plan` : standard.includes(n) ? { iso: "Iso view", top: "Plan (top)", front: "Front elevation", right: "Right elevation", back: "Back elevation", left: "Left elevation" }[n] ?? n : n);
  const make = () => {
    const chosen = all
      .filter((n) => picked[n]?.on)
      .map((n) => (n.startsWith("plan:") ? { view: "top", cut: levels[Number(n.slice(5))] + 1.22, scale: picked[n].scale.trim() || undefined, label: label(n) } : { view: n, scale: picked[n].scale.trim() || undefined, label: label(n) }));
    if (!chosen.length) return;
    const pages = [];
    const per = perPage ? 1 : 4;
    for (let i = 0; i < chosen.length; i += per) {
      const vs = chosen.slice(i, i + per);
      pages.push({ title: perPage ? vs[0].label : `Sheet ${pages.length + 1}`, viewports: vs });
    }
    onMake({ sheet, project: project.trim() || "Project", pages, blueprint });
  };
  return (
    <Sheet onClose={onClose} label="Drawing sheets" width={520}>
      <SheetHead title="Drawing sheets" note="Views on paper, to scale, with a title block." />
      <label className="field">
        <span>Project</span>
        <input value={project} onChange={(e) => setProject(e.target.value)} />
      </label>
      <label className="field">
        <span>Sheet</span>
        <select value={sheet} onChange={(e) => setSheet(e.target.value)}>
          <option value="a4">A4</option>
          <option value="a3">A3</option>
          <option value="a2">A2</option>
          <option value="a1">A1</option>
          <option value="a0">A0</option>
          <option value="letter">Letter</option>
          <option value="tabloid">Tabloid (11×17)</option>
          <option value="arch c">ARCH C (18×24 in)</option>
          <option value="arch d">ARCH D (24×36 in)</option>
          <option value="arch e">ARCH E (36×48 in)</option>
        </select>
      </label>
      <label className="field check">
        <input type="checkbox" checked={perPage} onChange={(e) => setPerPage(e.target.checked)} />
        <span>One view per sheet</span>
      </label>
      <label className="field check">
        <input type="checkbox" checked={blueprint} onChange={(e) => setBlueprint(e.target.checked)} />
        <span>Blueprint (white lines on blue)</span>
      </label>
      <div className="section-title" style={{ marginTop: 10 }}>
        Views <span className="muted" id="sheet-scale-hint">(scale like 1:100 or 1/4in=1ft for a true-scale drawing; blank to fit)</span>
      </div>
      {all.map((n) => (
        <div key={n} className="sheet-row">
          <input type="checkbox" id={`sheet-pick-${n}`} checked={!!picked[n]?.on} onChange={(e) => setPicked((p) => ({ ...p, [n]: { ...p[n], on: e.target.checked } }))} />
          <label htmlFor={`sheet-pick-${n}`}>{label(n)}</label>
          <input className="num" aria-label={`Scale of ${label(n)}`} aria-describedby="sheet-scale-hint" style={{ width: 70 }} placeholder="fit" value={picked[n]?.scale ?? ""} onChange={(e) => setPicked((p) => ({ ...p, [n]: { ...p[n], scale: e.target.value } }))} />
        </div>
      ))}
      <div className="row gap" style={{ marginTop: 12 }}>
        <button className="btn small" onClick={make}>
          Make PDF…
        </button>
        <button className="btn small ghost" onClick={onClose}>
          Cancel
        </button>
        {onLayout && (
          <button className="btn small ghost" onClick={onLayout} title="Saved pages you can come back to, reorder and change: the Layout workspace">
            Open in Layout
          </button>
        )}
      </div>
    </Sheet>
  );
}


/** SketchUp's Add Location: real terrain (and map) around a place. */
export function LocationDialog({ onAdd, onClose, current }: { onAdd: (p: { lat: number; lon: number; size: number; imagery: boolean }) => void; onClose: () => void; current?: { lat: number; lon: number } | null }) {
  const [lat, setLat] = useState(current ? String(current.lat) : "");
  const [lon, setLon] = useState(current ? String(current.lon) : "");
  const [size, setSize] = useState("300");
  const [imagery, setImagery] = useState(true);
  const paste = (v: string) => {
    // "37.8199, -122.4783" pasted into either box
    const m = /(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)/.exec(v);
    if (m) {
      setLat(m[1]);
      setLon(m[2]);
      return true;
    }
    return false;
  };
  const ok = () => {
    const [a, o, s] = [parseFloat(lat), parseFloat(lon), parseFloat(size)];
    if (Number.isFinite(a) && Number.isFinite(o) && s > 0) onAdd({ lat: a, lon: o, size: s, imagery });
  };
  return (
    <Sheet onClose={onClose} label="Add location" width={420}>
      <SheetHead title="Add location" note="Ground and map around a place." />
      <label className="field">
        <span>Latitude</span>
        <input value={lat} placeholder="37.8199" onChange={(e) => paste(e.target.value) || setLat(e.target.value)} />
      </label>
      <label className="field">
        <span>Longitude</span>
        <input value={lon} placeholder="-122.4783" onChange={(e) => paste(e.target.value) || setLon(e.target.value)} />
      </label>
      <label className="field">
        <span>Area (m across)</span>
        <input value={size} onChange={(e) => setSize(e.target.value)} />
      </label>
      <label className="field check">
        <input type="checkbox" checked={imagery} onChange={(e) => setImagery(e.target.checked)} />
        <span>Map imagery (OpenStreetMap)</span>
      </label>
      <p className="muted small">Paste "lat, lon" from a map. The model's origin sits on the ground there, north up (+Y). Elevation: AWS Terrain Tiles; map © OpenStreetMap contributors.</p>
      <div className="row gap" style={{ marginTop: 12 }}>
        <button className="btn small" onClick={ok}>
          Add
        </button>
        <button className="btn small ghost" onClick={onClose}>
          Cancel
        </button>
      </div>
    </Sheet>
  );
}
