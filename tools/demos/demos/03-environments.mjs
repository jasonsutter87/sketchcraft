import { townhouse } from '../setups.mjs';

export default {
  name: 'environments',
  title: 'Environments',
  topic: 'Present',
  about: 'Put a model in a ready-made setting (suburban lot, city block, park, desert, waterfront) in one click.',
  setup: townhouse,
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'Notes' }).click();
    await h.caption('Library › Environments: settings sized to your model');
    await h.click(page.locator('.tabs button', { hasText: 'Library' }));
    await page.locator('summary', { hasText: 'Environments' }).click();
    await page.locator('.env-tile').first().scrollIntoViewIfNeeded();
    const fit = page.locator('.views button', { hasText: 'fit' }).first();
    for (const [name, line] of [['Suburban lot', 'A suburban lot: lawn, fence, sidewalk, street'], ['City block', 'A city block, neighbours and all'], ['Park', 'A park with paths and a pond'], ['Desert', 'Desert sand, rocks and cacti'], ['Waterfront', 'A waterfront: boardwalk, beach and pier']]) {
      await h.caption(line);
      await h.click(page.locator('.env-tile', { hasText: name }), { after: 500 });
      await fit.click();
      await page.waitForTimeout(700);
      await h.shot(1500);
    }
    await h.caption('Swap any time: it stays one locked group, and ⌘Z takes it back');
    await h.shot(1500);
  },
};
