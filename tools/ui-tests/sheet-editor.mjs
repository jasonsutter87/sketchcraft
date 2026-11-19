#!/usr/bin/env node
// The sheet editor (#9, design step 12), in a real browser against the dev page and bridge: a sheet laid out entirely
// by mouse. Add a page, draw two viewports (picking their views), move one with snapping to the margin and another
// frame, resize it by a handle (snapped to the grid), nudge it with the keyboard, add a 3-point chain dimension
// attached to the model, place text and change it by double-click, draw lines, trim and extend them (with their hover
// previews), type the title block's project on the page, delete with the Delete tool, Escape a gesture; every gesture
// is one undo step (Ctrl+Z / Ctrl+Shift+Z). Then save, open the file again, and check the page came back the same.
//   SHOT_DIR=/some/folder node tools/ui-tests/sheet-editor.mjs
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
const near = (a, b, tol = 0.02) => Math.abs(a - b) <= tol;

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

// a 6 x 4 m box, 3 m high
await pageExec(p, { cmd: 'new' });
await pageExec(p, { cmd: 'units', units: 'm' });
const f = (await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] })).face;
await pageExec(p, { cmd: 'pushpull', face: f, distance: 3 });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);

// ---- a page ---------------------------------------------------------------------------------------------------------
await p.getByRole('tab', { name: 'Layout', exact: true }).click();
await p.waitForSelector('.layout-empty');
await p.locator('.layout-empty').getByRole('button', { name: /Add page/ }).click();
const info = async () => (await pageExec(p, { cmd: 'sheets_info' })).sheets?.[0];
await until(async () => !!(await info()));
const sheet = (await info()).id;
await p.waitForSelector('.paper-tools');
check((await p.locator('.paper-tools .paper-tool').count()) === 13, 'the paper has a tool bar of 13 tools');
check((await p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label')) === 'Select', 'Select is the tool to begin with');
const items = async () => (await info())?.items ?? [];
const ofKind = async (k) => (await items()).filter((i) => i.kind === k);

// paper mm <-> screen: the page's edge on the overlay
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
  await wait(120);
};
const dragMm = async (a, z, steps = 8) => {
  await move(a);
  await p.mouse.down();
  await move(z, steps);
  await wait(60);
  return async () => {
    await p.mouse.up();
    await wait(150);
  };
};
const tool = async (name) => {
  await p.locator('.paper-tools').getByRole('button', { name, exact: true }).click();
  await wait(80);
};
const hint = () => p.locator('.paper-hint').textContent();
const undo = async () => {
  await p.locator('.paper-view').focus();
  await p.keyboard.press('ControlOrMeta+z');
};
const redo = async () => {
  await p.locator('.paper-view').focus();
  await p.keyboard.press('ControlOrMeta+Shift+z');
};
const vpRect = async (id) => (await items()).find((i) => i.id === id)?.rect_mm;

// ---- two viewports, each drawn as a frame -------------------------------------------------------------------
await tool('Viewport');
check(/data:image\/svg\+xml/.test(await p.locator('.paper-view').evaluate((e) => e.style.cursor)), "the cursor shows the tool's icon");
check(/Drag a frame/.test(await hint()), `the hint says what to do (${await hint()})`);
let release = await dragMm([20.6, 19.4], [199.2, 160.4]);
check(/Let go/.test(await hint()), 'while dragging the hint says to let go');
check((await p.locator('.paper-overlay .ed-draw').count()) === 1, 'the frame is previewed');
await release();
await p.waitForSelector('.paper-chooser');
check(/Pick the view/.test(await hint()), 'then it asks which view goes in it');
await p.locator('.paper-chooser').getByRole('button', { name: 'Floor plan' }).click();
await until(async () => (await ofKind('viewport')).length === 1);
const plan = (await ofKind('viewport'))[0];
check(JSON.stringify(plan.rect_mm) === JSON.stringify([20, 20, 180, 140]) && plan.view?.type === 'plan', `the plan's frame is where it was dragged, on the 5 mm grid (${JSON.stringify(plan.rect_mm)}, ${plan.view?.type})`);
check((await p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label')) === 'Select' && (await p.locator('.layout-inspector[aria-label="Viewport"]').count()) === 1, 'placing it goes back to Select with the new view picked');
await undo();
await until(async () => (await ofKind('viewport')).length === 0);
check((await ofKind('viewport')).length === 0, 'one Undo takes the new viewport away');
await redo();
await until(async () => (await ofKind('viewport')).length === 1);
check((await ofKind('viewport')).length === 1, 'Redo brings it back');

await tool('Viewport');
release = await dragMm([221, 20], [399, 160.3]);
await release();
await p.waitForSelector('.paper-chooser');
await p.locator('.paper-chooser').getByRole('button', { name: 'Front elevation' }).click();
await until(async () => (await ofKind('viewport')).length === 2);
const front = (await ofKind('viewport')).find((v) => v.id !== plan.id);
check(front?.view?.type === 'elevation' && JSON.stringify(front.rect_mm) === JSON.stringify([220, 20, 180, 140]), `a second viewport, a front elevation (${JSON.stringify(front?.rect_mm)})`);
// a frame can be escaped before a view is chosen
await tool('Viewport');
release = await dragMm([30, 170], [80, 200]);
await release();
await p.waitForSelector('.paper-chooser');
await p.keyboard.press('Escape');
await until(async () => (await p.locator('.paper-chooser').count()) === 0);
check((await ofKind('viewport')).length === 2, 'Escape at the view chooser adds nothing');
for (let i = 0; i < 60 && (await p.locator('.paper-status').count()); i++) await wait(200);

// ---- move with snapping ------------------------------------------------------------------------------------------------
await tool('Select');
await p.keyboard.press('Escape');
release = await dragMm([110, 85], [100.6, 87.2], 10);
const guides = await p.locator('.paper-overlay .snap-guide').count();
check(guides >= 1, `while moving, snap guides show (${guides})`);
check((await p.locator('.paper-overlay .ed-live').count()) === 1, 'and the frame follows the mouse');
await release();
await until(async () => (await vpRect(plan.id))?.[0] === 10);
let r = await vpRect(plan.id);
check(r?.[0] === 10 && r?.[1] === 20 && r?.[2] === 180, `dragging the plan snaps its left edge to the margin and its top to the other frame's (${JSON.stringify(r)})`);
await undo();
await until(async () => (await vpRect(plan.id))?.[0] === 20);
check((await vpRect(plan.id))?.[0] === 20, 'one Undo puts it back');
await redo();
await until(async () => (await vpRect(plan.id))?.[0] === 10);
check((await vpRect(plan.id))?.[0] === 10, 'Redo moves it again');

// ---- resize by a handle ---------------------------------------------------------------------------------------------------
await click([100, 85]);
await until(async () => (await p.locator('.paper-overlay .ed-handle').count()) === 8);
check((await p.locator('.paper-overlay .ed-handle').count()) === 8, 'the picked viewport has 8 handles');
const hb = await p.locator('.paper-overlay .ed-handle[data-handle="4"]').boundingBox();
await p.mouse.move(hb.x + hb.width / 2, hb.y + hb.height / 2);
await p.mouse.down();
await move([171.3, 141.2], 8);
await p.mouse.up();
await until(async () => (await vpRect(plan.id))?.[2] === 160);
r = await vpRect(plan.id);
check(JSON.stringify(r) === JSON.stringify([10, 20, 160, 120]), `the bottom right handle resizes it, snapped to the grid (${JSON.stringify(r)})`);
await undo();
await until(async () => (await vpRect(plan.id))?.[2] === 180);
check((await vpRect(plan.id))?.[2] === 180, 'one Undo for the resize');
await redo();
await until(async () => (await vpRect(plan.id))?.[2] === 160);

// ---- nudge by the keyboard; scale in the inspector --------------------------------------------------------------------------
await p.locator('.paper-view').focus();
await p.keyboard.press('ArrowRight');
await until(async () => (await vpRect(plan.id))?.[0] === 11);
check((await vpRect(plan.id))?.[0] === 11, 'ArrowRight nudges the picked viewport 1 mm');
await p.keyboard.press('Shift+ArrowDown');
await until(async () => (await vpRect(plan.id))?.[1] === 25);
check((await vpRect(plan.id))?.[1] === 25, 'Shift+ArrowDown nudges it a grid step');
await undo();
await until(async () => (await vpRect(plan.id))?.[1] === 20);
await undo();
await until(async () => (await vpRect(plan.id))?.[0] === 10);
check(JSON.stringify(await vpRect(plan.id)) === JSON.stringify([10, 20, 160, 120]), 'each nudge is its own undo step');
const scaleIn = p.locator('.layout-inspector').getByLabel('Scale');
await scaleIn.fill('1:100');
await scaleIn.press('Enter');
await until(async () => (await items()).find((i) => i.id === plan.id)?.scale === 100);
check((await items()).find((i) => i.id === plan.id)?.scale === 100, 'the inspector sets the scale');
await undo();
await until(async () => (await items()).find((i) => i.id === plan.id)?.scale === 50);
await p.locator('.paper-view').focus();
await p.keyboard.press('Escape');

// ---- a chain dimension of three model points ---------------------------------------------------------------------------
for (let i = 0; i < 60 && (await p.locator('.paper-status').count()); i++) await wait(200);
const vpts = (await pageExec(p, { cmd: 'viewport_points', viewport: plan.id, kinds: ['vertex', 'midpoint'] })).points;
const ptAt = (x, kind) => vpts.find((q) => q.kind === kind && near(q.world[0], x, 1e-6) && near(q.world[1], 0, 1e-6));
const A = ptAt(0, 'vertex');
const M = ptAt(3, 'midpoint');
const C = ptAt(6, 'vertex');
check(!!(A && M && C), 'the plan shows the south wall: two corners and its middle');
await tool('Dimension');
check((await p.locator('.paper-opts .dim-modes [role="radio"]').count()) === 4, 'the dimension tool shows how it measures (Auto, Horizontal, Vertical, Aligned)');
await click([60, 200]);
check(/point of the model/.test(await hint()), `a click off the model says what to click (${await hint()})`);
await move([A.mm[0] + 0.6, A.mm[1] - 0.4]);
await until(async () => (await p.locator('.paper-overlay .snap-mark').count()) > 0, 3000);
check((await p.locator('.paper-overlay .snap-mark').count()) > 0, 'over a model corner a snap marker shows');
await click([A.mm[0] + 0.6, A.mm[1] - 0.4]);
await click([M.mm[0] - 0.5, M.mm[1] + 0.5]);
await click([C.mm[0] + 0.4, C.mm[1]]);
check(/chain/.test(await hint()), `after two points the hint says more points chain (${await hint()})`);
await move([C.mm[0] - 20, C.mm[1] + 12]);
check((await p.locator('.paper-overlay .ed-dim path').count()) >= 2, 'the chain is previewed');
await p.keyboard.press('Enter');
await until(async () => (await ofKind('dimension')).length === 1);
const dim = (await ofKind('dimension'))[0];
const spans = dim?.measured?.spans?.map((s) => s.value_m) ?? [];
check(dim?.anchors?.length === 3 && dim?.measured?.attached === true, `a 3-point chain dimension, attached to the model (${dim?.anchors?.length} anchors)`);
check(spans.length === 2 && near(spans[0], 3, 1e-6) && near(spans[1], 3, 1e-6) && dim.mode === 'horizontal', `it measures 3 m + 3 m, horizontally (${spans}, ${dim?.mode})`);
check(near(dim.offset_mm, -12, 0.6), `its line is where the mouse was (offset ${dim?.offset_mm} mm)`);
await undo();
await until(async () => (await ofKind('dimension')).length === 0);
check((await ofKind('dimension')).length === 0, 'one Undo takes the whole chain away');
await redo();
await until(async () => (await ofKind('dimension')).length === 1);
check((await ofKind('dimension')).length === 1, 'Redo brings it back');
await p.screenshot({ path: path.join(shots, 'sheet-editor-dimension.png') });

// ---- text ----------------------------------------------------------------------------------------------------------------
await tool('Text');
await click([30.8, 190.6]);
await p.waitForSelector('.paper-textbox');
check(/Type/.test(await hint()), 'the text tool asks you to type');
await p.keyboard.type('GENERAL NOTES');
await p.keyboard.press('Enter');
await until(async () => (await ofKind('text')).length === 1);
let t = (await ofKind('text'))[0];
check(t?.paragraphs?.[0] === 'GENERAL NOTES' && JSON.stringify(t.at_mm) === JSON.stringify([30, 190]), `typed text goes on the page where clicked (${JSON.stringify(t?.paragraphs)} at ${JSON.stringify(t?.at_mm)})`);
await tool('Select');
const [tx, ty] = await scr([34, 189]);
await p.mouse.dblclick(tx, ty);
await p.waitForSelector('.paper-textbox');
await p.keyboard.press('ControlOrMeta+a');
await p.keyboard.type('NOTES');
await p.keyboard.press('Enter');
await until(async () => (await ofKind('text'))[0]?.paragraphs?.[0] === 'NOTES');
t = (await ofKind('text'))[0];
check(t?.paragraphs?.[0] === 'NOTES' && (await ofKind('text')).length === 1, 'double-clicking text changes it in place');
await undo();
await until(async () => (await ofKind('text'))[0]?.paragraphs?.[0] === 'GENERAL NOTES');
check((await ofKind('text'))[0]?.paragraphs?.[0] === 'GENERAL NOTES', 'one Undo for the change');
await redo();

// ---- lines, then trim and extend ------------------------------------------------------------------------------------------
await tool('Line');
await click([30, 240]);
check(/Click the end/.test(await hint()), 'the line tool asks for the end');
await p.keyboard.press('Escape');
check(/starts/.test(await hint()), 'Escape cancels the line before it is drawn');
const lineCount = async () => (await items()).filter((i) => i.kind === 'line').length;
check((await lineCount()) === 0, 'and nothing was drawn');
for (const [a, z] of [
  [[30, 240], [130, 240]],
  [[60, 210], [60, 250]],
  [[100, 210], [100, 250]],
  [[30, 220], [50, 220]],
]) {
  await click(a);
  await click(z);
  await p.keyboard.press('Enter');
}
await until(async () => (await lineCount()) === 4);
check((await lineCount()) === 4, `four lines drawn by clicking (${await lineCount()})`);
const lines = await ofKind('line');
const hLine = lines.find((l) => l.pts_mm[0][1] === 240);
const short = lines.find((l) => l.pts_mm[0][1] === 220);
check(!!hLine && JSON.stringify(hLine.pts_mm) === JSON.stringify([[30, 240], [130, 240]]), `the lines are where they were clicked (${JSON.stringify(hLine?.pts_mm)})`);
await undo();
await until(async () => (await lineCount()) === 3);
check((await lineCount()) === 3, 'one Undo takes the last line away');
await redo();
await until(async () => (await lineCount()) === 4);

await tool('Trim');
await move([80, 240.3]);
await until(async () => (await p.locator('.paper-overlay .ed-cut').count()) === 1, 5000);
check((await p.locator('.paper-overlay .ed-cut').count()) === 1, 'hovering a line with Trim shows (in red) the piece that would go');
await click([80, 240.3]);
await until(async () => (await lineCount()) === 5, 10000);
const trimmed = (await ofKind('line')).filter((l) => l.pts_mm[0][1] === 240 && l.pts_mm[1][1] === 240).map((l) => l.pts_mm.map((q) => q[0]));
check(JSON.stringify(trimmed.sort()) === JSON.stringify([[100, 130], [30, 60]].sort()), `the middle piece between the two lines is cut away (${JSON.stringify(trimmed)})`);
await undo();
await until(async () => (await lineCount()) === 4);
check((await lineCount()) === 4, 'one Undo puts the whole line back');
await redo();
await until(async () => (await lineCount()) === 5);

await tool('Extend');
await move([47, 220.2]);
await until(async () => (await p.locator('.paper-overlay .ed-ext').count()) === 1, 5000);
check((await p.locator('.paper-overlay .ed-ext').count()) === 1, 'hovering near the end with Extend shows where it would run');
await click([47, 220.2]);
await until(async () => (await items()).find((i) => i.id === short.id)?.pts_mm?.[1]?.[0] === 60, 10000);
check(JSON.stringify((await items()).find((i) => i.id === short.id)?.pts_mm) === JSON.stringify([[30, 220], [60, 220]]), 'Extend runs the short line on to the next line');
await undo();
await until(async () => (await items()).find((i) => i.id === short.id)?.pts_mm?.[1]?.[0] === 50);
check((await items()).find((i) => i.id === short.id)?.pts_mm?.[1]?.[0] === 50, 'one Undo for the extend');
await redo();
await until(async () => (await items()).find((i) => i.id === short.id)?.pts_mm?.[1]?.[0] === 60);

// ---- rectangle, polyline, arc, offset -------------------------------------------------------------------------------
const kindCount = async (k) => (await ofKind(k)).length;
await tool('Rectangle');
release = await dragMm([180.4, 180.3], [219.6, 199.8]);
await release();
await until(async () => (await kindCount('rect')) === 1);
const rect = (await ofKind('rect'))[0];
check(JSON.stringify(rect?.pts_mm) === JSON.stringify([[180, 180], [220, 200]]), `Rectangle: a drag draws one (${JSON.stringify(rect?.pts_mm)})`);
await undo();
await until(async () => (await kindCount('rect')) === 0);
check((await kindCount('rect')) === 0, 'one Undo takes it away');
await redo();
await until(async () => (await kindCount('rect')) === 1);
await tool('Polyline');
await click([230, 180]);
await click([260, 180]);
const [qx, qy] = await scr([260, 200]);
await p.mouse.dblclick(qx, qy);
await until(async () => (await kindCount('polyline')) === 1);
check(JSON.stringify((await ofKind('polyline'))[0]?.pts_mm) === JSON.stringify([[230, 180], [260, 180], [260, 200]]), 'Polyline: clicks, and a double-click finishes it');
await tool('Arc');
await click([180, 230]);
await click([220, 230]);
await move([200, 221], 4);
check((await p.locator('.paper-overlay .ed-draw').count()) === 1, 'Arc: the bend is previewed');
await click([200, 220]);
await until(async () => (await kindCount('arc')) === 1);
check(JSON.stringify((await ofKind('arc'))[0]?.pts_mm) === JSON.stringify([[180, 230], [200, 220], [220, 230]]), 'Arc: start, end, then the bend');
await tool('Offset');
await click([200, 180.2]);
check(/side the copy goes/.test(await hint()), `Offset: after picking a line it asks where the copy goes (${await hint()})`);
await move([200, 175], 4);
check(/5 mm/.test((await p.locator('.paper-overlay .ed-readout').textContent()) ?? ''), 'and shows the distance');
await click([200, 175]);
await until(async () => (await kindCount('rect')) === 2);
const grown = (await ofKind('rect')).find((x) => x.id !== rect.id);
check(JSON.stringify(grown?.pts_mm) === JSON.stringify([[175, 175], [225, 205]]), `the copy is 5 mm outside the rectangle (${JSON.stringify(grown?.pts_mm)})`);
await undo();
await until(async () => (await kindCount('rect')) === 1);
check((await kindCount('rect')) === 1, 'one Undo for the offset');
await redo();
await until(async () => (await kindCount('rect')) === 2);

// ---- the title block, typed on the page ---------------------------------------------------------------------------------
await tool('Select');
const [bx, by] = await scr([355, 262]);
await p.mouse.dblclick(bx, by);
await p.waitForSelector('.paper-field');
await p.locator('.paper-field').fill('Villa Rotonda');
await p.locator('.paper-field').press('Enter');
await until(async () => (await info())?.fields?.project === 'Villa Rotonda');
check((await info())?.fields?.project === 'Villa Rotonda', 'double-clicking the title block types its project on the page');

// ---- the Delete tool, keyboard access -----------------------------------------------------------------------------------
await tool('Delete');
await click([33, 188.5]);
await until(async () => (await ofKind('text')).length === 0);
check((await ofKind('text')).length === 0, 'the Delete tool deletes what is clicked');
await undo();
await until(async () => (await ofKind('text')).length === 1);
check((await ofKind('text')).length === 1, 'and Undo brings it back');
await tool('Select');
const focusables = await p.locator('.paper-overlay [data-focus]').count();
const all = (await items()).length;
check(focusables === all, `everything on the page can be reached with the keyboard (${focusables} of ${all})`);
const labels = await p.locator('.paper-overlay [data-focus]').evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
check(labels.every((l) => l && l.length > 6) && labels.some((l) => /Chain dimension of 2/.test(l)), 'each has a label for screen readers');
await p.locator('.paper-overlay [data-item]').first().focus();
const firstId = await p.evaluate(() => document.activeElement?.getAttribute('data-item'));
await p.keyboard.press('ArrowRight');
const nextId = await p.evaluate(() => document.activeElement?.getAttribute('data-item'));
check(firstId && nextId && firstId !== nextId, 'arrow keys go from item to item');
await p.keyboard.press('l');
await until(async () => (await p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label')) === 'Line', 2000);
check((await p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label')) === 'Line', 'tool keys work on the paper (L: Line)');
await p.keyboard.press('Escape');
check((await p.locator('.paper-tools .paper-tool.on').getAttribute('aria-label')) === 'Select', 'Escape with nothing in progress goes back to Select');
await p.mouse.move(2, 2);
await wait(400);
await p.screenshot({ path: path.join(shots, 'sheet-editor.png') });

// ---- saved with the model: save, open again, the same page -------------------------------------------------------------------
const before = await info();
const file = path.join(path.dirname(process.env.SKETCHCRAFT_DOC), 'sheet-editor.sketchcraft');
const sv = await pageExec(p, { cmd: 'save', path: file });
check(sv.ok, `saved (${sv.error ?? file})`);
await pageExec(p, { cmd: 'new' });
await until(async () => !(await info()));
const op = await pageExec(p, { cmd: 'open', path: file });
check(op.ok, `opened again (${op.error ?? ''})`);
const after = await info();
const strip = (s) => JSON.stringify({ ...s, items: s?.items?.map(({ measured, paper_mm, ...rest }) => rest) });
check(strip(after) === strip(before), 'the page came back exactly: viewports, dimension, text, lines, title block');
check(after?.items?.find((i) => i.kind === 'dimension')?.measured?.attached === true, 'and the dimension is still attached');
// a fresh window on the opened file: Layout draws it all again
await p.reload();
await p.waitForSelector('.viewport canvas');
await p.getByRole('tab', { name: 'Layout', exact: true }).click();
await until(async () => (await p.locator('.paper-overlay [data-focus]').count()) === all, 15000);
check((await p.locator('.paper-overlay [data-focus]').count()) === all, `the reopened file shows it all on the paper (${await p.locator('.paper-overlay [data-focus]').count()} of ${all})`);
for (let i = 0; i < 80 && (await p.locator('.paper-status').count()); i++) await wait(150);
const runs = await pageExec(p, { cmd: 'sheet_ir', sheet: after.id });
check(JSON.stringify(runs).includes('"NOTES"') && JSON.stringify(runs).includes('Villa Rotonda'), 'and its drawing has the text and the title block project');

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
