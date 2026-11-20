#!/usr/bin/env node
// Right-click always opens the context menu (a jittery press, a bare contextmenu event, Shift+F10), a real right-drag
// still pans without one, the menu offers Make group / Make component, and a fresh page starts in the Classic look.
// Runs against the dev page and bridge (docs/dev/building.md), on a scratch model: SKETCHCRAFT_DOC must be set.
//   node tools/ui-tests/run.mjs right-click
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

const b = await launch();
const problems = [];
const open = async (storage = {}) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } });
  await ctx.addInitScript((s) => {
    try {
      for (const [k, v] of Object.entries(s)) localStorage.setItem(k, v);
    } catch {}
  }, storage);
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas');
  await wait(700);
  return { p, ctx };
};
const X = (p, c) => pageExec(p, c);
const screen = (p, [x, y, z]) =>
  p.evaluate(([x, y, z]) => {
    const T = window.__sketchcraftView.current;
    const v = T.camera.position.clone().set(x, y, z).project(T.camera);
    const r = document.querySelector('.viewport canvas').getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }, [x, y, z]);
const reshow = async (p) => {
  await p.reload();
  await p.waitForSelector('.viewport canvas');
  await wait(800);
};
const menus = (p) => p.locator('.menu-scrim .menu').count();
const menuText = (p) => p.locator('.menu-scrim .menu').first().innerText().catch(() => '');
const closeMenu = async (p) => {
  await p.keyboard.press('Escape');
  await wait(250);
};
const target = (p) => p.evaluate(() => window.__sketchcraftView.current.controls.target.toArray());

// ---- a fresh page starts in the Classic look ------------------------------------------------------
{
  const { p, ctx } = await open();
  const sky = await p.evaluate(() => window.__sketchcraftView.current.sky.visible);
  check(sky === true, 'a fresh page shows the sky and ground');
  const bg = await p.locator('.section', { hasText: 'Style' }).locator('select').nth(2).inputValue();
  check(bg === 'sky', `the Background setting reads Sky and ground (${bg})`);
  const prof = await p.getByLabel('Profiles', { exact: true }).isChecked();
  const lw = await p.getByLabel('Line weights', { exact: true }).isChecked();
  check(prof === true && lw === false, `Profiles on, Line weights off (${prof}, ${lw})`);
  // choosing another preset is remembered across a reload
  await p.locator('.section', { hasText: 'Style' }).locator('select').first().selectOption('Blueprint');
  await wait(300);
  await reshow(p);
  const sky2 = await p.evaluate(() => window.__sketchcraftView.current.sky.visible);
  const bg2 = await p.locator('.section', { hasText: 'Style' }).locator('select').nth(2).inputValue();
  check(sky2 === false && bg2 === 'blueprint', `a chosen preset is remembered (${bg2})`);
  await ctx.close();
}

