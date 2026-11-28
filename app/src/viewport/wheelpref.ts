// The Scroll wheel setting (View › Scroll wheel) and the device the wheel handler last saw for certain, both kept
// across launches (#232). See wheel.ts.
import { WHEEL_MODES, type WheelMemory, type WheelMode } from "./wheel";

export const WHEEL_MODE_KEY = "sketchcraft.wheel";
export const WHEEL_SEEN_KEY = "sketchcraft.wheelSeen";

export function readWheelMode(): WheelMode {
  try {
    const v = localStorage.getItem(WHEEL_MODE_KEY);
    return (WHEEL_MODES as readonly string[]).includes(v ?? "") ? (v as WheelMode) : "auto";
  } catch {
    return "auto";
  }
}

export function readWheelSeen(): WheelMemory["sure"] {
  try {
    const v = localStorage.getItem(WHEEL_SEEN_KEY);
    return v === "mouse" || v === "trackpad" ? v : null;
  } catch {
    return null;
  }
}
