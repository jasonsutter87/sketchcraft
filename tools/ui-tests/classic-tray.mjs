#!/usr/bin/env node
// The Classic look (#228): the Classic Color icons on the tool bar, and the Classic tray (a right-hand column of
// collapsible panels: Entity Info, Materials, Components, Styles, Environments, Tags, Scenes, Outliner).
// Off by default (Classic stays as it was), on only with the Classic layout, the open and closed panels are
// remembered, and the panels in it are SketchCraft's own. Compares element boxes and text, not screenshots.
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/classic-tray.mjs
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

const b = await launch();
const problems = [];
const open = async (settings) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
  await ctx.addInitScript((x) => {
    try {
      localStorage.setItem('sketchcraft.help', 'off');
      for (const [k, v] of Object.entries(x)) localStorage.setItem(k, v);
    } catch {}
  }, settings);
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas');
  await wait(500);
  return [ctx, p];
};
const heads = (p) => p.locator('.tray .tray-head').allInnerTexts();
const head = (p, name) => p.locator('.tray .tray-head', { hasText: name }).first();

// ---- Classic as it was: no tray, the default icons
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'classic', 'sketchcraft.skin': 'classic' });
  check((await p.locator('.tray').count()) === 0, 'Classic: no tray until you turn it on');
  check((await p.locator('.panel.left').isVisible()) && (await p.locator('.panel.right').isVisible()), 'Classic: the left and right panels are as they were');
  check((await p.locator('.toolbar img.skin-icon').count()) === 0, 'Classic: the tool bar keeps its own icons');
  await ctx.close();
}

// ---- Classic Color: colour icons and the tray
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'classic', 'sketchcraft.skin': 'classic-color', 'sketchcraft.tray': '1' });
  const tools = await p.locator('.toolbar .tool:not(.more)').count();
  const icons = await p.locator('.toolbar .tool img.skin-icon').count();
  check(tools > 20 && icons === tools, `Classic Color: every tool on the bar wears a colour icon (${icons} of ${tools})`);
  check((await p.locator('.toolbar .tool img.skin-icon').first().evaluate((i) => i.complete && i.naturalWidth > 0)) === true, 'the icon images load');

  check((await p.locator('.tray').count()) === 1, 'the tray is there');
  check(!(await p.locator('.panel.left').isVisible()), 'the left panel gives way to the tray');
  const names = await heads(p);
  check(names.join('|') === 'Entity Info|Materials|Components|Styles|Environments|Tags|Scenes|Outliner', `panels in order (${names.join('|')})`);
  const r = await p.locator('aside.panel.right').boundingBox();
  const v = await p.locator('.viewport').boundingBox();
  check(r.x >= v.x + v.width - 2 && r.width > 200 && r.width < 340, `the tray is a column on the right (x=${r.x}, w=${r.width})`);

  // first look: Entity Info and Materials open
  const expanded = async (name) => (await head(p, name).getAttribute('aria-expanded')) === 'true';
  check((await expanded('Entity Info')) && (await expanded('Materials')) && !(await expanded('Styles')), 'Entity Info and Materials start open, the rest closed');
  check((await p.locator('.tray-panel[data-panel="materials"] .swatch').count()) > 5, 'Materials holds the paint swatches');

  // open Styles and Tags, close Entity Info; reuse of the real panels
  await head(p, 'Styles').click();
  await head(p, 'Tags').click();
  await head(p, 'Entity Info').click();
  await wait(200);
  check((await p.locator('.tray-panel[data-panel="styles"] select').count()) >= 3, 'Styles holds the style controls');
  check((await p.locator('.tray-panel[data-panel="tags"] .tags').count()) === 1, 'Tags holds the tag list');
  check(!(await expanded('Entity Info')) && (await p.locator('.tray-panel[data-panel="entity"] .tray-body').count()) === 0, 'a closed panel hides its body');
  await head(p, 'Scenes').click();
  await head(p, 'Environments').click();
  await head(p, 'Components').click();
  await wait(200);
  check((await p.locator('.tray-panel[data-panel="scenes"] input[placeholder="Name this view"]').count()) === 1, 'Scenes holds the views panel');
  check((await p.locator('.tray-panel[data-panel="environments"] button').count()) > 0, 'Environments holds the environments');
  check((await p.locator('.tray-panel[data-panel="components"] .tray-body').innerText()).length > 0, 'Components holds the library');
  await p.screenshot({ path: path.join(shots, 'tray-open.png') });

  // remembered after a reload
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(700);
  check((await expanded('Styles')) && (await expanded('Tags')) && !(await expanded('Entity Info')) && (await expanded('Materials')), 'open and closed panels are remembered after a reload');
  check(JSON.parse(await p.evaluate(() => localStorage.getItem('sketchcraft.trayOpen'))).styles === true, 'saved in settings');

  // Entity Info shows the selection
  await head(p, 'Entity Info').click();
  check((await p.locator('.tray-panel[data-panel="entity"] .tray-empty').count()) === 1, 'Entity Info asks for a selection when there is none');
  await ctx.close();
}

// ---- a model: Entity Info with a selection
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'classic', 'sketchcraft.skin': 'classic-color', 'sketchcraft.tray': '1' });
  await pageExec(p, { cmd: 'new' });
  const face = (await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] })).face;
  await pageExec(p, { cmd: 'pushpull', face, distance: 2 });
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(800);
  await head(p, 'Outliner').click();
  await wait(300);
  const row = p.locator('.tray-panel[data-panel="outliner"] .orow').first();
  if (await row.count()) {
    await row.click();
    await wait(400);
    check((await p.locator('.tray-panel[data-panel="entity"] .tray-body').innerText()).trim().length > 0 && (await p.locator('.tray-empty').count()) === 0, 'Entity Info fills in once something is selected');
  } else check(true, '(no outliner row to select: skipped)');
  await ctx.close();
}

// ---- only with the Classic layout, and only when on
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'floating', 'sketchcraft.skin': 'classic-color', 'sketchcraft.tray': '1' });
  check((await p.locator('.tray').count()) === 0, 'Floating layout: no tray even when the setting is on');
  await ctx.close();
}
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'classic', 'sketchcraft.skin': 'classic-color' });
  check((await p.locator('.tray').count()) === 0, 'Classic Color with the tray setting off: no tray');
  await ctx.close();
}

// ---- turned on from Themes
{
  const [ctx, p] = await open({ 'sketchcraft.layout': 'classic', 'sketchcraft.skin': 'classic' });
  await p.locator('.skin-btn').click();
  const box = p.getByLabel(/Tray: collapsible panels/);
  await box.waitFor();
  await box.check();
  await wait(300);
  check((await p.locator('.tray').count()) === 1, 'the Themes window turns the tray on');
  await ctx.close();
}

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
