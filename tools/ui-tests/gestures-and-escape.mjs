#!/usr/bin/env node
// Double-click gestures, Escape on dialogs and menus, the Place message, and bad saved settings, in a real browser
// (Chromium sends PointerEvent.detail 0, so a double-click used to do nothing) against the dev page and bridge
// (docs/dev/building.md), on a scratch model: SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   node tools/ui-tests/gestures-and-escape.mjs
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

const b = await launch();
const problems = [];
const open = async (storage = {}) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
  await ctx.addInitScript((s) => {
    try {
      for (const [k, v] of Object.entries(s)) localStorage.setItem(k, v);
    } catch {}
  }, storage);
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas');
  await wait(600);
  return { p, ctx };
};
const X = (p, c) => pageExec(p, c);
const sheetOpen = (p) => p.locator('.sheet-backdrop').count().then((n) => n > 0);

// where a world point lands in the page (dev builds expose the view as window.__sketchcraftView)
const screen = (p, [x, y, z]) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, [x, y, z]);
// commands sent straight to the bridge don't redraw the page by themselves: reload it to show them
const reshow = async (p) => {
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(800);
};
const fit = async (p) => {
  await reshow(p);
  await p.getByRole('button', { name: 'Select', exact: true }).first().click();
  await p.keyboard.press('Shift+z');
  await wait(700);
};
const selectedTitle = (p) => p.locator('.section-title', { hasText: /selected|^Face \d|^Edge \d/ }).first().innerText({ timeout: 3000 }).catch(() => '');

// ---- double-click: finish a wall (Arch workspace) ----------------------------------------------
{
  const { p, ctx } = await open({ 'sketchcraft.workspace': 'arch' });
  await X(p, { cmd: 'new' });
  await reshow(p);
  await p.getByRole('button', { name: /^Wall/ }).first().click();
  const pts = [[-3, -3, 0], [3, -3, 0], [3, 3, 0]];
  for (const w of pts) {
    const [x, y] = await screen(p, w);
    await p.mouse.click(x, y);
    await wait(700);
  }
  const [x, y] = await screen(p, [-3, 3, 0]);
  await p.mouse.dblclick(x, y);
  await wait(900);
  const s = await pageScene(p);
  check(s.faces.length > 0 && s.groups.length >= 1, `a double-click finishes the wall (${s.faces.length} faces, ${s.groups.length} groups)`);
  await ctx.close();
}

// ---- double-click: open a group, select connected, triple-click; push/pull repeat -------------
{
  const { p, ctx } = await open();
  await X(p, { cmd: 'new' });
  const f = (await X(p, { cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] })).face;
  const g = (await X(p, { cmd: 'group', ids: [f] })).group;
  const f2 = (await X(p, { cmd: 'rect', from: [8, 0, 0], to: [12, 4, 0] })).face;
  await X(p, { cmd: 'rect', from: [12, 0, 0], to: [16, 4, 0] });
  await fit(p);
  const [gx, gy] = await screen(p, [2, 2, 0]);
  await p.mouse.dblclick(gx, gy);
  await wait(800);
  check((await pageScene(p)).activeGroup === g, 'a double-click opens the group');
  const [ox, oy] = await screen(p, [10, -6, 0]);
  await p.mouse.dblclick(ox, oy);
  await wait(800);
  check((await pageScene(p)).activeGroup == null, 'a double-click outside closes it');
  const [fx, fy] = await screen(p, [10, 2, 0]);
  await p.mouse.dblclick(fx, fy);
  await wait(800);
  const two = await selectedTitle(p);
  check(/^5 selected/.test(two), `a double-click selects the face and its edges (${two})`);
  await p.mouse.click(ox, oy);
  await wait(300);
  await p.mouse.click(fx, fy, { clickCount: 3 });
  await wait(800);
  const three = await selectedTitle(p);
  check(/^(\d+) selected/.test(three) && Number(three.split(' ')[0]) > 5, `a triple-click selects everything connected (${three})`);
  // push/pull: pull once, then a double-click on another face repeats the distance
  await p.keyboard.press('Escape');
  await X(p, { cmd: 'new' });
  await X(p, { cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] });
  await X(p, { cmd: 'rect', from: [10, 0, 0], to: [14, 4, 0] });
  await fit(p);
  await p.keyboard.press('p');
  const [ax, ay] = await screen(p, [2, 2, 0]);
  await p.mouse.click(ax, ay);
  await p.keyboard.type('2');
  await p.keyboard.press('Enter');
  await wait(800);
  const [cx, cy] = await screen(p, [12, 2, 0]);
  await p.mouse.dblclick(cx, cy);
  await wait(900);
  const s = await pageScene(p);
  const tops = s.faces.map((q) => Math.max(...q.outer.map((v) => v[2])));
  // one box pulled by typing 2 (5 faces at its top height or above), then the other by a double-click (5 more)
  check(s.faces.length === 12 && tops.filter((t) => Math.abs(t - 2) < 1e-6).length === 10, `a double-click with Push/Pull repeats the last distance (${s.faces.length} faces)`);
  await ctx.close();
}

