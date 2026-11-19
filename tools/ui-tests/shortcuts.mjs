#!/usr/bin/env node
// Customizable keyboard shortcuts in a real browser (app/src/keymap.ts and shortcutseditor.tsx): rebind a tool key
// and use it, keep it across a reload, see a conflict and resolve it, the SketchUp preset, importing SketchUp's
// shortcuts, text fields never firing shortcuts, and junk in storage never stopping the app from starting.
//   node tools/ui-tests/run.mjs shortcuts        (starts a scratch bridge and dev page; see CONTRIBUTING.md)
import { launch, PAGE, pageExec, pageScene } from '../demos/lib.mjs';

if (!process.env.SKETCHCRAFT_DOC) {
  console.error('Set SKETCHCRAFT_DOC to a scratch model path (and run the bridge with the same).');
  process.exit(2);
}
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failed++;
};

const KEY = 'sketchcraft.shortcuts';
const b = await launch();
const problems = [];
const open = async (storage = {}) => {
  const ctx = await b.newContext({ locale: 'en-US', viewport: { width: 1280, height: 860 } });
  await ctx.addInitScript(
    ([s, key]) => {
      try {
        // only the first load of each page: a reload must see what the app saved
        if (sessionStorage.getItem('seeded')) return;
        sessionStorage.setItem('seeded', '1');
        localStorage.setItem('sketchcraft.help', 'off');
        localStorage.setItem('sketchcraft.lessonsDone', '[]');
        for (const [k, v] of Object.entries(s)) localStorage.setItem(k === 'KEY' ? key : k, v);
      } catch {}
    },
    [storage, KEY],
  );
  const p = await ctx.newPage();
  p.on('pageerror', (e) => problems.push(String(e)));
  p.on('dialog', (d) => d.dismiss().catch(() => {}));
  await p.goto(PAGE);
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await wait(500);
  return { p, ctx };
};
/** the tool that is on, by its tool-bar tooltip's name ("Rectangle (R)" is "Rectangle") */
const activeTool = (p) => p.locator('nav.toolbar button.tool.on').first().getAttribute('aria-label').then((t) => (t ?? '').split(/[:(]/)[0].trim(), () => '');
const press = async (p, k) => {
  await p.keyboard.press(k);
  await wait(150);
};
/** the tool bar button's tooltip */
const tip = (p, name) => p.locator(`nav.toolbar button.tool[aria-label^="${name}"]`).first().getAttribute('title');
const row = (p, id) => p.locator(`.key-row[data-cmd="${id}"]`);
const keyOf = (p, id) => row(p, id).locator('.key-btn').innerText();
/** the ? sheet, then its Customize button */
const openEditor = async (p) => {
  await p.locator('body').click({ position: { x: 5, y: 450 } }).catch(() => {});
  await press(p, '?');
  await p.getByRole('button', { name: 'Customize…' }).click();
  await p.waitForSelector('.key-editor');
};
const closeEditor = async (p) => {
  await p.getByRole('button', { name: 'Close', exact: true }).click();
  await wait(150);
};
/** click a command's key and press a new one */
const rebind = async (p, id, key) => {
  await row(p, id).locator('.key-btn').click();
  await press(p, key);
};
const stored = (p) => p.evaluate((k) => localStorage.getItem(k), KEY);
const isMac = process.platform === 'darwin';

// ---- the defaults still work ---------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await press(p, 'l');
  check((await activeTool(p)) === 'Line', 'L picks Line with the default keys');
  await press(p, 'c');
  check((await activeTool(p)) === 'Circle', 'C picks Circle');
  check(/\(L\)/.test((await tip(p, 'Line')) ?? ''), `the Line button's tooltip shows its key: ${await tip(p, 'Line')}`);
  await ctx.close();
}

// ---- a language change only affects the presentation -------------------------------------------------------------
{
  const { p, ctx } = await open({ 'sketchcraft.language': 'en' });
  const sendNew = async (name) => {
    const request = p.waitForRequest((r) => r.method() === 'POST' &&
      new URL(r.url()).pathname === '/exec' && r.postDataJSON()?.cmd === 'new');
    await p.getByRole('menuitem', { name }).click();
    return (await request).postDataJSON();
  };
  await p.getByRole('menuitem', { name: 'File', exact: true }).click();
  const english = await sendNew(/^New/);
  await press(p, 'l');
  check((await activeTool(p)) === 'Line', 'L selects Line before changing language');

  await p.getByRole('menuitem', { name: 'View', exact: true }).click();
  await p.getByRole('menuitem', { name: 'Language' }).click();
  await p.getByRole('menuitemradio', { name: 'Italiano' }).click();
  check(await p.locator('html').getAttribute('lang') === 'it', 'the language switches without a reload');
  await p.getByRole('menuitem', { name: 'File', exact: true }).click();
  const italian = await sendNew(/^Nuovo/);
  check(JSON.stringify(english) === JSON.stringify(italian), 'New and Nuovo send the same engine command');
  await p.locator('body').click({ position: { x: 5, y: 450 } });
  await press(p, 'c');
  await press(p, 'l');
  check((await activeTool(p)) === 'Linea', 'L still selects Line with the Italian label');
  check(/\(L\)/.test((await tip(p, 'Linea')) ?? ''), 'the Italian tool retains its original shortcut');
  await p.reload();
  await p.waitForSelector('html[lang="it"]');
  await ctx.close();
}

