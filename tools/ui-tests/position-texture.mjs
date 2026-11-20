#!/usr/bin/env node
// Position Texture in a real browser: paint a face with a picture, right-click it > Texture > Position, drag the red pin
// and press Enter: the face's texture coordinates moved (read through `texture_frame`) and one Undo takes it back; Esc
// leaves them unchanged; the right-click menu flips the picture; and one visual reference of the hint bar (macOS only:
// Linux references are made on Linux, see visual.mjs).
//   node tools/ui-tests/run.mjs position-texture
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
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

// a 2x2 picture with four colours, so the picture's lay shows
const png2x2 = () => {
  const chunk = (type, data) => {
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(body) >>> 0);
    return Buffer.concat([head, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(2, 0);
  ihdr.writeUInt32BE(2, 4);
  ihdr.set([8, 2, 0, 0, 0], 8);
  const rows = Buffer.from([0, 220, 40, 40, 40, 160, 60, 0, 50, 70, 200, 240, 200, 40]);
  const file = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(rows)), chunk('IEND', Buffer.alloc(0))]);
  return 'data:image/png;base64,' + file.toString('base64');
};
const PNG = png2x2();

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1500, height: 950 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));
const X = (c) => pageExec(p, c);

const camera = (eye, target) =>
  p.evaluate(
    ([eye, target]) => {
      const T = window.__sketchcraftView.current;
      T.controls.target.set(...target);
      T.persp.position.set(...eye);
      T.persp.up.set(0, 0, 1);
      T.persp.lookAt(...target);
      T.controls.update();
    },
    [eye, target],
  );
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
const centre = async (loc) => {
  const r = await loc.boundingBox();
  return [r.x + r.width / 2, r.y + r.height / 2];
};
/** the face's picture-space corners (metres) as the engine holds them now, and whether it has its own coordinates */
const frame = async (face) => {
  const r = await X({ cmd: 'texture_frame', face });
  return { pic: r.picture?.[0] ?? [], own: r.own };
};
const same = (a, c) => a.length === c.length && a.every((q, i) => Math.abs(q[0] - c[i][0]) < 1e-6 && Math.abs(q[1] - c[i][1]) < 1e-6);
const overlay = p.locator('[data-testid="pos-overlay"]');

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(800);
await X({ cmd: 'new' });
const rect = await X({ cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] });
const face = rect.face;
const t = await X({ cmd: 'texture', name: 'Label', data: PNG, width: 2, height: 2 });
check(t.ok === true, 'a picture material "Label" is in the model');
check((await X({ cmd: 'paint', ids: [face], material: 'Label' })).ok === true, 'the face is painted with it');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);
await camera([2, 1.9995, 60], [2, 2, 0]);
await wait(300);

// ---- a face with no picture: nothing to position ---------------------------------------------------------------------
const bare = await X({ cmd: 'rect', from: [10, 0, 0], to: [12, 2, 0] });
check(bare.ok === true, 'a second face with no picture');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);
await camera([2, 1.9995, 60], [2, 2, 0]);
await wait(300);

await p.locator('nav.toolbar button[aria-label="Select"]').first().click();
const before = await frame(face);
check(before.pic.length === 4 && before.own === false, 'the painted face starts on the material\'s own placement');

// ---- right-click the face > Texture > Position ------------------------------------------------------------------------
const [fx, fy] = await px([2, 2, 0]);
await p.mouse.click(fx, fy);
await wait(300);
await p.mouse.click(fx, fy, { button: 'right' });
await wait(500);
await wait(400);
await p.locator('.menu button', { hasText: 'Texture › Position' }).click();
await overlay.waitFor({ timeout: 5000 });
check(await overlay.isVisible(), 'right-click > Texture > Position opens Position mode');
for (const k of ['red', 'green', 'blue', 'yellow']) check((await p.locator(`[data-testid="pos-pin-${k}"]`).count()) === 1, `the ${k} pin is there`);
check(((await p.locator('.pos-bar').innerText()) || '').includes('Red pin: move'), 'the hint bar explains the pins');
check((await p.locator('.pos-pin title').evaluateAll((e) => e.map((t) => t.textContent))).some((x) => x.includes('Green pin')), 'each pin says what it does');

