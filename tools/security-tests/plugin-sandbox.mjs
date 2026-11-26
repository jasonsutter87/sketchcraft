#!/usr/bin/env node
// Plugin sandbox checks in a real browser: what a hostile JavaScript or Ruby
// plugin can and can't do. Needs the bridge and dev page (docs/dev/building.md),
// on a scratch model (SKETCHCRAFT_DOC), never your real one.
//   node tools/security-tests/plugin-sandbox.mjs
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { BRIDGE, jsonHeaders, launch, PAGE, pageExec } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same) so nothing real is touched.');
  process.exit(2);
}

// a listener on loopback that records any request made to it
const hits = [];
const sink = http.createServer((req, res) => {
  hits.push(req.url);
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.end('/* sink */');
});
await new Promise((r) => sink.listen(0, '127.0.0.1', r));
const SINK = `http://127.0.0.1:${sink.address().port}`;
const marker = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'sc-sandbox-')), 'marker.txt');

const b = await launch();
const p = await (await b.newContext({ locale: 'en-US' })).newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

// run a plugin's code the way the app does; collect what it reported
const runJs = (code) =>
  p.evaluate(
    async ({ code, BRIDGE, HDR }) => {
      const { runPlugin } = await import('/src/plugins.tsx');
      const replies = [];
      const toasts = [];
      const plugin = { id: 'hostile', name: 'Hostile', commands: [{ id: 'go', label: 'Go' }], code: `sketchcraft.command('go', async (ctx) => { ${code} });`, builtin: false };
      await runPlugin(plugin, 'go', {
        selection: { faces: [], edges: [], groups: [], instances: [] },
        units: 'm',
        // what App.tsx does for plugin commands: the engine's plugin policy
        run: async (cmd) => {
          const r = await (await fetch(BRIDGE + '/exec?as=plugin', { method: 'POST', headers: HDR, body: JSON.stringify(cmd) })).json();
          replies.push([cmd.cmd, r.ok, r.error ?? null]);
          return r;
        },
        show: () => {},
        toast: (t) => toasts.push(t),
        ask: async () => null,
      });
      return { replies, toasts };
    },
    { code, BRIDGE, HDR: jsonHeaders() },
  );

// 1. no nested workers, no network of any kind from the plugin
let r = await runJs(`
  const out = {};
  for (const k of ['Worker','SharedWorker','fetch','XMLHttpRequest','WebSocket','importScripts','BroadcastChannel']) out[k] = typeof self[k];
  let spawned = 'no';
  try { const w = new Worker(URL.createObjectURL(new Blob(['fetch("${SINK}/nested")']))); spawned = 'yes'; } catch (e) { spawned = 'threw'; }
  let imported = 'no';
  try { await import('${SINK}/dynamic.js'); imported = 'yes'; } catch (e) { imported = 'threw'; }
  ctx.show({ text: JSON.stringify({ ...out, spawned, imported }) });
  await ctx.run({ cmd: 'rect', from: [0,0,0], to: [1,1,0] });
`);
await new Promise((res) => setTimeout(res, 800));
check(hits.length === 0, `nothing reached the network sink (${hits.join(', ') || 'no hits'})`);
const types = await p.evaluate(async () => {
  const { runPlugin } = await import('/src/plugins.tsx');
  let shown = null;
  await runPlugin({ id: 'x', name: 'X', commands: [{ id: 'go', label: 'Go' }], builtin: false, code: `sketchcraft.command('go', (ctx) => { const o = {}; for (const k of ['Worker','SharedWorker','fetch','XMLHttpRequest','WebSocket','importScripts','BroadcastChannel']) o[k] = typeof self[k]; let s = 'no'; try { new Worker('data:text/javascript,1'); s = 'yes'; } catch { s = 'threw'; } o.spawned = s; ctx.show({ text: JSON.stringify(o) }); });` }, 'go', {
    selection: { faces: [], edges: [], groups: [], instances: [] }, units: 'm', run: async () => ({ ok: true }), show: (res) => (shown = JSON.parse(res.text)), toast: () => {}, ask: async () => null,
  });
  return shown;
});
check(types && ['Worker', 'SharedWorker', 'fetch', 'XMLHttpRequest', 'WebSocket', 'importScripts', 'BroadcastChannel'].every((k) => types[k] === 'undefined') && types.spawned === 'threw', `lockdown: no Worker, no network APIs (${JSON.stringify(types)})`);

// 1b. the same names are closed on the prototypes (a real worker keeps fetch etc. there, not on self)
const protos = await p.evaluate(async () => {
  const { runPlugin } = await import('/src/plugins.tsx');
  let shown = null;
  await runPlugin({ id: 'x', name: 'X', commands: [{ id: 'go', label: 'Go' }], builtin: false, code: `sketchcraft.command('go', async (ctx) => {
    const reach = [];
    for (let o = self; o && o !== Object.prototype; o = Object.getPrototypeOf(o)) for (const k of ['fetch','importScripts','indexedDB','caches','createImageBitmap','XMLHttpRequest']) { try { if (typeof Reflect.get(o, k, self) === 'function' || (k === 'indexedDB' || k === 'caches') && Reflect.get(o, k, self)) reach.push(k); } catch {} }
    let dir = 'none'; try { const g = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(navigator), 'storage'); dir = typeof (g && g.get && g.get.call(navigator)); } catch { dir = 'threw'; }
    ctx.show({ text: JSON.stringify({ reach, dir }) }); });` }, 'go', {
    selection: { faces: [], edges: [], groups: [], instances: [] }, units: 'm', run: async () => ({ ok: true }), show: (res) => (shown = JSON.parse(res.text)), toast: () => {}, ask: async () => null,
  });
  return shown;
});
check(protos && protos.reach.length === 0 && (protos.dir === 'undefined' || protos.dir === 'none' || protos.dir === 'threw'), `lockdown: nothing left on the prototypes (${JSON.stringify(protos)})`);

