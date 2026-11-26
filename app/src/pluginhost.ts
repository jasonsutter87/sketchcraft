// The host side of a plugin run: what a plugin worker starts with, the checks on what it
// sends back, and the one-undo-step batch around its commands. The dialogs and the plugin
// types are in plugins.tsx (which re-exports all of this, so imports keep working).
import { exec, type Reply } from "./engine";
import { pluginSandboxHolds } from "./cspcheck";
import { SANDBOX_PREFACE, startWorker, type SandboxWorker } from "./sandbox";
import { cleanForm, cleanLabel, displayAllowed, cleanPoint, cleanPreview, cleanResult, type PluginForm, type PluginResult } from "./pluginclean";
import type { Plugin } from "./plugins";

/**
 * A first check on what a plugin sends. The real gate is the engine's:
 * commands from plugins run as `execPlugin`, which allows modeling and
 * inspection only (no files, programs, network or plugin management).
 */
export function allowed(cmd: Record<string, unknown>): string | null {
  if (!cmd || typeof cmd !== "object" || Array.isArray(cmd)) return "a command is an object with a cmd";
  if (typeof cmd.cmd !== "string" || !cmd.cmd) return "a command needs a cmd";
  return null;
}

// What runs in the worker before the plugin: the sketchcraft API, and no
// way to reach the network or load more code.
const PRELUDE = `
${SANDBOX_PREFACE}
const __commands = Object.create(null);
const __tools = Object.create(null);
let __tool = null; // the tool in use: { handlers, ctx }
const __waiting = new Map();
let __rid = 0;
const sketchcraft = Object.freeze({
  /** Add a command: fn(ctx) runs when it's picked. */
  command(id, fn) { __commands[String(id)] = fn; },
  /** Add a tool: { start(ctx), move(ctx, at), click(ctx, at), stop(ctx) }, all optional. */
  tool(id, handlers) { __tools[String(id)] = handlers || {}; },
});
self.onmessage = async (e) => {
  const m = e.data || {};
  if (m.type === "reply") {
    const w = __waiting.get(m.rid);
    __waiting.delete(m.rid);
    if (w) w(m.reply);
    return;
  }
  if (m.type === "tool-start" || m.type === "tool-event") return __toolMessage(m);
  if (m.type !== "invoke") return;
  const fn = __commands[m.id];
  if (typeof fn !== "function") return postMessage({ type: "error", error: "this plugin has no command " + JSON.stringify(m.id) });
  const ctx = Object.freeze({
    selection: m.selection,
    units: m.units,
    run: (cmd) => new Promise((res) => { const rid = ++__rid; __waiting.set(rid, res); postMessage({ type: "exec", rid, cmd: JSON.parse(JSON.stringify(cmd)) }); }),
    show: (r) => postMessage({ type: "show", result: JSON.parse(JSON.stringify(r)) }),
    ask: (form) => new Promise((res) => { const rid = ++__rid; __waiting.set(rid, res); postMessage({ type: "ask", rid, form: JSON.parse(JSON.stringify(form)) }); }),
    toast: (t) => postMessage({ type: "toast", text: String(t) }),
  });
  try { await fn(ctx); postMessage({ type: "done" }); }
  catch (err) { postMessage({ type: "error", error: String((err && err.message) || err) }); }
};
function __toolCtx(m) {
  return Object.freeze({
    selection: m.selection,
    units: m.units,
    run: (cmd) => new Promise((res) => { const rid = ++__rid; __waiting.set(rid, res); postMessage({ type: "exec", rid, cmd: JSON.parse(JSON.stringify(cmd)) }); }),
    ask: (form) => new Promise((res) => { const rid = ++__rid; __waiting.set(rid, res); postMessage({ type: "ask", rid, form: JSON.parse(JSON.stringify(form)) }); }),
    show: (r) => postMessage({ type: "show", result: JSON.parse(JSON.stringify(r)) }),
    toast: (t) => postMessage({ type: "toast", text: String(t) }),
    /** lines to draw while the tool runs: [[from, to], ...] (meters) */
    preview: (lines) => postMessage({ type: "preview", lines: JSON.parse(JSON.stringify(lines || [])) }),
    /** the hint in the corner while the tool runs */
    hint: (text) => postMessage({ type: "hint", text: String(text) }),
    /** a point the cursor's axis locks and rubber band start from (null: none) */
    anchor: (p) => postMessage({ type: "anchor", point: p ? JSON.parse(JSON.stringify(p)) : null }),
    /** leave the tool */
    finish: () => postMessage({ type: "finish" }),
  });
}
async function __toolMessage(m) {
  try {
    if (m.type === "tool-start") {
      const h = __tools[m.id];
      if (!h) return postMessage({ type: "error", error: "this plugin has no tool " + JSON.stringify(m.id) });
      __tool = { h, ctx: __toolCtx(m) };
      if (h.start) await h.start(__tool.ctx);
    } else if (__tool) {
      const f = __tool.h[m.kind];
      if (f) await f(__tool.ctx, m.at);
    }
    postMessage({ type: "handled", seq: m.seq });
  } catch (err) {
    postMessage({ type: "error", error: String((err && err.message) || err) });
  }
}
`;

