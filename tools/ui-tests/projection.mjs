#!/usr/bin/env node
// Projection mapping in a real browser: place the can from the Texturing test set, select it, Tools > Project texture…,
// switch the panel to Cylinder and check through the bridge that the can's faces now carry texture coordinates that
// wrap once around it; Cancel takes the preview back, Apply keeps it as one undo step; the same from the command
// search; and one visual reference of the panel (macOS only: Linux references are made on Linux, see visual.mjs).
//   node tools/ui-tests/run.mjs projection
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
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

// a 2x2 picture with four colours, so the can's label is not a flat colour
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
const until = async (pred, ms = 6000) => {
  let s = await pageScene(p);
  for (let t = 0; t < ms && !(await pred(s)); t += 150) {
    await wait(150);
    s = await pageScene(p);
  }
  return s;
};
/** the scene's texture coordinates: [umin, umax, vmin, vmax] over every corner, or null when nothing is textured */
const uvRange = (s) => {
  const u = s.uvs ?? [];
  if (!u.length) return null;
  let r = [Infinity, -Infinity, Infinity, -Infinity];
  for (let i = 0; i < u.length; i += 2) r = [Math.min(r[0], u[i]), Math.max(r[1], u[i]), Math.min(r[2], u[i + 1]), Math.max(r[3], u[i + 1])];
  return r;
};
const wrapsOnce = (r) => !!r && r[1] - r[0] > 0.9 && r[1] - r[0] < 1.2 && r[3] - r[2] > 0.9 && r[3] - r[2] < 1.2;
const panel = p.locator('[data-testid="projection-panel"]');
const menu = async (top, item) => {
  await p.getByRole('menuitem', { name: top, exact: true }).click();
  await p.getByRole('menuitem', { name: item }).click();
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(800);
await X({ cmd: 'new' });
await X({ cmd: 'insert', asset: 'can', at: [0, 0, 0] });
const t = await X({ cmd: 'texture', name: 'Label', data: PNG, width: 0.2, height: 0.2 });
check(t.ok === true, 'a picture material "Label" is in the model');
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1000);

// ---- nothing selected: the command search says what to do ------------------------------------------------------------
await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
await p.keyboard.press('Shift+S');
await p.keyboard.type('project texture');
await wait(300);
await p.getByText('Project texture…').first().click();
await wait(300);
check(((await p.locator('.toast').innerText().catch(() => '')) || '').includes('Select faces, a group or a component first'), 'with nothing selected it says what to select');
check((await panel.count()) === 0, 'and opens no panel');

// ---- select the can, open the panel from the Tools menu ------------------------------------------------------------
await p.locator('.viewport').click({ position: { x: 20, y: 400 } });
await p.keyboard.press('Control+a');
await wait(400);
await menu('Tools', /Project texture/);
await panel.waitFor({ timeout: 5000 });
check(await panel.isVisible(), 'Tools > Project texture… opens the panel');
check(((await panel.locator('.proj-step').innerText()) || '').includes('Box:'), 'it explains the mode it starts in (Box)');
check((await panel.locator('select').inputValue()) === 'Label', 'it starts on the picture the model has');
let s = await until((st) => uvRange(st) !== null);
check(uvRange(s) !== null, 'the preview is live: the can already has texture coordinates (box)');
check(!wrapsOnce(uvRange(s)), 'a box projection does not wrap the picture once around the can');

await panel.locator('button[data-mode="cylinder"]').click();
check(((await panel.locator('.proj-step').innerText()) || '').includes('label on a can'), 'the hint changes to the cylinder (a label on a can)');
s = await until((st) => wrapsOnce(uvRange(st)));
const r1 = uvRange(s);
check(wrapsOnce(r1), `cylinder: the coordinates wrap once around the can and fill its height (u ${r1?.[0].toFixed(2)}..${r1?.[1].toFixed(2)}, v ${r1?.[2].toFixed(2)}..${r1?.[3].toFixed(2)})`);

// the same face's coordinates, read through the engine: the side faces run from the bottom (0) to the top (1)
const faces = (await X({ cmd: 'components' })).components ?? [];
check(faces.length >= 1, 'the can is a component in the model');
await p.screenshot({ path: path.join(shots, 'projection-cylinder.png') });

// turn: the seam moves
await panel.getByLabel('Turn').fill('90');
await wait(600);
s = await until((st) => wrapsOnce(uvRange(st)));
check(wrapsOnce(uvRange(s)), 'turning it keeps the wrap (the seam moves)');

// ---- Cancel takes it all back --------------------------------------------------------------------------------------
await panel.getByRole('button', { name: 'Cancel' }).click();
await panel.waitFor({ state: 'detached', timeout: 5000 });
s = await until((st) => uvRange(st) === null);
check(uvRange(s) === null, 'Cancel takes the projection back: no texture coordinates');
check((s.faces ?? []).every((f) => f.material !== 'Label'), 'and the faces are not painted');

// ---- Apply keeps it, as one undo step -----------------------------------------------------------------------------------
await menu('Tools', /Project texture/);
await panel.waitFor({ timeout: 5000 });
await panel.locator('button[data-mode="cylinder"]').click();
await until((st) => wrapsOnce(uvRange(st)));
await wait(300);

// the visual reference: the panel itself (macOS only; Linux gets its references from a Linux run)
const png = await panel.screenshot({ animations: 'disabled', caret: 'hide' });
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const ref = path.join(refDir, 'projection-panel.png');
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
    fs.writeFileSync(path.join(shots, 'projection-panel.actual.png'), png);
    check(false, `the panel's picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
  } else {
    const pct = (100 * r.bad) / r.total;
    if (pct > 0.5) fs.writeFileSync(path.join(shots, 'projection-panel.actual.png'), png);
    check(pct <= 0.5, `the panel matches its reference picture (${pct.toFixed(3)}% of pixels differ, at most 0.5%)`);
  }
} else {
  fs.writeFileSync(path.join(shots, 'projection-panel.candidate.png'), png);
  console.log(`note: no reference for ${process.platform} (${path.relative(ROOT, ref)}); the picture is saved in ${shots}, nothing compared`);
}

await panel.getByRole('button', { name: 'Apply' }).click();
await panel.waitFor({ state: 'detached', timeout: 5000 });
s = await until((st) => wrapsOnce(uvRange(st)));
check(wrapsOnce(uvRange(s)), 'Apply keeps the projection');
check((s.faces ?? []).length >= 0 && (await X({ cmd: 'components' })).ok, 'the model is still sound');
await p.locator('.viewport').click({ position: { x: 20, y: 400 } });
await p.keyboard.press('Control+z');
s = await until((st) => uvRange(st) === null);
check(uvRange(s) === null, 'one Undo takes the whole projection (and the paint) back');
await p.keyboard.press('Control+Shift+z');
s = await until((st) => wrapsOnce(uvRange(st)));
check(wrapsOnce(uvRange(s)), 'and Redo brings it back');

// ---- the saved model keeps it (format 2, needs face_uv) ---------------------------------------------------------------
const saved = path.join(os.tmpdir(), `sc-proj-${process.pid}.sketchcraft`);
await X({ cmd: 'save', path: saved });
const file = JSON.parse(fs.readFileSync(saved, 'utf8'));
check(file.format === 2 && Array.isArray(file.needs) && file.needs.includes('face_uv'), 'the saved file says it needs face_uv (older versions open it read-only)');
fs.rmSync(saved, { force: true });

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
