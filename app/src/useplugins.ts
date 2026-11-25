// Plugins in the window: the installed list, the commands they add, and running them (a
// JavaScript command, a plugin's tool, or a SketchUp Ruby command replayed one undoable
// SketchCraft command at a time). What the plugins show (results, forms, a tool's hint) is
// held here for App to draw.
import { useEffect, useMemo, useRef, useState } from "react";
import { execPlugin, type Reply, type Scene } from "./engine";
import { modelForRuby, runRuby } from "./ruby";
import { RUBY_REPLAY_MS, announceChange, batcher, cleanForm, cleanResult, loadPlugins, MAX_COMMANDS, runPlugin, startPluginTool, type Plugin, type PluginCommand, type PluginForm, type PluginList, type PluginResult, type ToolSession } from "./plugins";
import type { Sel } from "./selection";
import type { Tool } from "./Viewport";

/** a short hash, to tell one version of a plugin's files from another */
function fingerprint(t: string): string {
  let h = 2166136261;
  for (let i = 0; i < t.length; i++) h = Math.imul(h ^ t.charCodeAt(i), 16777619);
  return `${t.length}.${(h >>> 0).toString(36)}`;
}

export function usePlugins({ sel, scene, tool, setTool, say, sayAs, runForPlugin, refresh }: {
  sel: Sel;
  scene: Scene | null;
  tool: Tool;
  setTool: (t: Tool | ((t: Tool) => Tool)) => void;
  say: (text: string, bad?: boolean) => void;
  sayAs: (plugin: Plugin, t: string, bad?: boolean, undo?: boolean) => void;
  runForPlugin: (cmd: Record<string, unknown>, plugin?: Plugin) => Promise<Reply>;
  refresh: () => Promise<void>;
}) {
  const [plugins, setPlugins] = useState<PluginList>({ dir: "", plugins: [], problems: [] });
  const [pluginShown, setPluginShown] = useState<{ result: PluginResult; plugin: Plugin } | null>(null);
  // plugin commands run this session (the plugins lesson watches it)
  const [pluginRuns, setPluginRuns] = useState(0);
  // a plugin's tool in use: its session, and what it shows
  const toolSession = useRef<ToolSession | null>(null);
  const [pluginTool, setPluginTool] = useState<{ name: string; hint: string; preview: [number, number, number][][]; anchor: [number, number, number] | null } | null>(null);
  const [pluginForm, setPluginForm] = useState<{ form: PluginForm; plugin: Plugin; done: (a: Record<string, unknown> | null) => void } | null>(null);
  const reloadPlugins = () => loadPlugins().then(setPlugins, () => {});
  // a SketchUp Ruby plugin's commands are the menu items it adds when it loads
  const [rubyMenus, setRubyMenus] = useState<Record<string, string[] | { error: string }>>({});
  // the menu is remembered with a fingerprint of the plugin's files, so a replaced plugin gets its new menu
  const rubySig = (p: Plugin) => `${p.id}:${p.version ?? ""}:${fingerprint(JSON.stringify(p.files ?? {}))}`;
  const menuSig = useRef<Record<string, string>>({});
  useEffect(() => {
    for (const p of plugins.plugins) {
      if (!p.ruby || p.enabled === false) continue;
      const sig = rubySig(p);
      if (rubyMenus[p.id] && menuSig.current[p.id] === sig) continue;
      menuSig.current[p.id] = sig;
      runRuby(p, {}).then((r) => {
        if (menuSig.current[p.id] === sig) setRubyMenus((m) => ({ ...m, [p.id]: r.error ? { error: r.error } : r.menu }));
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plugins]);
  // plugins that are on (Ruby ones with the commands they registered)
  const livePlugins: Plugin[] = plugins.plugins
    .filter((p) => p.enabled !== false)
    .map((p) => {
      if (!p.ruby) return p;
      const m = rubyMenus[p.id];
      return { ...p, commands: Array.isArray(m) ? m.map((label, i): PluginCommand => ({ id: String(i), label, hint: `${p.name} (SketchUp Ruby)` })) : [] };
    });
  // their commands with icons sit on the tool bar
  const pluginButtons = useMemo(
    () => livePlugins.flatMap((pl) => pl.commands.filter((c) => c.icon).map((c) => ({ id: `plugin:${pl.id}:${c.id}`, name: c.label.replace(/…$/, ""), icon: c.icon!, run: () => void runPluginCommandRef.current(pl, c.id) }))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [plugins, rubyMenus],
  );
  useEffect(() => {
    loadPlugins().then(setPlugins, () => {});
  }, []);

  // the tool bar's buttons call the latest runner (it reads the current selection)
  const runPluginCommandRef = useRef<(p: Plugin, c: string) => Promise<void>>(async () => {});
  // a command marked "tool" starts the plugin's tool instead of running once
  const startTool = async (plugin: Plugin, toolId: string, label: string) => {
    toolSession.current?.stop();
    const name = (plugin.builtin ? label : `${label} · plugin “${plugin.name.slice(0, 40)}”`).replace(/…$/, "");
    setPluginTool({ name, hint: "", preview: [], anchor: null });
    const session = await startPluginTool(plugin, toolId, {
      selection: { faces: sel.faces, edges: sel.edges, groups: sel.groups, instances: sel.instances },
      units: scene?.units || "m",
      run: (cmd) => runForPlugin(cmd, plugin),
      show: (result, pl) => setPluginShown({ result, plugin: pl }),
      toast: (t, bad, undo) => sayAs(plugin, t, bad, undo),
      ask: (form, pl) => new Promise((resolve) => setPluginForm({ form, plugin: pl, done: (a) => (setPluginForm(null), resolve(a)) })),
      preview: (lines) => setPluginTool((p) => (p ? { ...p, preview: lines } : p)),
      hint: (text) => setPluginTool((p) => (p ? { ...p, hint: plugin.builtin ? text : `Plugin: ${text}` } : p)),
      anchor: (pt) => setPluginTool((p) => (p ? { ...p, anchor: pt } : p)),
      ended: () => {
        if (toolSession.current === session) {
          toolSession.current = null;
          setPluginTool(null);
          setTool((t) => (t === "plugin" ? "select" : t));
          setPluginRuns((n) => n + 1);
        }
      },
    });
    if (!session) {
      // it couldn't start (the reason was shown): no banner, no tool
      setPluginTool(null);
      return;
    }
    toolSession.current = session;
    setTool("plugin");
  };
  // picking another tool (or Esc) leaves the plugin's tool
  useEffect(() => {
    if (tool !== "plugin" && toolSession.current) {
      const s = toolSession.current;
      toolSession.current = null;
      setPluginTool(null);
      s.stop();
    }
  }, [tool]);

  // a SketchUp Ruby command: run it (asking for its input boxes), then replay
  // what it recorded, one undoable SketchCraft command at a time
  const runRubyCommand = async (plugin: Plugin, item: number) => {
    const answers: string[][] = [];
    const selection = [
      ...sel.groups.map((id) => ({ id, name: scene?.groups.find((g) => g.id === id)?.name ?? "", kind: "group" })),
      ...sel.instances.map((id) => ({ id, name: scene?.instances.find((i) => i.id === id)?.name ?? "", kind: "instance" })),
      ...sel.faces.map((id) => ({ id, name: "", kind: "face" })),
    ];
    // the script may read the model as it is now (faces, groups); one read serves every round
    const model = modelForRuby(await execPlugin({ cmd: "faces", outlines: true }, plugin.name).catch(() => null), await execPlugin({ cmd: "info" }, plugin.name).catch(() => null));
    for (let round = 0; round < 12; round++) {
      const r = await runRuby(plugin, { item, answers, selection, model });
      if (r.need) {
        const need = r.need;
        const got = await new Promise<Record<string, unknown> | null>((resolve) =>
          setPluginForm({
            form: cleanForm({
              title: need.title,
              fields: need.prompts.map((label, i) => {
                const list = need.lists[i];
                return list && list.includes("|") ? { id: `f${i}`, label, type: "choice" as const, options: list.split("|"), value: need.defaults[i] } : { id: `f${i}`, label, type: "text" as const, value: need.defaults[i] ?? "" };
              }),
            }),
            plugin,
            done: (a) => (setPluginForm(null), resolve(a)),
          }),
        );
        if (!got) return;
        answers.push(need.prompts.map((_, i) => String(got[`f${i}`] ?? "")));
        continue;
      }
      if (r.error) return say(`${plugin.name}: ${r.error}`, true);
      if (r.journal.length > MAX_COMMANDS) return say(`${plugin.name} made ${r.journal.length} changes in one run; the most is ${MAX_COMMANDS}, so none were applied.`, true);
      let failed = "";
      // everything it recorded is ONE undo step (and one save)
      const batch = batcher((br) => announceChange(br, { toast: (t, bad, undo) => sayAs(plugin, t, bad, undo) }));
      try {
        await batch.begin();
        // replaying has the same wall-clock limit as a JavaScript plugin run (one command can be slow, 10,000 can take hours)
        const started = Date.now();
        for (const [i, cmd] of r.journal.entries()) {
          if (Date.now() - started > RUBY_REPLAY_MS) {
            failed = `step ${i + 1} (${String(cmd.cmd)}): it took over ${RUBY_REPLAY_MS / 1000} seconds, so the rest was skipped`;
            break;
          }
          // Ruby's recorded commands are plugin commands too: same limits as JavaScript plugins
          const res = await execPlugin(cmd, plugin.name);
          if (!res.ok) {
            failed = `step ${i + 1} (${String(cmd.cmd)}): ${res.error}`;
            break;
          }
        }
      } finally {
        await batch.end();
      }
      await refresh();
      setPluginRuns((n) => n + 1);
      for (const t of r.toasts.slice(0, 5)) sayAs(plugin, String(t).slice(0, 300));
      // panels (UI::HtmlDialog) and mouse tools run in tests and through the API; the app doesn't show or feed them yet
      for (const d of r.dialogs.slice(0, 3)) sayAs(plugin, `opened the panel "${String(d.title || "untitled").slice(0, 80)}", which SketchCraft doesn't show yet`);
      if (failed) say(`${plugin.name} stopped at ${failed}. ⌘Z undoes what it did.`, true);
      if (r.out.trim()) setPluginShown({ result: cleanResult({ title: plugin.name, text: r.out.trim() }), plugin });
      return;
    }
    say(`${plugin.name} kept asking for input, so it was stopped.`, true);
  };

  const runPluginCommand = (plugin: Plugin, commandId: string) => {
    if (plugin.ruby) return runRubyCommand(plugin, Number(commandId));
    const c = plugin.commands.find((x) => x.id === commandId);
    if (c?.tool) return startTool(plugin, commandId, c.label);
    return runPlugin(plugin, commandId, {
      onDone: () => setPluginRuns((n) => n + 1),
      selection: { faces: sel.faces, edges: sel.edges, groups: sel.groups, instances: sel.instances },
      units: scene?.units || "m",
      run: (cmd) => runForPlugin(cmd, plugin),
      show: (result, pl) => setPluginShown({ result, plugin: pl }),
      toast: (t, bad, undo) => sayAs(plugin, t, bad, undo),
      ask: (form, pl) =>
        new Promise((resolve) =>
          setPluginForm({
            form,
            plugin: pl,
            done: (a) => {
              setPluginForm(null);
              resolve(a);
            },
          }),
        ),
    });
  };

  runPluginCommandRef.current = runPluginCommand;
  return { plugins, setPlugins, pluginShown, setPluginShown, pluginRuns, setPluginRuns, toolSession, pluginTool, setPluginTool, pluginForm, setPluginForm, reloadPlugins, rubyMenus, setRubyMenus, livePlugins, pluginButtons, runPluginCommandRef, startTool, runRubyCommand, runPluginCommand };
}
