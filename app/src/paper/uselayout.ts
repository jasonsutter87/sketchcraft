// The Layout workspace's state: the saved sheets (`sheets_info`), the page you're on, the viewport picked on it, and
// that page's display list (`sheet_ir`). Read only while Layout is showing, and again whenever the model changes
// (your edits, undo, an agent's edits), so the paper always shows the model as it is.

import { useCallback, useEffect, useRef, useState } from "react";
import { exec, type Reply, type Scene } from "../engine";
import type { Ir } from "./draw";
import { inOrder, newPageName, reorderCommand, type SheetInfo } from "./pages";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;

export interface LayoutState {
  sheets: SheetInfo[];
  /** the sheets have been read at least once */
  ready: boolean;
  page: SheetInfo | undefined;
  setPage: (id: number) => void;
  viewport: number | null;
  setViewport: (id: number | null) => void;
  ir: Ir | null;
  /** drawing the page (the engine is building it) */
  drawing: boolean;
  irError: string;
  /** page commands, each one undo step */
  add: () => Promise<void>;
  duplicate: (id: number) => Promise<void>;
  remove: (id: number) => Promise<void>;
  rename: (id: number, name: string) => Promise<void>;
  /** put the sheets in this order (ids, first first): one command, one undo step */
  reorder: (ids: number[]) => Promise<void>;
}

export function useLayout({ active, scene, run, say }: { active: boolean; scene: Scene | null; run: Run; say: (t: string, bad?: boolean, undo?: boolean) => void }): LayoutState {
  const [sheets, setSheets] = useState<SheetInfo[]>([]);
  const [ready, setReady] = useState(false);
  const [pageId, setPageId] = useState<number | null>(null);
  const [viewport, setViewport] = useState<number | null>(null);
  const [ir, setIr] = useState<Ir | null>(null);
  const [drawing, setDrawing] = useState(false);
  const [irError, setIrError] = useState("");
  const seq = useRef(0);

  // the sheets, whenever the model changes while Layout shows
  useEffect(() => {
    if (!active || !scene) return;
    let live = true;
    exec({ cmd: "sheets_info" }).then((r) => {
      if (!live || !r.ok) return;
      setSheets(inOrder((r.sheets as SheetInfo[]) ?? []));
      setReady(true);
    });
    return () => {
      live = false;
    };
  }, [active, scene]);

  // the page you're on: the one you picked, else the first; a deleted page gives way to its neighbour
  const ordered = sheets;
  const lastIndex = useRef(0);
  let page = ordered.find((s) => s.id === pageId);
  if (!page && ordered.length) page = ordered[Math.min(lastIndex.current, ordered.length - 1)];
  if (page) lastIndex.current = ordered.indexOf(page);
  const pid = page?.id ?? null;
  // a viewport that isn't on this page any more is no longer picked
  const vpOk = viewport == null || !!page?.items.some((i) => i.id === viewport);
  useEffect(() => {
    if (!vpOk) setViewport(null);
  }, [vpOk]);

  // the page's drawing: built by the engine after a short pause (a burst of edits draws once)
  useEffect(() => {
    if (!active || pid == null) {
      setIr(null);
      return;
    }
    const my = ++seq.current;
    setDrawing(true);
    const t = window.setTimeout(async () => {
      const r = await exec({ cmd: "sheet_ir", sheet: pid });
      if (my !== seq.current) return;
      setDrawing(false);
      if (r.ok) {
        setIr(r as unknown as Ir);
        setIrError("");
      } else setIrError(r.error ?? "This page couldn't be drawn");
    }, 120);
    return () => window.clearTimeout(t);
  }, [active, pid, scene]);

  const setPage = useCallback((id: number) => {
    setPageId(id);
    setViewport(null);
  }, []);

  const add = async () => {
    const last = ordered[ordered.length - 1];
    const paper = last ? (last.paper.name ? last.paper.name : { w_mm: last.paper.w_mm, h_mm: last.paper.h_mm }) : "a3";
    const project = last?.fields?.project;
    const r = await run({ cmd: "sheet_add", name: newPageName(ordered), paper, ...(project ? { fields: { project } } : {}) });
    if (r.ok && typeof r.sheet === "number") setPage(r.sheet);
  };
  const duplicate = async (id: number) => {
    const r = await run({ cmd: "sheet_duplicate", sheet: id });
    if (r.ok && typeof r.sheet === "number") setPage(r.sheet);
  };
  const remove = async (id: number) => {
    const s = ordered.find((x) => x.id === id);
    const r = await run({ cmd: "sheet_delete", sheet: id });
    if (r.ok) say(`Deleted "${s?.name ?? "page"}"`, false, true);
  };
  const rename = async (id: number, name: string) => {
    const n = name.trim();
    const s = ordered.find((x) => x.id === id);
    if (!n || !s || s.name === n) return;
    await run({ cmd: "sheet_edit", sheet: id, name: n });
  };
  const reorder = async (ids: number[]) => {
    const c = reorderCommand(ids, ordered);
    if (!c) return;
    // show the new order at once; the engine's answer follows
    const by = new Map(ordered.map((s) => [s.id, s]));
    setSheets(ids.map((id, i) => ({ ...by.get(id)!, order: i * 10 })));
    await run(c);
  };

  return { sheets: ordered, ready, page, setPage, viewport: vpOk ? viewport : null, setViewport, ir: ir && ir.sheet === pid ? ir : null, drawing, irError, add, duplicate, remove, rename, reorder };
}
