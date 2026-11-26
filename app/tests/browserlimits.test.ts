import { describe, expect, it } from "vitest";
import { MAX_BROWSER_FILE, fileTooBig } from "../src/filelimits";

import { displayAllowed } from "../src/pluginclean";

describe("plugin messages", () => {
  it("drops messages past 20 in 5 seconds, then allows again", () => {
    const ok = Array.from({ length: 30 }, () => displayAllowed("t-rate", 1000)).filter(Boolean).length;
    expect(ok).toBe(20);
    expect(displayAllowed("t-rate", 7000)).toBe(true);
    expect(displayAllowed("other", 1000)).toBe(true);
  });
});

describe("browser file size", () => {
  it("refuses a file over the limit before it is read", () => {
    expect(fileTooBig({ size: MAX_BROWSER_FILE + 1 })).toBe(true);
    expect(fileTooBig({ size: 1024 })).toBe(false);
  });
});
