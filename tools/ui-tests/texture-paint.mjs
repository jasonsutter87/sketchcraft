#!/usr/bin/env node
// Texture Paint in a real browser: place the crate, explode it into a group, select it, pick Tools > Texture Paint, make
// a paint texture (the panel says the faces need unwrapping, one click unwraps and creates), drag a stroke across the model
// with the real pointer and check through the bridge (`texture_pixels`) that the picture changed; one Undo puts the pixels
// back exactly and Redo returns the stroke; then paint a stroke in the UV editor's Paint mode, undo it too. One visual
// reference of the panel (macOS only: Linux references are made on Linux, see visual.mjs).
//   node tools/ui-tests/run.mjs texture-paint
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
let step = '';
const check = (ok, what) => {
  step = what;
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
// React's warnings (e.g. "Maximum update depth exceeded") arrive as console errors: they fail the run like a page error
p.on('console', (m) => m.type() === 'error' && !/Failed to load resource/.test(m.text()) && problems.push(`[after: ${step}] console error: ${m.text().slice(0, 140)}`));
const X = (c) => pageExec(p, c);
const until = async (pred, ms = 8000) => {
  let s = await pageScene(p);
  for (let t = 0; t < ms && !(await pred(s)); t += 150) {
    await wait(150);
    s = await pageScene(p);
  }
  return s;
};
const panel = p.locator('[data-testid="texture-paint-panel"]');
const menu = async (top, item) => {
  await p.getByRole('menuitem', { name: top, exact: true }).click();
  await p.getByRole('menuitem', { name: item }).click();
};
/** a hash of the whole picture, the size, and a few pixels, from the engine */
const pic = async (material, x = 0, y = 0, w = 1, h = 1) => X({ cmd: 'texture_pixels', material, x, y, w, h });
const bounds = async () => {
  const pos = (await pageScene(p)).positions;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < pos.length; i += 3) for (let k = 0; k < 3; k++) (lo[k] = Math.min(lo[k], pos[i + k]), (hi[k] = Math.max(hi[k], pos[i + k])));
  return { mid: lo.map((v, k) => (v + hi[k]) / 2), size: Math.hypot(...hi.map((v, k) => v - lo[k])) };
};
const closeUp = async () => {
  const { mid, size } = await bounds();
  await p.evaluate(([m, d]) => {
    const T = window.__sketchcraftView.current;
    T.controls.target.set(...m);
    T.persp.position.set(m[0], m[1] - d, m[2] + d * 0.1);
    T.persp.up.set(0, 0, 1);
    T.persp.lookAt(...m);
    T.controls.update();
  }, [mid, size * 1.1]);
  await wait(500);
};
const centre = async () => {
  const { mid } = await bounds();
  return p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, mid);
};
const stroke = async (opts = {}) => {
  const [cx, cy] = await centre();
  if (opts.alt) await p.keyboard.down('Alt');
  await p.mouse.move(cx - 60, cy + 6);
  await p.mouse.down();
  await p.mouse.move(cx, cy - 6, { steps: 12 });
  await p.mouse.move(cx + 60, cy + 6, { steps: 12 });
  await p.mouse.up();
  if (opts.alt) await p.keyboard.up('Alt');
  await wait(900);
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(800);
await X({ cmd: 'new' });
const ins = await X({ cmd: 'insert', asset: 'crate', at: [0, 0, 0] });
check(ins.ok === true, 'the crate from the Texturing test set is placed');
const ex = await X({ cmd: 'explode', id: ins.instance });
check(ex.ok === true, 'and exploded into a group (its faces are the model\'s own)');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);
await closeUp();

// ---- select it, pick the tool -------------------------------------------------------------------------------------------
await p.locator('nav.toolbar button[aria-label="Select"]').first().click();
await p.locator('.viewport canvas').click({ position: { x: 700, y: 450 } });
await p.keyboard.press('ControlOrMeta+a');
await wait(300);
await menu('Tools', /Texture Paint/);
await panel.waitFor({ timeout: 5000 });
check(await panel.isVisible(), 'Tools > Texture Paint opens the panel');
check(((await p.locator('.coach').innerText().catch(() => '')) || '').includes('paint texture'), 'the step hint explains the tool (drag to paint, make a paint texture first)');
check(await p.getByRole('menuitem', { name: 'Tools', exact: true }).count() === 1, 'the Tools menu is there');

// ---- New paint texture: the faces have no UVs, so the panel says so and offers Unwrap -----------------------------------------
await panel.getByTestId('tp-create').click();
await panel.getByTestId('tp-needs').waitFor({ timeout: 5000 });
check(((await panel.getByTestId('tp-needs').innerText()) || '').includes('no UVs'), 'the panel says the faces have no UVs and what unwrapping does');
check((await (await pageScene(p)).materials.filter((m) => m.name.startsWith('Paint')).length) === 0, 'nothing was made yet');
await panel.getByTestId('tp-newbase').selectOption('material');
// the message is a toast that fades: record every toast the page shows, so a slow machine can't miss it
await p.evaluate(() => {
  window.__toasts = [];
  new MutationObserver(() => document.querySelectorAll('.toast').forEach((t) => window.__toasts.push(t.textContent ?? ''))).observe(document.body, { childList: true, subtree: true, characterData: true });
});
await panel.getByTestId('tp-unwrap-new').click();
const sc = await until((st) => st.materials.some((m) => m.name === 'Paint 1' && m.texture));
const mat = sc.materials.find((m) => m.name === 'Paint 1');
check(!!mat?.texture, 'Unwrap and create makes the material "Paint 1" with an embedded picture');
const st0 = await pic('Paint 1');
check(st0.ok && st0.width === 1024 && st0.height === 1024, `the picture is 1024 x 1024 (${st0.width} x ${st0.height})`);
const onCrate = sc.faces.filter((f) => f.material === 'Paint 1').length;
check(onCrate === 204, `all 204 faces of the crate wear it (${onCrate})`);
await p.waitForFunction(() => window.__toasts.some((t) => t.includes('Paint 1')), null, { timeout: 8000 }).catch(() => {});
check((await p.evaluate(() => window.__toasts)).some((t) => t.includes('Paint 1')), 'a message says what was made');

