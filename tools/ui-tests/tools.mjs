#!/usr/bin/env node
// Every tool on the tool bar, driven with real pointer and key events in a real browser, and the model checked
// through the bridge after each step. Needs the dev page and bridge on a scratch model (docs/dev/building.md):
//   node tools/ui-tests/run.mjs tools                 (starts them, runs this, stops them)
//   ONLY=line,rect node tools/ui-tests/run.mjs tools  (only the scenarios that cover these tools)
//
// How it works. Each scenario starts a new model, builds what it needs by sending commands to the bridge from the
// page (pageExec), and RELOADS the page: the dev page does not notice changes another client makes to the live
// model (it re-reads the model after its own commands and when another *program saves the file*), so a model built
// by commands sent straight to the bridge only shows after a reload. Then the camera is put above the drawing
// (a plan view, 20 px to the metre) so a world point is a screen point, and the tool is driven by the mouse and
// keyboard like a person would. Nothing here calls the engine to do what the tool should do: the engine is only
// asked what the tool did.
//
// A check that every tool has a scenario reads the tool list from the app's source (app/src/viewport/types.ts and
// workspace.ts) and fails when a tool is added without a test: the suite grows with the app.
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

// ---- the tool list, from the app's source -----------------------------------------------------------------------
const src = (f) => fs.readFileSync(path.join(ROOT, 'app/src', f), 'utf8');
const toolUnion = src('viewport/types.ts').match(/export type Tool =([^;]*);/s)[1];
const TOOLS = [...toolUnion.matchAll(/"([a-z0-9]+)"/g)].map((m) => m[1]);
const INFO = Object.fromEntries([...src('workspace.ts').matchAll(/^ {2}([a-z0-9]+): \{ name: "([^"]*)", key: "([^"]*)" \},?$/gm)].map((m) => [m[1], { name: m[2], key: m[3] }]));
const covered = new Set();

// ---- the browser ------------------------------------------------------------------------------------------------
const ONLY = (process.env.ONLY ?? '').split(',').filter(Boolean);
const b = await launch();
const problems = [];
const allTools = Object.keys(INFO);
const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1700, height: 1000 } });
// every tool on the tool bar, in one group (the Model workspace shows most; Arch and the sandbox have the rest)
await ctx.addInitScript((tools) => {
  try {
    localStorage.setItem('sketchcraft.workspaceEdits', JSON.stringify({ model: { tools: [tools] } }));
    localStorage.setItem('sketchcraft.help', 'off');
  } catch {}
}, allTools);
const p = await ctx.newPage();
p.on('pageerror', (e) => problems.push(String(e)));
p.on('dialog', (d) => d.dismiss().catch(() => {}));

const X = (c) => pageExec(p, c);
const scene = () => pageScene(p);
/** poll the scene until `pred` holds (or 2.5 s pass); returns the last scene */
const until = async (pred, ms = 2500) => {
  let s = await scene();
  for (let t = 0; t < ms && !pred(s); t += 100) {
    await wait(100);
    s = await scene();
  }
  return s;
};

// geometry from the scene
const near = (a, b, tol = 0.15) => Math.abs(a - b) <= tol;
const pt3 = (a, b, tol = 0.15) => a.length === 3 && b.every((v, i) => near(a[i], v, tol));
const bbox = (f) => {
  const o = f.outer;
  return { x0: Math.min(...o.map((v) => v[0])), x1: Math.max(...o.map((v) => v[0])), y0: Math.min(...o.map((v) => v[1])), y1: Math.max(...o.map((v) => v[1])), z0: Math.min(...o.map((v) => v[2])), z1: Math.max(...o.map((v) => v[2])) };
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
/** every edge as [[x,y,z],[x,y,z]] (the scene lists them flat) */
const edgeSegs = (s) => {
  const e = s.edges;
  const out = [];
  for (let i = 0; i + 5 < e.length; i += 6) out.push([[e[i], e[i + 1], e[i + 2]], [e[i + 3], e[i + 4], e[i + 5]]]);
  return out;
};
const len = (a, b) => Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
const sel = async () => {
  const t = await p.locator('.section-title', { hasText: /selected|^Face \d|^Edge \d|^Group|^Component/ }).first().innerText({ timeout: 800 }).catch(() => '');
  return t;
};

// ---- driving the page ---------------------------------------------------------------------------------------------
/** the camera: from `eye` to `target`, as a plan view by default (20 px to the metre around `at`) */
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
      // the page was still reloading: wait for it and try again
      if (i >= 4) throw e;
      await p.waitForLoadState('load').catch(() => {});
      await wait(500);
    }
  }
};
const plan = (cx = 0, cy = 0) => camera([cx, cy - 0.0005, 60], [cx, cy, 0]);
const iso = (t = [0, 0, 1]) => camera([t[0] + 12, t[1] - 14, t[2] + 10], t);
const cam = () => p.evaluate(() => { const T = window.__sketchcraftView.current; return { eye: T.camera.position.toArray(), target: T.controls.target.toArray(), fov: T.persp.fov }; });
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
const click = async (pt, opts) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.click(x, y, opts);
  await wait(160);
};
const dblclick = async (pt) => {
  await move(pt);
  const [x, y] = await px(pt);
  await p.mouse.dblclick(x, y);
  await wait(300);
};
/** press at `a`, move through `via`, release at the last point */
const drag = async (pts, { button = 'left', mods = [] } = {}) => {
  await move(pts[0]);
  for (const m of mods) await p.keyboard.down(m);
  await p.mouse.down({ button });
  for (const q of pts.slice(1)) await move(q, 5);
  await p.mouse.up({ button });
  for (const m of mods) await p.keyboard.up(m);
  await wait(250);
};
const key = async (k) => {
  await p.keyboard.press(k);
  await wait(120);
};
/** type into the Value box and press Enter */
const typed = async (text) => {
  await p.keyboard.type(text, { delay: 15 });
  await p.keyboard.press('Enter');
  await wait(250);
};
const holding = async (mods, fn) => {
  for (const m of mods) await p.keyboard.down(m);
  try {
    await fn();
  } finally {
    for (const m of mods) await p.keyboard.up(m);
  }
};

/** the toolbar button for a tool (by its name), through "+N more" if it does not fit */
const tool = async (id) => {
  const name = id === 'plugin:framing' ? 'Stud wall (plugin)' : INFO[id].name;
  const btn = p.locator(`nav.toolbar button[aria-label="${name}"]`);
  if (!(await btn.count())) {
    await p.locator('nav.toolbar button.more').click();
    await p.locator('.more-tools button[role=option]', { hasText: name.split(':')[0].split(' (')[0] }).first().click();
  } else await btn.first().click();
  await wait(150);
};
// the name of the tool that is on (a tool hidden behind "+N more" shows on the More button, named in its title)
const activeTool = async () => {
  const b = p.locator('nav.toolbar button.tool.on').first();
  const label = await b.getAttribute('aria-label').catch(() => null);
  if (label) return label;
  const title = await b.getAttribute('title').catch(() => '');
  return Object.values(INFO).find((i) => title?.startsWith(i.name))?.name ?? title;
};

/** a new model, built by `build` (commands from the page), shown, with the plan view at (cx, cy) */
const fresh = async (build, { at = [0, 0], view = 'plan' } = {}) => {
  await key('Escape');
  await X({ cmd: 'new' });
  const ids = (await build?.(X)) ?? {};
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  if (view === 'plan') await plan(...at);
  await wait(250);
  return ids;
};
const sh = (name) => (process.env.SHOT_DIR ? p.screenshot({ path: path.join(process.env.SHOT_DIR, `tools-${name}.png`) }).catch(() => {}) : null);

/** one scenario: it covers the tools it names; a failure inside it is reported, not thrown */
const scenario = async (name, covers, fn) => {
  for (const c of covers) covered.add(c);
  if (ONLY.length && !covers.some((c) => ONLY.includes(c))) return;
  const before = failed;
  try {
    await fn();
  } catch (e) {
    check(false, `${name}: stopped early (${String(e.message ?? e).split('\n')[0]})`);
    await sh(`stopped-${name.replace(/\W+/g, '-')}`);
  }
  if (failed > before) await sh(`failed-${name.replace(/\W+/g, '-')}`);
};

