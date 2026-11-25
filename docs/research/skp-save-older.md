# Saving .skp files that older SketchUp versions can open (spike for #38)

Status: research spike, no code. Date: 2026-11-24.

## Recommendation

**No-go on building a native .skp writer or version converter now. Conditional go on one small thing, later:
an optional, user-installed helper**, the same pattern SketchCraft already uses for DWG and IFC.

- **No-go, write the 2021+ format ourselves.** Nobody needs it: SketchUp 2021 and later open each other's files
  (open-ended format), so there is no demand to serve, and a wrong file would damage a user's model.
- **No-go, write the 2017-2020 (or older) binary ourselves.** This is the real demand, but it is a clean-room
  writer for a format we only read from samples, with no way to prove a file is accepted without owning every
  old SketchUp. Weeks of work for a feature whose failure mode is "file will not open".
- **No-go, bundle the SketchUp C SDK in SketchCraft.** The licence is not clear enough to ship (see below), and
  the SDK is a closed binary dependency in an app that is clean-room by design.
- **Later, only if asked for: a helper.** A separate, optional tool (the user installs the SDK or SketchUp
  themselves) that SketchCraft hands a file to. Cost is small, risk is the user's, and nothing is bundled.
- **Now, nearly free:** a short guide entry telling people what works today (see the workarounds section) and that
  SketchCraft already exports OBJ, FBX, DXF, DWG, IFC and Collada, which older SketchUp versions can import.

Close #38 with this doc. If the helper is wanted, open a new issue.

## What SketchCraft does today

- **Reads only. There is no .skp writer** (`core/src/commands/files.rs` has `import_skp`, no export; the README
  lists .skp only under "Opens your files").
- Two clean-room readers, written from sample files for interoperability:
  - `core/src/skp.rs` (1636 lines): SketchUp 6-8, the MFC CArchive object stream. Vertices, edges, faces,
    loops, curves, groups and components, materials, some annotations.
  - `core/src/skp_vff.rs` (424 lines): 2013 onward, a "VFF" marker followed by a ZIP holding `model.dat`
    (a tree of `[tag u16][length u32][body]` records) and `materials/*/material.xml`. Checked against
    SketchUp 26.2.
- `skp::parse` picks the reader. Test corpus (`core/tests/skp_corpus/`, MIT-licensed files from SketchUp's
  testup-2): v6, v13 (2013-2016), v20 (2017-2020), v21 (2021-2022), v22 (2021-2022 new block format).
- Reader coverage is partial by design (guide lines and section planes are read but not placed; see `Kind` in
  `skp.rs`). **A writer would have to round-trip everything the reader skips**, or silently drop it.

So there is no writer, no shared record schema, and the reader is lossy. Any writer starts from zero.

## Format landscape

