import vm from "node:vm";
import { describe, expect, it, vi } from "vitest";
import { LOCKDOWN_SOURCE } from "../src/lockdown";
import { cleanLabel, displayAllowed } from "../src/pluginclean";

// AskHost without a DOM: hooks run at once, effects are kept so the test can unmount.
const cleanups: (() => void)[] = [];
vi.mock("react", async (orig) => {
  const real = await orig<typeof import("react")>();
  return {
    ...real,
    useState: (v: unknown) => [v, () => {}],
    useRef: () => ({ current: null }),
    useEffect: (f: () => void | (() => void)) => {
      const c = f();
      if (typeof c === "function") cleanups.push(c);
    },
  };
});
vi.mock("../src/locale", () => ({ useLocale: () => ({ t: (s: string) => s }) }));

describe("question host going away", () => {
  it("answers the open question and does not hang the next one", async () => {
    const { AskHost, ask } = await import("../src/ask");
    AskHost();
    const first = ask("one");
    const second = ask("two"); // waits behind the first
    cleanups.splice(0).forEach((c) => c()); // the host unmounts with "one" open
    expect(await first).toBeNull();
    expect(await second).toBeNull();
    expect(await ask("three")).toBeNull(); // no host: answered at once, not stuck
  });
});

describe("text shown from a plugin", () => {
  it("cleaned labels are one line without bidi or zero-width characters", () => {
    const s = cleanLabel("Fine\nVerified by SketchCraft‮​⁦ tail", 300);
    expect(s).toBe("Fine Verified by SketchCraft tail");
    expect(cleanLabel("x".repeat(900), 300)).toHaveLength(300);
  });
  it("the rate limit is per plugin", () => {
    let ok = 0;
    for (let i = 0; i < 30; i++) if (displayAllowed("rl-test", 1000, 20, 5000)) ok++;
    expect(ok).toBe(20);
  });
});

describe("lockdown refuses to run when a name stays open", () => {
  const make = (frozen: boolean) => {
    const g = vm.createContext({});
    vm.runInContext(
      `class W { fetch() {} }
       const self = new W();
       ${frozen ? "Object.defineProperty(W.prototype, 'fetch', { value: () => 1, configurable: false, writable: false });" : ""}
       self.navigator = {}; self.WebAssembly = {}; globalThis.self = self;`,
      g,
    );
    return g;
  };
  it("passes when everything closes", () => {
    expect(() => vm.runInContext(`(function(){ "use strict"; ${LOCKDOWN_SOURCE} }).call(self)`, make(false))).not.toThrow();
  });
  it("throws a clear error when one cannot be closed", () => {
    expect(() => vm.runInContext(`(function(){ "use strict"; ${LOCKDOWN_SOURCE} }).call(self)`, make(true))).toThrow(/could not close fetch/);
  });
});