// ---- rebind a tool key, use it, keep it across a reload ---------------------------------------------------------------
{
  const { p, ctx } = await open();
  await openEditor(p);
  check(/^L$/.test(await keyOf(p, 'tool:line')), 'the editor shows Line on L');
  await rebind(p, 'tool:line', 'y');
  check(/^Y$/.test(await keyOf(p, 'tool:line')), 'the editor shows the new key');
  check(/\(Y\)/.test((await tip(p, 'Line')) ?? ''), `the tool tip changed at once: ${await tip(p, 'Line')}`);
  await closeEditor(p);
  await press(p, 'c');
  await press(p, 'y');
  check((await activeTool(p)) === 'Line', 'the new key Y picks Line');
  await press(p, 'c');
  await press(p, 'l');
  check((await activeTool(p)) === 'Circle', 'the old key L no longer picks Line');
  check(/"tool:line":\s*"Y"/.test((await stored(p)) ?? ''), 'it was saved');

  await p.reload();
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await wait(500);
  await press(p, 'c');
  await press(p, 'y');
  check((await activeTool(p)) === 'Line', 'the new key still works after a reload');
  check(/\(Y\)/.test((await tip(p, 'Line')) ?? ''), 'and the tooltip still shows it');
  // the ? sheet and the command search show it too
  await press(p, '?');
  const sheet = await p.locator('.sheet-sec', { hasText: 'Line' }).first().innerText();
  check(/Y\s*\n?\s*Line/.test(sheet), 'the ? sheet lists Line on Y');
  await press(p, 'Escape');
  await press(p, 'Shift+S');
  await p.locator('.palette input').fill('line');
  check((await p.locator('.palette li.on kbd').first().innerText().catch(() => '')) === 'Y', 'the command search shows Line on Y');
  await press(p, 'Escape');

  // Reset puts it back
  await openEditor(p);
  await row(p, 'tool:line').getByRole('button', { name: 'Reset Line' }).click();
  check(/^L$/.test(await keyOf(p, 'tool:line')), 'Reset brings the preset key back');
  await ctx.close();
}

// ---- a conflict is shown, and resolved by swapping or clearing --------------------------------------------------------
{
  const { p, ctx } = await open();
  await openEditor(p);
  await rebind(p, 'tool:rect', 'l'); // Line has L
  const notice = p.locator('.key-notice');
  check((await notice.innerText().catch(() => '')).includes('already') && (await notice.innerText()).includes('Line'), `a key already used says so: ${(await notice.innerText().catch(() => '')).replace(/\s+/g, ' ')}`);
  check(/^R$/.test(await keyOf(p, 'tool:rect')), 'nothing changed yet');
  await notice.getByRole('button', { name: 'Swap' }).click();
  check(/^L$/.test(await keyOf(p, 'tool:rect')) && /^R$/.test(await keyOf(p, 'tool:line')), 'Swap trades the two keys');
  // clear: Circle takes L from Rectangle
  await rebind(p, 'tool:circle', 'l');
  await notice.getByRole('button', { name: /^Clear Rectangle/ }).click();
  check(/^L$/.test(await keyOf(p, 'tool:circle')) && /none/.test(await keyOf(p, 'tool:rect')), 'Clear takes the key and leaves the other without one');
  // a reserved key is refused, with the reason
  await row(p, 'tool:arc').locator('.key-btn').click();
  await press(p, 'Enter');
  check((await p.locator('.key-notice').innerText().catch(() => '')).includes('Enter'), 'Enter is refused with a reason');
  await press(p, 'Escape');
  check((await p.locator('.key-editor').count()) === 1 && /^A$/.test(await keyOf(p, 'tool:arc')), 'Esc cancels the pick and leaves the editor open');
  await closeEditor(p);
  await ctx.close();
}

