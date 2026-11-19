import { expect, it } from "vitest";
import english from "../src/locale-en";
import italian from "../src/locale-it";
import { resolveLanguage, translate } from "../src/locale";
import { guide } from "../src/guide";
import { TOOL_INFO } from "../src/workspace";

it("covers every English message and built-in tool, with explicit null fallbacks", () => {
  expect(Object.keys(italian).sort()).toEqual(Object.keys(english).sort());
  const entries: Record<string, string | null> = italian;
  for (const [id, original] of Object.entries(english)) {
    expect(entries[id] === null || !!entries[id]?.trim()).toBe(true);
    expect(translate("it", id)).toBe(entries[id] ?? original);
  }
  for (const id of Object.keys(TOOL_INFO)) expect(Object.hasOwn(english, `tool.${id}`)).toBe(true);
  expect(translate("it", "toString", "fallback")).toBe("fallback");
});

it("starts from the system locale and honors the selected language", () => {
  expect(resolveLanguage("system", "it-IT")).toBe("it");
  expect(resolveLanguage(null, "de-DE")).toBe("en");
  expect(resolveLanguage("en", "it-IT")).toBe("en");
  expect(resolveLanguage("it", "en-GB")).toBe("it");
  expect(guide("select", { selected: 3, op: null }, "it").step).toContain("3");
});

it("keeps the modifier hints for the Italian paint and eraser tools", () => {
  expect(translate("it", "tool.paint")).toMatch(/campiona.*Ctrl.*Opzione.*collegate.*Maiusc.*stesso materiale/);
  expect(translate("it", "tool.erase")).toMatch(/Maiusc.*Ctrl.*ammorbidimento/);
});
