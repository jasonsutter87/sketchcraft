#!/usr/bin/env node
// The UV editor in a real browser: place the crate, explode it into a group, select it, open Tools > UV editor, mark
// seams with Auto seams, Unwrap, and see the islands drawn (one path each); click an island (its faces are selected in the
// 3D view), drag it (one Undo puts it back), nudge it with the arrow keys, pin corners and unwrap again
// (the pinned ones stay), make the lightmap UVs, toggle the checker (a mesh in the 3D view), and Reset. One visual
// reference of the panel (macOS only: Linux references are made on Linux, see visual.mjs).
//   node tools/ui-tests/run.mjs uv-editor
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, ROOT } from '../demos/lib.mjs';

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
const shots = process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-shots-'));
fs.mkdirSync(shots, { recursive: true });

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1500, height: 950 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));
const X = (c) => pageExec(p, c);

const panel = p.locator('[data-testid="uv-panel"]');
const islands = p.locator('[data-testid="uv-island"]');
const box = async (loc) => loc.boundingBox();
const attr = (name) => panel.getAttribute(name);
const reload = async () => {
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(1000);
};
const stats = async () => ({ faces: Number(await attr('data-faces')), islands: Number(await attr('data-islands')) });

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(800);
await X({ cmd: 'new' });
const ins = await X({ cmd: 'insert', asset: 'crate', at: [0, 0, 0] });
check(ins.ok === true, 'the crate is placed');
// a component's faces are numbered in its definition; exploded into a group they are the model's own, which the editor reaches
const ex = await X({ cmd: 'explode', id: ins.instance });
check(ex.ok === true, 'the crate is exploded into a group (its faces are the model\'s own)');
await reload();

// ---- select everything, open Tools > UV editor ---------------------------------------------------------------------------
await p.locator('nav.toolbar button[aria-label="Select"]').first().click();
await p.locator('.viewport canvas').click({ position: { x: 700, y: 450 } });
await p.keyboard.press('ControlOrMeta+a');
await wait(300);
await p.getByRole('menuitem', { name: 'Tools', exact: true }).click();
await p.getByRole('menuitem', { name: /UV editor/ }).click();
await panel.waitFor({ timeout: 5000 });
check(await panel.isVisible(), 'Tools > UV editor opens the panel');
check(((await p.locator('[data-testid="uv-step"]').innerText()) || '').includes('no UVs yet'), 'the step hint says the selected faces have no UVs yet and what to do');
let s0 = await stats();
check(s0.faces === 0, `no faces have UVs yet (${s0.faces})`);
check((await p.getByRole('menuitem', { name: 'Window', exact: true }).count()) === 1, 'the Window menu is there too');

// ---- auto seams, then Unwrap -------------------------------------------------------------------------------------------
await p.locator('[data-testid="uv-auto-seams"]').click();
await wait(600);
const seamCount = await p.evaluate(() => window.__sketchcraftView.current.model.children.filter((c) => c.name === 'uv-seams').length);
check(seamCount === 1, 'the marked seams are drawn in the 3D view');
await p.locator('[data-testid="uv-unwrap"]').click();
await p.waitForFunction(() => Number(document.querySelector('[data-testid="uv-panel"]')?.getAttribute('data-faces')) > 0, null, { timeout: 15000 });
await wait(500);
const s1 = await stats();
check(s1.faces === 204, `Unwrap: all 204 faces of the crate have UVs (${s1.faces})`);
check(s1.islands >= 34, `and they fall into islands along the seams (${s1.islands})`);
check((await islands.count()) === s1.islands, 'every island is drawn: one outline each');
check(((await p.locator('.toast').innerText().catch(() => '')) || '').includes('Unwrapped 204 faces'), 'a message says what the unwrap did');

// ---- click an island: its faces are selected in the 3D view ----------------------------------------------------------------
check((await panel.getAttribute('data-sel-faces')) === '0', 'before the click no single faces are selected in 3D');
const first = p.locator('[data-testid="uv-island"][data-island="0"]');
const fb = await box(first);
await p.mouse.click(fb.x + fb.width / 2, fb.y + fb.height / 2);
await wait(500);
check((await p.locator('[data-testid="uv-island"].on').count()) === 1, 'clicking an island selects it (it is highlighted)');
check(Number(await panel.getAttribute('data-sel-faces')) === 1, 'and its face is selected in the 3D view');
check((await stats()).faces === 204, 'the 2D view still shows every face (the selection it made is not a new scope)');

