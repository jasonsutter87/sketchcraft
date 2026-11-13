---
name: capture-demos
description: Re-record SketchCraft's feature demo GIFs (and MP4s) from the real app after the UI changes, or add a demo for a new feature. Use when the user asks to capture, record, refresh or redo demos, GIFs, screencasts or the Learn center animations, or invokes /capture-demos.
---

# Capture demos

Demos are scripts, not recordings: each one sets up its model with engine
commands, drives the real app in a headless browser by **names and labels**
(never screen positions), and leaves frames that become a GIF. When the UI
changes, re-run them; nothing needs re-clicking by hand.

## Run

```sh
cargo build --release                       # the bridge binary the demos talk to
node tools/demos/capture.mjs --list         # what exists
node tools/demos/capture.mjs all            # everything (~2 min)
node tools/demos/capture.mjs photo-modeling environments
```

It starts `sketchcraft bridge` and the Vite dev page on 127.0.0.1 if they
aren't running (and stops what it started). Output:

- `app/public/learn/<name>.gif` and `app/public/learn/demos.json`: the in-app
  Learn center reads these.
- `docs/demos/<name>.gif` and `.mp4`: for the README, releases and posts.

Needs ffmpeg and Playwright (the project's `playwright-core`, or the /design
skill's copy, which `tools/demos/lib.mjs` finds by itself).

## Add a demo

Create `tools/demos/demos/NN-name.mjs`:

```js
import { townhouse } from '../setups.mjs';   // or build a model with exec({cmd: ...})

export default {
  name: 'floor-plans',                 // file names and the Learn center key
  title: 'Floor plans',
  topic: 'Drawings',                   // Learn center section
  about: 'One sentence: what the viewer will see.',
  workspace: 'arch',                   // optional: start in this workspace
  setup: townhouse,
  async steps(h) {
    const { page } = h;
    await h.caption('What to notice right now');
    await h.click(page.getByRole('tab', { name: 'Arch' }));   // ring, frame, click, frame
    await h.click(page.locator('.ws-strip button', { hasText: 'Upper' }), { after: 1500 });
    await h.shot(1200);                                      // hold a frame
  },
};
```

Rules that keep demos durable and good:

1. **Find things by what they say**: `getByRole('button', { name })`,
   `getByLabel`, `locator(sel, { hasText })`. A button's text, aria-label or
   title is its contract; positions are not. If a demo can't find something,
   give the control an accessible name rather than a coordinate.
2. **Set up with engine commands** in `setup` (or `tools/demos/setups.mjs`
   for shared scenes), so the starting model is identical every run.
3. **Caption every beat** in plain words ("Hide the House: everything inside
   it goes too"). One idea per caption.
4. **Keep it short**: 15-50 frames, under ~1.5 MB per GIF. Hold frames
   (`h.shot(ms)`) instead of adding more.
5. Points in the 3D view: compute them (see `01-photo-modeling.mjs`, which
   projects known 3D corners into the photos) or use fractions of the canvas
   box, never fixed pixels.
6. After capturing, look at a frame or two (`ffmpeg -i docs/demos/x.mp4
   -vf select=eq(n\,100) -vframes 1 x.png`) before committing.

## When a demo breaks

The error names the locator that failed. Usually the label changed: update
the demo (or restore a name on the control). Then re-run just that demo.
