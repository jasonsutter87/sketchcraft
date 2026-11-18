#!/usr/bin/env node
// Visual regression: screenshots of the main screens and dialogs, compared with reference images.
//   node tools/ui-tests/run.mjs visual                  compare (starts a scratch bridge and dev page)
//   node tools/ui-tests/run.mjs visual -- --update      rewrite the references on THIS platform (then look at them, commit them)
//   UPDATE_VISUAL=1 node tools/ui-tests/run.mjs visual  the same
//   ONLY=layout-side node tools/ui-tests/run.mjs visual only the shots whose name contains this
//
// What is compared. The 3D view is a WebGL canvas, which draws a little differently on every machine, so it is
// hidden (a flat grey) and everything around it is the page: the tool bars, panels, menus and dialogs, in the
// three layouts (Classic, Floating, Vertical) and the light, dark and high-contrast themes. The page's fonts are
// the bundled ones in every shot, and animations, carets and anything that changes with the clock are switched
// off or covered, so the same build gives the same picture.
//
// Text is drawn a little differently on macOS and on Linux, so each platform has its own references in
// tools/ui-tests/visual-references/<platform>/. A platform with none is not failed: the run says so, writes its pictures
// to $SHOT_DIR/visual-candidates (CI uploads them), and the references for it are made by running the update on that
// platform (or by committing CI's candidates).
// A picture matches when at most 0.3% of its pixels differ by more than a small amount (VISUAL_TOLERANCE, in percent).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, ROOT } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same); tools/ui-tests/run.mjs does.');
  process.exit(2);
}
const UPDATE = process.argv.includes('--update') || !!process.env.UPDATE_VISUAL;
const ONLY = (process.env.ONLY ?? '').split(',').filter(Boolean);
const TOLERANCE = Number(process.env.VISUAL_TOLERANCE ?? 0.3); // percent of pixels
const refDir = path.join(ROOT, 'tools/ui-tests/visual-references', process.platform);
const outDir = path.join(process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-visual-')), 'visual');
fs.mkdirSync(outDir, { recursive: true });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = 0;
let passed = 0;
let wrote = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (ok) passed++;
  else failed++;
};

const b = await launch();
const problems = [];

// ---- comparing pictures (decoded and compared inside the browser: no image library needed) ----------------------------
const cmpPage = await (await b.newContext()).newPage();
await cmpPage.goto('about:blank');
const compare = (a, c) =>
  cmpPage.evaluate(
    async ([a, c]) => {
      const load = (b64) =>
        new Promise((res, rej) => {
          const i = new Image();
          i.onload = () => res(i);
          i.onerror = () => rej(new Error('not a picture'));
          i.src = 'data:image/png;base64,' + b64;
        });
      const [ia, ic] = await Promise.all([load(a), load(c)]);
      if (ia.width !== ic.width || ia.height !== ic.height) return { size: [ia.width, ia.height, ic.width, ic.height] };
      const draw = (i) => {
        const cv = document.createElement('canvas');
        cv.width = i.width;
        cv.height = i.height;
        const g = cv.getContext('2d');
        g.drawImage(i, 0, 0);
        return g.getImageData(0, 0, i.width, i.height);
      };
      const [da, dc] = [draw(ia), draw(ic)];
      const out = document.createElement('canvas');
      out.width = ia.width;
      out.height = ia.height;
      const og = out.getContext('2d');
      const od = og.createImageData(ia.width, ia.height);
      let bad = 0;
      for (let k = 0; k < da.data.length; k += 4) {
        const d = Math.max(Math.abs(da.data[k] - dc.data[k]), Math.abs(da.data[k + 1] - dc.data[k + 1]), Math.abs(da.data[k + 2] - dc.data[k + 2]));
        const diff = d > 24;
        if (diff) bad++;
        od.data[k] = diff ? 255 : dc.data[k] * 0.3 + 178;
        od.data[k + 1] = diff ? 0 : dc.data[k + 1] * 0.3 + 178;
        od.data[k + 2] = diff ? 0 : dc.data[k + 2] * 0.3 + 178;
        od.data[k + 3] = 255;
      }
      og.putImageData(od, 0, 0);
      return { bad, total: da.data.length / 4, diff: out.toDataURL('image/png').split(',')[1] };
    },
    [a.toString('base64'), c.toString('base64')],
  );

