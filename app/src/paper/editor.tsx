// The sheet editor on the paper (#9, design §5): the tools' gestures, snapping and what the SVG overlay shows while
// you work (previews, snap markers, the selection, handles, the text box, the view chooser, the title block's
// fields). PaperView hands it the pointer and the keys before its own pan and zoom; each finished gesture is one
// engine command (one undo step) sent through `run`. The maths is in snap.ts, items.ts and tools.ts (unit tested).

import { useEffect, useMemo, useRef, useState } from "react";
import { exec, type Reply, type Scene } from "../engine";
import { PAPER_FONT } from "./draw";
import { cellAt, cellRect, cutPath, describe, dimLines, distToItem, isDim, isShape, isTable, isText, itemAt, itemPoints, movedItem, nextCell, pasteCommand, polylineOf, tableRect, textBox, titleBlock, type PaperItem, type TableItem, type TextItem } from "./items";
import { STANDARD_VIEWS, viewArgs, type SheetInfo, type ViewportItem } from "./pages";
import { handlePoints, HANDLE_CURSORS, mergeGuides, nudge, ortho, pageGuides, rectGuides, rectOf, roundRect, snapMove, snapPoint, snapResize, SNAP_PX, type Guides, type Marker, type P, type Rect, type SnapOpts } from "./snap";
import { cursorFor, dimCommand, IDLE, stepHint, toolForKey, toolStep, type DimChoice, type PaperTool, type ToolEvent, type ToolState } from "./tools";
import { toPaper, toScreen, type PaperCam } from "./view";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

export interface EditorOpts {
  grid: number;
  gridOn: boolean;
  snapOn: boolean;
  dimChoice: DimChoice;
}

export interface EditorProps {
  cam: PaperCam | null;
  size: { w: number; h: number };
  page: SheetInfo;
  viewports: ViewportItem[];
  items: PaperItem[];
  tool: PaperTool;
  setTool: (t: PaperTool) => void;
  opts: EditorOpts;
  picked: number | null;
  onPick: (viewport: number | null) => void;
  item: number | null;
  setItem: (id: number | null) => void;
  run: Run;
  scene: Scene | null;
  say: (t: string, bad?: boolean) => void;
}

interface Drag {
  kind: "move" | "resize" | "item";
  id: number;
  start: P;
  rect: Rect;
  handle: number;
  moved: boolean;
  live: Rect;
  item?: PaperItem;
  delta: P;
}

/** Text widths in paper mm, measured with the paper's own font (as the engine measures it). */
let measureCtx: CanvasRenderingContext2D | null = null;
export function paperWidth(s: string, sizeMm: number): number {
  if (!measureCtx) measureCtx = typeof document !== "undefined" ? (document.createElement("canvas").getContext("2d") ?? null) : null;
  if (!measureCtx) return s.length * sizeMm * 0.55;
  measureCtx.font = `100px ${PAPER_FONT}`;
  return (measureCtx.measureText(s).width / 100) * sizeMm;
}

/** The model's points each viewport shows (`viewport_points`): what dimensions attach to and lines snap to. */
function useModelPoints(want: boolean, viewports: ViewportItem[], scene: Scene | null) {
  const [pts, setPts] = useState<Map<number, { p: P; why: string; ref: unknown; viewport: number }[]>>(new Map());
  const key = viewports.map((v) => `${v.id}:${v.rect_mm.join(",")}:${v.scale}:${JSON.stringify(v.view)}`).join("|");
  useEffect(() => {
    if (!want) return;
    let live = true;
    (async () => {
      const out = new Map<number, { p: P; why: string; ref: unknown; viewport: number }[]>();
      for (const v of viewports) {
        const r = await exec({ cmd: "viewport_points", viewport: v.id, kinds: ["vertex", "midpoint"], limit: 5000 });
        if (!r.ok) continue;
        const list = (r.points as { kind: string; mm: P; anchor: unknown }[]) ?? [];
        out.set(
          v.id,
          list.map((x) => ({ p: x.mm, why: x.kind === "midpoint" ? "model middle" : "model corner", ref: x.anchor, viewport: v.id })),
        );
      }
      if (live) setPts(out);
    })();
    return () => {
      live = false;
    };
  }, [want, key, scene]); // eslint-disable-line react-hooks/exhaustive-deps
  return pts;
}

