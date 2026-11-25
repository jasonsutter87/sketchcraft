// 3D mouse settings: use it or not, how fast, how far the cap must move first, which axes are flipped, and whether
// orbit or pan is locked. Changes save and apply at once (the live store is spacemousestore.ts).

import { useLocale } from "./locale";
import { Sheet, SheetHead } from "./sheet";
import { AXES, DEADZONE_RANGE, DEFAULT_SPACEMOUSE, SPEED_RANGE, type SpaceMouseSettings } from "./spacemouse";
import { inTauri } from "./engine";
import { spaceMouse, useSpaceMouse } from "./spacemousestore";

export function SpaceMouseDialog({ onClose }: { onClose: () => void }) {
  const { t } = useLocale();
  const { settings: s, status } = useSpaceMouse();
  const set = (patch: Partial<SpaceMouseSettings>) => spaceMouse.set({ ...s, ...patch });
  const state = !inTauri ? t("sm.status.browser") : !s.enabled ? t("sm.status.off") : status.connected ? t("sm.status.on").replace("{name}", status.name) : t("sm.status.none");
  const locks: SpaceMouseSettings["lock"][] = ["none", "rotation", "pan"];
  return (
    <Sheet onClose={onClose} label={t("sm.title")} className="spacemouse" width={460}>
      <SheetHead title={t("sm.title")} note={t("sm.note")} />
      <p className="muted" role="status" id="sm-status">
        {state}
      </p>
      <label className="field check">
        <input type="checkbox" checked={s.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
        <span>{t("sm.enabled")}</span>
      </label>
      <label className="field">
        <span>
          {t("sm.speed")} ({s.speed.toFixed(1)}×)
        </span>
        <input type="range" min={SPEED_RANGE[0]} max={SPEED_RANGE[1]} step={0.1} value={s.speed} onChange={(e) => set({ speed: Number(e.target.value) })} />
      </label>
      <label className="field">
        <span>
          {t("sm.deadzone")} ({Math.round(s.deadzone * 100)}%)
        </span>
        <input type="range" min={DEADZONE_RANGE[0]} max={DEADZONE_RANGE[1]} step={0.01} value={s.deadzone} onChange={(e) => set({ deadzone: Number(e.target.value) })} />
      </label>
      <label className="field check">
        <input type="checkbox" checked={s.dominant} onChange={(e) => set({ dominant: e.target.checked })} />
        <span>{t("sm.dominant")}</span>
      </label>
      <div className="section-title">{t("sm.lock")}</div>
      <div className="seg" role="radiogroup" aria-label={t("sm.lock")}>
        {locks.map((l) => (
          <button key={l} role="radio" aria-checked={s.lock === l} className={s.lock === l ? "on" : ""} onClick={() => set({ lock: l })}>
            {t(`sm.lock.${l}`)}
          </button>
        ))}
      </div>
      <div className="section-title" style={{ marginTop: 10 }}>
        {t("sm.invert")}
      </div>
      <div className="row gap" style={{ flexWrap: "wrap" }}>
        {AXES.map((a) => (
          <label key={a} className="field check">
            <input type="checkbox" checked={s.invert[a]} onChange={(e) => set({ invert: { ...s.invert, [a]: e.target.checked } })} />
            <span>{t(`sm.axis.${a}`)}</span>
          </label>
        ))}
      </div>
      <p className="muted">{t("sm.buttons")}</p>
      <div className="row gap" style={{ marginTop: 12 }}>
        <button className="btn small" onClick={() => spaceMouse.set({ ...DEFAULT_SPACEMOUSE })}>
          {t("sm.reset")}
        </button>
        <button className="btn small ghost" onClick={onClose}>
          {t("sm.close")}
        </button>
      </div>
    </Sheet>
  );
}
