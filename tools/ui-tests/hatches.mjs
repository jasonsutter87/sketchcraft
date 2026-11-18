#!/usr/bin/env node
// Hatch patterns per tag (#4), in a real browser against the dev page and bridge: the Tags panel's hatch button
// opens an editor with a hint and a preview; picking a pattern, colour, weight, scale, angle and scale mode sets the
// tag's hatch in the model (the scene says so, the saved file has it, the swatch shows it); a section plane shows
// the hatch on screen over the cut walls in the hatch colour; a .pat file imports and can be picked; Ctrl+Z and
// Ctrl+Shift+Z take changes back and forth; "None" removes it.
//   SHOT_DIR=/some/folder node tools/ui-tests/hatches.mjs
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

// walls around a room (the cut is a ring with a hole), one group on the "Walls" tag
await pageExec(p, { cmd: 'new' });
const g = (await pageExec(p, { cmd: 'begin_group', name: 'Walls' })).group;
const f = (await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [4, 3, 0] })).face;
const inner = (await pageExec(p, { cmd: 'rect', from: [0.3, 0.3, 0], to: [3.7, 2.7, 0] })).face;
await pageExec(p, { cmd: 'delete', ids: [inner] });
await pageExec(p, { cmd: 'pushpull', face: f, distance: 2.5 });
await pageExec(p, { cmd: 'end_group' });
await pageExec(p, { cmd: 'tag', ids: [g], tag: 'Walls' });
await pageExec(p, { cmd: 'tag_visible', tag: 'Walls', visible: true });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(800);

const scene = () => pageScene(p);
const hatchOf = async () => (await scene()).tagStyles?.Walls?.cut?.hatch;
const button = p.getByLabel('Hatch of tag Walls', { exact: true }).first();
const patternMenu = p.getByLabel('Hatch pattern of tag Walls');

// ---- the button and its editor -----------------------------------------------------------------------
check((await button.count()) === 1, 'each tag has a hatch button in the Tags panel');
check((await button.textContent())?.includes('Hatch'), 'with no hatch it says "Hatch"');
check(((await button.getAttribute('title')) ?? '').includes('cut walls'), 'its tooltip says what a hatch does');
await button.click();
await wait(400);
check((await patternMenu.count()) === 1, 'clicking it opens the editor with a pattern menu');
const hint = await p.locator('.hatch-editor .hint').first().textContent();
check(/cut walls in plans and sections/.test(hint ?? ''), `the editor explains itself ("${hint}")`);
const options = await patternMenu.locator('option').allTextContents();
check(['None (solid fill)', 'diagonal', 'brick', 'concrete', 'insulation'].every((o) => options.includes(o)), `the menu offers None and the built-ins (${options.join(', ')})`);

await patternMenu.selectOption('brick');
await wait(700);
let h = await hatchOf();
check(h?.pattern === 'brick' && (await scene()).tagStyles?.Walls?.cut?.fill === 'hatch', `picking brick sets the tag's hatch (${JSON.stringify(h)})`);
const swatch = button.locator('svg.hatch-swatch[data-pattern="brick"] path');
check((await swatch.count()) === 1 && ((await swatch.getAttribute('d')) ?? '').length > 20, 'the button now shows a brick swatch');
check((await p.locator('.hatch-editor svg.hatch-swatch[data-pattern="brick"] path').count()) === 1, 'and the editor a bigger preview');

