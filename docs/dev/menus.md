# The menu bar and About

The top bar has a menu bar: **File, Edit, View, Camera, Draw, Tools, Window, Help**. It replaced the single File
drop-down. Everything the File drop-down held is still reachable: Keyboard shortcuts and Check for updates are in Help,
Customize shortcuts is in Window, Language is in View, and the version is in Help > About.

## Where things are

| File | What it is |
| --- | --- |
| `app/src/menumodel.ts` | The menus as data: names, items, submenus, and the **action id** of each item. Pure. |
| `app/src/menuactions.ts` | `buildMenuActions(ctx)`: what each id does, and whether it is available or ticked now. Pure. |
| `app/src/menubar.tsx` | Draws the bar and its menus, and handles pointer and keyboard behaviour. |
| `app/src/about.tsx` | Help > About: name, version, the promise, links, the Thank you list. |
| `app/src/links.ts` | The few pages the menus link to (`open_link` in `src-tauri/src/main.rs` opens them by name). |
| `app/contributors.mjs` | Reads the contributor list from git history at build time. |
| `app/src/styles/_menubar.scss` | Styles for the bar, its menus and About. |

`App.tsx` builds a `MenuCtx` (the state the menus read and the handlers they call) and passes the result to
`<MenuBar>`. The handlers are the same ones the keyboard, the command search (Shift+S) and the buttons use, so a menu
item never has a second copy of anything.

## Adding an item

1. Add it to `MENUS` in `menumodel.ts` with an `id`. If the command has a shortcut, use the id the shortcuts list
   (`keymap.ts`) uses ("file.save", "tool:line", "view.xray"): the key shown beside the item is then whatever the person
   has set, and nothing else is needed. A new tool needs only `tool("name")` (its `tool.<name>` message is already
   there; the menu shows the part before the first ":" or "(").
2. Give it an action in `buildMenuActions` (`menuactions.ts`): `add(id, run, enabled, checked?)`. Use `enabled` to grey it
   out, `checked` for check and radio items.
3. Put its label in `locale-en.ts` and `locale-it.ts` (`menu.*`), or give the item `text` for a name that is the same in
   every language.

`app/tests/menus.test.ts` fails if an item has no action, an action no menu offers, a label with no English or Italian
text, or a tool the app doesn't have.

## Behaviour

- Click a name to open its menu; click it again or click away to close. With one open, pointing at another name opens
  that one.
- Roles: `menubar` > `menuitem` (each with a `menu`) > `menuitem`, `menuitemcheckbox` (View toggles) and
  `menuitemradio` (workspace, face style, background, projection, layout, language).
- Keyboard: **F10** puts the focus in the bar (so does tapping **Alt** on Windows and Linux; on a Mac Option is the copy
  key, so only F10 there). Left and Right move between menus, Down or Up opens one, Up and Down move through items
  (wrapping), Home and End jump, Right opens a submenu (or goes to the next menu), Left closes it (or goes to the previous
  menu), Enter or Space runs the item, **Esc** closes (the focus stays on the name; Esc again leaves the bar).
- Choosing an item closes the menu and gives the focus back to the model, so the tool keys keep working.
- An item that doesn't apply is greyed out: Cut, Copy, Delete, Group, Hide with nothing selected; Paste with nothing
  copied; Explode unless a component is selected; the model's tools, views and edit commands while the Layout workspace
  is up; Hide rest of model unless a group is open. Check for updates is left out of the browser build entirely.
- Submenus are drawn `position: fixed` beside their parent (a scrolling menu would clip them) and flip to the left when
  there is no room on the right. A menu taller than the window scrolls.
- On narrow windows the Live badge, then the name and the workspace switch (View > Workspaces has them), give way to the
  menus.

## Help > About and the Thank you list

About shows the icon, name and version (`__APP_VERSION__`), the line **"Always free. Never freemium. Never paid."**
under the name with "It's a promise." below it, links to the repository, the MIT license and the changelog, and a
**Thank you** list of everyone who has written code.

The list is **generated at build time**, never kept by hand: `vite.config.ts` runs `readContributors()` and bakes the
result in as `__CONTRIBUTORS__` (so `vite`, `npm run build`, `tauri dev` and CI all get it).

- It reads the authors of `git log` (so `.mailmap` is honored). "Co-authored-by" lines are ignored.
- Bots are left out (`dependabot`, `github-actions`, anything ending in `[bot]`, GitHub's own `noreply@github.com`).
- One person once: commits are merged by email, by GitHub login (from a `users.noreply.github.com` address) and by
  name.
- **Order: by first contribution**, oldest first. That puts the person who started the project first and the others in
  the order they joined, with no list of names in the code. (Alphabetical was the other choice.)
- No `.git`, no `git`, or an error: an empty list, and About leaves the section out. It never fails the build.
- **Shallow clones.** CI checks out one commit (`actions/checkout` with the default depth), which would list one author.
  A shallow repository is deepened with `git fetch --unshallow` (a public repository needs no credentials); if that
  fails the list is empty rather than wrong. `SKETCHCRAFT_NO_UNSHALLOW=1` skips the fetch. In the release workflow the
  `frontend` job (which runs `npm run build` once for every platform) checks out a single commit, so it relies on that
  fetch; a `fetch-depth: 0` on that checkout would make it certain.

## Not done: the native macOS menu

Tauri 2 can build a native menu bar (`tauri::menu`) and send each choice to the page, which could run the same
actions. It is left out for now: replacing the default macOS menu drops its Edit items (so ⌘C and ⌘V stop working in
text fields) unless they are rebuilt as predefined items, the item states (greyed, ticked, shortcuts the person has
changed) would have to be pushed from the page to Rust on every change, and none of it can be checked without running
a built, signed desktop app. The in-app bar works in the desktop app and in the browser on every platform.
