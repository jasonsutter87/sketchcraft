// SketchUp Ruby scripts in SketchCraft: compile with Opal (vendored in
// public/ruby), run against the API in ruby/sketchup.rb, and hand back the
// SketchCraft commands they recorded. Both steps run in sandboxed workers (no
// network, no files); the app replays the commands, so they undo as usual.
import SKETCHUP_RB from "./ruby/sketchup.rb?raw";
import SKETCHUP_TOOLS_RB from "./ruby/sketchup_tools.rb?raw";
import { SANDBOX_PREFACE, startWorker } from "./sandbox";
import { pluginSandboxHolds } from "./cspcheck";
import { RUBY_SHIMS } from "./rubyshims";
import type { Plugin } from "./plugins";

let runtime: Promise<string> | null = null;
let compiler: Promise<string> | null = null;
const text = (url: string) => fetch(url).then((r) => (r.ok ? r.text() : Promise.reject(new Error(`couldn't load ${url}`))));
const opal = () => (runtime ??= text("ruby/opal.js"));
const opalCompiler = () => (compiler ??= text("ruby/opal-compiler.js"));

async function worker(code: string, limitMs: number, msg: unknown): Promise<Record<string, unknown>> {
  const safe = await pluginSandboxHolds();
  if (!safe.ok) return { error: `plugins are off: ${safe.why}` };
  return new Promise((resolve) => {
    const { worker: w, stop } = startWorker([code]);
    const done = (r: Record<string, unknown>) => {
      clearTimeout(timer);
      stop();
      resolve(r);
    };
    const timer = setTimeout(() => done({ error: "it took too long, so it was stopped" }), limitMs);
    w.onmessage = (e) => done(e.data ?? {});
    w.onerror = (e) => done({ error: e.message || "it hit an error" });
    w.postMessage(msg);
  });
}

/** Ruby files to JavaScript modules (Opal.modules), by name without .rb. */
async function compile(files: Record<string, string>): Promise<{ js?: string; error?: string }> {
  const [rt, comp] = await Promise.all([opal(), opalCompiler()]);
  const driver = `
self.onmessage = (e) => {
  const out = [];
  try {
    Opal.require("corelib/string/unpack");
    Opal.require("opal/compiler");
    for (const [name, src] of Object.entries(e.data.files)) {
      try {
        const js = Opal.Opal.Compiler.$new(src, Opal.hash({ file: name, requirable: true })).$compile();
        out.push(js);
      } catch (err) {
        postMessage({ error: name + ".rb: " + String((err && (err.message || err.$message && err.$message())) || err) });
        return;
      }
    }
    postMessage({ js: out.join("\\n;\\n") });
  } catch (err) {
    postMessage({ error: "the Ruby compiler didn't start: " + String((err && err.message) || err) });
  }
};`;
  const r = await worker(`${SANDBOX_PREFACE}\n${rt}\n;\n${comp}\n;\n${driver}`, 60_000, { files });
  return r as { js?: string; error?: string };
}

// Compiled JavaScript is kept between sessions, so the 2 MB compiler only
// loads the first time a script is seen. The key is a SHA-256 of the Ruby
// source (all its files and the compiler's version), and the entry stores a
// SHA-256 of the JavaScript beside it: an entry that doesn't match its own
// hash (damaged or edited) is thrown away and compiled again, never run.
// Old entries are dropped, so the cache can't grow without end or outlive
// the plugins it was made for.
const OPAL = "opal-1.7.3";
const CACHE_PREFIX = `sketchcraft.rubyjs.${OPAL}.`;
const CACHE_MAX = 40;

async function sha256(s: string): Promise<string> {
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, "0")).join("");
}

function dropOldCache() {
  try {
    const keys: { k: string; t: number }[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (!k?.startsWith("sketchcraft.rubyjs.")) continue;
      // other Opal versions' entries are never used again
      if (!k.startsWith(CACHE_PREFIX)) {
        keys.push({ k, t: 0 });
        continue;
      }
      let t = 0;
      try {
        t = JSON.parse(localStorage.getItem(k) ?? "{}").t ?? 0;
      } catch {
        /* unreadable: oldest */
      }
      keys.push({ k, t });
    }
    keys.sort((a, b) => a.t - b.t);
    for (const { k } of keys.slice(0, Math.max(0, keys.length - CACHE_MAX))) localStorage.removeItem(k);
  } catch {
    /* no storage */
  }
}

