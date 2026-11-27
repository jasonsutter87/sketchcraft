// 3D mouse (3Dconnexion SpaceMouse): turns what the cap is doing (pushed and twisted along six axes, plus buttons) into
// camera motion. This file is pure: no device, no three.js, no timers of its own. The device is read by the desktop
// shell (app/src-tauri/src/spacemouse.rs) and arrives through a `SpaceMouseTransport`; the camera is moved by whoever
// applies the `Nav` this produces (Viewport's `navigate`). Both ends are injected, so all of it is tested without
// hardware.
//
// The axes, as the device reports them (raw, about -350..350 at full deflection):
//   tx  cap pushed right (+)        rx  cap tilted so its top goes away from you (+)
//   ty  cap pulled toward you (+)   ry  cap rolled to the right (+)
//   tz  cap pressed down (+)        rz  cap twisted clockwise seen from above (+)
// Defaults: push left/right and lift/press pan, push away/pull toward zooms, tilt forward/back orbits up and down,
// twist turns the model around (the view stays upright, so roll does nothing). Every axis can be inverted, because
// not every driver agrees on signs.

import { store } from "./skins";

/** Raw units at full deflection (a SpaceMouse reports about ±350; a stronger push is clamped to 1). */
export const FULL_SCALE = 350;

export interface SpaceMouseFrame {
  t: [number, number, number];
  r: [number, number, number];
  /** bit n is button n */
  buttons: number;
}

export const AXES = ["tx", "ty", "tz", "rx", "ry", "rz"] as const;
export type Axis = (typeof AXES)[number];

export interface SpaceMouseSettings {
  /** use a 3D mouse at all */
  enabled: boolean;
  /** how fast the camera moves, 1 is the default (0.1 to 4) */
  speed: number;
  /** how far the cap has to move before anything happens, as a share of full deflection (0 to 0.4) */
  deadzone: number;
  /** only the strongest axis counts at a time: a straight pan stays straight */
  dominant: boolean;
  /** "rotation": the cap only pans and zooms; "pan": it only orbits and zooms */
  lock: "none" | "rotation" | "pan";
  /** flip an axis' direction */
  invert: Record<Axis, boolean>;
}

export const DEFAULT_SPACEMOUSE: SpaceMouseSettings = {
  enabled: true,
  speed: 1,
  deadzone: 0.08,
  dominant: false,
  lock: "none",
  invert: { tx: false, ty: false, tz: false, rx: false, ry: false, rz: false },
};

export const SPACEMOUSE_KEY = "sketchcraft.spacemouse";
export const SPEED_RANGE = [0.1, 4] as const;
export const DEADZONE_RANGE = [0, 0.4] as const;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Saved settings made safe: unknown or out-of-range values fall back to the defaults, one at a time. */
export function cleanSettings(raw: unknown): SpaceMouseSettings {
  const d = DEFAULT_SPACEMOUSE;
  const o = raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const num = (v: unknown, fallback: number, [lo, hi]: readonly [number, number]) => (typeof v === "number" && Number.isFinite(v) ? clamp(v, lo, hi) : fallback);
  const inv = o.invert && typeof o.invert === "object" ? (o.invert as Record<string, unknown>) : {};
  return {
    enabled: typeof o.enabled === "boolean" ? o.enabled : d.enabled,
    speed: num(o.speed, d.speed, SPEED_RANGE),
    deadzone: num(o.deadzone, d.deadzone, DEADZONE_RANGE),
    dominant: typeof o.dominant === "boolean" ? o.dominant : d.dominant,
    lock: o.lock === "rotation" || o.lock === "pan" || o.lock === "none" ? o.lock : d.lock,
    invert: Object.fromEntries(AXES.map((a) => [a, typeof inv[a] === "boolean" ? inv[a] : d.invert[a]])) as Record<Axis, boolean>,
  };
}

/** The saved settings (the same defaults-one-by-one rule as every other setting). */
export function loadSettings(): SpaceMouseSettings {
  try {
    return cleanSettings(JSON.parse(store.get(SPACEMOUSE_KEY) ?? "null"));
  } catch {
    return cleanSettings(null);
  }
}

/** One raw axis as -1..1 after the dead zone. Past the dead zone the response starts at 0 and grows smoothly (a
 * square), so small pushes are fine control and full pushes are fast. */
export function shape(raw: number, deadzone: number): number {
  if (!Number.isFinite(raw)) return 0;
  const v = clamp(raw / FULL_SCALE, -1, 1);
  const m = Math.abs(v);
  if (m <= deadzone) return 0;
  const k = (m - deadzone) / (1 - deadzone);
  return Math.sign(v) * k * k;
}

/** The six axes after dead zone, inversion and "strongest only". Order: tx ty tz rx ry rz. */
export function axes(f: SpaceMouseFrame, s: SpaceMouseSettings): Record<Axis, number> {
  const raw = [...f.t, ...f.r];
  const v = AXES.map((a, i) => {
    const x = shape(raw[i], s.deadzone);
    return s.invert[a] ? -x : x;
  });
  if (s.dominant) {
    let big = 0;
    for (let i = 1; i < 6; i++) if (Math.abs(v[i]) > Math.abs(v[big])) big = i;
    for (let i = 0; i < 6; i++) if (i !== big) v[i] = 0;
  }
  return Object.fromEntries(AXES.map((a, i) => [a, v[i]])) as Record<Axis, number>;
}

/** What the camera should do per second, at speed 1 and full deflection. */
export const RATES = {
  /** radians per second of orbit */
  orbit: 2.2,
  /** share of the distance to the target panned per second */
  pan: 1.1,
  /** the camera's distance to the target changes by e^(zoom) per second */
  zoom: 1.6,
};

