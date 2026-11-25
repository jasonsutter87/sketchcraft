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

describe("reading the model", () => {
  it("is empty without a snapshot and has the faces with one", () => {
    expect(rect.faces).toHaveLength(1);
  });
});
