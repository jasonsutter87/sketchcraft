# Releasing SketchCraft

How a version goes from `develop` to the people who use SketchCraft. Every step is
required, and none of them is skipped for a "small" release.

```
develop ──PR──▶ staging ──security gate──▶ PR ──▶ main ──tag v*──▶ release ──▶ issues closed
```

## 1. Cut a release candidate

1. On `develop`, bump the version in `app/src-tauri/tauri.conf.json`, `app/package.json`,
   `app/package-lock.json`, `app/src-tauri/Cargo.toml`, `core/Cargo.toml` and `Cargo.lock`. Turn the
   CHANGELOG's **Unreleased** section into the new version. Do this through a `release/x.y.z` PR.
2. Open a PR from `develop` into `staging`. CI runs the full browser suite there: every tool,
   screenshot comparisons, Ruby, the plugin sandbox, the SketchUp corpus. It has to be green.

## 2. The security gate (every release, from 0.3.8 on)

Before a release candidate reaches `main`, it's attacked:

1. **Mr BlackKeys** attacks the candidate on `staging`, concentrating on what's new since the last
   release: new commands, file formats, importers, UI paths, anything an AI agent, a plugin or a
   hostile file can reach. **Burn1t** joins when resilience or performance changed. Reports go in
   `security-reports/R<version>-*.md` (git-ignored, never published as-is).
2. **Crimson** fixes everything rated Low or above. Each fix comes with a test, and goes through the
   normal flow: `feature/*` → `develop` → `staging`.
3. **Mr BlackKeys re-tests** the new candidate. Repeat until nothing above **Info** is left.
4. Owner-only risks (keys, signing, repository settings) are written up for the owner. Agents never
   accept risk themselves.

Rules for the attackers:
- scratch models only (`SKETCHCRAFT_DOC`, `HOME`, `XDG_CACHE_HOME` in a scratch folder)
- a memory watchdog (about 3 GB per process) and a timeout on every test
- one heavy job at a time
- nothing left running afterwards

## 3. Ship

1. Open a PR from `staging` into `main` and merge it (a merge commit, not a squash).
2. Tag `main`: `git tag -a vX.Y.Z -m "SketchCraft X.Y.Z" && git push origin vX.Y.Z`. `release.yml`
   builds every platform, checks that the signing key matches the app's update key (see
   [SIGNING.md](../SIGNING.md)), signs, and makes a **draft** release.
3. Check the draft before publishing:
   - every file is there, including the portable ZIP, the CLI zips and `SHA256SUMS.txt`
   - `latest-v2.json` names the new version, signed with the current key
   - `latest.json` is still the frozen 0.3.6 bridge
   - write the release notes
4. Publish. Then merge `main` back into `develop` so the branches stay in step.

## 4. After the release

- Comment on and close the milestone's issues, and close the milestone. Commits never say
  "fixes #N", so issues close here, when people can actually get the fix.
- Reply to anyone who reported something that's now fixed.
