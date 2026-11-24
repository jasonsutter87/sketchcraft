// Tools > Fix model and Check model: what to ask the engine for, and the toast that says what it did.
// The engine's `fix_model` command does the work (core/src/fix.rs, docs/dev/fix-model.md); this is only the words.
import type { Sel } from "./selection";

/** the lists in a `fix_model` reply that hold changes, in the order they are said */
export const FIX_CHANGES = ["duplicate_faces", "internal_faces", "non_manifold_fins", "reversed_faces", "merged_faces", "filled_holes", "stray_edges"] as const;
/** the lists in a reply's `remaining` that say what is still wrong */
export const FIX_REMAINING = ["open_edges", "non_manifold_edges", "inconsistent_edges", "inward_shells"] as const;

type Spots = unknown[] | undefined;
export interface FixReply {
  dry_run?: boolean;
  changes?: number;
  /** loose lines removed beyond the ones listed in `stray_edges` (the reply lists at most 1,000) */
  more_stray_edges?: number;
  remaining?: { solid?: boolean; faces?: number } & Partial<Record<(typeof FIX_REMAINING)[number], Spots>>;
  [key: string]: unknown;
}

/** The groups and faces the person has picked (edges and component placements are not a scope); none: the whole model. */
export function fixTargets(sel: Sel): number[] {
  return [...sel.groups, ...sel.faces];
}

/** The command to send. */
export function fixCommand(sel: Sel, dryRun: boolean): Record<string, unknown> {
  const ids = fixTargets(sel);
  return { cmd: "fix_model", ...(ids.length ? { ids } : {}), ...(dryRun ? { dry_run: true } : {}) };
}

const count = (v: unknown) => (Array.isArray(v) ? v.length : 0);

/** How many of a kind were changed: the listed ones, plus (for loose lines) the ones the reply only counts. */
function changed(r: FixReply, k: (typeof FIX_CHANGES)[number]): number {
  const more = k === "stray_edges" && typeof r.more_stray_edges === "number" && r.more_stray_edges > 0 ? r.more_stray_edges : 0;
  return count(r[k]) + more;
}

function list(parts: { id: string; n: number }[], t: (id: string) => string): string {
  return parts.filter((p) => p.n > 0).map((p) => t(p.id).replace("{n}", String(p.n))).join(", ");
}

/** What to tell the person: one sentence, counts only (the full list with ids and places is in the engine's reply). */
export function fixSummary(r: FixReply, t: (id: string) => string): { text: string; bad: boolean } {
  const rem = r.remaining ?? {};
  const changes = list(FIX_CHANGES.map((k) => ({ id: `fix.cat.${k}`, n: changed(r, k) })), t);
  const left = list(FIX_REMAINING.map((k) => ({ id: `fix.rem.${k}`, n: count(rem[k]) })), t);
  if (!rem.faces && !changes) return { text: t("fix.empty"), bad: false };
  if (!changes) return left ? { text: t("fix.stuck").replace("{list}", left), bad: true } : { text: t("fix.clean"), bad: false };
  const said = t(r.dry_run ? "fix.would" : "fix.fixed").replace("{list}", changes);
  return left ? { text: `${said} ${t("fix.still").replace("{list}", left)}`, bad: true } : { text: said, bad: false };
}