// ---- the shots ------------------------------------------------------------------------------------------------------
const VIEWPORT = { width: 1440, height: 900 };
const FIXED_CSS = `
  /* the 3D view is hidden (it is drawn a little differently on every machine); the fonts are the bundled ones */
  .viewport canvas { visibility: hidden !important; }
  .viewport { background: #8a8d94 !important; }
  :root { --font: "Bricolage Grotesque Variable", "Bricolage Grotesque", sans-serif !important; }
  *, *::before, *::after { animation: none !important; transition: none !important; caret-color: transparent !important; scroll-behavior: auto !important; }
  ::-webkit-scrollbar { display: none !important; }
  * { scrollbar-width: none !important; }
`;
/** a new browser page with these settings saved, a small fixed model, and the camera where it starts */
const open = async ({ layout, skin }) => {
  const ctx = await b.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  await ctx.addInitScript(
    ([layout, skin]) => {
      try {
        localStorage.setItem('sketchcraft.layout', layout);
        localStorage.setItem('sketchcraft.skin', skin);
        localStorage.setItem('sketchcraft.workspace', 'model');
        localStorage.setItem('sketchcraft.help', 'off');
        localStorage.setItem('sketchcraft.lessonsDone', '[]');
      } catch {}
    },
    [layout, skin],
  );
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await pageExec(p, { cmd: 'new' });
  await pageExec(p, { cmd: 'units', units: 'm' });
  const f = (await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] })).face;
  await pageExec(p, { cmd: 'pushpull', face: f, distance: 3 });
  await pageExec(p, { cmd: 'rect', from: [9, 0, 0], to: [12, 3, 0] });
  await p.reload();
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await p.addStyleTag({ content: FIXED_CSS });
  await wait(1200);
  await p.mouse.move(2, 2);
  // the camera is not part of the picture (the canvas is hidden), but the axis gizmo is: put it where it starts
  await p.evaluate(() => {
    const T = window.__sketchcraftView?.current;
    if (!T) return;
    T.controls.target.set(6, 2, 1);
    T.persp.position.set(18, -16, 12);
    T.persp.lookAt(6, 2, 1);
    T.controls.update();
  });
  await wait(300);
  return { p, ctx };
};
/** what changes with the clock or is animated: covered with a flat box in the picture */
const masks = (p) => [p.getByText(/^SketchCraft \d+\.\d+/), p.locator('.restore-when'), p.locator('img[src$=".gif"]'), p.locator('video'), p.locator('.toast')];

const LAYOUTS = [['classic', 'Classic layout'], ['floating', 'Floating layout'], ['side', 'Vertical layout']];
const SKINS = [['classic', 'light'], ['midnight', 'dark'], ['contrast', 'high-contrast']];
const shots = [];
for (const [layout] of LAYOUTS) for (const [skin, label] of SKINS) shots.push({ name: `main-${layout}-${label}`, layout, skin, run: async () => {} });
const dialogs = [
  ['file-menu', async (p) => p.locator('.file-btn').click()],
  ['themes', async (p) => p.locator('.skin-btn').click()],
  ['learn', async (p) => p.locator('.learn-btn').click()],
  ['plugins', async (p) => {
    await p.locator('.file-btn').click();
    await p.getByRole('button', { name: /^Plugins/ }).click();
    await p.getByRole('button', { name: /Manage plugins/ }).click();
  }],
  ['restore', async (p) => {
    await p.locator('.file-btn').click();
    await p.getByRole('button', { name: /Restore from backup/ }).click();
    await p.waitForSelector('[role=dialog][aria-label="Restore from backup"]');
  }],
  ['command-palette', async (p) => {
    await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
    await p.keyboard.press('Shift+S');
  }],
  ['shortcuts', async (p) => {
    await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
    await p.keyboard.press('?');
  }],
  ['shortcuts-editor', async (p) => {
    await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
    await p.keyboard.press('?');
    await p.getByRole('button', { name: 'Customize…' }).click();
    await p.waitForSelector('.key-editor');
    // one key changed and a conflict shown: Rectangle takes L from Line
    await p.locator('.key-row[data-cmd="tool:rect"] .key-btn').click();
    await p.keyboard.press('l');
    await p.waitForSelector('.key-notice');
  }],
  ['shortcuts-import', async (p) => {
    await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
    await p.keyboard.press('?');
    await p.getByRole('button', { name: 'Customize…' }).click();
    await p.getByRole('button', { name: 'Import / export' }).click();
    await p.getByLabel('Shortcuts file text').fill('Tools/Push/Pull = X\nDraw/Line = J\nEdit/Undo = Ctrl+Z\nWindow/Model Info = Shift+I\nTools/Dimensions = Banana\n');
    await p.getByRole('button', { name: 'Import', exact: true }).click();
    await p.waitForSelector('.key-report');
  }],
  ['library-tab', async (p) => p.locator('.tabs button', { hasText: 'Library' }).first().click()],
  ['views-tab', async (p) => p.locator('.tabs button', { hasText: 'Views' }).first().click()],
  ['more-tools', async (p) => {
    // a narrow window pushes tools behind "+N more"
    await p.setViewportSize({ width: 900, height: 900 });
    await wait(500);
    await p.locator('nav.toolbar button.more').first().click();
  }],
];
for (const [name, run] of dialogs) shots.push({ name: `classic-light-${name}`, layout: 'classic', skin: 'classic', run });
for (const [name, run] of dialogs.filter(([n]) => ['file-menu', 'themes', 'command-palette', 'plugins', 'shortcuts-editor'].includes(n))) shots.push({ name: `floating-dark-${name}`, layout: 'floating', skin: 'midnight', run });

