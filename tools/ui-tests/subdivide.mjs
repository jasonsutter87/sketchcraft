#!/usr/bin/env node
// Making a SubD object from the app (#56): from a box, right-click › Subdivide…, SubD mode opens, smooth faces
// exist, and Undo restores the box; the SubD box starter; the Shift+S entries and the Library buttons.
// In a real browser against the dev page and bridge (docs/dev/building.md), on a scratch model:
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/subdivide.mjs
// Models are built with pageExec, from inside the page (what the other ui-tests do): a command sent to the
// bridge from outside can sit unseen by a freshly loaded page until its next poll, so each build is followed
// by waiting for the page to show it.
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
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(600);
const X = (c) => pageExec(p, c);
const scene = () => pageScene(p);
const cages = async () => Object.keys((await scene()).cages ?? {}).length;
const inSubd = () => p.locator('.subd-bar').count().then((n) => n > 0);
/** wait for the page to show something (it polls the bridge), up to 6 s */
const until = async (fn, ms = 6000) => {
  for (let t = 0; t < ms; t += 150) {
    if (await fn()) return true;
    await wait(150);
  }
  return false;
};
const screen = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = T.renderer.domElement.getBoundingClientRect();
    return [r.left + (v.x * 0.5 + 0.5) * r.width, r.top + (-v.y * 0.5 + 0.5) * r.height];
  }, pt);
const box = async (name = 'Box') => {
  await X({ cmd: 'new' });
  await X({ cmd: 'begin_group', name });
  const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0], as: 'b' });
  await X({ cmd: 'pushpull', face: r.face, distance: 2 });
  await X({ cmd: 'end_group' });
  // the page doesn't redraw for commands sent straight to the bridge: reload to pick them up
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await p.locator('.views button', { hasText: 'fit' }).first().click();
  return until(async () => (await p.locator('.orow', { hasText: name }).count()) > 0);
};
const leave = async () => {
  if (await inSubd()) await p.keyboard.press('Enter');
  await until(async () => !(await inSubd()));
};

// ---- Subdivide a box: right-click menu -------------------------------------------------------
check(await box(), 'a box group shows in the Outliner');
const facesBefore = (await scene()).faces.length;
check(facesBefore === 6, `the box has 6 faces (${facesBefore})`);
await p.locator('.orow', { hasText: 'Box' }).first().click();
const [cx, cy] = await screen([1, 1, 1]);
await p.mouse.click(cx, cy, { button: 'right' });
const item = p.locator('.menu button', { hasText: 'Subdivide…' }).first();
check(await item.isVisible().catch(() => false), 'right-click on a group offers Subdivide…');
await p.screenshot({ path: path.join(shots, 'context-menu.png') });
await item.click();
await p.waitForSelector('[role=dialog][aria-label="Subdivide"]');
check(await p.getByLabel('Smoothness (levels)').isVisible(), 'the sheet asks for levels');
check(await p.getByText('Keep sharp edges').isVisible(), 'and whether to keep sharp edges');
check(await p.getByText('Tab to shape, Enter when done').isVisible(), 'and says how to shape it: Tab to shape, Enter when done');
await p.screenshot({ path: path.join(shots, 'subdivide-sheet.png') });
await p.locator('[role=dialog][aria-label="Subdivide"] button', { hasText: /^Subdivide$/ }).click();
check(await until(inSubd), 'SubD mode opens (the SubD bar shows)');
const s1 = await scene();
const gid = Object.keys(s1.cages ?? {})[0];
check(Object.keys(s1.cages ?? {}).length === 1, 'the group is now a SubD object with a cage');
check(s1.faces.length > 6 && s1.faces.length >= 48, `it has smooth faces (${s1.faces.length}; the box had 6)`);
check(s1.groups.find((g) => String(g.id) === gid)?.name === 'Box', 'it keeps its name');
check((await p.locator('.subd-coach').innerText()).includes('Enter'), 'the SubD hint mentions Enter when done');
await wait(400);
await p.screenshot({ path: path.join(shots, 'subd-mode-after-subdivide.png') });
await p.keyboard.press('Control+z');
check(await until(async () => (await cages()) === 0), 'Undo takes the SubD object away');
const s2 = await scene();
check(s2.faces.length === 6 && s2.groups.length === 1 && s2.groups[0].name === 'Box', `and the box is back: ${s2.faces.length} faces, group ${s2.groups[0]?.name}`);
await leave();
check(!(await inSubd()), 'and SubD mode is closed');

