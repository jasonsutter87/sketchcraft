// Plugins: small scripts that add commands to SketchCraft (see docs/PLUGINS.md).
// Each run gets a fresh Web Worker: no page, no network, no files. Its only
// way out is ctx.run(), and the engine only lets those commands model: draw,
// edit, inspect (core/src/policy.rs: an allowlist, enforced in Rust for
// JavaScript plugins and for SketchUp Ruby alike).
import { useRef, useState } from "react";
import { confirm } from "./ask";
import { exec } from "./engine";
import { pickSave } from "./files";
import { useLocale } from "./locale";
import { cleanForm, cleanResult, type PluginField, type PluginForm, type PluginResult } from "./pluginclean";

export { cleanForm, cleanResult };
export { allowed, announceChange, batcher, MAX_COMMANDS, RUBY_REPLAY_MS, runPlugin, startPluginTool } from "./pluginhost";
export type { ToolPoint, ToolSession } from "./pluginhost";
export type { PluginField, PluginForm, PluginResult };
import { formatLength, parseLength } from "./units";
import { Sheet, SheetHead, useEscapeToClose } from "./sheet";

export interface PluginCommand {
  id: string;
  label: string;
  hint?: string;
  /** its icon (a data URL), for the tool bar */
  icon?: string | null;
  /** an interactive tool (sketchcraft.tool), not a one-shot command */
  tool?: boolean;
}

export interface Plugin {
  id: string;
  name: string;
  version?: string;
  author?: string;
  description?: string;
  commands: PluginCommand[];
  code: string;
  builtin: boolean;
  /** off: its commands don't show anywhere */
  enabled?: boolean;
  folder?: string;
  /** a SketchUp Ruby extension: its .rb files (by path) and the ones that load it */
  ruby?: boolean;
  files?: Record<string, string>;
  entry?: string[];
}

export interface PluginList {
  dir: string;
  plugins: Plugin[];
  problems: { id: string; error: string }[];
}

export async function loadPlugins(): Promise<PluginList> {
  const r = await exec({ cmd: "plugins" });
  if (!r.ok) return { dir: "", plugins: [], problems: [] };
  const plugins = ((r.plugins as Plugin[]) ?? []).filter((p) => p && typeof p.code === "string" && Array.isArray(p.commands));
  return { dir: String(r.dir ?? ""), plugins, problems: (r.problems as PluginList["problems"]) ?? [] };
}


// ---- what a plugin hands the interface -----------------------------------------------
// Plugin code is untrusted, so what it asks the window to show is checked
// and cut down before React sees it (a wrong shape can't blank the app).

/**
 * Always shown above anything a plugin puts on screen, in the app's own words
 * (the plugin can't change it), so a window can't pass for SketchCraft's.
 */
export function PluginBanner({ plugin, form }: { plugin: Plugin; form?: boolean }) {
  return (
    <p className="plugin-banner small" role="note">
      {plugin.builtin ? "From a built-in plugin." : `This ${form ? "form" : "window"} comes from the ${plugin.ruby ? "SketchUp Ruby " : ""}plugin “${plugin.name.slice(0, 60)}”, not from SketchCraft.`}
      {form && !plugin.builtin ? " Never type a password or a key here." : ""}
    </p>
  );
}



/**
 * One CSV cell. A text cell that starts like a spreadsheet formula (= + - @, tab or
 * return) gets a leading quote, so opening the file in a spreadsheet can't run it;
 * real numbers are left as they are.
 */
