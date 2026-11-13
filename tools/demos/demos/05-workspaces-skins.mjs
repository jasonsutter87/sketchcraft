import { townhouse } from '../setups.mjs';

export default {
  name: 'workspaces-skins',
  title: 'Workspaces and skins',
  topic: 'Basics',
  about: 'Arch, Design and Previz arrange the tools for the job; skins change the whole look, and you can make your own.',
  setup: townhouse,
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'Notes' }).click();
    await h.caption('Workspaces arrange the app for the job');
    await h.click(page.getByRole('tab', { name: 'Arch' }));
    await h.caption('Arch: walls, stairs and roofs, plans by floor, blueprints');
    await h.click(page.locator('.ws-strip button', { hasText: 'Upper' }), { after: 1500 });
    await h.click(page.locator('.ws-strip button', { hasText: '3D' }));
    await h.caption('Design: solids, bevels and rounds; the toolbar down the side');
    await h.click(page.getByRole('tab', { name: 'Design' }));
    await h.caption('Previz: scenes, sun and photoreal renders');
    await h.click(page.getByRole('tab', { name: 'Previz' }));
    await h.caption('Skins change the whole look');
    await h.click(page.getByRole('button', { name: 'Skins' }));
    await h.click(page.locator('.skin-card', { hasText: 'Cassette Deck' }));
    await h.click(page.getByRole('button', { name: 'Done' }), { after: 1600 });
    await h.caption('…edit any skin\'s colors and share it as a .sketchskin file');
    await h.click(page.getByRole('button', { name: 'Skins' }));
    await h.click(page.locator('.skin-card', { hasText: 'Workshop' }));
    await h.click(page.getByRole('button', { name: 'Done' }));
    await h.click(page.getByRole('tab', { name: 'Model' }));
  },
};