export function useEditor(P: EditorProps) {
  const { cam, page, viewports, items, tool, opts, picked, onPick, item, setItem, run } = P;
  const [st, setSt] = useState<ToolState>(IDLE);
  const stRef = useRef(st);
  stRef.current = st;
  const [markers, setMarkers] = useState<Marker[]>([]);
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const [hoverItem, setHoverItem] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [preview, setPreview] = useState<{ item: number; kind: "cut" | "ext"; pts: P[] } | null>(null);
  const [editField, setEditField] = useState<null | { key: "project" | "name"; rect: Rect; value: string }>(null);
  const shift = useRef(false);
  const w = page.paper.w_mm;
  const h = page.paper.h_mm;

  // a new tool or page starts clean
  useEffect(() => {
    setSt(IDLE);
    setMarkers([]);
    setPreview(null);
    setNote("");
  }, [tool, page.id]);

  const wantModel = tool === "dimension" || tool === "line" || tool === "polyline" || tool === "arc" || tool === "rect";
  const modelPts = useModelPoints(wantModel, viewports, P.scene);
  const tolMm = cam ? SNAP_PX / cam.k : 2;
  const so: SnapOpts = { grid: opts.grid, gridOn: opts.gridOn && opts.snapOn, tol: opts.snapOn ? tolMm : 0 };

  // what snaps: the page, the other frames, line work's ends and middles, and the model's points
  const guides = useMemo((): Guides => {
    const pageG = pageGuides(w, h);
    const tb = titleBlock(w, h).rect;
    return mergeGuides(pageG, rectGuides([...viewports.map((v) => v.rect_mm), tb]));
  }, [w, h, viewports]);
  const pointGuides = useMemo((): Guides => {
    const model = [...modelPts.values()].flat();
    return { xs: guides.xs, ys: guides.ys, points: [...model, ...itemPoints(items), ...guides.points] };
  }, [guides, modelPts, items]);
  const modelOnly = useMemo((): Guides => ({ xs: [], ys: [], points: [...modelPts.values()].flat() }), [modelPts]);

  const paperAt = (s: [number, number]): P => (cam ? toPaper(cam, s) : [0, 0]);
  const pickTol = cam ? 6 / cam.k : 2;
  const hitItem = (p: P, only?: (i: PaperItem) => boolean) => itemAt(items, p, pickTol, only, paperWidth)?.id ?? null;
  const vpAt = (p: P) => {
    for (let i = viewports.length - 1; i >= 0; i--) {
      const [x, y, ww, hh] = viewports[i].rect_mm;
      if (p[0] >= x && p[0] <= x + ww && p[1] >= y && p[1] <= y + hh) return viewports[i];
    }
    return null;
  };
  const ctx = { sheet: page.id, dimChoice: opts.dimChoice, tol: tolMm };

  /** a tool's event, run through its state machine; a finished gesture's command goes to the engine */
  const feed = (e: ToolEvent) => {
    const out = toolStep(tool, stRef.current, e, ctx);
    stRef.current = out.s;
    setSt(out.s);
    if (out.say) setNote(out.say);
    else if (e.type === "down") setNote("");
    if (out.cmd) void send(out.cmd);
    return out;
  };
  const send = async (cmd: Record<string, unknown>) => {
    const r = await run(cmd);
    if (r.ok && cmd.cmd === "sheet_item_delete") {
      setItem(null);
      setHoverItem(null);
    }
    if (r.ok && (cmd.cmd === "sheet_text" || cmd.cmd === "sheet_line") && typeof r.item === "number" && tool === "select") setItem(r.item);
    // a new table is picked, and typing starts in its first cell; a pasted one is picked
    if (r.ok && (cmd.cmd === "sheet_table" || cmd.cmd === "sheet_table_paste") && cmd.id == null && typeof r.item === "number") {
      setItem(r.item);
      onPick(null);
      if (cmd.cmd === "sheet_table") {
        stRef.current = { t: "cell", id: r.item, row: 0, col: 0, text: "" };
        setSt(stRef.current);
      }
    }
    if (!r.ok && r.error) setNote(r.error);
    setPreview(null);
    return r;
  };
  /** the cell last clicked in a picked table: where a paste goes */
  const cellPick = useRef<{ id: number; cell: [number, number] } | null>(null);
  const lastPaper = useRef<P | null>(null);

  /** the point a drawing tool uses: snapped to points, lines and the grid; Shift keeps a line straight */
  const drawPoint = (raw: P, from?: P): { p: P; marker: Marker | null; anchor: { anchor: unknown; viewport?: number } | null } => {
    if (tool === "dimension") {
      const s = snapPoint(raw, modelOnly, { ...so, tol: Math.max(so.tol, tolMm) }, { pointsOnly: true });
      return { p: s.p, marker: s.marker, anchor: s.point ? { anchor: s.point.ref, viewport: s.point.viewport } : null };
    }
    let q = raw;
    if (shift.current && from) q = ortho(from, raw);
    const s = snapPoint(q, pointGuides, so);
    return { p: shift.current && from && !s.point ? q : s.p, marker: s.marker, anchor: null };
  };
  const lastPt = (s: ToolState): P | undefined => (s.t === "pts" ? s.pts[s.pts.length - 1] : s.t === "drag" ? s.from : undefined);

  // ---- trim and extend previews: the engine works out what would happen (`preview: true`), newest answer wins ----
  const pv = useRef(0);
  const askPreview = (id: number, at: P, extend: boolean) => {
    const my = ++pv.current;
    window.setTimeout(async () => {
      if (my !== pv.current) return;
      const r = await exec({ cmd: extend ? "sheet_extend" : "sheet_trim", item: id, at_mm: at, preview: true });
      if (my !== pv.current) return;
      const it = items.find((i) => i.id === id);
      if (!r.ok || !it || !isShape(it)) {
        setPreview(null);
        setNote(r.ok ? "" : `${extend ? "Can't extend" : "Can't trim"} here: ${r.error ?? ""}`);
        return;
      }
      setNote("");
      if (extend) setPreview({ item: id, kind: "ext", pts: [r.from as P, r.to as P] });
      else {
        const cut = r.cut as { from: P; to: P };
        setPreview({ item: id, kind: "cut", pts: cutPath(it, cut.from, cut.to) });
      }
    }, 70);
  };

  // ---- pointer ------------------------------------------------------------------------------------------------------
  /** true when the editor took the press (PaperView then doesn't pan or pick) */
  const down = (ev: React.PointerEvent, s: [number, number]): boolean => {
    if (!cam || ev.button !== 0) return false;
    shift.current = ev.shiftKey;
    const raw = paperAt(s);
    const target = ev.target as Element;
    if (tool === "select") {
      const hd = target.closest?.("[data-handle]");
      const vp = picked != null ? viewports.find((v) => v.id === picked) : undefined;
      if (hd && vp) {
        const d: Drag = { kind: "resize", id: vp.id, start: raw, rect: vp.rect_mm, handle: Number(hd.getAttribute("data-handle")), moved: false, live: vp.rect_mm, delta: [0, 0] };
        dragRef.current = d;
        setDrag(d);
        return true;
      }
      const it = hitItem(raw);
      if (it != null) {
        const found = items.find((i) => i.id === it)!;
        setItem(it);
        onPick(null);
        if (isTable(found)) {
          const c = cellAt(found, raw);
          cellPick.current = c ? { id: found.id, cell: c } : { id: found.id, cell: [0, 0] };
        }
        if (isShape(found) || isText(found) || isTable(found)) {
          const d: Drag = { kind: "item", id: it, start: raw, rect: [0, 0, 0, 0], handle: -1, moved: false, live: [0, 0, 0, 0], item: found, delta: [0, 0] };
          dragRef.current = d;
          setDrag(d);
        }
        return true;
      }
      const v = vpAt(raw);
      if (v) {
        setItem(null);
        const d: Drag = { kind: "move", id: v.id, start: raw, rect: v.rect_mm, handle: -1, moved: false, live: v.rect_mm, delta: [0, 0] };
        dragRef.current = d;
        setDrag(d);
        return true;
      }
      setItem(null);
      return false;
    }
    // the other tools
    const at = drawPoint(raw, lastPt(stRef.current));
    const hit = tool === "trim" || tool === "extend" || tool === "offset" ? hitItem(raw, isShape) : tool === "erase" ? hitItem(raw) : null;
    // an offset's side and distance come from the snapped point (so a copy lands on the grid); picking is by the mouse
    const offsetting = stRef.current.t === "offset";
    const typedDist = offsetting ? offsetDist(at.p) : undefined;
    feed({ type: "down", p: tool === "trim" || tool === "extend" || tool === "erase" || (tool === "offset" && !offsetting) ? raw : at.p, shift: ev.shiftKey, hit, anchor: at.anchor, dist: typedDist });
    return true;
  };

  const offsetDist = (p: P) => {
    const s = stRef.current;
    if (s.t !== "offset") return 0;
    const it = items.find((i) => i.id === s.item);
    return it ? distToItem(it, p) : 0;
  };

  const move = (ev: React.PointerEvent, s: [number, number]): boolean => {
    if (!cam) return false;
    shift.current = ev.shiftKey;
    const raw = paperAt(s);
    lastPaper.current = raw;
    const d = dragRef.current;
    if (d) {
      const dx = raw[0] - d.start[0];
      const dy = raw[1] - d.start[1];
      if (!d.moved && Math.hypot(dx, dy) * cam.k < 3) return true;
      const others = mergeGuides(pageGuides(w, h), rectGuides([...viewports.filter((v) => v.id !== d.id).map((v) => v.rect_mm), titleBlock(w, h).rect]));
      if (d.kind === "move") {
        const r = snapMove(d.rect, dx, dy, others, so);
        const nd = { ...d, moved: true, live: r.rect };
        dragRef.current = nd;
        setDrag(nd);
        setMarkers(r.markers);
      } else if (d.kind === "resize") {
        const r = snapResize(d.rect, d.handle, raw, others, so);
        const nd = { ...d, moved: true, live: r.rect };
        dragRef.current = nd;
        setDrag(nd);
        setMarkers(r.markers);
      } else {
        // line work and text move with their first point snapped
        const anchor = d.item && isShape(d.item) ? d.item.pts_mm[0] : d.item && (isText(d.item) || isTable(d.item)) ? d.item.at_mm : d.start;
        const sp = snapPoint([anchor[0] + dx, anchor[1] + dy], pointGuides, so);
        const nd = { ...d, moved: true, delta: [sp.p[0] - anchor[0], sp.p[1] - anchor[1]] as P };
        dragRef.current = nd;
        setDrag(nd);
        setMarkers(sp.marker ? [sp.marker] : []);
      }
      return true;
    }
    if (tool === "select") {
      setHoverItem(hitItem(raw));
      setMarkers([]);
      return false;
    }
    if (tool === "trim" || tool === "extend" || tool === "erase" || tool === "offset") {
      const hit = stRef.current.t === "offset" ? null : hitItem(raw, tool === "erase" ? undefined : isShape);
      setHoverItem(hit);
      if ((tool === "trim" || tool === "extend") && hit != null) askPreview(hit, raw, tool === "extend" || ev.shiftKey);
      else if (tool === "trim" || tool === "extend") {
        pv.current++;
        setPreview(null);
      }
      if (stRef.current.t === "offset") {
        const at = drawPoint(raw);
        setMarkers(at.marker ? [at.marker] : []);
        feed({ type: "move", p: at.p, dist: offsetDist(at.p) });
      }
      return true;
    }
    const at = drawPoint(raw, lastPt(stRef.current));
    setMarkers(at.marker ? [at.marker] : []);
    feed({ type: "move", p: at.p });
    return true;
  };

  const up = (ev: React.PointerEvent, s: [number, number]): boolean => {
    if (!cam) return false;
    const d = dragRef.current;
    if (d) {
      dragRef.current = null;
      setDrag(null);
      setMarkers([]);
      if (!d.moved) {
        if (d.kind !== "item") onPick(d.id);
        return true;
      }
      if (d.kind === "item" && d.item) {
        const c = movedItem(d.item, d.delta);
        if (c && (d.delta[0] || d.delta[1])) void send(c);
      } else void send({ cmd: "viewport_edit", id: d.id, rect_mm: roundRect(d.live) });
      if (d.kind !== "item") onPick(d.id);
      return true;
    }
    if (tool === "select") return false;
    if (tool === "rect" || tool === "viewport" || tool === "text" || tool === "table") {
      const at = drawPoint(paperAt(s), lastPt(stRef.current));
      feed({ type: "up", p: at.p, shift: ev.shiftKey });
    }
    return true;
  };

  /** start typing in a table's cell */
  const editCell = (t: TableItem, r: number, c: number) => {
    const rows = t.layout?.rows ?? t.rows;
    if (t.source) {
      setNote("This table is filled from the document's sheets: unlink it in the inspector to type in it");
      return;
    }
    setItem(t.id);
    onPick(null);
    cellPick.current = { id: t.id, cell: [r, c] };
    stRef.current = { t: "cell", id: t.id, row: r, col: c, text: rows[r]?.[c] ?? "" };
    setSt(stRef.current);
  };

  const dbl = (ev: React.MouseEvent, s: [number, number]) => {
    if (!cam) return;
    const raw = paperAt(s);
    if (tool === "select" || tool === "table") {
      const tb = items.find((i) => i.id === hitItem(raw, isTable));
      const c = tb && isTable(tb) ? cellAt(tb, raw) : null;
      if (tb && isTable(tb) && c) {
        editCell(tb, c[0], c[1]);
        return;
      }
      if (tool === "table") return;
    }
    if (tool === "select" || tool === "text") {
      const it = items.find((i) => i.id === hitItem(raw, isText));
      if (it && isText(it)) {
        stRef.current = { t: "text", at: it.at_mm, id: it.id, text: it.paragraphs.join("\n"), width: it.box_mm?.[0] };
        setSt(stRef.current);
        return;
      }
      if (tool === "select") {
        const tb = titleBlock(w, h);
        const inside = (r: Rect) => raw[0] >= r[0] && raw[0] <= r[0] + r[2] && raw[1] >= r[1] && raw[1] <= r[1] + r[3];
        if (inside(tb.project)) setEditField({ key: "project", rect: tb.project, value: page.fields?.project ?? "" });
        else if (inside(tb.title)) setEditField({ key: "name", rect: tb.title, value: page.name });
      }
      return;
    }
    feed({ type: "dbl", p: stRef.current.t === "dim" ? stRef.current.at : drawPoint(raw).p, shift: ev.shiftKey });
  };

  // ---- keys ---------------------------------------------------------------------------------------------------------
  const key = (e: React.KeyboardEvent): boolean => {
    shift.current = e.shiftKey;
    if (e.metaKey || e.ctrlKey || e.altKey) return false;
    const s = stRef.current;
    if (e.key === "Escape") {
      if (s.t !== "idle" || dragRef.current) {
        dragRef.current = null;
        setDrag(null);
        setMarkers([]);
        feed({ type: "esc" });
        setNote("");
        return true;
      }
      if (item != null) {
        setItem(null);
        return true;
      }
      if (tool !== "select") {
        P.setTool("select");
        return true;
      }
      return false;
    }
    if (e.key === "Enter" && s.t !== "idle" && s.t !== "text" && s.t !== "choose") {
      feed({ type: "enter", p: s.t === "dim" ? s.at : s.t === "offset" ? s.at : undefined, shift: e.shiftKey, dist: s.t === "offset" ? offsetDist(s.at) : undefined });
      return true;
    }
    if (e.key === "Backspace" && (s.t === "pts" || s.t === "dim" || s.t === "offset")) {
      feed({ type: "back" });
      return true;
    }
    if (s.t === "offset" && /^[0-9.]$/.test(e.key)) {
      feed({ type: "move", ch: e.key });
      return true;
    }
    // the picked thing: arrows nudge it, Delete deletes it
    const vp = picked != null ? viewports.find((v) => v.id === picked) : undefined;
    const it = item != null ? items.find((i) => i.id === item) : undefined;
    if (e.key.startsWith("Arrow") && (vp || it)) {
      if (vp) {
        const r = nudge(vp.rect_mm, e.key, e.shiftKey, opts.grid, opts.gridOn);
        if (r) void send({ cmd: "viewport_edit", id: vp.id, rect_mm: r });
      } else if (it) {
        const r = nudge([0, 0, 0, 0], e.key, e.shiftKey, opts.grid, opts.gridOn);
        const c = r && movedItem(it, [r[0], r[1]]);
        if (c) void send(c);
      }
      return true;
    }
    if ((e.key === "Delete" || e.key === "Backspace") && (vp || it)) {
      void send({ cmd: "sheet_item_delete", ids: [vp ? vp.id : it!.id] }).then((r) => r.ok && vp && onPick(null));
      return true;
    }
    if (e.key.length === 1 && !e.shiftKey) {
      const t = toolForKey(e.key);
      if (t) {
        P.setTool(t);
        return true;
      }
    }
    return false;
  };

  // ---- text in place ------------------------------------------------------------------------------------------------
  const commitText = async (text: string) => {
    const s = stRef.current;
    if (s.t !== "text") return;
    stRef.current = IDLE;
    setSt(IDLE);
    const words = text.replace(/\s+$/, "");
    if (s.id != null) {
      const old = items.find((i) => i.id === s.id) as TextItem | undefined;
      if (!words.trim()) await send({ cmd: "sheet_item_delete", ids: [s.id] });
      else if (old && old.paragraphs.join("\n") !== words) await send({ cmd: "sheet_text", id: s.id, text: words });
    } else if (words.trim()) await send({ cmd: "sheet_text", sheet: page.id, at_mm: [Math.round(s.at[0] * 100) / 100, Math.round(s.at[1] * 100) / 100], text: words, ...(s.width ? { width_mm: s.width } : {}) });
  };

  // ---- a table's cell in place ----------------------------------------------------------------------------------
  /** the cell's text is saved (one undo step when it changed); `go` moves on to the next cell */
  const commitCell = async (text: string, go: "tab" | "back" | "down" | null) => {
    const s = stRef.current;
    if (s.t !== "cell") return;
    const t = items.find((i) => i.id === s.id);
    const was = t && isTable(t) ? ((t.layout?.rows ?? t.rows)[s.row]?.[s.col] ?? "") : "";
    const next = go && t && isTable(t) ? nextCell(t, s.row, s.col, go) : null;
    // the next cell's text, as it is now (this cell's change doesn't touch it)
    const nextText = next && t && isTable(t) ? ((t.layout?.rows ?? t.rows)[next[0]]?.[next[1]] ?? "") : "";
    stRef.current = next ? { t: "cell", id: s.id, row: next[0], col: next[1], text: nextText } : IDLE;
    setSt(stRef.current);
    if (next) cellPick.current = { id: s.id, cell: next };
    if (text !== was) await send({ cmd: "sheet_table", id: s.id, cells: [{ row: s.row, col: s.col, text }] });
  };
  /** a grid pasted into a cell fills the table from there */
  const pasteInCell = (text: string): boolean => {
    const s = stRef.current;
    if (s.t !== "cell") return false;
    const cmd = pasteCommand(text, { table: s.id, cell: [s.row, s.col] }, page.id, [0, 0]);
    if (!cmd) return false;
    stRef.current = IDLE;
    setSt(IDLE);
    void send(cmd);
    return true;
  };
  // a grid pasted on the paper: into the picked table at its clicked cell, else a new table where the mouse is
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const a = document.activeElement;
      if (a && a !== document.body && !a.closest?.(".paper-view")) return;
      if (a && (a.tagName === "INPUT" || a.tagName === "TEXTAREA")) return;
      const text = e.clipboardData?.getData("text/plain") ?? "";
      const picked = item != null ? items.find((i) => i.id === item) : undefined;
      const into = picked && isTable(picked) && !picked.source ? { table: picked.id, cell: cellPick.current?.id === picked.id ? cellPick.current.cell : ([0, 0] as [number, number]) } : null;
      const at = lastPaper.current && lastPaper.current[0] >= 0 && lastPaper.current[1] >= 0 && lastPaper.current[0] < w && lastPaper.current[1] < h ? lastPaper.current : ([20, 30] as P);
      const cmd = pasteCommand(text, into, page.id, at);
      if (!cmd) return;
      e.preventDefault();
      void send(cmd);
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }); // eslint-disable-line react-hooks/exhaustive-deps
  const cancelText = () => {
    stRef.current = IDLE;
    setSt(IDLE);
  };

  // ---- the view chooser (after a viewport frame is drawn) ------------------------------------------------------
  const chooseView = async (name: string, label: string) => {
    const s = stRef.current;
    if (s.t !== "choose") return;
    stRef.current = IDLE;
    setSt(IDLE);
    const scaled = name === "plan" || (STANDARD_VIEWS.includes(name) && name !== "iso");
    const r = await send({ cmd: "viewport_add", sheet: page.id, rect_mm: s.rect, ...viewArgs(name), ...(scaled ? { scale: 50 } : {}), label });
    if (r.ok && typeof r.viewport === "number") {
      P.setTool("select");
      onPick(r.viewport);
    }
  };

  // ---- what the overlay shows ---------------------------------------------------------------------------------
  const scr = (p: P) => (cam ? toScreen(cam, p) : p);
  const pathOf = (pts: P[]) => pts.map((p, i) => `${i ? "L" : "M"}${scr(p)[0].toFixed(1)} ${scr(p)[1].toFixed(1)}`).join("");
  const overlay = cam ? (
    <g className="ed">
      {/* the grid, when it's on and big enough to see */}
      {opts.gridOn && opts.grid * cam.k >= 6 && (
        <>
          <defs>
            <pattern id="paper-grid" width={opts.grid * cam.k} height={opts.grid * cam.k} patternUnits="userSpaceOnUse" x={cam.ox} y={cam.oy}>
              <circle cx={0.75} cy={0.75} r={0.75} className="grid-dot" />
            </pattern>
          </defs>
          <rect className="paper-grid" x={cam.ox} y={cam.oy} width={w * cam.k} height={h * cam.k} fill="url(#paper-grid)" />
        </>
      )}
      {/* the items: focusable for the keyboard and screen readers, drawn when hovered or picked */}
      {items.map((i) => {
        const on = i.id === item;
        const hov = i.id === hoverItem;
        const cls = `pi ${on ? "on" : ""} ${hov ? "hover" : ""} ${hov && tool === "erase" ? "doomed" : ""}`;
        let d = "";
        if (isShape(i)) d = pathOf(polylineOf(i.kind, i.pts_mm));
        else if (isText(i)) {
          const [x, y, ww, hh] = textBox(i, paperWidth);
          d = pathOf([[x, y], [x + ww, y], [x + ww, y + hh], [x, y + hh], [x, y]]);
        } else if (isTable(i)) {
          const [x, y, ww, hh] = tableRect(i);
          d = pathOf([[x, y], [x + ww, y], [x + ww, y + hh], [x, y + hh], [x, y]]);
        } else if (isDim(i) && i.paper_mm) d = dimLines(i.paper_mm, i.mode, i.offset_mm ?? 8).spans.map(([a, b]) => pathOf([a, b])).join("");
        if (!d) return null;
        const moving = drag?.kind === "item" && drag.id === i.id && drag.moved;
        return (
          <path
            key={i.id}
            className={cls}
            d={d}
            data-item={i.id}
            data-focus=""
            transform={moving ? `translate(${drag.delta[0] * cam.k} ${drag.delta[1] * cam.k})` : undefined}
            role="button"
            tabIndex={0}
            aria-pressed={on}
            aria-label={`${describe(i)}. Enter selects it; then arrows move it and Delete deletes it.`}
            onFocus={() => setHoverItem(i.id)}
            onBlur={() => setHoverItem(null)}
            onKeyDown={(e) => itemKey(e, i.id)}
          />
        );
      })}
      {/* a frame being moved or resized */}
      {drag && drag.kind !== "item" && drag.moved && (() => {
        const [x, y] = scr([drag.live[0], drag.live[1]]);
        return <rect className="ed-live" x={x} y={y} width={drag.live[2] * cam.k} height={drag.live[3] * cam.k} />;
      })()}
      {/* the picked viewport's handles take the mouse */}
      {tool === "select" &&
        picked != null &&
        !drag?.moved &&
        (() => {
          const v = viewports.find((x) => x.id === picked);
          if (!v) return null;
          return handlePoints(v.rect_mm).map((p, i) => {
            const [hx, hy] = scr(p);
            return <rect key={i} className="ed-handle" data-handle={i} x={hx - 6} y={hy - 6} width={12} height={12} style={{ cursor: HANDLE_CURSORS[i] }} aria-hidden="true" />;
          });
        })()}
      {/* the cell being typed in */}
      {st.t === "cell" &&
        (() => {
          const t = items.find((i) => i.id === st.id);
          const r = t && isTable(t) ? cellRect(t, st.row, st.col) : null;
          if (!r) return null;
          const [x, y] = scr([r[0], r[1]]);
          return <rect className="ed-cell" x={x} y={y} width={r[2] * cam.k} height={r[3] * cam.k} />;
        })()}
      {/* what the tool is making */}
      {toolPreview()}
      {/* trim and extend: what goes, where it runs */}
      {preview && <path className={preview.kind === "cut" ? "ed-cut" : "ed-ext"} d={pathOf(preview.pts)} />}
      {/* snaps */}
      {markers.map((m, i) => {
        if (m.kind === "x") {
          const x = scr([m.v, 0])[0];
          return <line key={i} className="snap-guide" x1={x} x2={x} y1={cam.oy} y2={cam.oy + h * cam.k} />;
        }
        if (m.kind === "y") {
          const y = scr([0, m.v])[1];
          return <line key={i} className="snap-guide" y1={y} y2={y} x1={cam.ox} x2={cam.ox + w * cam.k} />;
        }
        const [x, y] = scr(m.at);
        return (
          <g key={i} className={`snap-mark ${m.kind}`}>
            {m.why.includes("middle") || m.why === "midpoint" ? <path d={`M${x} ${y - 6}L${x + 6} ${y + 4}L${x - 6} ${y + 4}Z`} /> : m.kind === "grid" ? <path d={`M${x - 4} ${y}H${x + 4}M${x} ${y - 4}V${y + 4}`} /> : <rect x={x - 5} y={y - 5} width={10} height={10} />}
            <text x={x + 9} y={y - 8}>
              {m.why}
            </text>
          </g>
        );
      })}
    </g>
  ) : null;

  function toolPreview() {
    if (!cam) return null;
    const s = st;
    if (s.t === "drag") return <path className="ed-draw" d={pathOf(polylineOf("rect", [s.from, s.to]))} />;
    if (s.t === "choose") return <path className="ed-draw" d={pathOf(polylineOf("rect", [[s.rect[0], s.rect[1]], [s.rect[0] + s.rect[2], s.rect[1] + s.rect[3]]]))} />;
    if (s.t === "pts") {
      if (tool === "arc" && s.pts.length === 2) return <path className="ed-draw" d={pathOf(polylineOf("arc", [s.pts[0], s.at, s.pts[1]]))} />;
      return <path className="ed-draw" d={pathOf([...s.pts, s.at])} />;
    }
    if (s.t === "dim" && s.picks.length) {
      const pts = [...s.picks.map((k) => k.p)];
      const cmd = pts.length >= 2 ? dimCommand(s.picks, s.at, shift.current || !!s.baseline, { sheet: page.id, dimChoice: opts.dimChoice }) : null;
      const g = cmd ? dimLines(pts, cmd.mode as string, cmd.offset_mm as number) : { spans: [], ext: [] };
      return (
        <g className="ed-dim">
          {pts.map((p, i) => {
            const [x, y] = scr(p);
            return <circle key={i} cx={x} cy={y} r={4} />;
          })}
          {g.ext.map(([a, b], i) => (
            <path key={`e${i}`} className="ext" d={pathOf([a, b])} />
          ))}
          {g.spans.map(([a, b], i) => (
            <path key={`s${i}`} d={pathOf([a, b])} />
          ))}
          {pts.length === 1 && <path className="ext" d={pathOf([pts[0], s.at])} />}
        </g>
      );
    }
    if (s.t === "offset") {
      const it = items.find((i) => i.id === s.item);
      if (!it || !isShape(it)) return null;
      const d = s.typed ? Number(s.typed) : distToItem(it, s.at);
      const [x, y] = scr(s.at);
      let copy: P[] | null = null;
      if (it.kind === "line") {
        const [a, b] = it.pts_mm;
        const l = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
        let n: P = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
        if ((s.at[0] - a[0]) * n[0] + (s.at[1] - a[1]) * n[1] < 0) n = [-n[0], -n[1]];
        copy = [
          [a[0] + n[0] * d, a[1] + n[1] * d],
          [b[0] + n[0] * d, b[1] + n[1] * d],
        ];
      } else if (it.kind === "rect") {
        const r = rectOf(it.pts_mm[0], it.pts_mm[1]);
        const inside = s.at[0] > r[0] && s.at[0] < r[0] + r[2] && s.at[1] > r[1] && s.at[1] < r[1] + r[3];
        const k = inside ? -d : d;
        copy = polylineOf("rect", [[r[0] - k, r[1] - k], [r[0] + r[2] + k, r[1] + r[3] + k]]);
      }
      return (
        <g className="ed-offset">
          {copy && <path className="ed-draw" d={pathOf(copy)} />}
          <text x={x + 12} y={y + 18} className="ed-readout">
            {`${(Math.round(d * 10) / 10).toString()} mm${s.typed ? " (typed)" : ""}`}
          </text>
        </g>
      );
    }
    return null;
  }

  // items reached with the keyboard: arrows go to the next one until it is picked, Enter picks
  const itemKey = (e: React.KeyboardEvent, id: number) => {
    if (item === id) return; // the paper's own keys (nudge, Delete) take it
    const i = items.findIndex((x) => x.id === id);
    void i;
    const go = (k: number) => focusNext(e.currentTarget as Element, k);
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      setItem(id);
      onPick(null);
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      go(1);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      go(-1);
    }
  };

  // HTML on top of the paper: the text box, the view chooser, the title block's field
  const html = cam ? (
    <>
      {st.t === "text" && (
        <TextBox
          key={`${st.id ?? "new"}:${st.at.join(",")}`}
          at={scr(st.at)}
          k={cam.k}
          height={(st.id != null ? (items.find((i) => i.id === st.id) as TextItem | undefined)?.height_mm : undefined) ?? 3.5}
          width={st.width}
          initial={st.text}
          onCommit={commitText}
          onCancel={cancelText}
        />
      )}
      {st.t === "cell" &&
        (() => {
          const t = items.find((i) => i.id === st.id);
          const r = t && isTable(t) ? cellRect(t, st.row, st.col) : null;
          if (!r || !t || !isTable(t)) return null;
          return <CellBox key={`${st.id}:${st.row}:${st.col}`} rect={r} scr={scr} k={cam.k} textMm={t.layout?.text_mm ?? t.text_mm ?? 2.5} header={st.row < (t.layout?.header_rows ?? t.header_rows ?? 1)} label={`Row ${st.row + 1}, column ${st.col + 1}`} initial={st.text} onDone={commitCell} onPaste={pasteInCell} onCancel={cancelText} />;
        })()}
      {st.t === "choose" && <ViewChooser at={scr([st.rect[0], st.rect[1] + st.rect[3]])} views={P.scene?.views.map((v) => v.name) ?? []} onPick={chooseView} onCancel={() => feed({ type: "esc" })} />}
      {editField && (
        <FieldBox
          rect={editField.rect}
          scr={scr}
          k={cam.k}
          label={editField.key === "project" ? "Project name (title block)" : "Page name (title block)"}
          value={editField.value}
          onDone={(v) => {
            const f = editField;
            setEditField(null);
            if (v == null || v === f.value) return;
            if (f.key === "project") void send({ cmd: "sheet_edit", sheet: page.id, fields: { project: v.trim() ? v : null } });
            else if (v.trim()) void send({ cmd: "sheet_edit", sheet: page.id, name: v.trim() });
          }}
        />
      )}
    </>
  ) : null;

  let cursor: string | undefined = tool === "select" ? undefined : cursorFor(tool);
  if (tool === "select" && hoverItem != null) cursor = "move";
  if (drag?.kind === "resize") cursor = HANDLE_CURSORS[drag.handle];
  else if (drag?.moved) cursor = "grabbing";
  const hint = note || stepHint(tool, st);
  const busy = st.t !== "idle" || !!drag;
  return { down, move, up, dbl, key, overlay, html, cursor, hint, busy, hoverItem, dragging: !!drag };
}

