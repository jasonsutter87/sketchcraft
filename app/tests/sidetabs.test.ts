import { describe, expect, it } from "vitest";
import { panelTabs, readSideTab, shownTab, stepTab } from "../src/sidetabs";
import english from "../src/locale-en";
import italian from "../src/locale-it";

describe("the Vertical layout's tabs", () => {
  it("adds Properties as the fourth tab in Vertical only", () => {
    expect(panelTabs("model", "side")).toEqual(["outliner", "library", "views", "properties"]);
    expect(panelTabs("layout", "side")).toEqual(["pages", "outliner", "views", "properties"]);
    expect(panelTabs("model", "classic")).toEqual(["outliner", "library", "views"]);
    expect(panelTabs("model", "floating")).toEqual(["outliner", "library", "views"]);
  });

  it("reads a remembered tab back only if it is a known one", () => {
    expect(readSideTab("properties")).toBe("properties");
    expect(readSideTab("views")).toBe("views");
    expect(readSideTab("nope")).toBeNull();
    expect(readSideTab(null)).toBeNull();
    expect(readSideTab("__proto__")).toBeNull();
  });

  it("falls back to the first tab when the chosen one isn't there (Properties after leaving Vertical)", () => {
    expect(shownTab("properties", panelTabs("model", "classic"))).toBe("outliner");
    expect(shownTab("library", panelTabs("layout", "side"))).toBe("pages");
    expect(shownTab("properties", panelTabs("model", "side"))).toBe("properties");
  });

  it("moves between tabs with the arrow keys, wrapping, and Home and End", () => {
    const tabs = panelTabs("model", "side");
    expect(stepTab(tabs, "outliner", "ArrowRight")).toBe("library");
    expect(stepTab(tabs, "properties", "ArrowRight")).toBe("outliner");
    expect(stepTab(tabs, "outliner", "ArrowLeft")).toBe("properties");
    expect(stepTab(tabs, "views", "Home")).toBe("outliner");
    expect(stepTab(tabs, "views", "End")).toBe("properties");
    expect(stepTab(tabs, "views", "a")).toBeNull();
  });

  it("has the tab's names in English and Italian", () => {
    expect(english["tab.properties"]).toBe("Properties");
    expect(italian["tab.properties"]).toBe("Proprietà");
  });
});