export const csvCell = (v: string | number) => {
  let s = String(v ?? "");
  const number = typeof v === "number" && Number.isFinite(v);
  if (!number && /^[=+\-@\t\r]/.test(s) && !/^[-+]?\d+(\.\d+)?([eE][-+]?\d+)?$/.test(s)) s = `'${s}`;
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

/** What a plugin showed: its text and table, to copy or save as CSV. */
export function PluginResultDialog({ result, plugin, onClose, onToast, onCutPlan }: { result: PluginResult; plugin: Plugin; onClose: () => void; onToast: (t: string, bad?: boolean) => void; onCutPlan?: () => void }) {
  const [copied, setCopied] = useState(false);
  const { t: tr } = useLocale();
  const t = result.table;
  const csv = t ? [t.columns, ...t.rows].map((r) => r.map(csvCell).join(",")).join("\n") + "\n" : "";
  const save = async () => {
    const path = await pickSave(`${(result.title ?? plugin.name).replace(/[^\w -]+/g, "").trim() || "plugin"}.csv`, "csv", "CSV");
    if (!path) return;
    const r = await exec({ cmd: "save_text", path, text: csv });
    if (r.ok) onToast(`Saved ${path.split("/").pop()}`);
    else onToast(r.error ?? "Couldn't save it", true);
  };
  return (
    <Sheet onClose={onClose} label={result.title ?? plugin.name} className="plugin-result">
      <div className="sheet-head">
        <b>{result.title ?? plugin.name}</b>
        <span className="muted">{plugin.builtin ? "Built-in plugin" : `Plugin: ${plugin.name}${plugin.version ? ` ${plugin.version}` : ""}`}</span>
      </div>
      <PluginBanner plugin={plugin} />
      {result.text && <p className="plugin-text">{result.text}</p>}
      {t && (
        <div className="plugin-table">
          <table>
            <thead>
              <tr>
                {t.columns.map((c, i) => (
                  <th key={i}>{c}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {t.rows.map((r, i) => (
                <tr key={i}>
                  {r.map((c, j) => (
                    <td key={j}>{String(c)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="row gap">
        {t && (
          <>
            <button
              className="btn small"
              onClick={() =>
                navigator.clipboard.writeText([t.columns, ...t.rows].map((r) => r.join("\t")).join("\n")).then(
                  () => setCopied(true),
                  () => onToast("Couldn't copy", true),
                )
              }
            >
              {copied ? "Copied" : "Copy table"}
            </button>
            <button className="btn small ghost" onClick={save}>
              Save CSV…
            </button>
          </>
        )}
        {plugin.builtin && plugin.id === "cutlist" && onCutPlan && (
          <button className="btn small primary" data-action="cut-plan" onClick={onCutPlan}>
            {tr("menu.cutPlan")}
          </button>
        )}
        <button className="btn small ghost" onClick={onClose}>
          Close
        </button>
      </div>
    </Sheet>
  );
}

/** The Plugins window: every plugin, on or off, install from a .zip, remove yours. */
export function PluginsDialog({
  list,
  reload,
  onClose,
  onToast,
  commandsOf,
}: {
  list: PluginList;
  reload: () => Promise<void>;
  onClose: () => void;
  onToast: (t: string, bad?: boolean) => void;
  /** a plugin's commands as found so far (Ruby ones register theirs when they load) */
  commandsOf?: (p: Plugin) => PluginCommand[];
}) {
  const file = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const install = async (f: File | undefined) => {
    if (!f) return;
    setBusy(true);
    const data = await new Promise<string>((res) => {
      const fr = new FileReader();
      fr.onload = () => res(String(fr.result).replace(/^data:[^,]*,/, ""));
      fr.onerror = () => res("");
      fr.readAsDataURL(f);
    });
    // look first: what is it, what does it contain, what would it replace
    const look = await exec({ cmd: "install_plugin", data, name: f.name, preview: true });
    if (!look.ok) {
      setBusy(false);
      return onToast(`Couldn't read ${f.name}: ${look.error}`, true);
    }
    const files = (look.files as { name: string; bytes: number }[]) ?? [];
    const cmds = (look.commands as string[]) ?? [];
    const was = look.installed as { version?: string | null; sha256?: string } | null;
    const replacing = !!was;
    const lines = [
      `${look.kind}${look.version ? ` · version ${look.version}` : ""}${look.author ? ` · by ${look.author}` : ""}`,
      look.description ? String(look.description) : "",
      cmds.length ? `${look.ruby ? "Menu items (read from its Ruby code)" : "Commands"}: ${cmds.slice(0, 12).join(", ")}` : look.ruby ? "Menu items: none found in its Ruby code (it may add them when it runs)" : "",
      look.ruby ? "This is a SketchUp Ruby extension: its Ruby code runs every time SketchCraft starts, and the menu it builds comes from that code." : "",
      `${files.length} file${files.length === 1 ? "" : "s"}: ${files.slice(0, 8).map((x) => `${x.name} (${x.bytes} B)`).join(", ")}${files.length > 8 ? ", …" : ""}`,
      `Fingerprint (SHA-256): ${String(look.sha256).slice(0, 16)}…`,
      replacing ? `This REPLACES the installed copy (version ${was?.version ?? "unknown"}${was?.sha256 === look.sha256 ? ", identical files" : ", different files"}). The old one is kept in .removed.` : "",
      "Plugins can draw, edit and inspect the model. Install only ones you trust.",
    ].filter(Boolean);
    const yes = await confirm(`${replacing ? "Replace" : "Install"} “${String(look.name).slice(0, 60)}”?`, lines.join("\n"), replacing ? "Replace" : "Install");
    if (!yes) {
      setBusy(false);
      return;
    }
    const r = await exec({ cmd: "install_plugin", data, name: f.name, replace: replacing });
    setBusy(false);
    if (!r.ok) return onToast(`Couldn't install ${f.name}: ${r.error}`, true);
    // what was shown must be what was installed: same kind and same files
    if (!!r.ruby !== !!look.ruby || r.sha256 !== look.sha256) {
      await exec({ cmd: "remove_plugin", id: r.id });
      await reload();
      return onToast(`${f.name} installed as something other than what was shown, so it was removed again.`, true);
    }
    await reload();
    onToast(`${r.replaced ? "Updated" : "Installed"} ${r.name}. Its commands are in File › Plugins and Shift+S.`);
  };
  const toggle = async (p: Plugin, on: boolean) => {
    const r = await exec({ cmd: "plugin_enable", id: p.id, on });
    if (!r.ok) return onToast(r.error ?? "Couldn't change it", true);
    await reload();
  };
  const remove = async (p: Plugin) => {
    if (!(await confirm(`Remove ${p.name}?`, "It's moved to the .removed folder inside Plugins, so you can put it back.", "Remove"))) return;
    const r = await exec({ cmd: "remove_plugin", id: p.id });
    if (!r.ok) return onToast(r.error ?? "Couldn't remove it", true);
    await reload();
    onToast(`Removed ${p.name}`);
  };
  return (
    <Sheet onClose={onClose} label="Plugins" className="plugins-dlg">
      <SheetHead title="Plugins" note="Add commands to SketchCraft. They can draw, edit and inspect the model, nothing else: the app refuses any plugin command that touches files, other programs or the network, and what they change can be undone." />
      <div className="plugin-list">
        {list.plugins.map((p) => (
          <div key={p.id} className={`plugin-row${p.enabled === false ? " off" : ""}`}>
            <label className="plugin-switch" title={p.enabled === false ? "Turn on" : "Turn off"}>
              <input type="checkbox" checked={p.enabled !== false} onChange={(e) => toggle(p, e.target.checked)} aria-label={`${p.name} on`} />
            </label>
            <div className="plugin-about">
              <b>{p.name}</b>
              <span className="muted small">
                {[p.version && `v${p.version}`, p.author, p.ruby ? "SketchUp Ruby" : null, p.builtin ? "built in" : "yours"].filter(Boolean).join(" · ")}
              </span>
              {p.description && <span className="small">{p.description}</span>}
              <span className="muted small">{(commandsOf?.(p) ?? p.commands).map((c) => c.label).join(", ")}</span>
            </div>
            {!p.builtin && (
              <button className="btn small ghost" onClick={() => remove(p)}>
                Remove
              </button>
            )}
          </div>
        ))}
        {list.problems.map((p) => (
          <div key={`x${p.id}`} className="plugin-row off">
            <span />
            <div className="plugin-about">
              <b>{p.id}</b>
              <span className="small warn">{p.error}</span>
            </div>
          </div>
        ))}
      </div>
      <input ref={file} type="file" accept=".zip,.rbz,.rb,application/zip" hidden onChange={(e) => (install(e.target.files?.[0]), (e.target.value = ""))} />
      <div className="row gap" style={{ marginTop: 14 }}>
        <button className="btn small" onClick={() => file.current?.click()} disabled={busy}>
          {busy ? "Installing…" : "Install from file…"}
        </button>
        {list.dir && (
          <button className="btn small ghost" onClick={() => exec({ cmd: "reveal", path: list.dir })}>
            Open plugins folder
          </button>
        )}
        <span className="muted small">A SketchCraft plugin (.zip), or a SketchUp Ruby extension (.rbz or .rb).</span>
        <span style={{ flex: 1 }} />
        <button className="btn small" onClick={onClose}>
          Done
        </button>
      </div>
    </Sheet>
  );
}

/** A length to put in a form field, without losing millimeters (0.018 m, not 0.02 m). */
const exactLength = (m: number, units: string) => (units === "m" || !units ? `${+m.toFixed(4)} m` : formatLength(m, units));

/** A plugin's form: fields from ctx.ask, answers back (lengths in meters). */
export function PluginFormDialog({ form, plugin, units, done }: { form: PluginForm; plugin: Plugin; units: string; done: (answers: Record<string, unknown> | null) => void }) {
  const fields = (form.fields ?? []).filter((f) => f && typeof f.id === "string").slice(0, 40);
  const [vals, setVals] = useState<Record<string, string | boolean>>(() =>
    Object.fromEntries(
      fields.map((f) => [
        f.id,
        f.type === "check" ? !!f.value : f.type === "length" && typeof f.value === "number" ? exactLength(f.value, units) : f.type === "choice" ? String(f.value ?? f.options?.[0] ?? "") : String(f.value ?? ""),
      ]),
    ),
  );
  const [bad, setBad] = useState<string | null>(null);
  const submit = () => {
    const out: Record<string, unknown> = {};
    for (const f of fields) {
      const v = vals[f.id];
      if (f.type === "check") out[f.id] = !!v;
      else if (f.type === "length" || f.type === "number") {
        const n = f.type === "length" ? parseLength(String(v), units) : Number(String(v).trim());
        if (!Number.isFinite(n)) return setBad(`${f.label}: type a ${f.type === "length" ? "length" : "number"}`);
        if (f.min != null && n < f.min) return setBad(`${f.label}: at least ${f.type === "length" ? formatLength(f.min, units) : f.min}`);
        if (f.max != null && n > f.max) return setBad(`${f.label}: at most ${f.type === "length" ? formatLength(f.max, units) : f.max}`);
        out[f.id] = n;
      } else out[f.id] = String(v);
    }
    done(out);
  };
  useEscapeToClose(() => done(null));
  return (
    <div className="sheet-backdrop" onClick={() => done(null)}>
      <form
        className="sheet plugin-form"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
        onKeyDown={(e) => e.stopPropagation()}
        role="dialog"
        aria-label={form.title ?? plugin.name}
      >
        <div className="sheet-head">
          <b>{form.title ?? plugin.name}</b>
          <span className="muted">{plugin.builtin ? "Built-in plugin" : `Plugin: ${plugin.name}`}</span>
        </div>
        <PluginBanner plugin={plugin} form />
        {form.text && <p className="plugin-text">{form.text}</p>}
        <div className="kv plugin-fields">
          {fields.map((f) =>
            f.type === "check" ? (
              <label key={f.id} className="field check" style={{ gridColumn: "1 / -1" }} title={f.hint}>
                <input type="checkbox" checked={!!vals[f.id]} onChange={(e) => setVals((v) => ({ ...v, [f.id]: e.target.checked }))} />
                <span>{f.label}</span>
              </label>
            ) : (
              <label key={f.id} className="plugin-field" title={f.hint} style={{ display: "contents" }}>
                <span>{f.label}</span>
                {f.type === "choice" ? (
                  <select value={String(vals[f.id])} onChange={(e) => setVals((v) => ({ ...v, [f.id]: e.target.value }))}>
                    {(f.options ?? []).map((o) => (
                      <option key={o} value={o}>
                        {o}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    type="text"
                    inputMode={f.type === "number" ? "decimal" : undefined}
                    value={String(vals[f.id])}
                    placeholder={f.type === "length" ? `e.g. ${exactLength(1, units)}` : undefined}
                    onChange={(e) => setVals((v) => ({ ...v, [f.id]: e.target.value }))}
                  />
                )}
              </label>
            ),
          )}
        </div>
        {bad && <p className="small warn">{bad}</p>}
        <div className="row gap" style={{ marginTop: 14 }}>
          <span style={{ flex: 1 }} />
          <button type="button" className="btn small ghost" onClick={() => done(null)}>
            Cancel
          </button>
          <button type="submit" className="btn small primary">
            {form.ok ?? "OK"}
          </button>
        </div>
      </form>
    </div>
  );
}
