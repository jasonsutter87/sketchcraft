// The timeline: clips played in order for a video. Turntables circle the
// model, walks replay a walk you record (or a path), scene clips glide
// between saved scenes, and sun studies hold a camera while the sun crosses
// the sky (from the built-in solar position). The engine works out every
// frame; this panel previews them in the viewport and exports a video by
// rendering each frame there (with shadows) for the engine to encode.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { exec, type CamSpec, type Clip, type Reply, type Scene, type TimelineFrame } from "./engine";
import { pickSave } from "./files";
import type { Tool, ViewportApi } from "./Viewport";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

const FPS = 30;
const KIND_NAME: Record<Clip["kind"], string> = { turntable: "Turntable", walk: "Walkthrough", scenes: "Scenes", sun: "Sun study" };
const SIZES: Record<string, [number, number]> = { "720p": [1280, 720], "1080p": [1920, 1080], "Square": [1080, 1080], "Vertical": [1080, 1920] };

function clipLabel(c: Clip): string {
  if (c.name) return c.name;
  if (c.kind === "scenes") return c.views.length ? c.views.join(" → ") : "Scenes";
  if (c.kind === "sun") return `Sun ${c.from ?? "07:00"}–${c.to ?? "19:00"}`;
  if (c.kind === "turntable") return (c.turns ?? 1) === 1 ? "Turntable" : `Turntable ×${c.turns}`;
  return "Walkthrough";
}

/** today in local time (toISOString is UTC: the evening would be tomorrow) */
const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