async function cachedCompile(files: Record<string, string>): Promise<{ js?: string; error?: string }> {
  let key = "";
  try {
    key = CACHE_PREFIX + (await sha256(JSON.stringify(files)));
    const raw = localStorage.getItem(key);
    if (raw) {
      const hit = JSON.parse(raw) as { js?: string; h?: string };
      if (typeof hit.js === "string" && typeof hit.h === "string" && (await sha256(hit.js)) === hit.h) return { js: hit.js };
      localStorage.removeItem(key); // doesn't match its own hash: not trusted
    }
  } catch {
    /* no storage or no crypto: compile every time */
  }
  const r = await compile(files);
  if (r.js && key) {
    try {
      localStorage.setItem(key, JSON.stringify({ js: r.js, h: await sha256(r.js), t: Date.now() }));
      dropOldCache();
    } catch {
      /* full: fine, it compiles again next time */
    }
  }
  return r;
}

const compiled = new Map<string, Promise<{ js?: string; error?: string }>>();
let apiJs: Promise<{ js?: string; error?: string }> | null = null;

/** The plugin's files compiled, once per version of them (keyed by a hash of every file). */
async function compiledPlugin(p: Plugin): Promise<{ js?: string; error?: string }> {
  const files = p.files ?? {};
  let key = p.id;
  try {
    key += ":" + (await sha256(JSON.stringify(files)));
  } catch {
    key += ":" + JSON.stringify(files); // no crypto: the whole text is the key
  }
  if (!compiled.has(key)) {
    const named = Object.fromEntries(Object.entries(files).map(([k, v]) => [k.replace(/\.rb$/i, ""), v]));
    compiled.set(key, cachedCompile(named));
  }
  return compiled.get(key)!;
}

export interface RubyDialog {
  id: number;
  title: string;
  html: string;
  file?: string | null;
  url?: string | null;
  scripts: string[];
  visible: boolean;
}

/** The model as the script may read it: faces in meters (the `faces` command with `outlines`), and groups. */
export interface RubyModel {
  faces: { id: number; normal: number[]; outer: number[][]; inner?: number[][][]; group?: number | null; material?: string | null }[];
  groups?: { id: number; name: string; kind?: string }[];
}

/** What the person (or a test) did while a Ruby tool was active: mouse and key events, in order. `at` is the world point under the cursor, meters. */
export interface RubyEvent {
  type: string;
  x?: number;
  y?: number;
  flags?: number;
  at?: number[];
  key?: number;
  repeat?: number;
  text?: string;
  reason?: number;
  ids?: number[];
  id?: number;
  name?: string;
  params?: string;
}

export interface RubyRun {
  /** SketchCraft commands to replay, in order */
  journal: Record<string, unknown>[];
  /** the menu items it added (its commands) */
  menu: string[];
  /** what it printed */
  out: string;
  /** messages it showed (UI.messagebox) */
  toasts: string[];
  /** dialogs it opened (UI::HtmlDialog): what it set, to show or test */
  dialogs: RubyDialog[];
  /** its status bar text (Sketchup.status_text=) */
  status?: string | null;
  /** a form it needs filled in (UI.inputbox) before it can finish */
  need?: { prompts: string[]; defaults: string[]; lists: string[]; title: string };
  error?: string;
}

/**
 * Load a Ruby plugin and (if `item` is given) run that menu item, against
 * `answers` for its input boxes so far. Nothing touches the model: the result
 * says what to do.
 */
