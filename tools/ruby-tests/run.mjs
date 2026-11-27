#!/usr/bin/env node
// Run the SketchUp Ruby test scripts through SketchCraft and check what they
// build. Needs the bridge and dev page (see docs/dev/building.md).
//   node tools/ruby-tests/run.mjs
import fs from 'node:fs';
import path from 'node:path';
import { BRIDGE, jsonHeaders, launch, PAGE, pageExec, ROOT } from '../demos/lib.mjs';

const dir = path.join(ROOT, 'tools/ruby-tests');
const b64 = (f) => fs.readFileSync(f).toString('base64');
// the House Kit as an .rbz (a zip), built here
const { execFileSync } = await import('node:child_process');
const rbz = path.join(dir, '.house_kit.rbz');
fs.rmSync(rbz, { force: true });
execFileSync('zip', ['-qr', rbz, 'house_kit.rb', 'house_kit'], { cwd: dir });

const b = await launch();
const p = await (await b.newContext({ locale: 'en-US' })).newPage();
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
// X: as the person using the app (installing plugins, new model); the page's own calls carry the token too
const HDR = jsonHeaders();
const X = (c) => pageExec(p, c);
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

// run one menu item of an installed plugin, answering forms with their defaults
async function runItem(id, label, selection = [], events = []) {
  return p.evaluate(async ({ id, label, selection, events, BRIDGE, HDR }) => {
    const { runRuby, modelForRuby } = await import('/src/ruby.ts');
    const call = async (c, as) => (await fetch(BRIDGE + '/exec' + (as ? '?as=plugin' : ''), { method: 'POST', headers: HDR, body: JSON.stringify(c) })).json();
    const list = await call({ cmd: 'plugins' });
    const pl = list.plugins.find((x) => x.id === id);
    if (!pl) return { error: 'not installed' };
    const menu = (await runRuby(pl, {})).menu;
    const item = menu.indexOf(label);
    if (item < 0) return { error: `no menu item ${label} (has ${menu.join(', ')})` };
    const answers = [];
    // the script reads the model as the app gives it (faces with their corners, groups)
    const model = modelForRuby(await call({ cmd: 'faces', outlines: true }, 'plugin'), await call({ cmd: 'info' }, 'plugin'));
    for (let k = 0; k < 6; k++) {
      const r = await runRuby(pl, { item, answers, selection, events, model });
      if (r.need) { answers.push(r.need.defaults.map((d, i) => (r.need.lists[i] && r.need.lists[i].includes('|') ? d || r.need.lists[i].split('|')[0] : d))); continue; }
      if (r.error) return { error: r.error, menu };
      for (const cmd of r.journal) {
        // replayed as plugin commands, like the app does
        const res = await call(cmd, 'plugin');
        if (!res.ok) return { error: `replay ${cmd.cmd}: ${res.error}`, menu };
      }
      return { ok: true, menu, out: r.out, toasts: r.toasts, dialogs: r.dialogs, status: r.status, steps: r.journal.length };
    }
    return { error: 'kept asking' };
  }, { id, label, selection, events, BRIDGE, HDR });
}

const parts = async () => (await X({ cmd: 'parts' })).parts;
const inch = (m) => Math.round(m / 0.0254);

for (const f of ['stairs.rb', 'fence_posts.rb', 'selection.rb', 'unsupported.rb', 'ext/bezier_tool.rb', 'ext/make_face_tool.rb', 'ext/observers_dialog.rb', 'ext/datum_height.rb']) check((await X({ cmd: 'install_plugin', data: b64(path.join(dir, f)), name: path.basename(f), replace: true })).ok, `install ${f}`);
check((await X({ cmd: 'install_plugin', data: b64(rbz), name: 'house_kit.rbz', replace: true })).ok, 'install house_kit.rbz');

await X({ cmd: 'new' });
let r = await runItem('stairs', 'Stairs…');
check(r.ok, `Stairs ran (${r.error ?? r.steps + ' steps'})`);
let ps = await parts();
const steps = ps.filter((q) => q.name.startsWith('Step'));
check(steps.length === 8, `8 steps (got ${steps.length})`);
check(steps.every((q) => inch(q.size[0]) === 36 && inch(q.size[1]) === 11 && inch(q.size[2]) === 7), `each step 36 x 11 x 7 in (${JSON.stringify(steps[0]?.size.map(inch))})`);
check(steps.every((q) => q.material === 'Wood'), 'painted Wood');

