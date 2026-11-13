import { exec } from '../lib.mjs';

// a box cage, and the same cage smoothed (like Blender's Subdivision modifier)
const P = [[-1, -1, 0], [1, -1, 0], [1, 1, 0], [-1, 1, 0], [-1, -1, 1.2], [1, -1, 1.2], [1, 1, 1.2], [-1, 1, 1.2]];
const F = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
const at = (dx) => P.map(([x, y, z]) => [x + dx, y, z]);

export default {
  name: 'subdivision',
  title: 'Subdivision surfaces',
  topic: 'Shape',
  about: 'Model a rough cage of quads and smooth it, the way Blender’s Subdivision modifier does (subd / subdivide).',
  async setup() {
    await exec({ cmd: 'new' });
    await exec({ cmd: 'subd', name: 'Cage', points: at(-3), faces: F, levels: 0 });
    await exec({ cmd: 'subd', name: 'Level 1', points: at(0), faces: F, levels: 1 });
    await exec({ cmd: 'subd', name: 'Level 3', points: at(3), faces: F, levels: 3 });
    await exec({ cmd: 'subd', name: 'Creased base', points: at(6), faces: F, levels: 3, creases: [[0, 1], [1, 2], [2, 3], [3, 0]] });
  },
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'fit' }).first().click();
    await page.waitForTimeout(700);
    await h.caption('A cage of quads (left), smoothed one and three levels');
    await h.shot(2200);
    await h.caption('Crease edges to keep them sharp: a flat base, a soft top');
    await h.shot(2200);
    await h.caption('Box-model a car, a chair or a bottle, then smooth it');
    await h.click(page.locator('.views button', { hasText: 'Iso' }), { after: 1800 });
  },
};
