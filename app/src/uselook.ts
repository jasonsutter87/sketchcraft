// Skin, layout and workspace customizations, remembered between launches.
import { useEffect, useState } from "react";
import { applySkin, readSkin, SKINS, store, type Skin } from "./skins";
import { cleanEdits, toolAfterSwitch, WORKSPACES, withEdits, type Layout, type LeftTab, type WorkspaceEdits, type WorkspaceId } from "./workspace";
import type { Styles, Tool } from "./Viewport";

/** the skin you wore last time (a built-in by id, or a saved one by its data) */
function rememberedSkin(): Skin {
  const id = store.get("sketchcraft.skin");
  const built = SKINS.find((s) => s.id === id);
  if (built) return built;
  try {
    const data = store.get("sketchcraft.skinData");
    const s = data ? readSkin(JSON.parse(data), "My skin") : null;
    if (s) return s;
  } catch {
    /* fall through */
  }
  return SKINS[0];
}
function rememberedEdits(): Partial<Record<WorkspaceId, WorkspaceEdits>> {
  try {
    return cleanEdits(JSON.parse(store.get("sketchcraft.workspaceEdits") ?? "{}"));
  } catch {
    return {};
  }
}

export type Tab = LeftTab;

export function useLook({ tool, chooseTool, setStyles, setTab }: {
  tool: Tool;
  chooseTool: (t: Tool) => void;
  setStyles: (f: (s: Styles) => Styles) => void;
  setTab: (t: Tab) => void;
}) {
  // skin and workspace, remembered between launches
  const [skin, setSkinState] = useState<Skin>(rememberedSkin);
  useEffect(() => applySkin(skin), [skin]);
  const wearSkin = (s: Skin, useBackdrop: boolean) => {
    setSkinState(s);
    store.set("sketchcraft.skin", s.id);
    store.set("sketchcraft.skinData", JSON.stringify(s));
    if (useBackdrop) setStyles((st) => ({ ...st, background: s.backdrop ?? "default" }));
    // picked from the gallery: it brings its layout (Classic docks, ours float)
    if (useBackdrop && s.layout) setLayout(s.layout);
  };
  const [wsId, setWsId] = useState<WorkspaceId>(() => (WORKSPACES.some((w) => w.id === store.get("sketchcraft.workspace")) ? (store.get("sketchcraft.workspace") as WorkspaceId) : "model"));
  const [wsEdits, setWsEdits] = useState(rememberedEdits);
  const [layout, setLayoutState] = useState<Layout>(() => {
    const v = store.get("sketchcraft.layout");
    return v === "classic" || v === "floating" || v === "side" ? v : (rememberedSkin().layout ?? "classic");
  });
  const setLayout = (l: Layout) => {
    setLayoutState(l);
    store.set("sketchcraft.layout", l);
    // the layout applies everywhere: drop per-workspace tool bar placements
    setWsEdits((all) => {
      const next = Object.fromEntries(Object.entries(all).map(([k, v]) => [k, v ? (({ dock: _d, ...rest }) => rest)(v) : v]));
      store.set("sketchcraft.workspaceEdits", JSON.stringify(next));
      return next;
    });
  };
  const ws = withEdits(WORKSPACES.find((w) => w.id === wsId)!, wsEdits[wsId], layout);
  const editWs = (e: WorkspaceEdits | null) => {
    const next = { ...wsEdits };
    if (e) next[wsId] = { ...next[wsId], ...e };
    else delete next[wsId];
    setWsEdits(next);
    store.set("sketchcraft.workspaceEdits", JSON.stringify(next));
    if (e?.leftTab) setTab(e.leftTab);
  };
  const switchWs = (id: WorkspaceId) => {
    const w = withEdits(WORKSPACES.find((x) => x.id === id)!, wsEdits[id]);
    setWsId(id);
    store.set("sketchcraft.workspace", id);
    setTab(w.leftTab);
    if (w.styles) setStyles((s) => ({ ...s, ...w.styles }));
    // a tool that isn't on this workspace's toolbar: back to Select (Layout leaves the model's tool alone)
    const next = toolAfterSwitch(w, tool);
    if (next !== tool) chooseTool(next);
  };
  return { skin, setSkinState, wearSkin, wsId, setWsId, wsEdits, setWsEdits, layout, setLayoutState, setLayout, ws, editWs, switchWs };
}
