// The Texture Paint panel: the brush (size, hardness, opacity, the paint colour chip), a paint texture for the selection
// (size, base, one-click unwrap) and Fill / Export for the picture. Dragging on the model paints (Viewport.tsx); the UV
// editor paints on the flat picture. The panel only holds the settings and sends commands.
import { useState } from "react";
import { exec, type Scene } from "./engine";
import { pickSave } from "./files";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { targetIds } from "./projection";
import type { Sel } from "./selection";
import { clampRadius, MAX_RADIUS, MIN_RADIUS } from "./vertexpaint";
import { paintMaterialOf, type TPaintSettings } from "./texturepaint";

const SIZES = [512, 1024, 2048];

export function TexturePaintPanel({ scene, settings, set, radius, sel, color, run, onToast, openUv }: { scene: Scene; settings: TPaintSettings; set: (v: TPaintSettings) => void; radius: number; sel: Sel; color: [number, number, number]; run: Run; onToast: (t: string, bad?: boolean) => void; openUv: () => void }) {
  const { t } = useLocale();
  const ids = targetIds(sel);
  const [size, setSize] = useState(1024);
  const [base, setBase] = useState<"color" | "material">("color");
  const [needs, setNeeds] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const edit = (patch: Partial<TPaintSettings>) => set({ ...settings, ...patch });
  const mat = paintMaterialOf(scene, sel);

  const create = async (unwrap: boolean) => {
    if (!ids.length) return onToast(t("tp.none"), true);
    setBusy(true);
    try {
      if (!unwrap) {
        const st = await exec({ cmd: "texture_paint_status", faces: ids });
        if (!st.ok) return void onToast(String(st.error ?? ""), true);
        const without = (st.without as number[]) ?? [];
        if (without.length || st.overlap || st.outside) {
          const why = [without.length ? t("tp.needs.without").replace("{n}", String(without.length)) : "", st.overlap ? t("tp.needs.overlap") : "", st.outside ? t("tp.needs.outside") : ""].filter(Boolean).join(" ");
          return setNeeds(why);
        }
      }
      const r = await run({ cmd: "texture_paint_new", faces: ids, size, unwrap, base: base === "material" ? "material" : color });
      if (r.ok) {
        setNeeds(null);
        onToast(t("tp.created").replace("{name}", String(r.material)));
      }
    } finally {
      setBusy(false);
    }
  };
  const fill = async () => {
    if (!mat) return onToast(t("tp.nomat"), true);
    const r = await run({ cmd: "texture_paint_stroke", material: mat, faces: ids, fill: true, color });
    if (r.ok) onToast(t("tp.filled"));
  };
  const exportPng = async () => {
    if (!mat) return onToast(t("tp.nomat"), true);
    await exportTexture(scene, mat, run, onToast, t("tp.exported"));
  };
  return (
    <div className="proj-panel vp-panel tp-panel" role="group" aria-label={t("tp.title")} data-testid="texture-paint-panel">
      <div className="proj-head">
        <b>{t("tp.title")}</b>
        <span className="vp-chip" style={{ background: `rgb(${color.map((c) => Math.round(c * 255)).join(",")})` }} title={t("vp.colorFrom")} />
      </div>
      <div className="proj-step" data-testid="tp-step">
        {mat ? t("tp.how") : t("tp.how.new")}
      </div>
      <label className="field">
        <span>
          {t("vp.size")} {radius < 1 ? `${Math.round(radius * 1000) / 10} cm` : `${Math.round(radius * 100) / 100} m`}
          {settings.radius === 0 && <span className="muted small"> ({t("vp.auto")})</span>}
        </span>
        <input type="range" min={Math.log(MIN_RADIUS)} max={Math.log(MAX_RADIUS)} step={0.02} value={Math.log(clampRadius(radius))} onChange={(e) => edit({ radius: clampRadius(Math.exp(Number(e.target.value))) })} aria-label={t("vp.size")} data-testid="tp-size" />
      </label>
      <label className="field">
        <span>
          {t("tp.hardness")} {Math.round(settings.hardness * 100)}%
        </span>
        <input type="range" min={0} max={1} step={0.05} value={settings.hardness} onChange={(e) => edit({ hardness: Number(e.target.value) })} aria-label={t("tp.hardness")} data-testid="tp-hardness" />
      </label>
      <label className="field">
        <span>
          {t("tp.opacity")} {Math.round(settings.opacity * 100)}%
        </span>
        <input type="range" min={0.05} max={1} step={0.05} value={settings.opacity} onChange={(e) => edit({ opacity: Number(e.target.value) })} aria-label={t("tp.opacity")} data-testid="tp-opacity" />
      </label>
      <div className="row gap proj-actions">
        <button className="btn small" onClick={() => void fill()} disabled={!mat} title={t("tp.fill.tip")} data-testid="tp-fill">
          {t("tp.fill")}
        </button>
        <button className="btn small ghost" onClick={() => void exportPng()} disabled={!mat} title={t("tp.export.tip")} data-testid="tp-export">
          {t("tp.export")}
        </button>
        <button className="btn small ghost" onClick={openUv} title={t("tp.uv.tip")} data-testid="tp-uv">
          {t("tp.uv")}
        </button>
      </div>
      <div className="proj-step tp-new" data-testid="tp-new">
        <b>{t("tp.new")}</b>
        <div className="row gap">
          <select value={size} onChange={(e) => setSize(Number(e.target.value))} aria-label={t("tp.size")} title={t("tp.size")} data-testid="tp-newsize">
            {SIZES.map((s) => (
              <option key={s} value={s}>
                {s} × {s}
              </option>
            ))}
          </select>
          <select value={base} onChange={(e) => setBase(e.target.value as "color" | "material")} aria-label={t("tp.base")} title={t("tp.base.tip")} data-testid="tp-newbase">
            <option value="color">{t("tp.base.color")}</option>
            <option value="material">{t("tp.base.material")}</option>
          </select>
        </div>
        {needs && (
          <div className="tp-warn" data-testid="tp-needs">
            {needs} {t("tp.needs.unwrap")}
          </div>
        )}
        <div className="row gap">
          {needs ? (
            <button className="btn small primary" onClick={() => void create(true)} disabled={busy} data-testid="tp-unwrap-new">
              {t("tp.unwrapNew")}
            </button>
          ) : (
            <button className="btn small primary" onClick={() => void create(false)} disabled={busy} data-testid="tp-create">
              {t("tp.create")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Save a material's picture as a PNG file (a picture that is not a PNG is converted). */
export async function exportTexture(scene: Scene, material: string, run: Run, onToast: (t: string, bad?: boolean) => void, doneText: string) {
  const m = scene.materials.find((x) => x.name === material);
  if (!m?.texture) return onToast("No picture", true);
  const name = `${material.replace(/[^A-Za-z0-9_-]+/g, "_") || "texture"}.png`;
  const path = await pickSave(name, "png", "PNG image");
  if (!path) return;
  let data = m.texture.data;
  if (!data.startsWith("data:image/png")) {
    data = await new Promise<string>((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const c = document.createElement("canvas");
        c.width = img.naturalWidth;
        c.height = img.naturalHeight;
        c.getContext("2d")!.drawImage(img, 0, 0);
        resolve(c.toDataURL("image/png"));
      };
      img.onerror = () => reject(new Error("unreadable"));
      img.src = data;
    });
  }
  const r = await run({ cmd: "save_image", path, data });
  if (r.ok) onToast(doneText);
}