/** Arrow keys between what's on the page (viewports, then line work, text and dimensions), wrapping round. */
export function focusNext(from: Element, k: number) {
  const all = [...document.querySelectorAll(".paper-overlay [data-focus]")] as SVGElement[];
  const i = all.indexOf(from as SVGElement);
  all[(i + k + all.length) % all.length]?.focus();
}

/** Typing in a table's cell where it is: Tab goes across, Enter down (Shift goes back), Escape stops; a grid pasted
 * from a spreadsheet fills the table from this cell. */
function CellBox({ rect, scr, k, textMm, header, label, initial, onDone, onPaste, onCancel }: { rect: Rect; scr: (p: P) => P; k: number; textMm: number; header: boolean; label: string; initial: string; onDone: (t: string, go: "tab" | "back" | "down" | null) => void; onPaste: (t: string) => boolean; onCancel: () => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const done = useRef(false);
  const [text, setText] = useState(initial);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const [x, y] = scr([rect[0], rect[1]]);
  const finish = (go: "tab" | "back" | "down" | null) => {
    if (done.current) return;
    done.current = true;
    onDone(text, go);
  };
  return (
    <textarea
      ref={ref}
      className="paper-cellbox"
      aria-label={`Table cell, ${label}: Tab next cell, Enter the cell below, Alt+Enter a new line, Escape stops`}
      value={text}
      spellCheck
      style={{ left: x, top: y, width: Math.max(40, rect[2] * k), height: Math.max(18, rect[3] * k), fontSize: `${Math.max(9, (textMm / 0.73) * k)}px`, fontWeight: header ? 600 : 400 }}
      onChange={(e) => setText(e.target.value)}
      onPointerDown={(e) => e.stopPropagation()}
      onPaste={(e) => {
        if (onPaste(e.clipboardData.getData("text/plain"))) {
          e.preventDefault();
          done.current = true;
        }
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Tab") {
          e.preventDefault();
          finish(e.shiftKey ? "back" : "tab");
        } else if (e.key === "Enter" && e.altKey) {
          // a new line inside the cell (as in a spreadsheet)
          e.preventDefault();
          const el = e.currentTarget;
          const at = el.selectionStart ?? text.length;
          setText(text.slice(0, at) + "\n" + text.slice(el.selectionEnd ?? at));
        } else if (e.key === "Enter") {
          e.preventDefault();
          finish(e.shiftKey ? null : "down");
        } else if (e.key === "Escape") {
          e.preventDefault();
          done.current = true;
          onCancel();
        }
      }}
      onBlur={() => finish(null)}
    />
  );
}

