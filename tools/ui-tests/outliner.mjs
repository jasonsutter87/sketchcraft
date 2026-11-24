#!/usr/bin/env node
// The Outliner with deeply nested groups, in a real browser against the dev page and bridge (docs/dev/building.md), on a
// scratch model: SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/outliner.mjs
// The engine keeps groups to 256 levels (a file nested deeper is brought up to it when it opens, and says so). The window
// walks the tree without recursion, so a model at the limit lists every level, a very deep file opens and draws, the
// search finds a group deep in the tree, and reloading the page (what the autosaved model does at the next start) is fine.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

const doc = process.env.SKETCHCRAFT_DOC;
if (!doc) {
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

const LIMIT = 256;
const b = await launch();
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
await ctx.addInitScript(() => {
  try {
    localStorage.setItem('sketchcraft.help', 'off');
  } catch {}
});
const p = await ctx.newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
const X = (c) => pageExec(p, c);
const rows = () => p.locator('.otree[role="tree"] [role="treeitem"]').count();
const drawn = async () => (await p.locator('text=SketchCraft hit a problem drawing the window').count()) === 0;
const reload = async () => {
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
};

// ---- a model nested all the way to the limit
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
const face = (await pageScene(p)).faces[0].id;
let top = (await X({ cmd: 'group', ids: [face], name: 'Level 256' })).group;
for (let k = LIMIT - 1; k >= 1; k--) top = (await X({ cmd: 'group', ids: [top], name: `Level ${k}` })).group;
const tooMany = await X({ cmd: 'group', ids: [top], name: 'Level 0' });
check(tooMany.ok === false && /levels deep/.test(tooMany.error ?? ''), `a ${LIMIT + 1}th level is refused ("${tooMany.error ?? ''}")`);
await reload();
check(await drawn(), 'the window draws a model nested to the limit');
check((await rows()) === LIMIT, `every one of the ${LIMIT} levels is a row (${await rows()})`);
const depths = await p.locator('.otree[role="tree"] [role="treeitem"]').evaluateAll((els) => els.map((e) => Number(e.getAttribute('aria-level'))));
check(depths[0] === 1 && depths[depths.length - 1] === LIMIT, `rows carry their level (first ${depths[0]}, last ${depths[depths.length - 1]})`);

// the search finds a group at the bottom of the tree, with the groups it sits in
const search = p.locator('input.otree-search');
await search.fill('Level 256');
await wait(300);
check((await rows()) === LIMIT, `searching for the deepest group keeps the groups around it (${await rows()} rows)`);
await search.fill('no such group');
await wait(200);
check((await rows()) === 0 && (await p.locator('.otree[role="tree"] .empty', { hasText: 'Nothing named' }).count()) === 1, 'a search with no match says so');
await search.fill('');
await wait(200);
await p.screenshot({ path: path.join(shots, 'outliner-deepest.png') });

// collapsing the top row hides everything under it
await p.locator('.otree[role="tree"] [role="treeitem"] .caret').first().click();
await wait(200);
check((await rows()) === 1, `collapsing the top group leaves one row (${await rows()})`);

// ---- a file nested far past the limit opens, is brought up to it, and says so
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
const src = path.join(path.dirname(doc), 'outliner-src.sketchcraft');
await X({ cmd: 'save', path: src });
const m = JSON.parse(fs.readFileSync(src, 'utf8'));
const DEEP = 3000;
let id = Math.max(m.next_id ?? 0, ...Object.keys(m.faces ?? {}).map(Number), ...Object.keys(m.vertices ?? {}).map(Number), ...Object.keys(m.edges ?? {}).map(Number)) + 1;
m.groups = {};
for (let i = 0; i < DEEP; i++, id++) m.groups[id] = { name: `Deep ${i + 1}`, tag: null, hidden: false, parent: i === 0 ? null : id - 1, locked: false };
for (const f of Object.values(m.faces)) f.group = id - 1;
for (const v of Object.values(m.vertices)) v.group = id - 1;
for (const e of Object.values(m.edges)) e.group = id - 1;
m.next_id = id + 1;
const deepFile = path.join(path.dirname(doc), 'outliner-deep.sketchcraft');
fs.writeFileSync(deepFile, JSON.stringify(m));
const opened = await X({ cmd: 'open', path: deepFile });
check(opened.ok === true && opened.groups_flattened === DEEP - LIMIT, `a ${DEEP}-level file opens, ${DEEP - LIMIT} groups brought up to the limit (${JSON.stringify({ ok: opened.ok, flattened: opened.groups_flattened })})`);
check(Array.isArray(opened.groups_flattened_names) && opened.groups_flattened_names.length > 0 && opened.groups_flattened_names.length <= 5, `the reply names a few of them (${(opened.groups_flattened_names ?? []).join(', ')})`);
const scene = await pageScene(p);
check(scene.groups.length === DEEP, `no group was lost (${scene.groups.length})`);
check(scene.faces.length === 1, 'the square is still there');
await reload();
check(await drawn(), 'the window draws the repaired model, and again after a reload (the autosaved copy)');
// the groups past the limit now sit beside the last level, so every group is still a row
// (up to the row cap of 2,000: the rest are counted in a line, and the big-model case below checks that line)
check((await rows()) === Math.min(DEEP, 2000), `the outliner lists the groups up to its row cap (${await rows()} of ${DEEP})`);
const levels = await p.locator('.otree[role="tree"] [role="treeitem"]').evaluateAll((els) => els.map((e) => Number(e.getAttribute('aria-level'))));
check(Math.max(...levels) <= LIMIT, `no row is deeper than the limit (${Math.max(...levels)})`);
await p.screenshot({ path: path.join(shots, 'outliner-repaired.png') });
// the window stays usable: pick the last row
await p.locator('.otree[role="tree"] [role="treeitem"] .orow').last().click();
await wait(200);
check(await drawn(), 'a row can be picked');

// ---- a big model: the outliner draws a capped number of rows, says how many more, and a search reaches the rest
const BIG = 40000;
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
await X({ cmd: 'save', path: src });
const big = JSON.parse(fs.readFileSync(src, 'utf8'));
let bid = Math.max(big.next_id ?? 0, ...Object.keys(big.faces ?? {}).map(Number), ...Object.keys(big.vertices ?? {}).map(Number), ...Object.keys(big.edges ?? {}).map(Number)) + 1;
big.groups = {};
for (let i = 0; i < BIG; i++, bid++) big.groups[bid] = { name: `Big ${String(i + 1).padStart(6, '0')}`, tag: null, hidden: false, parent: null, locked: false };
big.next_id = bid + 1;
const bigFile = path.join(path.dirname(doc), 'outliner-big.sketchcraft');
fs.writeFileSync(bigFile, JSON.stringify(big));
check((await X({ cmd: 'open', path: bigFile })).ok === true, `a ${BIG}-group file opens`);
const t0 = Date.now();
await reload();
const secs = (Date.now() - t0) / 1000;
check(await drawn(), 'the window draws a model with a very large number of groups');
check(secs < 20, `and reloads in a reasonable time (${secs.toFixed(1)} s)`);
const shown = await rows();
check(shown > 0 && shown <= 2000, `the outliner draws a capped number of rows (${shown})`);
const moreText = (await p.locator('.otree[role="tree"] [data-more-rows]').first().textContent()) ?? '';
check(moreText.includes(String(BIG - shown)) && /more rows/.test(moreText), `a line says how many are not shown ("${moreText}")`);
await search.fill('Big 039999');
await wait(500);
check((await rows()) === 1, `a search reaches a row past the cap (${await rows()})`);
await search.fill('');
await wait(300);
await p.screenshot({ path: path.join(shots, 'outliner-big.png') });
fs.rmSync(bigFile, { force: true });

check(!problems.length, `no page errors${problems.length ? ': ' + problems.slice(0, 3).join(' | ') : ''}`);
fs.rmSync(src, { force: true });
fs.rmSync(deepFile, { force: true });
await b.close();
process.exit(failed ? 1 : 0);
