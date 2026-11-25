#!/usr/bin/env node
// Textured faces in perspective (#222): a picture drawn over a plain base on the same plane must never lose to the base
// as the view orbits. A big solid-red floor is looked at from far away at many angles, in perspective: the floor's pixels
// must be the picture's red every time, not the base's pale colour showing through. Then the same from the same eye in
// Parallel projection.
//   node tools/ui-tests/run.mjs texture-depth
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { launch, PAGE, pageExec } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same); tools/ui-tests/run.mjs does.');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};
const shots = process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-depth-'));
fs.mkdirSync(shots, { recursive: true });

// a 2x2 solid red picture
const solidRed = () => {
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
  const row = [0, 230, 20, 20, 230, 20, 20];
  const rows = Buffer.from([...row, ...row]);
  const file = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(rows)), chunk('IEND', Buffer.alloc(0))]);
  return 'data:image/png;base64,' + file.toString('base64');
};

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1500, height: 950 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));
const X = (c) => pageExec(p, c);

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);
await X({ cmd: 'new' });
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(900);

// a 200 m square floor in a picture of solid red, and a 1 m tile so the image repeats
await X({ cmd: 'texture', name: 'Red', data: solidRed(), width: 1.0, height: 1.0 });
const floor = await X({ cmd: 'rect', from: [-100, -100, 0], to: [100, 100, 0] });
const painted = await X({ cmd: 'paint', ids: [floor.face], material: 'Red' });
check(floor.ok === true && painted.ok === true, 'a 200 m floor painted with a solid red picture');
// the page takes the model from the engine when it loads
await p.reload();
await p.waitForSelector('.viewport canvas');
await wait(1500);

/** the share of the canvas's pixels that are the picture's red, and the share that are pale (the base showing through) */
const sample = () =>
  p.evaluate(() => {
    const c = document.querySelector('.viewport canvas');
    const r = c.getBoundingClientRect();
    const T = window.__sketchcraftView.current;
    T.renderer.render(T.scene ?? T.world, T.camera);
    const tmp = document.createElement('canvas');
    tmp.width = c.width;
    tmp.height = c.height;
    const g = tmp.getContext('2d');
    g.drawImage(c, 0, 0);
    // the middle of the picture: the camera looks at the floor's centre, so this is always floor
    const x0 = Math.floor(tmp.width * 0.3);
    const y0 = Math.floor(tmp.height * 0.55);
    const w = Math.floor(tmp.width * 0.4);
    const h = Math.floor(tmp.height * 0.3);
    const d = g.getImageData(x0, y0, w, h).data;
    let red = 0;
    let pale = 0;
    for (let i = 0; i < d.length; i += 4) {
      if (d[i] > 90 && d[i + 1] < 70 && d[i + 2] < 70) red++;
      else pale++;
    }
    return { red, pale, w: r.width, h: r.height };
  });

const look = (eye, target, ortho) =>
  p.evaluate(
    ([eye, target]) => {
      const T = window.__sketchcraftView.current;
      T.controls.target.set(...target);
      T.persp.position.set(...eye);
      T.persp.up.set(0, 0, 1);
      T.persp.lookAt(...target);
      T.persp.updateProjectionMatrix();
      T.controls.update();
    },
    [eye, target]
  );

// orbit round a point on the floor, from a long way off and low over it (so the floor is seen at a grazing angle), at
// several heights and turns; in each the sky takes the top of the picture and the floor the rest
let worst = 0;
let n = 0;
for (const dist of [30, 80]) {
  for (const lift of [0.05, 0.12, 0.3]) {
    for (let a = 0; a < 360; a += 40) {
      const r = (a * Math.PI) / 180;
      await look([Math.cos(r) * dist, Math.sin(r) * dist, dist * lift * 1.0 + 0.5], [0, 0, 0], false);
      await wait(120);
      const s = await sample();
      const floorPx = s.red + s.pale;
      const share = floorPx ? s.pale / floorPx : 0;
      worst = Math.max(worst, share);
      n++;
      if (share > 0.01 && n < 40) await p.screenshot({ path: path.join(shots, `persp-${dist}-${lift}-${a}.png`) });
    }
  }
}
console.log(`${n} perspective views, worst share of the base showing through the picture: ${(worst * 100).toFixed(2)}%`);
check(worst < 0.01, `in perspective, the picture covers the base at every angle (worst ${(worst * 100).toFixed(2)}%)`);
check(problems.length === 0, `no page errors ${problems.join('; ')}`);
await b.close();
console.log(failed ? `${failed} failed` : 'all passed');
process.exit(failed ? 1 : 0);
