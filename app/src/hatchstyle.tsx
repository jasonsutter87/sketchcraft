// A tag's hatch (Outliner > Tags): the pattern its cut walls get in plans and sections, on screen and on sheets,
// with colour, line weight, scale and angle, and importing AutoCAD .pat files. The button shows a swatch of the
// hatch as it prints; opening it shows the controls with a bigger preview and a hint of what it does.

import { useEffect, useRef, useState } from "react";
import { exec, type Reply } from "./engine";
import { hatchLabel, importName, patOnly, swatchPath, type HatchPatternInfo, type HatchUseInfo } from "./hatchswatch";
import { PEN_WEIGHTS, penLabel } from "./linestyle";

/** A pattern drawn over a small box, or a tint when it's too fine to draw that small. */
export function HatchSwatch({ pattern, hatch, w, h, pxPerMm }: { pattern?: HatchPatternInfo; hatch?: HatchUseInfo | null; w: number; h: number; pxPerMm: number }) {
  const d = pattern ? swatchPath(pattern.families, w, h, pxPerMm, hatch?.scale ?? 1, hatch?.angle ?? 0) : "";
  const color = hatch?.color || "#1a1a1f";
  return (
    <svg className="hatch-swatch" width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden="true" data-pattern={pattern?.name ?? ""}>
      <rect width={w} height={h} fill="#fff" stroke="#9a9caa" strokeWidth={1} />
      {d === null ? <rect width={w} height={h} fill={color} opacity={0.3} data-tint="1" /> : d ? <path d={d} stroke={color} strokeWidth={Math.max(0.6, (hatch?.weight_mm ?? 0.13) * pxPerMm)} strokeLinecap="round" fill="none" /> : null}
    </svg>
  );
}

let cache: HatchPatternInfo[] | null = null;

async function loadPatterns(): Promise<HatchPatternInfo[]> {
  const r = await exec({ cmd: "hatch_patterns" });
  cache = r.ok ? ((r.patterns as HatchPatternInfo[]) ?? []) : cache ?? [];
  return cache;
}

/**
 * The hatch button for one tag, and its editor. `value` is the tag's hatch now (none: the tag's cut walls are the
 * plain dark fill). `run` sends a command and refreshes the scene.
 */