// ---- drag it: one command, so one Undo puts it back ---------------------------------------------------------------------------------
const before = await box(first);
await p.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
await p.mouse.down();
await p.mouse.move(before.x + before.width / 2 + 20, before.y + before.height / 2 + 12, { steps: 4 });
await p.mouse.move(before.x + before.width / 2 + 40, before.y + before.height / 2 + 30, { steps: 4 });
await p.mouse.up();
await wait(700);
const moved = await box(first);
check(Math.abs(moved.x - before.x - 40) < 1.5 && Math.abs(moved.y - before.y - 30) < 1.5, `dragging moved the island by the drag (${(moved.x - before.x).toFixed(1)}, ${(moved.y - before.y).toFixed(1)} px of 40, 30)`);
check(Math.abs(moved.width - before.width) < 0.5 && Math.abs(moved.height - before.height) < 0.5, 'and kept its size');
await p.locator('button.icon-btn[title^="Undo"]').click();
await wait(700);
const undone = await box(first);
check(Math.abs(undone.x - before.x) < 0.5 && Math.abs(undone.y - before.y) < 0.5, 'one Undo puts the island back');
await p.locator('button.icon-btn[title^="Redo"]').click();
await wait(700);
const redone = await box(first);
check(Math.abs(redone.x - moved.x) < 0.5, 'and Redo moves it again');
await p.locator('button.icon-btn[title^="Undo"]').click();
await wait(700);

// ---- the keyboard: arrows nudge, G moves --------------------------------------------------------------------------------
await p.locator('[data-testid="uv-stage"]').focus();
const k0 = await box(first);
await p.keyboard.press('Shift+ArrowRight');
await wait(600);
const k1 = await box(first);
check(Math.abs(k1.x - k0.x - 10) < 1.5, `Shift+Arrow nudges the chosen island 10 px (${(k1.x - k0.x).toFixed(1)})`);
await p.locator('button.icon-btn[title^="Undo"]').click();
await wait(600);

// ---- the checker is a mesh in the 3D view -------------------------------------------------------------------------------
const hasChecker = () => p.evaluate(() => window.__sketchcraftView.current.model.children.some((c) => c.name === 'uv-checker'));
check((await hasChecker()) === false, 'no checker in the 3D view to begin with');
await p.locator('[data-testid="uv-checker"]').check();
await wait(700);
check((await hasChecker()) === true, 'the Checker toggle puts a checker over the unwrapped faces in the 3D view');
await p.locator('[data-testid="uv-checker"]').uncheck();
await wait(500);
check((await hasChecker()) === false, 'and takes it off again');

// ---- pins: choose an island's corners, pin them, unwrap again; they stay where they are ------------------------------------
// click an island, then switch to points (1): its four corners stay chosen
const target = p.locator('[data-testid="uv-island"][data-island="1"]');
const tb = await box(target);
await p.mouse.click(tb.x + tb.width / 2, tb.y + tb.height / 2);
await wait(400);
await p.keyboard.press('1');
await wait(300);
await p.keyboard.press('p');
await wait(300);
const pins = await p.locator('[data-testid="uv-pin"]').count();
check(pins === 4, `P pins the chosen island's four corners (${pins} pins)`);
check(((await p.locator('[data-testid="uv-unwrap"]').innerText()) || '').includes('keep'), 'the Unwrap button says it keeps the pins');
// the engine's own numbers for island 1's face, before and after
const group = ((await X({ cmd: 'info' })).groups ?? []).map((g) => g.id);
const uvs = async () => Object.fromEntries(((await X({ cmd: 'uv_info', faces: group })).faces ?? []).map((f) => [f.id, f.uv]));
const beforeUv = await uvs();
const pinnedFace = Object.keys(beforeUv).map(Number).sort((a, c) => a - c)[1];
await p.locator('[data-testid="uv-unwrap"]').click();
await wait(1500);
const afterUv = await uvs();
const sameUv = beforeUv[pinnedFace].every((q, i) => Math.abs(q[0] - afterUv[pinnedFace][i][0]) < 1e-5 && Math.abs(q[1] - afterUv[pinnedFace][i][1]) < 1e-5);
const others = Object.keys(beforeUv).filter((f) => Number(f) !== pinnedFace && beforeUv[f].some((q, i) => Math.abs(q[0] - afterUv[f][i][0]) > 1e-5 || Math.abs(q[1] - afterUv[f][i][1]) > 1e-5)).length;
check(sameUv, 'after Unwrap the pinned island is exactly where it was');
check(others > 0, `and the other islands were packed around it (${others} moved)`);
check((await stats()).faces === 204, 'and every face still has UVs');

