import { describe, expect, it } from "vitest";
import { parsePitch } from "../src/pitch";

describe("roof pitch typed in Entity Info (#263)", () => {
  it("takes degrees or rise/run", () => {
    expect(parsePitch("30")).toBe(30);
    expect(parsePitch("30°")).toBe(30);
    expect(parsePitch(" 6/12 ")).toBe(26.6);
  });
  it("refuses what isn't a pitch", () => {
    expect(parsePitch("steep")).toBeNull();
    expect(parsePitch("85")).toBeNull();
    expect(parsePitch("-5")).toBeNull();
    expect(parsePitch("")).toBeNull();
    expect(parsePitch("°")).toBeNull();
  });
});
