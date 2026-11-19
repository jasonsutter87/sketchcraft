#!/usr/bin/env node
// Tables and text blocks on sheets (#7, design step 14), in a real browser against the dev page and bridge: a small
// door schedule made by mouse and keyboard. Draw a table with the Table tool, type its cells (Tab across, Enter
// down), change a cell by double-click, title it and add a row in the inspector, paste rows copied from a
// spreadsheet into it, draw a text block that wraps, undo and redo one step per edit, and export the page to PDF,
// SVG and DXF with the schedule in each. Then save, open again and check the page came back the same.
//   SHOT_DIR=/some/folder node tools/ui-tests/sheet-tables.mjs
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const shots = process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-shots-'));
fs.mkdirSync(shots, { recursive: true });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};
const until = async (f, ms = 8000) => {
  const t0 = Date.now();
  let v;
  while (Date.now() - t0 < ms) {
    v = await f();
    if (v) return v;
    await wait(150);
  }
  return v;
};

const b = await launch();
const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } });
await ctx.addInitScript(() => {
  try {
    localStorage.setItem('sketchcraft.workspace', 'model');
    localStorage.setItem('sketchcraft.help', 'off');
    localStorage.removeItem('sketchcraft.paper.opts');
  } catch {}
});
const p = await ctx.newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(400);
await pageExec(p, { cmd: 'new' });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(800);

// ---- a page -------------------------------------------------------------------------------------------------------
await p.getByRole('tab', { name: 'Layout', exact: true }).click();
await p.waitForSelector('.layout-empty');
await p.locator('.layout-empty').getByRole('button', { name: /Add page/ }).click();
const info = async () => (await pageExec(p, { cmd: 'sheets_info' })).sheets?.[0];
await until(async () => !!(await info()));
const sheet = (await info()).id;
await p.waitForSelector('.paper-tools');
const items = async () => (await info())?.items ?? [];
const tables = async () => (await items()).filter((i) => i.kind === 'table');
const table = async () => (await tables())[0];
const rows = async () => (await table())?.rows;

