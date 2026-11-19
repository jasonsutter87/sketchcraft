#!/usr/bin/env node
// The menu bar (File, Edit, View, Camera, Draw, Tools, Window, Help) and Help > About, in a real browser: every menu
// opens and closes, hovering moves between open menus, the keyboard works (F10 or Alt, arrows, Enter, Esc, submenus),
// shortcuts are shown and unavailable items are greyed out, one item from each menu does what it says, About shows the
// promise and the contributors, Italian renames the menus, and the bar and its menus fit in every layout and theme.
//   node tools/ui-tests/run.mjs menu-bar
// Like the other suites it runs on a scratch model, in a browser pinned to en-US, with real pointer and key events.
import fs from 'node:fs';
import path from 'node:path';
import { exec, launch, PAGE, ROOT } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same); tools/ui-tests/run.mjs does.');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
let passed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (ok) passed++;
  else failed++;
};
// the few tools the menus are tried with, on the tool bar where they can be seen switching on
const TOOLS = ['select', 'line', 'room', 'move'];
const NAMES = ['File', 'Edit', 'View', 'Camera', 'Draw', 'Tools', 'Window', 'Help'];
const WINDOWS_UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36';

const b = await launch();
const problems = [];
const open = async ({ storage = {}, viewport = { width: 1700, height: 1000 }, userAgent } = {}) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport, ...(userAgent ? { userAgent } : {}) });
  await ctx.route(/^https:\/\/(github\.com|raw\.githubusercontent\.com)\//, (r) => r.abort());
  await ctx.addInitScript(
    ([s, tools]) => {
      try {
        if (sessionStorage.getItem('seeded')) return;
        sessionStorage.setItem('seeded', '1');
        localStorage.setItem('sketchcraft.help', 'off');
        localStorage.setItem('sketchcraft.lessonsDone', '[]');
        localStorage.setItem('sketchcraft.workspaceEdits', JSON.stringify({ model: { tools: [tools] } }));
        for (const [k, v] of Object.entries(s)) localStorage.setItem(k, v);
      } catch {}
    },
    [storage, TOOLS],
  );
  // links open a new tab: note them instead
  await ctx.addInitScript(() => {
    window.__opened = [];
    window.open = (url) => (window.__opened.push(String(url)), null);
  });
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  p.on('dialog', (d) => d.dismiss().catch(() => {}));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await p.waitForSelector('[role=menubar]');
  await wait(500);
  return { p, ctx };
};
const top = (p, name) => p.locator(`[role=menubar] > .menubar-item > button[data-menu="${name.toLowerCase()}"]`);
const menu = (p, name) => p.getByRole('menu', { name, exact: true });
const item = (p, id) => p.locator(`[data-action="${id}"]`);
const openMenu = async (p, name) => {
  await top(p, name).click();
  await menu(p, name).waitFor();
};
const nothingOpen = async (p) => (await p.locator('.menubar-menu').count()) === 0;
const focused = (p) => p.evaluate(() => document.activeElement?.getAttribute('data-menu') ?? document.activeElement?.getAttribute('data-action') ?? (document.activeElement?.textContent ?? '').replace('▸', '').trim());
const execRequest = (p, cmd) => p.waitForRequest((r) => r.method() === 'POST' && new URL(r.url()).pathname === '/exec' && r.postDataJSON()?.cmd === cmd, { timeout: 5000 });
const activeTool = (p) => p.locator('nav.toolbar button.tool.on').first().getAttribute('aria-label').then((t) => (t ?? '').split(/[:(]/)[0].trim(), () => '');
const isMac = process.platform === 'darwin';

// ---- the bar, and each menu opening and closing ----------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  check((await p.locator('[role=menubar]').count()) === 1, 'there is one menu bar');
  const names = await p.locator('[role=menubar] > .menubar-item > button').allInnerTexts();
  check(JSON.stringify(names) === JSON.stringify(NAMES), `the menus are ${NAMES.join(', ')} (${names.join(', ')})`);
  check((await p.getByRole('button', { name: 'File ▾' }).count()) === 0, 'the lone File button is gone');
  for (const n of NAMES) {
    await top(p, n).click();
    await wait(80);
    const m = menu(p, n);
    check((await m.count()) === 1 && (await top(p, n).getAttribute('aria-expanded')) === 'true', `${n} opens`);
    const items = await m.locator('[role^=menuitem]').count();
    check(items >= 5, `${n} has items (${items})`);
    await p.keyboard.press('Escape');
    await wait(80);
    check(await nothingOpen(p), `Escape closes ${n}`);
    check((await top(p, n).getAttribute('aria-expanded')) === 'false', `${n} reports closed`);
  }
  // clicking the name again closes it; clicking away closes it
  await top(p, 'File').click();
  await top(p, 'File').click();
  check(await nothingOpen(p), 'clicking the open name closes the menu');
  await top(p, 'Edit').click();
  await p.mouse.click(700, 500);
  await wait(100);
  check(await nothingOpen(p), 'clicking away closes the menu');
  // roles
  await top(p, 'View').click();
  const roles = await p.locator('.menubar-menu [role^=menuitem]').evaluateAll((els) => [...new Set(els.map((e) => e.getAttribute('role')))].sort());
  check(JSON.stringify(roles) === JSON.stringify(['menuitem', 'menuitemcheckbox']), `View has items and checkboxes (${roles})`);
  check((await item(p, 'view.xray').getAttribute('aria-checked')) === 'false', 'a check item says whether it is on');
  await p.keyboard.press('Escape');
  check((await p.locator('[role=menubar]').getAttribute('aria-label')) === null && (await p.locator('nav[aria-label="Menu bar"]').count()) === 1, 'the bar is a labelled landmark');
  check((await item(p, 'help.updates').count()) === 0 && (await p.getByText('Check for updates').count()) === 0, 'Check for updates is only in the desktop app');
  await ctx.close();
}

