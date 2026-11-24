// Tools > Joinery > Dado / Rabbet / Tenon: a small prompt for two boards that touch. The engine does the cut and the
// undo step (dado, rabbet, tenon); the numbers and words are joinery.ts. Sizes are typed in mm, or inches for an
// imperial model, and left blank to let the engine pick (a third of the board for a dado, and so on).
import { useState } from "react";
import { jointCommand, jointSummary, JOINT_FIELDS, type JointForm, type JointKind } from "./joinery";
import { useLocale } from "./locale";
import type { Run } from "./panelkit";
import { Sheet, SheetHead } from "./sheet";

export function JoineryDialog({ kind, boards, names, units, run, onClose, onSay }: { kind: JointKind; boards: [number, number]; names: [string, string]; units: "mm" | "in"; run: Run; onClose: () => void; onSay: (text: string, bad?: boolean) => void }) {
  const { t } = useLocale();
  const [form, setForm] = useState<JointForm>({ cutFirst: true });
  const [error, setError] = useState<string | null>(null);
  const set = (k: string, v: string | boolean) => setForm({ ...form, [k]: v });
  const cutName = form.cutFirst ? names[0] : names[1];
  const fitName = form.cutFirst ? names[1] : names[0];

  const apply = async () => {
    const made = jointCommand(kind, boards, form, units);
    if ("error" in made) {
      setError(t("joint.err.field").replace("{field}", t(`joint.f.${made.error}`)));
      return;
    }
    setError(null);
    // the engine's own reasons (the boards do not touch, the tenon is too long...) read well as they are
    const r = await run(made.cmd);
    if (!r.ok) {
      setError(r.error ?? t("joint.err.generic"));
      return;
    }
    onSay(jointSummary(r, t));
    onClose();
  };

  return (
    <Sheet onClose={onClose} label={t(`joint.title.${kind}`)} width={460} className="joinery">
      <SheetHead title={t(`joint.title.${kind}`)} note={t(`joint.note.${kind}`)} />
      <label className="field" style={{ margin: "8px 0" }}>
        <span>{t("joint.cutWhich")}</span>
        <select value={form.cutFirst ? "0" : "1"} onChange={(e) => set("cutFirst", e.target.value === "0")} aria-label={t("joint.cutWhich")}>
          <option value="0">{names[0]}</option>
          <option value="1">{names[1]}</option>
        </select>
      </label>
      <p className="muted" style={{ margin: "2px 0 8px" }} data-cut={cutName} data-fit={fitName}>
        {t(`joint.says.${kind}`).replace("{cut}", cutName).replace("{fit}", fitName)}
      </p>
      <div className="row gap" style={{ flexWrap: "wrap" }}>
        {JOINT_FIELDS[kind].map((f) => (
          <label className="field" key={f} style={{ width: 120 }}>
            <span>
              {t(`joint.f.${f}`)} ({units})
            </span>
            <input value={String(form[f] ?? "")} inputMode="decimal" placeholder={t("joint.auto")} onChange={(e) => set(f, e.target.value)} aria-label={t(`joint.f.${f}`)} />
          </label>
        ))}
      </div>
      <p className="muted" style={{ margin: "8px 0 4px" }}>{t("joint.blank")}</p>
      {error && <p role="alert" style={{ color: "var(--bad, #e5383b)" }}>{error}</p>}
      <div className="row gap" style={{ marginTop: 10 }}>
        <button className="btn small primary" data-action="joint-apply" onClick={() => void apply()}>{t(`joint.apply.${kind}`)}</button>
        <button className="btn small ghost" onClick={onClose}>{t("cut.close")}</button>
      </div>
    </Sheet>
  );
}
