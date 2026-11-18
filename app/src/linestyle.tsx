// Line weight and line style pickers, for a tag (Outliner > Tags) and for selected edges (Entity Info).
// Weights are pen widths in paper millimetres: they print at that width on a sheet at any scale.

/** The pens a drawing offers (ISO 128 widths, plus the two thick ones). */
export const PEN_WEIGHTS = [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0];
export const LINE_STYLES: [string, string][] = [
  ["solid", "Solid"],
  ["dashed", "Dashed"],
  ["hidden", "Hidden"],
  ["center", "Center"],
];

export interface LineStyleValue {
  weight_mm?: number;
  line?: string;
}

/** What a weight reads as in a menu. */
export const penLabel = (w: number) => `${w} mm`;

/**
 * Two menus: weight and style. `value` is what is set now (nothing: the default or the tag's applies).
 * `mixed` shows "Mixed" when several things with different values are selected. Changing one sends only that one:
 * `{ weight_mm: 0 }` and `{ line: "" }` mean "back to the default".
 */
export function LineStyleControls({ value, mixed, label, none, onChange }: { value: LineStyleValue; mixed?: boolean; label: string; none: string; onChange: (c: { weight_mm?: number; line?: string }) => void }) {
  const w = value.weight_mm || undefined;
  return (
    <span className="linestyle">
      <select
        className="units-select"
        aria-label={`Line weight of ${label}`}
        title="Line weight: the pen width on paper, the same at any scale"
        value={mixed ? "mixed" : String(w ?? "")}
        onChange={(e) => e.target.value !== "mixed" && onChange({ weight_mm: e.target.value === "" ? 0 : Number(e.target.value) })}
      >
        {mixed && <option value="mixed">Mixed</option>}
        <option value="">{none}</option>
        {w != null && !PEN_WEIGHTS.includes(w) && <option value={String(w)}>{penLabel(w)}</option>}
        {PEN_WEIGHTS.map((p) => (
          <option key={p} value={String(p)}>
            {penLabel(p)}
          </option>
        ))}
      </select>
      <select
        className="units-select"
        aria-label={`Line style of ${label}`}
        title="Line style: solid, dashed, hidden or center"
        value={mixed ? "mixed" : value.line ?? ""}
        onChange={(e) => e.target.value !== "mixed" && onChange({ line: e.target.value })}
      >
        {mixed && <option value="mixed">Mixed</option>}
        <option value="">{none}</option>
        {LINE_STYLES.map(([k, name]) => (
          <option key={k} value={k}>
            {name}
          </option>
        ))}
      </select>
    </span>
  );
}
