// What's selected. Faces and edges are picked one by one; groups and
// component placements are picked as a whole (open them to pick inside).
export interface Sel {
  faces: number[];
  edges: number[];
  groups: number[];
  instances: number[];
}

export const EMPTY: Sel = { faces: [], edges: [], groups: [], instances: [] };

export const selIds = (s: Sel) => [...s.faces, ...s.edges, ...s.groups, ...s.instances];

export const isEmpty = (s: Sel) => selIds(s).length === 0;

/** How a click changes the selection, from the keys held (SketchUp's rules). */
export type SelMode = "replace" | "toggle" | "add" | "subtract";

/** Shift toggles; Option (Mac) or Ctrl (Windows) adds; both subtract. */
export function selMode(e: { shiftKey: boolean; altKey: boolean; ctrlKey: boolean }): SelMode {
  const add = e.altKey || e.ctrlKey;
  if (e.shiftKey && add) return "subtract";
  if (add) return "add";
  if (e.shiftKey) return "toggle";
  return "replace";
}

export function toggle(s: Sel, kind: keyof Sel, id: number, mode: SelMode | boolean): Sel {
  const m: SelMode = mode === true ? "toggle" : mode === false ? "replace" : mode;
  const has = s[kind].includes(id);
  if (m === "replace") return { ...EMPTY, [kind]: [id] };
  if (m === "add") return has ? s : { ...s, [kind]: [...s[kind], id] };
  if (m === "subtract") return { ...s, [kind]: s[kind].filter((x) => x !== id) };
  return { ...s, [kind]: has ? s[kind].filter((x) => x !== id) : [...s[kind], id] };
}

/** Apply a box (or other group) pick to the selection. */
export function combine(a: Sel, b: Sel, mode: SelMode): Sel {
  if (mode === "replace") return b;
  if (mode === "add") return merge(a, b);
  const minus = (x: number[], y: number[]) => x.filter((i) => !y.includes(i));
  if (mode === "subtract") return { faces: minus(a.faces, b.faces), edges: minus(a.edges, b.edges), groups: minus(a.groups, b.groups), instances: minus(a.instances, b.instances) };
  const xor = (x: number[], y: number[]) => [...minus(x, y), ...minus(y, x)];
  return { faces: xor(a.faces, b.faces), edges: xor(a.edges, b.edges), groups: xor(a.groups, b.groups), instances: xor(a.instances, b.instances) };
}

export function merge(a: Sel, b: Sel): Sel {
  const u = (x: number[], y: number[]) => [...new Set([...x, ...y])];
  return { faces: u(a.faces, b.faces), edges: u(a.edges, b.edges), groups: u(a.groups, b.groups), instances: u(a.instances, b.instances) };
}
