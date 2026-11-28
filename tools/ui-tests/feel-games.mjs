#!/usr/bin/env node
// User story: "I'm a 3D modeler who makes props for games."
// A SketchUp hand builds a low-poly crate from the keyboard: R, a click, "1,1"; P, "1"; Space; triple-click; G;
// M with a tapped Option/Ctrl to copy, then "x3" for a row; double-click in, Esc, Esc out; L with typed lengths.
// Every check is something a SketchUp user does without thinking. When SketchCraft does it differently it feels
// "off" even when the result can be had another way, so each check says what SketchUp does.
//   node tools/ui-tests/run.mjs feel-games
import { feel } from './feel-lib.mjs';

const { p, X, scene, until, near, zTop, iso, px, move, click, key, type, activeTool, busy, fresh, story, instances, check, wait, done } = await feel();

await story('a box from the keyboard', async () => {
  await fresh();
  await key('r');
  await click([0, 0, 0]);
  await move([1.5, 1.2, 0]);
  await type('1,1');
  let s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1, 'R, click, type "1,1" Enter: a 1 x 1 rectangle (SketchUp: the Measurements box takes it with no click)');
  check(/Rectangle/.test((await activeTool()) ?? ''), `after the rectangle the Rectangle tool stays on (on: ${await activeTool()})`);
  await key('p');
  await click([0.5, 0.5, 0]);
  await move([0.5, 0.7, 0]);
  await type('1');
  s = await until((s) => zTop(s) > 0.5);
  check(near(zTop(s), 1), `P, click the face, type "1" Enter: a 1 m box (top at ${zTop(s).toFixed(2)})`);
  check(/Push/.test((await activeTool()) ?? ''), `after the push Push/Pull stays on (on: ${await activeTool()})`);
  // double-click another face: the same distance again
  await key('r');
  await click([3, 0, 0]);
  await click([4, 1, 0]);
  await key('p');
  await click([3.5, 0.5, 0], 2);
  s = await until((s) => s.faces.filter((f) => f.outer.some((v) => v[0] > 2.9 && v[2] > 0.9)).length > 0);
  check(s.faces.some((f) => f.outer.some((v) => v[0] > 2.9 && v[2] > 0.9)), 'Push/Pull double-click on another face repeats the last distance');
  await key('ControlOrMeta+z');
  await key('ControlOrMeta+z');
});

await story('Esc, the way SketchUp has it', async () => {
  await fresh(async () => X({ cmd: 'rect', from: [0, 0, 0], to: [1, 1, 0] }));
  // a value typed, then Esc: one Esc ends the operation
  await key('p');
  await click([0.5, 0.5, 0]);
  await move([0.5, 0.8, 0]);
  check(await busy(), 'pushing: the Value box says what it measures');
  await type('5', false);
  await key('Escape');
  check(!(await busy()), 'type a value, think better of it, Esc once: the push is cancelled (SketchUp: one Esc)');
  if (await busy()) await key('Escape');
  // Esc with nothing going on keeps the tool (SketchUp: Esc never changes the tool)
  check(/Push/.test((await activeTool()) ?? ''), `Esc keeps the tool (on: ${await activeTool()})`);
  // a line mid-way: Esc drops only the rubber band, the lines drawn stay
  await key('l');
  await click([2, 0, 0]);
  await click([3, 0, 0]);
  await move([3, 1, 0]);
  await key('Escape');
  const s = await scene();
  check(s.edges.length / 6 >= 5, `Esc mid-chain keeps the lines already drawn (${s.edges.length / 6} edges)`);
});

await story('a line chain from typed lengths, axis-locked', async () => {
  await fresh();
  await key('l');
  await key('ArrowRight'); // lock before the first click: SketchUp lets you
  await click([0, 0, 0]);
  await move([1, 0.6, 0]);
  await type('2');
  let s = await until((s) => s.edges.length >= 6);
  const e = s.edges;
  check(e.length >= 6 && near(Math.abs(e[3] - e[0]), 2) && near(e[4], e[1]), `→ before the first click, then "2": a 2 m line along red (${JSON.stringify(e.slice(0, 6).map((v) => +v.toFixed(2)))})`);
  await move([2.1, 1, 0]);
  await type('1');
  s = await until((s) => s.edges.length >= 12);
  check(s.edges.length >= 12, `the chain goes on from the end: "1" draws the next line (${s.edges.length / 6} edges)`);
  // orbiting with the middle button in the middle of a line keeps the line going
  await move([2, 2, 0]);
  const [mx, my] = await px([2, 2, 0]);
  await p.mouse.down({ button: 'middle' });
  await p.mouse.move(mx + 60, my + 20, { steps: 6 });
  await p.mouse.up({ button: 'middle' });
  await wait(200);
  check(await busy(), 'a middle-button orbit mid-line keeps the line going');
  await key('Escape');
});