// ---- run them ------------------------------------------------------------------------------------------------------------
const haveRefs = fs.existsSync(refDir) && fs.readdirSync(refDir).some((f) => f.endsWith('.png'));
if (!UPDATE && !haveRefs) console.log(`note: no references for ${process.platform} yet (${path.relative(ROOT, refDir)}): the pictures are only saved, nothing is compared`);
fs.mkdirSync(UPDATE ? refDir : path.join(outDir, 'candidates'), { recursive: true });
let compared = 0;
for (const sh of shots) {
  if (ONLY.length && !ONLY.some((o) => sh.name.includes(o))) continue;
  const { p, ctx } = await open(sh);
  let png;
  try {
    await sh.run(p);
    await wait(400);
    png = await p.screenshot({ animations: 'disabled', caret: 'hide', mask: masks(p), maskColor: '#ff00ff', timeout: 15000 });
  } catch (e) {
    check(false, `${sh.name}: could not be shown (${String(e.message ?? e).split('\n')[0]})`);
    await ctx.close();
    continue;
  }
  await ctx.close();
  const ref = path.join(refDir, `${sh.name}.png`);
  if (UPDATE) {
    fs.writeFileSync(ref, png);
    wrote++;
    console.log(`wrote ${path.relative(ROOT, ref)}`);
    continue;
  }
  if (!haveRefs) {
    fs.writeFileSync(path.join(outDir, 'candidates', `${sh.name}.png`), png);
    continue;
  }
  if (!fs.existsSync(ref)) {
    fs.writeFileSync(path.join(outDir, 'candidates', `${sh.name}.png`), png);
    check(false, `${sh.name}: no reference picture (run the update for ${process.platform}, look at it, commit it)`);
    continue;
  }
  const r = await compare(fs.readFileSync(ref), png);
  compared++;
  if (r.size) {
    fs.writeFileSync(path.join(outDir, `${sh.name}.actual.png`), png);
    check(false, `${sh.name}: the picture is ${r.size[2]}x${r.size[3]}, the reference ${r.size[0]}x${r.size[1]}`);
    continue;
  }
  const pct = (100 * r.bad) / r.total;
  const ok = pct <= TOLERANCE;
  if (!ok) {
    fs.writeFileSync(path.join(outDir, `${sh.name}.actual.png`), png);
    fs.writeFileSync(path.join(outDir, `${sh.name}.diff.png`), Buffer.from(r.diff, 'base64'));
  }
  check(ok, `${sh.name}: ${pct.toFixed(3)}% of pixels differ (at most ${TOLERANCE}%)${ok ? '' : `; see ${path.join(outDir, `${sh.name}.diff.png`)}`}`);
}
if (!ONLY.length || UPDATE) check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
if (UPDATE) console.log(`wrote ${wrote} references for ${process.platform}`);
else if (compared === 0) console.log(`nothing compared (no references for ${process.platform}); pictures are in ${path.join(outDir, 'candidates')}`);
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