// ---- hover moves between open menus ----------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await top(p, 'File').hover();
  await wait(100);
  check(await nothingOpen(p), 'hovering alone opens nothing');
  await openMenu(p, 'File');
  await top(p, 'Edit').hover();
  await wait(150);
  check((await menu(p, 'Edit').count()) === 1 && (await menu(p, 'File').count()) === 0, 'with a menu open, pointing at the next name opens that one');
  await top(p, 'Help').hover();
  await wait(150);
  check((await menu(p, 'Help').count()) === 1 && (await menu(p, 'Edit').count()) === 0, 'and the one after that');
  await p.keyboard.press('Escape');
  // submenus open on hover
  await openMenu(p, 'File');
  await p.getByRole('menuitem', { name: 'Export' }).hover();
  await wait(150);
  check((await item(p, 'file.export.glb').isVisible()) && (await item(p, 'file.export.pdf').isVisible()), 'File > Export opens its formats beside it');
  const sub = await menu(p, 'Export').boundingBox();
  const parent = await menu(p, 'File').boundingBox();
  check(!!sub && !!parent && (sub.x >= parent.x + parent.width - 8 || sub.x + sub.width <= parent.x + 8), 'the submenu sits beside its menu, not over its items');
  await item(p, 'file.open').hover();
  await wait(150);
  check((await menu(p, 'Export').count()) === 0, 'pointing at another item closes the submenu');
  await ctx.close();
}

