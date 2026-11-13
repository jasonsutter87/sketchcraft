// Model from photos: mark the same corner in two or more matched photos and
// SketchCraft works out where it is in 3D (and how far each photo was taken
// from), then builds the edges you connect. The engine does the math
// (photo_solve); this is the marking table.

import { useMemo, useState } from "react";
import { type Reply, type SavedView, type Scene } from "./engine";
import { parseLength } from "./units";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;
type UV = [number, number];
interface Point {
  name: string;
  marks: Record<string, UV>;
}

// ---- the same camera as the engine (photomodel.rs): z up, fov over the photo's height
type V3 = [number, number, number];
const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mul = (a: V3, k: number): V3 => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: V3, b: V3): V3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: V3): V3 => mul(a, 1 / (Math.hypot(...a) || 1));
function frame(v: SavedView) {
  const fwd = norm(sub(v.target, v.eye));
  const up0: V3 = Math.hypot(...cross(fwd, [0, 0, 1])) < 1e-6 ? [0, 1, 0] : [0, 0, 1];
  const right = norm(cross(fwd, up0));
  const up = cross(right, fwd);
  return { eye: v.eye as V3, fwd, right, up, t: Math.tan((v.fov * Math.PI) / 360) };
}
function ray(v: SavedView, [u, w]: UV): V3 {
  const f = frame(v);
  return norm(add(f.fwd, add(mul(f.right, u * f.t), mul(f.up, w * f.t))));
}
function project(v: SavedView, p: V3): UV | null {
  const f = frame(v);
  const d = sub(p, f.eye);
  const z = dot(d, f.fwd);
  if (z <= 1e-6) return null;
  return [dot(d, f.right) / z / f.t, dot(d, f.up) / z / f.t];
}

