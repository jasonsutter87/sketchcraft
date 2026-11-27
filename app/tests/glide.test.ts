// Camera > Smooth Camera (#251): on unless turned off, remembered, and the viewport hears the switch.
import { afterEach, beforeEach, expect, it } from "vitest";
import { glideOn, setGlide, watchGlide } from "../src/glide";

const g = globalThis as unknown as { localStorage?: unknown; window?: unknown };
beforeEach(() => {
  const m = new Map<string, string>();
  g.localStorage = { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
  g.window = new EventTarget();
});
afterEach(() => {
  delete g.localStorage;
  delete g.window;
});

it("is on for everyone who never switched it", () => {
  expect(glideOn()).toBe(true);
});

it("stays off once turned off, and tells the viewport", () => {
  const heard: boolean[] = [];
  const stop = watchGlide((on) => heard.push(on));
  setGlide(false);
  expect(glideOn()).toBe(false);
  setGlide(true);
  expect(glideOn()).toBe(true);
  stop();
  setGlide(false);
  expect(heard).toEqual([false, true]);
});

it("stays on when storage can't be read (a private window)", () => {
  g.localStorage = { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); } };
  expect(glideOn()).toBe(true);
  expect(() => setGlide(false)).not.toThrow();
});
