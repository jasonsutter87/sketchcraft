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

export function toggle(s: Sel, kind: keyof Sel, id: number, additive: boolean): Sel {
  if (!additive) return { ...EMPTY, [kind]: [id] };
  const has = s[kind].includes(id);
  return { ...s, [kind]: has ? s[kind].filter((x) => x !== id) : [...s[kind], id] };
}

export function merge(a: Sel, b: Sel): Sel {
  const u = (x: number[], y: number[]) => [...new Set([...x, ...y])];
  return { faces: u(a.faces, b.faces), edges: u(a.edges, b.edges), groups: u(a.groups, b.groups), instances: u(a.instances, b.instances) };
}
