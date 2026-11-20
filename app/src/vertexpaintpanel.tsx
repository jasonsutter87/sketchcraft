// The Vertex Paint panel: the brush's size, strength and edge, and Fill / Clear for the selection. The colour is the
// paint colour chip's. Dragging on the model does the painting (Viewport.tsx); this only holds the settings.
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import type { Sel } from "./selection";
import { targetIds } from "./projection";
import { clampRadius, MAX_RADIUS, MIN_RADIUS, type Falloff, type VPaintSettings } from "./vertexpaint";

const FALLOFFS: Falloff[] = ["smooth", "linear", "hard"];

export function VertexPaintPanel({ settings, set, radius, sel, color, run, onToast }: { settings: VPaintSettings; set: (v: VPaintSettings) => void; radius: number; sel: Sel; color: [number, number, number]; run: Run; onToast: (t: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const ids = targetIds(sel);
  const edit = (patch: Partial<VPaintSettings>) => set({ ...settings, ...patch });
  const fill = async () => {
    if (!ids.length) return onToast(t("vp.none"), true);
    const r = await run({ cmd: "vertex_paint", faces: ids, color });
    if (r.ok) onToast(t("vp.filled"));
  };
  const clear = async () => {
    if (!ids.length) return onToast(t("vp.none"), true);
    const r = await run({ cmd: "vertex_paint_clear", faces: ids });
    if (r.ok) onToast(t("vp.cleared"));
  };
  return (
    <div className="proj-panel vp-panel" role="group" aria-label={t("vp.title")} data-testid="vertex-paint-panel">
      <div className="proj-head">
        <b>{t("vp.title")}</b>
        <span className="vp-chip" style={{ background: `rgb(${color.map((c) => Math.round(c * 255)).join(",")})` }} title={t("vp.colorFrom")} />
      </div>
      <div className="proj-step">{t("vp.how")}</div>
      <label className="field">
        <span>
          {t("vp.size")} {radius < 1 ? `${Math.round(radius * 1000) / 10} cm` : `${Math.round(radius * 100) / 100} m`}
          {settings.radius === 0 && <span className="muted small"> ({t("vp.auto")})</span>}
        </span>
        <input
          type="range"
          min={Math.log(MIN_RADIUS)}
          max={Math.log(MAX_RADIUS)}
          step={0.02}
          value={Math.log(clampRadius(radius))}
          onChange={(e) => edit({ radius: clampRadius(Math.exp(Number(e.target.value))) })}
          aria-label={t("vp.size")}
          data-testid="vp-size"
        />
      </label>
      <label className="field">
        <span>
          {t("vp.strength")} {Math.round(settings.strength * 100)}%
        </span>
        <input type="range" min={0.05} max={1} step={0.05} value={settings.strength} onChange={(e) => edit({ strength: Number(e.target.value) })} aria-label={t("vp.strength")} data-testid="vp-strength" />
      </label>
      <div className="seg" role="radiogroup" aria-label={t("vp.falloff")}>
        {FALLOFFS.map((f) => (
          <button key={f} role="radio" aria-checked={settings.falloff === f} className={settings.falloff === f ? "on" : ""} onClick={() => edit({ falloff: f })}>
            {t(`vp.falloff.${f}`)}
          </button>
        ))}
      </div>
      <div className="row gap proj-actions">
        <button className="btn small" onClick={() => void fill()} data-testid="vp-fill">
          {t("vp.fill")}
        </button>
        <button className="btn small ghost" onClick={() => void clear()} data-testid="vp-clear">
          {t("vp.clear")}
        </button>
      </div>
    </div>
  );
}
