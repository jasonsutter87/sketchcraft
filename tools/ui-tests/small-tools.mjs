#!/usr/bin/env node
// The small drawing tools (#25) in a real browser: the Curve tool (click points, a smooth curve goes through them),
// and Tools > Flatten to height… and Make faces on imported CAD linework. Driven with real pointer and key events;
// the model is checked through the bridge after each step. Needs the dev page and bridge on a scratch model:
//   node tools/ui-tests/run.mjs small-tools
// Like tools.mjs, each scenario builds its model with commands from the page, RELOADS the page (it does not notice
// changes another client makes), puts the camera over the drawing (20 px to the metre) and then uses the mouse.
import fs from 'node:fs';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene, ROOT } from '../demos/lib.mjs';

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
const INFO = Object.fromEntries(
  [...fs.readFileSync(path.join(ROOT, 'app/src/workspace.ts'), 'utf8').matchAll(/^ {2}([a-z0-9]+): \{ name: "([^"]*)", key: "([^"]*)" \},?$/gm)].map((m) => [m[1], { name: m[2], key: m[3] }])
);

const b = await launch();
const problems = [];
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1700, height: 1000 } });
await ctx.addInitScript((tools) => {
  try {
    localStorage.setItem('sketchcraft.workspaceEdits', JSON.stringify({ model: { tools: [tools] } }));
    localStorage.setItem('sketchcraft.help', 'off');
  } catch {}
}, Object.keys(INFO));
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));
const X = (c) => pageExec(p, c);
const scene = () => pageScene(p);
const until = async (pred, ms = 3000) => {
  let s = await scene();
  for (let t = 0; t < ms && !pred(s); t += 100) {
    await wait(100);
    s = await scene();
  }
  return s;
};
const near = (a, c, tol = 0.15) => Math.abs(a - c) <= tol;
const edgeSegs = (s) => {
  const e = s.edges;
  const out = [];
  for (let i = 0; i + 5 < e.length; i += 6) out.push([[e[i], e[i + 1], e[i + 2]], [e[i + 3], e[i + 4], e[i + 5]]]);
  return out;
};
const area = (f) => {
  const o = f.outer;
  let a = 0;
  for (let i = 0; i < o.length; i++) {
    const [p0, p1] = [o[i], o[(i + 1) % o.length]];
    a += p0[0] * p1[1] - p1[0] * p0[1];
  }
  return Math.abs(a) / 2;
};
const sh = (name) => (process.env.SHOT_DIR ? p.screenshot({ path: path.join(process.env.SHOT_DIR, `small-tools-${name}.png`) }).catch(() => {}) : null);

// ---- driving the page (the same as tools.mjs) ------------------------------------------------------------------------
const camera = async (eye, target) => {
  for (let i = 0; ; i++) {
    try {
      return await p.evaluate(([eye, target]) => {
        const T = window.__sketchcraftView.current;
        T.controls.target.set(...target);
        T.persp.position.set(...eye);
        T.persp.up.set(0, 0, 1);
        T.persp.lookAt(...target);
        T.controls.update();
      }, [eye, target]);
    } catch (e) {
      if (i >= 4) throw e;
      await p.waitForLoadState('load').catch(() => {});
      await wait(500);
    }
  }
};
const plan = (cx = 0, cy = 0) => camera([cx, cy - 0.0005, 60], [cx, cy, 0]);
const px = (pt) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, pt);
const move = async (pt, steps = 4) => {
  const [x, y] = await px(pt);
  await p.mouse.move(x, y, { steps });
  await wait(60);
};
const click = async (pt) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.click(x, y);
  await wait(160);
};
const dblclick = async (pt) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.dblclick(x, y);
  await wait(300);
};
const key = async (k) => {
  await p.keyboard.press(k);
  await wait(150);
};
const typed = async (text) => {
  await p.keyboard.type(text, { delay: 15 });
  await p.keyboard.press('Enter');
  await wait(250);
};
const tool = async (id) => {
  const btn = p.locator(`nav.toolbar button[aria-label="${INFO[id].name}"]`);
  if (!(await btn.count())) {
    await p.locator('nav.toolbar button.more').click();
    await p.locator('.more-tools button[role=option]', { hasText: INFO[id].name.split(':')[0].split(' (')[0] }).first().click();
  } else await btn.first().click();
  await wait(150);
};
const activeTool = async () => (await p.locator('nav.toolbar button.tool.on').first().getAttribute('aria-label').catch(() => '')) ?? '';
const fresh = async (build, { at = [0, 0] } = {}) => {
  await key('Escape');
  await X({ cmd: 'new' });
  const ids = (await build?.(X)) ?? {};
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  await plan(...at);
  await wait(250);
  return ids;
};
const scenario = async (name, fn) => {
  const before = failed;
  try {
    await fn();
  } catch (e) {
    check(false, `${name}: stopped early (${String(e.message ?? e).split('\n')[0]})`);
  }
  if (failed > before) await sh(`failed-${name.replace(/\W+/g, '-')}`);
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);

