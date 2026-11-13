// Learn: short interactive lessons whose steps watch what you do (no Next
// button: draw the face and the step ticks itself off), and the feature
// demos (GIFs captured from the real app by tools/demos; see the
// capture-demos skill). Progress stays in this browser.

import { useEffect, useMemo, useState } from "react";
import { type Scene } from "./engine";
import { store } from "./skins";

/** What a step can look at. */
export interface LearnCtx {
  scene: Scene;
  workspace: string;
  section: boolean;
  byTag: boolean;
}

/** The model's numbers when a lesson starts, so steps measure what you did. */
interface Base {
  faces: number;
  groups: number;
  painted: number;
  nested: number;
  hidden: number;
  tagged: number;
  views: number;
  photos: number;
  placed: number;
}

function measure(s: Scene): Base {
  return {
    faces: s.faces.length,
    groups: s.groups.length,
    painted: s.faces.filter((f) => f.material).length,
    nested: s.groups.filter((g) => g.parent != null).length,
    hidden: s.groups.filter((g) => g.hidden).length,
    tagged: s.groups.filter((g) => g.tag).length + s.instances.filter((i) => i.tag).length,
    views: s.views.length,
    photos: s.views.filter((v) => v.photo).length,
    placed: (s.photoPoints ?? []).filter((p) => Object.keys(p.marks).length >= 2).length,
  };
}

interface Step {
  /** what to do, in one line */
  do: string;
  /** how, if it isn't obvious */
  how?: string;
  done: (c: LearnCtx, b: Base) => boolean;
}

interface Lesson {
  id: string;
  title: string;
  about: string;
  minutes: number;
  /** a demo that shows it (tools/demos name) */
  demo?: string;
  steps: Step[];
}

export const LESSONS: Lesson[] = [
  {
    id: "first-box",
    title: "Your first box",
    about: "Draw an outline, pull it up: the move everything else is built from.",
    minutes: 2,
    steps: [
      { do: "Draw a rectangle on the ground", how: "Press R, click one corner, click the opposite corner.", done: (c, b) => c.scene.faces.length > b.faces },
      { do: "Pull it up into a box", how: "Press P, click the face, move up, click.", done: (c, b) => c.scene.faces.length >= b.faces + 6 },
      { do: "Paint a face", how: "Press B, pick a color on the right, click a face.", done: (c, b) => c.scene.faces.filter((f) => f.material).length > b.painted },
      { do: "Make it a group", how: "Press Space, drag a box around it, then right-click › Make group.", done: (c, b) => c.scene.groups.length > b.groups },
    ],
  },
  {
    id: "organize",
    title: "Organize with groups and tags",
    about: "Groups inside groups, hiding whole chains, tags as layers.",
    minutes: 3,
    demo: "hierarchy-tags",
    steps: [
      { do: "Make an empty group to organize into", how: "Outliner › + New group (call it Upper floor).", done: (c, b) => c.scene.groups.length > b.groups },
      { do: "Put a group inside another", how: "Drag a row in the Outliner onto a group, or select both and right-click › Group these.", done: (c, b) => c.scene.groups.filter((g) => g.parent != null).length > b.nested },
      { do: "Hide a group (and everything in it)", how: "The eye on its row in the Outliner.", done: (c, b) => c.scene.groups.filter((g) => g.hidden).length > b.hidden },
      { do: "Give something a tag", how: "Select it, then Outliner › Tag selected…", done: (c, b) => c.scene.groups.filter((g) => g.tag).length + c.scene.instances.filter((i) => i.tag).length > b.tagged },
      { do: "Color the model by tag", how: "Tick Color by tag above the tag list.", done: (c) => c.byTag },
    ],
  },
  {
    id: "photos",
    title: "Model from photos",
    about: "Match two photos, mark the same corners, get real 3D.",
    minutes: 6,
    demo: "photo-modeling",
    steps: [
      { do: "Match a photo", how: "Views › Match photo…: line up the red and green edges; put the origin on a corner.", done: (c, b) => c.scene.views.filter((v) => v.photo).length > b.photos },
      { do: "Match a second photo of the same building", how: "Same corner for the origin in both photos.", done: (c, b) => c.scene.views.filter((v) => v.photo).length >= b.photos + 2 || c.scene.views.filter((v) => v.photo).length >= 2 },
      { do: "Mark corners in both and Solve", how: "Views › Model from photos…: click a corner in one photo, then on the dashed line in the other.", done: (c, b) => (c.scene.photoPoints ?? []).filter((p) => Object.keys(p.marks).length >= 2).length > b.placed },
      { do: "Build the faces", how: "Trace an outline (click corner 1 again to close it), then Solve and build edges.", done: (c, b) => c.scene.faces.length > b.faces },
    ],
  },
  {
    id: "present",
    title: "Put it somewhere and render",
    about: "A setting around the model, a saved scene, a photoreal picture.",
    minutes: 3,
    demo: "environments",
    steps: [
      { do: "Add an environment", how: "Library › Environments, or Setting in the Arch / Previz strip.", done: (c) => !!c.scene.environment },
      { do: "Switch to the Previz workspace", how: "Previz, at the top.", done: (c) => c.workspace === "previz" },
      { do: "Frame a shot and save it as a scene", how: "Orbit to a nice angle, then + Scene in the strip.", done: (c, b) => c.scene.views.length > b.views },
    ],
  },
  {
    id: "plans",
    title: "Floor plans",
    about: "See each floor from above, the way a drawing does.",
    minutes: 2,
    steps: [
      { do: "Switch to the Arch workspace", how: "Arch, at the top.", done: (c) => c.workspace === "arch" },
      { do: "Cut a floor plan", how: "A floor button in the strip (Main, Upper), or Plan in the view bar.", done: (c) => c.section },
    ],
  },
];

