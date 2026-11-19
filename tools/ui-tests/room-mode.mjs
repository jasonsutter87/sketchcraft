#!/usr/bin/env node
// Room mode, in a real browser: drag two rooms side by side (they share a wall, no double wall), read the live size while
// dragging and the labels afterwards, pull the shared wall and watch both areas change, place a door, rename a room, turn
// walls down (a view setting only), get refused when rooms overlap, and Undo each action in one step.
//   node tools/ui-tests/run.mjs room-mode
// Like the other suites it runs on a scratch model, in a browser pinned to en-US, with real pointer and key events.
import fs from 'node:fs';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene, ROOT } from '../demos/lib.mjs';

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
const INFO = Object.fromEntries(
  [...fs.readFileSync(path.join(ROOT, 'app/src/workspace.ts'), 'utf8').matchAll(/^ {2}([a-z0-9]+): \{ name: "([^"]*)", key: "([^"]*)" \},?$/gm)].map((m) => [m[1], { name: m[2] }])
);

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1700, height: 1000 } });
await ctx.addInitScript((tools) => {
  try {
    localStorage.setItem('sketchcraft.workspaceEdits', JSON.stringify({ model: { tools: [tools] } }));
  } catch {}
}, Object.keys(INFO));
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
const near = (a, c, tol = 0.1) => Math.abs(a - c) <= tol;

// a plan view from 16 m up, centred on the two rooms: about 80 px to the metre, so the 0.1 m grid is 8 px
const camera = async (eye, target) => {
  for (let i = 0; ; i++) {
    try {
      return await p.evaluate(([eye, target]) => {
        const T = window.__sketchcraftView.current;
        T.controls.target.set(...target);
        T.persp.position.set(...eye);
        T.persp.up.set(0, 0, 1);
        T.persp.lookAt(...target);
        T.controls.update();
      }, [eye, target]);
    } catch (e) {
      if (i >= 4) throw e;
      await p.waitForLoadState('load').catch(() => {});
      await wait(500);
    }
  }
};
const plan = () => camera([4, 1.4995, 16], [4, 1.5, 0]);
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
const move = async (pt, steps = 4) => {
  const [x, y] = await px(pt);
  await p.mouse.move(x, y, { steps });
  await wait(80);
};
const click = async (pt) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.click(x, y);
  await wait(250);
};
const drag = async (pts, { release = true } = {}) => {
  await move(pts[0]);
  await p.mouse.down();
  for (const q of pts.slice(1)) await move(q, 6);
  if (release) {
    await p.mouse.up();
    await wait(350);
  }
};
const key = async (k) => {
  await p.keyboard.press(k);
  await wait(150);
};
const tool = async (id) => {
  const name = INFO[id].name;
  const btn = p.locator(`nav.toolbar button[aria-label="${name}"]`);
  if (!(await btn.count())) {
    await p.locator('nav.toolbar button.more').click();
    await p.locator('.more-tools button[role=option]', { hasText: name.split(':')[0].split(' (')[0] }).first().click();
  } else await btn.first().click();
  await wait(150);
};
const coach = () => p.locator('.coach').innerText({ timeout: 800 }).catch(() => '');
const toast = () => p.locator('.toast').innerText({ timeout: 800 }).catch(() => '');
const labels = async () => (await p.locator('.label span').allInnerTexts()).filter((t) => /·/.test(t));
/** the area in a "Room 1 · 14.4 m²" label */
const areaOf = (list, name) => {
  const m = list.map((t) => t.match(new RegExp(`^${name} · ([0-9.]+) m²`))).find(Boolean);
  return m ? Number(m[1]) : NaN;
};
const untilLabels = async (pred, ms = 3000) => {
  let l = await labels();
  for (let t = 0; t < ms && !pred(l); t += 100) {
    await wait(100);
    l = await labels();
  }
  return l;
};
const shape = (s) => JSON.stringify([s.faces.map((f) => [f.id, f.outer]), s.groups.map((g) => [g.id, g.name]), s.instances.length]);
const rooms = (s) => s.groups.filter((g) => g.tag === 'Rooms');
/** wall-sized upright faces that look along x and sit near the line x = at (a wall's two sides; end caps are small) */
const wallSidesAt = (s, at) => s.faces.filter((f) => f.normal && Math.abs(f.normal[0]) > 0.99 && Math.abs(f.outer[0][0] - at) < 0.15 && Math.max(...f.outer.map((v) => v[1])) - Math.min(...f.outer.map((v) => v[1])) > 2).length;

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);
await X({ cmd: 'new' });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);
await plan();
await wait(250);

