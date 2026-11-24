import { describe, expect, it } from "vitest";
import { isPolicyBlock } from "../src/cspcheck";

describe("plugin sandbox canary", () => {
  it("counts the policy's own refusals as blocked", () => {
    expect(isPolicyBlock("eval", { name: "EvalError", message: "Refused to evaluate a string as JavaScript because 'unsafe-eval' is not an allowed source of script" })).toBe(true);
    expect(isPolicyBlock("eval", { name: "EvalError", message: "call to eval() blocked by CSP" })).toBe(true);
    expect(isPolicyBlock("fetch", { name: "TypeError", message: "Refused to connect to 'data:text/plain,x' because it violates the following Content Security Policy directive: connect-src" })).toBe(true);
    expect(isPolicyBlock("fetch", { name: "TypeError", message: "Failed to fetch" })).toBe(true);
    expect(isPolicyBlock("fetch", { name: "TypeError", message: "Load failed" })).toBe(true);
    expect(isPolicyBlock("import", { name: "TypeError", message: "Failed to fetch dynamically imported module: data:text/javascript,export default 1" })).toBe(true);
    expect(isPolicyBlock("import", { name: "SecurityError", message: "script-src blocked it" })).toBe(true);
  });

  it("does not count any other throw", () => {
    expect(isPolicyBlock("eval", { name: "ReferenceError", message: "eval is not defined" })).toBe(false);
    expect(isPolicyBlock("eval", { name: "SyntaxError", message: "Unexpected token" })).toBe(false);
    expect(isPolicyBlock("fetch", { name: "ReferenceError", message: "fetch is not defined" })).toBe(false);
    expect(isPolicyBlock("fetch", { name: "TypeError", message: "fetch is not a function" })).toBe(false);
    expect(isPolicyBlock("import", { name: "TypeError", message: "Dynamic module import is disabled or not supported in this context" })).toBe(false);
    expect(isPolicyBlock("import", { name: "RangeError", message: "Maximum call stack size exceeded" })).toBe(false);
    expect(isPolicyBlock("eval", { name: "Error", message: "something odd" })).toBe(false);
    expect(isPolicyBlock("eval", null)).toBe(false);
    expect(isPolicyBlock("fetch", undefined)).toBe(false);
  });
});