// ---- keep sharp edges + levels reach the engine ----------------------------------------------
await box('Sharp');
await p.locator('.orow', { hasText: 'Sharp' }).first().click();
await p.keyboard.press('Shift+S');
await p.keyboard.type('subdivide');
await wait(300);
await p.getByText('Subdivide…').first().click();
await p.waitForSelector('[role=dialog][aria-label="Subdivide"]');
await p.getByLabel('Smoothness (levels)').selectOption('1');
await p.getByText('Keep sharp edges').click();
await p.locator('[role=dialog][aria-label="Subdivide"] button', { hasText: /^Subdivide$/ }).click();
check(await until(inSubd), 'Shift+S › Subdivide… works the same way');
const s3 = await scene();
const c3 = Object.values(s3.cages ?? {})[0];
check(c3?.levels === 1, `level 1 was used (${c3?.levels})`);
check((c3?.creases?.length ?? 0) > 0, `keep sharp made creases (${c3?.creases?.length ?? 0})`);
await leave();

// ---- Subdivide with nothing selected says what to do -----------------------------------------
await X({ cmd: 'new' });
await until(async () => (await p.locator('.orow').count()) === 0);
await p.keyboard.press('Shift+S');
await p.keyboard.type('subdivide');
await wait(300);
await p.getByText('Subdivide…').first().click();
await wait(300);
check((await p.locator('.toast').innerText().catch(() => '')).includes('Select one group or component'), 'with nothing selected it says what to select');
check((await p.locator('[role=dialog][aria-label="Subdivide"]').count()) === 0, 'and opens no sheet');

// ---- SubD box ---------------------------------------------------------------------------------
await X({ cmd: 'new' });
// the page doesn't redraw for commands sent straight to the bridge: reload, or the right-click below lands on the old box
await p.reload();
await p.waitForSelector('.viewport canvas');
check(await until(async () => (await p.locator('.orow').count()) === 0), 'the page shows the empty model');
await p.locator('body').click({ position: { x: 640, y: 400 } }).catch(() => {});
await wait(500);
await p.mouse.click(400, 600, { button: 'right' });
const sb = p.locator('.menu button', { hasText: 'SubD box' }).first();
check(await sb.isVisible().catch(() => false), 'right-click on empty space offers SubD box');
await sb.click();
check(await until(inSubd), 'SubD box opens SubD mode');
const s4 = await scene();
const c4 = Object.values(s4.cages ?? {})[0];
check(c4 && c4.points.length === 8 && c4.faces.length === 6, `it is a cube cage (${c4?.points.length} points, ${c4?.faces.length} faces)`);
check(s4.faces.length > 6, `with a smooth surface (${s4.faces.length} faces)`);
await wait(600);
await p.screenshot({ path: path.join(shots, 'subd-box.png') });
await leave();
await p.keyboard.press('Control+z');
check(await until(async () => (await cages()) === 0), 'Undo takes the SubD box away');

// ---- Library buttons --------------------------------------------------------------------------
await p.locator('.tabs button', { hasText: 'Library' }).click();
await p.locator('summary', { hasText: 'Smooth shapes' }).click();
await p.getByRole('button', { name: 'SubD box' }).click();
check(await until(inSubd), 'Library › Smooth shapes › SubD box opens SubD mode');
await leave();
// a component: Subdivide explodes it to a group first
await box('Part');
const gp = (await scene()).groups.find((g) => g.name === 'Part');
await X({ cmd: 'make_component', ids: [gp.id], name: 'Part' });
check(await until(async () => (await scene()).instances.length === 1), 'a component exists');
// the page doesn't redraw for commands sent straight to the bridge: reload, so it shows the component and not the old group
await p.reload();
await p.waitForSelector('.viewport canvas');
await p.locator('.views button', { hasText: 'fit' }).first().click();
await until(async () => (await p.locator('.orow', { hasText: 'Part' }).count()) > 0);
await p.locator('.orow', { hasText: 'Part' }).first().click();
const [px, py] = await screen([1, 1, 1]);
await p.mouse.click(px, py, { button: 'right' });
await p.locator('.menu button', { hasText: 'Subdivide…' }).first().click();
await p.waitForSelector('[role=dialog][aria-label="Subdivide"]');
await p.locator('[role=dialog][aria-label="Subdivide"] button', { hasText: /^Subdivide$/ }).click();
check(await until(inSubd), 'a selected component can be subdivided too (SubD mode opens)');
check((await cages()) === 1 && (await scene()).instances.length === 0, 'it became a SubD group');
await leave();

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
