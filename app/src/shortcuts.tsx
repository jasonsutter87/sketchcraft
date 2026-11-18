// The keyboard shortcuts sheet (?): your own keys, laid out by kind, and a way into the editor.
import { Sheet } from "./sheet";
import { COMMANDS, PRESETS, formatCombo, type CommandDef, type CommandGroup } from "./keymap";
import { keymap, useKeymap } from "./keymapstore";

/** the sheet's sections, each made of command groups */
const SECTIONS: [string, CommandGroup[]][] = [
  ["Tools", ["Tools"]],
  ["Camera", ["Camera"]],
  ["Edit and file", ["Edit", "File"]],
  ["View and windows", ["View", "Window", "Plugins"]],
];

/** What you do while using a tool: these are fixed, not shortcuts you can change. */
const WHILE_USING: [string, [string, string][]][] = [
  [
    "While using a tool",
    [
      ["Type + Enter", "Exact length, size, angle, or 24s for sides"],
      ["→  ←  ↑", "Lock to the red, green, blue axis"],
      ["Hold ⇧", "Lock the direction you're inferring"],
      ["Esc", "Cancel, or close the group you're in"],
      ["Option / Ctrl", "Move and Rotate: copy"],
      ["x5 after a copy", "Make 5 copies in a row"],
      ["Double-click", "Push/Pull: repeat the last distance; Select: open a group"],
      ["Hold Space", "Orbit while it's down; tap it to Select"],
    ],
  ],
  [
    "Selecting",
    [
      ["⇧ click", "Add or remove"],
      ["Option / Ctrl click", "Add"],
      ["⇧ Option click", "Remove"],
      ["⌘ / Option click", "Paint Bucket: sample a material"],
      ["⇧ click", "Paint Bucket: paint every matching face"],
      ["Option / Ctrl click", "Eraser: soften (⇧ too: unsoften)"],
      ["⇧ click", "Eraser: hide"],
    ],
  ],
];

export function ShortcutSheet({ onClose, onCustomize }: { onClose: () => void; onCustomize: () => void }) {
  const res = useKeymap();
  const all: CommandDef[] = [...COMMANDS, ...keymap.extra()];
  const preset = PRESETS.find((p) => p.id === keymap.state().preset)!;
  const changed = Object.keys(keymap.state().overrides).length;
  return (
    <Sheet onClose={onClose} label="Keyboard shortcuts">
      <div className="sheet-head">
        <b>Keyboard shortcuts</b>
        <span className="muted">
          {preset.name} keys{changed ? `, ${changed} changed by you` : ""}.
        </span>
        <button className="btn small primary" onClick={onCustomize}>
          Customize…
        </button>
        <button className="btn small ghost" onClick={onClose}>
          Close
        </button>
      </div>
      <div className="sheet-cols">
        {SECTIONS.map(([title, groups]) => {
          const rows = all.filter((d) => groups.includes(d.group) && res.bindings[d.id]);
          if (!rows.length) return null;
          return (
            <div key={title} className="sheet-sec">
              <div className="section-title">{title}</div>
              {rows.map((d) => (
                <div key={d.id} className="sheet-row">
                  <kbd>{formatCombo(res.bindings[d.id])}</kbd>
                  <span>{d.label}</span>
                </div>
              ))}
            </div>
          );
        })}
        {WHILE_USING.map(([title, rows]) => (
          <div key={title} className="sheet-sec">
            <div className="section-title">{title}</div>
            {rows.map(([k, what]) => (
              <div key={k + what} className="sheet-row">
                <kbd>{k}</kbd>
                <span>{what}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </Sheet>
  );
}
