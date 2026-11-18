#!/usr/bin/env node
// Cut fill per tag (#5), in a real browser against the dev page and bridge: walls, a slab and a desk on three tags;
// each tag's Cut button in the Tags panel opens an editor with a hint and the four choices (Default, Solid, Hatch,
// None) as swatches; picking Solid + a colour, Hatch, and None reaches the model (the scene and the saved file say
// so), the button's swatch follows, and with a section plane the screen shows three different fills: the slab in
// its colour, the walls hatched over white, the desk open (no cap) but outlined; outline weights reach the screen as
// thick lines. Pixels are checked at each cut's middle. Ctrl+Z / Ctrl+Shift+Z take a change back and forth.
//   SHOT_DIR=/some/folder node tools/ui-tests/cut-fills.mjs
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

// three solid blocks side by side, each a group on its own tag
await pageExec(p, { cmd: 'new' });
const block = async (name, lo, hi) => {
  const g = (await pageExec(p, { cmd: 'begin_group', name })).group;
  const f = (await pageExec(p, { cmd: 'rect', from: [lo[0], lo[1], 0], to: [hi[0], hi[1], 0] })).face;
  await pageExec(p, { cmd: 'pushpull', face: f, distance: 2.5 });
  await pageExec(p, { cmd: 'end_group' });
  await pageExec(p, { cmd: 'tag', ids: [g], tag: name });
  await pageExec(p, { cmd: 'tag_visible', tag: name, visible: true });
};
await block('Walls', [0, 0], [2, 2]);
await block('Slab', [3, 0], [5, 2]);
await block('Furniture', [6, 0], [8, 2]);
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(800);

const scene = () => pageScene(p);
const cutOf = async (t) => (await scene()).tagStyles?.[t]?.cut;
const btn = (t) => p.getByLabel(`Cut fill of tag ${t}`, { exact: true }).first();
const kind = (t, k) => p.locator(`[role="radiogroup"][aria-label="Cut fill kind of tag ${t}"] [data-kind="${k}"]`);

// ---- the buttons and the editor ----------------------------------------------------------------------
for (const t of ['Walls', 'Slab', 'Furniture']) check((await btn(t).count()) === 1, `tag ${t} has a Cut button next to its Hatch button`);
check((await btn('Slab').locator('svg.cut-swatch[data-fill="auto"]').count()) === 1, 'with no cut style its swatch is the default dark poché');
check(((await btn('Slab').getAttribute('title')) ?? '').includes('where a section cuts it'), 'its tooltip says what it does');
await btn('Slab').click();
await wait(400);
const hint = await p.locator('.cut-editor .hint').first().textContent();
check(/section plane or plan cuts Slab/.test(hint ?? ''), `the editor explains itself ("${hint}")`);
const kinds = await p.locator('.cut-editor .cut-kind').allTextContents();
check(['Default', 'Solid', 'Hatch', 'None'].every((k) => kinds.includes(k)), `it offers Default, Solid, Hatch and None (${kinds.join(', ')})`);
check((await p.locator('.cut-editor .cut-kind svg.cut-swatch').count()) === 4, 'each with a swatch of what it looks like');
check((await kind('Slab', 'auto').getAttribute('aria-checked')) === 'true', 'Default is picked');

// Slab: solid, red, 0.7 mm outline
await kind('Slab', 'solid').click();
await wait(700);
check((await cutOf('Slab'))?.fill === 'solid', 'Solid sets the fill');
check((await p.locator('.cut-editor .cut-kind-hint').textContent())?.includes('solid colour'), 'and the hint under the choices says what it is');
await p.getByLabel('Cut fill colour of tag Slab').fill('#ff0000');
await wait(700);
await p.getByLabel('Cut outline weight of tag Slab').selectOption('0.7');
await wait(700);
let c = await cutOf('Slab');
check(c?.fill === 'solid' && c?.color === '#ff0000' && c?.outline_mm === 0.7, `colour and outline reach the model (${JSON.stringify(c)})`);
check((await btn('Slab').locator('svg.cut-swatch[data-fill="solid"] rect').getAttribute('fill')) === '#ff0000', 'the button shows a red swatch');
await btn('Slab').click();
await wait(300);

// Walls: hatch (starts with the diagonal pattern)
await btn('Walls').click();
await wait(400);
await kind('Walls', 'hatch').click();
await wait(800);
c = await cutOf('Walls');
check(c?.fill === 'hatch' && c?.hatch?.pattern === 'diagonal', `Hatch with no hatch yet starts the diagonal one (${JSON.stringify(c)})`);
await btn('Walls').click();
await wait(300);

// Furniture: none, outline 0.5 mm
await btn('Furniture').click();
await wait(400);
await kind('Furniture', 'none').click();
await wait(700);
await p.getByLabel('Cut outline weight of tag Furniture').selectOption('0.5');
await wait(700);
c = await cutOf('Furniture');
check(c?.fill === 'none' && c?.outline_mm === 0.5, `None and an outline (${JSON.stringify(c)})`);
check((await btn('Furniture').locator('svg.cut-swatch[data-fill="none"]').count()) === 1, 'the button shows an empty swatch');
await btn('Furniture').click();
await wait(300);

