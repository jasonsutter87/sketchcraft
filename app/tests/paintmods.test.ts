import { describe, expect, it } from "vitest";
import { paintCommand, paintReach } from "../src/paintmods";

const keys = (o: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> = {}) => ({ altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...o });

describe("paintReach", () => {
  it("is just the face with no keys", () => {
    expect(paintReach(keys(), true)).toBe("face");
    expect(paintReach(keys(), false)).toBe("face");
  });
  it("Ctrl (and Option on a Mac) spreads to connected faces, Shift to every matching face, both to the whole object", () => {
    expect(paintReach(keys({ ctrlKey: true }), false)).toBe("connected");
    expect(paintReach(keys({ ctrlKey: true }), true)).toBe("connected");
    expect(paintReach(keys({ altKey: true }), true)).toBe("connected");
    expect(paintReach(keys({ shiftKey: true }), false)).toBe("all");
    expect(paintReach(keys({ ctrlKey: true, shiftKey: true }), false)).toBe("object");
    expect(paintReach(keys({ altKey: true, shiftKey: true }), true)).toBe("object");
  });
  it("samples with Command on a Mac and Alt elsewhere", () => {
    expect(paintReach(keys({ metaKey: true }), true)).toBe("sample");
    expect(paintReach(keys({ altKey: true }), false)).toBe("sample");
    expect(paintReach(keys({ altKey: true, shiftKey: true }), false)).toBe("sample");
  });
});

describe("paintCommand", () => {
  it("adds the scope only when the paint spreads", () => {
    expect(paintCommand("face", 4, { material: null, color: [1, 0, 0] })).toEqual({ cmd: "paint", ids: [4], color: [1, 0, 0] });
    expect(paintCommand("connected", 4, { material: "Brick", color: [1, 0, 0] })).toEqual({ cmd: "paint", ids: [4], material: "Brick", scope: "connected" });
    expect(paintCommand("object", 4, { material: null, color: [0, 1, 0] })).toEqual({ cmd: "paint", ids: [4], color: [0, 1, 0], scope: "object" });
  });
});
