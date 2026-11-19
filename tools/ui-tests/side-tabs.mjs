#!/usr/bin/env node
// The Vertical layout's sidebar: Outliner | Library | Views | Properties, one panel, every tab the full height
// (no split with a fixed lower panel). Properties holds Document, Style, Paint and the selection's Entity info.
// The tabs are a real tablist (arrow keys), the last tab is remembered, a selection doesn't change the tab but marks
// Properties, and Classic keeps its two panels and three tabs. Compares element boxes, not screenshots.
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/side-tabs.mjs
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
const open = async (layout, extra = {}) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
  await ctx.addInitScript(([l, x]) => {
    try {
      localStorage.setItem('sketchcraft.layout', l);
      localStorage.setItem('sketchcraft.help', 'off');
      for (const [k, v] of Object.entries(x)) localStorage.setItem(k, v);
    } catch {}
  }, [layout, extra]);
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas');
  await wait(400);
  return [ctx, p];
};
const CAGE = {
  cmd: 'subd', name: 'Pebble', levels: 2,
  points: [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0], [-1, -1, 2], [1, -1, 2], [1, 1, 2], [-1, 1, 2]],
  faces: [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]],
};
const box = (p, sel) => p.locator(sel).first().boundingBox();
const tab = (p, name) => p.locator('.panel.left [role="tab"]', { hasText: name }).first();

// ---- Vertical
{
  const [ctx, p] = await open('side');
  await pageExec(p, { cmd: 'new' });
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(600);

  const names = await p.locator('.panel.left [role="tab"]').allInnerTexts();
  check(names.join('|') === 'Outliner|Library|Views|Properties', `Vertical: the tabs are Outliner, Library, Views, Properties (${names.join('|')})`);
  check((await p.locator('aside.panel.right').count()) === 0, 'Vertical: there is no separate lower panel');
  check((await p.locator('.panel.left [role="tablist"]').count()) === 1 && (await p.locator('.panel.left [role="tabpanel"]').count()) === 1, 'Vertical: a tablist and one tabpanel');

  const view = await box(p, '.viewport');
  const top = (await box(p, '.topbar')).y + (await box(p, '.topbar')).height;
  const full = async (what) => {
    const pn = await box(p, '.panel.left');
    const ok = Math.abs(pn.y - top) < 2 && Math.abs(pn.y + pn.height - 800) < 2;
    check(ok, `Vertical: ${what} has the full sidebar height (panel ${Math.round(pn.y)}..${Math.round(pn.y + pn.height)} of 800)`);
    check(pn.x > view.x + view.width - 2, `Vertical: the sidebar sits right of the model view`);
  };
  await full('Outliner');
  const out = await box(p, '.panel.left [role="tabpanel"]');
  const pn = await box(p, '.panel.left');
  check(out.y + out.height >= pn.y + pn.height - 2, 'Vertical: the Outliner panel fills down to the bottom');

  await tab(p, 'Properties').click();
  await wait(200);
  check((await tab(p, 'Properties').getAttribute('aria-selected')) === 'true', 'Properties: selected');
  const doc = p.locator('.panel.left .section-title', { hasText: 'Document' }).first();
  check(await doc.isVisible(), 'Properties: the Document section is visible');
  check(await p.locator('.panel.left .section-title', { hasText: 'Style' }).first().isVisible(), 'Properties: the Style section is visible');
  await full('Properties');
  await p.screenshot({ path: path.join(shots, 'side-properties.png') });

  // arrow keys move between tabs and select them
  await tab(p, 'Properties').focus();
  await p.keyboard.press('ArrowRight');
  check((await tab(p, 'Outliner').getAttribute('aria-selected')) === 'true', 'ArrowRight from the last tab wraps to Outliner');
  await p.keyboard.press('ArrowLeft');
  check((await tab(p, 'Properties').getAttribute('aria-selected')) === 'true', 'ArrowLeft goes back to Properties');
  await p.keyboard.press('Home');
  check((await tab(p, 'Outliner').getAttribute('aria-selected')) === 'true', 'Home goes to the first tab');
  await p.keyboard.press('End');
  check((await tab(p, 'Properties').getAttribute('aria-selected')) === 'true', 'End goes to the last tab');
  check((await p.evaluate(() => document.activeElement?.getAttribute('role'))) === 'tab', 'focus stays on a tab');

  // remembered
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(600);
  check((await tab(p, 'Properties').getAttribute('aria-selected')) === 'true', 'the last chosen tab is remembered after a reload');

  // a selection doesn't move you off the tab; Properties is marked when you're elsewhere
  await pageExec(p, CAGE);
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(800);
  await tab(p, 'Outliner').click();
  await p.locator('.orow', { hasText: 'Pebble' }).first().click();
  await wait(400);
  check((await tab(p, 'Outliner').getAttribute('aria-selected')) === 'true', 'selecting something stays on the Outliner tab');
  check((await p.locator('.panel.left .tab-dot').count()) === 1, 'Properties is marked while something is selected');
  await tab(p, 'Properties').click();
  await wait(200);
  check((await p.locator('.panel.left .tab-dot').count()) === 0, 'the mark goes away on Properties itself');
  check((await p.locator('.panel.left .section-title', { hasText: 'Document' }).count()) === 0, 'with a selection the Document section gives way to the selection info');

  await tab(p, 'Library').click();
  await wait(200);
  await full('Library');
  await tab(p, 'Views').click();
  await wait(200);
  await full('Views');
  await ctx.close();
}

// ---- Classic stays as it was
{
  const [ctx, p] = await open('classic', { 'sketchcraft.sideTab': 'properties' });
  const names = await p.locator('.panel.left [role="tab"]').allInnerTexts();
  check(names.join('|') === 'Outliner|Library|Views', `Classic: still three tabs (${names.join('|')})`);
  check((await p.locator('aside.panel.right').count()) === 1, 'Classic: the right panel is still there');
  check((await tab(p, 'Outliner').getAttribute('aria-selected')) === 'true', 'Classic: starts on the Outliner (a remembered Vertical tab is ignored)');
  const l = await box(p, '.panel.left');
  const r = await box(p, '.panel.right');
  check(r.x > l.x + l.width - 2, 'Classic: the left and right panels are on either side of the view');
  await ctx.close();
}

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
