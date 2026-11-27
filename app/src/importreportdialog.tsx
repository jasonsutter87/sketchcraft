// After File > Import of a SketchUp file: what came across and what did not, with why. Dismissable (Esc, the backdrop or
// Close); a checkbox keeps it from opening again when everything came across. The words are importreport.ts.
import { useState } from "react";
import { importedLines, isComplete, readReportWhen, saveReportWhen, skippedLines, type NamedReport } from "./importreport";
import { Sheet, SheetHead } from "./sheet";

export function ImportReportDialog({ report, onClose }: { report: NamedReport; onClose: () => void }) {
  const [onlyIssues, setOnlyIssues] = useState(readReportWhen() === "issues");
  const done = importedLines(report);
  const left = skippedLines(report);
  const title = "SketchUp import report";
  return (
    <Sheet onClose={onClose} label={title} width={520}>
      <SheetHead title={title} note={`${report.name}${report.version ? ` (saved by SketchUp ${report.version.replace(/[{}]/g, "")})` : ""}`} />
      <p style={{ margin: "4px 0 8px" }}>
        <b>{isComplete(report) ? "Everything in the file came across." : "Some of the file did not come across."}</b>
      </p>
      <h4 style={{ margin: "8px 0 2px" }}>Came across</h4>
      <ul className="import-report-list" style={{ listStyle: "none", padding: 0, margin: "0 0 8px" }}>
        {done.length === 0 && <li className="muted">Nothing was drawn.</li>}
        {done.map((l) => (
          <li key={l.key} data-ok="1" style={{ display: "flex", gap: 8, padding: "2px 0" }}>
            <span aria-hidden style={{ width: 16, color: "var(--ok, #2fa84f)" }}>✓</span>
            <span>{l.text}</span>
          </li>
        ))}
      </ul>
      {left.length > 0 && (
        <>
          <h4 style={{ margin: "8px 0 2px" }}>Left out</h4>
          <ul className="import-report-list" style={{ listStyle: "none", padding: 0, margin: "0 0 8px" }}>
            {left.map((l) => (
              <li key={l.key} data-ok="0" style={{ padding: "2px 0" }}>
                <div style={{ display: "flex", gap: 8 }}>
                  <span aria-hidden style={{ width: 16, color: "var(--warn, #d9a400)" }}>!</span>
                  <span>{l.text}</span>
                </div>
                {l.items.length > 0 && (
                  <div className="muted" style={{ marginLeft: 24, fontSize: "0.92em", overflowWrap: "anywhere" }}>
                    {l.items.slice(0, 5).join("; ")}
                    {l.items.length > 5 ? `; and ${l.items.length - 5} more` : ""}
                  </div>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
      <div className="row gap" style={{ alignItems: "center", flexWrap: "wrap", marginTop: 10 }}>
        <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            type="checkbox"
            checked={onlyIssues}
            onChange={(e) => {
              setOnlyIssues(e.target.checked);
              saveReportWhen(e.target.checked ? "issues" : "always");
            }}
          />
          <span>Only show this when something was left out</span>
        </label>
        <button className="btn small primary" style={{ marginLeft: "auto" }} autoFocus onClick={onClose}>Close</button>
      </div>
    </Sheet>
  );
}
