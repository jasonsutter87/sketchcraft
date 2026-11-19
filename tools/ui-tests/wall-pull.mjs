#!/usr/bin/env node
// Wall pull, in a real browser: draw a room with the Wall tool, hover a wall (it says what it is), pull it by
// dragging and by typing a distance, check the room grew and the thickness stayed, Escape cancels, a refused pull
// says why and changes nothing, and one Undo puts the room back exactly.
//   node tools/ui-tests/run.mjs wall-pull
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
const xs = (s) => s.faces.flatMap((f) => f.outer.map((v) => v[0]));
const ys = (s) => s.faces.flatMap((f) => f.outer.map((v) => v[1]));
const lo = (a) => Math.min(...a);
const hi = (a) => Math.max(...a);

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
const plan = () => camera([0, -0.0005, 60], [0, 0, 0]);
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
  await wait(200);
};
const drag = async (pts) => {
  await move(pts[0]);
  await p.mouse.down();
  for (const q of pts.slice(1)) await move(q, 6);
  await p.mouse.up();
  await wait(300);
};
const key = async (k) => {
  await p.keyboard.press(k);
  await wait(150);
};
const typed = async (text) => {
  await p.keyboard.type(text, { delay: 15 });
  await p.keyboard.press('Enter');
  await wait(300);
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
const shape = (s) => JSON.stringify(s.faces.map((f) => [f.id, f.outer]));

/** a new model: a closed 6 x 6 m room drawn with the Wall tool (clicks round the corners, back to the first), plus a window on the south wall */
const room = async () => {
  await key('Escape');
  await X({ cmd: 'new' });
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  await plan();
  await wait(250);
  await tool('wall');
  await click([-3, -3, 0]);
  await click([3, -3, 0]);
  await click([3, 3, 0]);
  await click([-3, 3, 0]);
  await click([-3, -3, 0]);
  const s = await until((s) => s.faces.length > 0);
  await key('Escape');
  return s;
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);

// ---- the room ------------------------------------------------------------------------------------------------------
const s0 = await room();
check(s0.faces.length > 0 && s0.groups.length === 1, `the Wall tool draws a room: click round the corners, click the first again (${s0.faces.length} faces, ${s0.groups.length} group)`);
const before = shape(s0);
const west0 = lo(xs(s0));
check(near(west0, -3.1, 0.15) && near(hi(xs(s0)), 3.1, 0.15), `it is about 6 m wide, walls on the lines (x ${west0.toFixed(2)} to ${hi(xs(s0)).toFixed(2)})`);
const thick = (s, x) => {
  // the two upright faces that look along x near x: their planes differ by the wall's thickness
  const planes = s.faces.filter((f) => f.normal && Math.abs(f.normal[0]) > 0.99 && near(f.outer[0][0], x, 0.4)).map((f) => f.outer[0][0]).sort((a, c) => a - c);
  return planes.length >= 2 ? planes[1] - planes[0] : NaN;
};
const t0 = thick(s0, west0 + 0.1);

// ---- the tool is easy to find and explains itself ----------------------------------------------------------------
await tool('wallpull');
// (a tool that does not fit on the bar sits behind "+N more" and shows on that button, named in its title)
const onBtn = p.locator('nav.toolbar button.tool.on').first();
const onName = (await onBtn.getAttribute('aria-label').catch(() => null)) ?? (await onBtn.getAttribute('title').catch(() => '')) ?? '';
check(onName.startsWith(INFO.wallpull.name), `the toolbar has a "${INFO.wallpull.name}" button, and it turns the tool on (${onName.slice(0, 40)})`);
check(/Pull wall/.test(await coach()) && /Hover a wall/.test(await coach()), `the step hint says what to do first (${(await coach()).replace(/\s+/g, ' ').slice(0, 90)})`);

// ---- drag a wall ---------------------------------------------------------------------------------------------------
// the top of the west wall, nearer its outside (x -3.1) than its inside (x -2.9)
await move([-3.05, 0, 2.7]);
await wait(500);
const lit = await p.evaluate(() => {
  const T = window.__sketchcraftView.current;
  return T.overlay.children.length;
});
check(lit > 0, `hovering a wall highlights it (${lit} overlay objects)`);
await drag([[-3.05, 0, 2.7], [-3.55, 0, 2.7], [-4.05, 0, 2.7]]);
const s1 = await until((s) => lo(xs(s)) < west0 - 0.5);
check(near(lo(xs(s1)), west0 - 1, 0.12), `dragging the west wall a metre out moves it a metre (x ${lo(xs(s1)).toFixed(2)}, was ${west0.toFixed(2)})`);
check(near(thick(s1, lo(xs(s1)) + 0.1), t0, 0.005), `its thickness stayed (${thick(s1, lo(xs(s1)) + 0.1).toFixed(3)}, was ${t0.toFixed(3)})`);
check(near(hi(xs(s1)), hi(xs(s0)), 0.001) && near(lo(ys(s1)), lo(ys(s0)), 0.001) && near(hi(ys(s1)), hi(ys(s0)), 0.001), 'the other three walls did not move');
check(s1.faces.length === s0.faces.length, `no faces were made or lost (${s1.faces.length})`);
const grown = Math.max(...s1.faces.filter((f) => f.normal && Math.abs(f.normal[1]) > 0.99).map((f) => Math.max(...f.outer.map((v) => v[0])) - Math.min(...f.outer.map((v) => v[0]))));
check(near(grown, 7.2, 0.2), `and the walls it joins stretched to follow: the north and south walls are now about ${grown.toFixed(1)} m long`);
// one Undo
await key('Control+z');
const s2 = await until((s) => shape(s) === before);
check(shape(s2) === before, 'one Undo puts the room back exactly');

// ---- a typed distance ----------------------------------------------------------------------------------------------
await tool('wallpull');
await click([-3.05, 0, 2.7]);
check(/slide the wall/.test(await coach()), `after the first click the hint says how to finish (${(await coach()).replace(/\s+/g, ' ').slice(0, 100)})`);
await move([-3.5, 0, 2.7]);
await typed('1.2');
const s3 = await until((s) => lo(xs(s)) < west0 - 0.5);
check(near(lo(xs(s3)), west0 - 1.2, 0.001), `a typed 1.2 then Enter pulls it exactly 1.2 m (x ${lo(xs(s3)).toFixed(3)}, was ${west0.toFixed(3)})`);
check(near(thick(s3, lo(xs(s3)) + 0.1), t0, 0.001), 'with its thickness unchanged');
await key('Control+z');
check(shape(await until((s) => shape(s) === before)) === before, 'a single Undo takes the typed pull back too');

// ---- Escape --------------------------------------------------------------------------------------------------------
await tool('wallpull');
await click([-3.05, 0, 2.7]);
await move([-4.2, 0, 2.7]);
await key('Escape');
await wait(300);
check(shape(await scene()) === before, 'Escape drops a pull in progress: nothing changed');

// ---- pulling it the other way (shrinks the room) -------------------------------------------------------------------
await tool('wallpull');
await click([-3.05, 0, 2.7]);
await typed('-1');
const s4 = await until((s) => lo(xs(s)) > west0 + 0.5);
check(near(lo(xs(s4)), west0 + 1, 0.001), `a negative distance pushes the wall in and the room shrinks (x ${lo(xs(s4)).toFixed(3)})`);
await key('Control+z');
await until((s) => shape(s) === before);

// ---- a pull that cannot be done is refused, with a reason ----------------------------------------------------------
await tool('wallpull');
await click([-3.05, 0, 2.7]);
await typed('-9');
await wait(400);
const why = await toast();
check(/can't pull that far/i.test(why), `a pull that would flip the room inside out is refused with a reason (${why.replace(/\s+/g, ' ').slice(0, 100)})`);
check(shape(await scene()) === before, 'and the model is unchanged');

// ---- something that is not a wall says so --------------------------------------------------------------------------
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [-2, -2, 0], to: [2, 2, 0] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);
await plan();
await tool('wallpull');
await click([0, 0, 0]);
const nope = await toast();
check(/upright|wall/i.test(nope), `clicking a floor says why it can't be pulled (${nope.replace(/\s+/g, ' ').slice(0, 90)})`);

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
