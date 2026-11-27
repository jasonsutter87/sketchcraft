// Smooth camera (#251): after an orbit or a pan the camera glides to a stop. Some people want it to stop
// the moment the button comes up. A preference of the person, not of a model: it is kept apart from the
// view style (which a saved scene carries), the way the tool coach's on / off is.

const KEY = "sketchcraft.glide";
const EVENT = "sketchcraft-glide";

/** on unless turned off */
export function glideOn(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export function setGlide(on: boolean) {
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    /* private window: it holds for this session only */
  }
  window.dispatchEvent(new CustomEvent<boolean>(EVENT, { detail: on }));
}

/** call `cb` whenever it is switched; returns the unsubscribe */
export function watchGlide(cb: (on: boolean) => void): () => void {
  const h = (e: Event) => cb((e as CustomEvent<boolean>).detail);
  window.addEventListener(EVENT, h);
  return () => window.removeEventListener(EVENT, h);
}
