#!/usr/bin/env node
// Capture demo GIFs of SketchCraft features from the running app.
//
//   node tools/demos/capture.mjs all            every demo
//   node tools/demos/capture.mjs photo-modeling environments
//   node tools/demos/capture.mjs --list
//
// Starts the bridge and dev page if they aren't running (localhost only),
// plays each demo in tools/demos/demos/ in a headless browser, and writes
// docs/demos/<name>.gif + .mp4 and app/public/learn/<name>.gif, plus
// app/public/learn/demos.json (what the in-app Learn center shows).

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { encode, launch, PAGE, recorder, ROOT, servers } from './lib.mjs';

const dir = path.join(ROOT, 'tools/demos/demos');
const all = [];
for (const f of fs.readdirSync(dir).filter((f) => f.endsWith('.mjs')).sort()) all.push((await import(path.join(dir, f))).default);

const args = process.argv.slice(2);
if (args.includes('--list') || !args.length) {
  for (const d of all) console.log(`${d.name.padEnd(20)} ${d.title}: ${d.about}`);
  if (!args.length) console.log('\nnode tools/demos/capture.mjs all | <name> ...');
  process.exit(0);
}
const pick = args.includes('all') ? all : all.filter((d) => args.includes(d.name));
if (!pick.length) throw new Error(`no demo named ${args.join(', ')} (see --list)`);

const stop = await servers();
const browser = await launch();
const learn = path.join(ROOT, 'app/public/learn');
const docs = path.join(ROOT, 'docs/demos');
const manifestFile = path.join(learn, 'demos.json');
const manifest = fs.existsSync(manifestFile) ? JSON.parse(fs.readFileSync(manifestFile, 'utf8')) : {};
try {
  for (const d of pick) {
    const t0 = Date.now();
    await d.setup();
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 760 }, deviceScaleFactor: 1 });
    // the same starting look every time: Workshop skin, Model workspace
    await ctx.addInitScript((ws) => {
      try {
        localStorage.setItem('sketchcraft.skin', 'workshop');
        localStorage.removeItem('sketchcraft.skinData');
        localStorage.setItem('sketchcraft.workspace', ws);
        localStorage.removeItem('sketchcraft.workspaceEdits');
      } catch {
        /* fine */
      }
    }, d.workspace ?? 'model');
    const page = await ctx.newPage();
    await page.goto(PAGE);
    await page.waitForSelector('.viewport canvas', { timeout: 30000 });
    await page.waitForTimeout(2000);
    const frames = fs.mkdtempSync(path.join(os.tmpdir(), `demo-${d.name}-`));
    const h = recorder(page, frames);
    await d.steps(h);
    await h.caption('');
    await h.shot(1600);
    await ctx.close();
    const gif = path.join(learn, `${d.name}.gif`);
    await encode(h.frames, gif, { mp4: false });
    await encode(h.frames, path.join(docs, `${d.name}.gif`), { mp4: true });
    const kb = Math.round(fs.statSync(gif).size / 1024);
    manifest[d.name] = { title: d.title, about: d.about, topic: d.topic, gif: `learn/${d.name}.gif`, frames: h.frames.length, captured: new Date().toISOString().slice(0, 10) };
    console.log(`${d.name}: ${h.frames.length} frames, ${kb} KB, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  }
} finally {
  fs.mkdirSync(learn, { recursive: true });
  fs.writeFileSync(manifestFile, JSON.stringify(manifest, null, 2));
  await browser.close();
  stop();
}