const edge = async () => p.locator('.paper-overlay .paper-edge').boundingBox();
const scr = async ([x, y]) => {
  const e = await edge();
  const k = e.width / 420;
  return [e.x + x * k, e.y + y * k];
};
const move = async (mm, steps = 1) => {
  const [x, y] = await scr(mm);
  await p.mouse.move(x, y, { steps });
};
const click = async (mm) => {
  await move(mm);
  await wait(60);
  await p.mouse.down();
  await p.mouse.up();
  await wait(150);
};
const drag = async (a, z) => {
  await move(a);
  await p.mouse.down();
  await move(z, 8);
  await wait(60);
  await p.mouse.up();
  await wait(150);
};
const tool = async (name) => {
  await p.locator('.paper-tools').getByRole('button', { name, exact: true }).click();
  await wait(80);
};
const hint = () => p.locator('.paper-hint').textContent();
const active = () => p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label');
const undo = async () => {
  await p.locator('.paper-view').focus();
  await p.keyboard.press('ControlOrMeta+z');
};
const redo = async () => {
  await p.locator('.paper-view').focus();
  await p.keyboard.press('ControlOrMeta+Shift+z');
};
/** the centre of a cell, in paper mm, from the engine's layout */
const cellMid = async (r, c) => {
  const t = await table();
  const { xs, ys } = t.layout;
  return [(xs[c] + xs[c + 1]) / 2, (ys[r] + ys[r + 1]) / 2];
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ---- the Table tool: a frame makes a table, typing starts in its first cell ------------------------------------
check((await p.locator('.paper-tools .paper-tool').count()) === 13, 'the tool bar has the Table tool (13 tools)');
await p.locator('.paper-view').focus();
await p.keyboard.press('b');
await until(async () => (await active()) === 'Table', 2000);
check((await active()) === 'Table', 'B picks the Table tool');
check(/Click for a table/.test(await hint()), `its hint says what to do (${await hint()})`);
await drag([20, 30], [95, 54]);
await until(async () => (await tables()).length === 1);
let t = await table();
check(!!t && same(t.at_mm, [20, 30]) && t.rows.length === 4 && t.rows[0].length === 3, `a 75 x 24 mm frame makes a 4 row, 3 column table at its corner (${JSON.stringify(t?.at_mm)}, ${t?.rows?.length} x ${t?.rows?.[0]?.length})`);
await p.waitForSelector('.paper-cellbox');
check(await p.locator('.paper-cellbox').evaluate((e) => e === document.activeElement), 'typing starts in its first cell');
check(/Tab next cell/.test(await hint()), 'the hint says how to move between cells');
// header, then two doors: Tab goes across and on to the next row
const typed = ['Mark', 'Size', 'Type', 'D1', '900 x 2100', 'Single', 'D2', '1800 x 2100', 'Double'];
for (const [i, w] of typed.entries()) {
  await p.waitForSelector('.paper-cellbox');
  await p.keyboard.type(w);
  await p.keyboard.press(i === typed.length - 1 ? 'Enter' : 'Tab');
  await wait(120);
}
await until(async () => same((await rows())?.[2], ['D2', '1800 x 2100', 'Double']));
check(same(await rows(), [['Mark', 'Size', 'Type'], ['D1', '900 x 2100', 'Single'], ['D2', '1800 x 2100', 'Double'], ['', '', '']]), `typed cell by cell (${JSON.stringify(await rows())})`);
// Enter on the last one went down a row: Escape stops typing there without changing it
check((await p.locator('.paper-cellbox').count()) === 1, 'Enter moved down to the next row');
await p.keyboard.press('Escape');
await until(async () => (await p.locator('.paper-cellbox').count()) === 0, 2000);
check((await p.locator('.paper-cellbox').count()) === 0, 'Escape stops typing');
// one undo step per cell
await undo();
await until(async () => (await rows())?.[2]?.[2] === '');
check((await rows())?.[2]?.[2] === '', 'one Undo takes back the last cell typed');
await redo();
await until(async () => (await rows())?.[2]?.[2] === 'Double');
check((await rows())?.[2]?.[2] === 'Double', 'Redo puts it back');

// ---- change a cell by double-click --------------------------------------------------------------------------------------
await tool('Select');
const [cx, cy] = await scr(await cellMid(1, 1));
await p.mouse.dblclick(cx, cy);
await p.waitForSelector('.paper-cellbox');
check((await p.locator('.paper-cellbox').inputValue()) === '900 x 2100', 'double-clicking a cell opens it with its text');
await p.keyboard.press('ControlOrMeta+a');
await p.keyboard.type('910 x 2100');
await p.keyboard.press('Shift+Enter');
await until(async () => (await rows())?.[1]?.[1] === '910 x 2100');
check((await rows())?.[1]?.[1] === '910 x 2100', 'the cell is changed');
check((await p.locator('.paper-cellbox').count()) === 0, 'Shift+Enter finishes without moving on');

// ---- the inspector: title, a row added, borders ---------------------------------------------------------------------------
await click(await cellMid(0, 0));
await p.waitForSelector('.layout-inspector[aria-label="Table"]');
check((await p.locator('.layout-inspector[aria-label="Table"]').count()) === 1, 'clicking the table shows its inspector');
const insp = p.locator('.layout-inspector[aria-label="Table"]');
await insp.getByLabel('Title', { exact: true }).fill('DOOR SCHEDULE');
await insp.getByLabel('Title', { exact: true }).press('Enter');
await until(async () => (await table())?.title === 'DOOR SCHEDULE');
check((await table())?.title === 'DOOR SCHEDULE', 'the title is typed in the inspector');
await insp.getByRole('button', { name: 'Add a row' }).click();
await until(async () => (await rows())?.length === 5);
check((await rows())?.length === 5, 'Add a row adds one at the bottom');
await insp.getByLabel('Table borders').selectOption('rules');
await until(async () => (await table())?.border === 'rules');
check((await table())?.border === 'rules', 'the borders are chosen');

// ---- paste rows copied from a spreadsheet into the table, from the clicked cell --------------------------------------------
await click(await cellMid(3, 0));
await p.locator('.paper-view').focus();
const tsv = 'D3\t800 x 2100\tSingle\r\nD4\t900 x 2100\t"Single\nglazed"\r\nD5\t1000 x 2100\tSingle\r\n';
await p.evaluate((text) => {
  const dt = new DataTransfer();
  dt.setData('text/plain', text);
  document.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
}, tsv);
await until(async () => (await rows())?.length === 6);
const r = await rows();
check(r?.length === 6 && same(r[3], ['D3', '800 x 2100', 'Single']) && same(r[4], ['D4', '900 x 2100', 'Single\nglazed']) && r[5][0] === 'D5', `the pasted rows fill it from the clicked cell, growing it (${JSON.stringify(r?.slice(3))})`);
await undo();
await until(async () => (await rows())?.length === 5);
check((await rows())?.length === 5 && (await rows())[3][0] === '', 'one Undo takes the whole paste back');
await redo();
await until(async () => (await rows())?.length === 6);

// ---- a text block: drag across with the Text tool, the words wrap ---------------------------------------------------
await tool('Text');
await drag([120, 30], [200, 40]);
await p.waitForSelector('.paper-textbox');
const words = 'All doors to be hung on three hinges. Fire doors to close fully from any angle and to be fitted with intumescent seals.';
await p.keyboard.type(words);
await p.keyboard.press('Enter');
await until(async () => (await items()).some((i) => i.kind === 'text'));
const block = (await items()).find((i) => i.kind === 'text');
check(same(block?.box_mm, [80, 0]) && block?.paragraphs?.[0] === words, `a drag makes a text block as wide as the drag (${JSON.stringify(block?.box_mm)})`);
check((block?.layout?.lines ?? 0) >= 2 && Math.abs(block.layout.rect[2] - 80) < 0.01, `and it wraps to that width (${block?.layout?.lines} lines)`);

// ---- printed: the schedule and the block in the PDF, SVG and DXF -------------------------------------------------------------
const dir = path.dirname(process.env.SKETCHCRAFT_DOC);
for (const ext of ['pdf', 'svg', 'dxf']) {
  const file = path.join(dir, `schedule.${ext}`);
  const x = await pageExec(p, { cmd: 'export_sheets', path: file, pages: [sheet] });
  const ok = x.ok && fs.existsSync(file) && fs.statSync(file).size > 1000;
  const body = ext === 'pdf' ? '' : fs.readFileSync(file, 'utf8');
  check(ok && (ext === 'pdf' || (body.includes('DOOR SCHEDULE') && body.includes('1000 x 2100') && body.includes('hinges'))), `exported to ${ext.toUpperCase()}${ext === 'pdf' ? '' : ' with the schedule and the block in it'} (${x.error ?? ''})`);
}
const ir = JSON.stringify(await pageExec(p, { cmd: 'sheet_ir', sheet }));
check(ir.includes('DOOR SCHEDULE') && ir.includes('glazed') && ir.includes('intumescent'), 'the page on screen draws the table and the block');
await tool('Select');
await p.mouse.move(2, 2);
await wait(500);
await p.screenshot({ path: path.join(shots, 'sheet-tables.png') });

// ---- keyboard and screen readers ---------------------------------------------------------------------------------------
const labels = await p.locator('.paper-overlay [data-focus]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
check(labels.some((l) => /Table "DOOR SCHEDULE", 6 rows by 3 columns/.test(l)) && labels.some((l) => /^Text block/.test(l)), 'the table and the block have labels for screen readers');

// ---- saved and opened again ------------------------------------------------------------------------------------------
const before = await info();
const file = path.join(dir, 'tables.sketchcraft');
check((await pageExec(p, { cmd: 'save', path: file })).ok, 'saved');
await pageExec(p, { cmd: 'new' });
await until(async () => !(await info()));
check((await pageExec(p, { cmd: 'open', path: file })).ok, 'opened again');
const after = await info();
check(JSON.stringify(after) === JSON.stringify(before), 'the page came back exactly: the table, its cells and the text block');

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
