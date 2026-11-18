// A tag's cut intersection priority (design §3.4, #88): where cut regions of different tags overlap in a plan or
// section (a wall into a slab, insulation inside a wall), the higher number keeps the overlap and the lower tag is
// trimmed back. Pure helpers for the Outliner's priority list (cutpriorityeditor.tsx); the engine does the geometry.

import type { TagStyleInfo } from "./engine";

export const PRIORITY_MAX = 999;

/** A tag's priority as the engine reads it (0 when unset). */
export function priorityOf(style: TagStyleInfo | undefined | null): number {
  const p = style?.cut?.priority;
  return typeof p === "number" && Number.isInteger(p) && p >= 0 && p <= PRIORITY_MAX ? p : 0;
}

/** Tag names compared the way the engine sorts them (by code unit), not by locale. */
const byName = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** The tags from the one that wins (top) to the one that loses; equal priorities in name order, as the engine breaks ties. */
export function priorityOrder(tags: string[], styles: Record<string, TagStyleInfo> | undefined): string[] {
  return [...tags].sort((a, b) => priorityOf(styles?.[b]) - priorityOf(styles?.[a]) || byName(a, b));
}

/** `order` with the tag at `from` moved to `to` (indexes clamp; out-of-range `from` changes nothing). */
export function moveTag(order: string[], from: number, to: number): string[] {
  if (from < 0 || from >= order.length) return order;
  const at = Math.max(0, Math.min(order.length - 1, to));
  const next = [...order];
  const [t] = next.splice(from, 1);
  next.splice(at, 0, t);
  return next;
}

/**
 * The priorities that make `order` true (top wins): the bottom tag keeps 0 (the default), each one up is a step
 * higher (10, or smaller when there are very many tags). Only tags whose number changes are listed.
 */
export function renumber(order: string[], styles: Record<string, TagStyleInfo> | undefined): Record<string, number> {
  const n = order.length;
  const step = n > 1 ? Math.max(1, Math.min(10, Math.floor(PRIORITY_MAX / (n - 1)))) : 0;
  const out: Record<string, number> = {};
  order.forEach((t, i) => {
    const want = (n - 1 - i) * step;
    if (priorityOf(styles?.[t]) !== want) out[t] = want;
  });
  return out;
}

/** A typed number as a priority: a whole number 0 to 999, else null (not accepted). */
export function parsePriority(text: string): number | null {
  const s = text.trim();
  if (!/^\d{1,3}$/.test(s)) return null;
  const n = Number(s);
  return n <= PRIORITY_MAX ? n : null;
}

/** The `tag_style` command that sets one tag's priority. */
export function priorityCommand(tag: string, priority: number): Record<string, unknown> {
  return { cmd: "tag_style", tag, cut: { priority } };
}

/** The tags that share a priority with another (their overlap goes to the first by name). */
export function tiedTags(order: string[], styles: Record<string, TagStyleInfo> | undefined): Set<string> {
  const seen = new Map<number, string[]>();
  for (const t of order) {
    const p = priorityOf(styles?.[t]);
    seen.set(p, [...(seen.get(p) ?? []), t]);
  }
  return new Set([...seen.values()].filter((g) => g.length > 1).flat());
}