export function PhotoModeler({ scene, run, onClose, onToast }: { scene: Scene; run: Run; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const photos = scene.views.filter((v) => v.photo);
  const [points, setPoints] = useState<Point[]>(() => (scene.photoPoints ?? []).map((p) => ({ name: p.name, marks: { ...p.marks } })));
  const [edges, setEdges] = useState<[number, number][]>(() => (scene.photoEdges ?? []).map((e) => [e[0], e[1]]));
  const [active, setActive] = useState(() => Math.max(0, (scene.photoPoints?.length ?? 0) - 1));
  const [chain, setChain] = useState(true);
  const [aspect, setAspect] = useState<Record<string, number>>({});
  const [result, setResult] = useState<{ misses: [string, number][][]; placed: number } | null>(null);
  const [known, setKnown] = useState<{ edge: number; text: string }>({ edge: 0, text: "" });
  const units = scene.units || "m";
  const cur = points[active];

  const click = (view: string, e: React.MouseEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const uv: UV = [(e.clientX - r.left - r.width / 2) / (r.height / 2), (r.height / 2 - (e.clientY - r.top)) / (r.height / 2)];
    setResult(null);
    const next = points.map((p) => ({ ...p, marks: { ...p.marks } }));
    const here = cur?.marks[view];
    const marked = cur ? Object.keys(cur.marks).length : 0;
    if (!cur || (here && marked >= 2)) {
      // start the next corner (connected to the last one, when tracing)
      next.push({ name: `Corner ${next.length + 1}`, marks: { [view]: uv } });
      if (chain && cur && marked >= 2) setEdges([...edges, [active, next.length - 1]]);
      setPoints(next);
      setActive(next.length - 1);
      return;
    }
    // mark (or move the mark of) the corner in progress
    next[active].marks[view] = uv;
    setPoints(next);
  };

  // where the active corner must be in each photo: the line its other marks' rays make there
  const epipolar = useMemo(() => {
    const out: Record<string, [UV, UV][]> = {};
    if (!cur) return out;
    for (const v of photos) {
      if (cur.marks[v.name]) continue;
      for (const [other, uv] of Object.entries(cur.marks)) {
        const ov = photos.find((x) => x.name === other);
        if (!ov) continue;
        const r = ray(ov, uv);
        const pts: UV[] = [];
        for (let t = 0.3; t < 3000; t *= 1.25) {
          const q = project(v, add(ov.eye as V3, mul(r, t)));
          if (q && Math.abs(q[0]) < 50 && Math.abs(q[1]) < 50) pts.push(q);
        }
        if (pts.length >= 2) (out[v.name] ??= []).push([pts[0], pts[pts.length - 1]]);
      }
    }
    return out;
  }, [cur, photos]);

  const ready = points.filter((p) => Object.keys(p.marks).length >= 2).length;
  const solve = async (build: boolean) => {
    const len = known.text.trim() ? parseLength(known.text, units) : NaN;
    const scale = Number.isFinite(len) && len > 0 && edges[known.edge] ? { a: edges[known.edge][0], b: edges[known.edge][1], length: len } : undefined;
    const r = await run({ cmd: "photo_solve", points, edges, scale, place: true, build });
    if (!r.ok) return;
    const misses = r.misses as [string, number][][];
    const placed = (r.points as unknown[]).filter(Boolean).length;
    setResult({ misses, placed });
    const worst = Math.max(0, ...misses.flat().map((m) => m[1]));
    onToast(`${placed} corner${placed === 1 ? "" : "s"} placed${build ? `, ${r.built} edges built` : ""}. Every mark within ${fmtLen(worst, units)}.`);
  };
  const removePoint = (i: number) => {
    setPoints(points.filter((_, k) => k !== i));
    setEdges(edges.filter(([a, b]) => a !== i && b !== i).map(([a, b]) => [a > i ? a - 1 : a, b > i ? b - 1 : b]));
    setActive(Math.max(0, Math.min(active, points.length - 2)));
    setResult(null);
  };

  const step = !cur
    ? "Pick a corner you can see in two photos and click it in one of them."
    : Object.keys(cur.marks).length < 2
      ? `Now click ${cur.name} in another photo. The dashed line shows where it has to be.`
      : `${cur.name} is in ${Object.keys(cur.marks).length} photos. Click the next corner to start it${chain ? " (it'll connect to this one)" : ""}, or Solve.`;

  if (photos.length < 2) {
    return (
      <div className="sheet-backdrop" onClick={onClose}>
        <div className="sheet" style={{ width: "min(520px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()}>
          <div className="sheet-head">
            <b>Model from photos</b>
          </div>
          <p>Match at least two photos of the same building first (Views › Match photo…), each with the origin marker on the <b>same corner</b>. Then come back here to mark corners in both.</p>
          <button className="btn small" onClick={onClose}>
            OK
          </button>
        </div>
      </div>
    );
  }
  const missOf = (i: number) => (result?.misses[i]?.length ? Math.max(...result.misses[i].map((m) => m[1])) : null);
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet photomodel" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Model from photos">
        <div className="sheet-head">
          <b>Model from photos</b>
          <span className="muted">{step}</span>
        </div>
        <div className="pm-body">
          <div className="pm-photos">
            {photos.map((v) => {
              const a = aspect[v.name] ?? 4 / 3;
              const pos = ([u, w]: UV) => ({ left: `${50 + (u * 50) / a}%`, top: `${50 - w * 50}%` });
              const toSvg = ([u, w]: UV) => [50 + (u * 50) / a, 50 - w * 50];
              return (
                <figure key={v.name} className="pm-photo">
                  <div className="pm-frame" onClick={(e) => click(v.name, e)} title="Click a corner">
                    <img src={v.photo} alt={v.name} onLoad={(e) => {
                        const ratio = e.currentTarget.naturalWidth / e.currentTarget.naturalHeight;
                        setAspect((s) => ({ ...s, [v.name]: ratio }));
                      }} draggable={false} />
                    <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
                      {(epipolar[v.name] ?? []).map(([p, q], i) => {
                        const [x1, y1] = toSvg(p);
                        const [x2, y2] = toSvg(q);
                        return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} className="pm-epi" />;
                      })}
                      {edges.map(([i, j], k) => {
                        const p = points[i]?.marks[v.name];
                        const q = points[j]?.marks[v.name];
                        if (!p || !q) return null;
                        const [x1, y1] = toSvg(p);
                        const [x2, y2] = toSvg(q);
                        return <line key={`e${k}`} x1={x1} y1={y1} x2={x2} y2={y2} className="pm-edge" />;
                      })}
                    </svg>
                    {points.map((p, i) =>
                      p.marks[v.name] ? (
                        <span key={i} className={`pm-mark ${i === active ? "on" : ""}`} style={pos(p.marks[v.name])}>
                          {i + 1}
                        </span>
                      ) : null
                    )}
                  </div>
                  <figcaption>{v.name}</figcaption>
                </figure>
              );
            })}
          </div>
          <aside className="pm-side">
            <div className="section-title">Corners</div>
            <ol className="pm-points">
              {points.map((p, i) => {
                const n = Object.keys(p.marks).length;
                const miss = missOf(i);
                return (
                  <li key={i} className={i === active ? "on" : ""} onClick={() => setActive(i)}>
                    <b>{i + 1}</b>
                    <span className="name">{p.name}</span>
                    <span className={`count ${n < 2 ? "warn" : ""}`}>{n < 2 ? "1 photo" : miss != null ? `±${fmtLen(miss, units)}` : `${n} photos`}</span>
                    <button className="rowbtn" aria-label={`Remove ${p.name}`} onClick={(e) => (e.stopPropagation(), removePoint(i))}>
                      ×
                    </button>
                  </li>
                );
              })}
              {!points.length && <li className="empty">Corners you mark show up here.</li>}
            </ol>
            <button className="btn small ghost" onClick={() => (setPoints([...points, { name: `Corner ${points.length + 1}`, marks: {} }]), setActive(points.length))}>
              + New corner
            </button>
            <label className="field check" style={{ marginTop: 8 }}>
              <input type="checkbox" checked={chain} onChange={(e) => setChain(e.target.checked)} />
              <span>Connect as I go (trace an outline)</span>
            </label>
            <div className="section-title" style={{ marginTop: 12 }}>
              Edges {edges.length ? `(${edges.length})` : ""}
            </div>
            <p className="muted small">{edges.length ? edges.map(([a, b]) => `${a + 1}–${b + 1}`).join(", ") : "None yet. With Connect as I go on, each new corner joins the last."}</p>
            {edges.length > 0 && (
              <>
                <div className="section-title" style={{ marginTop: 10 }}>
                  Known length (sets the scale)
                </div>
                <div className="row gap">
                  <select value={known.edge} onChange={(e) => setKnown({ ...known, edge: Number(e.target.value) })}>
                    {edges.map(([a, b], k) => (
                      <option key={k} value={k}>
                        {a + 1}–{b + 1}
                      </option>
                    ))}
                  </select>
                  <input className="num" placeholder={units === "ft" || units === "in" ? `22'` : "6.7"} value={known.text} onChange={(e) => setKnown({ ...known, text: e.target.value })} />
                </div>
              </>
            )}
          </aside>
        </div>
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="btn small" disabled={!ready} onClick={() => solve(false)} title="Work out where the corners are; they appear as guide points">
            Solve ({ready} ready)
          </button>
          <button className="btn small ghost" disabled={!ready || !edges.length} onClick={() => solve(true)} title="Solve, then draw the edges between corners (closed outlines become faces)">
            Solve and build edges
          </button>
          <span className="muted small">{result ? `${result.placed} placed. Lines snap to the guide points, so you can keep drawing from them.` : "Every photo must have its origin marker on the same real corner."}</span>
          <span style={{ flex: 1 }} />
          <button className="btn small ghost" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function fmtLen(m: number, units: string) {
  if (units === "ft" || units === "in") {
    const inches = m / 0.0254;
    return inches < 12 ? `${inches.toFixed(inches < 1 ? 2 : 1)}"` : `${(inches / 12).toFixed(1)}'`;
  }
  return m < 1 ? `${Math.round(m * 1000) / 10} cm` : `${m.toFixed(2)} m`;
}
