import { studioStool } from '../setups.mjs';

export default {
  name: 'studio-render',
  title: 'Studio renders',
  topic: 'Present',
  about: 'A seamless studio backdrop with softbox lights, and a photoreal render that sharpens while you watch.',
  setup: studioStool,
  async steps(h) {
    const { page } = h;
    await page.locator('.views button', { hasText: 'fit' }).first().click();
    await page.waitForTimeout(800);
    await h.caption('The Studio environment: a seamless sweep and softbox lights');
    await h.shot(1600);
    await h.caption('Render: lit only by the softboxes');
    await h.click(page.locator('.top-actions .btn.primary'));
    await h.click(page.locator('.render-dlg button', { hasText: /^Render$/ }), { after: 600 });
    await h.caption('It sharpens while you watch; stop whenever it looks right');
    for (let i = 0; i < 6; i++) {
      await page.waitForTimeout(1300);
      await h.shot(700);
    }
    await page.waitForTimeout(3000);
    await h.caption('Done: save it as a PNG, or Sharpen more');
    await h.shot(2000);
  },
};
