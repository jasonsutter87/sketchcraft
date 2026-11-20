// Right button: a click opens the context menu, a drag pans. Windows (high-DPI mice, precision touchpads,
// pointer capture) doesn't always give a clean, still press and release, so the decision is forgiving.

/** a press that moved less than this (CSS px) is a click */
export const CLICK_PX = 6;
/** a quick press (shorter than this) may move a little more and still be a click */
export const CLICK_MS = 350;
export const QUICK_PX = 12;
/** a second menu request this soon after the first (or after a pan) is the same gesture */
export const SAME_GESTURE_MS = 600;

/** Was this right press a click (open the menu) rather than a pan? */
export function isRightClick(distance: number, elapsedMs: number): boolean {
  return distance < CLICK_PX || (elapsedMs < CLICK_MS && distance < QUICK_PX);
}

/**
 * What to do with a `contextmenu` event the browser raised:
 * - "wait": the press is still down, so the release will decide
 * - "skip": our menu already opened for this gesture, or the gesture was a pan
 * - "open": nothing else handled it (a two-finger tap, a lost pointerup): open the menu now
 */
export function contextEventAction(s: { pressed: boolean; sinceMenuMs: number; sincePanMs: number }): "wait" | "skip" | "open" {
  if (s.pressed) return "wait";
  if (s.sinceMenuMs < SAME_GESTURE_MS || s.sincePanMs < SAME_GESTURE_MS) return "skip";
  return "open";
}

/** Is this key the keyboard's way to the context menu (the Menu key, or Shift+F10)? */
export function isMenuKey(e: { key: string; shiftKey: boolean; ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean }): boolean {
  return e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey);
}