// ---- shortcuts are shown, and what is unavailable is greyed out ----------------------------------------------------------
{
  const { p, ctx } = await open();
  await openMenu(p, 'File');
  const key = async (id) => (await item(p, id).locator('kbd').count()) ? item(p, id).locator('kbd').innerText() : '';
  check((await key('file.new')) === (isMac ? '⌘N' : 'Ctrl+N'), `File > New shows its key (${await key('file.new')})`);
  check((await key('file.saveas')) === (isMac ? '⇧⌘S' : 'Ctrl+Shift+S'), `Save As shows its key (${await key('file.saveas')})`);
  await p.keyboard.press('Escape');
  await openMenu(p, 'Tools');
  check((await key('tool:move')) === 'M' && (await key('tool:push')) === 'P', 'Tools shows M for Move and P for Push/Pull');
  const labels = await menu(p, 'Tools').locator('.mi-label').allInnerTexts();
  check(['Select', 'Move', 'Rotate', 'Scale', 'Push / Pull', 'Pull wall', 'Follow Me', 'Offset', 'Tape Measure', 'Dimension', 'Text label', 'Paint Bucket', 'Eraser', 'Section cut'].every((l) => labels.includes(l)), `Tools has the tools by name (${labels.join(', ')})`);
  await p.keyboard.press('Escape');
  await openMenu(p, 'Draw');
  const draw = await menu(p, 'Draw').locator('.mi-label').allInnerTexts();
  check(['Line', 'Freehand', '2-Point Arc', 'Arc (center)', '3-Point Arc', 'Rectangle', 'Circle', 'Polygon', 'Wall', 'Room'].every((l) => draw.includes(l)), `Draw has its tools (${draw.join(', ')})`);
  await p.keyboard.press('Escape');
  await openMenu(p, 'Edit');
  check((await key('edit.undo')) === (isMac ? '⌘Z' : 'Ctrl+Z'), 'Edit > Undo shows its key');
  check(await item(p, 'edit.cut').isDisabled(), 'Cut is greyed out with nothing selected');
  check(await item(p, 'edit.delete').isDisabled(), 'Delete is greyed out with nothing selected');
  check(await item(p, 'edit.paste').isDisabled(), 'Paste is greyed out with nothing copied');
  check(await item(p, 'edit.explode').isDisabled(), 'Explode is greyed out');
  check(!(await item(p, 'edit.selectall').isDisabled()) && !(await item(p, 'edit.undo').isDisabled()), 'Select all and Undo are available');
  await p.keyboard.press('Escape');
  await openMenu(p, 'Camera');
  check((await key('view.fit')) === (isMac ? '⇧Z' : 'Shift+Z'), `Zoom extents shows its key (${await key('view.fit')})`);
  await p.keyboard.press('Escape');
  await openMenu(p, 'Help');
  check((await key('help.shortcuts')) === '?', 'Help > Keyboard shortcuts shows ?');
  await ctx.close();
}

// ---- the keyboard -----------------------------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  await p.keyboard.press('F10');
  check((await focused(p)) === 'file', 'F10 focuses the menu bar');
  check(await nothingOpen(p), 'and opens nothing');
  await p.keyboard.press('ArrowRight');
  check((await focused(p)) === 'edit', 'Right moves to Edit');
  await p.keyboard.press('ArrowLeft');
  await p.keyboard.press('ArrowLeft');
  check((await focused(p)) === 'help', 'Left from File wraps round to Help');
  await p.keyboard.press('ArrowRight');
  await p.keyboard.press('ArrowDown');
  await wait(100);
  check((await menu(p, 'File').count()) === 1 && (await focused(p)) === 'file.new', 'Down opens File and focuses New');
  await p.keyboard.press('ArrowDown');
  check((await focused(p)) === 'file.open', 'Down moves to Open');
  await p.keyboard.press('ArrowUp');
  await p.keyboard.press('ArrowUp');
  check((await focused(p)) === 'Plugins', 'Up from the first item wraps to the last (Plugins)');
  await p.keyboard.press('Home');
  check((await focused(p)) === 'file.new', 'Home goes to the first item');
  await p.keyboard.press('End');
  check((await focused(p)) === 'Plugins', 'End goes to the last item');
  await p.keyboard.press('ArrowRight');
  await wait(150);
  check((await menu(p, 'Plugins').count()) === 1 && (await p.evaluate(() => document.activeElement?.closest('[role=menu]')?.getAttribute('aria-label'))) === 'Plugins', `Right on Plugins opens its submenu and focuses an item in it (${await focused(p)})`);
  await p.keyboard.press('ArrowLeft');
  await wait(100);
  check((await menu(p, 'Plugins').count()) === 0 && (await focused(p)) === 'Plugins', 'Left closes the submenu and comes back');
  await p.keyboard.press('Home');
  for (let i = 0; i < 7; i++) await p.keyboard.press('ArrowDown');
  check((await focused(p)) === 'Export', `Down arrives at Export (${await focused(p)})`);
  await p.keyboard.press('ArrowRight');
  await wait(150);
  check((await focused(p)) === 'file.export.glb', 'Right on Export opens the formats');
  await p.keyboard.press('ArrowLeft');
  await wait(100);
  check((await menu(p, 'Export').count()) === 0 && (await menu(p, 'File').count()) === 1, 'Left closes only the submenu');
  await p.keyboard.press('Home');
  await p.keyboard.press('ArrowRight');
  await wait(150);
  check((await menu(p, 'Edit').count()) === 1 && (await menu(p, 'File').count()) === 0, 'Right on a plain item goes to the next menu');
  check((await focused(p)) === 'edit.undo', 'and focuses its first item');
  await p.keyboard.press('Escape');
  await wait(100);
  check(await nothingOpen(p) && (await focused(p)) === 'edit', 'Esc closes the menu and keeps the focus on its name');
  await p.keyboard.press('Escape');
  check((await focused(p)) === '' || (await focused(p)) === 'BODY' || !(await p.evaluate(() => document.activeElement?.closest('[role=menubar]'))), 'Esc again leaves the menu bar');
  // Enter runs the focused item: Edit > Undo
  await p.keyboard.press('F10');
  await p.keyboard.press('ArrowRight');
  await p.keyboard.press('ArrowDown');
  const undo = execRequest(p, 'undo');
  await p.keyboard.press('Enter');
  check(!!(await undo.catch(() => null)), 'Enter on Undo sends the undo command');
  check(await nothingOpen(p), 'and closes the menu');
  check(!(await p.evaluate(() => !!document.activeElement?.closest('[role=menubar]'))), 'and gives the focus back to the model');
  // the model's keys still work after menus
  await p.keyboard.press('l');
  await wait(150);
  check((await activeTool(p)) === 'Line', 'L still picks Line after using a menu');
  await ctx.close();
}