// ---- paint in the 3D view ----------------------------------------------------------------------------------------------------
await p.locator('.viewport canvas').hover();
const hash0 = st0.hash;
await stroke();
const afterStroke = await pic('Paint 1');
check(afterStroke.hash !== hash0, 'a drag across the model changed the picture');
const dataAfter = (await pageScene(p)).materials.find((m) => m.name === 'Paint 1').texture.data;
await p.screenshot({ path: path.join(shots, 'texture-paint-3d.png') });
await p.keyboard.press('Control+z');
await wait(700);
check((await pic('Paint 1')).hash === hash0, 'one Undo puts every pixel back exactly');
await p.keyboard.press('Control+Shift+z');
await wait(700);
check((await pic('Paint 1')).hash === afterStroke.hash, 'and Redo brings the stroke back');
check((await pageScene(p)).materials.find((m) => m.name === 'Paint 1').texture.data === dataAfter, 'the same picture, byte for byte');

// ---- Alt erases back to the base colour, [ and ] change the brush ---------------------------------------------------------------
await stroke({ alt: true });
const erased = await pic('Paint 1');
check(erased.hash !== afterStroke.hash, 'Alt-drag erases toward the base colour');
await p.keyboard.press('Control+z');
await wait(600);
const size = async () => (await panel.getByText(/^Size/).first().innerText()).replace(/\s+/g, ' ');
const s0 = await size();
await p.keyboard.press(']');
await wait(200);
const s1 = await size();
check(s0 !== s1, `] makes the brush bigger (${s0} -> ${s1})`);
await p.keyboard.press('[');
await p.keyboard.press('[');
await wait(200);
check((await size()) !== s1, '[ makes it smaller');

// ---- the UV editor paints the flat picture ---------------------------------------------------------------------------------------
await panel.getByTestId('tp-uv').click();
const uv = p.locator('[data-testid="uv-panel"]');
await uv.waitFor({ timeout: 5000 });
await wait(800);
await uv.locator('[data-testid="uv-mode-paint"]').click();
await wait(1200);
check(((await uv.locator('[data-testid="uv-step"]').innerText()) || '').includes('paint'), 'the UV editor\'s Paint mode explains itself');
const preUv = await pic('Paint 1');
const stage = await uv.locator('[data-testid="uv-stage"]').boundingBox();
const cx = stage.x + stage.width / 2;
const cy = stage.y + stage.height / 2;
await p.mouse.move(cx - 50, cy);
await p.mouse.down();
await p.mouse.move(cx, cy + 10, { steps: 10 });
await p.mouse.move(cx + 50, cy, { steps: 10 });
await p.mouse.up();
await wait(1200);
const postUv = await pic('Paint 1');
check(postUv.hash !== preUv.hash, 'a drag in the UV editor changed the picture');
const mid = await pic('Paint 1', 512 - 32, 512 - 16, 64, 32);
check(mid.pixels.flat().some((c) => c[0] !== 255 || c[1] !== 255 || c[2] !== 255), 'and the pixels in the middle of the picture are painted');
await p.keyboard.press('Control+z');
await wait(800);
check((await pic('Paint 1')).hash === preUv.hash, 'one Undo takes the stroke back exactly');
await uv.locator('[data-testid="uv-close"]').click();

// ---- the visual reference: the panel itself (macOS only) -----------------------------------------------------------------------------
await wait(300);
const png = await panel.screenshot({ animations: 'disabled', caret: 'hide' });
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const ref = path.join(refDir, 'texture-paint-panel.png');
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
    fs.writeFileSync(path.join(shots, 'texture-paint-panel.actual.png'), png);
    check(false, `the panel's picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
  } else {
    const pct = (100 * r.bad) / r.total;
    if (pct > 0.5) fs.writeFileSync(path.join(shots, 'texture-paint-panel.actual.png'), png);
    check(pct <= 0.5, `the panel matches its reference picture (${pct.toFixed(3)}% of pixels differ, at most 0.5%)`);
  }
} else {
  fs.writeFileSync(path.join(shots, 'texture-paint-panel.candidate.png'), png);
  console.log(`note: no reference for ${process.platform} (${path.relative(ROOT, ref)}); the picture is saved in ${shots}, nothing compared`);
}

check(problems.length === 0, `no page errors${problems.length ? ': ' + problems.join(' | ') : ''}`);
console.log(`\n${passed} passed, ${failed} failed`);
await b.close();
process.exit(failed ? 1 : 0);
