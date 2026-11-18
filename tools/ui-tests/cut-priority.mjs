#!/usr/bin/env node
// Cut intersection priority (#88), in a real browser against the dev page and bridge: a wall (red) runs into a slab
// (blue) in a section plane. Outliner > Tags > "Intersection priority" opens a list (top wins) with a hint and a
// tiny example. Reordering the two tags (the arrow buttons, a drag, Alt+Arrow on the grip, or typing a number) flips
// who owns the overlap: the pixel at the overlap's middle goes red <-> blue, and the on-screen cap of the lower tag
// is trimmed back (its triangles leave the winner's footprint). Ctrl+Z / Ctrl+Shift+Z take a reorder back and forth
// as ONE step. The saved file carries the numbers.
//   SHOT_DIR=/some/folder node tools/ui-tests/cut-priority.mjs
// SKETCHCRAFT_DOC must be set (and the bridge run with the same).
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
const ctx = await b.newContext({ viewport: { width: 1400, height: 900 } });
const p = await ctx.newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(400);

// a wall (x 0..3) running into a slab (x 2..6): they overlap on x 2..3
await pageExec(p, { cmd: 'new' });
const block = async (name, lo, hi, color) => {
  const g = (await pageExec(p, { cmd: 'begin_group', name })).group;
  const f = (await pageExec(p, { cmd: 'rect', from: [lo[0], lo[1], 0], to: [hi[0], hi[1], 0] })).face;
  await pageExec(p, { cmd: 'pushpull', face: f, distance: 2.5 });
  await pageExec(p, { cmd: 'end_group' });
  await pageExec(p, { cmd: 'tag', ids: [g], tag: name });
  await pageExec(p, { cmd: 'tag_visible', tag: name, visible: true });
  await pageExec(p, { cmd: 'tag_style', tag: name, cut: { fill: 'solid', color } });
};
await block('Wall', [0, 0], [3, 2], '#ff0000');
await block('Slab', [2, 0], [6, 2], '#0000ff');
await pageExec(p, { cmd: 'section_plane', origin: [0, 0, 1.2], normal: [0, 0, -1] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1200);
await p.evaluate(() => {
  const T = window.__sketchcraftView.current;
  T.controls.target.set(3, 1, 0);
  T.persp.position.set(3, 0.999, 22);
  T.persp.up.set(0, 0, 1);
  T.persp.lookAt(3, 1, 0);
  T.controls.update();
});
await wait(800);

const scene = () => pageScene(p);
const prios = async () => {
  const s = (await scene()).tagStyles ?? {};
  return { Wall: s.Wall?.cut?.priority ?? 0, Slab: s.Slab?.cut?.priority ?? 0 };
};
const rows = () => p.locator('.prio-list .prio-row').evaluateAll((els) => els.map((e) => e.getAttribute('data-tag')));
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
// the colour of the screen at a model point (the screenshot, read back through a canvas, averaged over 7 px)
const colorAt = async (pt, name) => {
  const at = await px(pt);
  const shot = await p.screenshot({ path: path.join(shots, `cut-priority-${name}.png`) });
  return p.evaluate(
    async ([b64, [x, y]]) => {
      const i = await new Promise((res, rej) => {
        const im = new Image();
        im.onload = () => res(im);
        im.onerror = rej;
        im.src = 'data:image/png;base64,' + b64;
      });
      const cv = document.createElement('canvas');
      cv.width = i.width;
      cv.height = i.height;
      const g = cv.getContext('2d');
      g.drawImage(i, 0, 0);
      const d = g.getImageData(Math.round(x) - 3, Math.round(y) - 3, 7, 7).data;
      const s = [0, 0, 0];
      for (let k = 0; k < d.length; k += 4) for (let j = 0; j < 3; j++) s[j] += d[k + j];
      return s.map((v) => Math.round(v / (d.length / 4)));
    },
    [shot.toString('base64'), at],
  );
};
const red = (c) => c[0] > 200 && c[1] < 60 && c[2] < 60;
const blue = (c) => c[2] > 200 && c[0] < 60 && c[1] < 60;
const slabTris = () =>
  p.evaluate(() => {
    let xs = [];
    window.__sketchcraftView.current.model.traverse((o) => {
      if (o.userData?.sectionSolid === 'Slab') {
        const a = o.geometry.getAttribute('position');
        for (let i = 0; i < a.count; i++) xs.push(a.getX(i));
      }
    });
    return xs.length ? Math.min(...xs) : null;
  });

// ---- the control ------------------------------------------------------------------------------------
const toggle = p.getByRole('button', { name: 'Intersection priority', exact: true });
check((await toggle.count()) === 1, 'the Tags panel has an Intersection priority button');
check((await p.locator('.prio-list').count()) === 0, 'closed until asked');
await toggle.click();
await wait(300);
const hint = (await p.locator('.prio-editor .hint').first().textContent()) ?? '';
check(/nearer the top keeps the overlap/.test(hint), `it explains itself ("${hint}")`);
check((await p.locator('.prio-editor .prio-example').count()) === 1 && /Wall above Slab/.test((await p.locator('.prio-eg').textContent()) ?? ''), 'with a tiny example');
check(JSON.stringify(await rows()) === JSON.stringify(['Slab', 'Wall']), `the list starts in name order, all equal (${await rows()})`);
check((await p.locator('.prio-row .tie').count()) === 2, 'equal numbers are marked as tied');
check((await p.getByLabel('Intersection priority of tag Wall').inputValue()) === '0', 'every number is 0 (the default)');

// ---- the arrow buttons: Wall up -> the wall wins -------------------------------------------------------------
await p.getByLabel('Move tag Wall up', { exact: true }).click();
await wait(1200);
check(JSON.stringify(await rows()) === JSON.stringify(['Wall', 'Slab']), `Wall moved above Slab (${await rows()})`);
let pr = await prios();
check(pr.Wall > pr.Slab, `the numbers say so (${JSON.stringify(pr)})`);
let c = await colorAt([2.5, 1, 1.2], 'wall-wins');
check(red(c), `the overlap is the wall's red (${c})`);
check(((await slabTris()) ?? 0) >= 3 - 1e-6, `the slab's cap is trimmed back to the wall's edge (starts at x=${await slabTris()})`);
check((await p.getByLabel('Intersection priority of tag Wall').inputValue()) === String(pr.Wall), 'the number field follows');
const wallColor = await colorAt([1, 1, 1.2], 'wall-body');
const slabColor = await colorAt([5, 1, 1.2], 'slab-body');
check(red(wallColor) && blue(slabColor), `each body keeps its own colour (${wallColor} / ${slabColor})`);

// ---- undo: one step for the reorder ----------------------------------------------------------------------
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(1000);
pr = await prios();
check(pr.Wall === 0 && pr.Slab === 0, `Ctrl+Z takes the whole reorder back in one step (${JSON.stringify(pr)})`);
await p.keyboard.press('Control+Shift+z');
await wait(1000);
pr = await prios();
check(pr.Wall > pr.Slab, `Ctrl+Shift+Z puts it back (${JSON.stringify(pr)})`);

// ---- drag: Slab dragged above Wall -> the slab wins ------------------------------------------------------------
await p.locator('.prio-row[data-tag="Slab"]').dragTo(p.locator('.prio-row[data-tag="Wall"]'));
await wait(1200);
check(JSON.stringify(await rows()) === JSON.stringify(['Slab', 'Wall']), `dragging Slab onto Wall puts it on top (${await rows()})`);
c = await colorAt([2.5, 1, 1.2], 'slab-wins');
check(blue(c), `the overlap is now the slab's blue (${c})`);
check(((await slabTris()) ?? 99) <= 2 + 1e-6, `the slab's cap reaches back to its own edge (x=${await slabTris()})`);

// ---- the keyboard: Alt+ArrowDown on the grip ---------------------------------------------------------------
await p.getByLabel('Move tag Slab: Alt+Up or Alt+Down').focus();
await p.keyboard.press('Alt+ArrowDown');
await wait(1200);
check(JSON.stringify(await rows()) === JSON.stringify(['Wall', 'Slab']), `Alt+Down on the grip moves Slab below Wall (${await rows()})`);
c = await colorAt([2.5, 1, 1.2], 'keyboard');
check(red(c), `and the wall owns the overlap again (${c})`);

// ---- the number field ------------------------------------------------------------------------------------
const num = p.getByLabel('Intersection priority of tag Slab');
await num.fill('500');
await num.press('Enter');
await wait(1200);
pr = await prios();
check(pr.Slab === 500, `typing 500 sets it (${JSON.stringify(pr)})`);
check(JSON.stringify(await rows()) === JSON.stringify(['Slab', 'Wall']), `and the list reorders (${await rows()})`);
c = await colorAt([2.5, 1, 1.2], 'typed');
check(blue(c), `the slab owns the overlap (${c})`);
await num.fill('1500');
await num.press('Enter');
await wait(500);
check(/0 to 999/.test((await p.locator('.prio-editor [role="status"]').textContent()) ?? '') && (await prios()).Slab === 500, 'a number out of range is refused, with a reason, and nothing changes');

// ---- the file keeps it -------------------------------------------------------------------------------
await wait(1500);
const onDisk = JSON.parse(fs.readFileSync(process.env.SKETCHCRAFT_DOC, 'utf8'));
const ts = onDisk.drafting?.tag_styles ?? {};
check(onDisk.format === 2 && ts.Slab?.cut?.priority === 500 && ts.Wall?.cut?.priority === 10, `the saved file has both priorities, format 2 (${JSON.stringify(ts.Slab?.cut)})`);

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