// ---- Escape closes dialogs and menus --------------------------------------------------------
{
  const { p, ctx } = await open();
  await X(p, { cmd: 'new' });
  for (const [name, click] of [
    ['Learn', () => p.locator('.learn-btn').click()],
    ['Themes', () => p.locator('.skin-btn').click()],
  ]) {
    await click();
    await wait(400);
    check(await sheetOpen(p), `${name} opens`);
    await p.keyboard.press('Escape');
    await wait(300);
    check(!(await sheetOpen(p)), `Escape closes ${name}`);
  }
  await p.getByRole('menuitem', { name: 'File', exact: true }).click();
  await wait(300);
  check((await p.getByRole('menu', { name: 'File' }).count()) === 1, 'the File menu opens');
  await p.keyboard.press('Escape');
  await wait(300);
  check((await p.getByRole('menu', { name: 'File' }).count()) === 0, 'Escape closes the File menu');
  await p.getByRole('menuitem', { name: 'File', exact: true }).click();
  await p.getByRole('menuitem', { name: /^Plugins/ }).click();
  await p.getByRole('menuitem', { name: /Manage plugins/ }).click();
  await wait(400);
  check(await sheetOpen(p), 'the Plugins dialog opens');
  await p.keyboard.press('Escape');
  await wait(300);
  check(!(await sheetOpen(p)), 'Escape closes the Plugins dialog');
  // a dialog's Escape doesn't also act on the model: Escape inside a group editing session keeps it
  const f = (await X(p, { cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] })).face;
  const g = (await X(p, { cmd: 'group', ids: [f] })).group;
  await X(p, { cmd: 'edit', id: g });
  await reshow(p);
  await p.locator('.learn-btn').click();
  await wait(300);
  await p.keyboard.press('Escape');
  await wait(500);
  check((await pageScene(p)).activeGroup === g, 'Escape on a dialog leaves the group being edited alone');
  await ctx.close();
}

// ---- Place while a component is open ---------------------------------------------------------
{
  const { p, ctx } = await open();
  await X(p, { cmd: 'new' });
  const f = (await X(p, { cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] })).face;
  const made = await X(p, { cmd: 'make_component', ids: [f], name: 'Part' });
  await X(p, { cmd: 'edit', id: made.instance });
  await reshow(p);
  await p.getByRole('button', { name: /^Library/ }).click().catch(() => p.locator('.tabs button', { hasText: 'Library' }).click());
  await wait(500);
  await p.locator('.tile-hit').first().click();
  await wait(500);
  check((await p.locator('.mode-pill', { hasText: /Placing/ }).count()) === 0, 'no Place mode is entered while a component is open');
  check(/Close the component you're editing first/.test(await p.locator('body').innerText()), 'and the message says to close the component first');
  await ctx.close();
}

