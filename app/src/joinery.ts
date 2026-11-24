// Tools > Joinery (dado, rabbet, tenon) and the CNC options of Cut plan: what to ask the engine for.
// The engine's `dado`, `rabbet`, `tenon` and `export_cnc` commands do the work (core/src/joinery.rs, core/src/cnc.rs,
// docs/dev/woodworker.md); this is the form's numbers, the commands and the words.
import type { Sel } from "./selection";
import { parseLength } from "./units";

export type JointKind = "dado" | "rabbet" | "tenon";

/** The two boards a joint needs: exactly two groups or components picked, nothing else. */
export function jointBoards(sel: Sel): [number, number] | null {
  const ids = [...sel.groups, ...sel.instances];
  return ids.length === 2 && !sel.faces.length && !sel.edges.length ? [ids[0], ids[1]] : null;
}

/** What the form shows for each joint: the fields, in the order they are asked. */
export const JOINT_FIELDS: Record<JointKind, string[]> = {
  dado: ["depth", "width", "clearance"],
  rabbet: ["depth", "width", "clearance"],
  tenon: ["length", "thickness", "shoulder", "clearance"],
};

export type JointForm = { cutFirst: boolean } & Record<string, string | boolean>;

/** A length typed by hand; blank is "let the engine choose"; NaN if it is not a length. */
function typed(text: string, units: string): number | undefined {
  const t = text.trim();
  if (!t) return undefined;
  const v = parseLength(t, units);
  return Number.isFinite(v) ? v : Number.NaN;
}

/** The command for a joint, or the field that cannot be sent. `boards` are [first picked, second picked]; the form says
 *  which of them is cut (the other fits into it). Lengths are in the model's units. */
export function jointCommand(kind: JointKind, boards: [number, number], form: JointForm, units: string): { cmd: Record<string, unknown> } | { error: string } {
  const cmd: Record<string, unknown> = { cmd: kind, ids: form.cutFirst ? boards : [boards[1], boards[0]] };
  for (const f of JOINT_FIELDS[kind]) {
    const v = typed(String(form[f] ?? ""), units);
    if (v === undefined) continue;
    // a clearance and a shoulder may be zero; the other sizes may not
    const mayBeZero = f === "clearance" || f === "shoulder";
    if (Number.isNaN(v) || v < 0 || (v === 0 && !mayBeZero)) return { error: f };
    cmd[f] = v;
  }
  return { cmd };
}

type Tr = (id: string) => string;

/** The toast after a joint: which one, and how much came off. */
export function jointSummary(r: Record<string, unknown>, t: Tr): string {
  const mm3 = typeof r.removed_volume === "number" ? Math.round(r.removed_volume * 1e9) : 0;
  const kind = String(r.joint ?? "");
  return t(`joint.done.${kind}`).replace("{cm3}", (mm3 / 1000).toFixed(1));
}

/** The tool diameter the form starts with: 6 mm, or 1/4 inch for an imperial model. */
export const defaultToolText = (units: "mm" | "in") => (units === "mm" ? "6" : "0.25");

/** The `export_cnc` command: the tool diameter typed in mm (or inches), a .zip path, the picked parts or all. */
export function cncCommand(sel: Sel, path: string, toolText: string, dogbones: boolean, units: "mm" | "in"): { cmd: Record<string, unknown> } | { error: string } {
  const t = toolText.trim();
  const tool = t ? parseLength(t, units) : Number.NaN;
  if (!Number.isFinite(tool) || tool < 0 || tool > 0.1) return { error: "tool" };
  const ids = [...sel.groups, ...sel.instances];
  return { cmd: { cmd: "export_cnc", path, ...(ids.length ? { ids } : {}), tool_diameter: tool, dogbones } };
}
