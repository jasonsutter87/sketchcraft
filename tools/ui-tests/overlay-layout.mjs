#!/usr/bin/env node
// The overlays on the model view stay clear of each other in every layout (#55): the axis gizmo (top right)
// is fully inside the view, not under the view bar, and clickable, in Classic, Floating and Vertical tools,
// at the smallest window, a usual one and a big one; in the Vertical layout the tool hint card, the Value
// box and the SubD bar stay clear of the tool bar, the view bar and the gizmo too, and every tool button sits inside
// the tool bar's background when it goes to two columns on a short window. It compares element boxes
// (getBoundingClientRect), not screenshots; SHOT_DIR keeps a picture of each for looking at.
// In a real browser against the dev page and bridge (docs/dev/building.md), on a scratch model:
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/overlay-layout.mjs
// A model built with pageExec doesn't show on a page that is already open (the bridge's /sync only sees another
// program's save, not the page's own commands), so the page is reloaded to show it.
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

const LAYOUTS = { classic: 'Classic', floating: 'Floating', side: 'Vertical' };
const SIZES = [[960, 600], [1280, 800], [1920, 1080]];
const OVERLAYS = { gizmo: '.axis-gizmo', views: '.views', toolbar: '.toolbar', hint: '.coach', value: '.measure', subd: '.subd-bar' };
const CAGE = {
  cmd: 'subd', name: 'Pebble', levels: 2,
  points: [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0], [-1, -1, 2], [1, -1, 2], [1, 1, 2], [-1, 1, 2]],
  faces: [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]],
};
const cross = (a, c) => a.x < c.x + c.width && c.x < a.x + a.width && a.y < c.y + c.height && c.y < a.y + a.height;
const inside = (a, o) => a.x >= o.x - 0.5 && a.y >= o.y - 0.5 && a.x + a.width <= o.x + o.width + 0.5 && a.y + a.height <= o.y + o.height + 0.5;

const b = await launch();
const problems = [];

for (const [layout, name] of Object.entries(LAYOUTS)) {
  for (const [w, h] of SIZES) {
    for (const subd of [false, true]) {
      const tag = `${name} ${w}x${h}${subd ? ' in SubD mode' : ''}`;
      const ctx = await b.newContext({ viewport: { width: w, height: h } });
      await ctx.addInitScript((l) => {
        try {
          localStorage.setItem('sketchcraft.layout', l);
        } catch {}
      }, layout);
      const p = await ctx.newPage();
      p.on('pageerror', (e) => problems.push(String(e)));
      await p.goto(PAGE);
      await p.waitForSelector('.viewport canvas');
      await wait(400);
      await pageExec(p, { cmd: 'new' });
      if (subd) {
        await pageExec(p, CAGE);
        await p.reload();
        await p.waitForSelector('.viewport canvas');
        await p.locator('.orow', { hasText: 'Pebble' }).first().click();
        await p.keyboard.press('Tab');
        await p.waitForSelector('.subd-bar');
      } else {
        await p.reload();
        await p.waitForSelector('.viewport canvas');
      }
      await wait(600);
      const box = async (sel) => (await p.locator(sel).count() ? p.locator(sel).first().boundingBox() : null);
      const view = await box('.viewport');
      const boxes = {};
      for (const [k, s] of Object.entries(OVERLAYS)) boxes[k] = await box(s);

      const g = boxes.gizmo;
      check(!!g && inside(g, view), `${tag}: the gizmo is fully inside the model view`);
      if (!g) continue;
      check(!cross(g, boxes.views), `${tag}: the gizmo is clear of the view bar (gizmo y ${Math.round(g.y)}, view bar bottom ${Math.round(boxes.views.y + boxes.views.height)})`);
      check(!cross(g, boxes.toolbar), `${tag}: and of the tool bar`);
      if (boxes.hint) check(!cross(g, boxes.hint), `${tag}: and of the tool hint card`);
      if (boxes.subd) check(!cross(g, boxes.subd), `${tag}: and of the SubD bar`);
      check(!cross(g, boxes.value), `${tag}: and of the Value box`);

      // clickable: what is under the gizmo's corners and centre is the gizmo, and clicking an axis turns the view
      const hit = await p.evaluate(({ x, y, width, height }) => [[3, 3], [width - 3, 3], [3, height - 3], [width - 3, height - 3], [width / 2, height / 2]].map(([dx, dy]) => !!document.elementFromPoint(x + dx, y + dy)?.closest('.axis-gizmo')), g);
      // (the corners of the square are outside the round gizmo's own shapes but still its element)
      check(hit.every(Boolean), `${tag}: nothing covers the gizmo (${hit.map((x) => (x ? 'gizmo' : 'COVERED')).join(', ')})`);
      const eye = () => p.evaluate(() => window.__sketchcraftView.current.camera.position.toArray().map((x) => Math.round(x * 100) / 100).join(','));
      const before = await eye();
      const end = await p.locator('.axis-gizmo [data-axis="+z"]').first().boundingBox();
      await p.mouse.click(end.x + end.width / 2, end.y + end.height / 2);
      await wait(900);
      check((await eye()) !== before, `${tag}: clicking the Z end of the gizmo turns the view (looks down from the top)`);

      // the other overlays, in the Vertical layout: clear of each other and of the tool bar
      if (layout === 'side') {
        const pairs = [['views', 'toolbar'], ['hint', 'toolbar'], ['hint', 'views'], ['hint', 'value'], ['value', 'toolbar'], ['subd', 'toolbar'], ['subd', 'value'], ['subd', 'gizmo'], ['subd', 'hint'], ['subd', 'views']];
        for (const [a, c] of pairs) if (boxes[a] && boxes[c]) check(!cross(boxes[a], boxes[c]), `${tag}: ${a} and ${c} don't overlap`);
        for (const k of ['views', 'toolbar', 'hint', 'value', 'subd']) if (boxes[k]) check(inside(boxes[k], view), `${tag}: ${k} is inside the model view`);
        // every tool button sits on the tool bar's background, whether it's one column or two (Photoshop-style)
        const tb = await p.evaluate(() => {
          const bar = document.querySelector('.toolbar');
          const r = bar.getBoundingClientRect();
          const out = [...bar.querySelectorAll('.tool, .chip')].filter((e) => { const b = e.getBoundingClientRect(); return b.left < r.left - 0.5 || b.right > r.right + 0.5 || b.top < r.top - 0.5 || b.bottom > r.bottom + 0.5; });
          return { cols: Number(bar.dataset.cols), out: out.map((e) => e.getAttribute('aria-label') ?? e.className) };
        });
        check(tb.out.length === 0, `${tag}: every tool is inside the tool bar (${tb.cols} column${tb.cols > 1 ? 's' : ''}${tb.out.length ? `; outside: ${tb.out.join(', ')}` : ''})`);
      }
      await p.screenshot({ path: path.join(shots, `${layout}-${w}x${h}${subd ? '-subd' : ''}.png`) });
      await ctx.close();
    }
  }
}

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