// Alt taps open the bar on Windows and Linux (on a Mac, Option is the copy key, so only F10 does)
{
  const { p, ctx } = await open({ userAgent: WINDOWS_UA });
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  await p.keyboard.press('Alt');
  check((await focused(p)) === 'file', 'tapping Alt focuses the menu bar (Windows and Linux)');
  await p.keyboard.press('Escape');
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  await p.keyboard.down('Alt');
  await p.keyboard.press('l');
  await p.keyboard.up('Alt');
  check((await focused(p)) !== 'file', 'Alt held with another key does not');
  await openMenu(p, 'File');
  check((await item(p, 'file.new').locator('kbd').innerText()) === 'Ctrl+N', 'and shortcuts read Ctrl+N there');
  await ctx.close();
}

// ---- one item from each menu does what it says --------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  // File > New sends the same engine command as the key
  await openMenu(p, 'File');
  const viaMenu = execRequest(p, 'new');
  await item(p, 'file.new').click();
  const fromMenu = (await viaMenu).postDataJSON();
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  const viaKey = execRequest(p, 'new');
  await p.keyboard.press('ControlOrMeta+n');
  const fromKey = (await viaKey).postDataJSON();
  check(JSON.stringify(fromMenu) === JSON.stringify(fromKey), `File > New sends the same engine command as ${isMac ? '⌘N' : 'Ctrl+N'} (${JSON.stringify(fromMenu).slice(0, 60)})`);
  check(await nothingOpen(p), 'choosing an item closes the menu');

  // File > Plugins lists plugin commands and the manager
  await openMenu(p, 'File');
  await p.getByRole('menuitem', { name: /^Plugins/ }).click();
  check(await item(p, 'win.plugins').isVisible(), 'File > Plugins has Manage plugins…');
  await item(p, 'win.plugins').click();
  await p.waitForSelector('[role=dialog]');
  check(true, 'Manage plugins… opens the plugins dialog');
  await p.keyboard.press('Escape');
  await wait(200);

  // Edit: Select all, then the selection items come alive, Hide/Unhide send their commands
  await openMenu(p, 'Edit');
  await item(p, 'edit.selectall').click();
  await wait(200);
  await openMenu(p, 'Edit');
  const unhide = execRequest(p, 'unhide');
  await item(p, 'edit.unhide').click();
  check(!!(await unhide.catch(() => null)), 'Edit > Unhide all sends unhide');

  // View: X-Ray toggles, and the view bar agrees
  const xray = () => p.locator('.views button', { hasText: /^X-ray$/ });
  check(!(await xray().getAttribute('class'))?.includes('on'), 'X-ray starts off');
  await openMenu(p, 'View');
  await item(p, 'view.xray').click();
  await wait(100);
  check((await xray().getAttribute('class'))?.includes('on'), 'View > X-Ray turns it on in the view bar');
  await openMenu(p, 'View');
  check((await item(p, 'view.xray').getAttribute('aria-checked')) === 'true', 'and the menu item is ticked');
  await item(p, 'view.xray').click();
  await wait(100);
  check(!(await xray().getAttribute('class'))?.includes('on'), 'choosing it again turns it off');
  // View > Face style (a radio) and Workspaces
  await openMenu(p, 'View');
  await p.getByRole('menuitem', { name: 'Face style' }).click();
  await item(p, 'view.face.wireframe').click();
  await wait(100);
  await openMenu(p, 'View');
  await p.getByRole('menuitem', { name: 'Face style' }).click();
  check((await item(p, 'view.face.wireframe').getAttribute('aria-checked')) === 'true' && (await item(p, 'view.face.textured').getAttribute('aria-checked')) === 'false', 'View > Face style > Wireframe is the chosen one');
  await item(p, 'view.face.textured').click();
  await wait(100);
  await openMenu(p, 'View');
  await p.getByRole('menuitem', { name: 'Workspaces' }).click();
  await item(p, 'ws.arch').click();
  await wait(300);
  check((await p.locator('.ws-switch button[aria-selected="true"]').innerText()) === 'Arch', 'View > Workspaces > Arch switches the workspace');
  await p.locator('.ws-switch button', { hasText: /^Model$/ }).click();
  await wait(200);

  // Camera: parallel projection
  await openMenu(p, 'Camera');
  await item(p, 'view.parallel').click();
  await wait(100);
  check((await p.locator('.views button', { hasText: /^Parallel$/ }).getAttribute('class'))?.includes('on'), 'Camera > Parallel projection turns the view bar button on');
  await openMenu(p, 'Camera');
  check((await item(p, 'view.perspective').getAttribute('aria-checked')) === 'false' && (await item(p, 'view.parallel').getAttribute('aria-checked')) === 'true', 'and the radio moves');
  await item(p, 'view.perspective').click();
  await wait(100);

  // Draw > Room and Tools > Move activate their tools
  await openMenu(p, 'Draw');
  await item(p, 'tool:room').click();
  await wait(200);
  check((await activeTool(p)) === 'Room', `Draw > Room activates the Room tool (${await activeTool(p)})`);
  await openMenu(p, 'Tools');
  await item(p, 'tool:move').click();
  await wait(200);
  check((await activeTool(p)) === 'Move', `Tools > Move activates Move (${await activeTool(p)})`);

  // Window > Library shows the Library tab
  await openMenu(p, 'Window');
  await item(p, 'win.library').click();
  await wait(200);
  check((await p.locator('.panel.left .tabs button.on').innerText()) === 'Library', 'Window > Library shows the Library tab');
  await openMenu(p, 'Window');
  await item(p, 'win.outliner').click();
  await wait(200);
  check((await p.locator('.panel.left .tabs button.on').innerText()) === 'Outliner', 'Window > Outliner shows the Outliner');
  // Window > Customize shortcuts
  await openMenu(p, 'Window');
  await item(p, 'win.keyeditor').click();
  await p.waitForSelector('.key-editor');
  check(true, 'Window > Customize shortcuts opens the shortcut editor');
  await p.keyboard.press('Escape');
  await wait(200);

  // Help > Keyboard shortcuts, Report a bug, Learn center
  await openMenu(p, 'Help');
  await item(p, 'help.shortcuts').click();
  await p.waitForSelector('[role=dialog][aria-label="Keyboard shortcuts"]');
  check(true, 'Help > Keyboard shortcuts opens the sheet');
  await p.keyboard.press('Escape');
  await wait(200);
  await openMenu(p, 'Help');
  await item(p, 'help.learn').click();
  await wait(300);
  check((await p.locator('[role=dialog]').count()) >= 1, 'Help > Learn center opens the Learn center');
  await p.keyboard.press('Escape');
  await wait(200);
  await openMenu(p, 'Help');
  await item(p, 'help.bug').click();
  const opened = await p.evaluate(() => window.__opened);
  check(opened.length === 1 && opened[0] === 'https://github.com/jasonsutter87/sketchcraft/issues', `Help > Report a bug opens the GitHub issues page (${opened})`);
  await ctx.close();
}

