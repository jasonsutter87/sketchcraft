// A tag's cut fill (design §3.4): what fills its cut walls, slabs and furniture in plans and sections, on screen
// and on sheets. Pure helpers for the Outliner's Cut button (cutstyle.tsx) and the on-screen section cap.

import type { TagStyleInfo } from "./engine";

export type CutFillKind = "auto" | "solid" | "hatch" | "none";

/** The drawing's own poché colour (sheets), used for a solid fill with no colour of its own. */
export const CUT_SOLID = "#292b33";
/** The cut outline's pen when a tag sets none (paper mm): the drawing's heavy pen. */
export const CUT_OUTLINE_MM = 0.35;

/** The choices, in the order the editor shows them, with what each does. */
export const CUT_FILLS: { kind: CutFillKind; label: string; hint: string }[] = [
  { kind: "auto", label: "Default", hint: "The drawing's own dark poché" },
  { kind: "solid", label: "Solid", hint: "A solid colour of your choice" },
  { kind: "hatch", label: "Hatch", hint: "The tag's hatch pattern over white" },
  { kind: "none", label: "None", hint: "Left blank: only the outline is drawn" },
];

/** How a tag's cut is filled now: a hatch fill with no hatch set draws as the default, as the engine does. */
export function cutFillOf(style: TagStyleInfo | undefined | null): CutFillKind {
  const c = style?.cut;
  const f = c?.fill ?? "auto";
  if (f === "hatch" && !c?.hatch) return "auto";
  return f === "solid" || f === "hatch" || f === "none" ? f : "auto";
}

/** The fill colour a solid cut shows (its own, else the poché). */
export function cutColorOf(style: TagStyleInfo | undefined | null): string {
  const c = style?.cut?.color;
  return c && /^#[0-9a-f]{6}$/i.test(c) ? c : CUT_SOLID;
}

/** The cut outline weight, paper mm. */
export function cutOutlineOf(style: TagStyleInfo | undefined | null): number {
  const w = style?.cut?.outline_mm;
  return w && w > 0 ? w : CUT_OUTLINE_MM;
}

/**
 * The `tag_style` command that picks `kind` for `tag`. Hatch with no hatch yet starts with the diagonal pattern
 * (the engine refuses a hatch fill with nothing to hatch with).
 */
export function cutFillCommand(tag: string, kind: CutFillKind, style: TagStyleInfo | undefined | null): Record<string, unknown> {
  const cmd: Record<string, unknown> = { cmd: "tag_style", tag, cut: { fill: kind } };
  if (kind === "hatch" && !style?.cut?.hatch) cmd.hatch = { pattern: "diagonal" };
  return cmd;
}

/** A short description for the button's tooltip. */
export function cutLabel(style: TagStyleInfo | undefined | null): string {
  const k = cutFillOf(style);
  const fill = k === "solid" ? `solid ${cutColorOf(style)}` : k === "hatch" ? `hatch ${style?.cut?.hatch?.pattern ?? ""}`.trim() : k === "none" ? "none (outline only)" : "default poché";
  const w = style?.cut?.outline_mm;
  return `${fill}, outline ${w && w > 0 ? `${w} mm` : "default"}`;
}