// 2. what the engine says to a plugin that asks for forbidden things
r = await runJs(`
  await ctx.run({ cmd: 'save_text', path: ${JSON.stringify(marker)}, text: 'x' });
  await ctx.run({ cmd: 'install_plugin', data: 'AAAA', name: 'x.rb' });
  await ctx.run({ cmd: 'plugin_enable', id: 'cutlist', on: false });
  await ctx.run({ cmd: 'remove_plugin', id: 'whatever' });
  await ctx.run({ cmd: 'reveal', path: '/' });
  await ctx.run({ cmd: 'skins' });
  await ctx.run({ cmd: 'add_location', lat: 1, lon: 1 });
  await ctx.run({ cmd: 'photoreal', samples: 4096 });
  await ctx.run({ cmd: 'video_finish', session: 'a', path: '${SINK}/steal.ts' });
`);
check(r.replies.length === 9 && r.replies.every(([, ok]) => ok === false), `JS plugin: all ${r.replies.length} forbidden commands refused (${r.replies.map((x) => x[0]).join(', ')})`);
check(!fs.existsSync(marker), 'JS plugin: the marker file was not written');

// 3. a Ruby plugin that emits file writes: the journal is replayed as plugin commands and refused
const ruby = `UI.menu("Plugins").add_item("Totally Harmless Cleanup") {
  SC.emit({"cmd"=>"save_text","path"=>${JSON.stringify(marker)},"text"=>"pwned"})
  SC.emit({"cmd"=>"install_plugin","data"=>"AAAA","name"=>"evil.rb"})
  SC.emit({"cmd"=>"reveal","path"=>"/"})
  \`globalThis.__journal.push({cmd: "export_obj", path: "${marker}.obj"})\`
}`;
const res = await p.evaluate(
  async ({ ruby, BRIDGE, HDR }) => {
    const { runRuby } = await import('/src/ruby.ts');
    const pl = { id: 'evil', name: 'Evil', ruby: true, builtin: false, commands: [], code: '', entry: ['evil.rb'], files: { 'evil.rb': ruby } };
    const run = await runRuby(pl, { item: 0 });
    const out = [];
    for (const cmd of run.journal) {
      const r = await (await fetch(BRIDGE + '/exec?as=plugin', { method: 'POST', headers: HDR, body: JSON.stringify(cmd) })).json();
      out.push([cmd.cmd, r.ok, r.error]);
    }
    return { out, error: run.error, menu: run.menu };
  },
  { ruby, BRIDGE, HDR: jsonHeaders() },
);
check(res.out.length === 4 && res.out.every(([, ok]) => ok === false), `Ruby plugin: ${res.out.length} emitted commands all refused (${res.out.map((x) => x[0]).join(', ')}) ${res.error ?? ''}`);
check(!fs.existsSync(marker) && !fs.existsSync(marker + '.obj'), 'Ruby plugin: no file was written');

// 4. hostile results can't blank the window
await p.evaluate(async () => {
  const { cleanResult } = await import('/src/pluginclean.ts');
  window.__ok = JSON.stringify(cleanResult({ table: { columns: 1 }, title: {} })).length > 0;
});
check(await p.evaluate(() => window.__ok === true), 'a malformed result is cleaned');
// 5. a plugin run is ONE undo step, and the window's policy is shown to hold in workers
const X = (c, as) => pageExec(p, c, as);
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
const before = (await X({ cmd: 'info' })).faces;
r = await runJs(`for (let i = 0; i < 6; i++) await ctx.run({ cmd: 'rect', from: [10 + i * 5, 0, 0], to: [13 + i * 5, 3, 0] });`);
const mid = (await X({ cmd: 'info' })).faces;
await X({ cmd: 'undo' });
const after = (await X({ cmd: 'info' })).faces;
check(mid === before + 6 && after === before, `a plugin run of 6 commands is one Undo (faces ${before} -> ${mid} -> ${after})`);
// R5-MBK-003: a plugin can't undo or redo (it would be stepping back through the person's own steps)
for (const c of ['undo', 'redo']) {
  const pr = await X({ cmd: c }, true);
  check(pr.ok === false, `a plugin's "${c}" is refused (${JSON.stringify(pr).slice(0, 80)})`);
}
check((await X({ cmd: 'info' })).faces === after, 'and the model is as it was');
const sandbox = await p.evaluate(async () => (await import('/src/cspcheck.ts')).pluginSandboxHolds());
check(sandbox.ok === true, `the window's policy blocks fetch, import() and eval inside workers (${JSON.stringify(sandbox)})`);
check(problems.length === 0, `no page errors (${problems.join('; ').slice(0, 200)})`);

await b.close();
sink.close();
console.log(failed ? `\n${failed} FAILED` : '\nall passed');
process.exit(failed ? 1 : 0);