await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
await wait(900);

const RECT = (x0, y0, x1, y1, z = 0) => ({ cmd: 'rect', from: [x0, y0, z], to: [x1, y1, z] });

// =====================================================================================================================
// drawing: line, rectangle, rotated rectangle, circle, polygon
// =====================================================================================================================
await scenario('line', ['line'], async () => {
  await fresh();
  await tool('line');
  check((await activeTool()) === INFO.line.name, 'the Line button turns the tool on');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  let s = await until((s) => edgeSegs(s).length >= 1);
  const e = edgeSegs(s);
  check(e.length === 1 && ((pt3(e[0][0], [0, 0, 0]) && pt3(e[0][1], [6, 0, 0])) || (pt3(e[0][1], [0, 0, 0]) && pt3(e[0][0], [6, 0, 0]))), `two clicks draw one edge from (0,0) to (6,0) (${JSON.stringify(e)})`);
  // keep going: the next click starts where the last ended; closing the loop makes a face
  await click([6, 4, 0]);
  await click([0, 4, 0]);
  await click([0, 0, 0]);
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && near(area(s.faces[0]), 24, 0.3), `a closed loop of four lines makes a face of 24 m² (${s.faces.length} faces, ${s.faces[0] ? area(s.faces[0]).toFixed(1) : '-'} m²)`);

  // typed length: the direction comes from the mouse, the length from the keyboard
  await fresh();
  await tool('line');
  await click([0, 0, 0]);
  await move([3, 0.02, 0]);
  await typed('5');
  s = await until((s) => edgeSegs(s).length >= 1);
  check(edgeSegs(s).length === 1 && near(len(...edgeSegs(s)[0]), 5, 0.02), `a typed 5 draws a 5 m line (${edgeSegs(s).map((q) => len(...q).toFixed(3))})`);

  // Esc cancels the line in progress; a double-click cancels it too
  await fresh();
  await tool('line');
  await click([0, 0, 0]);
  await move([4, 3, 0]);
  await key('Escape');
  await click([8, 3, 0]);
  await click([8, -3, 0]);
  s = await until((s) => edgeSegs(s).length >= 1);
  check(edgeSegs(s).length === 1 && near(len(...edgeSegs(s)[0]), 6, 0.2), `Esc drops the line in progress: the next two clicks make the only edge (${edgeSegs(s).length})`);
  await fresh();
  await tool('line');
  await click([0, 0, 0]);
  await dblclick([5, 0, 0]);
  await wait(300);
  s = await scene();
  check(edgeSegs(s).length <= 1, `a double-click ends the line (${edgeSegs(s).length} edges)`);

  // an arrow key locks the direction to an axis: → is red (x)
  await fresh();
  await tool('line');
  await click([0, 0, 0]);
  await move([5, 3, 0]);
  await key('ArrowRight');
  await click([5, 3, 0]);
  s = await until((s) => edgeSegs(s).length >= 1);
  const [a, c] = edgeSegs(s)[0] ?? [[0, 0, 0], [0, 0, 0]];
  check(edgeSegs(s).length === 1 && near(c[1], 0, 0.05) && near(c[0], 5, 0.3), `→ locks to the red axis: the end is (${c.map((v) => v.toFixed(2))})`);
  // Shift holds the direction the inference was showing
  await fresh();
  await tool('line');
  await click([0, 0, 0]);
  await move([6, 0.04, 0]);
  await holding(['Shift'], async () => {
    await move([7, 2.5, 0]);
    await click([7, 2.5, 0]);
  });
  s = await until((s) => edgeSegs(s).length >= 1);
  const [, d2] = edgeSegs(s)[0] ?? [[0, 0, 0], [0, 9, 0]];
  check(edgeSegs(s).length === 1 && near(d2[1], 0, 0.1), `Shift holds the direction: the end stays on the red axis (${d2.map((v) => v.toFixed(2))})`);
});

await scenario('rect', ['rect'], async () => {
  await fresh();
  await tool('rect');
  await click([0, 0, 0]);
  await click([10, 5, 0]);
  let s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && near(area(s.faces[0]), 50, 0.5), `two clicks draw a 10 by 5 rectangle (${s.faces.length} faces, ${s.faces[0] ? area(s.faces[0]).toFixed(1) : '-'} m²)`);
  // typed: width, height
  await fresh();
  await tool('rect');
  await click([0, 0, 0]);
  await move([3, 2, 0]);
  await typed('6,4');
  s = await until((s) => s.faces.length === 1);
  const bx = s.faces[0] && bbox(s.faces[0]);
  check(s.faces.length === 1 && near(bx.x1 - bx.x0, 6, 0.01) && near(bx.y1 - bx.y0, 4, 0.01), `typed 6,4 makes a 6 by 4 rectangle (${bx ? `${(bx.x1 - bx.x0).toFixed(2)} x ${(bx.y1 - bx.y0).toFixed(2)}` : 'none'})`);
  // toward the other corner: the sign of the mouse position picks the side
  await fresh();
  await tool('rect');
  await click([0, 0, 0]);
  await move([-3, -2, 0]);
  await typed('6,4');
  s = await until((s) => s.faces.length === 1);
  const b2 = s.faces[0] && bbox(s.faces[0]);
  check(s.faces.length === 1 && near(b2.x0, -6, 0.01) && near(b2.y0, -4, 0.01), `typed 6,4 toward the lower left goes the other way (${b2 ? `x ${b2.x0.toFixed(1)}..${b2.x1.toFixed(1)}` : 'none'})`);
  // drag: press, pull, release finishes it
  await fresh();
  await tool('rect');
  await drag([[0, 0, 0], [4, 4, 0], [8, 3, 0]]);
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && near(area(s.faces[0]), 24, 0.5), `press, drag, release draws a rectangle (${s.faces[0] ? area(s.faces[0]).toFixed(1) : 'none'} m²)`);
  // Esc cancels
  await fresh();
  await tool('rect');
  await click([0, 0, 0]);
  await move([4, 4, 0]);
  await key('Escape');
  await click([10, 10, 0]);
  await wait(300);
  s = await scene();
  check(s.faces.length === 0, `Esc cancels the rectangle in progress, and the next click only starts another (${s.faces.length} faces)`);
  // a rectangle drawn on a face is drawn in that face's plane
  await fresh((X) => X(RECT(0, 0, 8, 8)));
  await tool('rect');
  await click([2, 2, 0]);
  await click([5, 5, 0]);
  s = await until((s) => s.faces.length >= 2);
  check(s.faces.length === 2 && s.faces.every((f) => f.outer.every((v) => near(v[2], 0, 1e-6))), `a rectangle drawn on a face splits it in the face's plane (${s.faces.length} faces)`);
});

await scenario('rrect', ['rrect'], async () => {
  await fresh();
  await tool('rrect');
  await click([0, 0, 0]);
  await click([6, 3, 0]);
  await move([4, 6, 0]);
  await typed('2');
  const s = await until((s) => s.faces.length === 1);
  const f = s.faces[0];
  const sides = f ? f.outer.map((v, i) => len(v, f.outer[(i + 1) % f.outer.length])).map((x) => +x.toFixed(2)).sort((a, c) => a - c) : [];
  check(!!f && f.outer.length === 4 && near(sides[0], 2, 0.02) && near(sides[3], Math.hypot(6, 3), 0.05), `a rotated rectangle: one side along (0,0)-(6,3), the other typed 2 (sides ${sides})`);
  // clicking the second side instead of typing
  await fresh();
  await tool('rrect');
  await click([0, 0, 0]);
  await click([8, 0, 0]);
  await click([8, 3, 0]);
  const s2 = await until((s) => s.faces.length === 1);
  check(s2.faces.length === 1 && near(area(s2.faces[0]), 24, 1), `clicking the second side draws 8 by 3 (${s2.faces[0] ? area(s2.faces[0]).toFixed(1) : 'none'} m²)`);
});

