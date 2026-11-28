// Mouse wheel or trackpad (#232): a slow mouse notch on macOS (a few pixels straight down) used to be read as a
// trackpad swipe, so it orbited instead of zooming; a fast spin (bigger steps) zoomed.
import { describe, expect, it } from "vitest";
import { newWheelMemory, notch, STREAM_MS, wheelKind, type WheelMode } from "../src/viewport/wheel";

const ev = (deltaY: number, deltaX = 0, o: { ctrlKey?: boolean; shiftKey?: boolean; deltaMode?: number } = {}) => ({ deltaY, deltaX, ctrlKey: !!o.ctrlKey, shiftKey: !!o.shiftKey, deltaMode: o.deltaMode ?? 0 });

/** feed events at the given times; returns what each was read as */
function run(events: [number, ReturnType<typeof ev>][], mode: WheelMode = "auto", mem = newWheelMemory()) {
  return { kinds: events.map(([t, e]) => wheelKind(e, mode, t, mem)), mem };
}

describe("wheel: mouse or trackpad", () => {
  it("the reporter's case: after a fast spin, slow notches zoom too", () => {
    // a fast spin: accelerated steps, close together
    const spin: [number, ReturnType<typeof ev>][] = [[0, ev(48)], [20, ev(96)], [40, ev(120)]];
    // then slow notches, half a second apart, 4 px each (WebKit's smallest mouse step)
    const slow: [number, ReturnType<typeof ev>][] = [[1000, ev(4.000244140625)], [1500, ev(4.000244140625)], [2000, ev(-8.00048828125)]];
    const { kinds, mem } = run([...spin, ...slow]);
    expect(kinds).toEqual(["mouse", "mouse", "mouse", "mouse", "mouse", "mouse"]);
    expect(mem.sure).toBe("mouse");
  });

  it("remembers the mouse across launches: the first slow notch zooms", () => {
    expect(run([[0, ev(4)]], "auto", newWheelMemory("mouse")).kinds).toEqual(["mouse"]);
  });

  it("a mouse with line steps (Firefox, Windows) is a mouse", () => {
    const { kinds, mem } = run([[0, ev(3, 0, { deltaMode: 1 })]]);
    expect(kinds).toEqual(["mouse"]);
    expect(mem.sure).toBe("mouse");
  });

  it("with nothing known, a small straight step is a trackpad, as before", () => {
    expect(run([[0, ev(4)]]).kinds).toEqual(["trackpad"]);
  });

  it("a trackpad swipe orbits all the way, even its big steps", () => {
    const swipe: [number, ReturnType<typeof ev>][] = [[0, ev(2, 1)], [16, ev(12, 0)], [32, ev(55, 0)], [48, ev(80, 3)], [64, ev(20, 0)]];
    const { kinds, mem } = run(swipe);
    expect(kinds.every((k) => k === "trackpad")).toBe(true);
    expect(mem.sure).toBe("trackpad");
  });

  it("sideways movement turns a mouse-remembered stream into a swipe (a laptop's trackpad after the mouse)", () => {
    const { kinds, mem } = run([[0, ev(3)], [16, ev(4, 2)], [32, ev(5)]], "auto", newWheelMemory("mouse"));
    expect(kinds).toEqual(["mouse", "trackpad", "trackpad"]);
    expect(mem.sure).toBe("trackpad");
  });

  it("a fast spin stays a zoom: its small steps don't become an orbit", () => {
    expect(run([[0, ev(60)], [15, ev(8)], [30, ev(4)]]).kinds).toEqual(["mouse", "mouse", "mouse"]);
  });

  it("a gap longer than a stream starts a new reading", () => {
    const { kinds } = run([[0, ev(2, 1)], [STREAM_MS + 500, ev(80)]]);
    expect(kinds).toEqual(["trackpad", "mouse"]);
  });

  it("pinch is ctrl+wheel, whatever the setting", () => {
    for (const m of ["auto", "mouse", "trackpad"] as WheelMode[]) expect(run([[0, ev(3, 0, { ctrlKey: true })]], m).kinds).toEqual(["pinch"]);
  });

  it("Shift+wheel turned sideways is not taken as a swipe's sideways movement", () => {
    const mem = newWheelMemory("mouse");
    wheelKind(ev(0, 4, { shiftKey: true }), "auto", 0, mem);
    expect(mem.sure).toBe("mouse");
  });

  it("the setting decides outright", () => {
    expect(run([[0, ev(4)], [500, ev(2, 3)]], "mouse").kinds).toEqual(["mouse", "mouse"]);
    expect(run([[0, ev(120)], [500, ev(4)]], "trackpad").kinds).toEqual(["trackpad", "trackpad"]);
    // line steps can't come from a trackpad
    expect(run([[0, ev(3, 0, { deltaMode: 1 })]], "trackpad").kinds).toEqual(["mouse"]);
  });

  it("a notch zooms by its sign, also when Shift turned it sideways", () => {
    expect(notch({ deltaY: 4, deltaX: 0 })).toBe(1);
    expect(notch({ deltaY: -120, deltaX: 0 })).toBe(-1);
    expect(notch({ deltaY: 0, deltaX: -4 })).toBe(-1);
  });
});