// ---- the tool is easy to find and explains itself ---------------------------------------------------------------
await tool('room');
const onBtn = p.locator('nav.toolbar button.tool.on').first();
const onName = (await onBtn.getAttribute('aria-label').catch(() => null)) ?? (await onBtn.getAttribute('title').catch(() => '')) ?? '';
check(onName.startsWith(INFO.room.name), `the toolbar has a "${INFO.room.name}" button, and it turns the tool on (${onName.slice(0, 40)})`);
check(/Room/.test(await coach()) && /drag/i.test(await coach()), `the step hint says what to do first (${(await coach()).replace(/\s+/g, ' ').slice(0, 100)})`);
// (the Build panel lives on the Library tab of the left panel)
await p.locator('.panel.left .tabs button', { hasText: 'Library' }).click({ timeout: 2000 }).catch(() => {});
await wait(300);
check((await p.locator('.build-options').count()) > 0 || (await p.locator('.tiles.build button.tile.on').count()) > 0, 'the Build panel shows Room too (with its options)');

// ---- drag a room: the live size, then four walls and a floor ------------------------------------------------------
await drag([[0, 0, 0], [2, 1.5, 0], [4, 3, 0]], { release: false });
const live = (await p.locator('.label.live span').allInnerTexts()).join(' | ');
check(/3\.80 m × 2\.80 m · 10\.6 m²/.test(live), `while dragging it shows the inside size and area: ${live}`);
await p.mouse.up();
await wait(400);
let s = await until((st) => rooms(st).length === 1 && st.faces.length > 0);
const r1 = rooms(s)[0];
check(!!r1 && r1.name === 'Room 1', `release draws the room and names it "${r1?.name}"`);
const wallGroups = s.groups.filter((g) => g.tag === 'Walls');
check(wallGroups.length === 1 && wallGroups[0].parent === r1.id, `its walls are an ordinary group on the "Walls" tag inside it (${wallGroups.length})`);
const floor = s.faces.filter((f) => f.normal && f.normal[2] > 0.99 && f.group === r1.id);
check(floor.length === 1, 'and it has a floor face at the ground');
let l = await untilLabels((x) => x.length >= 1);
check(near(areaOf(l, 'Room 1'), 10.64, 0.06), `its label gives name and area (${l.join(', ')})`);
const one = shape(s);

// ---- a second room, dragged against the first: the wall is shared -----------------------------------------------
// (it starts on open ground at the far corner: pressing on a wall pulls it instead)
await drag([[8, 0, 0], [6, 1.5, 0], [4, 3, 0]]);
s = await until((st) => rooms(st).length === 2);
check(rooms(s).length === 2 && rooms(s)[1].name === 'Room 2', `the second room is "Room 2" (${rooms(s).map((g) => g.name).join(', ')})`);
check(wallSidesAt(s, 4.0) === 2, `the wall between them is shared: one wall, two sides, no double wall (${wallSidesAt(s, 4.0)} wall-sized faces across x 3.9 to 4.1)`);
l = await untilLabels((x) => x.length >= 2);
const a1 = areaOf(l, 'Room 1');
const a2 = areaOf(l, 'Room 2');
check(near(a1, 10.64, 0.06) && near(a2, 10.64, 0.06), `both labels show their areas (${l.join(', ')})`);
const two = shape(s);

