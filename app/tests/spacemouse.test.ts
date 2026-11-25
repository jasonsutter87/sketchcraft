import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_SPACEMOUSE,
  FULL_SCALE,
  SPACEMOUSE_KEY,
  SpaceMouseDriver,
  axes,
  cleanSettings,
  loadSettings,
  moves,
  navFor,
  pressed,
  readFrame,
  shape,
  type Nav,
  type SpaceMouseFrame,
  type SpaceMouseSettings,
  type SpaceMouseStatus,
  type SpaceMouseTransport,
} from "../src/spacemouse";

const frame = (t: [number, number, number] = [0, 0, 0], r: [number, number, number] = [0, 0, 0], buttons = 0): SpaceMouseFrame => ({ t, r, buttons });
const S = (patch: Partial<SpaceMouseSettings> = {}): SpaceMouseSettings => ({ ...DEFAULT_SPACEMOUSE, ...patch, invert: { ...DEFAULT_SPACEMOUSE.invert, ...patch.invert } });

describe("shape", () => {
  it("ignores the dead zone, then grows from zero to one", () => {
    expect(shape(0, 0.1)).toBe(0);
    expect(shape(0.1 * FULL_SCALE, 0.1)).toBe(0);
    expect(shape(-0.05 * FULL_SCALE, 0.1)).toBe(0);
    expect(shape(FULL_SCALE, 0.1)).toBe(1);
    expect(shape(-FULL_SCALE, 0.1)).toBe(-1);
    expect(shape(10 * FULL_SCALE, 0.1)).toBe(1); // stronger than full is clamped
    const a = shape(0.4 * FULL_SCALE, 0.1);
    const b = shape(0.7 * FULL_SCALE, 0.1);
    expect(a).toBeGreaterThan(0);
    expect(b).toBeGreaterThan(a);
  });
  it("treats junk as nothing", () => {
    expect(shape(NaN, 0.1)).toBe(0);
    expect(shape(Infinity, 0.1)).toBe(0);
  });
});

describe("navFor", () => {
  const full = FULL_SCALE;
  it("does nothing while the cap rests, or inside the dead zone", () => {
    expect(moves(navFor(frame(), S(), 1))).toBe(false);
    expect(moves(navFor(frame([10, 10, 10], [10, 10, 10]), S(), 1))).toBe(false);
  });
  it("pans with push and lift, zooms with push and pull, orbits with tilt and twist", () => {
    expect(navFor(frame([full, 0, 0]), S(), 1).panRight).toBeGreaterThan(0);
    expect(navFor(frame([0, 0, -full]), S(), 1).panUp).toBeGreaterThan(0); // lifting the cap lifts the view
    expect(navFor(frame([0, -full, 0]), S(), 1).dolly).toBeLessThan(1); // pushed away: in
    expect(navFor(frame([0, full, 0]), S(), 1).dolly).toBeGreaterThan(1); // pulled back: out
    expect(navFor(frame(), S(), 1).dolly).toBe(1);
    expect(navFor(frame([0, 0, 0], [full, 0, 0]), S(), 1).pitch).toBeGreaterThan(0);
    expect(navFor(frame([0, 0, 0], [0, 0, full]), S(), 1).yaw).toBeGreaterThan(0);
    expect(moves(navFor(frame([0, 0, 0], [0, full, 0]), S(), 1))).toBe(false); // roll keeps the view upright
  });
  it("scales with time and speed", () => {
    const one = navFor(frame([full, 0, 0]), S(), 1).panRight;
    expect(navFor(frame([full, 0, 0]), S(), 0.5).panRight).toBeCloseTo(one / 2);
    expect(navFor(frame([full, 0, 0]), S({ speed: 2 }), 1).panRight).toBeCloseTo(one * 2);
    expect(navFor(frame([full, 0, 0]), S(), 0).panRight).toBe(0);
  });
  it("flips any axis", () => {
    const base = navFor(frame([full, -full, -full], [full, 0, full]), S(), 1);
    const flipped = navFor(frame([full, -full, -full], [full, 0, full]), S({ invert: { tx: true, ty: true, tz: true, rx: true, ry: false, rz: true } }), 1);
    expect(flipped.panRight).toBeCloseTo(-base.panRight);
    expect(flipped.panUp).toBeCloseTo(-base.panUp);
    expect(flipped.pitch).toBeCloseTo(-base.pitch);
    expect(flipped.yaw).toBeCloseTo(-base.yaw);
    expect(flipped.dolly).toBeCloseTo(1 / base.dolly);
  });
  it("lock rotation leaves pan and zoom; lock pan leaves orbit and zoom", () => {
    const f = frame([full, -full, -full], [full, 0, full]);
    const noRot = navFor(f, S({ lock: "rotation" }), 1);
    expect([noRot.yaw, noRot.pitch]).toEqual([0, 0]);
    expect(noRot.panRight).toBeGreaterThan(0);
    expect(noRot.panUp).toBeGreaterThan(0);
    expect(noRot.dolly).toBeLessThan(1);
    const noPan = navFor(f, S({ lock: "pan" }), 1);
    expect([noPan.panRight, noPan.panUp]).toEqual([0, 0]);
    expect(noPan.yaw).toBeGreaterThan(0);
    expect(noPan.pitch).toBeGreaterThan(0);
    expect(noPan.dolly).toBeLessThan(1);
  });
  it("with the strongest axis only, a slightly crooked push stays straight", () => {
    const f = frame([full, 0.4 * full, 0], [0, 0, 0.3 * full]);
    const n = navFor(f, S({ dominant: true }), 1);
    expect(n.panRight).toBeGreaterThan(0);
    expect(n.dolly).toBe(1);
    expect(n.yaw).toBe(0);
    expect(navFor(f, S({ dominant: false }), 1).dolly).not.toBe(1);
  });
  it("applies the speed and the dead zone it is given", () => {
    expect(axes(frame([0.2 * full, 0, 0]), S({ deadzone: 0.3 })).tx).toBe(0);
    expect(axes(frame([0.2 * full, 0, 0]), S({ deadzone: 0 })).tx).toBeGreaterThan(0);
  });
});

