// Hatch preview swatches: a pattern's line families laid over a small rectangle, the way the engine's generator
// (core/src/draft/hatch.rs) lays them over a cut region, so the swatch in the Tags panel looks like the print.
// Pattern sizes are millimetres; the swatch shows them at `pxPerMm` (paper scale), times the hatch's own scale.

/** One family of parallel lines, as `hatch_patterns` lists it (mm, degrees; delta is along, then across). */
export interface HatchFamily {
  angle: number;
  origin: [number, number];
  delta: [number, number];
  dash: number[];
}

export interface HatchPatternInfo {
  name: string;
  desc: string;
  builtin: boolean;
  families: HatchFamily[];
}

/** A tag's hatch, as the scene's `tagStyles[tag].cut.hatch` holds it. */
export interface HatchUseInfo {
  pattern: string;
  color: string;
  weight_mm: number;
  scale: number;
  angle: number;
  scale_mode: "paper" | "model";
}

/** Most segments a swatch draws (a very fine pattern shows as a tint instead). */
export const SWATCH_MAX = 4000;

/**
 * The lines of a pattern over a `w` x `h` rectangle (pixels, y down), as SVG path data, or null when the pattern
 * is too fine to draw there (show a tint). `scale` and `angle` are the hatch's; the swatch shows paper scale.
 */
export function swatchPath(families: HatchFamily[], w: number, h: number, pxPerMm: number, scale = 1, angle = 0): string | null {
  const k = pxPerMm * (scale > 0 && Number.isFinite(scale) ? scale : 1);
  const rad = ((Number.isFinite(angle) ? angle : 0) * Math.PI) / 180;
  const [rs, rc] = [Math.sin(rad), Math.cos(rad)];
  const rot = (x: number, y: number): [number, number] => [x * rc - y * rs, x * rs + y * rc];
  const out: string[] = [];
  // y up inside, like the pattern; flipped when written
  const corners: [number, number][] = [
    [0, 0],
    [w, 0],
    [w, h],
    [0, h],
  ];
  const edges = corners.map((a, i) => [a, corners[(i + 1) % 4]] as const);
  const n2 = (v: number) => Math.round(v * 100) / 100;
  for (const f of families) {
    const fa = (f.angle * Math.PI) / 180;
    const [fs, fc] = [Math.sin(fa), Math.cos(fa)];
    const step = rot((f.delta[0] * fc - f.delta[1] * fs) * k, (f.delta[0] * fs + f.delta[1] * fc) * k);
    const base = rot(f.origin[0] * k, f.origin[1] * k);
    const a = fa + rad;
    const d: [number, number] = [Math.cos(a), Math.sin(a)];
    const nrm: [number, number] = [-Math.sin(a), Math.cos(a)];
    const spacing = step[0] * nrm[0] + step[1] * nrm[1];
    if (Math.abs(spacing) < 0.6) return null; // closer than a pixel or so
    const across = corners.map(([x, y]) => ((x - base[0]) * nrm[0] + (y - base[1]) * nrm[1]) / spacing);
    const i0 = Math.ceil(Math.min(...across));
    const i1 = Math.floor(Math.max(...across));
    const dash = f.dash.map((x) => x * k);
    const period = dash.reduce((s, x) => s + Math.abs(x), 0);
    if (dash.length && period < 0.5) return null;
    for (let i = i0; i <= i1; i++) {
      const p: [number, number] = [base[0] + step[0] * i, base[1] + step[1] * i];
      const ts: number[] = [];
      for (const [e0, e1] of edges) {
        const da = (e0[0] - p[0]) * nrm[0] + (e0[1] - p[1]) * nrm[1];
        const db = (e1[0] - p[0]) * nrm[0] + (e1[1] - p[1]) * nrm[1];
        if (da > 0 !== db > 0) {
          const t = da / (da - db);
          const x = [e0[0] + (e1[0] - e0[0]) * t, e0[1] + (e1[1] - e0[1]) * t];
          ts.push((x[0] - p[0]) * d[0] + (x[1] - p[1]) * d[1]);
        }
      }
      if (ts.length < 2) continue;
      ts.sort((x, y) => x - y);
      const at = (t: number) => `${n2(p[0] + d[0] * t)} ${n2(h - (p[1] + d[1] * t))}`;
      const seg = (s: number, e: number) => out.push(`M${at(s)}L${at(e)}`);
      for (let j = 0; j + 1 < ts.length; j += 2) {
        const [t0, t1] = [ts[j], ts[j + 1]];
        if (!dash.length) seg(t0, t1);
        else {
          // the dashes start at the line's own origin, as on the sheet
          let pos = Math.floor(t0 / period) * period;
          walk: while (pos <= t1) {
            for (const dl of dash) {
              const len = Math.abs(dl);
              if (dl >= 0) {
                const s = Math.max(pos, t0);
                const e = Math.min(pos + len, t1);
                if (dl === 0 ? pos >= t0 && pos <= t1 : e > s) seg(s, dl === 0 ? s : e);
              }
              pos += len;
              if (pos > t1) break walk;
            }
            if (out.length > SWATCH_MAX) return null;
          }
        }
        if (out.length > SWATCH_MAX) return null;
      }
    }
  }
  return out.join("");
}

/** What a hatch is, in words, for a tooltip or a screen reader. */
export function hatchLabel(h: HatchUseInfo | undefined | null): string {
  if (!h) return "No hatch";
  const bits = [h.pattern];
  if (h.scale !== 1) bits.push(`x${h.scale}`);
  if (h.angle) bits.push(`${h.angle}°`);
  bits.push(`${h.weight_mm} mm`);
  if (h.scale_mode === "model") bits.push("model scale");
  return bits.join(", ");
}

/**
 * The name an imported pattern is kept under: its own, unless a built-in has it (built-in names are taken), then
 * with "-file" added (names are at most 63 characters).
 */
export function importName(name: string, builtins: string[]): string {
  const taken = builtins.some((b) => b.toLowerCase() === name.toLowerCase());
  return taken ? `${name.slice(0, 58)}-file` : name;
}

/** The lines of one pattern (its header and what follows, up to the next header) out of a .pat text, with any
 * `;%UNITS=` line in force before it kept so the units stay right. */
export function patOnly(text: string, name: string): string {
  const lines = text.split(/\r?\n/);
  let units = "";
  const out: string[] = [];
  let inside = false;
  for (const l of lines) {
    const t = l.trim();
    if (/^;\s*%\s*UNITS\s*=/i.test(t)) {
      if (!inside) units = t;
      continue;
    }
    if (t.startsWith("*")) {
      if (inside) break;
      const n = t.slice(1).split(",")[0].trim();
      if (n.toLowerCase() === name.toLowerCase()) {
        inside = true;
        out.push(t);
      }
      continue;
    }
    if (inside) out.push(l);
  }
  return out.length ? [units, ...out].filter(Boolean).join("\n") + "\n" : "";
}