// ---- search, clear, reset all, the SketchUp preset -----------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await openEditor(p);
  await p.getByLabel('Search shortcuts').fill('push');
  check((await p.locator('.key-row').count()) === 1 && (await row(p, 'tool:push').count()) === 1, 'search narrows the list to Push / Pull');
  await p.getByLabel('Search shortcuts').fill('');
  await row(p, 'tool:circle').getByRole('button', { name: /Clear Circle/ }).click();
  check(/none/.test(await keyOf(p, 'tool:circle')), 'Clear takes a key away');
  await p.getByRole('button', { name: /^Reset all/ }).click();
  check(/^C$/.test(await keyOf(p, 'tool:circle')), 'Reset all puts every key back');

  check(/^N$/.test(await keyOf(p, 'tool:polygon')), 'SketchCraft has Polygon on N');
  await p.getByRole('button', { name: 'SketchUp', exact: true }).click();
  check(/none/.test(await keyOf(p, 'tool:polygon')) && /^L$/.test(await keyOf(p, 'tool:line')) && /^Space$/.test(await keyOf(p, 'tool:select')), 'the SketchUp preset applies: no key for Polygon, the rest the same');
  await closeEditor(p);
  await press(p, 'l');
  await press(p, 'n');
  check((await activeTool(p)) === 'Line', 'N does nothing in the SketchUp preset');
  await press(p, 'p');
  check((await activeTool(p)) === 'Push / Pull', 'P still picks Push / Pull');
  await press(p, 'Space');
  check((await activeTool(p)) === 'Select', 'Space picks Select');
  check(/"preset":\s*"sketchup"/.test((await stored(p)) ?? ''), 'the preset was saved');
  await ctx.close();
}

// ---- typing in a text field never fires a shortcut ----------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await press(p, 'p');
  await openEditor(p);
  await p.getByLabel('Search shortcuts').fill('');
  await p.getByLabel('Search shortcuts').click();
  await p.keyboard.type('lrpe', { delay: 30 });
  check((await p.getByLabel('Search shortcuts').inputValue()) === 'lrpe' && (await activeTool(p)) === 'Push / Pull', 'typing in the search box leaves the tool alone');
  // and the measurement box: a number starts it, then letters stay in it
  await p.keyboard.press('Escape');
  await wait(200);
  await press(p, '5');
  await p.keyboard.type('l', { delay: 30 });
  check((await p.locator('.measure input').first().inputValue().catch(() => 'x')).startsWith('5') && (await activeTool(p)) === 'Push / Pull', 'letters typed into the measurement box stay in it');
  await ctx.close();
}

