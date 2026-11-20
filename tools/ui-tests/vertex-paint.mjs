#!/usr/bin/env node
// Vertex Paint in a real browser: place the globe from the Texturing test set, pick Tools > Vertex Paint, drag a stroke
// across it with the real pointer, and check through the bridge (`vertex_colors`) that some corners changed and only
// some; one Undo takes the whole stroke back and Redo returns it; Alt-drag brushes the colour away; [ and ] change the
// brush; View > Vertex colours toggles the display; and one visual reference of the panel (macOS only: Linux references
// are made on Linux, see visual.mjs).
//   node tools/ui-tests/run.mjs vertex-paint
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';
import { ROOT } from '../demos/lib.mjs';

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
const until = async (pred, ms = 6000) => {
  let s = await pageScene(p);
  for (let t = 0; t < ms && !(await pred(s)); t += 150) {
    await wait(150);
    s = await pageScene(p);
  }
  return s;
};
const panel = p.locator('[data-testid="vertex-paint-panel"]');
const menu = async (top, item) => {
  await p.getByRole('menuitem', { name: top, exact: true }).click();
  await p.getByRole('menuitem', { name: item }).click();
};
/** every corner's colour, via the engine: the faces of the placed globe */
const corners = async (inst) => {
  const r = await X({ cmd: 'vertex_colors', faces: [inst] });
  const out = [];
  for (const rgba of Object.values(r.colors ?? {})) for (const c of rgba ?? []) out.push(c);
  return out;
};
const total = async (inst) => {
  const r = await X({ cmd: 'vertex_colors', faces: [inst] });
  return Object.values(r.colors ?? {}).reduce((n, v) => n + (v ? v.length : 0), 0);
};
const painted = (cs) => cs.filter((c) => c.some((x) => x < 0.999));
/** a drag with the real pointer across the middle of the model */
const stroke = async (opts = {}) => {
  const r = await p.locator('.viewport canvas').boundingBox();
  const [cx, cy] = [r.x + r.width / 2, r.y + r.height / 2];
  if (opts.alt) await p.keyboard.down('Alt');
  await p.mouse.move(cx - 70, cy + 4);
  await p.mouse.down();
  await p.mouse.move(cx, cy - 4, { steps: 10 });
  await p.mouse.move(cx + 70, cy + 4, { steps: 10 });
  await p.mouse.up();
  if (opts.alt) await p.keyboard.up('Alt');
  await wait(400);
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(800);
await X({ cmd: 'new' });
const ins = await X({ cmd: 'insert', asset: 'globe', at: [0, 0, 0] });
check(ins.ok === true, 'the globe from the Texturing test set is in the model');
const inst = ins.instance;
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);
await p.locator('.viewport').click({ position: { x: 20, y: 400 } });
await p.keyboard.press('Shift+Z');
await wait(900);

// ---- pick the tool from the Tools menu ---------------------------------------------------------------------------------
await menu('Tools', /Vertex Paint/);
await panel.waitFor({ timeout: 5000 });
check(await panel.isVisible(), 'Tools > Vertex Paint opens the brush panel');
check(((await p.locator('.coach').innerText().catch(() => '')) || '').includes('brush the colour'), 'the step hint explains the tool (drag, brush the colour on)');
check((await total(inst)) > 0 && (await corners(inst)).length > 0 && painted(await corners(inst)).length === 0, 'nothing is painted yet');

// ---- drag a stroke across it ------------------------------------------------------------------------------------------
await p.locator('.viewport canvas').hover();
await stroke();
let cs = await corners(inst);
const n = await total(inst);
let hit = painted(cs).length;
check(hit > 0, `the stroke painted ${hit} corners (of ${n})`);
check(hit < n, 'and only some of them: the brush is local');
let s = await pageScene(p);
check((s.vcolors ?? []).length === (s.positions ?? []).length && (s.vcolors ?? []).some((x) => x < 0.999), 'the viewport scene carries the tint per vertex');
await p.screenshot({ path: path.join(shots, 'vertex-paint-stroke.png') });