/** Typing a paper text where it goes: Enter finishes, Shift+Enter is a new line, Escape cancels. A text block
 * (`width`, mm) wraps as the page will. */
function TextBox({ at, k, height, width, initial, onCommit, onCancel }: { at: [number, number]; k: number; height: number; width?: number; initial: string; onCommit: (t: string) => void; onCancel: () => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const done = useRef(false);
  const [text, setText] = useState(initial);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const size = (height / 0.73) * k;
  const lines = Math.max(1, text.split("\n").length);
  const cols = Math.max(8, ...text.split("\n").map((l) => l.length + 2));
  return (
    <textarea
      ref={ref}
      className="paper-textbox"
      aria-label="Text on the page: Enter finishes, Shift+Enter starts a new line, Escape cancels"
      value={text}
      rows={width ? Math.max(lines, Math.ceil(text.length / Math.max(8, (width * k) / (size * 0.5)))) : lines}
      cols={cols}
      spellCheck
      style={{ left: at[0], top: at[1] - size * 0.95, fontSize: `${Math.max(8, size)}px`, lineHeight: 1.25, ...(width ? { width: width * k, whiteSpace: "pre-wrap" as const } : {}) }}
      onChange={(e) => setText(e.target.value)}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          done.current = true;
          onCommit(text);
        } else if (e.key === "Escape") {
          e.preventDefault();
          done.current = true;
          onCancel();
        }
      }}
      onBlur={() => {
        if (!done.current) {
          done.current = true;
          onCommit(text);
        }
      }}
    />
  );
}

