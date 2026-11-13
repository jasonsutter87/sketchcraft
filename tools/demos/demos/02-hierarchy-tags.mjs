import { townhouseNested } from '../setups.mjs';

export default {
  name: 'hierarchy-tags',
  title: 'Groups, hierarchy and tags',
  topic: 'Organize',
  about: 'Groups inside groups, a click that picks the whole house, hiding a whole chain, and tags with colors and solo.',
  setup: townhouseNested,
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'Notes' }).click();
    await page.keyboard.press('Space');
    await h.caption('The Outliner shows the model as a tree: House › Main floor › walls');
    await h.click(page.locator('.tabs button', { hasText: 'Outliner' }));
    const row = (name) => page.locator('.orow').filter({ has: page.locator('.name', { hasText: new RegExp(`^${name}$`) }) }).first();
    await h.caption('Collapse a floor to tidy the tree');
    await h.click(row('Main floor').locator('.caret'));
    const box = await page.locator('.viewport canvas').boundingBox();
    await h.caption('One click on the model picks the whole House');
    await h.clickAt(box.x + box.width * 0.52, box.y + box.height * 0.55, { after: 1300 });
    await h.caption('Hide the House: everything inside it goes too');
    await row('House').hover();
    await h.click(row('House').getByRole('button', { name: 'Hide' }), { after: 1500 });
    await h.caption('Show it again');
    await h.click(row('House').getByRole('button', { name: 'Show' }), { after: 1000 });
    await h.caption('Tags are layers: color the model by tag…');
    await h.click(page.getByLabel('Color by tag'), { after: 1500 });
    await h.caption('…or solo one tag to see just the walls');
    await h.click(page.getByRole('button', { name: 'Solo Walls' }), { after: 1500 });
    await h.caption('Solo again brings everything back');
    await h.click(page.getByRole('button', { name: 'Solo Walls' }), { after: 1200 });
    await page.getByLabel('Color by tag').click();
  },
};