await scenario('circle', ['circle'], async () => {
  await fresh();
  await tool('circle');
  await click([0, 0, 0]);
  await click([3, 0, 0]);
  let s = await until((s) => s.faces.length === 1);
  const f = s.faces[0];
  check(!!f && f.outer.length === 24 && near(Math.max(...f.outer.map((v) => Math.hypot(v[0], v[1]))), 3, 0.15), `a circle of radius 3 with the default 24 sides (${f ? `${f.outer.length} sides` : 'none'})`);
  await fresh();
  await tool('circle');
  await click([0, 0, 0]);
  await move([2, 0, 0]);
  await typed('5');
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && near(Math.max(...s.faces[0].outer.map((v) => Math.hypot(v[0], v[1]))), 5, 0.01), 'a typed 5 sets the radius');
  // "8s": sides
  await fresh();
  await tool('circle');
  await click([0, 0, 0]);
  await move([2, 0, 0]);
  await typed('8s');
  await typed('2');
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && s.faces[0].outer.length === 8, `typing 8s then 2 makes an 8-sided circle of radius 2 (${s.faces[0]?.outer.length} sides)`);
});

await scenario('polygon', ['polygon'], async () => {
  await fresh();
  await tool('polygon');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  let s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && s.faces[0].outer.length === 6, `a polygon has 6 sides by default (${s.faces[0]?.outer.length})`);
  await fresh();
  await tool('polygon');
  await click([0, 0, 0]);
  await move([3, 0, 0]);
  await typed('5s');
  await typed('3');
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && s.faces[0].outer.length === 5 && near(Math.max(...s.faces[0].outer.map((v) => Math.hypot(v[0], v[1]))), 3, 0.01), `5s then 3: a pentagon of radius 3 (${s.faces[0]?.outer.length} sides)`);
});

// =====================================================================================================================
// arcs, pie, freehand
// =====================================================================================================================
await scenario('arc', ['arc'], async () => {
  await fresh();
  await tool('arc');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  await click([3, 2, 0]);
  let s = await until((s) => edgeSegs(s).length > 2);
  let ys = edgeSegs(s).flatMap(([a, c]) => [a[1], c[1]]);
  check(edgeSegs(s).length > 2 && near(Math.max(...ys.map(Math.abs)), 2, 0.4), `2-point arc: ends, then the bulge: many short edges, 2 m high (${edgeSegs(s).length} edges, height ${Math.max(...ys.map(Math.abs)).toFixed(2)})`);
  await fresh();
  await tool('arc');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  await move([3, 1, 0]);
  await typed('1.5');
  s = await until((s) => edgeSegs(s).length > 2);
  ys = edgeSegs(s).flatMap(([a, c]) => [a[1], c[1]]);
  check(edgeSegs(s).length > 2 && near(Math.max(...ys.map(Math.abs)), 1.5, 0.05), `a typed bulge of 1.5 (${Math.max(...ys.map(Math.abs)).toFixed(3)})`);
});

await scenario('arccenter', ['arccenter'], async () => {
  await fresh();
  await tool('arccenter');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  await move([0, 4, 0], 12);
  await click([0, 4, 0]);
  let s = await until((s) => edgeSegs(s).length > 2);
  const pts = edgeSegs(s).flatMap(([a, c]) => [a, c]);
  const rs = pts.map((v) => Math.hypot(v[0], v[1]));
  check(edgeSegs(s).length > 2 && near(Math.min(...rs), 4, 0.1) && near(Math.max(...rs), 4, 0.1), `center arc: every point is 4 m from the center (${edgeSegs(s).length} edges)`);
  check(pts.some((v) => pt3(v, [4, 0, 0], 0.15)) && pts.some((v) => pt3(v, [0, 4, 0], 0.3)), 'and it runs from (4,0) to about (0,4)');
  await fresh();
  await tool('arccenter');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  await move([2, 2, 0]);
  await typed('90');
  s = await until((s) => edgeSegs(s).length > 2);
  const ends = edgeSegs(s).flatMap(([a, c]) => [a, c]);
  check(edgeSegs(s).length > 2 && ends.some((v) => pt3(v, [0, 4, 0], 0.05)), 'a typed 90 swings the arc a quarter turn, to (0,4)');
});

await scenario('arc3', ['arc3'], async () => {
  await fresh();
  await tool('arc3');
  await click([0, 0, 0]);
  await click([3, 2, 0]);
  await click([6, 0, 0]);
  const s = await until((s) => edgeSegs(s).length > 2);
  const ends = edgeSegs(s).flatMap(([a, c]) => [a, c]);
  check(edgeSegs(s).length > 2 && ends.some((v) => pt3(v, [0, 0, 0], 0.15)) && ends.some((v) => pt3(v, [6, 0, 0], 0.15)) && Math.max(...ends.map((v) => Math.abs(v[1]))) > 1.5, `3-point arc: start, a point on it, end (${edgeSegs(s).length} edges)`);
});

await scenario('pie', ['pie'], async () => {
  await fresh();
  await tool('pie');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  await move([0, 4, 0], 12);
  await click([0, 4, 0]);
  const s = await until((s) => s.faces.length === 1);
  const f = s.faces[0];
  check(s.faces.length === 1 && near(area(f), Math.PI * 16 / 4, 0.8), `a pie: a quarter circle face of about ${(Math.PI * 4).toFixed(1)} m² (${f ? area(f).toFixed(1) : 'none'})`);
});

await scenario('freehand', ['freehand'], async () => {
  await fresh();
  await tool('freehand');
  await drag([[0, 0, 0], [3, 0, 0], [6, 0, 0], [6, 2, 0], [6, 4, 0], [3, 4, 0], [0, 4, 0], [0, 2, 0], [0, 0, 0]]);
  let s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1 && near(area(s.faces[0]), 24, 1.5), `dragging round and back to the start closes a freehand shape into a face (${s.faces.length} faces, ${s.faces[0] ? area(s.faces[0]).toFixed(1) : '-'} m²)`);
  await fresh();
  await tool('freehand');
  await drag([[0, 0, 0], [2, 1, 0], [4, 0, 0], [6, 1, 0]]);
  s = await until((s) => edgeSegs(s).length >= 3);
  check(s.faces.length === 0 && edgeSegs(s).length >= 3, `an open freehand drag leaves lines, not a face (${edgeSegs(s).length} edges)`);
});

// =====================================================================================================================
// push/pull, offset, follow me
// =====================================================================================================================
await scenario('push', ['push'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('push');
  await click([2, 2, 0]);
  await typed('2');
  let s = await until((s) => s.faces.length === 6 || s.faces.length === 5);
  const tops = s.faces.map((f) => Math.max(...f.outer.map((v) => v[2])));
  check(s.faces.length >= 5 && Math.max(...tops) === 2 && near(Math.max(...tops), 2, 1e-6), `a typed 2 pulls the face up 2 m into a box (${s.faces.length} faces, top at ${Math.max(...tops)})`);
  // push/pull is undone as one step
  await key('Control+z');
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1, `Ctrl+Z takes the pull back (${s.faces.length} faces)`);
  // click, move, click
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await iso();
  await tool('push');
  await click([2, 2, 0]);
  await move([2, 2, 3], 8);
  await click([2, 2, 3]);
  s = await until((s) => s.faces.length >= 5);
  check(s.faces.length >= 5 && Math.max(...s.faces.map((f) => bbox(f).z1)) > 0.5, `click, move and click pulls a face to where the mouse was (${s.faces.length} faces, top ${Math.max(...s.faces.map((f) => bbox(f).z1)).toFixed(2)})`);
  // a negative distance pushes down
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('push');
  await click([2, 2, 0]);
  await typed('-1.5');
  s = await until((s) => s.faces.length >= 5);
  check(s.faces.length >= 5 && Math.min(...s.faces.map((f) => bbox(f).z0)) < -1.4, `a typed -1.5 pushes down (${Math.min(...s.faces.map((f) => bbox(f).z0)).toFixed(2)})`);
  // Esc drops a pull in progress
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('push');
  await click([2, 2, 0]);
  await move([2.5, 2.5, 0]);
  await key('Escape');
  await wait(300);
  s = await scene();
  check(s.faces.length === 1, `Esc drops the pull (${s.faces.length} face)`);
  // typing after the pull is done retypes it: 2, then 3
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('push');
  await click([2, 2, 0]);
  await typed('2');
  await until((s) => s.faces.length >= 5);
  await typed('3');
  s = await until((s) => Math.max(...s.faces.map((f) => bbox(f).z1)) > 2.9);
  check(near(Math.max(...s.faces.map((f) => bbox(f).z1)), 3, 1e-6) && s.faces.length >= 5, `typing a new distance straight after retypes the pull (top ${Math.max(...s.faces.map((f) => bbox(f).z1))})`);
});

