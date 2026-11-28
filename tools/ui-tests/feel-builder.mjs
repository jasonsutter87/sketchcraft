#!/usr/bin/env node
// User story: "I'm a home builder and I show clients their house."
// The SketchUp way: turn the house to sit on the lot (Q, "90"), paint it (B, Cmd to sample), dimension it (D),
// copy and paste a part, erase with a drag, save scenes and click their tabs to walk the client through,
// stand inside at eye height ("5'6"" with Position Camera).
//   node tools/ui-tests/run.mjs feel-builder
import { feel } from './feel-lib.mjs';

const { p, X, scene, until, near, plan, iso, move, click, key, type, activeTool, busy, fresh, story, check, wait, done } = await feel();
const xs = (s) => s.faces.flatMap((f) => f.outer.map((v) => v[0]));
const ys = (s) => s.faces.flatMap((f) => f.outer.map((v) => v[1]));

await story('turn the house onto the lot', async () => {
  await fresh(async () => X({ cmd: 'rect', from: [0, 0, 0], to: [4, 2, 0] }));
  await key('Space');
  await click([2, 1, 0], 2); // the face and its edges
  await key('q');
  await click([0, 0, 0]);
  await click([1, 0, 0]);
  await move([0.7, 0.7, 0]);
  await move([0, 1, 0]);
  await type('90');
  const s = await until((s) => Math.min(...xs(s)) < -1);
  check(near(Math.min(...xs(s)), -2, 0.05) && near(Math.max(...ys(s)), 4, 0.05), `Q, centre, reference, "90": a quarter turn (x ${Math.min(...xs(s)).toFixed(2)}..${Math.max(...xs(s)).toFixed(2)}, y ..${Math.max(...ys(s)).toFixed(2)})`);
  check(/Rotate/.test((await activeTool()) ?? ''), `Rotate stays on (on: ${await activeTool()})`);
});

await story('paint it', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 2 });
  });
  await iso([1, 1, 1]);
  await key('b');
  check(/Paint/.test((await activeTool()) ?? ''), 'B: the Paint Bucket');
  const before = await scene();
  const colorOf = (s, pred) => {
    const f = s.faces.find((f) => f.outer.every(pred));
    return f ? JSON.stringify([f.color, f.material]) : null;
  };
  const top = (v) => near(v[2], 2);
  const front = (v) => near(v[1], 0);
  await click([1, 1, 2]);
  let s = await scene();
  check(colorOf(s, top) !== colorOf(before, top), 'a click paints the top');
  // paint the front a different color, then sample the top with Cmd (Alt on Windows) and paint the front with it
  await X({ cmd: 'paint', ids: [s.faces.find((f) => f.outer.every(front)).id], color: [0.1, 0.5, 0.2] });
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  await iso([1, 1, 1]);
  await key('b');
  await p.keyboard.down(process.platform === 'darwin' ? 'Meta' : 'Alt');
  await click([1, 1, 2]);
  await p.keyboard.up(process.platform === 'darwin' ? 'Meta' : 'Alt');
  await click([1, 0, 1]);
  s = await scene();
  check(colorOf(s, front) === colorOf(s, top), `⌘-click samples the top's paint, the next click puts it on the front (${colorOf(s, front)} vs ${colorOf(s, top)})`);
});

await story('dimensions for the client', async () => {
  await fresh(async () => X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] }));
  await key('d');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  await move([3, -1, 0]);
  await click([3, -1, 0]);
  const s = await until((s) => s.annotations.length > 0);
  check(s.annotations.length === 1, `D, corner, corner, pull out, click: a dimension (${s.annotations.length})`);
  check(/Dimension/.test((await activeTool()) ?? ''), `Dimension stays on for the next one (on: ${await activeTool()})`);
  // an edge click then pull out: SketchUp dimensions the whole edge
  await click([0, 1.3, 0]);
  await move([-1, 1.3, 0]);
  await click([-1, 1.3, 0]);
  const s2 = await scene();
  const d2 = s2.annotations[1];
  const L2 = d2 ? Math.hypot(...d2.a.map((v, i) => v - d2.b[i])) : 0;
  check(s2.annotations.length === 2 && near(L2, 4, 0.01), `click on an edge, pull out, click: that edge's whole length (${s2.annotations.length} dimensions, ${L2.toFixed(2)} m)`);
});

await story('copy, paste, erase', async () => {
  await fresh(async () => X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] }));
  await key('Space');
  await click([1, 1, 0], 2);
  await key('ControlOrMeta+c');
  await key('ControlOrMeta+v');
  await move([5, 1, 0]);
  await click([5, 1, 0]);
  let s = await until((s) => s.faces.length === 2);
  check(s.faces.length === 2, `copy, paste, click: a second square where you click (${s.faces.length} faces)`);
  // the eraser: drag across edges to erase them all in one go (one undo)
  const edges0 = s.edges.length / 6;
  await key('e');
  await move([1, -0.5, 0]);
  await p.mouse.down();
  await move([1, 2.5, 0], 40);
  await move([5, 2.5, 0], 40);
  await p.mouse.up();
  await wait(400);
  s = await scene();
  check(s.edges.length / 6 <= edges0 - 2, `E and a drag across edges erases each one crossed (${edges0} -> ${s.edges.length / 6})`);
  await key('ControlOrMeta+z');
  const s2 = await scene();
  check(s2.edges.length / 6 === edges0, `one undo brings back the whole drag's worth (${s2.edges.length / 6} of ${edges0})`);
});

await story('scenes to walk the client through', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 3 });
    await X({ cmd: 'save_view', name: 'Street', eye: [3, -20, 1.7], target: [3, 2, 1.5] });
    await X({ cmd: 'save_view', name: 'Aerial', eye: [20, -20, 20], target: [3, 2, 0] });
  });
  const tabs = p.locator('.su-scene, .scene-tabs button, [role=tab]', { hasText: 'Street' });
  check((await tabs.count()) > 0, 'saved scenes show as tabs over the view, one click away (SketchUp), in the default layout');
  if (await tabs.count()) {
    await tabs.first().click();
    await wait(1500);
    const e = await p.evaluate(() => window.__sketchcraftView.current.camera.position.toArray());
    check(Math.hypot(e[0] - 3, e[1] + 20, e[2] - 1.7) < 0.5, `clicking "Street" goes to it (eye ${e.map((v) => v.toFixed(1))})`);
  }
});

await story('stand inside at eye height', async () => {
  await fresh(async () => {
    await X({ cmd: 'units', units: 'ft' });
    await X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] });
  });
  await plan(3, 2);
  // SketchUp has no default key for Position Camera; use the command search (Shift+S) like a user who knows it
  await key('Shift+s');
  await p.keyboard.type('Position Camera', { delay: 10 });
  await p.keyboard.press('Enter');
  await wait(300);
  await click([3, 2, 0]);
  await wait(800);
  await type(`5'6"`);
  await wait(400);
  const z = await p.evaluate(() => window.__sketchcraftView.current.camera.position.z);
  check(near(z, 5.5 * 0.3048, 0.05), `click to stand, "5'6\\"": the eye is 5'6" up (${(z / 0.3048).toFixed(2)}')`);
});

await done();