- **The format is not documented.** SketchUp publishes no .skp specification. Access is via the C SDK or Ruby API.
- **2021 change.** Since 2021 SketchUp uses an open-ended format: new releases add features without altering
  existing records, so 2021+ versions open newer files, loading what they know and ignoring the rest. Versions
  before 2021 cannot read it at all.
  Source: [forum: save v2025 as older](https://forums.sketchup.com/t/how-is-it-possible-to-save-a-v2025-skp-file-as-an-older-skp-file/346003)
  (community account, not an official statement).
- **2024 removal.** The "save as older version" option was removed from SketchUp 2024; the help page says saving
  with older, unsupported versions is no longer available.
  Sources: [forum: unable to save 2024 file](https://forums.sketchup.com/t/unable-to-save-2024-file-v-save-as-older-version/304232/2.md),
  [help: Saving a Model](https://help.sketchup.com/en/sketchup/saving-model).
- **Old versions are only reachable from a matching release.** Saving to V2023 is "only possible in SU 2023".
  The Ruby API still saves to 2021 and earlier.

## The official SDK

- **Down-saving.** `SUModelSaveToFileWithVersion` takes an `SUModelVersion`. In the 2021 and 2022+ SDKs
  `SUModelVersion_SU2021` equals `SUModelVersion_Current`, so you do **not** get a 2021-specific file. Trimble
  says a file saved by the 2022 SDK opens in 2021, and a real down-save is only needed for versions older than
  2021. The function is meant to be "versionless", and a staff reply said the docs need updating.
  Source: [forum: C SDK 10.0 save SU2021 format](https://forums.sketchup.com/t/c-sdk-api-version-10-0-sketchup-2022-0-save-su-2021-format-skp/188709).
  Reports on newer SDKs say the header can still say 2024 ([sketchucation thread](https://community.sketchucation.com/post/1610175)).
  **Unverified by us: whether the current SDK still writes pre-2021 (2017-2020) files.** Test before relying on it.
- **Reference docs:** [SketchUp C API](https://extensions.sketchup.com/developers/sketchup_c_api/sketchup/index.html).
- **Licence.** The SDK is governed by Trimble's API terms of service (`api-terms-of-service.pdf` inside the SDK
  zip), not the SketchUp app licence. I could not retrieve the text. What forum threads show:
  - the docs say an app built on the C API must ship `SketchUpAPI.dll` / `SketchUpCommonPreferences.dll`
    (Windows) or `SketchUpAPI.framework` (macOS), so redistribution of the runtime is expected in principle;
  - downloads sit "behind a login-wall", and a Trimble team member advised against putting the libraries in public
    repos; no staff member gave a definitive redistribution answer;
  - staff cannot give legal advice.
  Sources: [public GitHub repo thread](https://forums.sketchup.com/t/is-it-okay-to-put-the-c-api-libraries-in-a-public-github-repo/103223),
  [exporter DLLs redistributable](https://forums.sketchup.com/t/exporter-dlls-redistributable/41874),
  [is the SDK free to use](https://forums.sketchup.com/t/is-the-sketchup-sdk-free-to-use-for-me/220079).
  **Read the PDF in the SDK and get written confirmation from Trimble before bundling anything.** SketchCraft is an
  open-source repo, so a login-gated binary cannot live in it either.
- The SDK is native C with per-OS libraries. I found no Android build, so a phone story would not work.

## Existing tools and workarounds people use today

- **Dezmo "Save to older Ver"** SketchUp extension (Ruby API, runs inside SketchUp 2024+). Users report mixed
  results; one says the output is still a 2025 file. [Plugin page](https://sketchucation.com/plugin/2821-dezmo_save_to_older_ver).
- **Su4U "S4U Save As"**, a standalone Windows converter. Users cite it for 2017/2020; one post notes an old build
  that expired. Closed source, so no code to learn from.
  [sketchucation thread](https://community.sketchucation.com/topic/158049/program-for-windows-s4u-save-as-sketchup).
- **Save in a matching old release**, or use a trial of the older version.
- **Interchange formats** (DAE, OBJ, DXF/DWG, IFC, 3DS): lose groups, components, tags and materials in varying degree.
- **Eneroth's "open newer files in older SketchUp" extension** (mentioned in [this thread](https://forums.sketchup.com/t/convert-sketchup-2017-file-to-2016/108554.md)).
- I found **no open-source writer** and no published reverse-engineered spec for either era.

## Legal notes

This is not legal advice.

- Our readers are clean-room and for interoperability, and the repo says so. Writing files stands on the same
  footing in principle: interoperability is the usual basis for reverse engineering. Nothing here is copied from SketchUp.
- The EULA of the SketchUp application may forbid reverse engineering regardless of local law. Keep working only
  from files we may use (the testup-2 corpus is MIT).
- Do not use "SketchUp" as a product claim ("SketchUp compatible") beyond factual statements; the corpus README
  already disclaims affiliation.
- Using the SDK is a contract (API terms). That is the larger legal question, bigger than the clean-room one.

## Options, effort and risk

Effort is a rough estimate for one engineer, to a standard we would ship (round-trip tests, no silent loss).

| Option | Effort | Main risk | Verdict |
| --- | --- | --- | --- |
| A. Native writer, 2021+ format | 2-3 weeks (records known from the reader; materials, textures, components, tags, scenes to add) | Audience is small: 2021+ already open each other's files. Newer SketchUp may reject or repair a hand-built `model.dat` | No |
| B. Native writer, 2017-2020 binary (the real ask) | 6+ weeks, likely more: we have only read this layout; need every record the reader skips, plus header details | Cannot be verified without the old apps on three OSes; a bad file is worse than none; format changes per release | No |
| C. Native writer, 2013-2016 or SU6-8 | Similar to B, per era | Almost no demand | No |
| D. Bundle C SDK in-app | 2-3 weeks plus legal review | Unclear redistribution terms, closed binary, login-gated download, no Android build, may not write pre-2021 anyway | No |
| E. Optional helper (SDK on the user's machine) | 1-2 weeks after a licence read and a test that the SDK down-saves | Geometry only survives the interchange step; SketchCraft-only features lost | Later, if requested |
| F. Do nothing, document workarounds | Under a day | Rejects the feature | Yes, now |

## Scope if the helper (E) is ever opened

1. Read `api-terms-of-service.pdf` from the SDK; get written confirmation from Trimble on any redistribution.
2. With the SDK, write a one-file test: save a model with `SUModelSaveToFileWithVersion` for each `SUModelVersion`
   down to SU2017 and check the header and that a real older SketchUp opens it. This decides E in an afternoon.
3. If it works: a separate, optional download, like the DWG helper, that SketchCraft finds and calls. Nothing
   bundled. Geometry, groups/components, materials and tags only; say what is lost.
4. If it does not work: stay at F.

## Open questions

- Does the current SDK still write pre-2021 formats? (Step 2 above.)
- The exact redistribution terms of the API terms of service (could not retrieve the text).
- How many users actually want 2017-2020 files rather than "any older version"? The issue's source is forum searches.