await scenario('offset', ['offset'], async () => {
  await fresh((X) => X(RECT(0, 0, 8, 8)));
  await tool('offset');
  await click([4, 7.5, 0]);
  await move([4, 5, 0]);
  await typed('1');
  const s = await until((s) => s.faces.length === 2);
  const areas = s.faces.map(area).sort((a, c) => a - c);
  check(s.faces.length === 2 && (near(areas[0], 36, 0.5) || near(areas[0], 36, 0.5)) , `Offset 1 m makes an inner face of 6 by 6 inside the 8 by 8 (areas ${areas.map((x) => x.toFixed(1))})`);
});

await scenario('followme', ['followme'], async () => {
  // a 1 m square profile lying at the foot of a 5 m vertical path
  await fresh(async (X) => {
    await X(RECT(0, 0, 1, 1));
    await X({ cmd: 'line', points: [[0, 0, 0], [0, 0, 5]] });
  }, { view: 'iso' });
  await iso([0.5, 0.5, 2.5]);
  const s0 = await scene();
  const path = edgeSegs(s0).findIndex(([a, c]) => Math.abs(a[2] - c[2]) > 4);
  await tool('select');
  await click([0, 0, 2.5]);
  const picked = await sel();
  check(/Edge|selected/.test(picked), `the path edge is selected first (${picked || 'nothing'})`);
  await tool('followme');
  await click([0.5, 0.5, 0]);
  const s = await until((s) => s.faces.length >= 5);
  const top = Math.max(...s.faces.map((f) => bbox(f).z1));
  check(path >= 0 && s.faces.length >= 5 && near(top, 5, 0.05), `Follow Me sweeps the square along the path into a 5 m column (${s.faces.length} faces, top ${top.toFixed(2)})`);
});

// =====================================================================================================================
// move, rotate, scale
// =====================================================================================================================
await scenario('move', ['move'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('move');
  await click([2, 2, 0]);
  await move([5, 4, 0]);
  await click([5, 4, 0]);
  let s = await until((s) => s.faces[0] && bbox(s.faces[0]).x0 > 1);
  let bx = bbox(s.faces[0]);
  check(near(bx.x0, 3, 0.3) && near(bx.y0, 2, 0.3), `Move: click the face, click where it goes (now at ${bx.x0.toFixed(2)}, ${bx.y0.toFixed(2)})`);
  // typed distance along the direction the mouse is going
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('move');
  await click([2, 2, 0]);
  await move([4, 2.02, 0]);
  await typed('6');
  s = await until((s) => s.faces[0] && bbox(s.faces[0]).x0 > 1);
  bx = bbox(s.faces[0]);
  check(near(bx.x0, 6, 0.02) && near(bx.y0, 0, 0.1), `a typed 6 moves it 6 m the way the mouse points (x0 ${bx.x0.toFixed(2)})`);
  // Option (Alt) copies
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('move');
  await holding(['Alt'], async () => {
    await click([2, 2, 0]);
    await move([8, 2, 0]);
    await click([8, 2, 0]);
  });
  s = await until((s) => s.faces.length === 2);
  check(s.faces.length === 2 && s.faces.some((f) => near(bbox(f).x0, 0, 0.05)) && s.faces.some((f) => near(bbox(f).x0, 6, 0.3)), `Alt-move copies: the original stays and a copy is at x 6 (${s.faces.map((f) => bbox(f).x0.toFixed(1))})`);
  // x3 right after a copy makes an array
  await typed('x3');
  s = await until((s) => s.faces.length >= 4);
  check(s.faces.length === 4, `x3 after a copy makes an array of 3 copies (${s.faces.length} faces)`);
  // Ctrl copies too
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('move');
  await holding(['Control'], async () => {
    await click([2, 2, 0]);
    await move([2, 8, 0]);
    await click([2, 8, 0]);
  });
  s = await until((s) => s.faces.length === 2);
  check(s.faces.length === 2, `Ctrl-move copies too (${s.faces.length} faces)`);
  // Esc cancels
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('move');
  await click([2, 2, 0]);
  await move([6, 2, 0]);
  await key('Escape');
  await click([9, 9, 0]);
  s = await scene();
  check(s.faces.length === 1 && near(bbox(s.faces[0]).x0, 0, 1e-6), 'Esc cancels a move: the face has not moved');
});

await scenario('rotate', ['rotate'], async () => {
  await fresh((X) => X(RECT(0, 0, 6, 2)));
  await tool('select');
  await click([3, 1, 0]);
  await tool('rotate');
  await click([0, 0, 0]);
  await click([3, 0, 0]);
  await move([3, 3, 0]);
  await typed('90');
  const s = await until((s) => s.faces[0] && bbox(s.faces[0]).x1 < 0.5);
  const bx = s.faces[0] && bbox(s.faces[0]);
  check(!!bx && near(bx.x0, -2, 0.02) && near(bx.x1, 0, 0.02) && near(bx.y0, 0, 0.02) && near(bx.y1, 6, 0.02), `Rotate 90 degrees about the origin: the 6 by 2 face now stands along y (x ${bx?.x0.toFixed(2)}..${bx?.x1.toFixed(2)}, y ${bx?.y0.toFixed(2)}..${bx?.y1.toFixed(2)})`);
  // click the angle instead of typing it
  await fresh((X) => X(RECT(0, 0, 6, 2)));
  await tool('select');
  await click([3, 1, 0]);
  await tool('rotate');
  await click([0, 0, 0]);
  await click([3, 0, 0]);
  await move([0, 3, 0], 10);
  await click([0, 3, 0]);
  const s2 = await until((s) => s.faces[0] && bbox(s.faces[0]).x1 < 1);
  check(!!s2.faces[0] && near(bbox(s2.faces[0]).y1, 6, 0.6), `clicking the swing turns it about 90 degrees too (y1 ${s2.faces[0] ? bbox(s2.faces[0]).y1.toFixed(2) : '-'})`);
  // Alt rotates a copy
  await fresh((X) => X(RECT(0, 0, 6, 2)));
  await tool('select');
  await click([3, 1, 0]);
  await tool('rotate');
  // Alt is held for the first click (it chooses a copy); it is let go before typing, as Alt+9 types another character
  await holding(['Alt'], () => click([0, 0, 0]));
  await click([3, 0, 0]);
  await move([3, 3, 0]);
  await typed('90');
  const s3 = await until((s) => s.faces.length === 2);
  check(s3.faces.length === 2, `Alt-rotate leaves the original and turns a copy (${s3.faces.length} faces)`);
});

await scenario('scale', ['scale'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('select');
  await click([2, 2, 0]);
  await tool('scale');
  await click([4, 4, 0]);
  await typed('2');
  const s = await until((s) => s.faces[0] && area(s.faces[0]) > 20);
  const f = s.faces[0];
  check(!!f && near(area(f), 64, 0.5) && near(bbox(f).x1 - bbox(f).x0, 8, 0.02), `a typed 2 doubles the face: 4 by 4 becomes 8 by 8 (${f ? area(f).toFixed(1) : '-'} m²)`);
  await key('Control+z');
  const s2 = await until((s) => s.faces[0] && area(s.faces[0]) < 20);
  check(near(area(s2.faces[0]), 16, 0.1), 'Ctrl+Z puts the size back');
  // dragging to the right makes it bigger
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('select');
  await click([2, 2, 0]);
  await tool('scale');
  await click([4, 2, 0]);
  await move([8, 2, 0], 8);
  await click([8, 2, 0]);
  const s3 = await until((s) => s.faces[0] && area(s.faces[0]) > 20);
  check(!!s3.faces[0] && area(s3.faces[0]) > 20, `moving the mouse away from the center, then a click, scales it up (${s3.faces[0] ? area(s3.faces[0]).toFixed(1) : '-'} m²)`);
});

