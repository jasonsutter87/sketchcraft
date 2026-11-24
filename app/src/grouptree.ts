// The group hierarchy as the interface walks it. Every walk here is iterative (a loop and a list of what is still to
// visit, never a function calling itself once per level) and stops at MAX_OUTLINE_DEPTH, so a model nested hundreds,
// thousands or tens of thousands of levels deep (the engine refuses past 256, but a file, a plugin or another program
// may hand the window anything) draws as far as the cap and no further, instead of overflowing the stack and taking
// the whole window with it.

/** How many levels the window walks into (the engine's own limit is 256 levels, `MAX_GROUP_DEPTH` in core). */
export const MAX_OUTLINE_DEPTH = 256;

export interface TreeNode {
  id: number;
  kind: "groups" | "instances";
  name: string;
  hidden: boolean;
  locked: boolean;
  tag: string | null;
  faces: number;
  children: TreeNode[];
}

/** What the tree is built from: the scene's groups and placements, nothing more. */
export interface TreeScene {
  groups: { id: number; name: string; hidden: boolean; locked: boolean; tag: string | null; faces: number; parent: number | null }[];
  instances: { id: number; name: string; hidden: boolean; tag: string | null; parent: number | null }[];
}

export interface Row {
  node: TreeNode;
  /** 0 for a top-level row */
  depth: number;
  /** inside something hidden */
  parentHidden: boolean;
  /** the row has rows under it (open or not) */
  hasKids: boolean;
}

const byKindThenName = (a: TreeNode, b: TreeNode) => (a.kind === b.kind ? a.name.localeCompare(b.name, undefined, { numeric: true }) : a.kind === "groups" ? -1 : 1);

/** Groups and placements as a tree, by their parent links: groups first, then by name. Nodes whose parents come round to
 *  themselves (never in a model the engine made) hang off nothing and are left out. */
export function buildTree(scene: TreeScene): TreeNode[] {
  const nodes = new Map<number, TreeNode>();
  const parents = new Map<number, number | null>();
  for (const g of scene.groups) {
    nodes.set(g.id, { id: g.id, kind: "groups", name: g.name, hidden: g.hidden, locked: g.locked, tag: g.tag, faces: g.faces, children: [] });
    parents.set(g.id, g.parent);
  }
  for (const i of scene.instances) {
    nodes.set(i.id, { id: i.id, kind: "instances", name: i.name, hidden: i.hidden, locked: false, tag: i.tag, faces: 0, children: [] });
    parents.set(i.id, i.parent);
  }
  const roots: TreeNode[] = [];
  for (const n of nodes.values()) {
    const p = parents.get(n.id);
    const parent = p != null ? nodes.get(p) : undefined;
    (parent ? parent.children : roots).push(n);
  }
  roots.sort(byKindThenName);
  for (const n of nodes.values()) if (n.children.length > 1) n.children.sort(byKindThenName);
  return roots;
}

/** The ids of the rows a search keeps: those whose name or tag has `query` in it, and the groups they sit in. */
export function matching(roots: TreeNode[], query: string): Set<number> {
  const q = query.trim().toLowerCase();
  const keep = new Set<number>();
  if (!q) return keep;
  // parents before children, then settled from the bottom up
  const order: { node: TreeNode; parent: TreeNode | null }[] = [];
  const stack: { node: TreeNode; parent: TreeNode | null; depth: number }[] = roots.map((node) => ({ node, parent: null, depth: 0 }));
  while (stack.length) {
    const top = stack.pop()!;
    order.push(top);
    if (top.depth >= MAX_OUTLINE_DEPTH) continue;
    for (const c of top.node.children) stack.push({ node: c, parent: top.node, depth: top.depth + 1 });
  }
  for (let k = order.length - 1; k >= 0; k--) {
    const { node, parent } = order[k];
    if (keep.has(node.id) || node.name.toLowerCase().includes(q) || (node.tag ?? "").toLowerCase().includes(q)) {
      keep.add(node.id);
      if (parent) keep.add(parent.id);
    }
  }
  return keep;
}

