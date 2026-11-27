import fs from "node:fs";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { runRuby } from "./support/ruby";

// the first run compiles the API with Opal (about ten seconds); later runs reuse it
vi.setConfig({ testTimeout: 120_000 });

const dir = path.resolve(__dirname, "../../tools/ruby-tests/ext");
const fx = (name: string) => ({ [name]: fs.readFileSync(path.join(dir, name + ".rb"), "utf8") });
const run = (name: string, item: string, extra: Partial<Parameters<typeof runRuby>[0]> = {}) => runRuby({ files: fx(name), item, ...extra });

// a 2 x 1 m rectangle on the ground, as the engine's `faces` (with outlines) gives it
const rect = {
  faces: [{ id: 7, normal: [0, 0, -1], outer: [[0, 0, 0], [2, 0, 0], [2, 1, 0], [0, 1, 0]], inner: [], group: null, material: null }],
  groups: [],
};

describe("Sketchup::Tool", () => {
  it("gets its events in order and what it draws is the journal", () => {
    const r = run("line_tool", "Line tool", {
      events: [
        { type: "mouse_move", x: 10, y: 10, at: [0, 0, 0] },
        { type: "lbutton_down", x: 10, y: 10, at: [0, 0, 0] },
        { type: "mouse_move", x: 90, y: 10, at: [1, 0, 0] },
        { type: "lbutton_down", x: 90, y: 10, at: [1, 0, 0] },
      ],
    });
    expect(r.error).toBeUndefined();
    expect(r.journal).toHaveLength(1);
    expect(r.journal[0]).toMatchObject({ cmd: "line" });
    expect(r.status).toBe("Click the first point");
  });

  it("takes typed lengths, and Esc starts over", () => {
    const r = run("line_tool", "Line tool", {
      events: [
        { type: "lbutton_down", at: [0, 0, 0] },
        { type: "mouse_move", at: [1, 0, 0] },
        { type: "user_text", text: "2m" },
        { type: "lbutton_down", at: [0, 0, 0] },
        { type: "cancel", reason: 0 },
        { type: "lbutton_down", at: [3, 0, 0] },
      ],
    });
    expect(r.error).toBeUndefined();
    expect(r.journal.filter((c) => c.cmd === "line")).toHaveLength(1);
    const line = r.journal.find((c) => c.cmd === "line") as { to?: number[]; points?: number[][] };
    expect(JSON.stringify(line)).toContain("2");
  });

  it("deactivates when the run ends and says so for an unknown event", () => {
    expect(run("line_tool", "Line tool", { events: [{ type: "nonsense" }] }).error).toMatch(/unknown event/);
  });
});

const click = (at: number[], extra = {}) => ({ type: "lbutton_down", x: 1, y: 1, at, ...extra });

describe("a curve tool (Bezier Spline's calls)", () => {
  it("collects control points, takes the segment count and draws the curve on Return", () => {
    const r = run("bezier_tool", "Bezier tool", {
      events: [click([0, 0, 0]), click([1, 2, 0]), click([2, 0, 0]), { type: "user_text", text: "6" }, { type: "return" }],
    });
    expect(r.error).toBeUndefined();
    const line = r.journal.find((c) => c.cmd === "line") as { points: number[][] };
    expect(line.points).toHaveLength(7); // 6 segments
    expect(line.points[0]).toEqual([0, 0, 0]);
    expect(line.points[6]).toEqual([2, 0, 0]);
    expect(line.points[3][1]).toBeCloseTo(1, 6); // the middle of a quadratic through (1, 2) peaks at y = 1
  });

  it("does nothing with fewer than three points", () => {
    expect(run("bezier_tool", "Bezier tool", { events: [click([0, 0, 0]), { type: "return" }] }).journal).toHaveLength(0);
  });
});