/** After a viewport frame is drawn: which view of the model goes in it. */
function ViewChooser({ at, views, onPick, onCancel }: { at: [number, number]; views: string[]; onPick: (name: string, label: string) => void; onCancel: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    (ref.current?.querySelector("button") as HTMLButtonElement | null)?.focus();
  }, []);
  const opts: [string, string, string][] = [
    ["plan", "Floor plan", "Plan, 1:50, cut 1.2 m up"],
    ["front", "Front elevation", "Front elevation, 1:50"],
    ["back", "Back elevation", "Back elevation, 1:50"],
    ["left", "Left elevation", "Left elevation, 1:50"],
    ["right", "Right elevation", "Right elevation, 1:50"],
    ["top", "Top", "Top view, 1:50"],
    ["iso", "3D view", "3D view (iso)"],
    ...views.map((v): [string, string, string] => [v, v, `Scene: ${v}`]),
  ];
  return (
    <div
      ref={ref}
      className="paper-chooser"
      role="dialog"
      aria-label="Which view goes in this frame"
      style={{ left: at[0], top: at[1] + 6 }}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") onCancel();
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const bs = [...(ref.current?.querySelectorAll("button") ?? [])] as HTMLButtonElement[];
          const i = bs.indexOf(document.activeElement as HTMLButtonElement);
          bs[(i + (e.key === "ArrowDown" ? 1 : -1) + bs.length) % bs.length]?.focus();
        }
      }}
    >
      <div className="chooser-title">Show in this frame</div>
      {opts.map(([id, label, tip]) => (
        <button key={id} title={tip} onClick={() => onPick(id, label)}>
          {label}
        </button>
      ))}
      <button className="ghost" onClick={onCancel}>
        Cancel (Esc)
      </button>
    </div>
  );
}

/** A title block field typed where it is printed. */
function FieldBox({ rect, scr, k, label, value, onDone }: { rect: Rect; scr: (p: P) => P; k: number; label: string; value: string; onDone: (v: string | null) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const [x, y] = scr([rect[0], rect[1]]);
  const finish = (v: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(v);
  };
  return (
    <input
      ref={ref}
      className="paper-field"
      aria-label={label}
      title={`${label}: Enter saves, Escape cancels`}
      defaultValue={value}
      style={{ left: x + 2, top: y + 2, width: rect[2] * k - 4, height: Math.min(rect[3] * k - 4, 40), fontSize: Math.max(11, Math.min(rect[3] * k * 0.45, 22)) }}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish((e.target as HTMLInputElement).value);
        if (e.key === "Escape") finish(null);
      }}
      onBlur={(e) => finish(e.target.value)}
    />
  );
}

export { useModelPoints };
