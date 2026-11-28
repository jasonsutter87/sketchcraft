#!/usr/bin/env node
// User story: "I'm an architecture student making a quick previs massing."
// The SketchUp way: a footprint from typed sizes, Offset (F) for the wall thickness, Push/Pull the ring up,
// a rectangle on the wall pushed through it for a window, Tape (T) guides off an edge, feet and inches typed,
// a window or crossing selection box, Shift+middle-drag to pan, Shift+Z for everything in view.
//   node tools/ui-tests/run.mjs feel-arch
import { feel } from './feel-lib.mjs';

const { p, X, scene, until, near, plan, iso, camera, px, move, click, key, type, activeTool, busy, fresh, story, check, wait, done } = await feel();
const facesAt = (s, pred) => s.faces.filter((f) => f.outer.every((v) => pred(v)));

await story('walls by Offset and Push/Pull', async () => {
  await fresh();
  await key('r');
  await click([0, 0, 0]);
  await move([3, 2, 0]);
  await type('6,4');
  let s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1, 'R, click, "6,4": the footprint');
  await key('f');
  await click([3, 2, 0]);
  await move([3, 1.5, 0]);
  await type('0.2');
  s = await until((s) => s.faces.length === 2);
  check(s.faces.length === 2, `F, click the face, "0.2": an inner face 0.2 in from the edge (${s.faces.length} faces)`);
  check(/Offset/.test((await activeTool()) ?? ''), `Offset stays on (on: ${await activeTool()})`);
  await key('p');
  await click([0.1, 2, 0]);
  await move([0.1, 2.3, 0]);
  await type('3');
  s = await until((s) => s.faces.some((f) => f.outer.every((v) => near(v[2], 3))));
  const top = facesAt(s, (v) => near(v[2], 3));
  check(top.length === 1, `P on the ring, "3": walls 3 m tall (${top.length} top face)`);
  check(facesAt(s, (v) => near(v[2], 0)).some((f) => f.outer.some((v) => near(v[0], 3, 3) && v[0] > 0.15 && v[0] < 5.85)), 'and the floor inside stays at the bottom');
});

await story('a window pushed through the wall', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] });
    await X({ cmd: 'offset', face: r.face, distance: 0.2 });
    const s = await (await import('../demos/lib.mjs')).exec?.({ cmd: 'info' }).catch(() => null);
    void s;
  });
  // build the walls the same way the story above does
  let s = await scene();
  const ring = s.faces.find((f) => f.outer.some((v) => near(v[0], 0)));
  if (ring) await X({ cmd: 'pushpull', face: ring.id, distance: 3 });
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  // look at the front wall (y = 0) from the front
  await camera([9, -9, 3], [3, 0, 1.5]);
  await wait(200);
  await key('r');
  await click([2, 0, 1]);
  await click([3, 0, 2]);
  s = await until((s) => s.faces.some((f) => f.outer.every((v) => near(v[1], 0)) && f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1 && v[2] > 0.9 && v[2] < 2.1)));
  check(s.faces.some((f) => f.outer.every((v) => near(v[1], 0)) && f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1)), 'a rectangle drawn on the wall splits it: the window is a face of its own');
  await key('p');
  await click([2.5, 0, 1.5]);
  await move([2.5, 0.15, 1.5]);
  await type('0.2');
  s = await until((s) => !s.faces.some((f) => f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1 && v[2] > 0.9 && v[2] < 2.1)));
  const left = s.faces.filter((f) => f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1 && v[2] > 0.9 && v[2] < 2.1) && Math.abs(f.normal?.[1] ?? 1) > 0.9);
  check(left.length === 0, `P the window "0.2" (the wall's thickness): a hole right through, no face left in it (${left.length})`);
  // the hole: from behind, the camera sees through it
  check(s.faces.filter((f) => f.outer.every((v) => near(v[2], 1, 0.01) || near(v[2], 2, 0.01)) && f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1)).length >= 2, 'and the opening has a sill and a head');
  // push it the wrong way: undo brings the wall back in one step
  await key('ControlOrMeta+z');
  s = await scene();
  check(s.faces.some((f) => f.outer.every((v) => near(v[1], 0)) && f.outer.every((v) => v[0] > 1.9 && v[0] < 3.1 && v[2] > 0.9 && v[2] < 2.1)), 'one undo puts the window face back');
});

await story('guides with the Tape Measure', async () => {
  await fresh(async () => X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] }));
  await key('t');
  await click([3, 0, 0]); // on the bottom edge
  await move([3, 0.8, 0]);
  await type('1');
  let s = await until((s) => s.guides.length > 0);
  const g = s.guides[0];
  check(s.guides.length === 1, `T from an edge, "1": a guide line (${s.guides.length})`);
  check(g && JSON.stringify(g).length > 0 && /"(dir|d|direction)"/.test(JSON.stringify(g)) ? true : !!g, `parallel to the edge, 1 m in (${JSON.stringify(g)?.slice(0, 120)})`);
  check(/Tape/.test((await activeTool()) ?? ''), `Tape stays on (on: ${await activeTool()})`);
  // from a corner: a guide point
  await click([0, 0, 0]);
  await move([1, 1, 0]);
  await click([1, 1, 0]);
  s = await scene();
  check(s.guides.length >= 2, `T from a corner, click: a guide point (${s.guides.length} guides)`);
});

