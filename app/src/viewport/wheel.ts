// What a wheel event over the 3D view came from: a trackpad's two-finger swipe (orbits; with Shift, pans), a pinch
// (zooms) or a mouse wheel (zooms). Browsers don't say, so it is read from the event and the ones before it (#232).
//
// A slow mouse-wheel notch on macOS arrives as a few pixels straight up or down, the same as the start of a
// trackpad swipe, so a lone small step can't tell. What can:
//  - a pinch is ctrl+wheel;
//  - line or page steps (deltaMode 1/2) only come from a wheel;
//  - sideways movement without Shift only comes from a trackpad (Shift turns a wheel sideways on macOS);
//  - a big step straight up or down (40 px or more) is a wheel notch, unless it is part of a swipe already going;
//  - events less than STREAM_MS apart belong to the same swipe or the same spin of the wheel.
// A small straight step that starts something new goes by the last device seen for certain, which is remembered
// across launches. With nothing seen yet it is a trackpad, as it always was.
// The View › Scroll wheel setting can say outright: "mouse" always zooms, "trackpad" always swipes.

export type WheelMode = "auto" | "mouse" | "trackpad";
export const WHEEL_MODES: readonly WheelMode[] = ["auto", "mouse", "trackpad"];
export type WheelKind = "pinch" | "trackpad" | "mouse";

/** how close (ms) wheel events must be to belong to the same swipe or spin */
export const STREAM_MS = 60;

export interface WheelMemory {
  /** when the last wheel event came, and what it was read as */
  at: number;
  kind: WheelKind | null;
  /** the last device seen for certain */
  sure: "trackpad" | "mouse" | null;
}

export const newWheelMemory = (sure: WheelMemory["sure"] = null): WheelMemory => ({ at: -Infinity, kind: null, sure });

type WheelLike = Pick<WheelEvent, "ctrlKey" | "shiftKey" | "deltaMode" | "deltaX" | "deltaY">;

/** Read one wheel event; updates `mem` (the caller persists `mem.sure` when it changes). */
export function wheelKind(e: WheelLike, mode: WheelMode, now: number, mem: WheelMemory): WheelKind {
  const kind = read(e, mode, now, mem);
  mem.at = now;
  mem.kind = kind;
  return kind;
}

function read(e: WheelLike, mode: WheelMode, now: number, mem: WheelMemory): WheelKind {
  if (e.ctrlKey) return "pinch"; // macOS reports a pinch as ctrl+wheel; ctrl+wheel on a mouse zooms too
  if (mode === "mouse") return "mouse";
  if (e.deltaMode !== 0) {
    // lines or pages: a wheel, whatever the setting says
    if (mode === "auto") mem.sure = "mouse";
    return "mouse";
  }
  if (mode === "trackpad") return "trackpad";
  const going = now - mem.at < STREAM_MS && mem.kind !== null && mem.kind !== "pinch" ? mem.kind : null;
  if (Math.abs(e.deltaX) > 0 && !e.shiftKey) {
    mem.sure = "trackpad";
    return "trackpad";
  }
  if (going) return going; // a swipe stays a swipe, a spin stays a spin
  if (Math.abs(e.deltaY) >= 40) {
    mem.sure = "mouse";
    return "mouse";
  }
  if (Math.abs(e.deltaX) > 0) return "trackpad"; // Shift+sideways, nothing going: a Shift+swipe pans, as before
  return mem.sure ?? "trackpad";
}

/** the step to zoom by for a mouse notch: up or down, also when Shift has turned the wheel sideways */
export const notch = (e: Pick<WheelEvent, "deltaX" | "deltaY">) => Math.sign(e.deltaY || e.deltaX);
