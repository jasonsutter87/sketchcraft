// The Project texture panel: pick a projection (planar, box, cylinder, sphere), a picture, and its size and turn, and
// watch it land on the selection. Every change runs the real command (taking the last try back first), so what you see
// is what the model will hold; Apply keeps it as one undo step, Cancel takes it back.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Scene } from "./engine";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { useEscapeToClose } from "./sheet";
import type { Sel } from "./selection";
import { clampScale, fitLabelId, initialState, modeHintId, parseNumber, pickMaterial, PROJ_MODES, projectCommand, SCALE_MAX, SCALE_MIN, targetIds, withMode, wrapDegrees, type AxisChoice, type ProjMode, type ProjState } from "./projection";

type Cam = { eye: number[]; target: number[] };

export function ProjectionPanel({ scene, sel, run, camera, onClose, onToast }: { scene: Scene; sel: Sel; run: Run; camera: () => Cam; onClose: () => void; onToast: (t: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  // the selection when the panel opened: the panel is about these, whatever is clicked meanwhile
  const ids = useMemo(() => targetIds(sel), []); // eslint-disable-line react-hooks/exhaustive-deps
  const faceMaterials = useMemo(() => scene.faces.filter((f) => sel.faces.includes(f.id)).map((f) => f.material), []); // eslint-disable-line react-hooks/exhaustive-deps
  const textured = scene.materials.filter((m) => m.texture);
  const [s, setS] = useState<ProjState>(() => initialState("box", pickMaterial(scene.materials, faceMaterials, null)));
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  // the app hands a new function every render: read it through a ref so the preview timer is not restarted by every redraw
  const cam = useRef(camera);
  cam.current = camera;

  // live preview: the newest state wins; each try takes the one before back (one undo step in the end)
  const applied = useRef(false);
  const pending = useRef<Record<string, unknown> | null>(null);
  const working = useRef<Promise<void> | null>(null);
  const pump = useCallback(() => {
    if (working.current) return working.current;
    working.current = (async () => {
      setBusy(true);
      try {
        while (pending.current) {
          const c = pending.current;
          pending.current = null;
          if (applied.current) {
            await run({ cmd: "undo" });
            applied.current = false;
          }
          const r = await run(c);
          applied.current = r.ok;
        }
      } finally {
        working.current = null;
        setBusy(false);
      }
    })();
    return working.current;
  }, [run]);
  useEffect(() => {
    if (!s.material || !ids.length) return;
    const c = projectCommand(s, ids, cam.current());
    if (!c) return;
    const h = window.setTimeout(() => {
      pending.current = c;
      void pump();
    }, 140);
    return () => window.clearTimeout(h);
  }, [s, ids, pump]);

  const settle = async () => {
    while (working.current) await working.current;
  };
  const apply = async () => {
    await settle();
    if (applied.current) onToast(t("proj.done"));
    onClose();
  };
  const cancel = async () => {
    pending.current = null;
    await settle();
    if (applied.current) {
      await run({ cmd: "undo" });
      applied.current = false;
    }
    onClose();
  };
  useEscapeToClose(() => void cancel());
  const clear = async () => {
    pending.current = null;
    await settle();
    if (applied.current) {
      await run({ cmd: "undo" });
      applied.current = false;
    }
    await run({ cmd: "texture_project", faces: ids, mode: "clear" });
    onToast(t("proj.cleared"));
    onClose();
  };

  const set = (patch: Partial<ProjState>) => setS((p) => ({ ...p, ...patch }));
  const fit = s.fit ?? (s.mode === "cylinder" || s.mode === "sphere");
  const axes: AxisChoice[] = s.mode === "planar" ? ["view", "z", "y", "x"] : ["z", "x", "y"];
  const origin = s.origin ?? [NaN, NaN, NaN];
  const setOrigin = (i: number, text: string) => {
    const v = parseNumber(text);
    if (text.trim() === "") {
      const o = [...origin];
      o[i] = NaN;
      return set({ origin: o.every((x) => Number.isNaN(x)) ? null : (o.map((x) => (Number.isNaN(x) ? 0 : x)) as [number, number, number]) });
    }
    if (v == null) return;
    const o = (s.origin ?? [0, 0, 0]).slice() as [number, number, number];
    o[i] = v;
    set({ origin: o });
  };

  return (
    <div className="proj-panel" role="dialog" aria-label={t("proj.title")} data-testid="projection-panel">
      <div className="proj-head">
        <b>{t("proj.title")}</b>
        <span className="muted small">{t("proj.count").replace("{n}", String(ids.length))}</span>
      </div>
      <div className="proj-step" aria-live="polite">
        {textured.length === 0 ? t("proj.nopicture") : t(modeHintId(s.mode))}
      </div>
      <div className="seg" role="radiogroup" aria-label={t("proj.mode")}>
        {PROJ_MODES.map((m: ProjMode) => (
          <button key={m} role="radio" aria-checked={s.mode === m} className={s.mode === m ? "on" : ""} onClick={() => setS((p) => withMode(p, m))} data-mode={m}>
            {t(`proj.mode.${m}`)}
          </button>
        ))}
      </div>
      <label className="field">
        <span>{t("proj.material")}</span>
        <select value={s.material ?? ""} onChange={(e) => set({ material: e.target.value || null })} aria-label={t("proj.material")} disabled={textured.length === 0}>
          {textured.length === 0 && <option value="">{t("proj.none")}</option>}
          {textured.map((m) => (
            <option key={m.name} value={m.name}>
              {m.name.replace(/^[a-z]+:/, "")}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>
          {t("proj.scale")} {Math.round(s.scale * 100)}%
        </span>
        <input type="range" min={Math.log(SCALE_MIN)} max={Math.log(SCALE_MAX)} step={0.02} value={Math.log(clampScale(s.scale))} onChange={(e) => set({ scale: Math.round(Math.exp(Number(e.target.value)) * 100) / 100 })} aria-label={t("proj.scale")} />
      </label>
      <label className="field">
        <span>
          {t("proj.rotation")} {Math.round(s.rotation)}°
        </span>
        <input type="range" min={-180} max={180} step={5} value={wrapDegrees(s.rotation)} onChange={(e) => set({ rotation: Number(e.target.value) })} aria-label={t("proj.rotation")} />
      </label>
      <label className="field check">
        <input type="checkbox" checked={fit} onChange={(e) => set({ fit: e.target.checked })} />
        <span>{t(fitLabelId(s.mode))}</span>
      </label>
      <button className="btn small ghost proj-more" aria-expanded={more} onClick={() => setMore(!more)}>
        {t("proj.more")}
      </button>
      {more && (
        <div className="proj-more-body">
          <div className="seg" role="radiogroup" aria-label={t("proj.axis")}>
            {axes.map((a) => (
              <button key={a} role="radio" aria-checked={s.axis === a} className={s.axis === a ? "on" : ""} onClick={() => set({ axis: a })}>
                {t(`proj.axis.${s.mode === "planar" ? "planar." : ""}${a}`)}
              </button>
            ))}
          </div>
          <div className="proj-origin" title={t("proj.origin.hint")}>
            <span>{t("proj.origin")}</span>
            {["x", "y", "z"].map((a, i) => (
              <input key={a} aria-label={`${t("proj.origin")} ${a}`} placeholder={a} defaultValue={s.origin ? String(s.origin[i]) : ""} onBlur={(e) => setOrigin(i, e.target.value)} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
            ))}
          </div>
        </div>
      )}
      <div className="row gap proj-actions">
        <button className="btn small primary" onClick={() => void apply()} disabled={!s.material || busy} autoFocus>
          {t("proj.apply")}
        </button>
        <button className="btn small ghost" onClick={() => void cancel()}>
          {t("proj.cancel")}
        </button>
        <button className="btn small ghost" onClick={() => void clear()} title={t("proj.clear.tip")}>
          {t("proj.clear")}
        </button>
      </div>
    </div>
  );
}
