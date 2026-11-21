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
// the dev page and bridge ports (defaults 5174 and 5175; set both for a throwaway instance)
export const BRIDGE = `http://127.0.0.1:${process.env.VITE_BRIDGE_PORT ?? 5175}`;
export const PAGE = `http://127.0.0.1:${process.env.SKETCHCRAFT_DEV_PORT ?? 5174}/`;

/** The bridge's per-launch token: from SKETCHCRAFT_BRIDGE_TOKEN, else the file the bridge wrote beside the model. */
export function bridgeToken() {
  if (process.env.SKETCHCRAFT_BRIDGE_TOKEN) return process.env.SKETCHCRAFT_BRIDGE_TOKEN;
  const doc = process.env.SKETCHCRAFT_DOC ?? path.join(os.homedir(), 'Documents/SketchCraft/live.sketchcraft');
  const file = process.env.SKETCHCRAFT_BRIDGE_TOKEN_FILE ?? path.join(path.dirname(doc), '.bridge-token');
  try {
    return fs.readFileSync(file, 'utf8').trim();
  } catch {
    return '';
  }
}
export const authHeaders = () => ({ 'X-Sketchcraft-Token': bridgeToken() });

// tools/ui-tests/run.mjs points HOME at a scratch folder; browsers are still looked for in the real one
const realHome = process.env.SKETCHCRAFT_REAL_HOME ?? os.homedir();

