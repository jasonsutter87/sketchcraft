// Demo capture: drive the real app in a headless browser, take frames as it
// goes, and encode them into small GIFs (and MP4s). Demos find things by
// their names and labels (not screen positions), so they survive UI changes:
// after a redesign, run `node tools/demos/capture.mjs all` again.

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const BRIDGE = 'http://127.0.0.1:5175';
export const PAGE = 'http://127.0.0.1:5174/';

/** A browser: Playwright from the project if installed, else the /design skill's copy. */
export async function chromium() {
  const tries = ['playwright-core', 'playwright', path.join(os.homedir(), '.claude/skills/design/scripts/node_modules/playwright-core/index.mjs')];
  for (const t of tries) {
    try {
      const m = await import(t.startsWith('/') ? 'file://' + t : t);
      return m.chromium ?? m.default?.chromium;
    } catch {
      /* next */
    }
  }
  throw new Error('Playwright not found: npm i -D playwright-core (in tools/demos), or install the /design skill');
}

export async function launch() {
  const c = await chromium();
  const cache = path.join(os.homedir(), 'Library/Caches/ms-playwright');
  const attempts = [() => c.launch()];
  if (fs.existsSync(cache)) {
    for (const rev of fs.readdirSync(cache).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse()) {
      for (const sub of ['chrome-mac-arm64', 'chrome-mac', 'chrome-mac-x64']) {
        const exe = path.join(cache, rev, sub, 'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
        if (fs.existsSync(exe)) attempts.push(() => c.launch({ executablePath: exe }));
      }
    }
  }
  attempts.push(() => c.launch({ channel: 'chrome' }));
  let last;
  for (const a of attempts) {
    try {
      return await a();
    } catch (e) {
      last = e;
    }
  }
  throw new Error(`no browser: ${last?.message?.split('\n')[0]}`);
}

/** Send one engine command to the bridge. */
export async function exec(cmd) {
  const r = await fetch(BRIDGE + '/exec', { method: 'POST', body: JSON.stringify(cmd) }).then((r) => r.json());
  if (!r.ok) throw new Error(`${cmd.cmd}: ${r.error}`);
  return r;
}

async function up(url) {
  try {
    await fetch(url, { signal: AbortSignal.timeout(800) });
    return true;
  } catch {
    return false;
  }
}

/** Make sure the bridge and the dev page are running (localhost only); returns a stop(). */
export async function servers() {
  const kids = [];
  if (!(await up(BRIDGE + '/scene'))) {
    const bin = path.join(ROOT, 'target/release/sketchcraft');
    if (!fs.existsSync(bin)) throw new Error('build first: cargo build --release');
    const doc = path.join(os.tmpdir(), `sketchcraft-demo-${process.pid}.sketchcraft`);
    kids.push(spawn(bin, ['bridge'], { env: { ...process.env, SKETCHCRAFT_DOC: doc }, stdio: 'ignore' }));
  }
  if (!(await up(PAGE))) {
    kids.push(spawn('npx', ['vite', '--host', '127.0.0.1', '--port', '5174', '--strictPort'], { cwd: path.join(ROOT, 'app'), stdio: 'ignore' }));
  }
  for (let i = 0; i < 60 && !((await up(BRIDGE + '/scene')) && (await up(PAGE))); i++) await new Promise((r) => setTimeout(r, 500));
  return () => kids.forEach((k) => k.kill());
}

/** Run a list of commands (one engine command each), like the example .jsonl files. */
export async function script(lines) {
  for (const l of lines) await exec(l);
}

export async function runJsonl(file) {
  for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
    const t = line.trim();
    if (t && !t.startsWith('#') && !t.startsWith('//')) await exec(JSON.parse(t));
  }
}

/**
 * The recorder a demo gets: page actions that leave frames behind.
 *   await h.caption('Click a corner')         a caption bar along the bottom
 *   await h.click(locator)                   ring where it clicks, then the result
 *   await h.clickAt(x, y)                    same, at a point
 *   await h.shot(ms)                         a frame held for ms
 */
