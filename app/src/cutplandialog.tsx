// Tools > Cut plan…: the Cut list's parts packed onto sheets and boards. Three views: the cutting diagram, the stock
// and prices form, and the shopping summary, with the diagrams and the part labels as PDF files. The engine does the work
// (cut_stock, cut_plan, export_cut_diagrams, export_cut_labels); the numbers and shapes are cutplan.ts.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { pickSave } from "./files";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import {
  boardDiagram,
  formFromStock,
  lengthWords,
  money,
  planCommand,
  sheetDiagram,
  stockCommand,
  stockTitle,
  stockUnits,
  usedMaterials,
  type CutPlanReply,
  type Diagram,
  type StockForm,
} from "./cutplan";
import type { Sel } from "./selection";
import { Sheet, SheetHead } from "./sheet";

type View = "diagram" | "stock" | "buy";
const TINTS = ["#ecdec4", "#dee8d2", "#d2e0ec", "#eed6d6"];

function DiagramSvg({ d, label }: { d: Diagram; label: string }) {
  return (
    <svg className="cut-diagram" role="img" aria-label={label} viewBox={`0 0 ${d.width} ${d.height}`} width="100%" style={{ maxHeight: "52vh", background: "#faf7f0", border: "1px solid var(--line)" }}>
      <rect x={d.stock.x} y={d.stock.y} width={d.stock.w} height={d.stock.h} fill="#faf7f0" stroke="#1a1a1f" strokeWidth={1.2} />
      {d.shapes.map((s) => (
        <g key={s.key} data-part={s.number}>
          <rect x={s.x} y={s.y} width={s.w} height={s.h} fill={TINTS[s.tint]} stroke="#1a1a1f" strokeWidth={0.8} />
          {s.w > 26 && s.h > 14 && (
            <text x={s.x + s.w / 2} y={s.y + s.h / 2 - (s.h > 40 ? 4 : 0)} textAnchor="middle" dominantBaseline="middle" fontSize={Math.min(13, s.h * 0.4)} fontWeight={600} fill="#1a1a1f">
              #{s.number}
              {s.h <= 40 && s.w > 110 ? ` ${s.name}` : ""}
            </text>
          )}
          {s.w > 70 && s.h > 40 && (
            <text x={s.x + s.w / 2} y={s.y + s.h / 2 + 10} textAnchor="middle" fontSize={10} fill="#1a1a1f">
              {s.name} · {s.size}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}

export function CutPlanDialog({ sel, units, run, onClose, onSay }: { sel: Sel; units: string; run: Run; onClose: () => void; onSay: (text: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const su = stockUnits(units);
  const [view, setView] = useState<View>("diagram");
  const [plan, setPlan] = useState<CutPlanReply | null>(null);
  const [form, setForm] = useState<StockForm | null>(null);
  const [pick, setPick] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // the plan is asked for again when the picked parts change (by id, not by the selection object), never on a redraw
  const ask = JSON.stringify(planCommand(sel));
  const runRef = useRef(run);
  runRef.current = run;
  const load = useCallback(async () => {
    const r = await runRef.current(JSON.parse(ask));
    if (!r.ok) return;
    const p = r as unknown as CutPlanReply;
    setPlan(p);
    setForm((f) => f ?? formFromStock(p.stock, su, usedMaterials(p)));
  }, [ask, su]);
  useEffect(() => {
    void load();
  }, [load]);

  const stops = useMemo(() => (plan ? [...plan.sheets.map((s) => ({ kind: "sheet" as const, i: s.index })), ...plan.boards.map((b) => ({ kind: "board" as const, i: b.index }))] : []), [plan]);
  const current = stops[Math.min(pick, stops.length - 1)];

  const apply = async () => {
    if (!form) return;
    const made = stockCommand(form, su);
    if ("error" in made) {
      setError(t(`cut.err.${made.error}`));
      return;
    }
    setError(null);
    const r = await run(made.cmd);
    if (!r.ok) {
      setError(r.error ?? t("cut.err.generic"));
      return;
    }
    setForm(null);
    await load();
    setView("diagram");
  };

  const save = async (kind: "diagrams" | "svg" | "labels") => {
    const ext = kind === "svg" ? "svg" : "pdf";
    const path = await pickSave(`${t(kind === "labels" ? "cut.labelsFile" : "cut.diagramsFile")}.${ext}`, ext, ext.toUpperCase());
    if (!path) return;
    const ids = [...sel.groups, ...sel.instances];
    const r = await run({ cmd: kind === "labels" ? "export_cut_labels" : "export_cut_diagrams", path, ...(ids.length ? { ids } : {}) });
    if (r.ok) onSay(t("cut.saved").replace("{file}", path.split("/").pop() ?? path));
  };

  const imperial = su === "in";
  const body = () => {
    if (!plan) return null;
    if (plan.summary.parts === 0) return <p>{t("cut.none")}</p>;
    if (view === "stock" && form) {
      const set = (k: keyof StockForm, v: string) => setForm({ ...form, [k]: v });
      const unit = imperial ? "in" : "mm";
      return (
        <div className="cut-stock">
          <div className="row gap" style={{ flexWrap: "wrap" }}>
            <label className="field">
              <span>{t("cut.kerf")} ({unit})</span>
              <input value={form.kerf} onChange={(e) => set("kerf", e.target.value)} aria-label={t("cut.kerf")} inputMode="decimal" />
            </label>
            <label className="field">
              <span>{t("cut.trim")} ({unit})</span>
              <input value={form.trim} onChange={(e) => set("trim", e.target.value)} aria-label={t("cut.trim")} inputMode="decimal" />
            </label>
            <label className="field">
              <span>{t("cut.sheetSize")} ({unit})</span>
              <input value={form.sheet} onChange={(e) => set("sheet", e.target.value)} aria-label={t("cut.sheetSize")} />
            </label>
            <label className="field">
              <span>{t("cut.boardLengths")} ({unit})</span>
              <input value={form.boardLengths} onChange={(e) => set("boardLengths", e.target.value)} aria-label={t("cut.boardLengths")} />
            </label>
            <label className="field" style={{ width: 64 }}>
              <span>{t("cut.currency")}</span>
              <input value={form.currency} maxLength={8} onChange={(e) => set("currency", e.target.value)} aria-label={t("cut.currency")} />
            </label>
          </div>
          <table className="cut-prices" style={{ width: "100%", margin: "10px 0" }}>
            <thead>
              <tr>
                <th style={{ textAlign: "left" }}>{t("cut.material")}</th>
                <th>{t("cut.pricePerSheet")}</th>
                <th>{t("cut.pricePerBoard")}</th>
                <th>{t("cut.pricePerBf")}</th>
                <th>{t("cut.grain")}</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(form.materials).map(([name, m]) => {
                const edit = (patch: Partial<typeof m>) => setForm({ ...form, materials: { ...form.materials, [name]: { ...m, ...patch } } });
                return (
                  <tr key={name} data-material={name}>
                    <td>{name || t("cut.noMaterial")}</td>
                    <td><input value={m.sheetPrice} inputMode="decimal" onChange={(e) => edit({ sheetPrice: e.target.value })} aria-label={`${name || t("cut.noMaterial")} ${t("cut.pricePerSheet")}`} /></td>
                    <td><input value={m.boardPrice} inputMode="decimal" onChange={(e) => edit({ boardPrice: e.target.value })} aria-label={`${name || t("cut.noMaterial")} ${t("cut.pricePerBoard")}`} /></td>
                    <td><input value={m.boardFootPrice} inputMode="decimal" onChange={(e) => edit({ boardFootPrice: e.target.value })} aria-label={`${name || t("cut.noMaterial")} ${t("cut.pricePerBf")}`} /></td>
                    <td style={{ textAlign: "center" }}><input type="checkbox" checked={m.grain} onChange={(e) => edit({ grain: e.target.checked })} aria-label={`${name || t("cut.noMaterial")} ${t("cut.grain")}`} /></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <p className="muted" style={{ margin: "4px 0" }}>{t("cut.stockNote")}</p>
          {error && <p role="alert" style={{ color: "var(--bad, #e5383b)" }}>{error}</p>}
          <div className="row gap">
            <button className="btn small primary" onClick={() => void apply()}>{t("cut.apply")}</button>
          </div>
        </div>
      );
    }
    if (view === "buy") {
      return (
        <div className="cut-buy">
          <table style={{ width: "100%" }}>
            <thead>
              <tr>
                <th style={{ textAlign: "left" }}>{t("cut.material")}</th>
                <th>{t("cut.sheets")}</th>
                <th>{t("cut.boards")}</th>
                <th>{t("cut.boardFeet")}</th>
                <th>{t("cut.cost")}</th>
              </tr>
            </thead>
            <tbody>
              {plan.cost.by_material.map((c) => (
                <tr key={c.material} data-material={c.material}>
                  <td>{c.material || t("cut.noMaterial")}</td>
                  <td style={{ textAlign: "center" }}>{c.sheets}</td>
                  <td style={{ textAlign: "center" }}>{c.boards}</td>
                  <td style={{ textAlign: "center" }}>{c.board_feet.toFixed(2)}</td>
                  <td style={{ textAlign: "center" }}>{c.priced ? money(plan.cost.currency, c.cost) : t("cut.noPrice")}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p style={{ margin: "10px 0 4px" }} data-total={plan.cost.total}>
            <b>{t("cut.total")}: {money(plan.cost.currency, plan.cost.total)}</b> · {t("cut.waste").replace("{pct}", String(plan.summary.waste_pct))}
          </p>
        </div>
      );
    }
    // the cutting diagram
    const sheet = current?.kind === "sheet" ? plan.sheets.find((s) => s.index === current.i) : undefined;
    const board = current?.kind === "board" ? plan.boards.find((b) => b.index === current.i) : undefined;
    return (
      <div className="cut-view">
        {stops.length > 1 && (
          <label className="field" style={{ marginBottom: 6 }}>
            <span>{t("cut.show")}</span>
            <select value={Math.min(pick, stops.length - 1)} onChange={(e) => setPick(Number(e.target.value))} aria-label={t("cut.show")}>
              {stops.map((s, i) => {
                const sh = s.kind === "sheet" ? plan.sheets.find((x) => x.index === s.i) : undefined;
                const bd = s.kind === "board" ? plan.boards.find((x) => x.index === s.i) : undefined;
                return (
                  <option key={`${s.kind}${s.i}`} value={i}>
                    {sh ? stockTitle("sheet", sh.index, plan.sheets.length, sh.material, t) : bd ? stockTitle("board", bd.index, plan.boards.length, bd.material, t, bd.stock) : ""}
                  </option>
                );
              })}
            </select>
          </label>
        )}
        {sheet && (
          <>
            <p className="muted" style={{ margin: "2px 0 6px" }}>
              {stockTitle("sheet", sheet.index, plan.sheets.length, sheet.material, t)} · {lengthWords(sheet.size[0], su)} × {lengthWords(sheet.size[1], su)} · {t("cut.waste").replace("{pct}", String(sheet.waste_pct))}
              {sheet.grain ? ` · ${t("cut.grainLong")}` : ""}
            </p>
            <DiagramSvg d={sheetDiagram(sheet, 720, su)} label={stockTitle("sheet", sheet.index, plan.sheets.length, sheet.material, t)} />
          </>
        )}
        {board && (
          <>
            <p className="muted" style={{ margin: "2px 0 6px" }}>
              {stockTitle("board", board.index, plan.boards.length, board.material, t, board.stock)} · {lengthWords(board.length, su)} · {t("cut.waste").replace("{pct}", String(board.waste_pct))}
            </p>
            <DiagramSvg d={boardDiagram(board, 720, su)} label={stockTitle("board", board.index, plan.boards.length, board.material, t, board.stock)} />
          </>
        )}
        {plan.unplaced.length > 0 && (
          <p role="alert" data-unplaced={plan.unplaced.length} style={{ color: "var(--bad, #e5383b)" }}>
            {t("cut.unplaced").replace("{n}", String(plan.unplaced.length))} {plan.unplaced.map((u) => `#${u.number} ${u.name}`).join(", ")}
          </p>
        )}
      </div>
    );
  };

  return (
    <Sheet onClose={onClose} label={t("cut.title")} width={780} className="cut-plan">
      <SheetHead title={t("cut.title")} note={plan ? t("cut.note").replace("{parts}", String(plan.summary.parts)).replace("{sheets}", String(plan.summary.sheets)).replace("{boards}", String(plan.summary.boards)).replace("{total}", money(plan.cost.currency, plan.cost.total)) : ""} />
      <div className="row gap" role="tablist" style={{ margin: "8px 0" }}>
        {(["diagram", "stock", "buy"] as const).map((v) => (
          <button key={v} role="tab" aria-selected={view === v} className={`btn small${view === v ? " primary" : " ghost"}`} data-view={v} onClick={() => setView(v)}>
            {t(`cut.view.${v}`)}
          </button>
        ))}
      </div>
      <div style={{ minHeight: 120 }}>{body()}</div>
      <div className="row gap" style={{ flexWrap: "wrap", marginTop: 10 }}>
        <button className="btn small" data-action="export-diagrams" disabled={!plan || plan.summary.parts === 0} onClick={() => void save("diagrams")}>{t("cut.exportDiagrams")}</button>
        <button className="btn small" data-action="export-svg" disabled={!plan || plan.summary.parts === 0} onClick={() => void save("svg")}>{t("cut.exportSvg")}</button>
        <button className="btn small" data-action="print-labels" disabled={!plan || plan.summary.parts === 0} onClick={() => void save("labels")}>{t("cut.printLabels")}</button>
        <button className="btn small ghost" onClick={onClose}>{t("cut.close")}</button>
      </div>
    </Sheet>
  );
}
