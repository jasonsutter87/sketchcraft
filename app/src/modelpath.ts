/** A path that ⌘S may write to: a SketchCraft model file, never something else the storage was made to name. */
export function isModelPath(p: string | null | undefined): p is string {
  return !!p && p.length < 4096 && !p.includes("\0") && p.toLowerCase().endsWith(".sketchcraft");
}