interface Demo {
  title: string;
  about: string;
  topic?: string;
  gif: string;
  captured?: string;
}

function useDemos() {
  const [demos, setDemos] = useState<Record<string, Demo>>({});
  useEffect(() => {
    fetch("learn/demos.json")
      .then((r) => (r.ok ? r.json() : {}))
      .then(setDemos)
      .catch(() => setDemos({}));
  }, []);
  return demos;
}

const DONE_KEY = "sketchcraft.lessonsDone";
export const lessonsDone = (): string[] => {
  try {
    return JSON.parse(store.get(DONE_KEY) ?? "[]");
  } catch {
    return [];
  }
};

/** The Learn center: lessons to do, demos to watch. */
export function LearnCenter({ onStart, onClose }: { onStart: (id: string) => void; onClose: () => void }) {
  const demos = useDemos();
  const [q, setQ] = useState("");
  const [tab, setTab] = useState<"lessons" | "watch">("lessons");
  const [watching, setWatching] = useState<string | null>(null);
  // with reduced motion on, demos play only when asked
  const reduce = typeof matchMedia !== "undefined" && (matchMedia("(prefers-reduced-motion: reduce)").matches || document.documentElement.dataset.reduceMotion === "true");
  const done = lessonsDone();
  const match = (...t: string[]) => !q.trim() || t.join(" ").toLowerCase().includes(q.trim().toLowerCase());
  const lessons = LESSONS.filter((l) => match(l.title, l.about, ...l.steps.map((s) => s.do + " " + (s.how ?? ""))));
  const watch = Object.entries(demos).filter(([, d]) => match(d.title, d.about, d.topic ?? ""));
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet learn" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Learn">
        <div className="sheet-head">
          <b>Learn</b>
          <span className="muted">Lessons that watch what you do, and short demos of every feature.</span>
        </div>
        <div className="row gap learn-top">
          <div className="seg" role="tablist">
            <button role="tab" aria-selected={tab === "lessons"} className={tab === "lessons" ? "on" : ""} onClick={() => setTab("lessons")}>
              Lessons ({done.length}/{LESSONS.length})
            </button>
            <button role="tab" aria-selected={tab === "watch"} className={tab === "watch" ? "on" : ""} onClick={() => setTab("watch")}>
              Watch ({Object.keys(demos).length})
            </button>
          </div>
          <input className="learn-search" placeholder="Search: groups, photos, render…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search lessons and demos" autoFocus />
        </div>
        {tab === "lessons" && (
          <div className="learn-grid">
            {lessons.map((l) => (
              <article key={l.id} className={`learn-card ${done.includes(l.id) ? "done" : ""}`}>
                <header>
                  <b>{l.title}</b>
                  <span className="muted small">{done.includes(l.id) ? "Done ✓" : `${l.minutes} min · ${l.steps.length} steps`}</span>
                </header>
                <p>{l.about}</p>
                <ol className="small muted">
                  {l.steps.map((s, i) => (
                    <li key={i}>{s.do}</li>
                  ))}
                </ol>
                <div className="row gap">
                  <button className="btn small" onClick={() => onStart(l.id)}>
                    {done.includes(l.id) ? "Do it again" : "Start"}
                  </button>
                  {l.demo && demos[l.demo] && (
                    <button className="btn small ghost" onClick={() => (setTab("watch"), setWatching(l.demo!))}>
                      Watch first
                    </button>
                  )}
                </div>
              </article>
            ))}
            {!lessons.length && <p className="muted">Nothing matches “{q}”.</p>}
          </div>
        )}
        {tab === "watch" && (
          <div className="learn-grid">
            {watch.map(([name, d]) => (
              <article key={name} className={`learn-card demo ${watching === name ? "on" : ""}`} onClick={() => setWatching(watching === name ? null : name)}>
                <div className="demo-frame">{reduce ? <div className="demo-still">▶ Play the demo</div> : <img src={d.gif} alt={`${d.title}: ${d.about}`} loading="lazy" />}</div>
                <header>
                  <b>{d.title}</b>
                  {d.topic && <span className="muted small">{d.topic}</span>}
                </header>
                <p className="small">{d.about}</p>
              </article>
            ))}
            {!watch.length && <p className="muted">{Object.keys(demos).length ? `Nothing matches “${q}”.` : "No demos yet: run node tools/demos/capture.mjs all."}</p>}
          </div>
        )}
        {watching && demos[watching] && (
          <div className="demo-big" onClick={() => setWatching(null)} role="dialog" aria-label={demos[watching].title}>
            <img src={demos[watching].gif} alt={demos[watching].about} />
            <p>
              <b>{demos[watching].title}</b> {demos[watching].about} <span className="muted">(click to close)</span>
            </p>
          </div>
        )}
        <div className="row gap" style={{ marginTop: 12 }}>
          <span className="muted small">Press ? any time for every keyboard shortcut.</span>
          <span style={{ flex: 1 }} />
          <button className="btn small ghost" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

/** The lesson in progress: a card beside the work; steps tick themselves. */
export function LessonCard({ id, ctx, onClose, onToast }: { id: string; ctx: LearnCtx; onClose: () => void; onToast: (t: string) => void }) {
  const lesson = LESSONS.find((l) => l.id === id)!;
  const demos = useDemos();
  const [base] = useState<Base>(() => measure(ctx.scene));
  const [ticked, setTicked] = useState<boolean[]>(() => lesson.steps.map(() => false));
  const [show, setShow] = useState(false);
  // steps tick in order, and once ticked they stay ticked
  useEffect(() => {
    setTicked((t) => {
      const next = [...t];
      for (let i = 0; i < lesson.steps.length; i++) {
        if (next[i]) continue;
        if (i > 0 && !next[i - 1]) break;
        if (lesson.steps[i].done(ctx, base)) next[i] = true;
        else break;
      }
      return next.some((v, i) => v !== t[i]) ? next : t;
    });
  }, [ctx, base, lesson]);
  const current = ticked.findIndex((t) => !t);
  const finished = current === -1;
  useEffect(() => {
    if (!finished) return;
    const done = lessonsDone();
    if (!done.includes(id)) store.set(DONE_KEY, JSON.stringify([...done, id]));
    onToast(`Lesson done: ${lesson.title}.`);
  }, [finished, id, lesson.title, onToast]);
  const demo = lesson.demo ? demos[lesson.demo] : undefined;
  return (
    <aside className="lesson-card" aria-live="polite" aria-label={`Lesson: ${lesson.title}`}>
      <header>
        <span className="kicker">Lesson</span>
        <b>{lesson.title}</b>
        <button className="rowbtn" aria-label="End the lesson" onClick={onClose}>
          ×
        </button>
      </header>
      <ol>
        {lesson.steps.map((s, i) => (
          <li key={i} className={ticked[i] ? "ok" : i === current ? "now" : ""}>
            <span className="tick" aria-hidden="true">
              {ticked[i] ? "✓" : i + 1}
            </span>
            <span>
              {s.do}
              {i === current && s.how && <em>{s.how}</em>}
            </span>
          </li>
        ))}
      </ol>
      {finished ? (
        <div className="row gap">
          <b className="small">All done. Nice work.</b>
          <span style={{ flex: 1 }} />
          <button className="btn small" onClick={onClose}>
            Close
          </button>
        </div>
      ) : (
        demo && (
          <button className="btn small ghost" onClick={() => setShow((v) => !v)}>
            {show ? "Hide the demo" : "Show me"}
          </button>
        )
      )}
      {show && demo && <img className="lesson-demo" src={demo.gif} alt={demo.about} />}
    </aside>
  );
}

/** Memoized context so lesson checks only rerun when something they read changes. */
export function useLearnCtx(scene: Scene | null, workspace: string, section: boolean, byTag: boolean): LearnCtx | null {
  return useMemo(() => (scene ? { scene, workspace, section, byTag } : null), [scene, workspace, section, byTag]);
}