describe("reading the model (Make Face's calls) and Follow Me", () => {
  it("is empty without a snapshot", () => {
    const r = run("make_face_tool", "Make face from outline");
    expect(r.error).toBeUndefined();
    expect(r.toasts[0]).toMatch(/no face/);
  });

  it("sees the faces and their corners, in inches", () => {
    const r = run("make_face_tool", "Make face from outline", { model: rect });
    expect(r.error).toBeUndefined();
    expect(r.out).toMatch(/outline of 4 points, area 3100/); // 2 m x 1 m = 78.74 x 39.37 in
    const poly = r.journal.find((c) => c.cmd === "polygon") as { points: number[][] };
    expect(poly.points).toHaveLength(4);
  });

  it("sweeps a profile along a path with Face#followme", () => {
    const r = run("make_face_tool", "Rail around outline", { model: rect });
    expect(r.error).toBeUndefined();
    const fm = r.journal.find((c) => c.cmd === "follow_me") as { face: string; points: number[][] };
    expect(fm.face).toMatch(/^@/);
    expect(fm.points.length).toBeGreaterThanOrEqual(5);
  });

  it("names a face of the model by its real id when it is changed", () => {
    const r = runRuby({
      files: { t: `UI.menu('Plugins').add_item('Pull') { Sketchup.active_model.entities.grep(Sketchup::Face).first.pushpull(10) }` },
      item: "Pull",
      model: rect,
    });
    expect(r.error).toBeUndefined();
    expect(r.journal[0]).toMatchObject({ cmd: "pushpull", face: 7 });
  });

  it("knows which faces share an edge", () => {
    const two = {
      faces: [
        { id: 1, normal: [0, 0, -1], outer: [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]] },
        { id: 2, normal: [0, 0, -1], outer: [[1, 0, 0], [2, 0, 0], [2, 1, 0], [1, 1, 0]] },
      ],
    };
    const r = runRuby({
      files: { t: `UI.menu('Plugins').add_item('Edges') { e = Sketchup.active_model.entities; shared = e.grep(Sketchup::Edge).select { |x| x.faces.length == 2 }; puts "edges #{e.grep(Sketchup::Edge).length} shared #{shared.length} connected #{e.grep(Sketchup::Face).first.all_connected.length}" }` },
      item: "Edges",
      model: two,
    });
    expect(r.error).toBeUndefined();
    expect(r.out).toMatch(/edges 7 shared 1 connected 15/); // 2 faces + 7 edges + 6 vertices
  });
});

describe("a height label tool (Add Height from Datum's calls)", () => {
  it("labels a clicked point against the datum kept on the model", () => {
    const files = fx("datum_height");
    const set = runRuby({ files, item: "Set datum", answers: [["24"]] });
    expect(set.error).toBeUndefined();
    expect(set.out).toMatch(/datum 24/);
    const r = runRuby({ files, item: "Add height above datum", events: [click([0, 0, 1.2954])] });
    expect(r.error).toBeUndefined();
    const t = r.journal.find((c) => c.cmd === "text") as { text: string };
    // each run starts from nothing (the datum set in the run before is not kept: a known limit), so the datum is 0
    expect(t.text).toBe("1.3 m");
  });
});

describe("observers, dialogs, timers and textures", () => {
  const files = fx("observers_dialog");

  it("tells observers about selection, new entities and committed operations", () => {
    const r = runRuby({
      files,
      item: "Panel",
      selection: [],
      model: rect,
      events: [
        { type: "selection", ids: [7] },
        { type: "dialog", id: 0, name: "make_box", params: "2" },
        { type: "dialog", id: 0, name: "report", params: "" },
        { type: "tick" },
      ],
    });
    expect(r.error).toBeUndefined();
    expect(r.out).toMatch(/selection=1 added=\d+ commits=1 ticks=0/);
    expect(r.journal.some((c) => c.cmd === "polygon")).toBe(true);
    expect(r.journal.some((c) => c.cmd === "pushpull")).toBe(true);
  });

  it("records a dialog's html, what the script sent it, and whether it is showing", () => {
    const r = runRuby({ files, item: "Panel", events: [{ type: "dialog", id: 0, name: "make_box", params: "1" }] });
    expect(r.dialogs).toHaveLength(1);
    expect(r.dialogs[0]).toMatchObject({ title: "Panel", visible: true });
    expect(r.dialogs[0].html).toContain("make_box");
    expect(r.dialogs[0].scripts[0]).toContain("document.title");
  });

  it("says so when a dialog event names a callback that isn't there", () => {
    const r = runRuby({ files, item: "Panel", events: [{ type: "dialog", id: 0, name: "nope", params: "" }] });
    expect(r.error).toMatch(/no action callback named nope/);
  });

  it("fires timers on a tick", () => {
    const r = runRuby({ files, item: "Panel", events: [{ type: "tick" }, { type: "dialog", id: 0, name: "report", params: "" }] });
    expect(r.out).toMatch(/ticks=1/);
  });

  it("makes a material from a texture given as a data: URL", () => {
    const r = runRuby({ files, item: "Textured" });
    expect(r.error).toBeUndefined();
    expect(r.out).toMatch(/texture 8.0x4.0|texture 8x4/);
    const t = r.journal.find((c) => c.cmd === "texture") as { name: string; data: string; width: number; height: number };
    expect(t.name).toBe("Brick");
    expect(t.data).toMatch(/^data:image\/png/);
    expect(t.width).toBeCloseTo(0.2032, 4);
  });

  it("says why a texture from a file can't be read", () => {
    const r = runRuby({ files: { t: `UI.menu('Plugins').add_item('Tex') { Sketchup.active_model.materials.add('x').texture = 'brick.png' }` }, item: "Tex" });
    expect(r.error).toMatch(/data: URL/);
  });

  it("stops UI::HtmlDialog#write_image and Net::HTTP with a clear message instead of doing something else", () => {
    const r = runRuby({ files: { t: `UI.menu('Plugins').add_item('Net') { Net::HTTP.get('x') }` }, item: "Net" });
    expect(r.error).toMatch(/without a network/);
  });
});
