#!/usr/bin/env node
// Run the browser test suites against a throwaway bridge and dev page.
//   node tools/ui-tests/run.mjs                 every suite
//   node tools/ui-tests/run.mjs tools gestures  only the suites whose file name contains one of these
//   node tools/ui-tests/run.mjs --list
//   node tools/ui-tests/run.mjs visual -- --update   (what follows -- is passed to the suites)
//
// It makes a scratch folder (SKETCHCRAFT_SCRATCH, else a new one under the system temp folder), points HOME,
// XDG_CACHE_HOME and SKETCHCRAFT_DOC into it, starts `sketchcraft bridge` and `vite` on SKETCHCRAFT_DEV_PORT /
// VITE_BRIDGE_PORT (default 5501 / 5502) with a per-run bridge token, runs each suite as its own process with a
// timeout, and stops everything it started. Your real ~/Documents/SketchCraft is never touched.
// A watchdog stops the run if what it started grows past SKETCHCRAFT_RSS_MB (default 3072) of memory.
// Needs: `cargo build --release -p sketchcraft-core` and `npm ci` in app/ (and Playwright, see tools/demos/lib.mjs).
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ROOT } from '../demos/lib.mjs';

const SUITES = [
  ['undo-and-restore', 'tools/ui-tests/undo-and-restore.mjs', 240],
  ['read-only', 'tools/ui-tests/read-only.mjs', 240],
  ['gestures-and-escape', 'tools/ui-tests/gestures-and-escape.mjs', 240],
  ['overlay-layout', 'tools/ui-tests/overlay-layout.mjs', 300],
  ['side-tabs', 'tools/ui-tests/side-tabs.mjs', 300],
  ['line-styles', 'tools/ui-tests/line-styles.mjs', 300],
  ['hatches', 'tools/ui-tests/hatches.mjs', 300],
  ['cut-fills', 'tools/ui-tests/cut-fills.mjs', 300],
  ['cut-priority', 'tools/ui-tests/cut-priority.mjs', 300],
  ['layout-workspace', 'tools/ui-tests/layout-workspace.mjs', 300],
  ['sheet-editor', 'tools/ui-tests/sheet-editor.mjs', 420],
  ['sheet-tables', 'tools/ui-tests/sheet-tables.mjs', 300],
  ['subdivide', 'tools/ui-tests/subdivide.mjs', 300],
  ['wall-pull', 'tools/ui-tests/wall-pull.mjs', 300],
  ['room-mode', 'tools/ui-tests/room-mode.mjs', 300],
  ['fixtures', 'tools/ui-tests/fixtures.mjs', 300],
  ['texturing-set', 'tools/ui-tests/texturing-set.mjs', 300],
  ['shortcuts', 'tools/ui-tests/shortcuts.mjs', 300],
  ['menu-bar', 'tools/ui-tests/menu-bar.mjs', 420],
  ['tools', 'tools/ui-tests/tools.mjs', 900],
  ['plugin-sandbox', 'tools/security-tests/plugin-sandbox.mjs', 240],
  ['ruby', 'tools/ruby-tests/run.mjs', 300],
  ['visual', 'tools/ui-tests/visual.mjs', 600],
];
const args = process.argv.slice(2);
if (args.includes('--list')) {
  for (const [n, f] of SUITES) console.log(`${n.padEnd(22)} ${f}`);
  process.exit(0);
}
// `run.mjs visual -- --update`: what follows `--` goes to every suite that runs
const dd = args.indexOf('--');
const extra = dd >= 0 ? args.slice(dd + 1) : [];
const want = (dd >= 0 ? args.slice(0, dd) : args).filter((a) => !a.startsWith('--'));
const chosen = SUITES.filter(([n, f]) => fs.existsSync(path.join(ROOT, f)) && (!want.length || want.some((w) => n.includes(w))));
if (!chosen.length) {
  console.error('no suite matches', want);
  process.exit(2);
}