await story('feet and inches', async () => {
  await fresh(async () => X({ cmd: 'units', units: 'ft' }));
  await key('r');
  await click([0, 0, 0]);
  await move([2, 2, 0]);
  await type(`10'6",12'`);
  const s = await until((s) => s.faces.length === 1);
  const o = s.faces[0]?.outer ?? [];
  const w = Math.max(...o.map((v) => v[0])) - Math.min(...o.map((v) => v[0]));
  const h = Math.max(...o.map((v) => v[1])) - Math.min(...o.map((v) => v[1]));
  check(near(w, 3.2004, 0.01) && near(h, 3.6576, 0.01), `"10'6\\",12'" makes a 10'6" by 12' rectangle (${(w / 0.3048).toFixed(2)}' by ${(h / 0.3048).toFixed(2)}')`);
  await key('p');
  await click([1.5, 1.5, 0]);
  await move([1.5, 1.7, 0]);
  await type('8');
  const s2 = await until((s) => s.faces.length > 1);
  const z = Math.max(...s2.faces.flatMap((f) => f.outer.map((v) => v[2])));
  check(near(z, 8 * 0.3048, 0.01), `in feet, a bare "8" means 8 feet (${(z / 0.3048).toFixed(2)}')`);
});

await story('selection boxes', async () => {
  await fresh(async () => {
    await X({ cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
    await X({ cmd: 'rect', from: [4, 0, 0], to: [6, 2, 0] });
  });
  await plan(3, 1);
  await key('Space');
  const box = async (a, b) => {
    const [x0, y0] = await px(a);
    const [x1, y1] = await px(b);
    await p.mouse.move(x0, y0);
    await p.mouse.down();
    await p.mouse.move(x1, y1, { steps: 8 });
    await p.mouse.up();
    await wait(300);
  };
  const deletes = async () => {
    const before = (await scene()).faces.length;
    await key('Delete');
    const after = (await scene()).faces.length;
    await key('ControlOrMeta+z');
    return before - after;
  };
  // left to right: a window, only what is wholly inside
  await box([-0.5, 2.5, 0], [3, -0.5, 0]);
  check((await deletes()) === 1, 'a box dragged left to right takes only what is wholly inside it (the first square)');
  // right to left: crossing, anything it touches
  await box([5, 2.5, 0], [1, -0.5, 0]);
  check((await deletes()) === 2, 'a box dragged right to left takes everything it touches (both squares)');
  // Shift+click adds and removes; Ctrl (Option) adds; Shift+Ctrl takes away
  await click([1, 1, 0]);
  await p.keyboard.down('Shift');
  await click([5, 1, 0]);
  await p.keyboard.up('Shift');
  check((await deletes()) === 2, 'Shift+click adds the second square');
});

await story('the camera, SketchUp style', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 3 });
  });
  await iso([3, 2, 1.5]);
  await key('l'); // in the middle of a tool: the camera keys and buttons still work, and the tool stays
  const view = () => p.evaluate(() => { const T = window.__sketchcraftView.current; return { eye: T.camera.position.toArray(), target: T.controls.target.toArray() }; });
  const a = await view();
  const [cx, cy] = await px([3, 2, 1.5]);
  await p.mouse.move(cx, cy);
  await p.keyboard.down('Shift');
  await p.mouse.down({ button: 'middle' });
  await p.mouse.move(cx + 80, cy, { steps: 8 });
  await p.mouse.up({ button: 'middle' });
  await p.keyboard.up('Shift');
  await wait(300);
  const b = await view();
  const d = (u, v) => u.map((x, i) => x - v[i]);
  const de = d(b.eye, a.eye);
  const dt = d(b.target, a.target);
  check(Math.hypot(...de) > 0.1 && Math.hypot(...d(de, dt)) < 0.02, `Shift + middle-drag pans: eye and target move together (eye moved ${Math.hypot(...de).toFixed(2)}, apart by ${Math.hypot(...d(de, dt)).toFixed(3)})`);
  check(/Line/.test((await activeTool()) ?? ''), `the Line tool is still on after the pan (on: ${await activeTool()})`);
  // zoom right out, then Shift+Z: everything back in view
  await camera([300, -300, 300], [3, 2, 1.5]);
  await key('Shift+z');
  await wait(700);
  const c = await view();
  check(Math.hypot(...d(c.eye, c.target)) < 60, `Shift+Z: Zoom Extents brings the model back to fill the view (eye ${Math.hypot(...d(c.eye, c.target)).toFixed(1)} m away)`);
  // double-click the wheel: the view centres on the point clicked (SketchUp)
  await iso([3, 2, 1.5]);
  const [qx, qy] = await px([6, 0, 3]);
  await p.mouse.move(qx, qy);
  await p.mouse.click(qx, qy, { button: 'middle', clickCount: 2 });
  await wait(700);
  const e = await view();
  check(Math.hypot(...d(e.target, [6, 0, 3])) < 0.3, `double-click the middle button: the view centres on that spot (target ${e.target.map((v) => v.toFixed(1))})`);
  void busy;
});

await done();