const LIMIT_MS = 60_000;
/** The longest a Ruby script's recorded commands may take to replay (a JavaScript run has the same limit). */
export const RUBY_REPLAY_MS = LIMIT_MS;
/** The most commands one plugin run may send (each is checked, run and counted by the engine). */
export const MAX_COMMANDS = 10_000;

/**
 * Plugin commands run as ONE undo step: the engine's batch opens at the first command
 * and closes when the run (or one tool click) is over, so a single Undo takes back
 * everything the plugin did, however many commands that was.
 */
export function batcher(onEnd?: (r: Reply) => void) {
  let open = false;
  let id: number | undefined;
  // begin and end are sent in order, each after the last has answered (a plugin that finishes right
  // after an un-awaited command must not get its end ahead of its begin)
  let chain: Promise<unknown> = Promise.resolve();
  const then = (f: () => Promise<unknown>) => (chain = chain.then(f, f));
  return {
    async begin() {
      if (!open) {
        open = true;
        await then(async () => {
          const r = await exec({ cmd: "begin_batch" });
          id = typeof r.batch === "number" ? r.batch : undefined;
        });
      } else await chain;
    },
    async end() {
      if (open) {
        open = false;
        await then(async () => {
          const r = await exec(id === undefined ? { cmd: "end_batch" } : { cmd: "end_batch", id });
          id = undefined;
          onEnd?.(r);
        });
      } else await chain;
    },
  };
}

/** Counts a run's or a tool's commands and refuses the ones that are malformed or too many. */
function commandGate(noun: "run" | "tool") {
  let sent = 0;
  return {
    /** why this command is refused, or null to let it through (every call counts) */
    check(cmd: Record<string, unknown>): string | null {
      let why = allowed(cmd);
      if (!why && ++sent > MAX_COMMANDS) why = `a plugin ${noun} may send at most ${MAX_COMMANDS} commands`;
      return why;
    },
    get overLimit() {
      return sent > MAX_COMMANDS;
    },
  };
}

/** What the window does for a plugin besides drawing: run its commands, ask its forms, show its results. */
interface PluginEnv {
  run: (cmd: Record<string, unknown>) => Promise<Reply>;
  show: (r: PluginResult, plugin: Plugin) => void;
  toast: (text: string, bad?: boolean, undo?: boolean) => void;
  ask: (form: PluginForm, plugin: Plugin) => Promise<Record<string, unknown> | null>;
}

/** A plugin run that changed the model says so, with a way back (the engine counted what changed). */
export function announceChange(r: Reply, env: { toast: (text: string, bad?: boolean, undo?: boolean) => void }) {
  const n = typeof r.changed === "number" ? r.changed : 0;
  if (n > 0) env.toast(`changed ${n} thing${n === 1 ? "" : "s"}`, false, true);
}

/** Answer a worker's command: refused with a reason, or run inside the run's one undo step. */
async function answerExec(m: { rid: number; cmd: Record<string, unknown> }, worker: Worker, gate: ReturnType<typeof commandGate>, batch: ReturnType<typeof batcher>, env: PluginEnv) {
  const why = gate.check(m.cmd);
  let reply: Reply;
  if (why) reply = { ok: false, error: why };
  else {
    await batch.begin();
    reply = await env.run(m.cmd);
  }
  worker.postMessage({ type: "reply", rid: m.rid, reply });
}