// ---- Help > About ----------------------------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await openMenu(p, 'Help');
  await item(p, 'help.about').click();
  const dlg = p.getByRole('dialog', { name: 'About SketchCraft' });
  await dlg.waitFor();
  const text = await dlg.innerText();
  check(/SketchCraft/.test(await dlg.locator('.about-name').innerText()), 'About shows the name');
  check(/^Version \d+\.\d+\.\d+/.test(await dlg.locator('.about-version').innerText()), `and the version (${await dlg.locator('.about-version').innerText()})`);
  check((await dlg.locator('.about-tagline').innerText()) === 'Always free. Never freemium. Never paid.', 'the tagline is under the name');
  check((await dlg.locator('.about-promise').innerText()) === "It's a promise.", 'with "It\'s a promise." below it');
  const order = await dlg.locator('.about-head > *').evaluateAll((els) => els.map((e) => e.className));
  check(order.indexOf('about-name') < order.indexOf('about-tagline') && order.indexOf('about-tagline') < order.indexOf('about-promise'), 'in that order: name, tagline, promise');
  const links = await dlg.locator('.about-links a').evaluateAll((els) => els.map((e) => [e.textContent, e.getAttribute('href')]));
  check(links.length === 3 && links.some(([, h]) => h === 'https://github.com/jasonsutter87/sketchcraft') && links.some(([, h]) => /LICENSE$/.test(h)) && links.some(([, h]) => /CHANGELOG\.md$/.test(h)), `it links the repository, the license and the changelog (${links.map((l) => l[0]).join(', ')})`);
  check((await dlg.getByRole('heading', { name: 'Thank you' }).count()) === 1, 'it has a Thank you section');
  const thanked = await dlg.locator('.about-thanks li').allInnerTexts();
  const { readContributors } = await import(path.join(ROOT, 'app/contributors.mjs'));
  const expected = readContributors(ROOT).map((c) => c.name);
  check(expected.length > 0 && JSON.stringify(thanked) === JSON.stringify(expected), `the contributors are the git history's authors (${thanked.join(', ')})`);
  check(thanked.includes('Jason Sutter') && thanked.includes('Kad26') && thanked[0] === 'Jason Sutter', 'including the owner (first) and Kad26');
  check(!/dependabot|github-actions|\[bot\]/i.test(text), 'and no bots');
  await p.keyboard.press('Escape');
  await wait(200);
  check((await dlg.count()) === 0, 'Escape closes About');
  // the command search finds it too
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  await p.keyboard.press('Shift+S');
  await p.keyboard.type('about');
  await p.keyboard.press('Enter');
  await wait(300);
  check((await p.getByRole('dialog', { name: 'About SketchCraft' }).count()) === 1, 'Shift+S "about" opens About too');
  await p.getByRole('button', { name: 'Close' }).click();
  await wait(200);
  check((await p.getByRole('dialog', { name: 'About SketchCraft' }).count()) === 0, 'Close closes About');
  await ctx.close();
}