// ---- right-click ---------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await X(p, { cmd: 'new' });
  await X(p, { cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] });
  await reshow(p);
  await p.getByRole('button', { name: 'Select', exact: true }).first().click();
  await p.keyboard.press('Shift+z');
  await wait(700);
  const [fx, fy] = await screen(p, [2, 2, 0]);

  // a plain right-click on a face: menu, one only, with Make group and Make component
  await p.mouse.click(fx, fy, { button: 'right' });
  await wait(400);
  let t = await menuText(p);
  check((await menus(p)) === 1, 'a right-click opens one context menu');
  check(/Make group/.test(t) && /Make component/.test(t), `the menu on a face offers Make group and Make component (${t.replace(/\s+/g, ' ').slice(0, 80)})`);
  await closeMenu(p);
  check((await menus(p)) === 0, 'Escape closes it');

  // jitter: the mouse wobbles 5 px between press and release
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await p.mouse.move(fx + 3, fy + 4, { steps: 2 });
  await p.mouse.up({ button: 'right' });
  await wait(450);
  check((await menus(p)) === 1, 'a right-click with 5 px of jitter still opens the menu');
  await closeMenu(p);

  // a slow press that wobbles a little (a long, still-ish click) opens it too
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await wait(500);
  await p.mouse.move(fx + 2, fy + 2);
  await p.mouse.up({ button: 'right' });
  await wait(450);
  check((await menus(p)) === 1, 'a slow right-click that barely moved opens the menu');
  await closeMenu(p);

  // a contextmenu event with no pointer events at all (a touchpad two-finger tap)
  await p.evaluate(([x, y]) => {
    const c = document.querySelector('.viewport canvas');
    c.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 2 }));
  }, [fx, fy]);
  await wait(450);
  check((await menus(p)) === 1 && /Make group/.test(await menuText(p)), 'a bare contextmenu event opens the menu, on what is under it');
  await closeMenu(p);

  // a real right-drag pans and opens nothing (even with the contextmenu event Windows raises after it)
  await p.mouse.click(fx + 300, fy + 200); // clear the selection
  await wait(300);
  const before = await target(p);
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await p.mouse.move(fx + 40, fy + 10, { steps: 5 });
  await p.mouse.move(fx + 120, fy + 30, { steps: 8 });
  await wait(450);
  await p.mouse.up({ button: 'right' });
  await p.evaluate(([x, y]) => {
    document.querySelector('.viewport canvas').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 2 }));
  }, [fx + 120, fy + 30]);
  await wait(700);
  const after = await target(p);
  check(Math.hypot(...after.map((v, i) => v - before[i])) > 0.05, 'a right-drag pans the view');
  check((await menus(p)) === 0, 'a right-drag opens no menu');

  // Linux/Chromium raises `contextmenu` at the PRESS (before any movement), Windows/macOS at the release.
  // Emulate the Linux order here so every platform covers it: a contextmenu on every right pointerdown.
  await p.evaluate(() => {
    const c = document.querySelector('.viewport canvas');
    window.__linuxCtx = (e) => {
      if (e.button === 2) setTimeout(() => c.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: e.clientX, clientY: e.clientY, button: 2 })), 0); // after the pointerdown handlers, as the browser does
    };
    c.addEventListener('pointerdown', window.__linuxCtx, true);
  });
  await p.mouse.click(fx + 300, fy + 200);
  await wait(300);
  const before2 = await target(p);
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await wait(50);
  await p.mouse.move(fx + 8, fy + 2); // a small first move, well inside the quick-click limit
  await wait(450); // slower than the 300 ms held-button timer
  await p.mouse.move(fx + 60, fy + 20, { steps: 6 });
  await p.mouse.move(fx + 120, fy + 30, { steps: 6 });
  await wait(200);
  await p.mouse.up({ button: 'right' });
  await wait(800);
  const after2 = await target(p);
  check(Math.hypot(...after2.map((v, i) => v - before2[i])) > 0.05, 'Linux order: a right-drag pans the view');
  check((await menus(p)) === 0, 'Linux order: a right-drag opens no menu');
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await wait(50);
  await p.mouse.up({ button: 'right' });
  await wait(450);
  check((await menus(p)) === 1, 'Linux order: a right-click opens exactly one menu');
  await closeMenu(p);
  await p.mouse.move(fx, fy);
  await p.mouse.down({ button: 'right' });
  await wait(500);
  await p.mouse.move(fx + 2, fy + 2);
  await p.mouse.up({ button: 'right' });
  await wait(450);
  check((await menus(p)) === 1, 'Linux order: a slow right-click that barely moved opens one menu');
  await closeMenu(p);
  await p.evaluate(() => document.querySelector('.viewport canvas').removeEventListener('pointerdown', window.__linuxCtx, true));
  await reshow(p);
  await p.getByRole('button', { name: 'Select', exact: true }).first().click();
  await p.keyboard.press('Shift+z');
  await wait(700);

  // Shift+F10 opens the menu for the selection
  await p.mouse.click(fx, fy);
  await wait(400);
  await p.mouse.move(fx, fy);
  await p.keyboard.press('Shift+F10');
  await wait(450);
  t = await menuText(p);
  check((await menus(p)) === 1 && /Make group/.test(t) && /Make component/.test(t), 'Shift+F10 opens the menu for the selection');
  await closeMenu(p);
  await p.keyboard.press('ContextMenu');
  await wait(450);
  check((await menus(p)) === 1, 'the Menu key opens it too');
  await closeMenu(p);

  // Make group from the menu, then right-click the group: Make group is still offered
  await p.mouse.click(fx, fy, { button: 'right' });
  await wait(400);
  await p.locator('.menu-scrim .menu button', { hasText: 'Make group' }).click();
  await wait(700);
  check((await pageScene(p)).groups.length === 1, 'Make group in the right-click menu makes a group');
  await p.mouse.click(fx, fy, { button: 'right' });
  await wait(400);
  check(/Make group/.test(await menuText(p)) && /Make component/.test(await menuText(p)), 'the menu on a group offers Make group and Make component');
  await closeMenu(p);
  await ctx.close();
}

await b.close();
if (problems.length) console.log('page errors:', problems.slice(0, 5));
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