export function HatchControls({ tag, value, run }: { tag: string; value: HatchUseInfo | null | undefined; run: (c: Record<string, unknown>) => Promise<Reply> }) {
  const [open, setOpen] = useState(false);
  const [patterns, setPatterns] = useState<HatchPatternInfo[]>(cache ?? []);
  const [note, setNote] = useState("");
  const [units, setUnits] = useState<"mm" | "inch">("mm");
  const file = useRef<HTMLInputElement>(null);
  useEffect(() => {
    let live = true;
    void loadPatterns().then((p) => live && setPatterns(p));
    return () => {
      live = false;
    };
  }, [open, value?.pattern]);
  const pat = patterns.find((p) => p.name.toLowerCase() === value?.pattern?.toLowerCase());
  const set = async (h: Record<string, unknown>) => {
    const r = await run({ cmd: "tag_style", tag, hatch: h });
    setNote(r.ok ? "" : String(r.error ?? "That didn't work"));
  };
  const importPat = async (f: File) => {
    const text = await f.text();
    const peek = await exec({ cmd: "hatch_patterns", text, units });
    const found = (peek.patterns as HatchPatternInfo[] | undefined) ?? [];
    const warnings = (peek.warnings as string[] | undefined) ?? [];
    if (!found.length) {
      setNote(`No pattern could be read from ${f.name}${warnings.length ? `: ${warnings[0]}` : ""}`);
      return;
    }
    const builtins = patterns.filter((p) => p.builtin).map((p) => p.name);
    const kept: string[] = [];
    for (const p of found.slice(0, 50)) {
      const name = importName(p.name, builtins);
      // one pattern per command, taken out of the file by its own name
      const r = await run({ cmd: "hatch_pattern", name, text: patOnly(text, p.name) || text, units });
      if (r.ok) kept.push(name);
    }
    setPatterns(await loadPatterns());
    if (kept.length && !value) await set({ pattern: kept[0] });
    setNote(kept.length ? `Imported ${kept.length} pattern${kept.length > 1 ? "s" : ""} from ${f.name}: ${kept.slice(0, 6).join(", ")}${kept.length > 6 ? "…" : ""}${warnings.length ? ` (${warnings.length} line${warnings.length > 1 ? "s" : ""} skipped)` : ""}` : `Nothing from ${f.name} could be kept`);
  };
  const id = `hatch-${tag.replace(/[^a-z0-9]/gi, "_")}`;
  return (
    <span className="hatchstyle">
      <button
        className="rowbtn hatch-btn"
        aria-label={`Hatch of tag ${tag}`}
        aria-expanded={open}
        aria-controls={id}
        title={`Hatch: how ${tag}'s cut walls are filled in plans and sections. Now: ${hatchLabel(value)}`}
        onClick={() => setOpen(!open)}
      >
        {value ? <HatchSwatch pattern={pat} hatch={value} w={26} h={14} pxPerMm={2} /> : <span className="hatch-none">Hatch</span>}
      </button>
      {open && (
        <div className="hatch-editor" id={id} role="group" aria-label={`Hatch of ${tag}`}>
          <p className="hint">Fills {tag}'s cut walls in plans and sections, on screen and on sheets. Sizes are paper mm at scale 1.</p>
          <div className="hatch-row">
            <HatchSwatch pattern={pat} hatch={value ?? undefined} w={96} h={56} pxPerMm={3} />
            <select className="units-select" aria-label={`Hatch pattern of tag ${tag}`} title="Pattern" value={value?.pattern ?? ""} onChange={(e) => void set({ pattern: e.target.value })}>
              <option value="">None (solid fill)</option>
              {patterns.map((p) => (
                <option key={p.name} value={p.name} title={p.desc}>
                  {p.builtin ? p.name : `${p.name} (imported)`}
                </option>
              ))}
            </select>
          </div>
          {value && (
            <div className="hatch-row">
              <label className="swatch-in" title="Hatch colour">
                <span style={{ background: value.color }} />
                <input type="color" aria-label={`Hatch colour of tag ${tag}`} value={value.color} onChange={(e) => void set({ color: e.target.value })} />
              </label>
              <select className="units-select" aria-label={`Hatch line weight of tag ${tag}`} title="Hatch line weight: pen width on paper" value={String(value.weight_mm)} onChange={(e) => void set({ weight_mm: Number(e.target.value) })}>
                {!PEN_WEIGHTS.includes(value.weight_mm) && <option value={String(value.weight_mm)}>{penLabel(value.weight_mm)}</option>}
                {PEN_WEIGHTS.map((p) => (
                  <option key={p} value={String(p)}>
                    {penLabel(p)}
                  </option>
                ))}
              </select>
              <NumberBox label={`Hatch scale of tag ${tag}`} title="Scale: 2 makes the pattern twice as big" value={value.scale} min={0.01} max={1000} step={0.25} onCommit={(v) => void set({ scale: v })} />
              <NumberBox label={`Hatch angle of tag ${tag}`} title="Angle in degrees, counter-clockwise" value={value.angle} min={-360} max={360} step={15} suffix="°" onCommit={(v) => void set({ angle: v })} />
              <select className="units-select" aria-label={`Hatch scale mode of tag ${tag}`} title="Paper: the pattern is the same size on every drawing. Model: it is real size (a 1:100 drawing shows it half as big as 1:50)" value={value.scale_mode} onChange={(e) => void set({ scale_mode: e.target.value })}>
                <option value="paper">Paper size</option>
                <option value="model">Model size</option>
              </select>
            </div>
          )}
          <div className="hatch-row">
            <button className="btn small ghost" onClick={() => file.current?.click()} title="Load patterns from an AutoCAD .pat file; they are kept in this model">
              Import .pat…
            </button>
            <select className="units-select" aria-label="Units of the .pat file" title="What a .pat file's numbers mean when it doesn't say: AutoCAD's acadiso.pat is millimetres, acad.pat is inches" value={units} onChange={(e) => setUnits(e.target.value as "mm" | "inch")}>
              <option value="mm">File in mm</option>
              <option value="inch">File in inches</option>
            </select>
            <input
              ref={file}
              type="file"
              accept=".pat,text/plain"
              aria-label="Import a .pat file"
              hidden
              onChange={(e) => {
                const f = e.target.files?.[0];
                e.target.value = "";
                if (f) void importPat(f);
              }}
            />
          </div>
          {note && (
            <p className="hint hatch-note" role="status">
              {note}
            </p>
          )}
        </div>
      )}
    </span>
  );
}

function NumberBox({ label, title, value, min, max, step, suffix, onCommit }: { label: string; title: string; value: number; min: number; max: number; step: number; suffix?: string; onCommit: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = Number(text);
    if (Number.isFinite(v) && v >= min && v <= max && v !== value) onCommit(v);
    else setText(String(value));
  };
  return (
    <span className="hatch-num" title={title}>
      <input type="number" aria-label={label} value={text} min={min} max={max} step={step} onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
      {suffix}
    </span>
  );
}
