// Starting models for the demos, built with engine commands (so they always
// match the current engine).

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { exec, ROOT, runJsonl } from './lib.mjs';

export async function townhouse() {
  await exec({ cmd: 'new' });
  await runJsonl(path.join(ROOT, 'examples/townhouse/townhouse.jsonl'));
}

/** The townhouse organized: House › Main floor / Upper floor, Site; tags. */
export async function townhouseNested() {
  await townhouse();
  const info = await exec({ cmd: 'info' });
  const g = Object.fromEntries(info.groups.map((x) => [x.name, x.id]));
  const has = (n, ks) => ks.some((k) => n.toLowerCase().includes(k));
  const names = Object.keys(g);
  const main = names.filter((n) => has(n, ['main walls', 'suite', 'bath1', 'wic', 'garage', 'laundry', 'foyer', 'pantry', 'stairs', 'kitchen', 'range', 'refrigerator', 'island', 'vanity', 'shower', 'washer', 'dryer', 'toilet tank 1', 'toilet 1'])).map((n) => g[n]);
  const upper = names.filter((n) => has(n, ['second floor', 'upper walls', 'bedroom wall', 'bed divider', 'bath2', 'stair rail', 'bathtub', 'toilet tank 2', 'toilet 2', 'closet', 'roof'])).map((n) => g[n]);
  const site = ['Foundation', 'Driveway', 'Front walk'].filter((n) => g[n]).map((n) => g[n]);
  const mf = (await exec({ cmd: 'group', ids: main, name: 'Main floor' })).group;
  const uf = (await exec({ cmd: 'group', ids: upper, name: 'Upper floor' })).group;
  const house = (await exec({ cmd: 'group', ids: [mf, uf], name: 'House' })).group;
  const st = (await exec({ cmd: 'group', ids: site, name: 'Site' })).group;
  const tag = (ids, t) => exec({ cmd: 'tag', ids, tag: t });
  await tag([g['Main walls'], g['Upper walls']].filter(Boolean), 'Walls');
  await tag([st], 'Site');
  await tag(names.filter((n) => has(n, ['kitchen', 'range', 'refrigerator', 'island', 'vanity', 'shower', 'washer', 'dryer', 'toilet', 'bathtub'])).map((n) => g[n]), 'Fixtures');
  await tag(names.filter((n) => has(n, ['roof'])).map((n) => g[n]), 'Roof');
  for (const t of ['Walls', 'Site', 'Fixtures', 'Roof']) await exec({ cmd: 'tag_visible', tag: t, visible: true });
  const scene = await fetch('http://127.0.0.1:5175/scene').then((r) => r.json());
  await exec({ cmd: 'set_parent', ids: scene.instances.map((i) => i.id), parent: house });
}

/** The true cameras the demo photos were taken from (Photo B's distance is then guessed 35% long). */
export const PHOTO_CAMS = { 'Photo A': [14, -9, 5], 'Photo B': [-8, -12, 6] };
export const PHOTO_TARGET = [3, 2, 1.5];
export const BOX_CORNERS = [[0, 0, 0], [6, 0, 0], [6, 0, 3], [0, 0, 3], [0, 4, 3], [6, 4, 3]];

/** Two "photos" of a 6 x 4 x 3 m box, matched; the box itself gone. */
export async function photoBox() {
  await exec({ cmd: 'new' });
  const g = (await exec({ cmd: 'begin_group', name: 'Truth' })).group;
  const f = (await exec({ cmd: 'rect', from: [0, 0, 0], to: [6, 4, 0] })).face;
  await exec({ cmd: 'pushpull', face: f, distance: 3, toward: [0, 0, 1] });
  await exec({ cmd: 'end_group' });
  await exec({ cmd: 'paint', ids: [g], color: [0.85, 0.7, 0.55] });
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'demo-photos-'));
  for (const [name, eye] of Object.entries(PHOTO_CAMS)) {
    const out = path.join(tmp, name.replace(' ', '_') + '.png');
    await exec({ cmd: 'render', camera: { eye, target: PHOTO_TARGET, fov: 45 }, width: 960, height: 720, grid: false, out });
    const photo = 'data:image/png;base64,' + fs.readFileSync(out).toString('base64');
    const k = name === 'Photo B' ? 1.35 : 1;
    const e = eye.map((x) => x * k);
    const target = PHOTO_TARGET.map((t, i) => t + (e[i] - eye[i]));
    await exec({ cmd: 'save_view', name, eye: e, target, fov: 45, photo });
  }
  await exec({ cmd: 'delete', ids: [g] });
}

/** A lacquered stool in the studio. */
export async function studioStool() {
  await exec({ cmd: 'new' });
  const g = (await exec({ cmd: 'begin_group', name: 'Stool' })).group;
  const f = (await exec({ cmd: 'circle', center: [0, 0, 0], radius: 0.2, segments: 32 })).face;
  await exec({ cmd: 'pushpull', face: f, distance: 0.45, toward: [0, 0, 1] });
  await exec({ cmd: 'end_group' });
  await exec({ cmd: 'material', name: 'Red lacquer', color: [0.75, 0.12, 0.1] });
  await exec({ cmd: 'paint', ids: [g], material: 'Red lacquer' });
  await exec({ cmd: 'environment', kind: 'studio' });
}
