// What to tell the person when groups in a file, a restored backup or a library part had to be repaired as it came in:
// groups that sat inside each other in a loop, and groups nested more than the limit's levels deep. The engine reports the
// facts (counts and the first few names); the words are here, so they come out in the person's language.

/** How deep groups may nest (`MAX_GROUP_DEPTH` in core). */
export const MAX_GROUP_DEPTH = 256;

export interface GroupRepairInfo {
  loops: number;
  flattened: number;
  loopNames: string[];
  deepNames: string[];
}

const names = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
const count = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) && v > 0 ? Math.floor(v) : 0);

/** The repair a reply (an open, a restore, an inserted file) or a scene's `notice_repair` carries, if it carries one. */
export function groupRepairOf(r: Record<string, unknown> | null | undefined): GroupRepairInfo | null {
  if (!r) return null;
  const info = { loops: count(r.group_loops_repaired), flattened: count(r.groups_flattened), loopNames: names(r.group_loop_names), deepNames: names(r.groups_flattened_names) };
  return info.loops || info.flattened ? info : null;
}

/** `text` with each {hole} replaced, as plain text (a group's name may hold anything, `$&` included). */
const fill = (text: string, vars: Record<string, string>): string => text.replace(/\{(\w+)\}/g, (hole, k: string) => (Object.hasOwn(vars, k) ? vars[k] : hole));

/** " (A, B and 3 more)": the names the engine sent, and how many more there were. */
function named(t: (id: string) => string, n: number, list: string[]): string {
  if (!list.length) return "";
  const shown = n > list.length ? fill(t("repair.more"), { names: list.join(", "), more: String(n - list.length) }) : list.join(", ");
  return fill(t("repair.names"), { names: shown });
}

/** The notice, translated. `what` is "file", "backup" or "part". */
export function groupRepairNotice(t: (id: string) => string, info: GroupRepairInfo, what: "file" | "backup" | "part" = "file"): string {
  const parts: string[] = [];
  if (info.loops) {
    parts.push(fill(t(info.loops === 1 ? "repair.loops.one" : "repair.loops.many"), { n: String(info.loops), names: named(t, info.loops, info.loopNames) }));
  }
  if (info.flattened) {
    parts.push(fill(t(info.flattened === 1 ? "repair.deep.one" : "repair.deep.many"), { n: String(info.flattened), max: String(MAX_GROUP_DEPTH), names: named(t, info.flattened, info.deepNames) }));
  }
  const joined = parts.length === 2 ? fill(t("repair.join"), { a: parts[0], b: parts[1] }) : parts[0];
  return fill(t("repair.notice"), { what: t(`repair.what.${what}`), parts: joined });
}