/** Answer a worker's form: the time the person spends filling it in doesn't count against the plugin's limit. */
async function answerAsk(m: { rid: number; form: unknown }, worker: Worker, plugin: Plugin, env: PluginEnv, timer: { pause: () => void; resume: () => void }) {
  timer.pause();
  const answers = await env.ask(cleanForm(m.form), plugin);
  timer.resume();
  worker.postMessage({ type: "reply", rid: m.rid, reply: answers });
}

/** The messages that only show something to the person (true when it was one of them). */
function relayDisplay(m: { type: string; result?: unknown; text?: unknown }, plugin: Plugin, env: PluginEnv): boolean {
  if (m.type !== "show" && m.type !== "toast") return false;
  if (!displayAllowed(plugin.id)) return true;
  if (m.type === "show") env.show(cleanResult(m.result), plugin);
  else if (m.type === "toast") env.toast(cleanLabel(m.text, 300));
  else return false;
  return true;
}

/** A plugin said it hit an error. */
function reportError(m: { error?: unknown }, plugin: Plugin, env: PluginEnv) {
  if (!displayAllowed(plugin.id)) return;
  env.toast(`${plugin.name}: ${cleanLabel(m.error, 300)}`, true);
}

/** Run one plugin command. `run` sends engine commands (and refreshes the model). */
export function runPlugin(
  plugin: Plugin,
  commandId: string,
  env: {
    selection: { faces: number[]; edges: number[]; groups: number[]; instances: number[] };
    units: string;
    run: (cmd: Record<string, unknown>) => Promise<Reply>;
    show: (r: PluginResult, plugin: Plugin) => void;
    toast: (text: string, bad?: boolean, undo?: boolean) => void;
    /** show a form; the answers, or null if cancelled */
    ask: (form: PluginForm, plugin: Plugin) => Promise<Record<string, unknown> | null>;
    /** the command finished without an error */
    onDone?: () => void;
  },
): Promise<void> {
  return pluginSandboxHolds().then((safe) => new Promise<void>((resolve) => {
    if (!safe.ok) {
      env.toast(`Plugins are off: ${safe.why}.`, true);
      return resolve();
    }
    let sandbox: SandboxWorker;
    try {
      sandbox = startWorker([PRELUDE, "\n;\n", plugin.code]);
    } catch (e) {
      env.toast(`${plugin.name} couldn't start: ${String(e)}`, true);
      return resolve();
    }
    const { worker } = sandbox;
    const batch = batcher((r) => announceChange(r, env));
    const gate = commandGate("run");
    const finish = () => {
      clearTimeout(timer);
      sandbox.stop();
      void batch.end().finally(() => resolve());
    };
    // a minute of running (time spent filling in its forms doesn't count)
    let timer = 0 as unknown as ReturnType<typeof setTimeout>;
    const arm = () => {
      timer = setTimeout(() => {
        env.toast(`${plugin.name} took over a minute, so it was stopped.`, true);
        finish();
      }, LIMIT_MS);
    };
    arm();
    worker.onerror = (e) => {
      env.toast(`${plugin.name}: ${e.message || "it hit an error"}`, true);
      finish();
    };
    worker.onmessage = async (e) => {
      const m = e.data ?? {};
      if (m.type === "exec") {
        await answerExec(m, worker, gate, batch, env);
        if (gate.overLimit) {
          env.toast(`${plugin.name} sent more than ${MAX_COMMANDS} commands, so it was stopped.`, true);
          finish();
        }
      } else if (m.type === "ask") await answerAsk(m, worker, plugin, env, { pause: () => clearTimeout(timer), resume: arm });
      else if (relayDisplay(m, plugin, env)) return;
      else if (m.type === "error") {
        reportError(m, plugin, env);
        finish();
      } else if (m.type === "done") {
        env.onDone?.();
        finish();
      }
    };
    worker.postMessage({ type: "invoke", id: commandId, selection: env.selection, units: env.units });
  }));
}

/** Where the cursor is, for a plugin tool: the snapped point and what it's on. */
export interface ToolPoint {
  point: [number, number, number];
  /** what it snapped to: endpoint, midpoint, center, origin, on-edge, face, ground, axis… */
  snap: string;
  /** the face under the cursor (its id), if any */
  face: number | null;
  normal: [number, number, number];
  shift: boolean;
  alt: boolean;
}

export interface ToolSession {
  move: (at: ToolPoint) => void;
  click: (at: ToolPoint) => void;
  stop: () => void;
}

const EVENT_LIMIT_MS = 10_000;

