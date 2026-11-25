# Tombstones: finding code nothing reaches

SketchCraft grows fast, so code that nothing calls anymore piles up. A **tombstone** is a mark on a function. Run every
test suite on a copy of the code that has the marks; a mark nobody walked past is a lead for dead code. The tooling is
in `tools/tombstones/`. It changes nothing in a normal build: the marks only exist in a throwaway checkout.

Nothing here removes code by itself. The default output is a **report**.

## The flow

```
tombstone/<date> checkout (throwaway)
  1. mark          put  // @debug tomb:<id>  at the top of functions (and, with --blocks, branches)
  2. probe         TS: a Vite plugin turns each mark into __tomb("<id>")   Rust: rewrite-rust turns it into crate::__tomb!("<id>")
  3. run tests     cargo test, Vitest, Playwright: each id is written to tomb-hits.log once
  4. report        remove the marks that were hit; write .tombstones/report.md and report.json
  5. (on request)  deprecate: turn report entries into @deprecate blocks for code-janitor
```

### Run it

```sh
# 0. a throwaway checkout on a branch named tombstone/... (mark refuses any other branch unless --force)
git worktree add -b tombstone/2026-12-01 ../sketchcraft-tomb origin/staging
cd ../sketchcraft-tomb && (cd app && npm ci)
T=tools/tombstones/tomb.mjs

# 1. marks (a slice with --only, comma separated globs; --lang ts,rust; --blocks adds else/catch/case/match-arm blocks)
node $T mark --root .

# 2-3. run the suites with the log switched on. Use a fresh log every run.
export SKETCHCRAFT_TOMB_LOG=$PWD/tomb-hits.log; rm -f $SKETCHCRAFT_TOMB_LOG
node $T rewrite-rust --root .                 # Rust only: marks become probes, core/src/__tomb.rs is added
cargo test --release -p sketchcraft-core      # scratch HOME as in CONTRIBUTING.md
(cd app && npm test)                          # the Vite plugin loads because the env var is set
node tools/ui-tests/run.mjs                   # the dev page loads it too; the page POSTs hits to /__tomb

# 4. the report (a script after the runs, not a watcher)
node $T report --root .                       # reads $SKETCHCRAFT_TOMB_LOG, uses tools/tombstones/by-hand.allow
```

`report` refuses to run when the log is empty (a missing log would make everything look dead). It removes only the
mark lines that were hit, puts the Rust probes back to plain marks (or drops them), and removes `__tomb.rs`. What is
left in `git diff` against the base is the list of marks still standing: the same list as `report.md`.

Run the browser suites on non-default ports as usual (`SKETCHCRAFT_DEV_PORT`, `VITE_BRIDGE_PORT`), and never against
`~/Documents/SketchCraft`.

### What the marks look like

```ts
export function priorityOrder(tags: string[]) {
  // @debug tomb:d97e1182
  return ...
}
```

- **Granularity.** Named functions and methods in `app/src/**/*.ts(x)` (not `*.test.*`, not `.d.ts`; not
  constructors, anonymous callbacks or one-expression arrows: the function around them carries the mark) and `fn`
  bodies in the library crate `core/src` (not `const fn`, `#[test]`, `#[cfg(test)]` modules or `macro_rules!`). With
  `--blocks`: `if`/`else`/`catch`/`case` in TypeScript, `=> {` arms and `else {` in Rust. Blocks are noisy and are never
  turned into deprecations automatically.
- **Ids.** 8 hex digits of `sha1(file | symbol | kind | n-th of that)`. Moving code around in a file keeps the id;
  renaming the function or moving the file changes it. The manifest (`.tombstones/manifest.json`) records id, file,
  line (in the base checkout), end line, symbol and kind.
- **The probes.** TypeScript: `tools/tombstones/vite-plugin.mjs`, loaded by `app/vite.config.ts` and
  `app/vitest.config.ts` only when `SKETCHCRAFT_TOMB_LOG` is set. In Node it appends to the log; in the browser and in
  workers it batches ids and sends them to the dev server's `/__tomb` endpoint. Rust: each probe site is a
  `static AtomicBool`, so after the first hit it costs one relaxed swap; each test process writes an id once, with a
  single `write` call so parallel tests never tear a line.
- **Zero cost.** With the env var unset, the plugin is never imported. Without the rewrite, a mark is a comment. The
  generated files (`.tombstones/`, `tomb-hits.log`) are in `.gitignore`.

### "Check by hand"