export async function runRuby(
  p: Plugin,
  opts: { item?: number; answers?: string[][]; selection?: { id: number; name: string; kind: string }[]; model?: RubyModel; events?: RubyEvent[] },
): Promise<RubyRun> {
  const empty = { journal: [], menu: [], out: "", toasts: [], dialogs: [] };
  apiJs ??= cachedCompile({ sketchup_api: SKETCHUP_RB, sketchup_tools: SKETCHUP_TOOLS_RB });
  const [rt, api, mine] = await Promise.all([opal(), apiJs, compiledPlugin(p)]);
  if (api.error) return { ...empty, error: `SketchCraft's Ruby layer: ${api.error}` };
  if (mine.error) return { ...empty, error: mine.error };
  const entries = (p.entry ?? []).map((e) => e.replace(/\.rb$/i, ""));
  const driver = `
self.onmessage = (e) => {
  const d = e.data;
  globalThis.__runId = d.runId;
  globalThis.__journal = []; globalThis.__menu = []; globalThis.__out = []; globalThis.__toasts = [];
  globalThis.__answers = d.answers; globalThis.__askCount = 0; globalThis.__need = null;
  globalThis.__selection = d.selection; globalThis.__model = d.model || null; globalThis.__dialogs = []; globalThis.__status = null; globalThis.__events = d.events || [];
  const reply = (extra) => postMessage(Object.assign({ journal: globalThis.__journal, menu: globalThis.__menu, out: globalThis.__out.join(""), toasts: globalThis.__toasts, dialogs: globalThis.__dialogs, status: globalThis.__status }, extra));
  const msg = (err) => {
    if (!err) return "it hit an error";
    const m = err.$message ? err.$message() : err.message;
    const cls = err.$class ? err.$class().$name() : "";
    return (cls && cls !== "RuntimeError" ? cls + ": " : "") + String(m || err);
  };
  try {
    Opal.require("sketchup_api");
    Opal.require("sketchup_tools");
    for (const name of d.entries) Opal.require(name);
    if (d.item != null) {
      const items = Opal.UI.$items();
      const it = items[d.item];
      if (!it) return reply({ error: "it has no menu item " + d.item });
      if (it[1] === Opal.nil || !it[1]) return reply({ error: "that menu item does nothing" });
      it[1].$call();
      Opal.SC.$dispatch(d.events || []);
      Opal.SC.$end_tool();
      Opal.SC.$report();
      Opal.SC.$finish();
    }
    reply({});
  } catch (err) {
    if (globalThis.__need) return reply({ need: globalThis.__need, journal: [] });
    reply({ error: msg(err), journal: [] });
  }
};`;
  // Kernel#require finds the plugin's own files by name; a few SketchUp
  // library names are already loaded
  const shims = RUBY_SHIMS;
  // loading it (no menu item chosen) gets a short leash: it runs on every launch
  const limit = opts.item == null ? 10_000 : 30_000;
  const r = await worker(`${SANDBOX_PREFACE}\n${rt}\n;\n${api.js}\n;\n${mine.js}\n;\n${shims}\n;\n${driver}`, limit, {
    runId: Date.now().toString(36),
    entries,
    item: opts.item ?? null,
    answers: opts.answers ?? [],
    selection: opts.selection ?? [],
    model: opts.model ?? null,
    events: opts.events ?? [],
  });
  return {
    journal: (r.journal as RubyRun["journal"]) ?? [],
    menu: (r.menu as string[]) ?? [],
    out: String(r.out ?? ""),
    toasts: (r.toasts as string[]) ?? [],
    dialogs: (r.dialogs as RubyDialog[]) ?? [],
    status: (r.status as string | null) ?? null,
    need: r.need as RubyRun["need"],
    error: r.error as string | undefined,
  };
}

/** Most faces a script may read from the model (a bigger model is cut off, and the script is told). */
export const MAX_READ_FACES = 20_000;

/** Turn the engine's `faces` (with outlines) and `info` replies into the model a script reads. */
export function modelForRuby(faces: unknown, info: unknown): RubyModel | undefined {
  const fl = (faces as { faces?: RubyModel["faces"] } | undefined)?.faces;
  if (!Array.isArray(fl) || fl.length > MAX_READ_FACES) return undefined;
  const gl = (info as { groups?: { id: number; name: string }[] } | undefined)?.groups ?? [];
  return {
    faces: fl.filter((f) => Array.isArray(f.outer) && f.outer.length >= 3),
    groups: gl.map((g) => ({ id: g.id, name: String(g.name ?? ""), kind: "group" })),
  };
}