// the visual reference: the hint bar (macOS only; Linux gets its references from a Linux run)
const png = await p.locator('.pos-bar').screenshot({ animations: 'disabled', caret: 'hide' });
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const ref = path.join(refDir, 'position-texture-bar.png');
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
    fs.writeFileSync(path.join(shots, 'position-texture-bar.actual.png'), png);
    check(false, `the hint bar's picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
  } else {
    const pct = (100 * r.bad) / r.total;
    if (pct > 0.5) fs.writeFileSync(path.join(shots, 'position-texture-bar.actual.png'), png);
    check(pct <= 0.5, `the hint bar matches its reference picture (${pct.toFixed(3)}% of pixels differ, at most 0.5%)`);
  }
} else {
  fs.writeFileSync(path.join(shots, 'position-texture-bar.candidate.png'), png);
  console.log(`note: no reference for ${process.platform} (${path.relative(ROOT, ref)}); the picture is saved in ${shots}, nothing compared`);
}

// ---- drag the red pin, Enter keeps it ----------------------------------------------------------------------------------
const dragRed = async (dx, dy) => {
  const [rx, ry] = await centre(p.locator('[data-testid="pos-pin-red"]'));
  await p.mouse.move(rx, ry);
  await p.mouse.down();
  await p.mouse.move(rx + dx / 2, ry + dy / 2, { steps: 4 });
  await p.mouse.move(rx + dx, ry + dy, { steps: 4 });
  await wait(500);
  await p.mouse.up();
  await wait(400);
};
await dragRed(60, 0);
let mid = await frame(face);
check(mid.own === true && !same(mid.pic, before.pic), 'dragging the red pin moves the picture live (the face holds its own coordinates)');
await p.keyboard.press('Enter');
await overlay.waitFor({ state: 'detached', timeout: 5000 });
const after = await frame(face);
const dxs = after.pic.map((q, i) => q[0] - before.pic[i][0]);
const dys = after.pic.map((q, i) => q[1] - before.pic[i][1]);
// on a face looking up the picture's u runs along -y and its v along +x, so a drag to the right moves v
check(dys.every((d) => Math.abs(d - dys[0]) < 1e-5) && Math.abs(dys[0]) > 0.5, `Enter keeps it: every corner's v shifted by the same ${dys[0].toFixed(2)} m`);
check(dxs.every((d) => Math.abs(d) < 0.15), 'and u did not move (a drag along x)');
check(dys[0] < 0, 'the picture followed the pointer: it moved right, so each corner now reads lower down in it');

// one Undo takes the whole session back
await p.locator('button.icon-btn[title^="Undo"]').click();
await wait(500);
const undone = await frame(face);
check(same(undone.pic, before.pic) && undone.own === false, 'one Undo takes the whole Position session back');
await p.locator('button.icon-btn[title^="Redo"]').click();
await wait(500);
check(same((await frame(face)).pic, after.pic), 'and Redo brings it back');
await p.locator('button.icon-btn[title^="Undo"]').click();
await wait(500);

// ---- Esc leaves it as it was --------------------------------------------------------------------------------------------
await p.mouse.click(fx, fy);
await wait(300);
await p.keyboard.press('Shift+S');
await p.keyboard.type('position texture');
await wait(300);
await p.getByText('Position texture…').first().click();
await overlay.waitFor({ timeout: 5000 });
check(await overlay.isVisible(), 'the command search opens Position mode too');
await dragRed(0, -50);
check(!same((await frame(face)).pic, before.pic), 'a drag moves it live...');
await p.keyboard.press('Escape');
await overlay.waitFor({ state: 'detached', timeout: 5000 });
await wait(400);
const esc = await frame(face);
check(same(esc.pic, before.pic) && esc.own === false, '...and Esc takes it all back: the coordinates are as they were');

// ---- the right-click menu inside Position: Flip left/right ------------------------------------------------------------
await p.mouse.click(fx, fy);
await wait(300);
await p.mouse.click(fx, fy, { button: 'right' });
await p.locator('.menu button', { hasText: 'Texture › Position' }).click();
await overlay.waitFor({ timeout: 5000 });
const [qx, qy] = await centre(p.locator('[data-testid="pos-quad"]'));
await p.mouse.click(qx, qy, { button: 'right' });
await p.locator('.menu button', { hasText: 'Flip left/right' }).click();
await wait(600);
const flipped = await frame(face);
check(flipped.own === true && !same(flipped.pic, before.pic), 'Flip left/right in the menu changes the coordinates');
await p.getByTestId('pos-cancel').click();
await overlay.waitFor({ state: 'detached', timeout: 5000 });
await wait(400);
check(same((await frame(face)).pic, before.pic), 'Cancel takes the flip back');

// ---- a face with no picture: no Texture > Position in its menu, and the Tools menu says what to do --------------------------
const [bx, by] = await px([11, 1, 0]);
await p.mouse.click(bx, by);
await wait(300);
await p.mouse.click(bx, by, { button: 'right' });
await wait(300);
check((await p.locator('.menu button', { hasText: 'Texture › Position' }).count()) === 0, 'a face with no picture has no Texture > Position in its menu');
await p.keyboard.press('Escape');
await p.getByRole('menuitem', { name: 'Tools', exact: true }).click();
await p.getByRole('menuitem', { name: /Position texture/ }).click();
await wait(300);
check(((await p.locator('.toast').innerText().catch(() => '')) || '').includes('no picture yet'), 'Tools > Position texture says to paint it first');
check((await overlay.count()) === 0, 'and opens nothing');

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
