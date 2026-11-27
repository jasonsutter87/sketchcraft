// The welcome screen: what to start from (a new model in a style and units, a starter, an example), Open file, the
// models opened or saved lately, and Learn. It shows at launch (unless turned off) and from Help › Welcome.
import { useState } from "react";
import { Icons, Logo } from "./icons";
import { Sheet } from "./sheet";
import { CLASSIC_STYLE, STYLE_PRESETS } from "./styledefaults";
import { EXAMPLES, TEMPLATES } from "./templates";
import type { RecentFile } from "./recent";
import type { Styles } from "./viewport/types";

const KEY = "sketchcraft.welcome";

/** Show the welcome screen when the app starts? On unless turned off; never under automation (test browsers). */
export function welcomeAtStart(store: Pick<Storage, "getItem"> | undefined = globalThis.localStorage, automated = typeof navigator !== "undefined" && !!navigator.webdriver): boolean {
  if (automated) return false;
  try {
    return store?.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export function setWelcomeAtStart(on: boolean, store: Pick<Storage, "setItem"> | undefined = globalThis.localStorage) {
  try {
    store?.setItem(KEY, on ? "on" : "off");
  } catch {
    /* storage unavailable: it shows again next time */
  }
}

/** A new, empty model: a look and the units it measures in. */
export interface Start {
  id: string;
  name: string;
  units: string;
  unitsLabel: string;
  about: string;
  styles: Partial<Styles>;
  /** how the preview is drawn */
  look: "sky" | "studio" | "daylight";
}

const preset = (name: string) => STYLE_PRESETS.find((p) => p.name === name)?.styles ?? {};
const VISUAL: Partial<Styles> = { face: "textured", background: "daylight", profiles: false, sketchy: false, extensions: false, ao: false, edges: true, lineWeights: false, shadows: true };

export const STARTS: Start[] = [
  { id: "simple-m", name: "Simple", units: "m", unitsLabel: "Meters", about: "The Classic look: sky, ground and bold outlines", styles: CLASSIC_STYLE, look: "sky" },
  { id: "simple-ft", name: "Simple", units: "ft", unitsLabel: "Feet and inches", about: "The Classic look: sky, ground and bold outlines", styles: CLASSIC_STYLE, look: "sky" },
  { id: "arch-mm", name: "Architectural", units: "mm", unitsLabel: "Millimeters", about: "Plain faces, line weights and soft shading where surfaces meet", styles: preset("Architectural"), look: "studio" },
  { id: "arch-ft", name: "Architectural", units: "ft", unitsLabel: "Feet and inches", about: "Plain faces, line weights and soft shading where surfaces meet", styles: preset("Architectural"), look: "studio" },
  { id: "vis-m", name: "Visualization", units: "m", unitsLabel: "Meters", about: "Daylight, shadows and pictures on faces, ready to render", styles: VISUAL, look: "daylight" },
  { id: "vis-ft", name: "Visualization", units: "ft", unitsLabel: "Feet and inches", about: "Daylight, shadows and pictures on faces, ready to render", styles: VISUAL, look: "daylight" },
];

/** A small drawn preview of a start: its backdrop, the axes and a box in its look. */
function StartArt({ look }: { look: Start["look"] }) {
  const sky = look === "sky" ? ["#cfe3f5", "#eef4f8"] : look === "daylight" ? ["#f4f8fc", "#ffffff"] : ["#eceef2", "#f6f7f9"];
  const ground = look === "sky" ? "#c6cdb9" : look === "daylight" ? "#e9ecef" : "#e2e4e8";
  const outline = look === "daylight" ? 1 : 2.4;
  return (
    <svg viewBox="0 0 160 100" className="welcome-art" aria-hidden="true">
      <defs>
        <linearGradient id={`sky-${look}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={sky[0]} />
          <stop offset="1" stopColor={sky[1]} />
        </linearGradient>
      </defs>
      <rect width="160" height="100" fill={`url(#sky-${look})`} />
      <rect y="52" width="160" height="48" fill={ground} />
      {/* the axes from the origin */}
      <line x1="58" y1="72" x2="150" y2="96" stroke="#d65a5a" strokeWidth="1" />
      <line x1="58" y1="72" x2="150" y2="44" stroke="#4fa36b" strokeWidth="1" />
      <line x1="58" y1="72" x2="58" y2="6" stroke="#5a78d6" strokeWidth="1" />
      {/* a box standing on the ground */}
      <g stroke="#2a2d34" strokeWidth={outline} strokeLinejoin="round">
        <path d="M70 70 L96 78 L122 70 L96 62 Z" fill="#ffffff" />
        <path d="M70 70 L70 46 L96 54 L96 78 Z" fill="#e6e8ee" />
        <path d="M96 78 L96 54 L122 46 L122 70 Z" fill="#d7dbe4" />
        <path d="M70 46 L96 38 L122 46 L96 54 Z" fill="#ffffff" />
      </g>
      {look === "daylight" && <path d="M96 78 L122 70 L140 76 L114 84 Z" fill="rgba(40,44,52,0.12)" />}
    </svg>
  );
}

const ago = (at: number, now = Date.now()) => {
  const m = Math.round((now - at) / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  return d < 30 ? `${d} day${d === 1 ? "" : "s"} ago` : new Date(at).toLocaleDateString();
};

export function Welcome({
  recent,
  onStart,
  onStarter,
  onExample,
  onOpen,
  onRecent,
  onLearn,
  onClose,
}: {
  recent: RecentFile[];
  onStart: (s: Start) => void;
  onStarter: (lines: string[]) => void;
  onExample: (name: string, data: string) => void;
  onOpen: () => void;
  onRecent: (path: string) => void;
  onLearn: () => void;
  onClose: () => void;
}) {
  const [atStart, setAtStart] = useState(() => welcomeAtStart(undefined, false));
  return (
    <Sheet onClose={onClose} label="Welcome to SketchCraft" className="welcome" width={980}>
      <nav className="welcome-nav">
        <div className="welcome-brand">
          <Logo />
          <span>SketchCraft</span>
        </div>
        <button className="welcome-tab on" aria-current="page">
          Home
        </button>
        <button className="welcome-tab" onClick={onLearn}>
          Learn
        </button>
        <div className="welcome-foot">
          <label className="field">
            <input
              type="checkbox"
              checked={atStart}
              onChange={(e) => {
                setAtStart(e.target.checked);
                setWelcomeAtStart(e.target.checked);
              }}
            />
            <span>Show this when SketchCraft starts</span>
          </label>
          <p className="muted small">Free, forever. Help › Welcome… brings this back.</p>
        </div>
      </nav>
      <div className="welcome-main">
        <div className="welcome-head">
          <h2>Start a new model</h2>
          <button className="btn small ghost" onClick={onClose} aria-label="Close" title="Close (Esc): keep working on the model that's open">
            Close
          </button>
        </div>
        <div className="welcome-starts" role="list">
          {STARTS.map((s) => (
            <button key={s.id} className="welcome-start" role="listitem" onClick={() => onStart(s)} title={`${s.name}, in ${s.unitsLabel.toLowerCase()}: ${s.about}`} data-testid={`start-${s.id}`}>
              <StartArt look={s.look} />
              <b>{s.name}</b>
              <span className="muted small">{s.unitsLabel}</span>
            </button>
          ))}
        </div>

        <div className="welcome-row">
          <button className="btn" onClick={onOpen}>
            <Icons.import /> Open file…
          </button>
          <span className="muted small">.sketchcraft, SketchUp (.skp), DXF, DWG, IFC, FBX, OBJ and scans</span>
        </div>

        <h3>Starters and examples</h3>
        <div className="welcome-starters">
          {TEMPLATES.map((t) => (
            <button key={t.name} className="welcome-starter" onClick={() => onStarter(t.lines)} title={t.about}>
              <span className="welcome-starter-art" style={{ background: t.tint }}>
                <Icons.cube />
              </span>
              <span>{t.name}</span>
            </button>
          ))}
          {EXAMPLES.map((x) => (
            <button key={x.name} className="welcome-starter" onClick={() => onExample(x.name, x.data)} title={x.about}>
              <span className="welcome-starter-art">
                <img src={x.thumb} alt="" />
              </span>
              <span>{x.name}</span>
            </button>
          ))}
        </div>

        <h3>Recent files</h3>
        {recent.length ? (
          <ul className="welcome-recent">
            {recent.map((r) => (
              <li key={r.path}>
                <button onClick={() => onRecent(r.path)} title={r.path}>
                  <b>{r.path.split(/[\\/]/).pop()}</b>
                  <span className="muted small">{r.path.replace(/[\\/][^\\/]*$/, "")}</span>
                  <span className="muted small">{ago(r.at)}</span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="muted small">Models you open or save show up here.</p>
        )}
      </div>
    </Sheet>
  );
}