const devPort = String(process.env.SKETCHCRAFT_DEV_PORT ?? 5501);
const bridgePort = String(process.env.VITE_BRIDGE_PORT ?? 5502);
const scratch = process.env.SKETCHCRAFT_SCRATCH ?? fs.mkdtempSync(path.join(os.tmpdir(), 'sc-ui-'));
const ownScratch = !process.env.SKETCHCRAFT_SCRATCH;
for (const d of ['home', 'cache', 'shots']) fs.mkdirSync(path.join(scratch, d), { recursive: true });
const bin = process.env.SKETCHCRAFT_BIN ?? path.join(ROOT, `target/release/sketchcraft${process.platform === 'win32' ? '.exe' : ''}`);
if (!fs.existsSync(bin)) {
  console.error('build the engine first: cargo build --release -p sketchcraft-core');
  process.exit(2);
}
const env = {
  ...process.env,
  SKETCHCRAFT_REAL_HOME: process.env.SKETCHCRAFT_REAL_HOME ?? os.homedir(),
  HOME: path.join(scratch, 'home'),
  XDG_CACHE_HOME: path.join(scratch, 'cache'),
  SKETCHCRAFT_DOC: path.join(scratch, 'live.sketchcraft'),
  SKETCHCRAFT_DEV_PORT: devPort,
  VITE_BRIDGE_PORT: bridgePort,
  SKETCHCRAFT_BRIDGE_TOKEN: [...crypto.getRandomValues(new Uint8Array(16))].map((x) => x.toString(16).padStart(2, '0')).join(''),
  SHOT_DIR: process.env.SHOT_DIR ?? path.join(scratch, 'shots'),
  // the date sheets print in their title block (2026-01-01), so the Layout workspace's references don't change daily
  SOURCE_DATE_EPOCH: process.env.SOURCE_DATE_EPOCH ?? '1767225600',
};
// Playwright found beside tools/demos/lib.mjs is fine; elsewhere, SKETCHCRAFT_PLAYWRIGHT names its folder
const kids = [];
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const up = async (url) => {
  try {
    await fetch(url, { signal: AbortSignal.timeout(800), headers: { 'X-Sketchcraft-Token': env.SKETCHCRAFT_BRIDGE_TOKEN } });
    return true;
  } catch {
    return false;
  }
};
const stop = () => {
  for (const k of kids) {
    try {
      process.kill(k.pid, 'SIGTERM');
    } catch {}
  }
};
let tripped = false;
// memory watchdog: the processes this run started and their children
const rssMb = () => {
  try {
    const out = execFileSync('ps', ['-axo', 'pid=,ppid=,rss='], { encoding: 'utf8' });
    const rows = out.trim().split('\n').map((l) => l.trim().split(/\s+/).map(Number));
    const mine = new Set([process.pid, ...kids.map((k) => k.pid)]);
    let grew = true;
    while (grew) {
      grew = false;
      for (const [pid, ppid] of rows) if (mine.has(ppid) && !mine.has(pid)) (mine.add(pid), (grew = true));
    }
    return rows.filter(([pid]) => mine.has(pid)).reduce((a, [, , r]) => a + r, 0) / 1024;
  } catch {
    return 0;
  }
};
const limit = Number(process.env.SKETCHCRAFT_RSS_MB ?? 3072);
const dog = setInterval(() => {
  const m = rssMb();
  if (m > limit) {
    console.error(`memory watchdog: ${Math.round(m)} MB is over ${limit} MB; stopping`);
    tripped = true;
    stop();
    process.exit(3);
  }
}, 2000);
const done = (code) => {
  clearInterval(dog);
  stop();
  if (ownScratch && !process.env.SKETCHCRAFT_KEEP) fs.rmSync(scratch, { recursive: true, force: true });
  process.exit(code);
};
process.on('SIGINT', () => done(130));
process.on('SIGTERM', () => done(143));

kids.push(spawn(bin, ['bridge', '--port', bridgePort, '--origin', `http://127.0.0.1:${devPort}`], { env, stdio: 'ignore' }));
const vite = path.join(ROOT, 'app/node_modules/.bin/vite');
kids.push(spawn(vite, ['--host', '127.0.0.1', '--port', devPort, '--strictPort'], { cwd: path.join(ROOT, 'app'), env, stdio: 'ignore' }));
const BR = `http://127.0.0.1:${bridgePort}/scene`;
const PG = `http://127.0.0.1:${devPort}/`;
for (let i = 0; i < 120 && !((await up(BR)) && (await up(PG))); i++) await wait(500);
if (!((await up(BR)) && (await up(PG)))) {
  console.error('the bridge and dev page did not start');
  done(2);
}

const results = [];
for (const [name, file, secs] of chosen) {
  const t0 = Date.now();
  console.log(`\n=== ${name} ===`);
  const code = await new Promise((resolve) => {
    const k = spawn('node', [path.join(ROOT, file), ...extra], { env, stdio: 'inherit' });
    kids.push(k);
    const timer = setTimeout(() => {
      console.error(`${name}: timed out after ${secs}s`);
      k.kill('SIGKILL');
    }, secs * 1000);
    k.on('exit', (c) => (clearTimeout(timer), resolve(c ?? 1)));
  });
  results.push([name, code, Math.round((Date.now() - t0) / 1000)]);
  // a suite that fails to start the page leaves nothing stale: each one makes a new model
}
console.log('\n' + results.map(([n, c, s]) => `${c === 0 ? 'ok  ' : 'FAIL'} ${n} (${s}s)`).join('\n'));
done(tripped || results.some(([, c]) => c !== 0) ? 1 : 0);