// =====================================================================================================================
// measuring and annotating: tape, protractor, dimension, text, axes
// =====================================================================================================================
await scenario('tape', ['tape'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('tape');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  let s = await until((s) => (s.guides ?? []).length >= 1);
  check((s.guides ?? []).length === 1 && pt3(s.guides[0].p, [4, 0, 0], 0.1), `Tape Measure: two clicks leave a guide at the second point (${JSON.stringify(s.guides?.[0])})`);
  // typing the real length resizes the model (after saying so)
  await typed('8');
  await p.waitForSelector('.sheet-backdrop, [role=dialog]', { timeout: 3000 }).catch(() => {});
  const resize = p.getByRole('button', { name: 'Resize', exact: true });
  check(await resize.isVisible().catch(() => false), 'a typed length asks before resizing the model');
  await resize.click().catch(() => {});
  s = await until((s) => s.faces[0] && area(s.faces[0]) > 40);
  check(!!s.faces[0] && near(area(s.faces[0]), 64, 0.5), `confirming makes the 4 m edge 8 m: the model doubles (${s.faces[0] ? area(s.faces[0]).toFixed(1) : '-'} m²)`);
  // a guide along an edge, from one click on the edge and one off it
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('tape');
  await click([1, 0, 0]); // on the edge, but not at its middle (that would snap to the midpoint)
  await click([1, -3, 0]);
  s = await until((s) => (s.guides ?? []).length >= 1);
  check((s.guides ?? []).length === 1 && s.guides[0].dir != null && near(Math.abs(s.guides[0].dir[1]), 0, 0.01), `Tape Measure: a click on an edge, then one away, leaves a guide along that edge (${JSON.stringify(s.guides?.[0])})`);
});

await scenario('protractor', ['protractor'], async () => {
  await fresh();
  await tool('protractor');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  await move([3, 3, 0]);
  await typed('45');
  const s = await until((s) => (s.guides ?? []).length >= 1);
  const g = (s.guides ?? [])[0];
  check(!!g && pt3(g.p, [0, 0, 0], 0.1) && pt3(g.dir, [Math.SQRT1_2, Math.SQRT1_2, 0], 0.01), `Protractor: center, base, a typed 45 leaves a guide through the center at 45 degrees (${JSON.stringify(g)})`);
});

await scenario('dimension', ['dimension'], async () => {
  await fresh();
  await tool('dimension');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  await move([3, -3, 0]);
  await click([3, -3, 0]);
  const s = await until((s) => (s.annotations ?? []).length >= 1);
  const a = (s.annotations ?? [])[0];
  check((s.annotations ?? []).length === 1 && a.kind === 'dimension' && pt3(a.a, [0, 0, 0], 0.1) && pt3(a.b, [6, 0, 0], 0.1) && near(a.offset[1], -3, 0.3), `Dimension: click, click, pull out: one dimension from (0,0) to (6,0), pulled out 3 m (${JSON.stringify(a)})`);
});

await scenario('text', ['text'], async () => {
  await fresh();
  await tool('text');
  await click([1, 1, 0]);
  await click([4, 4, 0]);
  const box = p.locator('.sheet input, [role=dialog] input').first();
  await box.waitFor({ timeout: 3000 }).catch(() => {});
  check(await box.isVisible().catch(() => false), 'Text: a point and a place ask for the label text');
  await box.fill('Kitchen');
  await key('Enter');
  const s = await until((s) => (s.annotations ?? []).length >= 1);
  const t = (s.annotations ?? [])[0];
  check((s.annotations ?? []).length === 1 && t.kind === 'text' && t.text === 'Kitchen' && pt3(t.leader, [1, 1, 0], 0.1) && pt3(t.at, [4, 4, 0], 0.1), `the label is in the model: "Kitchen", pointing at (1,1), placed at (4,4) (${JSON.stringify(t)})`);
});

await scenario('axes', ['axes'], async () => {
  await fresh();
  await tool('axes');
  await click([2, 2, 0]);
  await click([6, 2, 0]);
  await click([2, 6, 0]);
  const s = await until((s) => s.axes != null);
  check(s.axes != null, `Axes: origin, red direction, green direction moves the drawing axes (${JSON.stringify(s.axes)?.slice(0, 100)})`);
});

// =====================================================================================================================
// paint, eraser, select
// =====================================================================================================================
await scenario('paint', ['paint'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  const before = (await scene()).colors.slice(0, 3);
  await tool('paint');
  await p.locator('.swatch, .swatches button').nth(8).click().catch(() => {});
  await click([2, 2, 0]);
  let s = await until((s) => s.colors.slice(0, 3).some((v, i) => Math.abs(v - before[i]) > 0.01));
  const now = s.colors.slice(0, 3);
  check(now.some((v, i) => Math.abs(v - before[i]) > 0.01), `Paint Bucket: a click changes the face's color (${before.map((v) => v.toFixed(2))} to ${now.map((v) => v.toFixed(2))})`);
  // Shift paints every face with the same material
  await fresh(async (X) => {
    await X(RECT(0, 0, 3, 3));
    await X(RECT(6, 0, 9, 3));
  });
  await tool('paint');
  await p.locator('.swatch, .swatches button').nth(9).click().catch(() => {});
  await holding(['Shift'], () => click([1.5, 1.5, 0]));
  const rgb = (s, i) => s.colors.slice(i * 3, i * 3 + 3);
  s = await until((s) => s.colors.length >= 6 && s.triFace.length >= 2 && rgb(s, 0).some((v, i) => Math.abs(v - 0.95) > 0.02 || i > 0));
  const unpainted = [0.95, 0.94, 0.91];
  const fcolors = s.faces.map((f) => rgb(s, s.triFace.indexOf(f.id)));
  check(fcolors.length === 2 && fcolors.every((c) => c.some((v, i) => Math.abs(v - unpainted[i]) > 0.05)) && fcolors[0].every((v, i) => near(v, fcolors[1][i], 0.01)), `Shift-click paints every face with the same material: both squares are now ${fcolors.map((c) => c.map((v) => v.toFixed(2)))}`);
  // Ctrl-click paints every connected face: one click on a box colors all six
  await fresh(async (X) => {
    const f = (await X(RECT(0, 0, 4, 4))).face;
    await X({ cmd: 'pushpull', face: f, distance: 2 });
  }, { view: 'iso' });
  await iso([2, 2, 1]);
  await tool('paint');
  await p.locator('.swatch, .swatches button').nth(9).click().catch(() => {});
  await holding(['Control'], () => click([2, 2, 2]));
  s = await until((s) => s.faces.length === 6 && s.faces.every((f) => f.material));
  const boxMats = new Set(s.faces.map((f) => f.material));
  check(s.faces.length === 6 && s.faces.every((f) => f.material) && boxMats.size === 1, `Ctrl-click paints all six faces of a box with one material (${s.faces.length} faces, materials ${[...boxMats]})`);
});