// ---- an idle view draws (almost) nothing ---------------------------------------------------
// The view draws a frame only when the picture would change. Drawing every frame kept a machine without a GPU
// (WebGL on the CPU, as on Linux CI) busy on every core and slowed everything else in the page (engine round trips,
// a plugin's commands) by 10 to 60 times. CI draws with SwiftShader, so this runs there as a user's VM would.
{
  const { p, ctx } = await open({ 'sketchcraft.help': 'off' });
  await X(p, { cmd: 'new' });
  const r = await X(p, { cmd: 'rect', from: [-2, -2, 0], to: [2, 2, 0] });
  await X(p, { cmd: 'pushpull', face: r.face, distance: 2 });
  await fit(p);
  // resting on the box: the Select tool's see-through hover highlight is in the picture (it once kept the view drawing)
  const [hx, hy] = await screen(p, [0, 0, 2]);
  await p.mouse.move(hx, hy, { steps: 3 });
  await wait(1000);
  const drawn = () => p.evaluate(() => window.__sketchcraftFrames?.drawn ?? -1);
  const d0 = await drawn();
  const idle = await p.evaluate(async () => {
    const t0 = performance.now();
    let frames = 0;
    await new Promise((res) => {
      const f = () => (++frames, performance.now() - t0 < 3000 ? requestAnimationFrame(f) : res());
      requestAnimationFrame(f);
    });
    const a = performance.now();
    for (let i = 0; i < 50; i++) await new Promise((r) => setTimeout(r, 0));
    return { frames, hop: (performance.now() - a) / 50 };
  });
  const idleDrawn = (await drawn()) - d0;
  console.log(`note: idle for 3 s, the view drew ${idleDrawn} of ${idle.frames} animation frames; a setTimeout(0) took ${idle.hop.toFixed(1)} ms`);
  check(d0 > 0 && idleDrawn >= 0 && idleDrawn <= 5, `an idle view draws only now and then, not every frame (${idleDrawn} frames drawn in 3 s)`);
  const shot = () => p.locator('.viewport canvas').screenshot();
  const before = await shot();
  const d1 = await drawn();
  await p.mouse.wheel(0, -400);
  await wait(800);
  check((await drawn()) > d1, `zooming draws again (${(await drawn()) - d1} frames)`);
  check(!before.equals(await shot()), 'and the picture shows the new view');
  await ctx.close();
}

