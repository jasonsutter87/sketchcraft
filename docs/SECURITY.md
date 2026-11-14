# Security

SketchCraft opens files people send each other (SketchUp, FBX, DXF, DWG, OBJ)
and can be driven by tools (MCP, the dev bridge). This page says what's
exposed, what has been tested, and how to test it again.

## What's exposed

| Surface | Who can reach it | Guard |
| --- | --- | --- |
| Desktop app (Tauri) | the app's own window | three IPC calls only (`exec`, `scene`, `sync`); `core:default` + `dialog:default` permissions; strict Content-Security-Policy (own files only, no remote scripts) |
| Dev bridge (`sketchcraft bridge`, 127.0.0.1:5175) | programs on this Mac | refuses any browser `Origin` but the dev page (other websites can't send it commands); refuses any `Host` but 127.0.0.1/localhost (no DNS rebinding); 64 MB request cap |
| MCP server | the agent you connect | stdio only; no network listener |
| File importers | anyone who sends you a file | fuzzed (below); limits on sizes, depth and parse time |
| External programs (ffmpeg, DWG converters, `open`) | paths you choose | arguments, never a shell; paths can't be read as flags; Reveal shows apps, never launches them |

## Review, 2026-11-13

| # | Finding | Severity | Fix |
| --- | --- | --- | --- |
| 1 | Bridge ran commands from any website the user visited (a plain POST needs no CORS preflight), and was open to DNS rebinding: arbitrary file writes | Critical | Origin + Host checks, body cap (`core/src/http.rs`) |
| 2 | Desktop app had no Content-Security-Policy | High | strict CSP (`app/src-tauri/tauri.conf.json`) |
| 3 | A 1 KB FBX with a node length pointing backwards looped until the app ran out of memory | High | node ends must follow their start (`core/src/fbx.rs`); regression test |
| 4 | FBX arrays reserved their declared size up front and inflated zlib data without a ceiling | Medium | reserve as data arrives, stop at the declared size, reject mismatches |
| 5 | A damaged SketchUp file kept the importer searching for 29 s | Medium | a whole-file deadline (4 s + 4 s/MB; real files take < 0.4 s) |
| 6 | Paths starting with `-` became options to ffmpeg / `open` / converters; Reveal could launch an `.app` | Medium | `safety::path_arg`; Reveal uses `open -R` for bundles |
| 7 | Video frames were unbounded | Low | frame index and size limits |
| 8 | A panic in one command would poison the engine lock and break the app | Low | IPC calls and the bridge catch panics and answer with an error |
| 9 | `zip` 2.6 (RUSTSEC-2026-0367/0368/0369/0429; we never called `extract`), unsound `i_tree` via `geo` | Medium (deps) | zip 9, geo 0.33 |

Not fixed, and why: `glib` (unsound) and `proc-macro-error` (unmaintained)
come from Tauri's Linux GTK layer, aren't in the Mac build, and need a Tauri
release. TypeScript 7 (a new compiler) is left for a deliberate upgrade.

## Testing it again

```sh
cargo audit                                   # Rust advisories (0 vulnerabilities on 2026-11-13)
(cd app && npm audit)                         # npm advisories (0)
cargo test --release --test fuzz -- --ignored --nocapture       # DXF, OBJ (20,000 cases each), SketchUp samples
cargo test --release --test fbx -- --ignored --nocapture        # FBX (20,000 cases)
FUZZ_TRACE=1 FUZZ_SKP_ITERS=150 cargo test --release --test fuzz fuzz_skp -- --ignored --nocapture
```

The fuzzers save any input that panics or takes over 2 s to `out/fuzz/`;
`FUZZ_TRACE=1` also writes each input before running it, so a crash that
can't be caught (an abort, running out of memory) leaves its input behind.
SketchUp fuzzing uses your own sample files and never commits them.

Report a problem privately: open a GitHub security advisory on the repo.
