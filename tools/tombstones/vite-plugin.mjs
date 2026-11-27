// Step 2 (TypeScript): a Vite plugin that turns each `// @debug tomb:<id>` mark into `__tomb("<id>");`.
// Used by Vite (the dev page the Playwright suites open) and Vitest, and only when SKETCHCRAFT_TOMB_LOG is set:
// app/vite.config.ts and app/vitest.config.ts add it conditionally, so a normal build never loads this file.
//
// The probe logs each id once per page/process. In Node (Vitest) it appends to the log file directly; in the browser
// (and in workers) it batches ids and POSTs them to the dev server's /__tomb endpoint, which appends them.
import fs from "node:fs";

const RUNTIME_ID = "virtual:tomb-runtime";
const RESOLVED = "\0" + RUNTIME_ID;
const MARK_LINE = /^([ \t]*)\/\/ @debug tomb:([0-9a-f]{8})[ \t]*$/gm;

/** Source with every mark replaced by a probe call (same line count, so source maps stay right). */
export function injectProbes(code) {
  if (!code.includes("@debug tomb:")) return null;
  let n = 0;
  const body = code.replace(MARK_LINE, (_m, ind, id) => { n++; return `${ind}__tomb("${id}");`; });
  return n ? `import { __tomb } from "${RUNTIME_ID}";${body}` : null;
}

const RUNTIME = (logPath) => `
const seen = new Set();
const isNode = typeof process !== "undefined" && !!process.versions && !!process.versions.node;
let pending = [];
let timer = 0;
const LOG = ${JSON.stringify(logPath)};
function flush() {
  timer = 0;
  if (!pending.length) return;
  const body = JSON.stringify(pending);
  pending = [];
  try {
    const url = (globalThis.location && globalThis.location.origin ? globalThis.location.origin : "") + "/__tomb";
    if (typeof navigator !== "undefined" && navigator.sendBeacon && typeof document !== "undefined") navigator.sendBeacon(url, body);
    else fetch(url, { method: "POST", body, keepalive: true }).catch(() => {});
  } catch (e) {}
}
if (!isNode && typeof addEventListener === "function") {
  addEventListener("pagehide", flush);
  addEventListener("visibilitychange", () => { if (typeof document !== "undefined" && document.visibilityState === "hidden") flush(); });
}
export function __tomb(id) {
  if (seen.has(id)) return;
  seen.add(id);
  if (isNode) {
    const fs = process.getBuiltinModule("node:fs");
    fs.appendFileSync(LOG, id + "\\n");
  } else {
    pending.push(id);
    if (!timer) timer = setTimeout(flush, 300);
  }
}
`;

export default function tombstones({ log = process.env.SKETCHCRAFT_TOMB_LOG } = {}) {
  if (!log) throw new Error("tombstones plugin: set SKETCHCRAFT_TOMB_LOG to the path of the hits log");
  return {
    name: "sketchcraft-tombstones",
    enforce: "pre",
    resolveId(id) { return id === RUNTIME_ID ? RESOLVED : null; },
    load(id) { return id === RESOLVED ? RUNTIME(log) : null; },
    transform(code, id) {
      const file = id.split("?")[0];
      if (!/\.(ts|tsx)$/.test(file) || file.includes("/node_modules/")) return null;
      const out = injectProbes(code);
      return out ? { code: out, map: null } : null;
    },
    configureServer(server) {
      server.middlewares.use("/__tomb", (req, res) => {
        if (req.method !== "POST") { res.statusCode = 405; res.end(); return; }
        let data = "";
        req.on("data", (c) => { data += c; if (data.length > 1e6) req.destroy(); });
        req.on("end", () => {
          try {
            const ids = JSON.parse(data).filter((x) => /^[0-9a-f]{8}$/.test(x));
            if (ids.length) fs.appendFileSync(log, ids.join("\n") + "\n");
          } catch { /* a bad beacon is dropped */ }
          res.statusCode = 204; res.end();
        });
      });
    },
  };
}