// =====================================================================================================================
// the Curve tool
// =====================================================================================================================
const POINTS = [[-6, -2, 0], [-2, 3, 0], [2, -3, 0], [6, 2, 0]];
// the engine's curve through those points, to compare with what the tool drew
const expectedEdges = async (pts, segments) => {
  await X({ cmd: 'new' });
  const r = await X({ cmd: 'spline', points: pts, segments });
  const s = await scene();
  await X({ cmd: 'new' });
  return { n: r.edges.length, segs: edgeSegs(s) };
};

await scenario('curve', async () => {
  await fresh();
  check(INFO.spline.key === 'U', 'the Curve tool has a free key (U)');
  await tool('spline');
  check((await activeTool()) === INFO.spline.name, 'the Curve button turns the tool on');
  for (const q of POINTS) await click(q);
  await key('Enter');
  let s = await until((s) => edgeSegs(s).length >= 1);
  // three spans of the default 12 pieces
  check(edgeSegs(s).length === 36, `four clicks and Enter draw a curve of 3 x 12 pieces (${edgeSegs(s).length} edges)`);
  const ends = edgeSegs(s).flat();
  for (const q of POINTS) check(ends.some((e) => Math.hypot(e[0] - q[0], e[1] - q[1]) < 0.15), `the curve goes through the clicked point (${q[0]}, ${q[1]})`);
  check(s.faces.length === 0, 'an open curve makes no face');
  // the same curve the engine draws for those points
  const want = await expectedEdges(POINTS, 12);
  await fresh();
  await tool('spline');
  for (const q of POINTS) await click(q);
  await key('Enter');
  s = await until((s) => edgeSegs(s).length >= 1);
  const got = edgeSegs(s);
  const sameShape = got.length === want.n && got.every((g) => want.segs.some((w) => [0, 1].every((i) => Math.hypot(g[i][0] - w[i][0], g[i][1] - w[i][1]) < 0.15)));
  check(sameShape, 'it is the same curve the spline command draws');

  // Undo takes the whole curve back in one step
  await key('ControlOrMeta+z');
  s = await until((s) => edgeSegs(s).length === 0);
  check(edgeSegs(s).length === 0, 'one Undo takes the whole curve back');
});

await scenario('curve-finish-and-count', async () => {
  // double-click finishes; a typed number is the pieces between points
  await fresh();
  await tool('spline');
  await click([-6, 0, 0]);
  await click([0, 4, 0]);
  await move([3, 2, 0]);
  await typed('4');
  await dblclick([6, 0, 0]);
  let s = await until((s) => edgeSegs(s).length >= 1);
  check(edgeSegs(s).length === 8, `a typed 4 makes 4 pieces between points: 2 spans, 8 edges (${edgeSegs(s).length})`);
  // the count is kept for the next curve
  await click([-6, -5, 0]);
  await click([0, -3, 0]);
  await key('Enter');
  s = await until((s) => edgeSegs(s).length >= 12);
  check(edgeSegs(s).length === 12, `the next curve keeps the 4 (${edgeSegs(s).length} edges)`);

  // clicking the first point closes the curve into a face
  await fresh();
  await tool('spline');
  await click([-4, -4, 0]);
  await click([4, -4, 0]);
  await click([4, 4, 0]);
  await click([-4, 4, 0]);
  await click([-4, -4, 0]);
  s = await until((s) => s.faces.length >= 1);
  const a = s.faces[0] ? area(s.faces[0]) : 0;
  check(s.faces.length === 1 && a > 64 && a < 110, `a click on the first point closes the curve into a face of ${a.toFixed(1)} m2`);

  // Esc drops a curve in progress
  await fresh();
  await tool('spline');
  await click([-4, 0, 0]);
  await click([0, 3, 0]);
  await move([4, 0, 0]);
  await key('Escape');
  await wait(300);
  s = await scene();
  check(edgeSegs(s).length === 0, `Esc drops a curve in progress: nothing drawn (${edgeSegs(s).length} edges)`);

  // the U key picks the tool
  await fresh();
  await tool('select');
  await key('u');
  check((await activeTool()) === INFO.spline.name, 'the U key picks the Curve tool');
});