export function recorder(page, dir) {
  let n = 0;
  const frames = [];
  const shot = async (ms = 900) => {
    const f = path.join(dir, `f${String(n++).padStart(4, '0')}.png`);
    await page.screenshot({ path: f });
    frames.push({ f, ms });
  };
  const overlay = (fn, arg) => page.evaluate(fn, arg);
  const ring = (p) =>
    overlay(({ x, y }) => {
      const d = document.createElement('div');
      d.className = 'demo-ring';
      Object.assign(d.style, { position: 'fixed', left: x - 16 + 'px', top: y - 16 + 'px', width: '32px', height: '32px', borderRadius: '50%', border: '3px solid #f2c230', boxShadow: '0 0 0 3px rgba(21,19,28,.55)', zIndex: 2147483647, pointerEvents: 'none' });
      document.body.appendChild(d);
    }, p);
  const clearRing = () => overlay(() => document.querySelectorAll('.demo-ring').forEach((e) => e.remove()));
  const settle = (ms = 450) => page.waitForTimeout(ms);
  return {
    page,
    frames,
    shot,
    settle,
    async caption(text) {
      await overlay((t) => {
        let c = document.getElementById('demo-caption');
        if (!c) {
          c = document.createElement('div');
          c.id = 'demo-caption';
          Object.assign(c.style, { position: 'fixed', left: '50%', bottom: '86px', transform: 'translateX(-50%)', maxWidth: '80%', padding: '9px 16px', borderRadius: '10px', background: 'rgba(21,19,28,.88)', color: '#fff', font: '600 16px/1.35 -apple-system, "Avenir Next", system-ui, sans-serif', zIndex: 2147483647, pointerEvents: 'none', textAlign: 'center' });
          document.body.appendChild(c);
        }
        c.textContent = t;
        c.style.display = t ? 'block' : 'none';
      }, text);
    },
    async click(locator, { hold = 700, after = 900 } = {}) {
      const box = await locator.boundingBox();
      if (!box) throw new Error('nothing to click: ' + locator);
      const p = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
      await ring(p);
      await shot(hold);
      await clearRing();
      await locator.click();
      await settle();
      await shot(after);
    },
    async clickAt(x, y, { hold = 500, after = 600, dbl = false } = {}) {
      await ring({ x, y });
      await shot(hold);
      await clearRing();
      if (dbl) await page.mouse.dblclick(x, y);
      else await page.mouse.click(x, y);
      await settle(300);
      await shot(after);
    },
  };
}

/** Frames -> GIF (and MP4) with ffmpeg: one palette for the whole clip, 960 wide. */
export function encode(frames, out, { width = 960, mp4 = true } = {}) {
  const dir = path.dirname(frames[0].f);
  const list = path.join(dir, 'list.txt');
  const lines = frames.flatMap(({ f, ms }) => [`file '${f}'`, `duration ${(ms / 1000).toFixed(3)}`]);
  lines.push(`file '${frames[frames.length - 1].f}'`);
  fs.writeFileSync(list, lines.join('\n'));
  fs.mkdirSync(path.dirname(out), { recursive: true });
  const run = (args) =>
    new Promise((ok, bad) => {
      const p = spawn('ffmpeg', ['-y', '-loglevel', 'error', ...args], { stdio: 'inherit' });
      p.on('exit', (c) => (c === 0 ? ok() : bad(new Error('ffmpeg failed'))));
    });
  const gif = run(['-f', 'concat', '-safe', '0', '-i', list, '-vf', `fps=10,scale=${width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle`, '-loop', '0', out]);
  const video = mp4 ? run(['-f', 'concat', '-safe', '0', '-i', list, '-vf', `fps=24,scale=${Math.max(width, 1280)}:-2:flags=lanczos,format=yuv420p`, '-c:v', 'libx264', '-crf', '28', '-movflags', '+faststart', out.replace(/\.gif$/, '.mp4')]) : Promise.resolve();
  return Promise.all([gif, video]);
}