// ---- importing SketchUp's shortcuts -----------------------------------------------------------------------------------------
{
  const { p, ctx } = await open();
  await openEditor(p);
  await p.getByRole('button', { name: 'Import / export' }).click();
  // the text of a Windows-style export, with a command that has no match here
  const sample = ['Tools/Push/Pull = X', 'Draw/Line = J', 'Tools/Offset = Ctrl+Shift+F', 'Edit/Undo = Ctrl+Z', 'Window/Model Info = Shift+I', ''].join('\n');
  await p.getByLabel('Shortcuts file text').fill(sample);
  await p.getByRole('button', { name: 'Import', exact: true }).click();
  const report = await p.locator('.key-report').innerText();
  check(/Imported \d+/.test(report) && /Model Info/.test(report), `the import says what it did and what had no match: ${report.replace(/\s+/g, ' ').slice(0, 120)}`);
  check(/^X$/.test(await keyOf(p, 'tool:push')) && /^J$/.test(await keyOf(p, 'tool:line')), 'the imported keys are set');
  check(/none/.test(await keyOf(p, 'tool:polygon')), 'the rest follows the SketchUp preset');
  await p.getByLabel('Shortcuts file text').fill('this is not a shortcuts file');
  await p.getByRole('button', { name: 'Import', exact: true }).click();
  check(/doesn't look like/.test(await p.locator('.key-report').innerText()) && /^X$/.test(await keyOf(p, 'tool:push')), 'a file that is not shortcuts changes nothing');
  // an unrelated text file that merely has "name: value" lines changes nothing either (it must not wipe your keys)
  await p.getByLabel('Shortcuts file text').fill('title: My notes\nauthor: someone\ncolor = blue\n');
  await p.getByRole('button', { name: 'Import', exact: true }).click();
  check(/Nothing in that file matched/.test(await p.locator('.key-report').innerText()) && /^X$/.test(await keyOf(p, 'tool:push')), 'an unrelated "name: value" file leaves your keys alone');
  // export, change something, import it back
  await p.getByRole('button', { name: 'Export', exact: true }).click();
  const json = await p.getByLabel('Shortcuts file text').inputValue();
  check(json.includes('"sketchcraft-shortcuts"') && json.includes('"tool:line": "J"'), 'Export writes our JSON');
  await p.getByRole('button', { name: /^Reset all/ }).click();
  await p.getByRole('button', { name: 'SketchCraft', exact: true }).click();
  check(/^L$/.test(await keyOf(p, 'tool:line')), 'reset again');
  await p.getByLabel('Shortcuts file text').fill(json);
  await p.getByRole('button', { name: 'Import', exact: true }).click();
  check(/^J$/.test(await keyOf(p, 'tool:line')) && /^X$/.test(await keyOf(p, 'tool:push')), 'our own file imports back the same');
  await closeEditor(p);
  await press(p, 'j');
  check((await activeTool(p)) === 'Line', 'and the imported key works');
  await ctx.close();
}

// ---- keys the viewport and the app keep: Space (hold to orbit, tap for Select) and Backspace (a second Delete) --------------------
{
  const { p, ctx } = await open();
  await press(p, 'l');
  await p.keyboard.down('Space');
  await wait(300);
  check((await activeTool(p)) === 'Line', 'holding Space leaves the tool you are using alone (it orbits)');
  await p.keyboard.up('Space');
  await wait(300);
  check((await activeTool(p)) === 'Select', 'tapping Space picks Select');
  // a line in progress survives holding Space: click the start, hold Space (with auto-repeat), click the end
  await pageExec(p, { cmd: 'new' });
  await p.reload();
  await p.waitForSelector('.viewport canvas', { state: 'attached' });
  await wait(700);
  await press(p, 'l');
  const box = await p.locator('.viewport canvas').boundingBox();
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await p.mouse.click(cx - 80, cy);
  await wait(300);
  await p.keyboard.down('Space');
  await p.keyboard.down('Space'); // auto-repeat
  await wait(300);
  const still = await activeTool(p);
  await p.mouse.move(cx + 80, cy);
  await p.keyboard.up('Space');
  await wait(200);
  check(still === 'Line', 'holding Space in the middle of a line keeps the Line tool');
  await press(p, 'l');
  await p.mouse.click(cx - 80, cy);
  await wait(200);
  await p.mouse.click(cx + 80, cy);
  await wait(500);
  check((await pageScene(p)).edges.length > 0, 'and a line can still be drawn afterwards');
  // Delete and Backspace both delete the selection
  for (const key of ['Delete', 'Backspace']) {
    await pageExec(p, { cmd: 'new' });
    await pageExec(p, { cmd: 'rect', from: [0, 0, 0], to: [2, 2, 0] });
    await p.reload();
    await p.waitForSelector('.viewport canvas', { state: 'attached' });
    await wait(700);
    await p.keyboard.press('Control+a');
    await wait(200);
    const before = (await pageScene(p)).faces.length;
    await p.keyboard.press(key);
    await wait(500);
    const after = (await pageScene(p)).faces.length;
    check(before > 0 && after === 0, `${key} deletes the selection (${before} faces, then ${after})`);
  }
  await ctx.close();
}

// ---- junk in storage never stops the app -----------------------------------------------------------------------------------
for (const [what, value] of [
  ['text that is not JSON', '{not json'],
  ['null', 'null'],
  ['a list', '[1,2,3]'],
  ['the wrong kinds', '{"preset":5,"overrides":{"tool:line":5,"tool:rect":{"a":1},"tool:circle":[],"no.such":"K"}}'],
  ['reserved and nonsense keys', '{"preset":"sketchup","overrides":{"tool:line":"Escape","tool:rect":"Enter","tool:move":"Ctrl+Banana"}}'],
  ['a partly good value', '{"overrides":{"tool:line":"J","tool:rect":99,"tool:circle":"Escape"}}'],
]) {
  const { p, ctx } = await open({ KEY: value });
  const sawCanvas = (await p.locator('.viewport canvas').count()) > 0;
  await press(p, 'c');
  const eraser = (await activeTool(p)) === 'Circle';
  await press(p, 'r');
  const rect = (await activeTool(p)) === 'Rectangle';
  check(sawCanvas && eraser && rect, `junk in storage (${what}): the app starts and the default keys work`);
  if (what === 'a partly good value') {
    await press(p, 'j');
    check((await activeTool(p)) === 'Line', 'the good part of a partly good value is kept');
  }
  await ctx.close();
}

check(problems.length === 0, `no page errors ${problems.slice(0, 3).join(' | ').slice(0, 300)}`);
void isMac;

await b.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
