import { useLocale } from "./locale";
// The right-click menu: what it offers for the selection, and the menu itself.
import { useLayoutEffect, useRef, useState } from "react";
import { ask } from "./ask";
import { exec, type Reply, type Scene } from "./engine";
import { EMPTY, isEmpty, selIds, type Sel } from "./selection";
import { formatLength, parseLength } from "./units";
import { type Run } from "./panelkit";
import { useEscapeToClose } from "./sheet";

export interface MenuItem {
  labelId?: string;
  label: string;
  key?: string;
  danger?: boolean;
  run: () => void;
}

export function contextItems(scene: Scene, sel: Sel, run: Run, setSel: (s: Sel) => void, fitSel: () => void, section?: { origin: number[]; normal: number[] } | null, onCage?: (id: number) => void, onToast?: (t: string) => void, subd?: { subdivide: () => void; subdBox: () => void; projectTexture?: () => void }): MenuItem[] {
  const ids = selIds(sel);
  const items: MenuItem[] = [];
  if (isEmpty(sel)) {
    items.push({ label: "Unhide all", run: () => run({ cmd: "unhide" }) });
    items.push({ label: "Clear guides", run: () => run({ cmd: "clear_guides" }) });
    items.push({ label: "SubD box", run: () => subd?.subdBox() });
    items.push({ label: "Soften flat seams", run: () => run({ cmd: "soften", coplanar: true }) });
    if (scene.editing != null || scene.activeGroup != null) items.push({ label: "Close editing", key: "Esc", run: () => run({ cmd: "close" }) });
    return items;
  }
  const one = ids.length === 1;
  if (one && sel.groups.length && scene.cages?.[String(ids[0])]) items.push({ label: "Shape in SubD mode", key: "Tab", run: () => onCage?.(ids[0]) });
  if (one && (sel.groups.length || sel.instances.length) && !sel.faces.length && !scene.cages?.[String(ids[0])]) items.push({ label: "Subdivide…", run: () => subd?.subdivide() });
  if (one && (sel.groups.length || sel.instances.length)) items.push({ label: `Edit ${sel.groups.length ? "group" : "component"}`, run: () => (run({ cmd: "edit", id: ids[0] }), setSel(EMPTY)) });
  // groups and components selected go inside the new group as they are (nesting)
  if (sel.faces.length || sel.groups.length || sel.instances.length) {
    items.push({ label: sel.faces.length ? "Make group" : "Group these (nest them)", labelId: sel.faces.length ? "context.group" : "context.nest", run: () => run({ cmd: "group", ids: [...sel.faces, ...sel.groups, ...sel.instances] }).then((r) => r.ok && setSel({ ...EMPTY, groups: [r.group as number] })) });
  }
  if (one && sel.groups.length) {
    const g = scene.groups.find((x) => x.id === ids[0]);
    items.push({ label: "Ungroup (keep what's inside)", run: () => run({ cmd: "ungroup", id: ids[0] }).then(() => setSel(EMPTY)) });
    if (g) items.push({ label: g.locked ? "Unlock" : "Lock", run: () => run({ cmd: "lock", ids: [g.id], locked: !g.locked }) });
  }
  // walls from an imported plan: the thin bands between wall lines, pulled up
  if (one && sel.groups.length) {
    items.push({
      label: "Raise walls…",
      run: async () => {
        const units = scene.units || "m";
        const raw = await ask("Wall height", formatLength(2.7, units), "The thin bands between wall lines go up this high (wall layers first, if the drawing has them).", "dialog.wallHeight");
        if (raw == null) return;
        const height = parseLength(raw, units);
        if (!(height > 0)) return onToast?.("That isn't a height");
        const r = await run({ cmd: "raise_walls", group: ids[0], height });
        if (!r.ok) return;
        const n = r.walls as number;
        const layers = (r.layers as string[]) ?? [];
        const w = (r.windows as number) ?? 0;
        onToast?.(n || w ? `Raised ${n} wall${n === 1 ? "" : "s"}${w ? ` with ${w} window opening${w === 1 ? "" : "s"}` : ""}${layers.length ? ` from ${layers.join(", ")}` : ""}` : "No walls found: wall bands need both lines of the wall, closed at the ends");
      },
    });
  }
  // a shared component: give this one its own definition
  if (one && sel.instances.length) {
    const def = scene.instances.find((i) => i.id === ids[0])?.component;
    if (def != null && scene.instances.filter((i) => i.component === def).length > 1) items.push({ label: "Make unique", run: () => run({ cmd: "make_unique", id: ids[0] }) });
  }
  // a picture laid on the selection by planar, box, cylinder or sphere projection
  if (subd?.projectTexture && (sel.faces.length || sel.groups.length || sel.instances.length)) items.push({ label: "Project texture…", labelId: "context.projectTexture", run: () => subd.projectTexture?.() });
  if (sel.faces.length === 1 && !sel.groups.length && !sel.instances.length) items.push({ label: "Orient faces (match this one)", run: () => run({ cmd: "orient_faces", face: sel.faces[0] }) });
  // Select ›: what's connected, what shares the paint, what shares the tag
  const pickInto = (r: Reply) => r.ok && setSel({ ...EMPTY, faces: (r.faces as number[]) ?? [], edges: (r.edges as number[]) ?? [], groups: (r.groups as number[]) ?? [], instances: (r.instances as number[]) ?? [] });
  const seed = sel.faces[0] ?? sel.edges[0];
  if (seed != null && !sel.groups.length && !sel.instances.length) {
    items.push({ label: "Select all connected", run: () => exec({ cmd: "connected", id: seed, all: true }).then(pickInto) });
    if (sel.faces.length) {
      const mat = scene.faces.find((f) => f.id === sel.faces[0])?.material ?? null;
      items.push({ label: mat ? `Select all painted ${mat.replace(/^[a-z]+:/, "")}` : "Select all unpainted faces", run: () => exec({ cmd: "same_as", by: "material", material: mat }).then(pickInto) });
    }
  }
  if (one && (sel.groups.length || sel.instances.length)) {
    const tag = sel.groups.length ? scene.groups.find((g) => g.id === ids[0])?.tag : scene.instances.find((i) => i.id === ids[0])?.tag;
    if (tag) items.push({ label: `Select all on tag "${tag}"`, run: () => exec({ cmd: "same_as", by: "tag", tag }).then(pickInto) });
  }
  // a matched photo onto these faces, as textures (SketchUp's Project Photo)
  if (sel.faces.length || sel.groups.length)
    for (const v of scene.views.filter((x) => x.photo).slice(0, 4))
      items.push({
        label: `Project photo "${v.name}"`,
        run: () =>
          run({ cmd: "project_photo", view: v.name, ids: [...sel.faces, ...sel.groups] }).then((r) => {
            if (!r.ok) return;
            const n = (r.faces as number[]).length;
            onToast?.(`Photo projected onto ${n} face${n === 1 ? "" : "s"}${r.away ? `; ${r.away} facing away from that camera were left alone` : ""}`);
          }),
      });
  if (sel.faces.length || sel.groups.length) {
    items.push({
      label: "Make component…", labelId: "context.component",
      key: "G",
      run: async () => {
        const name = await ask("Component name", "Component", undefined, "dialog.componentName");
        if (name) run({ cmd: "make_component", ids: [...sel.faces, ...sel.groups], name }).then(() => setSel(EMPTY));
      },
    });
  }
  // edge tools: bevel / round outside edges of solids, divide an edge
  if (sel.edges.length) {
    const units = scene.units || "m";
    const askSize = async (title: string) => {
      const t = await ask(title, units === "ft" || units === "in" ? `2"` : "0.05", "A length, in any units (2\", 5cm, 0.1).");
      if (t == null) return null;
      const v = parseLength(t, units);
      return Number.isFinite(v) && v > 0 ? v : null;
    };
    items.push({
      label: "Bevel (chamfer)…",
      run: async () => {
        const size = await askSize("Bevel these edges by");
        if (size) run({ cmd: "bevel", edges: sel.edges, size, segments: 1 }).then((r) => r.ok && setSel(EMPTY));
      },
    });
    items.push({
      label: "Round…",
      run: async () => {
        const size = await askSize("Round these edges with a radius of");
        if (size) run({ cmd: "bevel", edges: sel.edges, size, segments: 8 }).then((r) => r.ok && setSel(EMPTY));
      },
    });
    // curves: weld a chain of edges into one, or explode one back to edges
    const curves = new Set(sel.edges.map((id) => scene.edgeList.find((e) => e.id === id)?.curve).filter((c) => c != null));
    if (curves.size) items.push({ label: "Explode curve", run: () => run({ cmd: "explode_curve", edges: sel.edges }) });
    if (sel.edges.length >= 2 && !(curves.size === 1 && sel.edges.every((id) => scene.edgeList.find((e) => e.id === id)?.curve != null)))
      items.push({ label: "Weld edges", run: () => run({ cmd: "weld", edges: sel.edges }) });
    if (sel.edges.length === 1)
      items.push({
        label: "Divide…",
        run: async () => {
          const t = await ask("Divide this edge into how many parts?", "4");
          const n = t ? parseInt(t, 10) : NaN;
          if (n >= 2) run({ cmd: "divide", edge: sel.edges[0], count: n });
        },
      });
  }
  // slice a solid along the section cut you've placed
  if (section && sel.groups.length + sel.instances.length === 1 && !sel.faces.length) {
    items.push({ label: "Slice along the section cut", run: () => run({ cmd: "slice", id: [...sel.groups, ...sel.instances][0], origin: section.origin, normal: section.normal }).then((r) => r.ok && setSel(EMPTY)) });
  }
  // Intersect Faces and Flip Along (SketchUp's Edit / context menu)
  const shapes = [...sel.faces, ...sel.groups, ...sel.instances];
  if (shapes.length) {
    items.push({ label: "Intersect Faces: with model", run: () => run({ cmd: "intersect", ids: shapes, with: "model" }) });
    if (shapes.length > 1) items.push({ label: "Intersect Faces: with selection", run: () => run({ cmd: "intersect", ids: shapes, with: "selection" }) });
    for (const [axis, name] of [["red", "Red"], ["green", "Green"], ["blue", "Blue"]]) items.push({ label: `Flip Along: ${name}`, run: () => run({ cmd: "flip", ids: shapes, axis }) });
  }
  // Solid Tools: in pick order (for Subtract and Trim the first one cuts the second)
  const solids = [...sel.groups, ...sel.instances];
  if (solids.length >= 2 && !sel.faces.length && !sel.edges.length) {
    const tool = (label: string, op: string, two = false) =>
      (!two || solids.length === 2) && items.push({ label: `Solid: ${label}`, run: () => run({ cmd: "solid", op, ids: solids }).then((r) => r.ok && setSel({ ...EMPTY, groups: (r.groups as number[]) ?? [] })) });
    tool("Outer Shell", "outer_shell");
    tool("Union", "union");
    tool("Intersect", "intersect");
    tool("Subtract (first cuts second)", "subtract", true);
    tool("Trim (first cuts second, kept)", "trim", true);
    tool("Split", "split", true);
  }
  if (sel.instances.length) items.push({ label: "Explode", labelId: "context.explode", run: () => Promise.all(sel.instances.map((id) => run({ cmd: "explode", id }))).then(() => setSel(EMPTY)) });
  if (sel.faces.length || sel.groups.length) items.push({ label: "Reverse faces", labelId: "context.reverse", run: () => run({ cmd: "reverse", ids: [...sel.faces, ...sel.groups] }) });
  if (sel.edges.length) items.push({ label: "Soften edges", run: () => run({ cmd: "soften", ids: sel.edges }) });
  items.push({ label: "Zoom to selection", key: "⇧Z", run: fitSel });
  items.push({ label: "Hide", labelId: "context.hide", run: () => run({ cmd: "hide", ids }).then(() => setSel(EMPTY)) });
  items.push({ label: "Delete", labelId: "context.delete", key: "⌫", danger: true, run: () => run({ cmd: "delete", ids }).then(() => setSel(EMPTY)) });
  return items;
}

