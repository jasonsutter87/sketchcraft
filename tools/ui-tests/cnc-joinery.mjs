#!/usr/bin/env node
// Tools > Joinery > Dado and Cut plan > CNC files (#24) in a real browser: two touching boards get a dado from the
// menu and its prompt (one undo step, the board's size unchanged), and Cut plan writes a .zip of DXF files, one per part.
// Needs the dev page and bridge on a scratch model:
//   node tools/ui-tests/run.mjs cnc-joinery
// The model is built with commands from the page and the page is RELOADED (it does not notice changes another client
// makes); then the menus and dialogs are driven with real clicks, and the bridge is asked what happened.
import fs from 'node:fs';
import path from 'node:path';
import { launch, PAGE, pageExec } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same); tools/ui-tests/run.mjs does.');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
let passed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (ok) passed++;
  else failed++;
};
const OUT = path.join(path.dirname(process.env.SKETCHCRAFT_DOC), 'cnc-joinery-out');
fs.mkdirSync(OUT, { recursive: true });

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1500, height: 950 } });
await ctx.addInitScript(() => {
  try {
    localStorage.setItem('sketchcraft.help', 'off');
  } catch {}
});
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
// the Save button asks for a path with a prompt in the browser: answer with the next path in the queue
const answers = [];
p.on('dialog', (d) => (d.type() === 'prompt' && answers.length ? d.accept(answers.shift()) : d.dismiss()).catch(() => {}));
const X = (c) => pageExec(p, c);
const toastText = () => p.locator('.toast').first().innerText({ timeout: 2500 }).catch(() => '');

const board = async (name, lo, hi) => {
  const g = (await X({ cmd: 'begin_group', name })).group;
  const f = (await X({ cmd: 'rect', from: [lo[0], lo[1], lo[2]], to: [hi[0], hi[1], lo[2]] })).face;
  await X({ cmd: 'pushpull', face: f, distance: hi[2] - lo[2], toward: [0, 0, 1] });
  await X({ cmd: 'end_group' });
  return g;
};
const T = 0.018;
const build = async () => {
  await X({ cmd: 'new' });
  await board('Side', [0, 0, 0], [T, 0.56, 0.72]);
  await board('Shelf', [T, 0.02, 0.3], [0.582, 0.54, 0.3 + T]);
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
};
const jointItem = async (id) => {
  await p.locator('[role=menubar] > .menubar-item > button[data-menu="tools"]').click();
  await p.getByRole('menuitem', { name: 'Joinery' }).hover();
  await p.locator(`[data-action="${id}"]`).click();
  await wait(300);
};
const sizes = async () => Object.fromEntries((await X({ cmd: 'parts' })).parts.map((q) => [q.name, q.size]).sort((a, c) => a[0].localeCompare(c[0])));
const volume = async (name) => {
  const part = (await X({ cmd: 'parts' })).parts.find((q) => q.name === name);
  return (await X({ cmd: 'volume', id: part.id })).volume;
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);