/** A browser: Playwright from the project if installed, else the /design skill's copy. */
export async function chromium() {
  const own = process.env.SKETCHCRAFT_PLAYWRIGHT ? [path.join(process.env.SKETCHCRAFT_PLAYWRIGHT, 'node_modules/playwright-core/index.mjs')] : [];
  const tries = [...own, 'playwright-core', 'playwright', path.join(realHome, '.claude/skills/design/scripts/node_modules/playwright-core/index.mjs')];
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

/**
 * With SKETCHCRAFT_TRACE_DIR set (CI does), every context a suite opens is traced, and when it closes it leaves a
 * Playwright trace (open it with `npx playwright show-trace`) and a last screenshot of each page in that folder.
 */
function traced(browser) {
  const dir = process.env.SKETCHCRAFT_TRACE_DIR;
  if (!dir) return browser;
  fs.mkdirSync(dir, { recursive: true });
  const make = browser.newContext.bind(browser);
  let n = 0;
  const open = [];
  const closeBrowser = browser.close.bind(browser);
  // closing the browser closes the contexts the long way, so each leaves its trace
  browser.close = async (...x) => {
    for (const c of [...open]) await c.close().catch(() => {});
    return closeBrowser(...x);
  };
  browser.newContext = async (...a) => {
    const ctx = await make(...a);
    open.push(ctx);
    const id = `${path.basename(process.argv[1] ?? 'suite', '.mjs')}-${++n}`;
    await ctx.tracing.start({ screenshots: true, snapshots: true }).catch(() => {});
    const close = ctx.close.bind(ctx);
    ctx.close = async (...x) => {
      open.splice(open.indexOf(ctx) >>> 0, 1);
      let i = 0;
      for (const pg of ctx.pages()) await pg.screenshot({ path: path.join(dir, `${id}-page${i++}.png`) }).catch(() => {});
      await ctx.tracing.stop({ path: path.join(dir, `${id}.zip`) }).catch(() => {});
      return close(...x);
    };
    return ctx;
  };
  return browser;
}

/** Chromium flags for WebGL drawn on the CPU (SwiftShader), the way a machine without a GPU draws. */
export const SOFTWARE_GL_ARGS = ['--disable-gpu', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'];

export async function launch() {
  return traced(await launchRaw());
}

async function launchRaw() {
  const c = await chromium();
  const cache = path.join(realHome, 'Library/Caches/ms-playwright');
  // SKETCHCRAFT_SOFTWARE_GL=1 draws with SwiftShader (no GPU), as a Linux CI runner or a VM does
  const args = process.env.SKETCHCRAFT_SOFTWARE_GL ? SOFTWARE_GL_ARGS : [];
  const attempts = [() => c.launch({ args })];
  if (fs.existsSync(cache)) {
    for (const rev of fs.readdirSync(cache).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse()) {
      for (const sub of ['chrome-mac-arm64', 'chrome-mac', 'chrome-mac-x64']) {
        const exe = path.join(cache, rev, sub, 'Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
        if (fs.existsSync(exe)) attempts.push(() => c.launch({ executablePath: exe, args }));
      }
    }
  }
  attempts.push(() => c.launch({ channel: 'chrome', args }));
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

/** Headers for a JSON call to the bridge, with the token. */
export const jsonHeaders = () => ({ 'Content-Type': 'application/json', ...authHeaders() });

/** Send one engine command to the bridge from inside the page `p` (the bridge answers the page's origin). `as: 'plugin'` sends it as a plugin would. */
export const pageExec = (p, c, as) =>
  p.evaluate(async ({ c, as, BRIDGE, HDR }) => (await fetch(BRIDGE + '/exec' + (as ? '?as=plugin' : ''), { method: 'POST', headers: HDR, body: JSON.stringify(c) })).json(), { c, as, BRIDGE, HDR: jsonHeaders() });

/** The live scene, fetched from inside the page `p`. */
export const pageScene = (p) => p.evaluate(async ({ BRIDGE, HDR }) => (await fetch(BRIDGE + '/scene', { headers: HDR })).json(), { BRIDGE, HDR: jsonHeaders() });

/** Send one engine command to the bridge. */
export async function exec(cmd) {
  const r = await fetch(BRIDGE + '/exec', { method: 'POST', headers: authHeaders(), body: JSON.stringify(cmd) }).then((r) => r.json());
  if (!r.ok) throw new Error(`${cmd.cmd}: ${r.error}`);
  return r;
}

async function up(url) {
  try {
    await fetch(url, { signal: AbortSignal.timeout(800), headers: authHeaders() });
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
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sketchcraft-demo-'));
    // a bridge of our own: its own model and token (the dev page gets the token from the environment too)
    process.env.SKETCHCRAFT_BRIDGE_TOKEN = Math.random().toString(16).slice(2).padEnd(16, '0') + Date.now().toString(16);
    process.env.SKETCHCRAFT_DOC = path.join(dir, 'live.sketchcraft');
    kids.push(spawn(bin, ['bridge'], { env: process.env, stdio: 'ignore' }));
  }
  if (!(await up(PAGE))) {
    kids.push(spawn('npx', ['vite', '--host', '127.0.0.1', '--port', String(process.env.SKETCHCRAFT_DEV_PORT ?? 5174), '--strictPort'], { cwd: path.join(ROOT, 'app'), stdio: 'ignore', env: process.env }));
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
 *   await h.at([x, y, z])                    where a model point is on screen
 *   await h.glide(x, y, { frames })          move the mouse there, filmed
 *   await h.type('2.6')                      type (into the Value box), Enter
 *   await h.key('p', 'Push/Pull')            press a key, with a key badge
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
    /** A model point (meters) on screen, from the live camera (dev builds). */
    async at(p) {
      return page.evaluate((w) => {
        const T = window.__sketchcraftView.current;
        const v = T.camera.position.clone().set(w[0], w[1], w[2]).project(T.camera);
        const r = T.renderer.domElement.getBoundingClientRect();
        return [r.left + (v.x * 0.5 + 0.5) * r.width, r.top + (-v.y * 0.5 + 0.5) * r.height];
      }, p);
    },
    /** Move the mouse to (x, y) in steps, a frame every step: a live preview. */
    async glide(x, y, { frames = 10, ms = 70, from } = {}) {
      const start = from ?? (await page.evaluate(() => window.__demoMouse ?? [0, 0]));
      for (let i = 1; i <= frames; i++) {
        const k = i / frames;
        const px = start[0] + (x - start[0]) * k, py = start[1] + (y - start[1]) * k;
        await page.mouse.move(px, py);
        await page.evaluate((p) => (window.__demoMouse = p), [px, py]);
        await page.waitForTimeout(25);
        await shot(ms);
      }
    },
    /** Type into whatever has the keyboard (the Value box while a tool runs), then Enter. */
    async type(text, { enter = true, after = 900 } = {}) {
      for (const ch of text) {
        await page.keyboard.type(ch);
        await shot(110);
      }
      if (enter) await page.keyboard.press('Enter');
      await settle();
      await shot(after);
    },
    /** Press a key, showing it as a badge so viewers see the shortcut. */
    async key(k, label, { after = 700 } = {}) {
      await overlay(
        ({ k, label }) => {
          const b = document.createElement('div');
          b.className = 'demo-key';
          b.textContent = label ? `${k.toUpperCase()} · ${label}` : k.toUpperCase();
          Object.assign(b.style, { position: 'fixed', right: '24px', top: '64px', padding: '6px 12px', borderRadius: '8px', background: 'rgba(21,19,28,.88)', color: '#fff', font: '700 15px/1.2 -apple-system, system-ui, sans-serif', zIndex: 2147483647, pointerEvents: 'none' });
          document.body.appendChild(b);
        },
        { k, label },
      );
      await page.keyboard.press(k);
      await settle(250);
      await shot(after);
      await overlay(() => document.querySelectorAll('.demo-key').forEach((e) => e.remove()));
    },
    async clickAt(x, y, { hold = 500, after = 600, dbl = false } = {}) {
      await ring({ x, y });
      await shot(hold);
      await clearRing();
      if (dbl) await page.mouse.dblclick(x, y);
      else await page.mouse.click(x, y);
      await page.evaluate((p) => (window.__demoMouse = p), [x, y]);
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
