import vm, { type Context } from "node:vm";
import { describe, expect, it } from "vitest";
import { LOCKDOWN_SOURCE } from "../src/lockdown";

// A pretend worker whose network and storage live on prototypes, like a browser's.
function worker() {
  const g = vm.createContext({});
  vm.runInContext(
    `
    class WorkerGlobalScope { fetch() { return "net"; } importScripts() {} get indexedDB() { return 1; } createImageBitmap() {} }
    class DedicatedWorkerGlobalScope extends WorkerGlobalScope {}
    class WorkerNavigator { get storage() { return { getDirectory() {} }; } get hid() { return 1; } get gpu() { return 1; } sendBeacon() {} }
    const self = new DedicatedWorkerGlobalScope();
    self.WebAssembly = { instantiateStreaming() {}, compileStreaming() {} };
    self.XMLHttpRequest = function () {};
    Object.defineProperty(self, "navigator", { value: new WorkerNavigator() });
    globalThis.self = self;
  `,
    g,
  );
  return g;
}
const run = (g: Context, src: string) => vm.runInContext(src, g);

describe("plugin lockdown", () => {
  it("closes names on the prototypes too, not only on self", () => {
    const g = worker();
    expect(run(g, `typeof Object.getPrototypeOf(Object.getPrototypeOf(self)).fetch`)).toBe("function");
    run(g, `(function(){ "use strict"; ${LOCKDOWN_SOURCE} }).call(self)`);
    for (const k of ["fetch", "importScripts", "indexedDB", "createImageBitmap"]) {
      expect(run(g, `self.${k}`)).toBeUndefined();
      expect(run(g, `(() => { let p = self, f = false; while (p) { if (Reflect.get(p, "${k}", self) !== undefined) f = true; p = Object.getPrototypeOf(p); } return f; })()`)).toBe(false);
    }
    expect(run(g, `Object.getPrototypeOf(Object.getPrototypeOf(self)).fetch`)).toBeUndefined();
    expect(run(g, `Reflect.get(Object.getPrototypeOf(self.navigator), "storage", self.navigator)`)).toBeUndefined();
    expect(run(g, `Reflect.get(Object.getPrototypeOf(self.navigator), "hid", self.navigator) ?? Reflect.get(Object.getPrototypeOf(self.navigator), "gpu", self.navigator)`)).toBeUndefined();
    expect(run(g, `Object.getOwnPropertyDescriptor(Object.getPrototypeOf(self.navigator), "storage").get`)).toBeUndefined();
  });
  it("cannot be put back by a plugin", () => {
    const g = worker();
    run(g, `(function(){ "use strict"; ${LOCKDOWN_SOURCE} }).call(self)`);
    run(g, `try { Object.defineProperty(Object.getPrototypeOf(Object.getPrototypeOf(self)), "fetch", { value: () => 1 }); } catch {}`);
    expect(run(g, `Object.getPrototypeOf(Object.getPrototypeOf(self)).fetch`)).toBeUndefined();
  });
});
