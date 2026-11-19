import { blank } from '../setups.mjs';

export default {
  name: 'shelves-cut-list',
  title: 'Plugins: shelves and a cut list',
  topic: 'Plugins',
  about: 'The Shelves plugin builds a bookcase from a few sizes; the Cut list plugin lists the boards and what to buy.',
  setup: blank,
  async steps(h) {
    const { page } = h;
    await h.caption('Plugins add commands: File › Plugins, or Shift+S');
    await h.click(page.getByRole('menuitem', { name: 'File', exact: true }));
    await h.click(page.getByRole('menuitem', { name: /^Plugins/ }), { after: 1200 });
    await h.caption('Shelf unit… asks for a few sizes');
    await h.click(page.getByRole('menuitem', { name: 'Shelf unit' }));
    const form = page.locator('.plugin-form');
    await form.waitFor();
    const field = (label) => form.locator('label', { hasText: new RegExp(`^${label}`) }).locator('input');
    await field('Width').fill('1.2 m');
    await field('Shelves between').fill('4');
    await h.shot(1400);
    await h.caption('Build: every board is its own group');
    await h.click(form.getByRole('button', { name: 'Build' }), { after: 400 });
    await page.waitForTimeout(1500);
    await page.locator('button', { hasText: /^\s*Iso\s*$/i }).first().click();
    await page.waitForTimeout(400);
    await page.locator('button', { hasText: /^\s*Fit\s*$/i }).first().click();
    await page.waitForTimeout(900);
    await h.shot(1500);
    await h.caption('Cut list: the parts to cut, and the sheets and boards to buy');
    await page.keyboard.press('Shift+S');
    await page.waitForTimeout(200);
    await page.keyboard.type('cut list');
    await h.shot(700);
    await page.keyboard.press('Enter');
    await page.locator('.plugin-result').waitFor();
    await page.waitForTimeout(400);
    await h.shot(2600);
    await h.caption('Copy the table, or save it as CSV');
    await h.shot(1400);
    await page.getByRole('button', { name: 'Close' }).last().click();
  },
};
