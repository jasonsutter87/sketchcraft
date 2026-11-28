// Shared by the feel-* user stories: a page driven with the mouse and keyboard like a SketchUp user would.
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

export async function feel() {

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  let failed = 0;
  const check = (ok, what) => {
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
    if (!ok) failed++;
  };

  const b = await launch();
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1500, height: 950 } });
  await ctx.addInitScript(() => localStorage.setItem('sketchcraft.help', 'off'));
  const p = await ctx.newPage();
  const problems = [];
  p.on('pageerror', (e) => problems.push(String(e)));
  p.on('dialog', (d) => d.dismiss().catch(() => {}));
  const X = (c) => pageExec(p, c);
  const scene = () => pageScene(p);
  const until = async (pred, ms = 2500) => {
    let s = await scene();
    for (let t = 0; t < ms && !pred(s); t += 100) (await wait(100), (s = await scene()));
    return s;
  };
  const near = (a, b, tol = 0.05) => Math.abs(a - b) <= tol;
  const zTop = (s) => Math.max(...s.faces.flatMap((f) => f.outer.map((v) => v[2])), 0);

  const camera = (eye, target) =>
    p.evaluate(([eye, target]) => {
      const T = window.__sketchcraftView.current;
      T.controls.target.set(...target);
      T.persp.position.set(...eye);
      T.persp.up.set(0, 0, 1);
      T.persp.lookAt(...target);
      T.controls.update();
    }, [eye, target]);
  const plan = (cx = 0, cy = 0) => camera([cx, cy - 0.0005, 40], [cx, cy, 0]);
  const iso = (t = [0, 0, 0.5]) => camera([t[0] + 7, t[1] - 8, t[2] + 6], t);
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
  const click = async (pt, n = 1) => {
    await move(pt);
    const [x, y] = await px(pt);
    await p.mouse.click(x, y, { clickCount: n });
    await wait(n > 1 ? 350 : 160);
  };
  const key = async (k) => (await p.keyboard.press(k), await wait(150));
  const type = async (text, enter = true) => {
    await p.keyboard.type(text, { delay: 15 });
    if (enter) await p.keyboard.press('Enter');
    await wait(300);
  };
  // the tool that is on (one tucked behind "+N more" shows on the More button, named in its title)
  const activeTool = async () => {
    const b = p.locator('nav.toolbar button.tool.on').first();
    return (await b.getAttribute('aria-label').catch(() => null)) || (await b.getAttribute('title').catch(() => null));
  };
  /** an operation is in progress: the Value box names what it measures */
  const busy = () => p.locator('.measure label:not(.idle)').count().then((n) => n > 0);
  const fresh = async (build) => {
    await key('Escape');
    await X({ cmd: 'new' });
    await build?.();
    await p.reload();
    await p.waitForSelector('.viewport canvas');
    await wait(900);
    await plan();
    await wait(200);
    await p.mouse.move(5, 500);
  };
  const story = async (name, fn) => {
    console.log(`\n## ${name}`);
    try {
      await fn();
    } catch (e) {
      check(false, `${name}: stopped early (${String(e.message ?? e).split('\n')[0]})`);
    }
  };
  const instances = async () => (await scene()).instances?.length ?? 0;

  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas');
  await wait(900);
  const done = async () => {
    console.log(problems.length ? `page errors:\n${problems.join('\n')}` : 'no page errors');
    console.log(failed ? `\n${failed} feel gaps` : '\nfeels like SketchUp');
    await b.close();
    process.exit(failed ? 1 : 0);
  };
  return { p, X, scene, until, near, zTop, camera, plan, iso, px, move, click, key, type, activeTool, busy, fresh, story, instances, check, wait, done };
}
