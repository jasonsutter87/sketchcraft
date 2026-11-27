// The welcome screen: when it shows at launch, and the recent files it lists.
import { expect, it } from "vitest";
import { addRecent, forgetRecent, MAX_RECENT, recentFiles } from "../src/recent";
import { STARTS, welcomeAtStart } from "../src/welcome";

const store = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
};

it("shows at launch unless turned off, and never in a test browser", () => {
  const s = store();
  expect(welcomeAtStart(s, false)).toBe(true);
  s.setItem("sketchcraft.welcome", "off");
  expect(welcomeAtStart(s, false)).toBe(false);
  expect(welcomeAtStart(store(), true)).toBe(false);
});

it("lists models newest first, each once, and only model files", () => {
  const s = store();
  addRecent("/a/one.sketchcraft", 1, s);
  addRecent("/a/two.sketchcraft", 2, s);
  addRecent("/a/one.sketchcraft", 3, s);
  addRecent("/a/not-a-model.obj", 4, s);
  expect(recentFiles(s).map((r) => r.path)).toEqual(["/a/one.sketchcraft", "/a/two.sketchcraft"]);
  for (let i = 0; i < 20; i++) addRecent(`/m/${i}.sketchcraft`, 10 + i, s);
  expect(recentFiles(s)).toHaveLength(MAX_RECENT);
  forgetRecent("/m/19.sketchcraft", s);
  expect(recentFiles(s)[0].path).toBe("/m/18.sketchcraft");
});

it("reads junk in storage as no list", () => {
  const s = store();
  s.setItem("sketchcraft.recent", "{not json");
  expect(recentFiles(s)).toEqual([]);
  s.setItem("sketchcraft.recent", JSON.stringify([{ path: "/x/.hidden", at: 1 }, { path: 5, at: 2 }, "nope"]));
  expect(recentFiles(s).every((r) => r.path.endsWith(".sketchcraft"))).toBe(true);
});

it("offers each look in metric and in feet and inches", () => {
  for (const name of ["Simple", "Architectural", "Visualization"]) {
    const units = STARTS.filter((s) => s.name === name).map((s) => s.units);
    expect(units).toContain("ft");
    expect(units.some((u) => u === "m" || u === "mm")).toBe(true);
  }
});
