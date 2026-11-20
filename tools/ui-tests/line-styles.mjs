#!/usr/bin/env node
// Line weight and style per tag and per edge (#3), in a real browser against the dev page and bridge:
// the Tags panel sets a tag's weight and style through its menus; the model keeps it (the scene says so, the menus
// still show it after a reload) and the model view gets a pen for it; Ctrl+Z takes it back and Ctrl+Shift+Z puts it
// back; an edge picked in the view gets its own weight in Entity Info, which beats its tag.
//   SHOT_DIR=/some/folder node tools/ui-tests/line-styles.mjs
// SKETCHCRAFT_DOC must be set (and the bridge run with the same). A model built with pageExec doesn't show on a
// page that is already open, so the page is reloaded to show it.
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

const b = await launch();
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1400, height: 900 } });
const p = await ctx.newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(400);

// two boxes, each its own group on its own tag
await pageExec(p, { cmd: 'new' });
for (const [name, x] of [['Walls', 0], ['Doors', 4]]) {
  const g = (await pageExec(p, { cmd: 'begin_group', name })).group;
  const f = (await pageExec(p, { cmd: 'rect', from: [x, 0, 0], to: [x + 3, 2, 0] })).face;
  await pageExec(p, { cmd: 'pushpull', face: f, distance: 2 });
  await pageExec(p, { cmd: 'end_group' });
  await pageExec(p, { cmd: 'tag', ids: [g], tag: name });
}
// a loose line on the ground, apart from the boxes: an edge that can be picked without opening a group
await pageExec(p, { cmd: 'line', points: [[0, -3, 0], [3, -3, 0]] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(800);

const scene = () => pageScene(p);
const weightMenu = (tag) => p.getByLabel(`Line weight of tag ${tag}`);
const styleMenu = (tag) => p.getByLabel(`Line style of tag ${tag}`);

// ---- a tag, through the Tags panel ----------------------------------------------------------------
check((await weightMenu('Walls').count()) === 1 && (await styleMenu('Walls').count()) === 1, 'the Tags panel has a weight menu and a style menu for each tag');
check((await weightMenu('Walls').inputValue()) === '' && (await styleMenu('Walls').inputValue()) === '', 'they start at Default');
let s = await scene();
check(!s.edgeStyle?.length, 'with no styles the model view draws every edge with the plain 1 px line');

await weightMenu('Walls').selectOption('0.7');
await wait(500);
await styleMenu('Walls').selectOption('dashed');
await wait(700);
s = await scene();
check(s.tagStyles?.Walls?.weight_mm === 0.7 && s.tagStyles?.Walls?.line === 'dashed', `the model holds the tag's weight and style (${JSON.stringify(s.tagStyles)})`);
const pens = s.edgeStyles ?? [];
check(pens.length === 1 && pens[0].w === 0.7 && pens[0].dash.length === 2, `the model view gets one pen for it: 0.7 mm, dashed (${JSON.stringify(pens)})`);
const styled = (s.edgeStyle ?? []).filter((k) => k > 0).length;
check(styled === 12 && (s.edgeStyle ?? []).length === 25, `12 of the 25 edges (the Walls box) use it; the Doors box and the loose line keep the default (${styled} of ${(s.edgeStyle ?? []).length})`);
check(!s.tagStyles?.Doors, 'the other tag is untouched');
await p.screenshot({ path: path.join(shots, 'line-styles-tag.png') });

// it persists: the file on disk, and the menus after a reload
await wait(1500);
const onDisk = JSON.parse(fs.readFileSync(process.env.SKETCHCRAFT_DOC, 'utf8'));
check(onDisk.drafting?.tag_styles?.Walls?.weight_mm === 0.7 && onDisk.format === 2, 'the saved file has it, as format 2');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);
check((await weightMenu('Walls').inputValue()) === '0.7' && (await styleMenu('Walls').inputValue()) === 'dashed', 'after a reload the menus still show 0.7 mm and Dashed');

// ---- undo and redo, from the keyboard ---------------------------------------------------------------
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(800);
s = await scene();
check(s.tagStyles?.Walls?.weight_mm === 0.7 && !s.tagStyles?.Walls?.line, `Ctrl+Z takes back the style first (${JSON.stringify(s.tagStyles?.Walls)})`);
await p.keyboard.press('Control+z');
await wait(800);
s = await scene();
check(!s.tagStyles || !s.tagStyles.Walls, `and Ctrl+Z again the weight (${JSON.stringify(s.tagStyles)})`);
check(!s.edgeStyle?.length, 'the model view is back to plain lines');
check((await weightMenu('Walls').inputValue()) === '', 'and the menu shows Default');
await p.keyboard.press('Control+Shift+z');
await p.keyboard.press('Control+Shift+z');
await wait(900);
s = await scene();
check(s.tagStyles?.Walls?.weight_mm === 0.7 && s.tagStyles?.Walls?.line === 'dashed', 'Ctrl+Shift+Z twice puts both back');

// the Line weights toggle hides the pens in the model view without touching the model
const toggle = p.getByLabel('Line weights');
if (await toggle.count()) {
  const bucketCount = () => p.evaluate(() => { let n = 0; window.__sketchcraftView.current.model.traverse((o) => { if (o.userData?.pen) n++; }); return n; });
  await toggle.check(); // the Classic look starts with it off
  await wait(500);
  const on = await bucketCount();
  await toggle.uncheck();
  await wait(500);
  const off = await bucketCount();
  await toggle.check();
  await wait(500);
  check(on === 1 && off === 0 && (await bucketCount()) === 1, `the "Line weights" style turns the pens on and off in the view (${on} -> ${off} -> ${await bucketCount()})`);
} else {
  console.log('note: the Line weights check box is in the Styles panel; not open here, so the toggle isn\'t driven');
}

// ---- one edge, picked in the view -------------------------------------------------------------------
// the loose line, projected to the screen
await p.keyboard.press('Escape');
const spot = await p.evaluate(async ({ edgeList }) => {
  const v = window.__sketchcraftView.current;
  v.camera.updateMatrixWorld();
  const view = v.camera.matrixWorldInverse.elements;
  const proj = v.camera.projectionMatrix.elements;
  const eye = v.camera.position;
  const rect = v.renderer.domElement.getBoundingClientRect();
  const doors = edgeList.filter((e) => e.a[1] < -2.9 && e.b[1] < -2.9);
  doors.sort((a, c) => {
    const d = (e) => Math.hypot((e.a[0] + e.b[0]) / 2 - eye.x, (e.a[1] + e.b[1]) / 2 - eye.y, (e.a[2] + e.b[2]) / 2 - eye.z);
    return d(a) - d(c);
  });
  const e = doors[0];
  const m = [0, 1, 2].map((i) => (e.a[i] + e.b[i]) / 2);
  const mul = (M, x) => [0, 1, 2, 3].map((r) => M[r] * x[0] + M[4 + r] * x[1] + M[8 + r] * x[2] + M[12 + r] * x[3]);
  const c = mul(proj, mul(view, [...m, 1]));
  return { id: e.id, x: rect.left + ((c[0] / c[3] + 1) / 2) * rect.width, y: rect.top + ((1 - c[1] / c[3]) / 2) * rect.height };
}, { edgeList: (await scene()).edgeList });
await p.locator('nav.toolbar button[aria-label="Select"]').first().click();
await p.mouse.click(spot.x, spot.y);
await wait(600);
const edgeWeight = p.getByLabel('Line weight of selected edges');
if (!(await edgeWeight.count())) {
  await p.screenshot({ path: path.join(shots, 'line-styles-edge-miss.png') });
  check(false, `clicking edge ${spot.id} at ${Math.round(spot.x)},${Math.round(spot.y)} selects it (Entity Info shows its line menus)`);
} else {
  check(true, 'picking an edge shows its line weight and style in Entity Info');
  await edgeWeight.selectOption('1.4');
  await wait(700);
  s = await scene();
  const e = s.edgeList.find((x) => x.id === spot.id);
  check(e?.draft?.weight_mm === 1.4, `the edge holds its own weight (${JSON.stringify(e?.draft)})`);
  check((s.edgeStyles ?? []).some((x) => x.w === 1.4), 'and the model view has a 1.4 mm pen for it');
  await p.screenshot({ path: path.join(shots, 'line-styles-edge.png') });
  await p.keyboard.press('Control+z');
  await wait(700);
  s = await scene();
  check(!s.edgeList.find((x) => x.id === spot.id)?.draft, 'Ctrl+Z takes the edge\'s own weight back');
}

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