export function ContextMenu({ x, y, items, close }: { x: number; y: number; items: MenuItem[]; close: () => void }) {
  const { t } = useLocale();
  useEscapeToClose(close);
  // keep the whole menu on screen: open upward or leftward near an edge
  const box = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    // measured where it landed (x, y are relative to the menu's container)
    const m = 8;
    const r = el.getBoundingClientRect();
    const dx = r.left - x; // the container's offset from the window
    const dy = r.top - y;
    setAt({
      left: r.right > window.innerWidth - m ? Math.max(m - dx, x - r.width) : x,
      top: r.bottom > window.innerHeight - m ? Math.max(m - dy, window.innerHeight - m - r.height - dy) : y,
    });
  }, [x, y, items.length]);
  return (
    <div className="menu-scrim" onPointerDown={close} onContextMenu={(e) => (e.preventDefault(), close())}>
      <div className="menu" ref={box} style={at} onPointerDown={(e) => e.stopPropagation()}>
        {items.map((it) => (
          <button
            key={it.label}
            className={it.danger ? "danger" : ""}
            onClick={() => {
              close();
              it.run();
            }}
          >
            <span>{it.labelId ? t(it.labelId) : it.label}</span>
            {it.key && <kbd>{it.key}</kbd>}
          </button>
        ))}
      </div>
    </div>
  );
}
