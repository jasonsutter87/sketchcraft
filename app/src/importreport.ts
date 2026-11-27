// After a SketchUp import: what came across and what did not, with why. The engine's `import_skp` reply carries a
// `report` (core/src/skp.rs, ImportReport); this turns it into the lines the report dialog shows.
import type { Reply } from "./engine";

/** One thing the import left out: what it was, how many (null: the file has some but the reader can't count them), and why. */
export interface ReportSkip {
  what: string;
  count: number | null;
  why: string;
  /** names or words from the file (data, shown as plain text) */
  items: string[];
}

export interface ImportReport {
  /** the SketchUp version that saved the file, as the file says it */
  version: string;
  /** counts of what came across, by kind */
  imported: Record<string, number>;
  skipped: ReportSkip[];
}

/** A report with its file's name, for the dialog. */
export interface NamedReport extends ImportReport {
  name: string;
}

/** Plain counts and strings only: anything else in a reply is ignored, so a odd reply can't break the dialog. */
export function parseReport(r: Reply): ImportReport | null {
  const raw = (r as { report?: unknown }).report;
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const imported: Record<string, number> = {};
  if (o.imported && typeof o.imported === "object") {
    for (const [k, v] of Object.entries(o.imported as Record<string, unknown>)) if (typeof v === "number" && Number.isFinite(v)) imported[k] = v;
  }
  const skipped: ReportSkip[] = [];
  if (Array.isArray(o.skipped)) {
    for (const s of o.skipped as unknown[]) {
      if (!s || typeof s !== "object") continue;
      const e = s as Record<string, unknown>;
      if (typeof e.what !== "string" || typeof e.why !== "string") continue;
      // the file's own words come wrapped as { untrusted_text: [...] } (core/src/policy.rs); a bare list is read too
      const wrapped = e.items && typeof e.items === "object" && !Array.isArray(e.items) ? (e.items as { untrusted_text?: unknown }).untrusted_text : e.items;
      const items = Array.isArray(wrapped) ? wrapped.filter((x): x is string => typeof x === "string") : [];
      skipped.push({ what: e.what, count: typeof e.count === "number" && Number.isFinite(e.count) ? e.count : null, why: e.why, items });
    }
  }
  return { version: typeof o.version === "string" ? o.version : "", imported, skipped };
}

const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** What came across, one line per kind that has something (a kind with none is left out). */
export function importedLines(rep: ImportReport): { key: string; text: string }[] {
  const n = (k: string) => rep.imported[k] ?? 0;
  const out: { key: string; text: string }[] = [];
  if (n("faces")) out.push({ key: "faces", text: `Geometry: ${plural(n("faces"), "face")}` });
  if (n("components")) out.push({ key: "components", text: `Components: ${plural(n("components"), "definition")}, ${n("placed")} placed in the model` });
  if (n("materials")) {
    const textured = n("textured_materials");
    const pics = n("textures");
    out.push({ key: "materials", text: `Materials: ${n("materials").toLocaleString()}${textured ? `, ${pics} of ${textured} textured with their picture` : ""}` });
  }
  if (n("dimensions")) out.push({ key: "dimensions", text: `Dimensions: ${n("dimensions")}${n("labels") ? ` (${plural(n("labels"), "label")})` : ""}` });
  else if (n("labels")) out.push({ key: "labels", text: `Labels: ${n("labels")}` });
  return out;
}

/** What was left out, one line each, with the reason. */
export function skippedLines(rep: ImportReport): { key: string; text: string; items: string[] }[] {
  return rep.skipped.map((s, i) => ({ key: `${i}-${s.what}`, text: `${s.count == null ? s.what : `${s.count.toLocaleString()} ${s.what}`}: ${s.why}`, items: s.items }));
}

/** Everything came across (nothing is listed as left out). */
export const isComplete = (rep: ImportReport) => rep.skipped.length === 0;

/** The short line for the status bar. */
export function reportHeadline(rep: ImportReport): string {
  if (isComplete(rep)) return "everything in the file came across";
  const n = rep.skipped.length;
  return `${n} kind${n === 1 ? "" : "s"} of thing left out`;
}

/** How often the report opens: after every SketchUp import, or only when something was left out. */
export type ReportWhen = "always" | "issues";
const KEY = "sketchcraft.importReport";

export function readReportWhen(): ReportWhen {
  try {
    if (localStorage.getItem(KEY) === "issues") return "issues";
  } catch {}
  return "always";
}

export function saveReportWhen(w: ReportWhen) {
  try {
    localStorage.setItem(KEY, w);
  } catch {}
}

/** Whether to open the report now. */
export const shouldShow = (rep: ImportReport, when: ReportWhen) => when === "always" || !isComplete(rep);
