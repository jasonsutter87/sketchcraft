// Runs the SketchUp Ruby layer in Node (Opal in a vm), the way ruby.ts runs it in a worker, so the API can be
// tested without a browser: compile sketchup.rb + sketchup_tools.rb + the script's files, run a menu item, feed
// events to its tool, and read back the journal.
import fs from "node:fs";
import path from "node:path";
import vm, { type Context } from "node:vm";
import { RUBY_SHIMS } from "../../src/rubyshims";

const root = path.resolve(__dirname, "../..");
const read = (p: string) => fs.readFileSync(path.join(root, p), "utf8");

let compilerCtx: Context | null = null;
function compiler(): Context {
  if (compilerCtx) return compilerCtx;
  const ctx = vm.createContext({ console });
  vm.runInContext(read("public/ruby/opal.js"), ctx);
  vm.runInContext(read("public/ruby/opal-compiler.js"), ctx);
  vm.runInContext(`Opal.require("corelib/string/unpack"); Opal.require("opal/compiler");`, ctx);
  compilerCtx = ctx;
  return ctx;
}

const compiled = new Map<string, string>();
export function compile(name: string, src: string): string {
  const key = name + "\0" + src;
  const hit = compiled.get(key);
  if (hit) return hit;
  const ctx = compiler();
  ctx.__src = src;
  ctx.__name = name;
  const js = vm.runInContext(`Opal.Opal.Compiler.$new(__src, Opal.hash({ file: __name, requirable: true })).$compile()`, ctx) as string;
  compiled.set(key, js);
  return js;
}

export interface Ran {
  journal: Record<string, unknown>[];
  menu: string[];
  out: string;
  toasts: string[];
  dialogs: { id: number; title: string; html: string; scripts: string[]; visible: boolean; file?: string | null }[];
  status: string | null;
  error?: string;
  need?: unknown;
}

export interface RunOpts {
  /** the menu item to run (its label); without one the extension is only loaded */
  item?: string;
  events?: Record<string, unknown>[];
  model?: unknown;
  selection?: unknown[];
  answers?: string[][];
  /** the extension's files, by name (with or without .rb) */
  files: Record<string, string>;
  /** which of them are required at load, in order (default: all) */
  entry?: string[];
}

export function runRuby(o: RunOpts): Ran {
  const ctx = vm.createContext({ console });
  vm.runInContext(read("public/ruby/opal.js"), ctx);
  const mods = [
    compile("sketchup_api", read("src/ruby/sketchup.rb")),
    compile("sketchup_tools", read("src/ruby/sketchup_tools.rb")),
    ...Object.entries(o.files).map(([n, s]) => compile(n.replace(/\.rb$/i, ""), s)),
  ];
  for (const js of mods) vm.runInContext(js, ctx);
  // the same name shims as ruby.ts
  vm.runInContext(RUBY_SHIMS, ctx);
  const entries = (o.entry ?? Object.keys(o.files)).map((e) => e.replace(/\.rb$/i, ""));
  ctx.process_debug = !!process.env.RUBY_DEBUG; ctx.__dj = JSON.stringify({ entries, item: o.item ?? null, answers: o.answers ?? [], selection: o.selection ?? [], model: o.model ?? null, events: o.events ?? [] });
  const out = vm.runInContext(
    `(function () {
  const d = JSON.parse(__dj);
  globalThis.__runId = "t"; globalThis.__debug = !!process_debug;
  globalThis.__journal = []; globalThis.__menu = []; globalThis.__out = []; globalThis.__toasts = [];
  globalThis.__answers = d.answers; globalThis.__askCount = 0; globalThis.__need = null;
  globalThis.__selection = d.selection; globalThis.__model = d.model; globalThis.__dialogs = []; globalThis.__status = null;
  const reply = (extra) => Object.assign({ journal: globalThis.__journal, menu: globalThis.__menu, out: globalThis.__out.join(""), toasts: globalThis.__toasts, dialogs: globalThis.__dialogs, status: globalThis.__status }, extra);
  const msg = (err) => { const m = err.$message ? err.$message() : err.message; const cls = err.$class ? err.$class().$name() : ""; return (cls && cls !== "RuntimeError" ? cls + ": " : "") + String(m || err); };
  try {
    Opal.require("sketchup_api"); Opal.require("sketchup_tools");
    for (const name of d.entries) Opal.require(name);
    if (d.item != null) {
      const items = Opal.UI.$items();
      const it = items.find((x) => x[0] === d.item);
      if (!it) return reply({ error: "no menu item " + d.item + " (has " + items.map((x) => x[0]).join(", ") + ")" });
      it[1].$call();
      Opal.SC.$dispatch(d.events);
      Opal.SC.$end_tool();
      Opal.SC.$report();
      Opal.SC.$finish();
    }
    return reply({});
  } catch (err) {
    if (globalThis.__need) return reply({ need: globalThis.__need, journal: [] });
    return reply({ error: msg(err) + (globalThis.__debug ? " " + err.stack : ""), journal: [] });
  }
})()`,
    ctx,
  ) as Ran;
  // values made inside the vm have its own prototypes: copy them out
  return JSON.parse(JSON.stringify(out)) as Ran;
}