await story('a crate component, copied in a row', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [1, 1, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 1 });
  });
  await iso();
  await move([3, 3, 0]);
  await wait(400);
  await key('Space');
  check(/Select/.test((await activeTool()) ?? ''), 'Space: Select');
  await click([0.5, 0, 0.5], 3);
  await key('g');
  const dlg = p.locator('.sheet.ask input');
  check((await dlg.count()) > 0, 'G asks for the component name');
  if (await dlg.count()) {
    await dlg.fill('Crate');
    await p.keyboard.press('Enter');
    await wait(500);
  }
  check((await instances()) === 1, `Enter makes the component (${await instances()} instance)`);
  const comp = await scene();
  check(comp.faces.filter((f) => f.group == null).length === 0, `triple-click had picked the whole box: no loose faces left (${comp.faces.filter((f) => f.group == null).length})`);
  const s0 = await scene();
  check((await p.locator('.section-title', { hasText: /Crate|Component/ }).count()) > 0, 'and the new component is selected, ready to move (SketchUp: "Replace selection")');
  // M, click its corner, move along red, tap Option/Ctrl for a copy, click
  await key('Escape');
  await key('m');
  await click([1, 0, 0]);
  await move([1.3, 0, 0]);
  check(await busy(), 'M, click the crate\'s corner: it is picked up');
  await move([2, 0, 0]);
  await move([2.5, 0, 0]);
  await key('Alt');
  await click([2.5, 0, 0]);
  const n1 = await instances();
  check(n1 === 2, `M, pick up, TAP Option/Ctrl, click: a copy (${n1} instances; SketchUp toggles copy with a tap any time during the move)`);
  if (n1 !== 2) {
    // the way that works here: hold it when you pick up
    const moved = JSON.stringify((await scene()).instances) !== JSON.stringify(s0.instances);
    console.log(`     (the tap: ${moved ? 'moved the crate, no copy' : 'nothing happened'}; still busy: ${await busy()})`);
    await key('Escape');
    if (moved) await key('ControlOrMeta+z');
    await p.keyboard.down('Alt');
    await click([1, 0, 0]);
    await p.keyboard.up('Alt');
    await move([2.5, 0, 0]);
    await click([2.5, 0, 0]);
  }
  await type('x3');
  const n2 = await instances();
  check(n2 === 4, `"x3" right after: a row of 4 (${n2})`);
  // retype the count right away: "*5" (SketchUp takes * as well as x) makes it a row of 6
  await type('*5');
  check((await instances()) === 6, `then "*5": the row becomes 6 (${await instances()})`);
  void s0;
});

await story('Move then Esc puts it back', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [1, 1, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 1 });
    const s = await (await fetch('about:blank').catch(() => null), null);
    void s;
  });
  await iso();
  await key('Space');
  await click([0.5, 0, 0.5], 3);
  await key('m');
  await click([1, 0, 0]);
  await move([3, 0, 0]);
  await key('Escape');
  await wait(300);
  const s = await scene();
  const xs = s.faces.flatMap((f) => f.outer.map((v) => v[0]));
  check(near(Math.max(...xs), 1), `Esc mid-move leaves the box where it was (max x ${Math.max(...xs).toFixed(2)})`);
  check(/Move/.test((await activeTool()) ?? ''), `and Move stays on (on: ${await activeTool()})`);
});

await story('in and out of a group', async () => {
  await fresh(async () => {
    const r = await X({ cmd: 'rect', from: [0, 0, 0], to: [1, 1, 0] });
    await X({ cmd: 'pushpull', face: r.face, distance: 1 });
  });
  await iso();
  await key('Space');
  await click([0.5, 0, 0.5], 3);
  await key('Control+g').catch(() => {});
  let s = await scene();
  if (!s.groups?.length) {
    // the group shortcut differs; use the command
    const ids = s.faces.map((f) => f.id);
    await X({ cmd: 'group', ids });
    await p.reload();
    await p.waitForSelector('.viewport canvas');
    await wait(900);
    await iso();
  }
  s = await scene();
  check((s.groups?.length ?? 0) === 1, `there is a group (${s.groups?.length})`);
  await key('Space');
  await click([0.5, 0, 0.5], 2);
  s = await scene();
  check(s.activeGroup != null || s.editing != null, 'double-click opens the group');
  await click([0.5, 0, 0.5]);
  await key('Escape');
  s = await scene();
  check(s.activeGroup != null || s.editing != null, 'with a face picked inside, the first Esc clears the pick and stays in (SketchUp)');
  await key('Escape');
  s = await scene();
  check(s.activeGroup == null && s.editing == null, 'the next Esc closes the group');
});

await done();
