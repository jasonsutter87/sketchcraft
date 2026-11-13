import { BOX_CORNERS, PHOTO_CAMS, PHOTO_TARGET, photoBox } from '../setups.mjs';

export default {
  name: 'photo-modeling',
  title: 'Model from photos',
  topic: 'Photos',
  about: 'Mark the same corners in two matched photos; SketchCraft works out where they are in 3D and builds the faces.',
  setup: photoBox,
  async steps(h) {
    const { page } = h;
    await h.caption('Two photos of the same building, already matched (Views › Match photo)');
    await h.click(page.locator('.tabs button', { hasText: 'Views' }));
    await h.click(page.locator('.views-list li', { hasText: 'Photo A' }));
    await h.caption('Model from photos: mark corners you can see in both');
    await h.click(page.getByRole('button', { name: /Model from photos/ }));
    await page.waitForTimeout(800);
    // where each true corner shows in each photo, from the cameras that took them
    const pts = await page.evaluate(({ cams, tgt, corners }) => {
      const frames = [...document.querySelectorAll('.pm-frame')].map((f) => f.getBoundingClientRect());
      const sub = (a, b) => a.map((x, i) => x - b[i]), dot = (a, b) => a.reduce((s, x, i) => s + x * b[i], 0);
      const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
      const norm = (a) => { const l = Math.hypot(...a); return a.map((x) => x / l); };
      return Object.values(cams).map((eye, k) => {
        const f = norm(sub(tgt, eye)), r = norm(cross(f, [0, 0, 1])), u = cross(r, f), t = Math.tan((45 * Math.PI) / 360), R = frames[k];
        return corners.map((p) => { const d = sub(p, eye), z = dot(d, f); return [R.left + R.width / 2 + (dot(d, r) / z / t) * R.height / 2, R.top + R.height / 2 - (dot(d, u) / z / t) * R.height / 2]; });
      });
    }, { cams: PHOTO_CAMS, tgt: PHOTO_TARGET, corners: BOX_CORNERS });
    const [A, B] = pts;
    await h.caption('Click a corner in one photo…');
    await h.clickAt(...A[0]);
    await h.caption('…the dashed line shows where that corner is in the other photo');
    await h.shot(1400);
    await h.clickAt(...B[0]);
    await h.caption('Trace the outline: each new corner connects to the last');
    for (const i of [1, 2, 3]) {
      await h.clickAt(...A[i], { hold: 300, after: 300 });
      await h.clickAt(...B[i], { hold: 300, after: 300 });
    }
    await h.caption('Click corner 1 again to close the face');
    await h.clickAt(...A[0]);
    await h.clickAt(...A[3], { hold: 200, after: 200 });
    await h.caption('Then the top: corners 5 and 6, and back to 3');
    for (const i of [4, 5]) {
      await h.clickAt(...A[i], { hold: 300, after: 300 });
      await h.clickAt(...B[i], { hold: 300, after: 300 });
    }
    await h.clickAt(...A[2]);
    await h.caption('One length you know sets the scale: edge 1–2 is 6 m');
    await h.page.locator('.pm-side input.num').fill('6');
    await h.shot(900);
    await h.caption('Solve: every corner in 3D, every photo at its true distance');
    await h.click(page.getByRole('button', { name: 'Solve and build edges' }), { after: 2200 });
    await h.click(page.getByRole('button', { name: 'Done' }));
    await h.caption('The new faces sit right on the photo');
    await h.click(page.locator('.views-list li', { hasText: 'Photo A' }), { after: 1600 });
    await h.caption('A real 3D model, ready to keep building');
    await h.click(page.locator('.views button', { hasText: 'Iso' }), { after: 1800 });
  },
};
