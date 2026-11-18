#!/usr/bin/env node
// The format guard (#57), in a real browser: a model file saved by a newer SketchCraft opens read-only, the page says so
// in a bar that stays, changes and saves are refused with a reason, and the file on disk is never touched.
//   SHOT_DIR=/some/folder node tools/ui-tests/read-only.mjs     (SKETCHCRAFT_DOC: the scratch model the bridge uses)
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

const doc = process.env.SKETCHCRAFT_DOC;
if (!doc) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const shots = process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-shots-'));
fs.mkdirSync(shots, { recursive: true });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const NOTICE = "Saved by a newer SketchCraft — update to edit. You can look, but changes won't be saved here.";

const b = await launch();
const p = await (await b.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
const X = (c) => pageExec(p, c);
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] });
await wait(1500);
check((await p.locator('[data-testid=read-only-notice]').count()) === 0, 'a normal model shows no read-only bar');

// a newer SketchCraft saves the model with something this version doesn't know (0.3.8 reads format 2 and sheets, so this
// stands in for whatever comes after it)
const v = JSON.parse(fs.readFileSync(doc, 'utf8'));
v.format = 3;
v.needs = ['holograms'];
v.holograms = { 1: { name: 'H101' } };
await wait(1100);
const bytes = Buffer.from(JSON.stringify(v));
fs.writeFileSync(doc, bytes);

const bar = p.locator('[data-testid=read-only-notice]');
await bar.waitFor({ state: 'visible', timeout: 15000 });
check((await bar.innerText()) === NOTICE, 'the bar says: ' + NOTICE);
await p.screenshot({ path: path.join(shots, 'read-only.png') });
await wait(2500);
check(await bar.isVisible(), 'the bar stays (it is not a toast)');
check((await pageScene(p)).read_only?.length > 0, 'the scene says read-only');

const r = await X({ cmd: 'rect', from: [10, 0, 0], to: [14, 4, 0] });
check(r.ok === false && /newer SketchCraft/.test(r.error) && /read-only/.test(r.error), `drawing is refused with a reason (${r.error})`);
const s = await X({ cmd: 'save', path: path.join(path.dirname(doc), 'elsewhere.sketchcraft') });
check(s.ok === false && /newer SketchCraft/.test(s.error), 'Save is refused');
check(!fs.existsSync(path.join(path.dirname(doc), 'elsewhere.sketchcraft')), 'and writes nothing');
const agent = await pageExec(p, { cmd: 'rect', from: [10, 0, 0], to: [14, 4, 0] }, true);
check(agent.ok === false, 'a plugin or agent is refused too');
await wait(1500);
check(Buffer.compare(fs.readFileSync(doc), bytes) === 0, 'the file on disk is byte-for-byte what the newer program saved');

// starting over ends it
await X({ cmd: 'new' });
await p.waitForFunction(() => !document.querySelector('[data-testid=read-only-notice]'), null, { timeout: 15000 });
check(true, 'New ends read-only: the bar goes away');
check((await X({ cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] })).ok === true, 'and drawing works again');
check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
