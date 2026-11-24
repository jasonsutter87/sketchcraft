#!/usr/bin/env node
// Tools > Cut plan… (#24) in a real browser: a small cabinet becomes a cutting diagram, the stock and prices form saves
// prices with the model, the buy list shows the cost, and the diagrams and the part labels save as PDF files.
// Needs the dev page and bridge on a scratch model:
//   node tools/ui-tests/run.mjs cut-plan
// The model is built with commands from the page and the page is RELOADED (it does not notice changes another client
// makes); then the dialog is driven with real clicks, and the bridge is asked what happened.
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
const OUT = path.join(path.dirname(process.env.SKETCHCRAFT_DOC), 'cut-plan-out');
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
// the Save buttons ask for a path with a prompt in the browser: answer with the next path in the queue
const answers = [];
p.on('dialog', (d) => (d.type() === 'prompt' && answers.length ? d.accept(answers.shift()) : d.dismiss()).catch(() => {}));
const X = (c) => pageExec(p, c);
const sh = (name) => (process.env.SHOT_DIR ? p.screenshot({ path: path.join(process.env.SHOT_DIR, `cut-plan-${name}.png`) }).catch(() => {}) : null);

const board = async (name, lo, hi, material) => {
  const g = (await X({ cmd: 'begin_group', name })).group;
  const f = (await X({ cmd: 'rect', from: [lo[0], lo[1], lo[2]], to: [hi[0], hi[1], lo[2]] })).face;
  await X({ cmd: 'pushpull', face: f, distance: hi[2] - lo[2], toward: [0, 0, 1] });
  await X({ cmd: 'end_group' });
  if (material) {
    await X({ cmd: 'material', name: material, color: [0.8, 0.7, 0.5] });
    await X({ cmd: 'paint', ids: [g], material });
  }
  return g;
};
const T = 0.018;
const cabinet = async () => {
  await X({ cmd: 'new' });
  const ply = 'Birch ply';
  await board('Side left', [0, 0, 0], [T, 0.56, 0.72], ply);
  await board('Side right', [0.582, 0, 0], [0.6, 0.56, 0.72], ply);
  await board('Bottom', [T, 0, 0.1], [0.582, 0.56, 0.1 + T], ply);
  await board('Top', [T, 0, 0.702], [0.582, 0.56, 0.72], ply);
  await board('Shelf', [T, 0.02, 0.4], [0.582, 0.54, 0.4 + T], ply);
  await board('Back', [T, 0.56, 0], [0.582, 0.566, 0.72], 'Hardboard');
  await board('Stile', [0, -0.019, 0], [0.038, 0, 0.72], 'Maple');
  await board('Rail', [0.038, -0.019, 0.68], [0.562, 0, 0.718], 'Maple');
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
};
const openDialog = async () => {
  await p.locator('[role=menubar] > .menubar-item > button[data-menu="tools"]').click();
  await p.locator('[data-action="tools.cutplan"]').click();
  await p.waitForSelector('.sheet.cut-plan svg.cut-diagram, .sheet.cut-plan .cut-view', { timeout: 8000 });
  await wait(300);
};
const toastText = () => p.locator('.toast').first().innerText({ timeout: 2500 }).catch(() => '');

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);