// =====================================================================================================================
// Tools > Flatten to height… and Make faces, on imported CAD linework
// =====================================================================================================================
const menuItem = async (id) => {
  await p.locator('[role=menubar] > .menubar-item > button[data-menu="tools"]').click();
  await p.locator(`[data-action="${id}"]`).click();
  await wait(300);
};
const toastText = () => p.locator('.toast').first().innerText({ timeout: 2500 }).catch(() => '');
const allFlat = (s, z) => {
  const zs = edgeSegs(s).flatMap((e) => [e[0][2], e[1][2]]);
  return zs.length > 0 && zs.every((v) => near(v, z, 0.001));
};

await scenario('flatten-and-make-faces', async () => {
  const dxf = path.join(ROOT, 'core/tests/data/plate_3d_linework.dxf');
  await fresh((X) => X({ cmd: 'import_dxf', path: dxf }), { at: [5, 3] });
  let s = await scene();
  check(edgeSegs(s).length === 9 && s.faces.length === 0, `the plate drawn at odd heights comes in as lines only (${edgeSegs(s).length} lines, ${s.faces.length} faces)`);
  check(!allFlat(s, 0), 'its points are at different heights');

  // no selection: Flatten asks for one
  await tool('select');
  await menuItem('tools.flatten');
  check(/Pick the lines/.test(await toastText()), 'Flatten with nothing picked says to pick something first');
  check((await p.locator('.ask input').count()) === 0, 'and does not ask for a height');

  // pick everything, flatten to 0 through the menu and its height box
  await p.locator('.viewport canvas').click({ position: { x: 850, y: 120 } });
  await key('ControlOrMeta+a');
  await menuItem('tools.flatten');
  const box = p.locator('.ask input');
  await box.waitFor({ timeout: 3000 });
  check((await box.inputValue()).startsWith('0'), `the height box starts at 0 (${await box.inputValue()})`);
  await box.fill('0');
  await box.press('Enter');
  s = await until((s) => allFlat(s, 0));
  check(allFlat(s, 0), 'every line is now at height 0');
  check(/Flattened to/.test(await toastText()), `the toast says what it did (${(await toastText()).slice(0, 80)})`);

  // Make faces on the flat plate: the plate with its hole, and the piece in the hole
  await key('ControlOrMeta+a');
  await menuItem('tools.makefaces');
  s = await until((s) => s.faces.length >= 2);
  const areas = s.faces.map((f) => f.area).sort((a, c) => a - c);
  check(s.faces.length === 2 && near(areas[0], 4, 0.05) && near(areas[1], 56, 0.05), `Make faces: the plate with its hole cut out (56 m2) and the piece in the hole (4 m2) (${areas.map((a) => a.toFixed(1))})`);
  check(/Made faces/.test(await toastText()), `the toast says what it made (${(await toastText()).slice(0, 80)})`);

  // each is one Undo
  await key('ControlOrMeta+z');
  s = await until((s) => s.faces.length === 0);
  check(s.faces.length === 0 && allFlat(s, 0), 'one Undo takes the faces back, the lines stay flat');
  await key('ControlOrMeta+z');
  s = await until((s) => !allFlat(s, 0));
  check(!allFlat(s, 0) && edgeSegs(s).length === 9, 'a second Undo takes the flattening back');
});

await scenario('make-faces-skips-bent-loops', async () => {
  const dxf = path.join(ROOT, 'core/tests/data/plate_3d_linework.dxf');
  await fresh((X) => X({ cmd: 'import_dxf', path: dxf }), { at: [5, 3] });
  await tool('select');
  await p.locator('.viewport canvas').click({ position: { x: 850, y: 120 } });
  await key('ControlOrMeta+a');
  await menuItem('tools.makefaces');
  const s = await scene();
  const t = await toastText();
  check(s.faces.length === 0, `loops that are not flat make no face (${s.faces.length} faces)`);
  check(/Flatten/.test(t) && /not flat/.test(t), `and the toast says to flatten first (${t.slice(0, 110)})`);
});

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
