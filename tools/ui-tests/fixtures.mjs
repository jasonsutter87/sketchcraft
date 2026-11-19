#!/usr/bin/env node
// Fixtures & furniture, in a real browser: open the Library section, search it, read a tile's size, and place a toilet in a
// room made with the Room tool. Near a wall it turns and sits with its back flush against it (and says so); Shift
// places it freely; one Undo removes it.
//   node tools/ui-tests/run.mjs fixtures
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

// a 5 x 4 m room (centre lines): 4.8 x 3.8 m inside, walls 0.2 m thick, so the south wall's inside face is at y = 0.1
const made = await X({ cmd: 'room_rect', from: [0, 0, 0], to: [5, 4, 0] });
check(made.ok === true, 'a room made with the Room tool command exists');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);
await plan();
await wait(300);

// ---- the Library section ----------------------------------------------------------------------------------------
await p.locator('.panel.left .tabs button', { hasText: 'Library' }).click({ timeout: 3000 }).catch(() => {});
await wait(300);
const section = p.locator('details.fixtures');
check((await section.count()) === 1, 'the Library has a "Fixtures & furniture" section');
check(((await section.locator('> summary').textContent()) || '').trim() === 'Fixtures & furniture', 'named "Fixtures & furniture"');
if (!(await section.evaluate((d) => d.open))) await section.locator('> summary').click();
await wait(300);
const groups = (await section.locator('details[data-group] > summary').allTextContents()).map((t) => t.trim());
check(JSON.stringify(groups) === JSON.stringify(['Bath', 'Kitchen', 'Bedroom', 'Living & dining', 'Laundry']), `it has the five rooms as sub-groups (${groups.join(', ')})`);
const tiles = await section.locator('.tile.comp').count();
check(tiles === 26, `with 26 tiles (${tiles})`);
const toilet = section.locator('.tile.comp', { hasText: 'Toilet' });
const tip = (await toilet.first().getAttribute('title')) ?? '';
check(/Toilet, 0\.38 × 0\.70 × 0\.78 m \(15 × 28 × 31 in\)/.test(tip), `a tile's tooltip gives its name and size (${tip.slice(0, 70)})`);
await toilet.first().scrollIntoViewIfNeeded();
await wait(1500); // thumbnails arrive one at a time
const thumb = await toilet.first().locator('img.thumb').count();
check(thumb === 1, 'the tile has a thumbnail picture');

// searching finds fixtures by name and by room
await p.locator('.lib-search').fill('bed');
await wait(300);
const beds = (await section.locator('.tile-name').allInnerTexts()).map((t) => t.trim());
check(['Queen bed', 'King bed', 'Twin bed'].every((n) => beds.includes(n)) && !beds.includes('Toilet'), `searching "bed" finds the three beds, and their room (${beds.join(', ')})`);
await p.locator('.lib-search').fill('queen');
await wait(300);
check((await section.locator('.tile.comp').count()) === 1, 'searching "queen" leaves one tile');
await p.locator('.lib-search').fill('laundry');
await wait(300);
const laundry = (await section.locator('.tile-name').allInnerTexts()).map((t) => t.trim());
check(laundry.join() === 'Washer,Dryer', `searching a room name finds its fixtures (${laundry.join(', ')})`);
await p.locator('.lib-search').fill('toilet');
await wait(300);
check((await section.locator('.tile.comp').count()) === 1, 'searching "toilet" leaves one tile');

// ---- place it: near the south wall it sits back against it ----------------------------------------------------------
await section.locator('.tile.comp').first().locator('.tile-hit').click();
await wait(300);
check((await p.locator('.mode-pill').innerText().catch(() => '')).includes('Placing Toilet'), 'the tile starts placing');
await move([2.5, 0.35, 0]);
const hint = await p.locator('.snap').innerText({ timeout: 1500 }).catch(() => '');
check(/Against the wall · Shift to place freely/.test(hint), `near a wall it says so (${hint.replace(/\s+/g, ' ').slice(0, 60)})`);
await click([2.5, 0.35, 0]);
let s = await until((st) => st.instances.length === 1);
check(s.instances.length === 1, 'a click places one toilet');
let t = s.instances[0] ? spot(s.instances[0]) : null;
check(!!t && near(t.x, 2.5, 0.06), `it is where the cursor was along the wall (x ${t?.x.toFixed(3)})`);
check(!!t && near(t.y, 0.1, 0.01), `its back edge is flush on the wall's inside face (y ${t?.y.toFixed(3)} is 0.1)`);
check(!!t && near(t.z, 0, 0.01), 'standing on the floor');
check(!!t && near(t.cos, -1, 0.01), `turned to face into the room, away from the wall (cos ${t?.cos.toFixed(3)})`);
await key('Control+z');
s = await until((st) => st.instances.length === 0);
check(s.instances.length === 0, 'one Undo removes it');

// the east wall: a quarter turn the other way, and again flush
await move([4.6, 2.0, 0]);
await click([4.6, 2.0, 0]);
s = await until((st) => st.instances.length === 1);
t = s.instances[0] ? spot(s.instances[0]) : null;
check(!!t && near(t.x, 4.9, 0.01) && near(t.sin, -1, 0.01), `at the east wall it sits flush at x 4.9 and turns a quarter (x ${t?.x.toFixed(3)}, sin ${t?.sin.toFixed(3)})`);
await key('Control+z');
await until((st) => st.instances.length === 0);

// Shift places it freely, even next to a wall: where the cursor is, not turned
await p.keyboard.down('Shift');
await move([2.5, 0.35, 0]);
await p.mouse.down();
await p.mouse.up();
await wait(400);
await p.keyboard.up('Shift');
s = await until((st) => st.instances.length === 1);
t = s.instances[0] ? spot(s.instances[0]) : null;
check(!!t && near(t.y, 0.35, 0.06) && near(t.cos, 1, 0.01), `Shift places it freely, where the cursor is and not turned (y ${t?.y.toFixed(3)}, cos ${t?.cos.toFixed(3)})`);
await key('Control+z');
await until((st) => st.instances.length === 0);

// far from any wall it just goes where you click
await click([2.5, 2.0, 0]);
s = await until((st) => st.instances.length === 1);
t = s.instances[0] ? spot(s.instances[0]) : null;
check(!!t && near(t.x, 2.5, 0.06) && near(t.y, 2.0, 0.06) && near(t.cos, 1, 0.01), `in the middle of the room it goes where you click, unturned (${t?.x.toFixed(2)}, ${t?.y.toFixed(2)})`);
await key('Control+z');
s = await until((st) => st.instances.length === 0);
check(s.instances.length === 0, 'and Undo takes it away');

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