describe("buttons", () => {
  it("reports a press once, not while held or on release", () => {
    expect(pressed(0, 0b01)).toEqual(["fit"]);
    expect(pressed(0b01, 0b01)).toEqual([]);
    expect(pressed(0b01, 0)).toEqual([]);
    expect(pressed(0b01, 0b11)).toEqual(["iso"]);
    expect(pressed(0, 0b11)).toEqual(["fit", "iso"]);
    expect(pressed(0, 1 << 7)).toEqual([]); // a button with no action
  });
});

describe("settings", () => {
  let data: Map<string, string>;
  beforeEach(() => {
    data = new Map();
    (globalThis as { localStorage?: unknown }).localStorage = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
  });
  afterEach(() => {
    delete (globalThis as { localStorage?: unknown }).localStorage;
  });

  it("starts from the defaults, with the 3D mouse on", () => {
    expect(loadSettings()).toEqual(DEFAULT_SPACEMOUSE);
    expect(DEFAULT_SPACEMOUSE.enabled).toBe(true);
  });
  it("reads what was saved", () => {
    data.set(SPACEMOUSE_KEY, JSON.stringify({ enabled: false, speed: 2, deadzone: 0.2, dominant: true, lock: "pan", invert: { ty: true } }));
    expect(loadSettings()).toEqual(S({ enabled: false, speed: 2, deadzone: 0.2, dominant: true, lock: "pan", invert: { ty: true } as never }));
  });
  it("repairs bad values one at a time and survives garbage", () => {
    expect(cleanSettings({ speed: 99, deadzone: -1, lock: "sideways", enabled: "yes", invert: { tx: "no", rz: true } })).toEqual(S({ speed: 4, deadzone: 0, invert: { rz: true } as never }));
    expect(cleanSettings({ speed: NaN }).speed).toBe(1);
    for (const junk of [null, 5, "x", [], undefined]) expect(cleanSettings(junk)).toEqual(DEFAULT_SPACEMOUSE);
    data.set(SPACEMOUSE_KEY, "{not json");
    expect(loadSettings()).toEqual(DEFAULT_SPACEMOUSE);
  });
  it("reads frames from the shell defensively", () => {
    expect(readFrame({ t: [1, 2, 3], r: [4, 5, 6], buttons: 5 })).toEqual(frame([1, 2, 3], [4, 5, 6], 5));
    expect(readFrame({ t: [1, "x"], r: "no", buttons: -1 })).toEqual(frame([1, 0, 0], [0, 0, 0], 0));
    expect(readFrame(null)).toEqual(frame());
  });
});