// ---- one undo takes the whole stroke back --------------------------------------------------------------------------------
await p.keyboard.press('Control+z');
await wait(500);
check(painted(await corners(inst)).length === 0, 'one Undo takes the whole stroke back');
await p.keyboard.press('Control+Shift+z');
await wait(500);
check(painted(await corners(inst)).length === hit, 'and Redo brings it back');

// ---- Alt brushes it away -------------------------------------------------------------------------------------------------
const sum = (cs2) => cs2.reduce((a, c) => a + (3 - c[0] - c[1] - c[2]), 0);
const before = sum(await corners(inst));
await stroke({ alt: true });
const after = sum(await corners(inst));
check(after < before, `Alt-drag brushes the colour away (${before.toFixed(2)} -> ${after.toFixed(2)})`);
await p.keyboard.press('Control+z');
await wait(400);
check(Math.abs(sum(await corners(inst)) - before) < 1e-3, 'and is one undo step too');

// ---- [ and ] change the brush ------------------------------------------------------------------------------------------------
const size = async () => (await panel.getByText(/^Size/).first().innerText()).replace(/\s+/g, ' ');
const s0 = await size();
await p.keyboard.press(']');
await wait(200);
const s1 = await size();
check(s0 !== s1, `] makes the brush bigger (${s0} -> ${s1})`);
await p.keyboard.press('[');
await p.keyboard.press('[');
await wait(200);
const s2 = await size();
check(s2 !== s1 && s2 !== s0, `[ makes it smaller (${s2})`);

// ---- Fill and Clear for the selection ------------------------------------------------------------------------------------------
await p.keyboard.press('Escape');
await p.locator('.viewport').click({ position: { x: 20, y: 400 } });
await p.keyboard.press('Control+a');
await wait(300);
await panel.getByTestId('vp-fill').click();
await wait(500);
check(painted(await corners(inst)).length === n, 'Fill paints every corner of the selection');
await panel.getByTestId('vp-clear').click();
await wait(500);
check(painted(await corners(inst)).length === 0, 'Clear takes the colours off again');

// ---- the display toggle ---------------------------------------------------------------------------------------------------------
await panel.getByTestId('vp-fill').click();
await wait(400);
await p.getByRole('menuitem', { name: 'View', exact: true }).click();
const item = p.getByRole('menuitemcheckbox', { name: /Vertex colours/ });
check((await item.getAttribute('aria-checked')) === 'true', 'View > Vertex colours is on by default');
await item.click();
await wait(300);
await p.getByRole('menuitem', { name: 'View', exact: true }).click();
check((await p.getByRole('menuitemcheckbox', { name: /Vertex colours/ }).getAttribute('aria-checked')) === 'false', 'and one click turns it off');
await p.keyboard.press('Escape');
await menu('View', /Vertex colours/);

// the visual reference: the panel itself (macOS only; Linux gets its references from a Linux run)
await wait(300);
const png = await panel.screenshot({ animations: 'disabled', caret: 'hide' });
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const ref = path.join(refDir, 'vertex-paint-panel.png');
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
    fs.writeFileSync(path.join(shots, 'vertex-paint-panel.actual.png'), png);
    check(false, `the panel's picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
  } else {
    const pct = (100 * r.bad) / r.total;
    if (pct > 0.5) fs.writeFileSync(path.join(shots, 'vertex-paint-panel.actual.png'), png);
    check(pct <= 0.5, `the panel matches its reference picture (${pct.toFixed(3)}% of pixels differ, at most 0.5%)`);
  }
} else {
  fs.writeFileSync(path.join(shots, 'vertex-paint-panel.candidate.png'), png);
  console.log(`note: no reference for ${process.platform} (${path.relative(ROOT, ref)}); the picture is saved in ${shots}, nothing compared`);
}

// ---- the saved model keeps it (format 2, needs face_vcol) ------------------------------------------------------------------------
const saved = path.join(os.tmpdir(), `sc-vcol-${process.pid}.sketchcraft`);
await X({ cmd: 'save', path: saved });
const file = JSON.parse(fs.readFileSync(saved, 'utf8'));
check(file.format === 2 && Array.isArray(file.needs) && file.needs.includes('face_vcol'), 'the saved file says it needs face_vcol (older versions open it read-only)');
fs.rmSync(saved, { force: true });

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