await X({ cmd: 'new' });
r = await runItem('fence-posts', 'Fence posts');
check(r.ok, `Fence posts ran (${r.error ?? r.steps + ' steps'})`);
ps = await parts();
check(ps.length === 10 && ps.every((q) => q.kind === 'component'), `10 post components (got ${ps.length} ${[...new Set(ps.map((q) => q.kind))]})`);
check(ps.every((q) => inch(q.size[0]) === 36 && inch(q.size[1]) === 4), `each post 36 x 4 x 4 in (${JSON.stringify(ps[0]?.size.map(inch))})`);

await X({ cmd: 'new' });
r = await runItem('house-kit', 'Build a house');
check(r.ok, `House Kit (.rbz, extension loader) ran (${r.error ?? r.steps + ' steps'})`);
ps = await parts();
const house = ps.find((q) => q.name === 'House');
check(!!house && inch(house.size[0]) === 360 && inch(house.size[1]) === 240 && inch(house.size[2]) === 156, `a 360 x 240 in house, 156 in to the ridge (${JSON.stringify(house?.size.map(inch))})`);
check((await X({ cmd: 'info' })).materials.includes('Roof'), 'the Roof material');

const g = (await X({ cmd: 'info' })).groups.find((x) => x.name === 'House');
r = await runItem('selection', 'Paint selection red', [{ id: g.id, name: 'House', kind: 'group' }]);
check(r.ok && /Painted 1/.test(r.out) && r.toasts.includes('Painted 1'), `selection: ${r.error ?? JSON.stringify([r.out.trim(), r.toasts])}`);

r = await runItem('unsupported', 'Import a file');
check(!r.ok && /import/.test(r.error ?? ''), `unsupported API says so: ${r.error}`);

// ---- the grown API: tools, reading the model, Follow Me, textures, dialogs (fixtures in ext/) ----
const at = (x, y, z) => [x, y, z];
const ev = (type, a, extra = {}) => ({ type, x: 5, y: 5, at: a, ...extra });
await X({ cmd: 'new' });
r = await runItem('bezier-tool', 'Bezier tool', [], [ev('lbutton_down', at(0, 0, 0)), ev('lbutton_down', at(1, 2, 0)), ev('lbutton_down', at(2, 0, 0)), { type: 'user_text', text: '6' }, { type: 'return' }]);
check(r.ok && r.steps >= 1, `Bezier tool drew a curve from clicks (${r.error ?? r.steps + ' steps'})`);
check(/Click control points/.test(r.status ?? ''), `tool set the status bar (${r.status})`);

// the script reads the faces that are in the model
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 1, 0] });
r = await runItem('make-face-tool', 'Make face from outline');
check(r.ok && /outline of 4 points/.test(r.out), `Make face read the model's face (${r.error ?? r.out.trim()})`);
check((await parts()).some((q) => q.kind === 'group'), 'Make face built a group');
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 1, 0] });
r = await runItem('make-face-tool', 'Rail around outline');
check(r.ok, `Follow Me ran in the engine (${r.error ?? r.steps + ' steps'})`);

// a dialog and its callbacks, observers, a timer, and a texture
await X({ cmd: 'new' });
r = await runItem('observers-dialog', 'Panel', [], [{ type: 'dialog', id: 0, name: 'make_box', params: '1' }, { type: 'tick' }]);
check(r.ok && r.dialogs?.length === 1 && r.dialogs[0].visible, `a dialog opened and its callback built a box (${r.error ?? r.steps + ' steps'})`);
check((await X({ cmd: 'faces' })).faces.length >= 6, 'the dialog callback made a box (6 faces)');
r = await runItem('observers-dialog', 'Textured');
check(r.ok && /texture 8/.test(r.out), `texture material (${r.error ?? r.out.trim()})`);
check((await X({ cmd: 'info' })).materials.includes('Brick'), 'the Brick material is in the model');

for (const id of ['stairs', 'fence-posts', 'selection', 'unsupported', 'house-kit', 'bezier-tool', 'make-face-tool', 'observers-dialog', 'datum-height']) await X({ cmd: 'remove_plugin', id });
fs.rmSync(rbz, { force: true });
await b.close();
console.log(failed ? `\n${failed} failed` : '\nall passed');
process.exit(failed ? 1 : 0);