try {
  await build();
  const sizesBefore = await sizes();
  const sideBefore = await volume('Side');

  // nothing picked: it says what to pick, and opens nothing
  await jointItem('tools.dado');
  check(/two boards/i.test(await toastText()), 'Dado with nothing picked says to pick two boards');
  check((await p.locator('.sheet.joinery').count()) === 0, 'and opens no prompt');

  // both boards picked: the prompt names them and which one is cut
  await p.locator('.viewport canvas').click({ position: { x: 850, y: 120 } });
  await p.keyboard.press('ControlOrMeta+a');
  await jointItem('tools.dado');
  const dlg = p.locator('.sheet.joinery');
  await dlg.waitFor({ timeout: 4000 });
  const options = await dlg.locator('select option').allInnerTexts();
  check(options.length === 2 && options.includes('Side') && options.includes('Shelf'), `the prompt names the two boards (${options})`);
  await dlg.locator('select').selectOption({ label: 'Side' });
  check((await dlg.locator('[data-cut]').getAttribute('data-cut')) === 'Side' && (await dlg.locator('[data-fit]').getAttribute('data-fit')) === 'Shelf', 'it says the side is cut and the shelf fits into it');
  // a depth that is not a length is refused where it is typed
  await dlg.locator('input[aria-label="Depth"]').fill('deep');
  await dlg.locator('[data-action="joint-apply"]').click();
  await wait(200);
  check(/Depth/.test(await dlg.locator('[role=alert]').innerText().catch(() => '')), 'a depth that is not a length is refused with a message');
  // 8 mm deep: the engine cuts it
  await dlg.locator('input[aria-label="Depth"]').fill('8');
  await dlg.locator('[data-action="joint-apply"]').click();
  await wait(900);
  check((await p.locator('.sheet.joinery').count()) === 0, 'the prompt closes when the dado is cut');
  check(/Dado cut/.test(await toastText()), `the toast says what was cut (${(await toastText()).slice(0, 60)})`);
  const want = 0.52 * 0.018 * 0.008;
  const sideAfter = await volume('Side');
  check(Math.abs(sideBefore - sideAfter - want) < want * 0.01, `the side lost the dado's volume (${((sideBefore - sideAfter) * 1e6).toFixed(1)} cm3)`);
  check(JSON.stringify(await sizes()) === JSON.stringify(sizesBefore), "and the cut list sizes are the same");

  // one Undo takes it back
  await X({ cmd: 'undo' });
  check(Math.abs((await volume('Side')) - sideBefore) < 1e-12, 'one Undo gives the side back');

  // boards that do not touch are refused with the engine's reason
  const far = await board('Floater', [0.2, 0.2, 0.9], [0.4, 0.4, 0.918]);
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  const side = (await X({ cmd: 'parts' })).parts.find((q) => q.name === 'Side').id;
  const r = await X({ cmd: 'dado', ids: [side, far] });
  const refused = r.ok ? 'ok' : String(r.error);
  check(/touch/.test(refused), `boards that do not touch are refused (${refused.slice(0, 80)})`);
  await X({ cmd: 'delete', ids: [far] });

  // CNC files from the Cut plan dialog
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  await p.locator('[role=menubar] > .menubar-item > button[data-menu="tools"]').click();
  await p.locator('[data-action="tools.cutplan"]').click();
  const cut = p.locator('.sheet.cut-plan');
  await cut.waitFor({ timeout: 8000 });
  await cut.locator('[data-action="export-cnc"]').click();
  check((await cut.locator('input[aria-label="Tool diameter"]').inputValue()) === '6', 'the CNC page starts with a 6 mm tool');
  await cut.locator('input[aria-label="Tool diameter"]').fill('wide');
  const zip = path.join(OUT, 'cnc.zip');
  answers.push(zip);
  await cut.locator('[data-action="cnc-save"]').click();
  await wait(300);
  check(/tool diameter/i.test(await cut.locator('[role=alert]').innerText().catch(() => '')) && !fs.existsSync(zip), 'a tool diameter that is not a length is refused and nothing is written');
  await cut.locator('input[aria-label="Tool diameter"]').fill('6');
  await cut.locator('input[aria-label="Dog-bone corners"]').check();
  answers.push(zip);
  await cut.locator('[data-action="cnc-save"]').click();
  await wait(1200);
  const bytes = fs.existsSync(zip) ? fs.readFileSync(zip) : Buffer.alloc(0);
  check(bytes.subarray(0, 2).toString() === 'PK', 'Save writes a .zip');
  const text = bytes.toString('latin1');
  check(/01-Side\.dxf/.test(text) && /02-Shelf\.dxf/.test(text), 'with one DXF per part, numbered like the cut plan (01-Side.dxf, 02-Shelf.dxf)');
  check(/Saved cnc\.zip: 2 parts/.test(await toastText()), `the toast says what it saved (${(await toastText()).slice(0, 60)})`);
} catch (e) {
  check(false, `stopped early: ${String(e.message ?? e).split('\n')[0]}`);
}

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