// colour, weight, scale, angle, scale mode
await p.getByLabel('Hatch colour of tag Walls').fill('#cc0000');
await wait(600);
await p.getByLabel('Hatch line weight of tag Walls').selectOption('0.35');
await wait(600);
const scaleBox = p.getByLabel('Hatch scale of tag Walls');
await scaleBox.fill('50');
await scaleBox.press('Enter');
await wait(600);
const angleBox = p.getByLabel('Hatch angle of tag Walls');
await angleBox.fill('30');
await angleBox.press('Enter');
await wait(600);
await p.getByLabel('Hatch scale mode of tag Walls').selectOption('model');
await wait(700);
h = await hatchOf();
check(h?.color === '#cc0000' && h?.weight_mm === 0.35 && h?.scale === 50 && h?.angle === 30 && h?.scale_mode === 'model', `colour, weight, scale, angle and scale mode all reach the model (${JSON.stringify(h)})`);
const stroke = await p.locator('.hatch-editor svg.hatch-swatch path').first().getAttribute('stroke');
check(stroke === '#cc0000', `the preview draws in the hatch colour (${stroke})`);

// a bad number is refused in the box, not sent
await scaleBox.fill('-4');
await scaleBox.press('Enter');
await wait(400);
check((await hatchOf())?.scale === 50 && (await scaleBox.inputValue()) === '50', 'a scale out of range goes back to the one set');

// ---- on screen: the section's cut walls show the hatch ------------------------------------------------
await pageExec(p, { cmd: 'section_plane', origin: [0, 0, 1.2], normal: [0, 0, -1] });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1500);
const onScreen = () =>
  p.evaluate(() => {
    const out = { hatch: 0, shader: 0, color: '', px: 0, paper: 0, solid: 0 };
    window.__sketchcraftView.current.model.traverse((o) => {
      if (o.userData?.sectionHatch === 'Walls' && o.userData?.hatchShader) {
        out.shader++;
        out.color = o.material.userData.color;
        out.px = o.material.userData.px;
      } else if (o.userData?.sectionHatch === 'Walls') {
        out.hatch += o.geometry.getAttribute('position').count / 2;
        out.color = '#' + o.material.color.getHexString();
      }
      if (o.userData?.sectionHatchFill) out.paper += o.geometry.getAttribute('position').count / 3;
      else if (o.userData?.sectionFill) out.solid += o.geometry.getAttribute('position').count / 3;
    });
    return out;
  });
let sc = await onScreen();
check(sc.shader === 1 && sc.hatch === 0 && sc.color === '#cc0000', `with a section plane the hatch shader draws the cut walls in the hatch's colour (${JSON.stringify(sc)})`);
check(sc.paper > 0 && sc.solid === 0, 'over paper white instead of the dark fill');
check(Math.abs(sc.px - 0.35 * 6) < 1e-6, `at the hatch's true weight: 0.35 mm is ${sc.px} px (6 px per paper mm)`);
// the pixels: look down on the walls and count the hatch colour's pixels on the cap
const lookDown = async () => {
  await p.evaluate(() => {
    const T = window.__sketchcraftView.current;
    T.controls.target.set(2, 1.5, 0);
    T.persp.position.set(2, 1.499, 9);
    T.persp.up.set(0, 0, 1);
    T.persp.lookAt(2, 1.5, 0);
    T.controls.update();
  });
  await wait(800);
};
await lookDown();
const inked = async (name) => {
  const shot = await p.screenshot({ path: path.join(shots, name) });
  return p.evaluate(async (b64) => {
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
    const d = g.getImageData(0, 0, i.width, i.height).data;
    let red = 0;
    let paper = 0;
    for (let k = 0; k < d.length; k += 4) {
      if (d[k] > 150 && d[k + 1] < 90 && d[k + 2] < 90) red++;
      else if (Math.abs(d[k] - 0xf6) < 4 && Math.abs(d[k + 1] - 0xf5) < 4 && Math.abs(d[k + 2] - 0xf0) < 4) paper++;
    }
    return { red, paper };
  }, shot.toString('base64'));
};
const thick = await inked('hatches-section.png');
check(thick.red > 300 && thick.paper > 1000, `the cap shows red hatch lines on paper white (${JSON.stringify(thick)})`);
// a thinner pen draws thinner lines on screen (true weight; the engine's lines were always 1 px)
await pageExec(p, { cmd: 'tag_style', tag: 'Walls', hatch: { weight_mm: 0.13 } });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1500);
await lookDown();
const thin = await inked('hatches-section-thin.png');
check(thin.red > 50 && thick.red > thin.red * 1.4, `a 0.35 mm hatch inks more pixels than a 0.13 mm one (${thick.red} against ${thin.red})`);
sc = await onScreen();
check(Math.abs(sc.px - 1) < 1e-6, `0.13 mm is drawn 1 px wide, never thinner (${sc.px})`);
// no cap on lines: a hatch far too dense for engine lines (it used to fall back to a tint) is still drawn
await pageExec(p, { cmd: 'tag_style', tag: 'Walls', hatch: { scale: 0.05, scale_mode: 'paper' } });
const dense = await pageExec(p, { cmd: 'section', origin: [0, 0, 1.2], normal: [0, 0, -1] });
check(dense.hatches?.length === 1 && dense.hatches[0].families.length > 0 && dense.hatch.length === 0 && dense.fill.length === 0, `a very dense hatch still goes to the shader, not a tint (${dense.hatches?.length} shaded, ${dense.fill.length / 9} tinted triangles)`);
await pageExec(p, { cmd: 'undo' });
await pageExec(p, { cmd: 'undo' });
await wait(1200);

