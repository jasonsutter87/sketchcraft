// Tools > Print check…: the report as a short list, with Fix model when the model is not solid and buttons that pick the
// problem faces so you can see them. The engine does the work (print_check); the lines are printcheck.ts.
import { useCallback, useEffect, useState } from "react";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { PRINT_DEFAULTS, printCommand, printLines, problemSelection, type PrintOptions, type PrintReply } from "./printcheck";
import type { Sel } from "./selection";
import { Sheet, SheetHead } from "./sheet";

export function PrintCheckDialog({ sel, run, onFix, onSelect, onClose }: { sel: Sel; run: Run; onFix: () => Promise<void>; onSelect: (s: Partial<Sel>) => void; onClose: () => void }) {
  const { t } = useLocale();
  const [o, setO] = useState<PrintOptions>(PRINT_DEFAULTS);
  const [r, setR] = useState<PrintReply | null>(null);
  const check = useCallback(
    async (opts: PrintOptions) => {
      const reply = await run(printCommand(sel, opts));
      if (reply.ok) setR(reply as PrintReply);
    },
    [run, sel],
  );
  useEffect(() => {
    void check(PRINT_DEFAULTS);
  }, [check]);
  const lines = r ? printLines(r, o, t) : [];
  const num = (v: string, d: number) => (Number.isFinite(Number(v)) && v.trim() !== "" ? Math.max(0, Number(v)) : d);
  const pick = (kind: "thin" | "over" | "open") => {
    if (r) onSelect(problemSelection(r, kind));
  };
  const has = (kind: "thin" | "over" | "open") => {
    if (!r) return false;
    const p = problemSelection(r, kind);
    return (p.faces?.length ?? 0) + (p.edges?.length ?? 0) > 0;
  };
  return (
    <Sheet onClose={onClose} label={t("print.title")} width={460}>
      <SheetHead title={t("print.title")} note={t("print.note")} />
      <ul className="print-lines" style={{ listStyle: "none", padding: 0, margin: "8px 0" }}>
        {lines.map((l) => (
          <li key={l.key} data-ok={l.ok ? "1" : "0"} style={{ display: "flex", gap: 8, padding: "3px 0" }}>
            <span aria-hidden style={{ width: 16, color: l.ok ? (l.warn ? "var(--warn, #d9a400)" : "var(--ok, #2fa84f)") : "var(--bad, #e5383b)" }}>
              {l.ok ? (l.warn ? "!" : "✓") : "✗"}
            </span>
            <span>{l.text}</span>
          </li>
        ))}
      </ul>
      {r?.ready && <p style={{ margin: "4px 0 8px" }}><b>{t("print.ready")}</b></p>}
      <div className="row gap" style={{ flexWrap: "wrap", margin: "8px 0" }}>
        <label className="field" style={{ flex: 1 }}>
          <span>{t("print.minWall")}</span>
          <input type="number" min={0} step={0.1} value={o.minWall} onChange={(e) => setO({ ...o, minWall: num(e.target.value, o.minWall) })} aria-label={t("print.minWall")} />
        </label>
        <label className="field" style={{ flex: 1 }}>
          <span>{t("print.angle")}</span>
          <input type="number" min={0} max={90} step={5} value={o.angle} onChange={(e) => setO({ ...o, angle: Math.min(90, num(e.target.value, o.angle)) })} aria-label={t("print.angle")} />
        </label>
        <button className="btn small" onClick={() => void check(o)}>{t("print.recheck")}</button>
      </div>
      <div className="row gap" style={{ flexWrap: "wrap" }}>
        {r && !r.solid && r.size_mm && (
          <button
            className="btn small primary"
            autoFocus
            onClick={async () => {
              await onFix();
              await check(o);
            }}
          >
            {t("menu.fixModel")}
          </button>
        )}
        {has("open") && <button className="btn small" onClick={() => pick("open")}>{t("print.selectOpen")}</button>}
        {has("thin") && <button className="btn small" onClick={() => pick("thin")}>{t("print.selectThin")}</button>}
        {has("over") && <button className="btn small" onClick={() => pick("over")}>{t("print.selectOver")}</button>}
        <button className="btn small ghost" onClick={onClose}>{t("print.close")}</button>
      </div>
    </Sheet>
  );
}