// ---- pull the shared wall: both rooms change, both labels follow --------------------------------------------------
await drag([[4.05, 1.5, 2.7], [4.55, 1.5, 2.7], [5.05, 1.5, 2.7]]);
s = await until((st) => shape(st) !== two);
l = await untilLabels((x) => Math.abs(areaOf(x, 'Room 1') - a1) > 0.5);
const b1 = areaOf(l, 'Room 1');
const b2 = areaOf(l, 'Room 2');
check(b1 > a1 + 2 && b2 < a2 - 2, `dragging the shared wall a metre east grows Room 1 and shrinks Room 2, and the labels follow (${a1} -> ${b1}, ${a2} -> ${b2})`);
check(near(b1 - a1, 2.8, 0.25) && near(a2 - b2, 2.8, 0.25), `by a metre of depth each (${(b1 - a1).toFixed(2)} and ${(a2 - b2).toFixed(2)} m²)`);
const pulled = shape(s);
await key('Control+z');
s = await until((st) => shape(st) === two);
check(shape(s) === two, 'one Undo puts the wall back, and both rooms with it');
l = await untilLabels((x) => near(areaOf(x, 'Room 1'), a1, 0.06));
check(near(areaOf(l, 'Room 1'), a1, 0.06) && near(areaOf(l, 'Room 2'), a2, 0.06), `and the labels go back (${l.join(', ')})`);

// ---- a door: click a wall ------------------------------------------------------------------------------------------
await click([6, 3.02, 2.7]);
s = await until((st) => st.instances.length === 1);
check(s.instances.length === 1, `a click on a wall places a door (${s.instances.length} placed)`);
const door = s.instances[0];
check(!!door, `it is a placed door${door ? ' (' + (door.name ?? door.def ?? '') + ')' : ''}`);
await key('Control+z');
s = await until((st) => st.instances.length === 0);
check(s.instances.length === 0 && shape(s) === two, 'one Undo takes the door away again');

// ---- a window, from the panel's choice ----------------------------------------------------------------------------
const choice = p.locator('.build-options select').first();
if (await choice.count()) {
  await choice.selectOption('window');
  await click([6, 3.02, 2.7]);
  s = await until((st) => st.instances.length === 1);
  check(s.instances.length === 1, 'choosing Window in the Room options makes the next click on a wall a window');
  await key('Control+z');
  await until((st) => st.instances.length === 0);
} else check(false, 'the Room options have a Door / Window choice');

// ---- an overlap is refused, with a reason, and nothing changes -----------------------------------------------------
await drag([[7, 1, 0], [8.5, 1.5, 0], [10, 2, 0]]);
await wait(300);
const why = await toast();
check(/overlap/i.test(why), `a room dragged over another is refused with a reason (${why.replace(/\s+/g, ' ').slice(0, 110)})`);
check(shape(await scene()) === two, 'and the model is unchanged');

// ---- rename ---------------------------------------------------------------------------------------------------------
const nameBox = p.locator('.room-list input').first();
if (await nameBox.count()) {
  await nameBox.fill('Kitchen');
  await nameBox.press('Enter');
  l = await untilLabels((x) => x.some((t) => t.startsWith('Kitchen')));
  check(l.some((t) => t.startsWith('Kitchen · ')), `a room can be renamed in the Room options, and its label follows (${l.join(', ')})`);
  await key('Control+z');
} else check(false, 'the Room options list the rooms by name');

// ---- walls down: a view setting, nothing in the model --------------------------------------------------------------
const before = shape(await scene());
const walls = p.locator('label.field', { hasText: 'Walls down' }).locator('input[type=checkbox]');
if (await walls.count()) {
  await walls.check();
  await wait(400);
  const cut = await p.evaluate(() => window.__sketchcraft.current.cutaway != null);
  check(cut, 'Walls down cuts the view low (a clip plane in the viewport)');
  check(shape(await scene()) === before, 'and changes nothing in the model');
  await walls.uncheck();
  await wait(300);
  check(!(await p.evaluate(() => window.__sketchcraft.current.cutaway != null)), 'turning it off puts the walls back');
} else check(false, 'the Room options have a Walls down switch');

// ---- undo each room: one step each ---------------------------------------------------------------------------------
await tool('select');
await key('Control+z');
s = await until((st) => rooms(st).length === 1);
check(rooms(s).length === 1 && shape(s) === one, 'Undo removes the whole second room in one step, back to exactly the first');
await key('Control+z');
s = await until((st) => rooms(st).length === 0);
check(rooms(s).length === 0 && s.faces.length === 0, 'and one more removes the first');

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