/** A slice of camera motion. */
export interface Nav {
  /** radians turned around the model's vertical axis (+ = the model turns clockwise seen from above) */
  yaw: number;
  /** radians the camera goes up over the model (+) or down (-) */
  pitch: number;
  /** pan, as a share of the distance to the target: right (+) and up (+) */
  panRight: number;
  panUp: number;
  /** multiply the distance to the target by this (< 1 zooms in) */
  dolly: number;
}

export const NO_NAV: Nav = { yaw: 0, pitch: 0, panRight: 0, panUp: 0, dolly: 1 };

/** The camera motion for `dt` seconds of the cap held as `f`. */
export function navFor(f: SpaceMouseFrame, s: SpaceMouseSettings, dt: number): Nav {
  const a = axes(f, s);
  const k = s.speed * dt;
  const orbit = s.lock === "rotation" ? 0 : 1;
  const pan = s.lock === "pan" ? 0 : 1;
  return {
    yaw: orbit * a.rz * RATES.orbit * k,
    pitch: orbit * a.rx * RATES.orbit * k,
    panRight: pan * a.tx * RATES.pan * k,
    panUp: pan * -a.tz * RATES.pan * k,
    dolly: Math.exp(a.ty * RATES.zoom * k),
  };
}

/** Does this motion move the camera at all? */
export const moves = (n: Nav) => n.yaw !== 0 || n.pitch !== 0 || n.panRight !== 0 || n.panUp !== 0 || n.dolly !== 1;

/** What a button does. Button 0 (the left one) frames the whole model; button 1 (the right) goes to the iso view. */
export type ButtonAction = "fit" | "iso";
const BUTTONS: Record<number, ButtonAction> = { 0: "fit", 1: "iso" };

/** The actions for buttons that went down between two frames' button bits. */
export function pressed(before: number, now: number): ButtonAction[] {
  const out: ButtonAction[] = [];
  for (const [bit, action] of Object.entries(BUTTONS)) if ((now & ~before) & (1 << Number(bit))) out.push(action);
  return out;
}

/** What the shell says about the device. */
export interface SpaceMouseStatus {
  connected: boolean;
  name: string;
  via: string;
}
export const NO_DEVICE: SpaceMouseStatus = { connected: false, name: "", via: "" };

/** Where the device's frames come from: the desktop shell in the app, a script in tests, nothing in a browser. */
export interface SpaceMouseTransport {
  /** start (or stop) reading the device; `on` mirrors the "use a 3D mouse" setting */
  enable(on: boolean): void;
  onFrame(cb: (f: SpaceMouseFrame) => void): () => void;
  onStatus(cb: (s: SpaceMouseStatus) => void): () => void;
}

/** A frame from the shell, checked: wrong shapes and non-numbers become "nothing happening". */
export function readFrame(raw: unknown): SpaceMouseFrame {
  const o = (raw ?? {}) as Record<string, unknown>;
  const three = (v: unknown): [number, number, number] => {
    const a = Array.isArray(v) ? v : [];
    const n = (i: number) => (typeof a[i] === "number" && Number.isFinite(a[i]) ? (a[i] as number) : 0);
    return [n(0), n(1), n(2)];
  };
  return { t: three(o.t), r: three(o.r), buttons: typeof o.buttons === "number" && Number.isInteger(o.buttons) && o.buttons >= 0 ? o.buttons : 0 };
}

export interface DriverHooks {
  /** move the camera */
  apply(n: Nav): void;
  /** a button was pressed */
  button(a: ButtonAction): void;
  /** call `fn` before the next picture; returns a way to cancel (requestAnimationFrame in the app) */
  schedule(fn: (nowMs: number) => void): () => void;
  settings(): SpaceMouseSettings;
}

/** Runs the camera while the cap is held. It only asks for frames while something is moving, so an idle (or
 * missing) device costs nothing. */
export class SpaceMouseDriver {
  private frame: SpaceMouseFrame = { t: [0, 0, 0], r: [0, 0, 0], buttons: 0 };
  private last: number | null = null;
  private cancel: (() => void) | null = null;
  private off: (() => void)[] = [];

  constructor(private transport: SpaceMouseTransport, private hooks: DriverHooks) {
    this.off.push(transport.onFrame((f) => this.feed(f)));
    this.sync();
  }

  /** The setting changed: tell the shell, and drop whatever was held if it was turned off. */
  sync() {
    const on = this.hooks.settings().enabled;
    this.transport.enable(on);
    if (!on) this.feed({ t: [0, 0, 0], r: [0, 0, 0], buttons: 0 });
  }

  feed(f: SpaceMouseFrame) {
    const s = this.hooks.settings();
    if (!s.enabled) f = { t: [0, 0, 0], r: [0, 0, 0], buttons: 0 };
    for (const a of pressed(this.frame.buttons, f.buttons)) this.hooks.button(a);
    this.frame = f;
    if (!this.cancel && moves(navFor(f, s, 1))) this.cancel = this.hooks.schedule(this.tick);
  }

  private tick = (now: number) => {
    this.cancel = null;
    const dt = this.last === null ? 1 / 60 : clamp((now - this.last) / 1000, 0, 0.05);
    this.last = now;
    const n = navFor(this.frame, this.hooks.settings(), dt);
    if (!moves(n)) {
      this.last = null;
      return;
    }
    this.hooks.apply(n);
    this.cancel = this.hooks.schedule(this.tick);
  };

  dispose() {
    this.cancel?.();
    this.cancel = null;
    for (const o of this.off) o();
    this.off = [];
  }
}