// ---- Italian ----------------------------------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await openMenu(p, 'View');
  await p.getByRole('menuitem', { name: 'Language' }).click();
  check((await item(p, 'lang.system').getAttribute('aria-checked')) === 'true', 'the language radio starts on System language');
  await item(p, 'lang.it').click();
  await p.waitForSelector('html[lang="it"]');
  const names = await p.locator('[role=menubar] > .menubar-item > button').allInnerTexts();
  check(JSON.stringify(names) === JSON.stringify(['File', 'Modifica', 'Vista', 'Camera', 'Disegna', 'Strumenti', 'Finestra', 'Aiuto']), `the menus are in Italian (${names.join(', ')})`);
  await top(p, 'Draw').click();
  const draw = await p.getByRole('menu', { name: 'Disegna' }).locator('.mi-label').allInnerTexts();
  check(draw.includes('Linea') && draw.length >= 10, `Draw's tools are in Italian (${draw.join(', ')})`);
  await p.keyboard.press('Escape');
  await top(p, 'Help').click();
  await item(p, 'help.about').click();
  const dlg = p.getByRole('dialog', { name: 'Informazioni su SketchCraft' });
  await dlg.waitFor();
  check((await dlg.locator('.about-tagline').innerText()) === 'Sempre gratis. Mai freemium. Mai a pagamento.' && (await dlg.getByRole('heading', { name: 'Grazie' }).count()) === 1, 'About is in Italian too');
  await p.keyboard.press('Escape');
  await ctx.close();
}