// ---- a slow mouse wheel zooms, a trackpad swipe still orbits (#232) -------------------------------
// On macOS a slow notch of a mouse wheel is a few pixels straight down, like the start of a swipe, and it orbited.
// Zooming keeps the direction the eye looks in and changes its distance; orbiting turns it.
{
  const { p, ctx } = await open({ 'sketchcraft.help': 'off' });
  await X(p, { cmd: 'new' });
  const r = await X(p, { cmd: 'rect', from: [-2, -2, 0], to: [2, 2, 0] });
  await X(p, { cmd: 'pushpull', face: r.face, distance: 2 });
  await fit(p);
  const box = await p.locator('.viewport canvas').boundingBox();
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  const look = () =>
    p.evaluate(() => {
      const T = window.__sketchcraftView.current;
      const d = T.camera.position.clone().sub(T.controls.target);
      return { dir: d.clone().normalize().toArray(), dist: d.length() };
    });
  const turned = (a, b) => Math.acos(Math.min(1, a.dir.reduce((s, v, i) => s + v * b.dir[i], 0)));
  /** wheel steps far enough apart to be separate notches */
  const notches = async (steps) => {
    for (const [dx, dy] of steps) {
      await p.mouse.wheel(dx, dy);
      await wait(250);
    }
  };
  // with nothing known yet, a small straight step is a swipe, as before; sideways makes it one for certain
  let a = await look();
  await notches([[3, 4]]);
  let b = await look();
  check(turned(a, b) > 0.005, `a trackpad swipe orbits (turned ${turned(a, b).toFixed(3)} rad)`);
  // a fast notch: a mouse for certain; then slow 4 px notches zoom, not orbit
  await notches([[0, 120]]);
  a = await look();
  await notches([[0, 4], [0, 4], [0, 4]]);
  b = await look();
  check(turned(a, b) < 1e-4 && b.dist > a.dist * 1.2, `after a mouse is seen, slow notches zoom (turned ${turned(a, b).toFixed(5)} rad, ${a.dist.toFixed(2)} -> ${b.dist.toFixed(2)})`);
  check((await p.evaluate(() => localStorage.getItem('sketchcraft.wheelSeen'))) === 'mouse', 'and the mouse is remembered for next time');
  // View › Scroll wheel › Trackpad: a straight step swipes again; Mouse: even a sideways one zooms
  await p.getByRole('menuitem', { name: 'View', exact: true }).click();
  await p.getByRole('menuitem', { name: 'Scroll wheel' }).hover();
  await p.getByRole('menuitemradio', { name: /^Trackpad/ }).click();
  check((await p.evaluate(() => localStorage.getItem('sketchcraft.wheel'))) === 'trackpad', 'View › Scroll wheel › Trackpad is kept');
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  a = await look();
  await notches([[0, 4]]);
  b = await look();
  check(turned(a, b) > 0.005, `set to Trackpad, a small step orbits (turned ${turned(a, b).toFixed(3)} rad)`);
  await p.getByRole('menuitem', { name: 'View', exact: true }).click();
  await p.getByRole('menuitem', { name: 'Scroll wheel' }).hover();
  await p.getByRole('menuitemradio', { name: /^Mouse/ }).click();
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  a = await look();
  await notches([[2, 4]]);
  b = await look();
  check(turned(a, b) < 1e-4 && b.dist !== a.dist, `set to Mouse, every step zooms (turned ${turned(a, b).toFixed(5)} rad)`);
  await ctx.close();
}

