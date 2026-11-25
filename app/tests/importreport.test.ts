import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { importedLines, isComplete, parseReport, readReportWhen, reportHeadline, saveReportWhen, shouldShow, skippedLines } from "../src/importreport";

const reply = (report: unknown) => ({ ok: true, report }) as never;

const full = {
  version: "{20.2.172}",
  imported: { faces: 63, components: 3, placed: 2, materials: 5, textured_materials: 2, textures: 1, dimensions: 4, labels: 1 },
  skipped: [
    { what: "textures", count: 1, why: "its picture can't be read; the material keeps its color", items: { untrusted_text: ["Brick: its picture can't be read"], note: "data" } },
    { what: "tags", count: null, why: "SketchUp tags are not imported yet", items: { untrusted_text: [], note: "data" } },
  ],
};

describe("import report", () => {
  it("reads the engine's report and ignores what isn't one", () => {
    expect(parseReport({ ok: true } as never)).toBeNull();
    expect(parseReport(reply("nope"))).toBeNull();
    const r = parseReport(reply(full))!;
    expect(r.version).toBe("{20.2.172}");
    expect(r.imported.faces).toBe(63);
    expect(r.skipped).toHaveLength(2);
    expect(r.skipped[0].items).toEqual(["Brick: its picture can't be read"]);
    expect(r.skipped[1].count).toBeNull();
  });

  it("drops entries and counts of the wrong type instead of showing them", () => {
    const r = parseReport(reply({ imported: { faces: "many", components: 2, bad: Infinity }, skipped: [7, { what: 1, why: "x" }, { what: "a", why: "b", count: "3", items: ["ok", 5, null] }] }))!;
    expect(r.imported).toEqual({ components: 2 });
    expect(r.skipped).toEqual([{ what: "a", why: "b", count: null, items: ["ok"] }]);
    expect(r.version).toBe("");
  });

  it("says what came across, a line per kind that has something", () => {
    const lines = importedLines(parseReport(reply(full))!);
    expect(lines.map((l) => l.key)).toEqual(["faces", "components", "materials", "dimensions"]);
    expect(lines[0].text).toBe("Geometry: 63 faces");
    expect(lines[1].text).toBe("Components: 3 definitions, 2 placed in the model");
    expect(lines[2].text).toBe("Materials: 5, 1 of 2 textured with their picture");
    expect(lines[3].text).toBe("Dimensions: 4 (1 label)");
    expect(importedLines(parseReport(reply({ imported: { faces: 1 }, skipped: [] }))!)[0].text).toBe("Geometry: 1 face");
    expect(importedLines(parseReport(reply({ imported: {}, skipped: [] }))!)).toEqual([]);
  });

  it("says what was left out and why, with the counts that are known", () => {
    const lines = skippedLines(parseReport(reply(full))!);
    expect(lines[0].text).toBe("1 textures: its picture can't be read; the material keeps its color");
    expect(lines[0].items).toEqual(["Brick: its picture can't be read"]);
    expect(lines[1].text).toBe("tags: SketchUp tags are not imported yet");
  });

  it("knows a complete import, and when to open the report", () => {
    const done = parseReport(reply({ imported: { faces: 6 }, skipped: [] }))!;
    const part = parseReport(reply(full))!;
    expect(isComplete(done)).toBe(true);
    expect(isComplete(part)).toBe(false);
    expect(reportHeadline(done)).toBe("everything in the file came across");
    expect(reportHeadline(part)).toBe("2 kinds of thing left out");
    expect(shouldShow(done, "always")).toBe(true);
    expect(shouldShow(done, "issues")).toBe(false);
    expect(shouldShow(part, "issues")).toBe(true);
  });
});

describe("import report setting", () => {
  // the report setting is read from localStorage; a plain Map stands in for it
  beforeEach(() => {
    const data = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
  });
  afterEach(() => {
    delete (globalThis as { localStorage?: unknown }).localStorage;
  });
  it("falls back to always when there is no storage", () => {
    delete (globalThis as { localStorage?: unknown }).localStorage;
    expect(readReportWhen()).toBe("always");
    saveReportWhen("issues"); // does not throw
  });
  it("opens after every import until told to open only when something was left out", () => {
    expect(readReportWhen()).toBe("always");
    saveReportWhen("issues");
    expect(readReportWhen()).toBe("issues");
    saveReportWhen("always");
    expect(readReportWhen()).toBe("always");
  });
});

describe("import report dialog", () => {
  it("shows what came across, what was left out and why, and a way to close it", async () => {
    const { createElement } = await import("react");
    const { renderToStaticMarkup } = await import("react-dom/server");
    const { ImportReportDialog } = await import("../src/importreportdialog");
    const rep = { ...parseReport(reply(full))!, name: "Tower.skp" };
    const html = renderToStaticMarkup(createElement(ImportReportDialog, { report: rep, onClose: () => {} }));
    expect(html).toContain("SketchUp import report");
    expect(html).toContain("Tower.skp");
    expect(html).toContain("saved by SketchUp 20.2.172");
    expect(html).toContain("Some of the file did not come across.");
    expect(html).toContain("Geometry: 63 faces");
    expect(html).toContain("1 textures: its picture can&#x27;t be read; the material keeps its color");
    expect(html).toContain("Brick: its picture can&#x27;t be read");
    expect(html).toContain("tags: SketchUp tags are not imported yet");
    expect(html).toContain("Close");
    const done = { ...parseReport(reply({ imported: { faces: 6 }, skipped: [] }))!, name: "Box.skp" };
    const ok = renderToStaticMarkup(createElement(ImportReportDialog, { report: done, onClose: () => {} }));
    expect(ok).toContain("Everything in the file came across.");
    expect(ok).not.toContain("Left out");
  });
});
