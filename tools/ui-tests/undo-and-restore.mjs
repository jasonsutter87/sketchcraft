#!/usr/bin/env node
// Per-client undo and Restore from backup, in a real browser against the dev page and bridge
// (docs/dev/building.md), on a scratch model: SKETCHCRAFT_DOC must be set (and the bridge run with the same).
//   SHOT_DIR=/some/folder node tools/ui-tests/undo-and-restore.mjs
// "Another program" is `sketchcraft run --open FILE --save FILE` (set SKETCHCRAFT_BIN if it isn't target/release/sketchcraft).
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { launch, PAGE, pageExec, pageScene, ROOT } from '../demos/lib.mjs';

const doc = process.env.SKETCHCRAFT_DOC;
if (!doc) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const bin = process.env.SKETCHCRAFT_BIN ?? path.join(ROOT, 'target/release/sketchcraft');
const shots = process.env.SHOT_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-shots-'));
fs.mkdirSync(shots, { recursive: true });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const b = await launch();
const p = await (await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 800 } })).newPage();
const problems = [];
p.on('pageerror', (e) => problems.push(String(e)));
await p.goto(PAGE);
await p.waitForSelector('.viewport canvas');
const X = (c) => pageExec(p, c);
const scene = () => pageScene(p);
const lefts = async () => (await scene()).faces.map((f) => Math.round(Math.min(...f.outer.map((v) => v[0])))).sort((a, c) => a - c);
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};
const other = (lines) => {
  const f = path.join(os.tmpdir(), `sc-other-${process.pid}.jsonl`);
  fs.writeFileSync(f, lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
  execFileSync(bin, ['run', f, '--open', doc, '--save', doc], { env: { ...process.env }, stdio: 'pipe' });
  fs.rmSync(f, { force: true });
};

// ---- per-client undo -------------------------------------------------------------------
await X({ cmd: 'new' });
await X({ cmd: 'rect', from: [0, 0, 0], to: [4, 4, 0] });
await wait(1200);
other([{ cmd: 'rect', from: [10, 0, 0], to: [14, 4, 0] }, { cmd: 'rect', from: [20, 0, 0], to: [24, 4, 0] }]);
// the page polls and picks up the other program's save
await p.waitForFunction(async () => true);
await wait(2200);
check(JSON.stringify(await lefts()) === '[0,10,20]', 'the page shows its own rectangle and the other program\'s two');
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(800);
check(JSON.stringify(await lefts()) === '[10,20]', 'Ctrl+Z took back only the person\'s rectangle; the other program\'s two stay');
const onDisk = JSON.parse(fs.readFileSync(doc, 'utf8'));
check(Object.keys(onDisk.faces).length === 2, 'and the file on disk still has theirs');
await p.keyboard.press('Control+Shift+z');
await wait(800);
check(JSON.stringify(await lefts()) === '[0,10,20]', 'redo brings the person\'s rectangle back, theirs untouched');

// R5-BRN-001: the same on ordinary (non-round) numbers: a rectangle at odd coordinates, turned by 33.3 degrees,
// then another program saves; Ctrl+Z takes back the turn, Ctrl+Shift+Z does it again
await X({ cmd: 'new' });
const rr = await X({ cmd: 'rect', from: [5.123456789, 0.7071067811, 0], to: [9.456789012, 4.3217654321, 0] });
await X({ cmd: 'rotate', ids: [rr.face], center: [1.5, 2.5, 0], axis: [0, 0, 1], angle: 33.3 });
const turned = await lefts();
await wait(1200);
other([{ cmd: 'rect', from: [50, 0, 0], to: [54, 4, 0] }]);
await wait(2200);
check(JSON.stringify(await lefts()) === JSON.stringify([...turned, 50]), 'odd-number model: the page picked up the other program\'s rectangle');
await p.locator('.viewport canvas').click({ position: { x: 5, y: 5 } }).catch(() => {});
await p.keyboard.press('Control+z');
await wait(800);
check(JSON.stringify(await lefts()) === '[5,50]', `odd-number model: Ctrl+Z took back the turn only (${JSON.stringify(await lefts())})`);
await p.keyboard.press('Control+Shift+z');
await wait(800);
check(JSON.stringify(await lefts()) === JSON.stringify([...turned, 50].sort((a, c) => a - c)), 'odd-number model: redo turns it again');

// ---- Restore from backup ---------------------------------------------------------------
await X({ cmd: 'new' });
await wait(300);
await X({ cmd: 'rect', from: [30, 0, 0], to: [34, 4, 0] });
// "before New" backups are paced (one per 15 s unless the model's size doubles or halves; core/src/live.rs): wait out
// the pacing from the New above, or the backup of this model is skipped and the test restores an older one
await wait(16000);
await X({ cmd: 'new' });
await wait(300);
check((await lefts()).length === 0, 'New leaves an empty model');
await p.locator('[role=menubar] button[data-menu="file"]').click();
await p.locator('[data-action="file.restore"]').click();
await p.waitForSelector('[role=dialog][aria-label="Restore from backup"]');
await wait(500);
const rows = await p.locator('.restore-row').allInnerTexts();
check(rows.some((r) => /before New/.test(r)), `the dialog lists a "before New" backup (${rows.length} rows)`);
await p.screenshot({ path: path.join(shots, 'restore-dialog.png') });
await p.locator('.restore-row', { hasText: 'before New' }).first().getByRole('button', { name: 'Restore' }).click();
await wait(1000);
check(JSON.stringify(await lefts()) === '[30]', `Restore put the model back (${JSON.stringify(await lefts())}; rows: ${JSON.stringify(rows)})`);
await p.keyboard.press('Control+z');
await wait(800);
check((await lefts()).length === 0, 'Ctrl+Z takes the restore back (one step)');
// Shift+S finds it too
await p.locator('body').click({ position: { x: 5, y: 400 } }).catch(() => {});
await p.keyboard.press('Shift+S');
await wait(300);
await p.keyboard.type('restore');
await wait(300);
check(await p.getByText('Restore from backup…').first().isVisible(), 'Shift+S, "restore" finds Restore from backup…');
await p.screenshot({ path: path.join(shots, 'palette.png') });
check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
