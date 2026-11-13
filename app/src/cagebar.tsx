// The SubD mode bar: what to do now, the component switch, and every cage
// operation as a button with its key, so nothing has to be memorized.

import type { CageEditor, CageUi } from "./cage";

const AXES = ["red", "green", "blue"];

export function CageBar({ ui, ed, onDone }: { ui: CageUi; ed: CageEditor; onDone: () => void }) {
  const hint =
    ui.tool === "loop"
      ? "Insert loop: click an edge where the new loop should run (Esc to stop)."
      : ui.selected
        ? `${ui.selected} ${ui.mode === "vertex" ? "point" : ui.mode}${ui.selected === 1 ? "" : "s"} selected: drag to move${ui.lock != null ? ` along ${AXES[ui.lock]}` : ""}${ui.symmetric ? ", the other side follows" : ""}.`
        : ui.mode === "vertex"
          ? "Click a point (or drag a box) and drag it to shape the surface."
          : ui.mode === "edge"
            ? "Click edges to move them, crease them (C), or start a loop (L)."
            : "Click faces to move, extrude (E) or delete them.";
  const btn = (label: string, k: string, on: boolean, act: () => void, title?: string, disabled = false) => (
    <button className={on ? "on" : ""} onClick={act} title={title ?? `${label} (${k})`} disabled={disabled}>
      {label} <kbd>{k}</kbd>
    </button>
  );
  return (
    <>
      <div className="coach subd-coach" aria-live="polite">
        <div className="coach-head">
          <b>SubD</b>
          <span className="muted small">
            {ui.points} points · {ui.faces} faces · level {ui.levels}
          </span>
        </div>
        <div className="coach-step">{hint}</div>
        <div className="coach-keys">
          <span>
            <kbd>→ ← ↑</kbd> lock to red / green / blue
          </span>
          <span>
            <kbd>⇧ click</kbd> add or remove
          </span>
          <span>
            <kbd>Middle-drag</kbd> orbit
          </span>
        </div>
      </div>
      <div className="ws-strip subd-bar" role="toolbar" aria-label="SubD">
        <span className="lbl">Pick</span>
        {btn("Points", "1", ui.mode === "vertex" && ui.tool === "select", () => ed.setMode("vertex"))}
        {btn("Edges", "2", ui.mode === "edge" && ui.tool === "select", () => ed.setMode("edge"))}
        {btn("Faces", "3", ui.mode === "face" && ui.tool === "select", () => ed.setMode("face"))}
        <span className="sep" />
        {btn("Loop", "L", ui.tool === "loop", () => ed.setTool(ui.tool === "loop" ? "select" : "loop"), "Insert an edge loop: click an edge")}
        {btn("Extrude", "E", false, () => void ed.extrude(), "Extrude the selected faces", ui.mode !== "face" || !ui.selected)}
        {btn("Crease", "C", false, () => void ed.crease(), "Crease (or uncrease) the selected edges: they stay sharp", ui.mode !== "edge" || !ui.selected)}
        {btn("Merge", "M", false, () => void ed.merge(), "Merge the selected points into one", ui.mode !== "vertex" || ui.selected < 2)}
        {btn("Delete", "⌫", false, () => void ed.remove(), "Delete the selected faces", ui.mode !== "face" || !ui.selected)}
        <span className="sep" />
        {btn(ui.symmetric ? "Symmetry on" : "Symmetry off", "", ui.symmetric, () => void ed.toggleSymmetry(), "Mirror every edit across the middle")}
        <label className="strip-select" title="Soft select: nearby points follow, less the further away (B toggles)">
          <span className="lbl">Soft</span>
          <input type="range" min={0} max={Math.max(0.01, ed.size() * 0.5)} step={ed.size() / 200 || 0.001} value={ui.soft} onChange={(e) => ed.setSoft(Number(e.target.value))} />
        </label>
        <label className="strip-select" title="How smooth the surface is (+ / -)">
          <span className="lbl">Level</span>
          <select value={ui.levels} onChange={(e) => void ed.setLevels(Number(e.target.value))}>
            {[0, 1, 2, 3, 4].map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
        </label>
        {btn(ui.display === "both" ? "Cage + smooth" : "Cage only", "Tab", false, () => ed.setDisplay(ui.display === "both" ? "cage" : "both"), "Show the smooth surface or just the cage")}
        <span className="sep" />
        <button className="on" onClick={onDone} title="Leave SubD mode (Enter)">
          Done <kbd>↵</kbd>
        </button>
      </div>
    </>
  );
}
