#!/usr/bin/env node
// The texturing test set, in a real browser: open the Library's "Texturing test set" section, check its six tiles (names,
// tooltips with size and purpose, thumbnails), search it, then place every model with a click in the model and check it
// arrives as its own component; one Undo removes each.
//   node tools/ui-tests/run.mjs texturing-set
// Like the other suites it runs on a scratch model, in a browser pinned to en-US, with real pointer and key events.
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

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

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1700, height: 1000 } });
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));
const X = (c) => pageExec(p, c);
const scene = () => pageScene(p);
const until = async (pred, ms = 3000) => {
  let s = await scene();
  for (let t = 0; t < ms && !pred(s); t += 100) {
    await wait(100);
    s = await scene();
  }
  return s;
};
const near = (a, c, tol = 0.02) => Math.abs(a - c) <= tol;

// a plan view from 12 m up, centred on the room
const plan = () =>
  p.evaluate(() => {
    const T = window.__sketchcraftView.current;
    const target = [2.5, 2, 0];
    T.controls.target.set(...target);
    T.persp.position.set(2.5, 1.9995, 12);
    T.persp.up.set(0, 0, 1);
    T.persp.lookAt(...target);
    T.controls.update();
  });
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
const move = async (pt) => {
  const [x, y] = await px(pt);
  await p.mouse.move(x, y, { steps: 4 });
  await wait(120);
};
const click = async (pt) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.click(x, y);
  await wait(300);
};
const key = async (k) => {
  await p.keyboard.press(k);
  await wait(200);
};
/** where a placement's origin is, and which way its local x axis points (cos of the turn) */
const spot = (inst) => ({ x: inst.transform[12], y: inst.transform[13], z: inst.transform[14], cos: inst.transform[0], sin: inst.transform[1] });

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);
await X({ cmd: 'new' });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);

await plan();
await wait(300);

// ---- the Library section ----------------------------------------------------------------------------------------
await p.locator('.panel.left .tabs button', { hasText: 'Library' }).click({ timeout: 3000 }).catch(() => {});
await wait(300);
const section = p.locator('details.texset');
check((await section.count()) === 1, 'the Library has a "Texturing test set" section');
check(((await section.locator('> summary').textContent()) || '').trim() === 'Texturing test set', 'named "Texturing test set"');
if (!(await section.evaluate((d) => d.open))) await section.locator('> summary').click();
await wait(300);
const names = (await section.locator('.tile-name').allInnerTexts()).map((t) => t.trim());
check(names.join() === 'Beaver,Dice,Can,Globe,Crate,Soccer ball', `it holds the six models (${names.join(', ')})`);
const tip = (await section.locator('.tile.comp', { hasText: 'Soccer ball' }).first().getAttribute('title')) ?? '';
check(/Soccer ball, 0\.22 × 0\.22 × 0\.22 m: 12 pentagons and 20 hexagons/.test(tip), `a tooltip gives size and purpose (${tip.slice(0, 80)})`);
await section.scrollIntoViewIfNeeded();
await wait(2500); // thumbnails arrive one at a time
check((await section.locator('img.thumb').count()) === 6, 'every tile has a thumbnail picture');

await p.locator('.lib-search').fill('ball');
await wait(300);
check((await section.locator('.tile.comp').count()) === 1, 'searching "ball" leaves one tile');
await p.locator('.lib-search').fill('texturing');
await wait(300);
check((await section.locator('.tile.comp').count()) === 6, 'searching the collection name shows all six');
await p.locator('.lib-search').fill('');
await wait(300);

// ---- place each one -----------------------------------------------------------------------------------------------
const expected = { Beaver: 'Beaver', Dice: 'Dice', Can: 'Can', Globe: 'Globe', Crate: 'Crate', 'Soccer ball': 'Soccer ball' };
const openSection = async () => {
  if (!(await section.evaluate((d) => d.open))) await section.locator('> summary').click();
  await wait(200);
};
for (const [tile, comp] of Object.entries(expected)) {
  await openSection();
  await section.locator('.tile.comp', { hasText: tile }).first().locator('.tile-hit').click();
  await wait(300);
  check((await p.locator('.mode-pill').innerText().catch(() => '')).includes(`Placing ${tile}`), `${tile}: the tile starts placing`);
  await click([2.5, 2.0, 0]);
  let s = await until((st) => st.instances.length === 1);
  const c = s.components.find((x) => x.name === comp);
  check(s.instances.length === 1 && !!c && c.placed === 1, `${tile}: a click places one ${comp} component`);
  const t = s.instances[0] ? spot(s.instances[0]) : null;
  check(!!t && near(t.x, 2.5, 0.06) && near(t.y, 2.0, 0.06) && near(t.z, 0, 0.01), `${tile}: it stands on the floor where you clicked (${t?.x.toFixed(2)}, ${t?.y.toFixed(2)})`);
  await key('Escape');
  await key('Control+z');
  s = await until((st) => st.instances.length === 0);
  check(s.instances.length === 0, `${tile}: one Undo removes it`);
  await p.locator('.panel.left .tabs button', { hasText: 'Library' }).click({ timeout: 3000 }).catch(() => {});
}

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