/** The most rows the outliner draws at once: a model with a hundred thousand groups must not make a hundred thousand
 *  list items (the page stops answering); the rest are counted in a "more rows" line, and a search or a closed group
 *  brings them within reach. */
export const MAX_OUTLINE_ROWS = 2000;

/** The rows to draw, top to bottom: every row reached through open rows (all of them, when searching), at most
 *  MAX_OUTLINE_DEPTH levels down and `limit` rows in all. `only`: just these ids (a search's matches). `cut` counts the
 *  rows left below the depth cap, `more` the rows left over once `limit` rows are drawn. */
export function flatten(roots: TreeNode[], isOpen: (id: number) => boolean, only?: Set<number> | null, limit = MAX_OUTLINE_ROWS): { rows: Row[]; cut: number; more: number } {
  const rows: Row[] = [];
  let cut = 0;
  let more = 0;
  const stack: { node: TreeNode; depth: number; parentHidden: boolean }[] = [];
  for (let k = roots.length - 1; k >= 0; k--) if (!only || only.has(roots[k].id)) stack.push({ node: roots[k], depth: 0, parentHidden: false });
  while (stack.length) {
    if (rows.length >= limit) {
      // what is still waiting: the rows on the stack and, for the open ones, everything under them
      for (const waiting of stack) more += countOpen(waiting.node, waiting.depth, isOpen, only);
      break;
    }
    const { node, depth, parentHidden } = stack.pop()!;
    const kids = only ? node.children.filter((c) => only.has(c.id)) : node.children;
    rows.push({ node, depth, parentHidden, hasKids: kids.length > 0 });
    if (!kids.length || !isOpen(node.id)) continue;
    if (depth + 1 >= MAX_OUTLINE_DEPTH) {
      cut += countBelow(kids, only);
      continue;
    }
    const off = parentHidden || node.hidden;
    for (let k = kids.length - 1; k >= 0; k--) stack.push({ node: kids[k], depth: depth + 1, parentHidden: off });
  }
  return { rows, cut, more };
}

/** The rows `node` stands for, once the cap on rows is passed: itself, and what is under it while it is open (to the depth cap). */
function countOpen(node: TreeNode, depth: number, isOpen: (id: number) => boolean, only?: Set<number> | null): number {
  let n = 0;
  const stack = [{ node, depth }];
  while (stack.length) {
    const x = stack.pop()!;
    n++;
    if (x.depth + 1 >= MAX_OUTLINE_DEPTH || !isOpen(x.node.id)) continue;
    for (const c of x.node.children) if (!only || only.has(c.id)) stack.push({ node: c, depth: x.depth + 1 });
  }
  return n;
}

/** How many rows there are in these nodes and everything under them (no recursion). */
function countBelow(nodes: TreeNode[], only?: Set<number> | null): number {
  let n = 0;
  const stack = [...nodes];
  while (stack.length) {
    const x = stack.pop()!;
    n++;
    for (const c of x.children) if (!only || only.has(c.id)) stack.push(c);
  }
  return n;
}

/** Every group in `roots` and everything nested in them: one pass over the groups, however deep. */
export function descendants(groups: { id: number; parent: number | null }[], roots: Iterable<number>): Set<number> {
  const kids = new Map<number, number[]>();
  for (const g of groups) {
    if (g.parent == null) continue;
    const list = kids.get(g.parent);
    if (list) list.push(g.id);
    else kids.set(g.parent, [g.id]);
  }
  const out = new Set<number>(roots);
  const stack = [...out];
  while (stack.length) {
    for (const k of kids.get(stack.pop()!) ?? []) {
      if (!out.has(k)) {
        out.add(k);
        stack.push(k);
      }
    }
  }
  return out;
}