// ---- orbit pivots about the point under the cursor (#224) -----------------------------------------
// A middle-button drag that starts on the box's top face turns the view about that spot (it stays under the
// cursor, and the eye does not jump when the drag starts); started on empty sky, about the view's centre as before;
// and with the setting sketchcraft.orbitPivot=off, as before everywhere. Perspective and parallel.
for (const [parallel, storage] of [[false, {}], [true, {}], [false, { 'sketchcraft.orbitPivot': 'off' }]]) {
  const { p, ctx } = await open({ 'sketchcraft.help': 'off', ...storage });
  await X(p, { cmd: 'new' });
  const r = await X(p, { cmd: 'rect', from: [-2, -2, 0], to: [2, 2, 0] });
  await X(p, { cmd: 'pushpull', face: r.face, distance: 2 });
  await fit(p);
  if (parallel) {
    await p.getByRole('button', { name: /parallel/i }).first().click({ timeout: 2000 }).catch(() => {});
    await wait(500);
  }
  const eye = () => p.evaluate(() => window.__sketchcraftView.current.camera.position.toArray());
  const isParallel = await p.evaluate(() => window.__sketchcraftView.current.camera.isOrthographicCamera);
  const corner = [1.4, 1.2, 2]; // on the top face, off the edges
  const [cx, cy] = await screen(p, corner);
  const drag = async (x, y) => {
    const e0 = await eye();
    await p.mouse.move(x, y);
    await p.mouse.down({ button: 'middle' });
    await p.mouse.move(x + 1, y, { steps: 1 });
    await wait(150);
    const e1 = await eye();
    await p.mouse.move(x + 90, y + 25, { steps: 12 });
    await p.mouse.up({ button: 'middle' });
    await wait(1500);
    return { e0, e1, e2: await eye() };
  };
  const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
  const { e0, e1, e2 } = await drag(cx, cy);
  const [nx, ny] = await screen(p, corner);
  const off = storage['sketchcraft.orbitPivot'] === 'off';
  const label = `${off ? 'setting off' : isParallel ? 'parallel' : 'perspective'}`;
  check(!!parallel === !!isParallel, `the camera is ${isParallel ? 'parallel' : 'perspective'} as asked`);
  check(dist(e0, e1) < 0.2, `${label}: the eye does not jump when the orbit starts (${dist(e0, e1).toFixed(4)})`);
  check(dist(e1, e2) > 1, `${label}: the drag orbits (${dist(e1, e2).toFixed(2)})`);
  const moved = Math.hypot(nx - cx - 0, ny - cy - 0);
  if (off) check(moved > 40, `${label}: the spot swings away without the pivot (${moved.toFixed(0)} px)`);
  else check(moved < 6, `${label}: the spot under the cursor stays put (${moved.toFixed(1)} px)`);
  await ctx.close();
}
{
  // on empty sky the view's centre stays where it is on screen
  const { p, ctx } = await open({ 'sketchcraft.help': 'off' });
  await X(p, { cmd: 'new' });
  const r = await X(p, { cmd: 'rect', from: [-2, -2, 0], to: [2, 2, 0] });
  await X(p, { cmd: 'pushpull', face: r.face, distance: 2 });
  await fit(p);
  const tgt = await p.evaluate(() => window.__sketchcraftView.current.controls.target.toArray());
  const [tx, ty] = await screen(p, tgt);
  const box = await p.locator('.viewport canvas').boundingBox();
  await p.mouse.move(box.x + 25, box.y + 25);
  await p.mouse.down({ button: 'middle' });
  await p.mouse.move(box.x + 120, box.y + 50, { steps: 12 });
  await p.mouse.up({ button: 'middle' });
  await wait(1500);
  const t2 = await p.evaluate(() => window.__sketchcraftView.current.controls.target.toArray());
  const [ux, uy] = await screen(p, tgt);
  check(Math.hypot(t2[0] - tgt[0], t2[1] - tgt[1], t2[2] - tgt[2]) < 1e-6, 'orbiting from empty sky keeps the view centre as the pivot');
  check(Math.hypot(ux - tx, uy - ty) < 6, 'and the centre stays put on screen');
  await ctx.close();
}

// ---- bad saved settings never stop the app starting ------------------------------------------
for (const [what, storage] of [
  ['workspace edits: tools not a list', { 'sketchcraft.workspaceEdits': '{"model":{"tools":"x"}}' }],
  ['workspace edits: tools of numbers', { 'sketchcraft.workspaceEdits': '{"model":{"tools":[[1,2],[3]],"dock":7},"arch":null}' }],
  ['workspace edits: an array', { 'sketchcraft.workspaceEdits': '[1,2,3]' }],
  ['workspace edits: not JSON', { 'sketchcraft.workspaceEdits': '{{{' }],
  ['build settings: wrong kinds', { 'sketchcraft.build': '{"roofKind":"dome","wallHeight":"tall","columnRound":3}' }],
  ['render settings: wrong kinds', { 'sketchcraft.render': '{"size":{"a":1},"sun":null,"width":"x"}' }],
  ['everything else', { 'sketchcraft.skin': 'x', 'sketchcraft.skinData': '[1]', 'sketchcraft.layout': 'diagonal', 'sketchcraft.workspace': '{}', 'sketchcraft.figure': '9', 'sketchcraft.help': 'maybe', 'sketchcraft.currentFile': '' }],
]) {
  const before = problems.length;
  const { p, ctx } = await open(storage);
  const ok = (await p.locator('nav.toolbar button.tool').count()) > 3 && (await p.locator('.viewport canvas').count()) === 1;
  check(ok && problems.length === before, `starts with ${what}`);
  await ctx.close();
}

await b.close();
if (problems.length) console.log('page errors:', problems.slice(0, 5));
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