await scenario('erase', ['erase'], async () => {
  await fresh(async (X) => {
    await X({ cmd: 'line', points: [[0, 0, 0], [6, 0, 0]] });
    await X(RECT(0, 4, 4, 8));
  });
  await tool('erase');
  await click([3, 0, 0]);
  let s = await until((s) => edgeSegs(s).length <= 4);
  check(edgeSegs(s).every(([a, c]) => !(near(a[1], 0, 0.01) && near(c[1], 0, 0.01))), 'Eraser: a click on an edge deletes it');
  await click([2, 6, 0]);
  s = await until((s) => s.faces.length === 0);
  check(s.faces.length === 0, `a click on a face deletes it (${s.faces.length} faces)`);
  await key('Control+z');
  s = await until((s) => s.faces.length === 1);
  check(s.faces.length === 1, 'Ctrl+Z brings it back');
  // Shift hides instead of deleting
  await holding(['Shift'], () => click([2, 6, 0]));
  s = await until((s) => s.faces.length === 0 || s.faces[0].hidden);
  check(s.faces.length === 0 || s.faces[0].hidden === true, `Shift-click hides the face (${s.faces.length} visible)`);
  // Alt softens an edge of a box
  await fresh(async (X) => {
    const f = (await X(RECT(0, 0, 4, 4))).face;
    await X({ cmd: 'pushpull', face: f, distance: 2 });
  }, { view: 'iso' });
  await iso([2, 2, 1]);
  await tool('erase');
  await holding(['Alt'], () => click([4, 0, 1]));
  s = await until((s) => (s.softEdges ?? []).length > 0);
  check((s.softEdges ?? []).length > 0, `Alt softens an edge (${(s.softEdges ?? []).length} soft)`);
});

await scenario('select', ['select'], async () => {
  await fresh(async (X) => {
    await X(RECT(0, 0, 3, 3));
    await X(RECT(6, 0, 9, 3));
    await X(RECT(12, 0, 15, 3));
  }, { at: [7.5, 0] });
  await tool('select');
  await click([1.5, 1.5, 0]);
  check(/^Face|1 selected/.test(await sel()), `Select: a click picks a face (${await sel()})`);
  await holding(['Shift'], () => click([7.5, 1.5, 0]));
  check(/^2 selected|Faces/.test(await sel()) || /2/.test(await sel()), `Shift-click adds one (${await sel()})`);
  await key('Escape');
  await click([7.5, 8, 0]);
  // a window dragged from left to right takes only what it fully holds
  await drag([[-1, -2, 0], [10, 5, 0]]);
  const two = await sel();
  check(/^(10|11|12|13|14|15|16) selected|^[0-9]+ selected/.test(two) && Number(two.split(' ')[0]) >= 10 && Number(two.split(' ')[0]) <= 16, `a window over two squares selects them and their edges (${two})`);
  await click([7.5, 8, 0]);
  // a crossing box, dragged right to left, takes what it touches
  await drag([[14, 5, 0], [8, -2, 0]]);
  const cross = await sel();
  check(Number(cross.split(' ')[0]) >= 6, `a crossing box (dragged right to left) takes whatever it touches, even partly: two faces and the edges with an end inside (${cross})`);
  await key('Control+a');
  const all = await sel();
  check(Number(all.split(' ')[0]) >= 3, `Ctrl+A selects everything (${all})`);
  await key('Escape');
  await click([7.5, 8, 0]);
  check(!(await sel()) || /^0|^$/.test(await sel()), 'a click on nothing clears it');
});

// =====================================================================================================================
// building: wall, stairs, roof, column
// =====================================================================================================================
await scenario('wall', ['wall'], async () => {
  await fresh();
  await tool('wall');
  await click([-3, -3, 0]);
  await click([3, -3, 0]);
  await click([3, 3, 0]);
  await dblclick([-3, 3, 0]);
  const s = await until((s) => s.faces.length > 0);
  check(s.faces.length > 0 && s.groups.length >= 1, `Wall: click the corners, double-click the last (${s.faces.length} faces, ${s.groups.length} group)`);
  // a typed length, then Enter: a wall of that length
  await fresh();
  await tool('wall');
  await click([0, 0, 0]);
  await move([3, 0.02, 0]);
  await typed('4');
  await key('Enter');
  const s2 = await until((s) => s.faces.length > 0);
  const xs = s2.faces.flatMap((f) => f.outer.map((v) => v[0]));
  check(s2.faces.length > 0 && near(Math.max(...xs) - Math.min(...xs), 4, 0.4), `a typed 4 then Enter builds a 4 m wall (${s2.faces.length} faces, ${(Math.max(...xs) - Math.min(...xs)).toFixed(2)} m long)`);
  // Esc drops a wall that is still being drawn
  await fresh();
  await tool('wall');
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  await move([4, 3, 0]);
  await key('Escape');
  await wait(300);
  const s3 = await scene();
  check(s3.faces.length === 0, `Esc drops a wall in progress: nothing was built (${s3.faces.length} faces)`);
});

await scenario('wallpull', ['wallpull'], async () => {
  // a room built by command; the full drawing, dragging, typing, Escape and Undo story is tools/ui-tests/wall-pull.mjs
  await fresh((X) => X({ cmd: 'wall', points: [[-3, -3, 0], [3, -3, 0], [3, 3, 0], [-3, 3, 0]], closed: true, thickness: 0.2, height: 2.7 }));
  const before = await scene();
  const west0 = Math.min(...before.faces.flatMap((f) => f.outer.map((v) => v[0])));
  await tool('wallpull');
  check((await activeTool()) === INFO.wallpull.name, 'the Pull wall button turns the tool on');
  await click([-3.05, 0, 2.7]);
  await typed('1');
  const s = await until((s) => Math.min(...s.faces.flatMap((f) => f.outer.map((v) => v[0]))) < west0 - 0.5);
  check(near(Math.min(...s.faces.flatMap((f) => f.outer.map((v) => v[0]))), west0 - 1, 0.001), 'Pull wall: a click on a wall and a typed 1 moves the whole wall a metre out');
});

await scenario('room', ['room'], async () => {
  // the full story (a second room sharing a wall, labels, pulling it, a door, Undo, refusals) is tools/ui-tests/room-mode.mjs
  await fresh();
  await tool('room');
  check((await activeTool()) === INFO.room.name, 'the Room button turns the tool on');
  await drag([[-3, -2, 0], [0, 0, 0], [3, 2, 0]]);
  const s = await until((s) => s.faces.length > 0 && s.groups.some((g) => g.tag === 'Rooms'));
  const room = s.groups.find((g) => g.tag === 'Rooms');
  check(!!room && /^Room 1$/.test(room.name), `a press, drag and release draws a room, named "${room?.name}"`);
  const xs = s.faces.flatMap((f) => f.outer.map((v) => v[0]));
  check(near(Math.max(...xs) - Math.min(...xs), 6.2, 0.4) && s.faces.some((f) => f.normal && f.normal[2] > 0.99 && f.outer.every((v) => near(v[2], 0, 0.001))), 'about 6 m wide with a floor at the ground');
});

await scenario('stairs', ['stairs'], async () => {
  await fresh();
  await tool('stairs');
  await click([0, 0, 0]);
  await click([6, 0, 0]);
  const s = await until((s) => s.faces.length > 4);
  check(s.faces.length > 10, `Stairs: click the bottom, then the way up (${s.faces.length} faces)`);
  const top = Math.max(...s.faces.map((f) => bbox(f).z1));
  check(top > 1, `and they climb (top ${top.toFixed(2)} m)`);
});

await scenario('roof', ['roof'], async () => {
  await fresh(async (X) => {
    const f = (await X(RECT(0, 0, 8, 6))).face;
    await X({ cmd: 'pushpull', face: f, distance: 3 });
  });
  await tool('roof');
  await click([4, 3, 3]);
  const s = await until((s) => s.faces.length > 6);
  const top = Math.max(...s.faces.map((f) => bbox(f).z1));
  check(s.faces.length > 6 && top > 3.3, `Roof: a click on a flat top adds a roof (${s.faces.length} faces, highest ${top.toFixed(2)} m)`);
});

await scenario('column', ['column'], async () => {
  await fresh();
  await tool('column');
  await click([3, 2, 0]);
  const s = await until((s) => s.faces.length >= 5);
  const xs = s.faces.flatMap((f) => f.outer.map((v) => v[0]));
  check(s.faces.length >= 5 && near((Math.min(...xs) + Math.max(...xs)) / 2, 3, 0.2), `Column: a click drops one there (${s.faces.length} faces around x 3)`);
  check(Math.max(...s.faces.map((f) => bbox(f).z1)) > 1, 'and it stands up');
});