/** Start a plugin's tool: a worker that stays while the tool is in use and
 * gets every move and click. Moves are dropped while the last one is busy.
 * Checks the window's policy itself (like `runPlugin`, not left to the caller) and answers null, with the
 * reason shown, when the tool can't start. */
export async function startPluginTool(
  plugin: Plugin,
  toolId: string,
  env: {
    selection: { faces: number[]; edges: number[]; groups: number[]; instances: number[] };
    units: string;
    run: (cmd: Record<string, unknown>) => Promise<Reply>;
    show: (r: PluginResult, plugin: Plugin) => void;
    toast: (text: string, bad?: boolean, undo?: boolean) => void;
    ask: (form: PluginForm, plugin: Plugin) => Promise<Record<string, unknown> | null>;
    preview: (lines: [number, number, number][][]) => void;
    hint: (text: string) => void;
    anchor: (p: [number, number, number] | null) => void;
    /** the tool ended (finished, failed or stopped) */
    ended: () => void;
  },
): Promise<ToolSession | null> {
  const safe = await pluginSandboxHolds();
  if (!safe.ok) {
    env.toast(`Plugins are off: ${safe.why}.`, true);
    return null;
  }
  let sandbox: SandboxWorker;
  try {
    sandbox = startWorker([PRELUDE, "\n;\n", plugin.code]);
  } catch (e) {
    env.toast(`${plugin.name} couldn't start: ${String(e)}`, true);
    return null;
  }
  const { worker } = sandbox;
  let seq = 0;
  let busy = false;
  let pendingMove: ToolPoint | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let over = false;
  // one undo step per tool event (a click): the batch opens at its first command and closes when it is handled
  const batch = batcher((r) => announceChange(r, env));
  const gate = commandGate("tool");
  const end = () => {
    if (over) return;
    over = true;
    void batch.end();
    clearTimeout(timer);
    sandbox.stop();
    env.preview([]);
    env.anchor(null);
    env.ended();
  };
  const arm = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      env.toast(`${plugin.name} took over 10 seconds on one step, so its tool was stopped.`, true);
      end();
    }, EVENT_LIMIT_MS);
  };
  const send = (m: Record<string, unknown>) => {
    if (over) return;
    busy = true;
    arm();
    worker.postMessage({ ...m, seq: ++seq });
  };
  worker.onerror = (e) => {
    env.toast(`${plugin.name}: ${e.message || "it hit an error"}`, true);
    end();
  };
  worker.onmessage = async (e) => {
    const m = e.data ?? {};
    if (m.type === "exec") {
      await answerExec(m, worker, gate, batch, env);
      // the gate refuses every command past the limit; stop the tool now instead of leaving it to its step timer
      if (gate.overLimit) {
        env.toast(`${plugin.name} sent more than ${MAX_COMMANDS} commands, so its tool was stopped.`, true);
        end();
      }
    } else if (m.type === "ask") await answerAsk(m, worker, plugin, env, { pause: () => clearTimeout(timer), resume: arm });
    else if (relayDisplay(m, plugin, env)) return;
    else if (m.type === "preview") env.preview(cleanPreview(m.lines));
    else if (m.type === "hint") env.hint(String(m.text).slice(0, 300));
    else if (m.type === "anchor") env.anchor(cleanPoint(m.point));
    else if (m.type === "finish") end();
    else if (m.type === "error") {
      reportError(m, plugin, env);
      end();
    } else if (m.type === "handled") {
      busy = false;
      clearTimeout(timer);
      await batch.end();
      if (pendingMove) {
        const at = pendingMove;
        pendingMove = null;
        send({ type: "tool-event", kind: "move", at });
      }
    }
  };
  send({ type: "tool-start", id: toolId, selection: env.selection, units: env.units });
  return {
    move: (at) => {
      if (busy) pendingMove = at;
      else send({ type: "tool-event", kind: "move", at });
    },
    click: (at) => {
      // a click always goes through, after whatever's in flight
      pendingMove = null;
      const go = () => (busy ? setTimeout(go, 15) : send({ type: "tool-event", kind: "click", at }));
      go();
    },
    stop: () => {
      if (over) return;
      // let the plugin tidy up, briefly
      worker.postMessage({ type: "tool-event", kind: "stop", at: null, seq: ++seq });
      setTimeout(end, 150);
    },
  };
}
