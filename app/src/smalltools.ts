// Tools > Make faces and Flatten to height…: what to ask the engine for, and the toast that says what it did.
// The engine's `make_faces` and `flatten` commands do the work (core/src/makefaces.rs, core/src/flatten.rs,
// docs/dev/small-tools.md); this is only the words. The Curve tool's maths is in curvemath.ts.
import type { Sel } from "./selection";

type Tr = (id: string) => string;
type Reply = Record<string, unknown>;

const count = (v: unknown) => (typeof v === "number" ? v : Array.isArray(v) ? v.length : 0);

function list(parts: { id: string; n: number }[], t: Tr): string {
  return parts.filter((p) => p.n > 0).map((p) => t(p.id).replace("{n}", String(p.n))).join(", ");
}

// ---- make faces ----------------------------------------------------------------------------------------------------

/** The lines and groups picked (faces and component placements are not a source of loose lines); none: every loose line. */
export function makeFacesTargets(sel: Sel): number[] {
  return [...sel.edges, ...sel.groups];
}

export function makeFacesCommand(sel: Sel, openHoles = false): Record<string, unknown> {
  const ids = makeFacesTargets(sel);
  return { cmd: "make_faces", ...(ids.length ? { ids } : {}), ...(openHoles ? { open_holes: true } : {}) };
}

/** What to tell the person after `make_faces`: counts only (the engine's reply has the ids and places). */
export function makeFacesSummary(r: Reply, t: Tr): { text: string; bad: boolean } {
  const made = list(
    [
      { id: "makefaces.cat.created", n: count(r.created) },
      { id: "makefaces.cat.with_holes", n: count(r.with_holes) },
      { id: "makefaces.cat.split", n: count(r.split) },
    ],
    t
  );
  const left = list(
    [
      { id: "makefaces.left.non_planar", n: count(r.non_planar) },
      { id: "makefaces.left.degenerate", n: count(r.degenerate) },
      { id: "makefaces.left.open_edges", n: count(r.open_edges) },
    ],
    t
  );
  const bent = count(r.non_planar) > 0;
  if (!count(r.created) && !count(r.split)) {
    if (count(r.existing) > 0 && !left) return { text: t("makefaces.already"), bad: false };
    const why = left ? ` ${t("makefaces.left").replace("{list}", left)}` : "";
    return { text: `${t("makefaces.none")}${why}${bent ? ` ${t("makefaces.flattenFirst")}` : ""}`, bad: bent };
  }
  const said = t("makefaces.made").replace("{list}", made);
  return left ? { text: `${said} ${t("makefaces.left").replace("{list}", left)}${bent ? ` ${t("makefaces.flattenFirst")}` : ""}`, bad: bent } : { text: said, bad: false };
}

// ---- flatten --------------------------------------------------------------------------------------------------------

/** What Flatten works on: the lines, faces and groups picked (a placed component is opened for editing first). */
export function flattenTargets(sel: Sel): number[] {
  return [...sel.edges, ...sel.faces, ...sel.groups];
}

export function flattenCommand(sel: Sel, z: number): Record<string, unknown> {
  return { cmd: "flatten", ids: flattenTargets(sel), z };
}

/** The height typed, in the model's units, as meters; null when it is not a height (zero and negative are heights). */
export function flattenHeight(raw: string | null, parse: (text: string) => number): number | null {
  if (raw == null) return null;
  const z = parse(raw);
  return Number.isFinite(z) ? z : null;
}

export function flattenSummary(r: Reply, height: string, t: Tr): { text: string; bad: boolean } {
  const parts = list(
    [
      { id: "flatten.cat.moved", n: count(r.moved) },
      { id: "flatten.cat.merged", n: count(r.merged) },
      { id: "flatten.cat.edges_removed", n: count(r.edges_removed) },
      { id: "flatten.cat.faces_removed", n: count(r.faces_removed) },
      { id: "flatten.cat.faces_turned", n: count(r.faces_turned) },
    ],
    t
  );
  if (!parts) return { text: t("flatten.same"), bad: false };
  return { text: t("flatten.done").replace("{z}", height).replace("{list}", parts), bad: false };
}
