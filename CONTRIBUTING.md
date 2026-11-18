# Contributing to SketchCraft

Bug reports and ideas: [issues](https://github.com/jasonsutter87/sketchcraft/issues). Where the project is going:
[ROADMAP.md](ROADMAP.md). What people asked for: [FEATURE-REQUESTS.md](FEATURE-REQUESTS.md).
How the code is laid out: [docs/dev/architecture.md](docs/dev/architecture.md). Building and running it:
[docs/dev/building.md](docs/dev/building.md).

## The rule: a feature lands with its tests

The test suite grows with the app. A change that adds or changes behavior comes with the tests that would have caught
it going wrong:

- a **new tool** needs a scenario in `tools/ui-tests/tools.mjs` (the suite reads the tool list from the app's source
  and **fails if a tool has no scenario**);
- a **new engine command** or importer needs a Rust test in `core/tests/`;
- **pure interface code** (parsing, validation, settings, skins) needs a Vitest test in `app/tests/`;
- a **change to how a screen looks** needs the visual references updated (below), and you look at the new pictures
  before you commit them;
- a **bug fix** needs a test that fails without the fix.

A pull request that changes behavior and no test will be asked for one.

## Running the tests

You need Rust (the version in `rust-toolchain.toml`) and Node.js 22. The browser suites also need Playwright (see
"Browser suites" below). **Never run any suite against your real models.** The browser suites start their own
bridge on a scratch model in a temporary folder with its own `HOME`, so `~/Documents/SketchCraft` is never touched.
If you run a Rust test or the CLI by hand, point `SKETCHCRAFT_DOC`, `HOME` and `XDG_CACHE_HOME` at a scratch folder:

```sh
export SCRATCH=$(mktemp -d)
export SKETCHCRAFT_DOC=$SCRATCH/live.sketchcraft HOME=$SCRATCH/home XDG_CACHE_HOME=$SCRATCH/cache
mkdir -p "$HOME" "$XDG_CACHE_HOME"
# (rustup and cargo find their files through HOME: set CARGO_HOME and RUSTUP_HOME to their usual places first
#  if you have changed HOME, or cargo will download everything again)
```

### The engine (Rust)

```sh
cargo test --release -p sketchcraft-core
```

This includes the importers' corruption fuzzing (`core/tests/fuzz.rs`) and the **SketchUp corpus**
(`core/tests/skp_corpus.rs`): five real `.skp` files from SketchUp's own test suite, one per format era, each checked
for its faces, components and placed counts, plus 600 corrupted copies that must never panic, hang or balloon in memory.
The files are MIT licensed, with their license and attribution in `core/tests/skp_corpus/`.

### The interface's unit tests (Vitest)

```sh
cd app
npm ci
npm test              # Vitest: unit conversion, saved settings, skins, plugin forms, CSV export
npx tsc --noEmit -p . # types (tests included)
npm run check         # skin values, the plugin sandbox lockdown, plugin data
```

### The browser suites (Playwright)

These drive the real app in Chromium against the real engine: undo and restore, gestures and Esc, **every tool on the
tool bar** (`tools.mjs`), the plugin sandbox, the SketchUp Ruby scripts, and the visual regression shots.

```sh
cargo build --release -p sketchcraft-core      # the engine the suites run
cd app && npm ci && cd ..
node tools/ui-tests/run.mjs                    # everything
node tools/ui-tests/run.mjs tools gestures     # only suites whose name contains one of these
node tools/ui-tests/run.mjs --list             # what there is
ONLY=line,rect node tools/ui-tests/run.mjs tools   # only the tools' scenarios that cover these
```

`tools/ui-tests/run.mjs` makes a scratch folder, starts `sketchcraft bridge` and the Vite dev page on
`SKETCHCRAFT_DEV_PORT` / `VITE_BRIDGE_PORT` (default 5501 / 5502), runs each suite with a timeout, and stops everything
it started. It also stops itself if what it started grows past `SKETCHCRAFT_RSS_MB` (3072) of memory.

Playwright is not a dependency of the app. Install it where the suites can find it, once:

```sh
mkdir -p ~/sketchcraft-playwright && npm install --prefix ~/sketchcraft-playwright playwright-core@1.64.1
node ~/sketchcraft-playwright/node_modules/playwright-core/cli.js install chromium
export SKETCHCRAFT_PLAYWRIGHT=~/sketchcraft-playwright   # tools/demos/lib.mjs looks there
```

(That is what CI does. `tools/demos/lib.mjs` also finds a `playwright-core` installed beside it, or a copy of
Chrome for Testing in the usual Playwright cache.)

**A note for anyone writing a browser test.** The dev page does not notice changes that another client makes to the
live model through the bridge: it re-reads the model after its *own* commands, and when another *program saves the
file*. A model built by commands sent straight to the bridge (including with `pageExec`, which still sends them to
the bridge) does not appear on a page that is already open until it is reloaded. The suites build their model, reload
the page, then drive the tool with the mouse and keyboard, and ask the bridge (`pageScene`) what happened.

On failure, set `SHOT_DIR` (screenshots) and `SKETCHCRAFT_TRACE_DIR` (Playwright traces; open one with
`npx playwright show-trace`) to keep what the browser saw. CI keeps both for a week.

### Visual regression

`tools/ui-tests/visual.mjs` takes screenshots of the main screens (the three layouts, each in a light, a dark and a
high-contrast theme) and of the main dialogs and menus, and compares each with a reference picture in
`tools/ui-tests/visual-references/<platform>/` (at most 0.3% of pixels may differ). The 3D view is hidden, the fonts
are the bundled ones, and anything driven by the clock is covered, so the same build gives the same picture. Text is
drawn a little differently on macOS and on Linux, so each has its own references.

When you change how something looks **on purpose**:

```sh
node tools/ui-tests/run.mjs visual -- --update    # rewrites this platform's references
git diff --stat tools/ui-tests/visual-references  # then LOOK at the changed pictures before committing them
```

A failed comparison leaves `<name>.actual.png` and `<name>.diff.png` (changed pixels in red) in `$SHOT_DIR/visual`.
If your platform has no references yet, the suite does not fail: it saves what it saw (CI uploads them as the
`visual-candidates` artifact), and committing those, after looking at them, makes the references.

## How changes flow

```
feature/*  --PR-->  develop  --PR-->  staging  --PR-->  main  --tag v*-->  release
```

- **Open your pull request against `develop`** (the default branch), from a short-lived branch such as `feature/hatch-patterns` or `fix/flip-edge`. It's squash-merged when CI is green, and the branch is deleted.
- **`staging`** is the release candidate: `develop` is merged into it when a release is close, and the full browser suite has to pass there.
- **`main`** holds released code only. It takes pull requests from `staging`, and releases are tagged from it.
- The `flow` check refuses a pull request into `staging` that doesn't come from `develop`, and one into `main` that doesn't come from `staging`.
- Every release also passes a **security gate** on `staging` before it reaches `main`: an attack pass, fixes, and a re-test until nothing above Info is left. See [docs/dev/releasing.md](docs/dev/releasing.md).

## What CI checks on a pull request

`.github/workflows/ci.yml` runs on every pull request and every push to `develop`, `staging` and `main`. A change is ready when all of it is green:

| Job | What it runs |
| --- | ------------ |
| `test` | the Rust engine tests (including the SketchUp corpus and the importer fuzzing), `tsc`, `npm run check`, the Vitest unit tests; that every action is pinned to a commit SHA; that CI and `rust-toolchain.toml` name the same Rust |
| `ui-tests` | **on `staging`, `main` and release tags** (and pull requests into them): builds the release engine, installs a pinned Playwright and Chromium, starts a scratch bridge and the dev page, and runs all the browser suites above, visual regression included. A failure uploads screenshots and traces |
| `audit` | no high advisory in the shipped npm packages; lockfiles name only the public registries |
| `flow` | pull requests into `staging` and `main` come from the branch before them |

CI has read-only access to the repository and no secrets. Every third-party action is pinned to a full commit SHA.
The only dependency added for testing is Vitest, at an exact version.

## Pull requests

- Small, logical commits; say what changed and why.
- Run what you touched before you push (the commands above).
- Don't commit models, `.env` files or anything from your home folder.
- Security problems: see [SECURITY.md](SECURITY.md), not a public issue.

## License

By contributing you agree your work is released under the [MIT license](LICENSE).
