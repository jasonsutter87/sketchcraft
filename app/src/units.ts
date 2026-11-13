// Display units. The engine always works in meters; this formats lengths for
// labels and parses what you type: 2.5, 2.5m, 250cm, 2500mm, 8', 8ft, 96",
// 96in, 10'6", 10' 6 1/2". A bare number means the current unit.

export type Units = "m" | "cm" | "mm" | "ft" | "in";

export const UNIT_OPTIONS: { id: Units; label: string }[] = [
  { id: "m", label: "Meters" },
  { id: "cm", label: "Centimeters" },
  { id: "mm", label: "Millimeters" },
  { id: "ft", label: "Feet & inches" },
  { id: "in", label: "Inches" },
];

const INCH = 0.0254;

function frac(x: number): string {
  const e = Math.round(Math.abs(x) * 8);
  const whole = Math.floor(e / 8);
  let num = e % 8;
  let den = 8;
  while (num > 0 && num % 2 === 0) {
    num /= 2;
    den /= 2;
  }
  const sign = x < 0 ? "-" : "";
  if (num === 0) return `${sign}${whole}`;
  if (whole === 0) return `${sign}${num}/${den}`;
  return `${sign}${whole} ${num}/${den}`;
}

export function formatLength(m: number, units: Units | string): string {
  switch (units) {
    case "cm":
      return `${(m * 100).toFixed(1)} cm`;
    case "mm":
      return `${Math.round(m * 1000)} mm`;
    case "in":
      return `${frac(m / INCH)}"`;
    case "ft": {
      const total = Math.round((Math.abs(m) / INCH) * 8) / 8;
      const feet = Math.floor(total / 12);
      const inches = total - feet * 12;
      const sign = m < 0 ? "-" : "";
      return feet === 0 ? `${sign}${frac(inches)}"` : `${sign}${feet}' ${frac(inches)}"`;
    }
    default:
      return `${m.toFixed(2)} m`;
  }
}

/** One typed length (no commas) to meters, or NaN. */
export function parseLength(raw: string, units: Units | string): number {
  const s = raw.trim().toLowerCase();
  if (!s) return NaN;
  const num = (t: string) => {
    // "6 1/2" or "1/2" or "6.5"
    const m = /^(-?\d+(?:\.\d+)?)?\s*(?:(\d+)\/(\d+))?$/.exec(t.trim());
    if (!m || (m[1] === undefined && m[2] === undefined)) return NaN;
    const whole = m[1] !== undefined ? Number(m[1]) : 0;
    const f = m[2] !== undefined ? Number(m[2]) / Number(m[3]) : 0;
    return whole < 0 ? whole - f : whole + f;
  };
  // feet and inches: 10'6", 10' 6 1/2", 10ft 6in
  const fi = /^(-?[\d.]+)\s*(?:'|ft|feet)\s*(?:([\d./ ]+)\s*(?:"|in|inch|inches)?)?$/.exec(s);
  if (fi) {
    const ft = Number(fi[1]);
    const inch = fi[2] ? num(fi[2]) : 0;
    return (ft * 12 + (ft < 0 ? -inch : inch)) * INCH;
  }
  const unit = /^([-\d./ ]+?)\s*(mm|cm|m|"|in|inch|inches)$/.exec(s);
  if (unit) {
    const v = num(unit[1]);
    const k: Record<string, number> = { mm: 0.001, cm: 0.01, m: 1, '"': INCH, in: INCH, inch: INCH, inches: INCH };
    return v * k[unit[2]];
  }
  const v = num(s);
  const base: Record<string, number> = { m: 1, cm: 0.01, mm: 0.001, ft: 12 * INCH, in: INCH };
  return v * (base[units] ?? 1);
}

/** Split typed text into lengths: "8,12", "8' 12'", "10'6\" 12'". */
export function parseLengths(text: string, units: Units | string): number[] {
  const parts = text.includes(",") || text.includes(";") ? text.split(/[,;]/) : splitSpaces(text);
  return parts.map((p) => parseLength(p, units)).filter((x) => !Number.isNaN(x));
}

function splitSpaces(text: string): string[] {
  // keep "10' 6 1/2\"" together: split only where a new number follows a unit mark
  return text.trim().split(/(?<=['"a-z])\s+(?=-?\d)/i);
}
