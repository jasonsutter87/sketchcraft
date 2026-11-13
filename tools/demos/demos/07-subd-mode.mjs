import { exec } from '../lib.mjs';

// a simple cage: a box, 3 x 2 x 1.2 m
const P = [[-1.5, -1, 0], [1.5, -1, 0], [1.5, 1, 0], [-1.5, 1, 0], [-1.5, -1, 1.2], [1.5, -1, 1.2], [1.5, 1, 1.2], [-1.5, 1, 1.2]];
const F = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];

export default {
  name: 'subd-mode',
  title: 'SubD mode',
  topic: 'Shape',
  about: 'Shape a smooth object by its cage: pick points, edges or faces, drag (the other side follows), add loops, extrude, crease.',
  async setup() {
    await exec({ cmd: 'new' });
    await exec({ cmd: 'subd', name: 'Pebble', points: P, faces: F, levels: 2 });
  },
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'fit' }).first().click();
    await page.waitForTimeout(600);
    await h.caption('A smooth object from a box cage. Select it, press Tab: SubD mode');
    await h.click(page.locator('.tabs button', { hasText: 'Outliner' }));
    await page.locator('.orow', { hasText: 'Pebble' }).click();
    await page.keyboard.press('Tab');
    await page.waitForTimeout(700);
    await h.shot(1500);
    // the cage editor is exposed in dev builds: find points on screen by position
    const at = (fn) => page.evaluate(fn);
    const corner = await at(() => { const ed = window.__cage; const i = ed.pts.findIndex((p) => p[0] > 1 && p[1] < 0 && p[2] > 1); const s = ed.screen(ed.pts[i]); return [s.x, s.y]; });
    await h.caption('Drag a point: the mirrored point on the other side follows');
    await h.clickAt(corner[0], corner[1], { after: 300 });
    await page.mouse.move(corner[0], corner[1]);
    await page.mouse.down();
    for (let k = 1; k <= 6; k++) {
      await page.mouse.move(corner[0] + k * 10, corner[1] - k * 9);
      await page.waitForTimeout(60);
    }
    await page.mouse.up();
    await page.waitForTimeout(700);
    await h.shot(1500);
    await h.caption('L: insert an edge loop where you click an edge');
    await page.keyboard.press('l');
    const edge = await at(() => { const ed = window.__cage; const i = ed.edges.findIndex(([a, b]) => Math.abs(ed.pts[a][2] - ed.pts[b][2]) > 0.5 && ed.pts[a][1] < 0 && ed.pts[a][0] < 0); const [a, b] = ed.edges[i]; const s = ed.screen(ed.pts.map((_, k) => k).includes(a) ? [0, 1, 2].map((k) => (ed.pts[a][k] + ed.pts[b][k]) / 2) : ed.pts[a]); return [s.x, s.y]; });
    await h.clickAt(edge[0], edge[1], { after: 1400 });
    await page.keyboard.press('Escape');
    await h.caption('3 for faces, E to extrude, then drag it out');
    await page.keyboard.press('3');
    const top = await at(() => { const ed = window.__cage; let best = -1, z = -1; ed.data.faces.forEach((f, i) => { const c = f.reduce((s, v) => s + ed.pts[v][2], 0) / f.length; if (c > z) (z = c), (best = i); }); const f = ed.data.faces[best]; const c = [0, 1, 2].map((k) => f.reduce((s, v) => s + ed.pts[v][k], 0) / f.length); const s = ed.screen(c); return [s.x, s.y]; });
    await h.clickAt(top[0], top[1], { after: 500 });
    await page.keyboard.press('e');
    await page.waitForTimeout(900);
    await page.keyboard.press('ArrowUp');
    await page.mouse.move(top[0], top[1]);
    await page.mouse.down();
    for (let k = 1; k <= 6; k++) {
      await page.mouse.move(top[0], top[1] - k * 9);
      await page.waitForTimeout(60);
    }
    await page.mouse.up();
    await page.waitForTimeout(800);
    await h.shot(1600);
    await h.caption('Tab shows just the cage; Enter when you\'re done');
    await page.keyboard.press('Tab');
    await page.waitForTimeout(500);
    await h.shot(1400);
    await page.keyboard.press('Tab');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(600);
    await h.shot(1200);
  },
};