`tools/tombstones/by-hand.allow` lists the places the suites cannot reach (old file formats, native-only code, Android
and phone code, rare dialogs). One rule per line, `path-glob[::symbol-glob]   # why`. A mark that matches goes to the
report's own **Check by hand** section with its reason and is never deprecated automatically. The seed list is a
guess: tune it as reports show what really is out of reach.

### Deprecating (only on request)

```sh
node $T deprecate --root . --remove-on 12-15-2026 --remove-in 0.3.12            # dry run: lists what it would do
node $T deprecate --root . --remove-on 12-15-2026 --remove-in 0.3.12 --write    # comments the functions out
```

This is code-janitor's format (its README and `parser.ts`): a header `// @deprecate MM-DD-YYYY reason: ...` and the
deprecated code commented out under it; code-janitor deletes the block once the date has passed. A removal *version*
is not something code-janitor understands, so the version goes in the reason text and the date is the machine
deadline (choose the release date). Only whole functions and methods are commented out (with their attributes and doc
comment); blocks are listed as "edit by hand". Then run the build and the tests: if something still called the
function, the compiler says so, and you put it back. `--ids a,b` picks entries.

```ts
// @deprecate 12-15-2026 reason: tombstone 2d57d917 (deadHelper) was not reached by any test run; remove in 0.3.12
// /** Nobody calls this. */
// export function deadHelper(a: number): number {
  // return a * 3;
// }
```

## The open questions

### Marks or coverage data?

Coverage answers the same question without comments. Honest comparison:

| | marks (this tool) | coverage |
|---|---|---|
| Rust | needs the finder, the rewrite and a module in the crate; works with plain `cargo test` | `cargo llvm-cov --lcov --release` with no source change; line and branch precision; needs the `llvm-tools` component and an instrumented build (slower, a second `target/`) |
| Vitest | a Vite plugin | `vitest run --coverage` (v8) gives lcov out of the box |
| Playwright / browser | the same plugin; the dev page POSTs ids; also works in workers | `page.coverage.startJSCoverage()` gives byte ranges of Vite's per-module output; mapping them back to `.ts` needs source maps and a converter (v8-to-istanbul, c8 or monocart); every suite file in `tools/ui-tests` must start and stop it; the plugin sandbox workers are not covered by the page API |
| Effort to keep | marks are rewritten on every run on a fresh checkout | one extra command per runner, nothing touches the source |
| Answers | "was this function entered" | per-line, per-branch |

Verdict: **for Rust, coverage is clearly simpler and better** (no source change, finer detail). **For the browser
side, marks are simpler today**: the Vite plugin covers the dev page and workers with about a hundred lines, while the
V8-coverage route needs a converter and edits to every Playwright suite. So the recommendation is a mix: lcov for
Rust, marks for TypeScript, until someone wants the V8 bridge. Both feed the same report: the manifest (marks are
optional for the lcov route: only the manifest matters, and `mark` on a scratch copy produces it) is matched against
an lcov file by function start line:

```sh
cargo llvm-cov --release --lcov --output-path rust.lcov -p sketchcraft-core   # on the throwaway checkout
node $T report --root . --hits tomb-hits.log --lcov rust.lcov
```

With `--lcov`, a mark counts as reached when an lcov function that starts in its signature has a count above 0 (blocks:
a counted line inside the block). Marks the lcov does not mention are left out of the report instead of being called
dead. The lcov path is tested on fixtures only: `cargo llvm-cov` is not installed on the machine this was built on, so
no real llvm-cov output has been fed to it yet.

### Paths only a person reaches

The allowlist above. A "never reached" mark in one of those files is a question for a person, not a candidate.
Also remember what the suites do not run at all: the packaged Tauri shell, Android, real GPU paths, plugins people
install. When in doubt, add the path to the list.

### Cadence, and report or PR

- **Cadence:** once per release, on the staging branch, before the red team (a full Playwright run is already part of
  that). Weekly would mostly produce the same report, and each run costs a full suite run plus an instrumented build.
- **Report, not PR.** The run writes `report.md` and `report.json` and stops. A person reads it, adds by-hand entries
  to the allowlist, and picks entries to deprecate (`deprecate --write`, then build and test). The cleanup, never the
  marks, goes into a normal PR to staging, and code-janitor removes the expired blocks later in its own PR. Opening
  PRs automatically is a later step, once a few reports have shown that the false-positive rate is low.
- **False positives to expect:** code reached only through `eval`-like dispatch by name, code used only by a
  test that is not in the run, and anything behind a feature the suites never switch on. That is why a candidate is a
  lead, and why deprecation comments the code out for a release instead of deleting it.