// =====================================================================================================================
// section, terrain
// =====================================================================================================================
await scenario('section', ['section'], async () => {
  await fresh(async (X) => {
    const f = (await X(RECT(0, 0, 4, 4))).face;
    await X({ cmd: 'pushpull', face: f, distance: 3 });
  }, { view: 'iso' });
  await iso([2, 2, 1.5]);
  await tool('section');
  await click([4, 2, 1.5]);
  const s = await until((s) => (s.sections ?? []).length >= 1);
  check((s.sections ?? []).length === 1, `Section: a click on a face cuts there (${(s.sections ?? []).length} plane)`);
});

const terrain = async (X) => ({ g: (await X({ cmd: 'terrain_grid', at: [-10, -10, 0], width: 20, depth: 20, spacing: 2 })).group });
await scenario('smoove', ['smoove'], async () => {
  await fresh(terrain);
  const z0 = Math.max(...(await scene()).positions.filter((_, i) => i % 3 === 2));
  await tool('smoove');
  await click([0, 0, 0]);
  const s = await until((s) => Math.max(...s.positions.filter((_, i) => i % 3 === 2)) > z0 + 0.01);
  check(Math.max(...s.positions.filter((_, i) => i % 3 === 2)) > z0 + 0.01, `Smoove: a click raises the terrain there (top ${Math.max(...s.positions.filter((_, i) => i % 3 === 2)).toFixed(2)} m)`);
  await holding(['Shift'], () => click([6, 6, 0]));
  const s2 = await until((s) => Math.min(...s.positions.filter((_, i) => i % 3 === 2)) < -0.01);
  check(Math.min(...s2.positions.filter((_, i) => i % 3 === 2)) < -0.01, 'Shift-click digs');
});

await scenario('stamp', ['stamp'], async () => {
  await fresh(async (X) => {
    await terrain(X);
    await X(RECT(-2, -2, 2, 2, 2));
  });
  await tool('select');
  await click([0, 0, 2]);
  await tool('stamp');
  await click([7, 7, 0]);
  const s = await until((s) => s.positions.some((v, i) => i % 3 === 2 && Math.abs(v - 2) < 0.05));
  check(s.positions.some((v, i) => i % 3 === 2 && Math.abs(v - 2) < 0.05), 'Stamp: select a footprint above the terrain, click the terrain: a pad at the footprint height');
});

await scenario('drape', ['drape'], async () => {
  await fresh(async (X) => {
    await terrain(X);
    await X({ cmd: 'line', points: [[-6, -3, 3], [6, 3, 3]] });
  });
  const n0 = (await scene()).faces.length;
  await tool('select');
  await click([0, 0, 3]);
  await tool('drape');
  await click([7, -7, 0]);
  const s = await until((s) => s.faces.length > n0);
  check(s.faces.length > n0, `Drape: select a line above the terrain, click the terrain: it is laid on, splitting faces (${n0} to ${s.faces.length})`);
});

await scenario('flipedge', ['flipedge'], async () => {
  // The terrain is a closed group: Flip edge reaches inside it, as the other Sandbox tools do (it used to find nothing
  // until the group was opened for editing)
  await fresh(async (X) => {
    await terrain(X);
  });
  const s0 = await scene();
  // a diagonal between two triangles: an edge that is neither along x nor along y (the grid's diagonals are soft edges)
  const diag = s0.edgeList.find((e) => Math.abs(e.a[0] - e.b[0]) > 0.5 && Math.abs(e.a[1] - e.b[1]) > 0.5 && Math.abs(e.a[0]) < 6 && Math.abs(e.a[1]) < 6);
  const sig = (s) => JSON.stringify(s.edgeList.map((e) => [e.a, e.b]).sort());
  await tool('flipedge');
  await click(diag.a.map((v, i) => (v + diag.b[i]) / 2));
  const s = await until((s) => sig(s) !== sig(s0));
  check(!!diag && sig(s) !== sig(s0), 'Flip edge: a click on a diagonal swaps it for the other diagonal of its square');
});

// =====================================================================================================================
// the camera tools
// =====================================================================================================================
await scenario('orbit', ['orbit'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('orbit');
  const c0 = await cam();
  await drag([[0, 0, 0], [3, 2, 0]]);
  const c1 = await cam();
  const moved = Math.hypot(c1.eye[0] - c0.eye[0], c1.eye[1] - c0.eye[1], c1.eye[2] - c0.eye[2]);
  const s = await scene();
  check(moved > 0.5 && near(Math.hypot(...c1.eye.map((v, i) => v - c1.target[i])), Math.hypot(...c0.eye.map((v, i) => v - c0.target[i])), 0.5), `Orbit: a drag swings the camera round the target at the same distance (moved ${moved.toFixed(1)} m)`);
  check(s.faces.length === 1 && near(area(s.faces[0]), 16, 0.01), 'and leaves the model alone');
});

await scenario('pan', ['pan'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('pan');
  const c0 = await cam();
  await drag([[0, 0, 0], [5, 3, 0]]);
  const c1 = await cam();
  const dt = Math.hypot(c1.target[0] - c0.target[0], c1.target[1] - c0.target[1]);
  check(dt > 1 && near(c1.eye[2], c0.eye[2], 0.5), `Pan: a drag slides the view sideways (target moved ${dt.toFixed(1)} m)`);
});

await scenario('zoom', ['zoom'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('zoom');
  const d = (c) => Math.hypot(...c.eye.map((v, i) => v - c.target[i]));
  const c0 = await cam();
  const [x, y] = await px([0, 0, 0]);
  await p.mouse.move(x, y);
  await p.mouse.down();
  await p.mouse.move(x, y - 150, { steps: 8 });
  await p.mouse.up();
  await wait(300);
  const c1 = await cam();
  check(d(c1) < d(c0) - 1 || c1.fov < c0.fov, `Zoom: dragging up brings the model closer (${d(c0).toFixed(1)} m to ${d(c1).toFixed(1)} m)`);
  await typed('60');
  const c2 = await cam();
  check(near(c2.fov, 60, 0.5), `typing 60 sets the field of view (${c2.fov.toFixed(0)} degrees)`);
});

await scenario('zoomwindow', ['zoomwindow'], async () => {
  await fresh((X) => X(RECT(0, 0, 4, 4)));
  await tool('zoomwindow');
  const d = (c) => Math.hypot(...c.eye.map((v, i) => v - c.target[i]));
  const c0 = await cam();
  await drag([[-2, -2, 0], [3, 2, 0]]);
  const c1 = await cam();
  check(d(c1) < d(c0) * 0.8, `Zoom Window: a dragged box fills the view (${d(c0).toFixed(1)} m to ${d(c1).toFixed(1)} m away)`);
});

await scenario('position', ['position', 'lookaround'], async () => {
  await fresh((X) => X(RECT(0, 0, 8, 8)));
  await tool('position');
  await click([4, 4, 0]);
  await wait(1200); // the camera glides there
  const c = await cam();
  check(near(c.eye[2], 1.68, 0.1) && near(c.eye[0], 4, 0.6) && near(c.eye[1], 4, 0.6), `Position Camera: a click stands you there at eye height (${c.eye.map((v) => v.toFixed(2))})`);
  check((await activeTool()) === INFO.lookaround.name, 'and switches to Look Around');
  const t0 = c.target;
  await p.mouse.move(800, 500);
  await p.mouse.down();
  await p.mouse.move(1000, 500, { steps: 8 });
  await p.mouse.up();
  await wait(300);
  const c1 = await cam();
  const turned = Math.hypot(c1.target[0] - t0[0], c1.target[1] - t0[1]);
  check(turned > 0.2 && near(c1.eye[0], c.eye[0], 0.05) && near(c1.eye[1], c.eye[1], 0.05), `Look Around: a drag turns your head without moving you (looked ${turned.toFixed(1)} m aside)`);
});