describe("the driver, with a fake device and a hand-turned clock", () => {
  /** a transport the test pushes frames into; remembers what the driver asked of it */
  class Fake implements SpaceMouseTransport {
    enabled: boolean[] = [];
    private cb: ((f: SpaceMouseFrame) => void)[] = [];
    enable(on: boolean) {
      this.enabled.push(on);
    }
    onFrame(cb: (f: SpaceMouseFrame) => void) {
      this.cb.push(cb);
      return () => void (this.cb = this.cb.filter((c) => c !== cb));
    }
    onStatus(_cb: (s: SpaceMouseStatus) => void) {
      return () => {};
    }
    send(f: SpaceMouseFrame) {
      for (const c of this.cb) c(f);
    }
    listeners = () => this.cb.length;
  }

  function rig(settings: SpaceMouseSettings = S()) {
    const dev = new Fake();
    const moved: Nav[] = [];
    const buttons: string[] = [];
    let pending: ((now: number) => void) | null = null;
    let now = 0;
    let asked = 0;
    const cfg = { s: settings };
    const driver = new SpaceMouseDriver(dev, {
      apply: (n) => moved.push(n),
      button: (a) => buttons.push(a),
      schedule: (fn) => {
        asked++;
        pending = fn;
        return () => void (pending = null);
      },
      settings: () => cfg.s,
    });
    /** one picture, `ms` after the last */
    const frames = (n: number, ms = 16) => {
      for (let i = 0; i < n && pending; i++) {
        now += ms;
        const fn = pending;
        pending = null;
        fn(now);
      }
    };
    return { dev, driver, moved, buttons, frames, cfg, asked: () => asked, waiting: () => pending !== null };
  }

  it("tells the shell whether the setting is on, and asks for no pictures with nothing moving", () => {
    const r = rig();
    expect(r.dev.enabled).toEqual([true]);
    r.dev.send(frame());
    expect(r.waiting()).toBe(false);
    expect(r.asked()).toBe(0);
  });

  it("moves the camera every picture while the cap is held, and stops when it is let go", () => {
    const r = rig();
    r.dev.send(frame([FULL_SCALE, 0, 0]));
    r.frames(4);
    expect(r.moved).toHaveLength(4);
    expect(r.moved.every((n) => n.panRight > 0)).toBe(true);
    r.dev.send(frame());
    r.frames(3);
    expect(r.moved).toHaveLength(4 + 0); // the pending picture finds nothing to do and stops asking
    expect(r.waiting()).toBe(false);
  });

  it("moves the same distance however fast the pictures come", () => {
    const total = (ms: number, n: number) => {
      const r = rig();
      r.dev.send(frame([FULL_SCALE, 0, 0]));
      r.frames(n, ms);
      return r.moved.reduce((a, m) => a + m.panRight, 0);
    };
    // the first picture counts as 1/60 s; compare the rest of a second
    const slow = total(50, 21);
    const fast = total(10, 101);
    expect(slow).toBeCloseTo(fast, 1);
  });

  it("does not jump after a stall", () => {
    const r = rig();
    r.dev.send(frame([FULL_SCALE, 0, 0]));
    r.frames(1);
    r.frames(1, 5000); // the app froze for five seconds
    const stalled = r.moved[1].panRight;
    expect(stalled).toBeLessThanOrEqual(navFor(frame([FULL_SCALE, 0, 0]), S(), 0.05).panRight + 1e-12);
  });

  it("presses buttons once", () => {
    const r = rig();
    r.dev.send(frame([0, 0, 0], [0, 0, 0], 1));
    r.dev.send(frame([0, 0, 0], [0, 0, 0], 1));
    r.dev.send(frame([0, 0, 0], [0, 0, 0], 0));
    r.dev.send(frame([0, 0, 0], [0, 0, 0], 2));
    expect(r.buttons).toEqual(["fit", "iso"]);
  });

  it("reads the settings every time, so a change applies at once", () => {
    const r = rig();
    r.dev.send(frame([FULL_SCALE, 0, 0], [0, 0, FULL_SCALE]));
    r.frames(1);
    expect(r.moved[0].yaw).toBeGreaterThan(0);
    r.cfg.s = S({ lock: "rotation" });
    r.frames(1);
    expect(r.moved[1].yaw).toBe(0);
    expect(r.moved[1].panRight).toBeGreaterThan(0);
  });

  it("turned off, it ignores the device, drops what was held, and says so to the shell", () => {
    const r = rig();
    r.dev.send(frame([FULL_SCALE, 0, 0]));
    r.cfg.s = S({ enabled: false });
    r.driver.sync();
    expect(r.dev.enabled).toEqual([true, false]);
    r.frames(2);
    expect(r.moved).toHaveLength(0);
    r.dev.send(frame([FULL_SCALE, 0, 0], [0, 0, 0], 1));
    expect(r.buttons).toEqual([]);
    expect(r.waiting()).toBe(false);
  });

  it("lets go of the device on dispose", () => {
    const r = rig();
    r.dev.send(frame([FULL_SCALE, 0, 0]));
    r.driver.dispose();
    expect(r.waiting()).toBe(false);
    expect(r.dev.listeners()).toBe(0);
  });
});