// ---- the file keeps it -------------------------------------------------------------------------------
await wait(1500);
const onDisk = JSON.parse(fs.readFileSync(process.env.SKETCHCRAFT_DOC, 'utf8'));
check(onDisk.drafting?.tag_styles?.Walls?.cut?.hatch?.pattern === 'brick' && onDisk.format === 2, 'the saved file has the hatch, as format 2');

// ---- import a .pat file --------------------------------------------------------------------------------
await button.click();
await wait(400);
const pat = ';%UNITS=MM\n*Shingle, roof shingles\n0, 0,0, 0,4\n90, 0,0, 4,4, 4,-4\n*Brick, a brick of its own\n0,0,0,0,2\n';
await p.getByLabel('Import a .pat file').setInputFiles({ name: 'mine.pat', mimeType: 'text/plain', buffer: Buffer.from(pat) });
await wait(1200);
const note = await p.locator('.hatch-note').textContent().catch(() => '');
check(/Imported 2 patterns from mine\.pat/.test(note ?? ''), `the import says what it kept ("${note}")`);
const opts = await patternMenu.locator('option').allTextContents();
check(opts.includes('Shingle (imported)') && opts.includes('Brick-file (imported)'), `the imported patterns are in the menu, a clash with a built-in renamed (${opts.join(', ')})`);
await patternMenu.selectOption('Shingle');
await wait(700);
check((await hatchOf())?.pattern === 'Shingle', 'and one can be picked');
const listed = (await pageExec(p, { cmd: 'hatch_patterns' })).patterns.map((x) => x.name);
check(listed.includes('Shingle') && listed.includes('Brick-file'), 'the model keeps them');

// ---- undo and redo -----------------------------------------------------------------------------------
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(900);
check((await hatchOf())?.pattern === 'brick', `Ctrl+Z takes back the pattern change (${(await hatchOf())?.pattern})`);
await p.keyboard.press('Control+Shift+z');
await wait(900);
check((await hatchOf())?.pattern === 'Shingle', 'Ctrl+Shift+Z puts it back');

// ---- none ----------------------------------------------------------------------------------------------
if (!(await patternMenu.count())) await button.click();
await patternMenu.selectOption('');
await wait(1200);
const s = await scene();
check(!s.tagStyles?.Walls?.cut?.hatch, `None removes the hatch (${JSON.stringify(s.tagStyles?.Walls)})`);
sc = await onScreen();
check(sc.hatch === 0 && sc.paper === 0 && sc.solid > 0, `and the section is the plain dark fill again (${JSON.stringify(sc)})`);
check((await button.textContent())?.includes('Hatch'), 'the button says "Hatch" again');
await p.screenshot({ path: path.join(shots, 'hatches-none.png') });

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
