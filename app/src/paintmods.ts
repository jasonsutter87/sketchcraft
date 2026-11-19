// What a Paint Bucket click reaches, from the keys held (the same modifiers SketchUp's bucket uses).
//   click          the clicked face (or group)
//   Ctrl / Option  every touching face with the same material, the picture running on across the edges
//   Shift          every face in the group with the same material
//   Ctrl+Shift     every face with the same material on the whole connected mesh (crossing other materials)
//   ⌘ (Alt on Windows and Linux)   pick up the material instead of painting

export type PaintReach = "sample" | "face" | "connected" | "all" | "object";

export interface PaintKeys {
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

export function paintReach(e: PaintKeys, mac: boolean): PaintReach {
  if (mac ? e.metaKey : e.altKey || e.metaKey) return "sample";
  const connected = e.ctrlKey || (mac && e.altKey);
  if (connected && e.shiftKey) return "object";
  if (connected) return "connected";
  if (e.shiftKey) return "all";
  return "face";
}

/** The engine's `paint` command for a click that reached `reach` from face `id`. */
export function paintCommand(reach: PaintReach, id: number, what: { material: string | null; color: [number, number, number] }) {
  const base = what.material ? { cmd: "paint", ids: [id], material: what.material } : { cmd: "paint", ids: [id], color: what.color };
  return reach === "connected" || reach === "all" || reach === "object" ? { ...base, scope: reach } : base;
}