// the visual reference of the panel: macOS only (Linux gets its references from a Linux run)
await p.keyboard.press('Escape');
await p.mouse.click(10, 10);
await wait(300);
const png = await panel.screenshot({ animations: 'disabled', caret: 'hide' });
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const ref = path.join(refDir, 'uv-editor-panel.png');
if (process.env.UPDATE_VISUAL || process.argv.includes('--update')) {
  fs.mkdirSync(refDir, { recursive: true });
  fs.writeFileSync(ref, png);
  console.log(`wrote ${path.relative(ROOT, ref)}`);
} else if (fs.existsSync(ref)) {
  const cmp = await (await b.newContext()).newPage();
  await cmp.goto('about:blank');
  const r = await cmp.evaluate(
    async ([a, c]) => {
      const load = (b64) => new Promise((res, rej) => ((i) => ((i.onload = () => res(i)), (i.onerror = rej), (i.src = 'data:image/png;base64,' + b64)))(new Image()));
      const [ia, ic] = await Promise.all([load(a), load(c)]);
      if (ia.width !== ic.width || ia.height !== ic.height) return { size: [ia.width, ia.height, ic.width, ic.height] };
      const px = (i) => {
        const cv = document.createElement('canvas');
        cv.width = i.width;
        cv.height = i.height;
        const g = cv.getContext('2d');
        g.drawImage(i, 0, 0);
        return g.getImageData(0, 0, i.width, i.height).data;
      };
      const [da, dc] = [px(ia), px(ic)];
      let bad = 0;
      for (let k = 0; k < da.length; k += 4) if (Math.max(Math.abs(da[k] - dc[k]), Math.abs(da[k + 1] - dc[k + 1]), Math.abs(da[k + 2] - dc[k + 2])) > 24) bad++;
      return { bad, total: da.length / 4 };
    },
    [fs.readFileSync(ref).toString('base64'), png.toString('base64')],
  );
  if (r.size) {
    fs.writeFileSync(path.join(shots, 'uv-editor-panel.actual.png'), png);
    check(false, `the panel's picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
  } else {
    const pct = (100 * r.bad) / r.total;
    if (pct > 0.5) fs.writeFileSync(path.join(shots, 'uv-editor-panel.actual.png'), png);
    check(pct <= 0.5, `the panel matches its reference picture (${pct.toFixed(3)}% of pixels differ, at most 0.5%)`);
  }
} else {
  fs.writeFileSync(path.join(shots, 'uv-editor-panel.candidate.png'), png);
  console.log(`note: no reference for ${process.platform} (${path.relative(ROOT, ref)}); the picture is saved in ${shots}, nothing compared`);
}

// ---- the lightmap channel -----------------------------------------------------------------------------------------------
await p.locator('[data-testid="uv-lightmap"]').click();
await wait(1500);
check((await attr('data-set')) === '2', 'Lightmap UVs makes the second set and shows it');
const lm = await stats();
check(lm.faces === 204 && lm.islands > 0, `the lightmap set holds every face (${lm.faces} faces, ${lm.islands} islands)`);
await p.locator('[data-testid="uv-set-1"]').click();
await wait(500);
check((await attr('data-set')) === '1' && (await stats()).faces === 204, 'the Picture set is still there, unchanged');

// ---- reset --------------------------------------------------------------------------------------------------------------
await p.locator('[data-testid="uv-reset"]').click();
await wait(1000);
check((await stats()).faces === 0, 'Reset takes the picture UVs off');
check(((await p.locator('[data-testid="uv-step"]').innerText()) || '').includes('no UVs yet'), 'and the hint says to unwrap again');

// ---- close ---------------------------------------------------------------------------------------------------------------
await p.locator('[data-testid="uv-close"]').click();
await panel.waitFor({ state: 'detached', timeout: 3000 });
check(true, 'the close button closes it');

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