try {
  await cabinet();
  await openDialog();
  const dlg = p.locator('.sheet.cut-plan');
  const note = await dlg.locator('.sheet-head').innerText();
  check(/8 parts/.test(note) && /2 sheets/.test(note) && /1 boards?/.test(note), `the heading counts the parts, sheets and boards (${note.replace(/\s+/g, ' ')})`);
  // the diagram: the first sheet (the plywood) has its five parts drawn
  const first = await dlg.locator('svg.cut-diagram g[data-part]').count();
  check(first === 5, `the first sheet's diagram draws its five parts (${first})`);
  check(!!(await dlg.locator('svg.cut-diagram').getAttribute('aria-label')), 'the diagram has a label for screen readers');
  // pick the second sheet from the list
  await dlg.locator('select').selectOption({ index: 1 });
  await wait(200);
  const second = await dlg.locator('svg.cut-diagram g[data-part]').count();
  check(second === 1, `the second sheet is the hardboard back (${second} part)`);
  await dlg.locator('select').selectOption({ index: 2 });
  await wait(200);
  check((await dlg.locator('svg.cut-diagram g[data-part]').count()) === 2, 'and the board holds the stile and the rail');
  await sh('diagram');

  // stock & prices
  await dlg.locator('button[data-view="stock"]').click();
  const row = dlg.locator('tr[data-material="Birch ply"]');
  check((await row.count()) === 1 && (await dlg.locator('tr[data-material="Maple"]').count()) === 1, 'the form lists each material the plan uses');
  await row.locator('input').nth(0).fill('64.5');
  await dlg.locator('tr[data-material="Hardboard"] input').nth(0).fill('20');
  await dlg.locator('tr[data-material="Maple"] input').nth(2).fill('9');
  await dlg.locator('tr[data-material="Maple"] input[type=checkbox]').check();
  // a silly kerf is refused where it is typed, nothing is sent
  await dlg.locator('input[aria-label="Saw kerf"]').fill('wide');
  await dlg.getByRole('button', { name: 'Update plan' }).click();
  await wait(300);
  check(/kerf/i.test(await dlg.locator('[role=alert]').innerText().catch(() => '')), 'a kerf that is not a length is refused with a message');
  await dlg.locator('input[aria-label="Saw kerf"]').fill('3');
  await dlg.getByRole('button', { name: 'Update plan' }).click();
  await wait(700);
  const saved = await X({ cmd: 'cut_plan' });
  check(saved.stock.materials['Birch ply']?.sheet_price === 64.5 && saved.stock.materials.Maple?.grain === true, 'the prices and the grain are kept in the model');
  check(Math.abs(saved.cost.total - (64.5 + 20 + saved.cost.by_material.find((m) => m.material === 'Maple').board_feet * 9)) < 0.06, `the engine's cost is the stock at those prices ($${saved.cost.total})`);

  // what to buy
  await dlg.locator('button[data-view="buy"]').click();
  const total = await dlg.locator('[data-total]').getAttribute('data-total');
  check(Math.abs(Number(total) - saved.cost.total) < 0.005, `the buy list shows the same total (${total})`);
  check((await dlg.locator('tr[data-material="Birch ply"]').innerText()).includes('$64.50'), 'and the plywood row shows $64.50');
  await sh('buy');

  // the files
  const pdf = path.join(OUT, 'diagrams.pdf');
  const labels = path.join(OUT, 'labels.pdf');
  answers.push(pdf);
  await dlg.locator('[data-action="export-diagrams"]').click();
  await wait(900);
  check(fs.existsSync(pdf) && fs.readFileSync(pdf).subarray(0, 5).toString() === '%PDF-', 'Export diagrams writes a PDF');
  check(/diagrams\.pdf/.test(await toastText()), 'and says which file it saved');
  answers.push(labels);
  await dlg.locator('[data-action="print-labels"]').click();
  await wait(900);
  check(fs.existsSync(labels) && fs.readFileSync(labels).subarray(0, 5).toString() === '%PDF-', 'Print labels writes a PDF');
  const svg = path.join(OUT, 'diagrams.svg');
  answers.push(svg);
  await dlg.locator('[data-action="export-svg"]').click();
  await wait(900);
  check(fs.existsSync(svg) && fs.readFileSync(svg, 'utf8').includes('Side left'), 'Export SVG writes the diagrams with the part names');

  // Escape closes it; nothing in the model changed besides the stock
  await p.keyboard.press('Escape');
  await wait(200);
  check((await p.locator('.sheet.cut-plan').count()) === 0, 'Escape closes the dialog');
  // one Undo takes the stock back
  const before = await X({ cmd: 'cut_plan' });
  await X({ cmd: 'undo' });
  const after = await X({ cmd: 'cut_plan' });
  check(before.stock.materials['Birch ply']?.sheet_price === 64.5 && after.stock.materials['Birch ply']?.sheet_price !== 64.5, 'Undo takes the prices back');
} catch (e) {
  check(false, `stopped early: ${String(e.message ?? e).split('\n')[0]}`);
  await sh('failed');
}

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