export function TimelinePanel({
  scene,
  run,
  api,
  setTool,
  onToast,
  onClose,
}: {
  scene: Scene;
  run: Run;
  api: React.MutableRefObject<ViewportApi | null>;
  setTool: (t: Tool) => void;
  onToast: (t: string, bad?: boolean) => void;
  onClose: () => void;
}) {
  const clips = useMemo(() => scene.timeline?.clips ?? [], [scene.timeline]);
  const total = clips.reduce((s, c) => s + c.seconds, 0);
  const [selected, setSelected] = useState<number | null>(null);
  const [frames, setFrames] = useState<TimelineFrame[]>([]);
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [recording, setRecording] = useState<{ start: number; keys: CamSpec[] } | null>(null);
  const [exporting, setExporting] = useState<{ done: number; of: number } | null>(null);
  const [size, setSize] = useState("1080p");
  const cancel = useRef(false);
  const raf = useRef(0);

  // the frames, worked out by the engine whenever the clips change
  useEffect(() => {
    let live = true;
    if (!clips.length) return setFrames([]);
    exec({ cmd: "timeline_frames", fps: FPS }).then((r) => {
      if (live && r.ok) setFrames(r.frames as TimelineFrame[]);
    });
    return () => {
      live = false;
    };
  }, [clips]);

  const show = useCallback(
    (t: number) => {
      if (!frames.length || !api.current) return;
      const f = frames[Math.min(frames.length - 1, Math.max(0, Math.round(t * FPS)))];
      api.current.setCamera(f);
      api.current.setSun(f.sun ?? null);
    },
    [frames, api],
  );

  // put the light back when the panel closes
  useEffect(() => () => api.current?.setSun(null), [api]);

  // play
  useEffect(() => {
    if (!playing) return;
    const t0 = performance.now() - time * 1000;
    const tick = () => {
      const t = (performance.now() - t0) / 1000;
      if (t >= total) {
        setTime(total);
        show(total);
        setPlaying(false);
        return;
      }
      setTime(t);
      show(t);
      raf.current = requestAnimationFrame(tick);
    };
    raf.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing]);

  // record a walk: sample the camera while you walk
  useEffect(() => {
    if (!recording) return;
    const id = window.setInterval(() => {
      const c = api.current?.camera();
      if (c) recording.keys.push({ eye: c.eye, target: c.target, fov: c.fov });
    }, 1000 / 15);
    return () => window.clearInterval(id);
  }, [recording, api]);

  const save = (next: Clip[]) => run({ cmd: "timeline", clips: next });
  const camNow = (): CamSpec | null => {
    const c = api.current?.camera();
    return c ? { eye: c.eye, target: c.target, fov: c.fov } : null;
  };
  const add = async (c: Clip) => {
    const r = await save([...clips, c]);
    if (r.ok) setSelected(clips.length);
  };
  const update = (i: number, patch: Partial<Clip>) => save(clips.map((c, k) => (k === i ? ({ ...c, ...patch } as Clip) : c)));
  const remove = (i: number) => {
    setSelected(null);
    save(clips.filter((_, k) => k !== i));
  };
  const shift = (i: number, by: number) => {
    const j = i + by;
    if (j < 0 || j >= clips.length) return;
    const next = clips.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setSelected(j);
    save(next);
  };

  const startWalk = () => {
    setPlaying(false);
    setTool("walk");
    const c = camNow();
    setRecording({ start: performance.now(), keys: c ? [c] : [] });
    const high = (c?.eye[2] ?? 0) > 6;
    onToast(high ? "Recording from up here: for a walk at eye height, Stop, use Position Camera (click where to stand), then record." : "Recording: walk with the arrow keys or WASD (or drag). Press Stop when you're done.");
  };
  const stopWalk = async () => {
    if (!recording) return;
    const keys = recording.keys;
    const seconds = Math.max(2, Math.round(((performance.now() - recording.start) / 1000) * 10) / 10);
    setRecording(null);
    // drop the standing-still at either end
    const moved = (a: CamSpec, b: CamSpec) => Math.hypot(a.eye[0] - b.eye[0], a.eye[1] - b.eye[1], a.eye[2] - b.eye[2]) + Math.hypot(a.target[0] - b.target[0], a.target[1] - b.target[1]) > 1e-3;
    let s = 0, e = keys.length - 1;
    while (s < e && !moved(keys[s], keys[s + 1])) s++;
    while (e > s && !moved(keys[e - 1], keys[e])) e--;
    const kept = keys.slice(s, e + 1);
    if (kept.length < 2) return onToast("Nothing recorded: walk around while recording", true);
    await add({ kind: "walk", seconds, keys: kept });
    onToast(`Walkthrough: ${seconds}s. Drag its length to speed it up or slow it down.`);
  };

  const exportVideo = async () => {
    if (!frames.length) return;
    const path = await pickSave("Timeline.mp4", "mp4", "MP4 video");
    if (!path || !api.current) return;
    setPlaying(false);
    cancel.current = false;
    const [w, h] = SIZES[size];
    const session = `t${Date.now().toString(36)}`;
    setExporting({ done: 0, of: frames.length });
    try {
      for (let i = 0; i < frames.length; i++) {
        if (cancel.current) {
          onToast("Export stopped");
          return;
        }
        const f = frames[i];
        api.current.setCamera(f);
        api.current.setSun(f.sun ?? null);
        const data = api.current.captureFrame(w, h, f.clock);
        const r = await exec({ cmd: "video_frame", session, index: i, data });
        if (!r.ok) return onToast(String(r.error), true);
        if (i % 3 === 0) setExporting({ done: i + 1, of: frames.length });
      }
      setExporting({ done: frames.length, of: frames.length });
      const r = await exec({ cmd: "video_finish", session, path, fps: FPS });
      if (r.ok) onToast(`Exported ${frames.length} frames (${total.toFixed(1)}s) to ${path.split("/").pop()}`);
      else onToast(String(r.error), true);
    } finally {
      setExporting(null);
      api.current?.setSun(frames[Math.min(frames.length - 1, Math.round(time * FPS))]?.sun ?? null);
    }
  };

  const sel = selected != null ? clips[selected] : null;
  const scrub = (e: React.PointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const t = Math.max(0, Math.min(total, ((e.clientX - r.left) / r.width) * total));
    setPlaying(false);
    setTime(t);
    show(t);
  };

  return (
    <div className="timeline" role="region" aria-label="Timeline">
      <div className="tl-head">
        <button className="tl-play" onClick={() => (time >= total - 0.01 ? (setTime(0), setPlaying(true)) : setPlaying(!playing))} disabled={!frames.length || !!recording} aria-label={playing ? "Pause" : "Play"} title={playing ? "Pause" : "Play"}>
          {playing ? "❚❚" : "▶"}
        </button>
        <span className="tl-time">
          {time.toFixed(1)}s / {total.toFixed(1)}s
        </span>
        <span className="sep" />
        <span className="lbl">Add</span>
        <button onClick={() => add({ kind: "turntable", seconds: 8, turns: 1, from: camNow() })} title="Circle the model from this view">
          Turntable
        </button>
        {recording ? (
          <button className="tl-rec on" onClick={stopWalk}>
            ● Stop recording
          </button>
        ) : (
          <button onClick={startWalk} title="Walk through the model with the arrow keys; it's recorded as you go">
            Walkthrough
          </button>
        )}
        <button onClick={() => add({ kind: "scenes", seconds: Math.max(4, scene.views.length * 3), views: scene.views.map((v) => v.name), hold: 1 })} disabled={scene.views.length < 1} title={scene.views.length ? "Glide through your saved scenes" : "Save a scene first (+ Scene)"}>
          Scenes
        </button>
        <button onClick={() => add({ kind: "sun", seconds: 8, date: today(), from: "07:00", to: "19:00", camera: camNow() })} title="Hold this view while the sun crosses the sky">
          Sun study
        </button>
        <span style={{ flex: 1 }} />
        <select value={size} onChange={(e) => setSize(e.target.value)} aria-label="Video size">
          {Object.keys(SIZES).map((k) => (
            <option key={k}>{k}</option>
          ))}
        </select>
        {exporting ? (
          <button onClick={() => (cancel.current = true)}>
            Stop ({Math.round((exporting.done / exporting.of) * 100)}%)
          </button>
        ) : (
          <button className="on" onClick={exportVideo} disabled={!frames.length || !!recording}>
            Export video…
          </button>
        )}
        <button className="tl-close" onClick={onClose} aria-label="Close the timeline" title="Close">
          ×
        </button>
      </div>

      <div className="tl-lane">
      <div
        className="tl-ruler"
        onPointerDown={(e) => total > 0 && (e.currentTarget.setPointerCapture(e.pointerId), scrub(e))}
        onPointerMove={(e) => e.buttons & 1 && total > 0 && scrub(e)}
        title="Drag to scrub"
        aria-label="Time: drag to scrub"
      >
        {total > 0 &&
          Array.from({ length: Math.floor(total) + 1 }, (_, s) => s)
            .filter((s) => s % (total > 60 ? 10 : total > 20 ? 5 : 1) === 0)
            .map((s) => (
              <span key={s} style={{ left: `${(s / total) * 100}%` }}>
                {s}s
              </span>
            ))}
      </div>
      <div className="tl-track">
        {clips.length === 0 && <span className="tl-empty">Add a clip: a turntable, a walk you record, a glide through scenes, or a sun study.</span>}
        {clips.map((c, i) => (
          <button
            key={i}
            className={`tl-clip k-${c.kind} ${selected === i ? "sel" : ""}`}
            style={{ flexGrow: c.seconds }}
            onClick={() => {
              setSelected(selected === i ? null : i);
              const start = clips.slice(0, i).reduce((s, x) => s + x.seconds, 0);
              setPlaying(false);
              setTime(start);
              show(start);
            }}
            title={`${KIND_NAME[c.kind]}: ${c.seconds}s`}
          >
            <b>{KIND_NAME[c.kind]}</b>
            <span>
              {clipLabel(c)} · {c.seconds}s
            </span>
          </button>
        ))}
      </div>
        {total > 0 && <div className="tl-head-line" style={{ left: `calc(4px + (100% - 8px) * ${time / total})` }} />}
      </div>

      {sel && selected != null && (
        <div className="tl-inspect">
          <b>{KIND_NAME[sel.kind]}</b>
          <label>
            Seconds
            <input className="num" type="number" min={0.5} step={0.5} value={sel.seconds} onChange={(e) => Number(e.target.value) > 0 && update(selected, { seconds: Number(e.target.value) })} />
          </label>
          {sel.kind === "turntable" && (
            <>
              <label>
                Turns
                <input className="num" type="number" step={0.25} value={sel.turns ?? 1} onChange={(e) => update(selected, { turns: Number(e.target.value) })} />
              </label>
              <button onClick={() => update(selected, { from: camNow() })} title="Start the turn from where you're looking now">
                Start from this view
              </button>
            </>
          )}
          {sel.kind === "scenes" && (
            <>
              <span className="tl-views">
                {scene.views.map((v) => (
                  <label key={v.name} className="check">
                    <input
                      type="checkbox"
                      checked={sel.views.includes(v.name)}
                      onChange={(e) => {
                        const views = e.target.checked ? scene.views.map((x) => x.name).filter((n) => n === v.name || sel.views.includes(n)) : sel.views.filter((n) => n !== v.name);
                        if (views.length) update(selected, { views });
                      }}
                    />
                    {v.name}
                  </label>
                ))}
              </span>
              <label>
                Hold
                <input className="num" type="number" min={0} step={0.5} value={sel.hold ?? 1} onChange={(e) => update(selected, { hold: Number(e.target.value) })} />
              </label>
            </>
          )}
          {sel.kind === "sun" && (
            <>
              <label>
                Date
                <input type="date" value={sel.date ?? today()} onChange={(e) => e.target.value && update(selected, { date: e.target.value })} />
              </label>
              <label>
                From
                <input type="time" value={sel.from ?? "07:00"} onChange={(e) => e.target.value && update(selected, { from: e.target.value })} />
              </label>
              <label>
                To
                <input type="time" value={sel.to ?? "19:00"} onChange={(e) => e.target.value && update(selected, { to: e.target.value })} />
              </label>
              <button onClick={() => update(selected, { camera: camNow() })}>Use this view</button>
              <span className="muted small">{scene.location ? "At your model's location" : "No location set: File › Add location… for real sun angles"}</span>
            </>
          )}
          {sel.kind === "walk" && <span className="muted small">{sel.keys?.length ? `${sel.keys.length} recorded steps, played at a steady pace` : `${sel.path?.length ?? 0} path points`}</span>}
          <span style={{ flex: 1 }} />
          <button onClick={() => shift(selected, -1)} disabled={selected === 0} aria-label="Move earlier" title="Move earlier">
            ←
          </button>
          <button onClick={() => shift(selected, 1)} disabled={selected === clips.length - 1} aria-label="Move later" title="Move later">
            →
          </button>
          <button onClick={() => remove(selected)}>Delete</button>
        </div>
      )}
    </div>
  );
}
