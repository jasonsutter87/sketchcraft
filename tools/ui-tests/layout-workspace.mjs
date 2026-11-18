#!/usr/bin/env node
// The Layout workspace (#63, design step 11), in a real browser against the dev page and bridge: switch to Layout
// (the 3D view stays loaded underneath: same canvas, same camera, same selection, same model), the empty state's
// "Add page", add three pages, reorder by dragging and by Alt+Arrow (each one undo step: Ctrl+Z / Ctrl+Shift+Z),
// rename, put a view on a page, pick it on the paper, and the opacity slider changes the saved viewport (#6) and
// what the page draws. Then back to Model: nothing there has changed. The File menu's "Open in Layout" goes there too.
//   SHOT_DIR=/some/folder node tools/ui-tests/layout-workspace.mjs
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

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
/** wait until `f()` is truthy (or the time is up); returns its last value */
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
  } catch {}
});
const p = await ctx.newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(400);

// a box and a loose rectangle
await pageExec(p, { cmd: 'new' });
await pageExec(p, { cmd: 'units', units: 'm' });
const f = (await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] })).face;
await pageExec(p, { cmd: 'pushpull', face: f, distance: 3 });
await pageExec(p, { cmd: 'rect', from: [9, 0, 0], to: [12, 3, 0] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1200);
await p.evaluate(() => {
  const T = window.__sketchcraftView.current;
  T.controls.target.set(3, 2, 0);
  T.persp.position.set(3, 1.99, 20);
  T.persp.up.set(0, 0, 1);
  T.persp.lookAt(3, 2, 0);
  T.controls.update();
  // mark the 3D view's canvas: if Layout remounted the view, this mark would be gone
  document.querySelector('.viewport canvas').dataset.layoutMark = 'kept';
});
await wait(500);

// select the box's top face with the mouse (Select tool)
await p.locator('nav.toolbar button[aria-label="Select"]').first().click();
const vb = await p.locator('.viewport canvas').boundingBox();
const center = await p.evaluate(() => {
  const T = window.__sketchcraftView.current;
  const v = T.camera.position.clone().set(3, 2, 3).project(T.camera);
  const r = document.querySelector('.viewport canvas').getBoundingClientRect();
  return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
});
await p.mouse.click(center[0], center[1]);
await wait(600);
const selTitle = async () => (await p.locator('.panel.right .section-title').first().textContent().catch(() => '')) ?? '';
const selBefore = await selTitle();
check(vb && /face|Face/i.test(selBefore), `a face is selected in the model (Entity Info: "${selBefore}")`);

const camera = () =>
  p.evaluate(() => {
    const T = window.__sketchcraftView.current;
    return [...T.camera.position.toArray(), ...T.controls.target.toArray()].map((x) => Math.round(x * 1e6) / 1e6);
  });
const counts = async () => {
  const s = await pageScene(p);
  return `${s.faces.length} faces, ${s.edgeList.length} edges, ${s.groups.length} groups, ${s.instances.length} components`;
};
const cam0 = await camera();
const count0 = await counts();

// ---- switch to Layout -------------------------------------------------------------------------------------------
const tab = (name) => p.getByRole('tab', { name, exact: true });
let t0 = Date.now();
await tab('Layout').click();
await p.waitForSelector('.layout-stage');
const switchMs = Date.now() - t0;
check(switchMs < 1500, `switching to Layout is quick (${switchMs} ms, with the click)`);
check((await tab('Layout').getAttribute('aria-selected')) === 'true', 'the Layout tab is selected');
check((await p.locator('nav.toolbar').count()) === 0, "the model's tool bar is put away");
check((await p.locator('.viewport canvas[data-layout-mark="kept"]').count()) === 1, 'the 3D view is still there underneath (same canvas)');
const empty = p.locator('.layout-empty');
await until(() => empty.isVisible());
check(await empty.isVisible(), 'with no pages, the paper says what Layout is for');
check((await p.locator('.tabs button.on').textContent()) === 'Pages', 'the left panel opens on Pages');
await p.screenshot({ path: path.join(shots, 'layout-empty.png') });

// ---- add three pages ---------------------------------------------------------------------------------------------
const sheets = async () => (await pageExec(p, { cmd: 'sheets_info' })).sheets ?? [];
const names = async () => (await sheets()).map((s) => s.name);
const rows = () => p.locator('.pages-list .page-row .page-name').allTextContents();
await empty.getByRole('button', { name: /Add page/ }).click();
await until(async () => (await sheets()).length === 1);
await p.locator('.pages-head').getByRole('button', { name: /Add page/ }).click();
await until(async () => (await sheets()).length === 2);
await p.locator('.pages-head').getByRole('button', { name: /Add page/ }).click();
await until(async () => (await sheets()).length === 3);
check(JSON.stringify(await names()) === JSON.stringify(['Page 1', 'Page 2', 'Page 3']), `three pages (${await names()})`);
await until(async () => (await rows()).length === 3);
check(JSON.stringify(await rows()) === JSON.stringify(['Page 1', 'Page 2', 'Page 3']), `the Pages list shows them in order (${await rows()})`);
check((await p.locator('.page-row.on .page-name').textContent()) === 'Page 3', 'the page just added is the one shown');
// the paper is drawn from the engine's display list
await until(async () => (await p.locator('.paper-status').count()) === 0, 10000);
const paperPx = async (fx, fy) =>
  p.evaluate(
    ([fx, fy]) => {
      const c = document.querySelector('.paper-canvas');
      const g = c.getContext('2d');
      const d = g.getImageData(Math.round(c.width * fx), Math.round(c.height * fy), 1, 1).data;
      return [d[0], d[1], d[2]];
    },
    [fx, fy],
  );
const mid = await paperPx(0.5, 0.5);
check(mid.every((v) => v > 245), `the middle of the page is white paper (${mid})`);
const desk = await paperPx(0.005, 0.5);
check(!desk.every((v) => v > 245), `around it is the desk, not paper (${desk})`);

// ---- reorder: drag, one undo step ----------------------------------------------------------------------------------
const row = (name) => p.locator('.page-row', { has: p.locator('.page-name', { hasText: new RegExp(`^${name}$`) }) });
await row('Page 3').dragTo(row('Page 1'));
await until(async () => (await names())[0] === 'Page 3');
check(JSON.stringify(await names()) === JSON.stringify(['Page 3', 'Page 1', 'Page 2']), `dragging Page 3 onto Page 1 puts it first (${await names()})`);
await until(async () => (await rows())[0] === 'Page 3');
check(JSON.stringify(await rows()) === JSON.stringify(['Page 3', 'Page 1', 'Page 2']), `and the list follows (${await rows()})`);
await p.locator('.paper-view').focus();
await p.keyboard.press('ControlOrMeta+z');
await until(async () => (await names())[0] === 'Page 1');
check(JSON.stringify(await names()) === JSON.stringify(['Page 1', 'Page 2', 'Page 3']), `one Undo takes the whole reorder back (${await names()})`);
await until(async () => (await rows())[0] === 'Page 1');
check(JSON.stringify(await rows()) === JSON.stringify(['Page 1', 'Page 2', 'Page 3']), 'and the list follows the undo');
await p.keyboard.press('ControlOrMeta+Shift+z');
await until(async () => (await names())[0] === 'Page 3');
check(JSON.stringify(await names()) === JSON.stringify(['Page 3', 'Page 1', 'Page 2']), `Redo puts it back (${await names()})`);

// ---- reorder by keyboard ------------------------------------------------------------------------------------------
await until(async () => (await rows())[0] === 'Page 3');
await p.getByRole('button', { name: /^Page 1 of 3: Page 3\./ }).focus();
await p.keyboard.press('Alt+ArrowDown');
await until(async () => (await names())[1] === 'Page 3');
check(JSON.stringify(await names()) === JSON.stringify(['Page 1', 'Page 3', 'Page 2']), `Alt+Down moves the focused page down (${await names()})`);
check(await p.evaluate(() => document.activeElement?.closest('.page-row')?.querySelector('.page-name')?.textContent) === 'Page 3', 'the keyboard stays on the page that moved');

// ---- rename -------------------------------------------------------------------------------------------------------
await row('Page 1').locator('.page-main').dblclick({ position: { x: 24, y: 12 } });
const ed = p.locator('.page-name-edit');
await ed.fill('Ground floor');
await ed.press('Enter');
await until(async () => (await names())[0] === 'Ground floor');
check((await names())[0] === 'Ground floor', `double-click renames a page (${await names()})`);

// ---- a view on the page, picked on the paper, and its opacity ------------------------------------------------------
await row('Ground floor').locator('.page-main').click({ position: { x: 24, y: 12 } });
await until(async () => (await p.locator('.layout-inspector [aria-label="View to add"]').count()) === 1);
await p.locator('.layout-inspector [aria-label="View to add"]').selectOption('front');
await p.locator('.layout-inspector').getByRole('button', { name: 'Add view' }).click();
const page1 = async () => (await sheets()).find((s) => s.name === 'Ground floor');
const vpOf = async () => (await page1())?.items.find((i) => i.kind === 'viewport');
await until(vpOf);
const v = await vpOf();
check(v?.view?.type === 'elevation', `"Add view" puts a front elevation on the page (${v?.view?.type}, 1:${v?.scale})`);
await until(async () => (await p.locator(`.vp[data-viewport="${v.id}"]`).count()) === 1);
await until(async () => (await p.locator('.paper-status').count()) === 0, 15000);
await p.screenshot({ path: path.join(shots, 'layout-view.png') });
const frame = await p.locator(`.vp[data-viewport="${v.id}"] .vp-frame`).boundingBox();
await p.mouse.move(frame.x + frame.width / 2, frame.y + frame.height / 2);
await wait(200);
check((await p.locator('.paper-view').evaluate((e) => e.style.cursor)) === 'pointer', 'over a viewport the cursor says a click picks it');
check(/select this viewport/i.test(await p.locator('.paper-hint').textContent()), 'and the hint says so');
await p.mouse.click(frame.x + frame.width / 2, frame.y + frame.height / 2);
await until(async () => (await p.locator('.layout-inspector[aria-label="Viewport"]').count()) === 1);
check((await p.locator(`.vp.on[data-viewport="${v.id}"]`).count()) === 1, 'clicking it picks it: frame and handles');
check((await p.locator('.vp.on .vp-handle').count()) === 8, 'eight handles');
const slider = p.getByLabel('Viewport opacity');
check((await slider.inputValue()) === '100', 'the opacity slider starts at 100%');
await slider.fill('40');
await slider.press('Tab');
await until(async () => (await vpOf())?.opacity === 0.4);
check((await vpOf())?.opacity === 0.4, `the slider sets the saved viewport's opacity (${(await vpOf())?.opacity})`);
const ir = await pageExec(p, { cmd: 'sheet_ir', sheet: (await page1()).id });
const groups = [];
const walk = (items) => items.forEach((i) => (groups.push(i), i.items && walk(i.items)));
walk(ir.items);
check(groups.some((g) => g.k === 'g' && g.o === 0.4), 'and the page draws that viewport at 40%');
await p.locator('.paper-view').focus();
await p.keyboard.press('ControlOrMeta+z');
await until(async () => (await vpOf())?.opacity === undefined);
check((await vpOf())?.opacity === undefined, 'one Undo puts it back to full strength');
await until(async () => (await slider.inputValue()) === '100');
check((await slider.inputValue()) === '100', 'and the slider follows');
// Escape on the paper goes back to the page
await p.locator('.paper-view').focus();
await p.keyboard.press('Escape');
await until(async () => (await p.locator('.layout-inspector[aria-label="Page"]').count()) === 1);
check((await p.locator('.layout-inspector[aria-label="Page"]').count()) === 1, 'Escape on the paper goes back to the page inspector');
// zoom and fit
const zoom0 = await p.locator('.paper-zoom').textContent();
await p.keyboard.press('+');
await wait(100);
const zoom1 = await p.locator('.paper-zoom').textContent();
await p.keyboard.press('0');
await wait(100);
check(parseInt(zoom1) > parseInt(zoom0) && (await p.locator('.paper-zoom').textContent()) === zoom0, `+ zooms in (${zoom0} -> ${zoom1}), 0 fits again`);

// ---- back to Model: nothing has changed --------------------------------------------------------------------------------
t0 = Date.now();
await tab('Model').click();
await p.waitForSelector('nav.toolbar');
check(Date.now() - t0 < 1500, `switching back is quick (${Date.now() - t0} ms)`);
check((await p.locator('.layout-stage').count()) === 0, 'the paper is gone');
check((await p.locator('.viewport canvas[data-layout-mark="kept"]').count()) === 1, 'the same 3D view (never reloaded)');
const cam1 = await camera();
check(JSON.stringify(cam1) === JSON.stringify(cam0), `the camera is where it was (${cam1})`);
const count1 = await counts();
check(count1 === count0, `the model is all there (${count1})`);
check((await selTitle()) === selBefore, `the selection is kept (${await selTitle()})`);
check((await p.locator('nav.toolbar button.tool.on[aria-label="Select"]').count()) === 1, 'the tool is the one you had');

// ---- the File menu goes to Layout ------------------------------------------------------------------------------------------
await p.locator('.file-btn').click();
await p.getByRole('button', { name: 'Open in Layout' }).click();
await p.waitForSelector('.layout-stage');
check((await tab('Layout').getAttribute('aria-selected')) === 'true', 'File > Open in Layout switches to Layout');
await until(async () => (await rows()).length === 3);
check((await rows())[0] === 'Ground floor', 'and the pages are as you left them');
await tab('Model').click();

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