await scenario('walk', ['walk'], async () => {
  await fresh((X) => X(RECT(0, 0, 8, 8)));
  await tool('position');
  await click([4, 4, 0]);
  await wait(1200);
  await tool('walk');
  const c0 = await cam();
  await p.keyboard.down('w');
  await wait(900);
  await p.keyboard.up('w');
  const c1 = await cam();
  const walked = Math.hypot(c1.eye[0] - c0.eye[0], c1.eye[1] - c0.eye[1]);
  check(walked > 0.3 && near(c1.eye[2], c0.eye[2], 0.2), `Walk: holding W walks forward at eye height (${walked.toFixed(1)} m)`);
});

// =====================================================================================================================
// Place from the Library, and a plugin's tool
// =====================================================================================================================
await scenario('place', ['place'], async () => {
  await fresh();
  await p.locator('.tabs button', { hasText: 'Library' }).first().click();
  await wait(500);
  await p.locator('.tile-hit').first().click();
  await wait(300);
  check((await p.locator('.mode-pill', { hasText: /Placing/ }).count()) === 1, 'Place: a Library tile puts the app in Place mode');
  await click([3, 3, 0]);
  const s = await until((s) => s.faces.length > 0 || s.instances.length > 0 || s.groups.length > 0);
  check(s.faces.length > 0 || s.instances.length > 0 || s.groups.length > 0, `a click in the model places it there (${s.faces.length} faces, ${s.instances.length} instances, ${s.groups.length} groups)`);
  const at = s.instances[0]?.transform?.slice(12, 15);
  check(!!at && pt3(at, [3, 3, 0], 0.3), `at the point clicked (${at?.map((v) => v.toFixed(2))})`);
  await key('Escape');
  await wait(200);
  check((await p.locator('.mode-pill', { hasText: /Placing/ }).count()) === 0, 'Esc leaves Place mode');
});

await scenario('plugin', ['plugin'], async () => {
  await fresh();
  const btn = p.locator('nav.toolbar button[aria-label="Stud wall (plugin)"]');
  await btn.waitFor({ timeout: 6000 }).catch(() => {});
  check((await btn.count()) === 1, 'the Framing plugin puts its Stud wall tool on the tool bar');
  await btn.click();
  const draw = p.getByRole('button', { name: 'Draw walls' });
  await draw.waitFor({ timeout: 5000 }).catch(() => {});
  check(await draw.isVisible().catch(() => false), 'it asks for the wall sizes (a form from the plugin, marked as one)');
  check(/built-in plugin/.test(await p.locator('.plugin-banner').innerText().catch(() => '')), 'and the form says whose it is (a built-in plugin)');
  await draw.click();
  await wait(500);
  await click([0, 0, 0]);
  await click([4, 0, 0]);
  // every board is a command of its own: a slow machine (CI) takes a while, so wait for the count to reach what it should,
  // and say how long it took so a slowdown shows up in the log rather than as a flaky failure
  const t0 = Date.now();
  const s = await until((s) => s.groups.length >= 14, 120000);
  console.log(`note: the stud wall took ${((Date.now() - t0) / 1000).toFixed(1)} s for ${s.groups.length} groups`);
  // 4 m at 0.4064 m: a bottom plate, a double top plate and eleven studs
  check(s.groups.length >= 12, `two clicks build a stud wall: plates and studs, every board its own group (${s.groups.length} groups)`);
  await key('Escape');
});

/** a close-up of whatever is in the model, from the front: it fills the view */
const closeUp = async () => {
  const pos = (await scene()).positions;
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < pos.length; i += 3) for (let k = 0; k < 3; k++) (lo[k] = Math.min(lo[k], pos[i + k]), (hi[k] = Math.max(hi[k], pos[i + k])));
  const mid = lo.map((v, k) => (v + hi[k]) / 2);
  const size = Math.hypot(...hi.map((v, k) => v - lo[k]));
  await camera([mid[0], mid[1] - size * 1.1, mid[2] + size * 0.11], mid);
  await wait(400);
  return mid;
};
/** a drag across the middle of the model, in screen pixels */
const strokeAcross = async (mid) => {
  const [cx, cy] = await px(mid);
  await p.mouse.move(cx - 50, cy + 5);
  await p.mouse.down();
  await p.mouse.move(cx, cy - 5, { steps: 10 });
  await p.mouse.move(cx + 50, cy + 5, { steps: 10 });
  await p.mouse.up();
  await wait(900);
};

await scenario('vpaint', ['vpaint'], async () => {
  let inst;
  await fresh(async (X) => {
    inst = (await X({ cmd: 'insert', asset: 'globe', at: [0, 0, 0] })).instance;
  }, { view: 'none' });
  const mid = await closeUp();
  const cornersOf = async () => Object.values((await X({ cmd: 'vertex_colors', faces: [inst] })).colors ?? {}).flatMap((v) => v ?? []);
  const painted = (cs) => cs.filter((c) => c.some((x) => x < 0.999)).length;
  check(painted(await cornersOf()) === 0, 'Vertex Paint: the placed globe starts with no painted corners');
  await tool('vpaint');
  check(await p.locator('[data-testid="vertex-paint-panel"]').isVisible().catch(() => false), 'the tool opens its brush panel');
  await p.locator('.viewport canvas').hover();
  await strokeAcross(mid);
  const hit = painted(await cornersOf());
  check(hit > 0, `a drag across the model paints corners (${hit})`);
  await key('Control+z');
  await wait(400);
  check(painted(await cornersOf()) === 0, 'one Undo takes the whole stroke back');
  await key('Escape');
  check(painted(await cornersOf()) === 0, 'Esc changes nothing in the model');
  await tool('select');
  check(!(await p.locator('[data-testid="vertex-paint-panel"]').isVisible().catch(() => false)), 'leaving the tool closes its brush panel');
});

await scenario('tpaint', ['tpaint'], async () => {
  await fresh(async (X) => {
    const ins = await X({ cmd: 'insert', asset: 'crate', at: [0, 0, 0] });
    await X({ cmd: 'explode', id: ins.instance });
  }, { view: 'none' });
  const mid = await closeUp();
  await tool('select');
  await p.locator('.viewport canvas').click({ position: { x: 850, y: 500 } });
  await key('ControlOrMeta+a');
  await tool('tpaint');
  const panel = p.locator('[data-testid="texture-paint-panel"]');
  check(await panel.isVisible().catch(() => false), 'Texture Paint: the tool opens its panel');
  await panel.getByTestId('tp-create').click();
  await panel.getByTestId('tp-needs').waitFor({ timeout: 5000 }).catch(() => {});
  // a base of the faces' own material, so a stroke in the brush colour shows on it
  await panel.getByTestId('tp-newbase').selectOption('material');
  await panel.getByTestId('tp-unwrap-new').click();
  const s = await until((s) => s.materials.some((m) => m.name === 'Paint 1' && m.texture), 8000);
  check(s.materials.some((m) => m.name === 'Paint 1' && m.texture), 'Unwrap and create makes a paint texture (Paint 1)');
  const pic = () => X({ cmd: 'texture_pixels', material: 'Paint 1', x: 0, y: 0, w: 1, h: 1 });
  const h0 = (await pic()).hash;
  await p.locator('.viewport canvas').hover();
  await strokeAcross(mid);
  const h1 = (await pic()).hash;
  check(h1 !== h0, 'a drag across the model changes the picture');
  await key('Control+z');
  await wait(700);
  check((await pic()).hash === h0, 'one Undo puts every pixel back');
  await key('Escape');
  check((await pic()).hash === h0, 'Esc changes nothing in the picture');
  await tool('select');
  check(!(await panel.isVisible().catch(() => false)), 'leaving the tool closes its panel');
});

// ---- the whole list: every tool must have a scenario -------------------------------------------------------------
{
  // `plugin` is the tool a plugin button turns on; the scenario above covers it
  const need = [...new Set([...TOOLS, ...allTools])];
  const missing = need.filter((t) => !covered.has(t));
  check(need.length >= 40, `the tool list was read from the app source (${need.length} tools)`);
  if (!ONLY.length) check(missing.length === 0, missing.length ? `every tool has a test: MISSING ${missing.join(', ')}` : `every tool has a test (${need.length} tools)`);
}

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