// ---- every layout and theme: the bar fits and its menus open inside the window ---------------------------------------------
for (const layout of ['classic', 'floating', 'side']) {
  for (const skin of ['classic', 'midnight', 'contrast']) {
    const { p, ctx } = await open({ storage: { 'sketchcraft.layout': layout, 'sketchcraft.skin': skin } });
    const where = `${layout} layout, ${skin} theme`;
    const bar = await p.locator('[role=menubar]').boundingBox();
    const win = p.viewportSize();
    check(!!bar && bar.x >= 0 && bar.x + bar.width <= win.width && bar.height > 20 && bar.height < 40, `${where}: the bar is in the window (${Math.round(bar?.width)} wide)`);
    const sw = await p.locator('.ws-switch').boundingBox();
    check(!!sw && bar.x + bar.width <= sw.x, `${where}: the bar doesn't run into the workspace switch`);
    let fine = true;
    for (const n of ['File', 'View', 'Help']) {
      await openMenu(p, n);
      const m = await menu(p, n).boundingBox();
      const topBtn = await top(p, n).boundingBox();
      if (!(m && m.x >= 0 && m.y >= topBtn.y + topBtn.height - 2 && m.x + m.width <= win.width && m.y + m.height <= win.height)) fine = false;
      const bg = await menu(p, n).evaluate((el) => getComputedStyle(el).backgroundColor);
      if (/rgba\(0, 0, 0, 0\)|transparent/.test(bg)) fine = false;
      await p.keyboard.press('Escape');
    }
    check(fine, `${where}: File, View and Help open below their names, in the window, on a solid background`);
    // the long File menu reaches its last item (it scrolls rather than running off a short window)
    await p.setViewportSize({ width: 1700, height: 520 });
    await openMenu(p, 'File');
    const fm = await menu(p, 'File').boundingBox();
    check(fm.y + fm.height <= 520, `${where}: on a short window the File menu stays inside it (${Math.round(fm.y + fm.height)} of 520)`);
    await p.keyboard.press('Escape');
    await ctx.close();
  }
}

// ---- File > Export > FBX writes a real file -----------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  const out = path.join(path.dirname(process.env.SKETCHCRAFT_DOC), 'menu-export.fbx');
  fs.rmSync(out, { force: true });
  const rect = await exec({ cmd: 'rect', from: [0, 0, 0], to: [2, 1, 0] });
  await exec({ cmd: 'pushpull', face: rect.face, distance: 1 });
  p.removeAllListeners('dialog');
  p.on('dialog', (d) => (d.type() === 'prompt' ? d.accept(out) : d.dismiss()).catch(() => {}));
  await openMenu(p, 'File');
  await p.getByRole('menuitem', { name: 'Export' }).hover();
  await item(p, 'file.export.fbx').waitFor();
  check((await item(p, 'file.export.fbx').innerText()).includes('FBX'), 'File > Export lists FBX');
  const sent = execRequest(p, 'export_fbx');
  await item(p, 'file.export.fbx').click();
  await sent.then(() => check(true, 'choosing FBX sends export_fbx'), () => check(false, 'choosing FBX sends export_fbx'));
  for (let i = 0; i < 50 && !fs.existsSync(out); i++) await wait(100);
  const size = fs.existsSync(out) ? fs.statSync(out).size : 0;
  check(size > 0, `the FBX file exists and is not empty (${size} bytes)`);
  check(size > 27 && fs.readFileSync(out).subarray(0, 20).toString('latin1') === 'Kaydara FBX Binary  ', 'it is a binary FBX');
  fs.rmSync(out, { force: true });
  await ctx.close();
}

// ---- narrow windows ----------------------------------------------------------------------------------------------------------
for (const width of [1366, 1280, 1100, 900]) {
  const { p, ctx } = await open({ viewport: { width, height: 800 } });
  const buttons = await p.locator('[role=menubar] > .menubar-item > button').evaluateAll((els) => els.map((e) => e.getBoundingClientRect()));
  const sw = await p.locator('.ws-switch').boundingBox();
  const last = buttons[buttons.length - 1];
  check(buttons.every((r) => r.width > 0) && last.x + last.width <= width, `${width}px wide: every menu name is on screen`);
  check(!sw || last.x + last.width <= sw.x + 1, `${width}px wide: the menus don't overlap the workspace switch (${Math.round(last.x + last.width)} vs ${Math.round(sw?.x)})`);
  await openMenu(p, 'Help');
  const m = await menu(p, 'Help').boundingBox();
  check(m.x >= 0 && m.x + m.width <= width, `${width}px wide: Help opens inside the window`);
  await ctx.close();
}

check(problems.length === 0, `no page errors ${problems.join(' | ').slice(0, 200)}`);
await b.close();
console.log(failed ? `${failed} check(s) failed, ${passed} passed` : `all ${passed} checks passed`);
process.exit(failed ? 1 : 0);
