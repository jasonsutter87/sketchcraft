// Add a reference image: a photo or drawing on a board (front, side or top),
// see-through and locked, so you can model over it (and see a SubD cage
// through it). Boards land centered on what's in the model.

import { useState } from "react";
import { type Reply, type Scene } from "./engine";
import { prepareImage } from "./textures";

type Run = (cmd: Record<string, unknown>) => Promise<Reply>;
type Plane = "front" | "side" | "top";

/** The image turned a quarter (for the top board, so it reads the right way up). */
function quarterTurn(data: string, w: number, h: number): Promise<string> {
  return new Promise((ok) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      c.width = h;
      c.height = w;
      const ctx = c.getContext("2d")!;
      // texture (x, y) shows image (w - y, x): see the board math in tools/blender
      ctx.setTransform(0, -1, 1, 0, 0, w);
      ctx.drawImage(img, 0, 0);
      ok(c.toDataURL("image/jpeg", 0.9));
    };
    img.src = data;
  });
}

export function RefImageDialog({ scene, run, onClose, onToast }: { scene: Scene; run: Run; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [plane, setPlane] = useState<Plane>("side");
  const [width, setWidth] = useState("5");
  const [busy, setBusy] = useState(false);
  const place = async () => {
    if (!file) return;
    const w = Number(width);
    if (!(w > 0)) return onToast("Give the image's width in meters", true);
    setBusy(true);
    try {
      const img = await prepareImage(file, 2048);
      const h = (w * img.height) / img.width;
      // centered on the model (or the origin), set back so it's behind it
      const xs = scene.positions.filter((_, i) => i % 3 === 0), ys = scene.positions.filter((_, i) => i % 3 === 1);
      const mid = (a: number[]) => (a.length ? (Math.min(...a) + Math.max(...a)) / 2 : 0);
      const span = (a: number[]) => (a.length ? Math.max(...a) - Math.min(...a) : 4);
      const cx = mid(xs), cy = mid(ys);
      const name = `Reference: ${file.name.replace(/\.[^.]+$/, "")}`;
      let corners: number[][];
      let data = img.data;
      let tw = w, th = h, project: number[], origin: number[];
      if (plane === "front") {
        // facing the front camera (-Y), behind the model
        const y = cy + span(ys) / 2 + 1;
        corners = [[cx - w / 2, y, 0], [cx + w / 2, y, 0], [cx + w / 2, y, h], [cx - w / 2, y, h]];
        project = [0, 1, 0];
        origin = corners[0];
      } else if (plane === "side") {
        // facing the right camera (+X), behind the model
        const x = cx - span(xs) / 2 - 1;
        corners = [[x, cy - w / 2, 0], [x, cy + w / 2, 0], [x, cy + w / 2, h], [x, cy - w / 2, h]];
        project = [-1, 0, 0];
        origin = corners[0];
      } else {
        // on the ground, under the model
        corners = [[cx - w / 2, cy - h / 2, -0.01], [cx + w / 2, cy - h / 2, -0.01], [cx + w / 2, cy + h / 2, -0.01], [cx - w / 2, cy + h / 2, -0.01]];
        data = await quarterTurn(img.data, img.width, img.height);
        [tw, th] = [h, w];
        project = [0, 0, -1];
        origin = corners[0];
      }
      const g = (await run({ cmd: "begin_group", name })).group as number;
      await run({ cmd: "polygon", points: corners });
      await run({ cmd: "end_group" });
      await run({ cmd: "texture", name, data, width: tw, height: th, project, origin });
      await run({ cmd: "paint", ids: [g], material: name });
      await run({ cmd: "opacity", material: name, value: 0.5 });
      await run({ cmd: "tag", ids: [g], tag: "References" });
      await run({ cmd: "tag_visible", tag: "References", visible: true });
      await run({ cmd: "lock", ids: [g] });
      onToast(`${name}: locked and see-through. Its tag hides all your references at once.`);
      onClose();
    } catch (e) {
      onToast(String(e), true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" style={{ width: "min(520px, calc(100vw - 32px))" }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Add a reference image">
        <div className="sheet-head">
          <b>Reference image</b>
          <span className="muted">A photo or drawing to model over: see-through and locked.</span>
        </div>
        <label className="field">
          <span>Image</span>
          <input type="file" accept="image/*" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
        </label>
        <div className="field">
          <span>Which view</span>
          <div className="seg" role="radiogroup">
            {(["front", "side", "top"] as Plane[]).map((p) => (
              <button key={p} className={plane === p ? "on" : ""} onClick={() => setPlane(p)} aria-pressed={plane === p}>
                {p === "front" ? "Front" : p === "side" ? "Side" : "Top"}
              </button>
            ))}
          </div>
        </div>
        <label className="field">
          <span>Width (meters): how wide the picture is in real life</span>
          <input className="num" value={width} onChange={(e) => setWidth(e.target.value)} />
        </label>
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="btn small" disabled={!file || busy} onClick={place}>
            {busy ? "Placing…" : "Place it"}
          </button>
          <span style={{ flex: 1 }} />
          <button className="btn small ghost" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