// ---- on screen: three fills ----------------------------------------------------------------------------
// the plane keeps what is below the cut, seen from above
await pageExec(p, { cmd: 'section_plane', origin: [0, 0, 1.2], normal: [0, 0, -1] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1200);
await p.evaluate(() => {
  const T = window.__sketchcraftView.current;
  T.controls.target.set(4, 1, 0);
  T.persp.position.set(4, 0.999, 22);
  T.persp.up.set(0, 0, 1);
  T.persp.lookAt(4, 1, 0);
  T.controls.update();
});
await wait(800);
const onScreen = () =>
  p.evaluate(() => {
    const out = { solid: {}, outline: {}, hatch: 0, paper: 0, plain: 0 };
    window.__sketchcraftView.current.model.traverse((o) => {
      const u = o.userData ?? {};
      if (u.sectionSolid) out.solid[u.sectionSolid] = '#' + o.material.color.getHexString();
      else if (u.sectionOutline) out.outline[u.sectionOutline] = o.material.linewidth;
      else if (u.sectionHatch) out.hatch += o.geometry.getAttribute('position').count / 2;
      else if (u.sectionHatchFill) out.paper += o.geometry.getAttribute('position').count / 3;
      else if (u.sectionFill) out.plain += o.geometry.getAttribute('position').count / 3;
    });
    return out;
  });
let sc = await onScreen();
check(sc.solid.Slab === '#ff0000', `the slab's cut is its own red (${JSON.stringify(sc.solid)})`);
check(sc.hatch > 5 && sc.paper > 0, `the walls are hatched over white (${sc.hatch} lines)`);
check(sc.plain === 0, 'nothing is left in the default dark fill');
check(sc.outline.Slab >= 4 && sc.outline.Furniture >= 3 && sc.outline.Slab > sc.outline.Furniture, `outline weights show as thick lines, the 0.7 mm one wider (${JSON.stringify(sc.outline)})`);

// pixels at each cut's middle (the screenshot, read back through a canvas)
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
const shot = await p.screenshot({ path: path.join(shots, 'cut-fills-section.png') });
const at = await Promise.all([[1, 1, 1.2], [4, 1, 1.2], [7, 1, 1.2]].map(px));
const colors = await p.evaluate(
  async ([b64, pts]) => {
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
    // the average over a 15 px square (hatch lines and white mix)
    return pts.map(([x, y]) => {
      const d = g.getImageData(Math.round(x) - 7, Math.round(y) - 7, 15, 15).data;
      const s = [0, 0, 0];
      for (let k = 0; k < d.length; k += 4) for (let j = 0; j < 3; j++) s[j] += d[k + j];
      return s.map((v) => Math.round(v / (d.length / 4)));
    });
  },
  [shot.toString('base64'), at],
);
const [walls, slab, desk] = colors;
const near = (a, b, tol) => a.every((v, i) => Math.abs(v - b[i]) <= tol);
const poche = [0x3a, 0x3d, 0x4a];
check(slab[0] > 200 && slab[1] < 60 && slab[2] < 60, `the slab's pixels are red (${slab})`);
check(walls[0] > 150 && walls[1] > 150 && walls[2] > 150 && !near(walls, [0xf6, 0xf5, 0xf0], 3), `the walls' pixels are white with hatch lines (${walls})`);
check(!near(desk, poche, 20) && !near(desk, slab, 40) && !near(desk, walls, 6), `the desk is open, neither poché, red nor hatched (${desk})`);

// ---- the file keeps it -------------------------------------------------------------------------------
await wait(1500);
const onDisk = JSON.parse(fs.readFileSync(process.env.SKETCHCRAFT_DOC, 'utf8'));
const ts = onDisk.drafting?.tag_styles ?? {};
check(onDisk.format === 2 && ts.Slab?.cut?.fill === 'solid' && ts.Slab?.cut?.color === '#ff0000' && ts.Furniture?.cut?.fill === 'none' && ts.Walls?.cut?.fill === 'hatch', 'the saved file has all three, as format 2');

// ---- undo and redo -----------------------------------------------------------------------------------
// one more change through the panel (the section plane was the last undo step), then back and forth
await btn('Furniture').click();
await wait(400);
await p.getByLabel('Cut outline weight of tag Furniture').selectOption('1');
await wait(900);
check((await cutOf('Furniture'))?.outline_mm === 1, 'a 1 mm outline for the desk');
await btn('Furniture').click();
await wait(300);
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(900);
check((await cutOf('Furniture'))?.outline_mm === 0.5 && (await cutOf('Furniture'))?.fill === 'none', `Ctrl+Z takes it back (${JSON.stringify(await cutOf('Furniture'))})`);
await p.keyboard.press('Control+Shift+z');
await wait(900);
check((await cutOf('Furniture'))?.outline_mm === 1, 'Ctrl+Shift+Z puts it back');

// ---- back to the default -----------------------------------------------------------------------------
await btn('Slab').click();
await wait(400);
await kind('Slab', 'auto').click();
await wait(1200);
sc = await onScreen();
check(!sc.solid.Slab && sc.plain > 0, `Default gives the slab the dark fill again (${JSON.stringify(sc)})`);
await p.screenshot({ path: path.join(shots, 'cut-fills-default.png') });

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
