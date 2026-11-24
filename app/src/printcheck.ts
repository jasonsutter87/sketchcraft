// Tools > Print check: what to ask the engine for, and the lines of the report.
// The engine's `print_check` command does the work (core/src/print.rs, docs/dev/print-check.md); this is the words.
import { FIX_REMAINING } from "./fixmodel";
import type { Sel } from "./selection";

type Spots = { id: number; at?: number[]; in_component?: boolean }[] | undefined;

export interface PrintReply {
  solid?: boolean;
  solid_check?: { faces?: number } & Partial<Record<(typeof FIX_REMAINING)[number], Spots>>;
  thin_walls?: { count?: number; min_wall_mm?: number; thinnest_mm?: number | null; items?: Spots };
  overhangs?: { count?: number; angle_deg?: number; items?: Spots };
  size_mm?: number[] | null;
  units_hint?: { kind: "too_small" | "too_large" } | null;
  fit?: { fits: boolean; fits_if_turned: boolean; over_mm: number[] } | null;
  ready?: boolean;
  [key: string]: unknown;
}

export interface PrintOptions {
  minWall: number;
  angle: number;
}
export const PRINT_DEFAULTS: PrintOptions = { minWall: 0.8, angle: 45 };

/** The groups and faces the person has picked; none: the whole model. */
export function printCommand(sel: Sel, o: PrintOptions): Record<string, unknown> {
  const ids = [...sel.groups, ...sel.faces];
  return { cmd: "print_check", ...(ids.length ? { ids } : {}), min_wall: o.minWall, overhang_angle: o.angle };
}

export type Line = { key: string; ok: boolean; warn?: boolean; text: string };

const n = (x: unknown) => (Array.isArray(x) ? x.length : 0);
const mm = (x: number) => String(Math.round(x * 100) / 100);

/** The report as lines: green ones are fine, `warn` ones are worth a look but do not stop a print, the rest need fixing. */
export function printLines(r: PrintReply, o: PrintOptions, t: (id: string) => string): Line[] {
  const fill = (id: string, v: Record<string, string>) => Object.entries(v).reduce((s, [k, x]) => s.replace(`{${k}}`, x), t(id));
  const out: Line[] = [];
  if (!r.size_mm) return [{ key: "empty", ok: false, text: t("print.empty") }];
  const [x, y, z] = r.size_mm;
  out.push({ key: "size", ok: true, text: fill("print.size", { x: mm(x), y: mm(y), z: mm(z) }) });
  if (r.units_hint) {
    const big = r.units_hint.kind === "too_large";
    out.push({ key: "units", ok: false, text: fill(big ? "print.units.large" : "print.units.small", { mm: mm(Math.max(x, y, z)), m: mm(Math.max(x, y, z) / 1000) }) });
  }
  if (r.solid) out.push({ key: "solid", ok: true, text: t("print.solid.ok") });
  else {
    const sc = r.solid_check ?? {};
    const left = FIX_REMAINING.map((k) => ({ k, c: n(sc[k]) }))
      .filter((p) => p.c > 0)
      .map((p) => t(`fix.rem.${p.k}`).replace("{n}", String(p.c)))
      .join(", ");
    out.push({ key: "solid", ok: false, text: fill("print.solid.bad", { list: left || t("print.solid.other") }) });
  }
  const thin = r.thin_walls?.count ?? 0;
  out.push(
    thin
      ? { key: "thin", ok: false, text: fill("print.thin.bad", { n: String(thin), mm: mm(o.minWall), t: mm(r.thin_walls?.thinnest_mm ?? 0) }) }
      : { key: "thin", ok: true, text: fill("print.thin.ok", { mm: mm(o.minWall) }) },
  );
  const over = r.overhangs?.count ?? 0;
  out.push(
    over
      ? { key: "over", ok: true, warn: true, text: fill("print.over.bad", { n: String(over), deg: mm(o.angle) }) }
      : { key: "over", ok: true, text: fill("print.over.ok", { deg: mm(o.angle) }) },
  );
  if (r.fit) {
    out.push(
      r.fit.fits
        ? { key: "fit", ok: true, text: t("print.fit.ok") }
        : { key: "fit", ok: false, text: fill("print.fit.bad", { x: mm(r.fit.over_mm[0]), y: mm(r.fit.over_mm[1]), z: mm(r.fit.over_mm[2]) }) + (r.fit.fits_if_turned ? ` ${t("print.fit.turn")}` : "") },
    );
  }
  return out;
}

/** What "Select ..." picks: root faces only (a face inside a component is named by its placement and cannot be picked here). */
export function problemSelection(r: PrintReply, kind: "thin" | "over" | "open"): Partial<Sel> {
  const faces = (s: Spots) => [...new Set((s ?? []).filter((i) => !i.in_component).map((i) => i.id))];
  if (kind === "thin") return { faces: faces(r.thin_walls?.items) };
  if (kind === "over") return { faces: faces(r.overhangs?.items) };
  return { edges: [...new Set((r.solid_check?.open_edges ?? []).map((i) => i.id).filter((id) => id > 0))] };
}
