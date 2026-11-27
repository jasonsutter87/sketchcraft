/** A roof pitch typed as degrees ("30", "30°") or rise/run ("6/12"), to a tenth of a degree; null when it isn't one
 * from 0 up to (not including) 80°, or nothing was typed. */
export function parsePitch(text: string): number | null {
  const t = text.trim().replace(/°$/, "").trim();
  if (!t) return null;
  const ratio = /^(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)$/.exec(t);
  const deg = ratio ? (Math.atan(Number(ratio[1]) / Number(ratio[2])) * 180) / Math.PI : Number(t);
  return Number.isFinite(deg) && deg >= 0 && deg < 80 ? Math.round(deg * 10) / 10 : null;
}
