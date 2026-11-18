# SketchCraft 0.3.8 "Drafting": design

Status: proposal (2026-11-17). Build steps 2 and 3 (display list, PDF/SVG/DXF, hidden lines) landed in #62; see "Built so far" at the end. Scope: issues #3–#10, #18, #19 in the **0.3.8 — Drafting** milestone (renamed **0.3.8 — Drafting** after 0.3.7 became the safety-and-polish release).
Order follows the pain-point research (§2, §5): big-document speed, then attached dimensions,
then weights, hatches and cut fills, then opacity, trim/extend, the editor, AI drafting, and tables last.

Findings that shaped this design:

- **There is no Layout workspace.** `app/src/workspace.ts` has `model | arch | design | previz`, and
  "design" is for products and furniture. Sheets are only reachable through `SheetsDialog`
  (`app/src/dialogs.tsx`), from the File menu and the Arch strip.
- **Viewports already shift when the model changes.** `layout.rs` re-centres every drawing on
  the model's bounds (or the cut's bounds) on each export.
- **Model dimensions are not attached to geometry.** `Annotation::Dimension` stores fixed world points.
- **An older app silently drops new data.** Files have no format version and unknown fields are
  ignored on open, so 0.3.6 would delete sheets on save (including an MCP agent writing the
  shared live file).
- **Hatch licensing.** Don't ship Autodesk's `acad.pat`. Write our own built-ins.

## 0. What exists today

| Area | Where | What it does now |
|---|---|---|
| `sheets` | `core/src/layout.rs` `write_sheets`; `Command::Sheets` in `commands/command.rs` | Runs once and saves nothing: pages are described in the command (`PageSpec`, `ViewportSpec`). A4–A2, Letter, Tabloid only (`sheet_mm`). `"1:N"` scales only (`scale_of`). Max 50 pages |
| Viewports | `write_sheets` | 3D views, elevations and sections are **raster** images (`render::render`, ≤6 px/mm). Only floor plans (`plan_cut`) are vector |
| Viewport placement | `write_sheets`, `plan_cut` | Re-fitted on every export (`m.world_bounds()` centre). Adding a tree moves every drawing |
| `blueprint` | `layout.rs` `blueprint` | Automatic pages: `floor_levels`, `callouts::auto_dims`, `callouts::schedule`, `section_through` |
| Line work | `plan_cut`, `dimension_lift` | Weights and colours hard-coded in the PDF (`0.4 w`, `0.25 w`, `0.2 w`). No hidden-line removal |
| `section` | `core/src/section.rs` `Model::section` → `Cut{segments, regions}` | Regions grouped by top-level owner (step 8: by innermost tagged owner). SVG has one fill colour. DXF R12 lines only |
| Tags | `Model.tags`, `Model.tag_colors`; `commands/tags.rs` | Only on groups and components. Edges have no tag or style |
| Dimensions and text | `Annotation::{Dimension{a,b,offset}, Text, Swing}` in `model.rs` | Fixed world points. Not attached |
| `export_pdf` | `export2::write_pdf` | One raster image on one page |
| Views | `model::View{name, eye, target, fov, style, tags, hidden, section}` | Identified by name only |
| On-screen edges | `Viewport.tsx`; `viewport/edges.ts` `styledEdges` | One `LineSegments`, always 1 px. Section fill is a single-colour mesh |
| File format | `commands/files.rs` `Open`/`Save` | Plain serde JSON of `Model`, **no version**. Unknown fields silently ignored |
| Undo | `commands.rs` `exec`; `history.rs` `Patch`, `TYPED`, `skeleton` | Per-client patches compare id-keyed maps entry by entry |
| Policy | `core/src/policy.rs` | `sheets`/`blueprint` in `FILES` (agents allowed, in their workspace) and `WRITES` (`pdf`) |

## 1. Data model

### 1.1 Where things live

Sheets live **in the model file**, not a separate layout file. That keeps them live (no "update
reference" step), lets dimensions point at real entity ids, and lets agents and the live file
work on them with no new sync code.

New `Model` fields (all `#[serde(default, skip_serializing_if = …)]`):

```rust
pub drafting: Drafting,                          // styles: small, rarely changes
pub sheets: BTreeMap<Id, Sheet>,                 // page records: name, number, size, order, title block
pub sheet_items: BTreeMap<Id, Arc<SheetItem>>,   // everything on paper, flat, keyed by id
pub title_blocks: BTreeMap<Id, Arc<TitleBlock>>,
```

**Why flat items.** `Patch::between` compares id-keyed maps entry by entry. With a flat map, an
agent's undo on one dimension never clashes with the person dragging another on the same sheet.
`Arc` keeps the per-step `Snap` clone cheap, as `Definition.model` already does.

`Sheet`: `{name, number, paper: Paper{w_mm, h_mm, name}, order, title_block, fields, issued: Vec<Issue>}`.

`SheetItem` (`#[serde(tag="kind")]`, each with `sheet: Id`, `z: i32`):

- **`Viewport`**:
  - placement: `rect_mm`, `crop`, `anchor: [f64;3]` (the model point pinned to the rect centre), `rotation`, `scale`
  - `kind`: `Plan{cut_z, floor_z, depth}`, `Elevation{dir, depth}`, `Section{plane, depth}` or `View3d{eye, target, fov, parallel}`
  - display: `tags: BTreeMap<String, TagOverride>`, `opacity`, `render: Vector | Raster | Hybrid`, `scene`
- **`Dimension`**: `{anchors: Vec<Anchor>, mode: Aligned|Horizontal|Vertical|Baseline, offset_mm, style, text_override}`. Two anchors make one dimension; more make a chain.
- **`Text`**: `{at_mm, box_mm, paragraphs, style, leader}`
- **Paper line work**: `Line`, `Polyline`, `Arc`, `Rect` (`{pts_mm, pen}`), and `Hatch { ring_mm, holes, hatch }`
- **`Table`**: `{at_mm, cols_mm, rows, cell_styles, source}`
- **`TitleBlockRef`**

`Drafting`:

- `tag_styles: BTreeMap<String, TagStyle>`. A `TagStyle` is `{weight_mm, line, color, cut: CutStyle{fill: None|Solid|Hatch, outline_mm}, surface_hatch, beyond: {weight_mm, color}}`.
- `patterns`: parsed `.pat`, kept in the file so it's self-contained.
- `line_types`, `dim_styles` (arrows, text, units, precision) and `text_styles`.
- `HatchUse = {pattern, color, weight_mm, scale, angle, scale_mode: Paper|Model}`.

**Per-edge override.** `Edge.draft: Option<Box<EdgeDraft{weight_mm, line, color}>>`. It's boxed to
keep `Edge` small. Edges have no tag, so an edge with no override uses the style of its nearest
tagged group or instance.

**Precedence for a line:** edge override → viewport `TagOverride` → `TagStyle` of the innermost
tagged owner → document default.

Places the compiler or tests will force us to update:

- `history.rs` `skeleton`
- `TYPED` (add `sheets` and `sheet_items`, compared with `Arc::ptr_eq ||` equality, like `definitions`)
- `Model::broken_refs`
- `max_own_id`/`repair_ids`
- `check_sane`
- `text_bytes`/`size_estimate`

### 1.2 File-format compatibility

- **Old files open unchanged.** Every new field has a default. Model-space
  `Annotation::Dimension` gains optional `a_ref`/`b_ref` anchors, which serde ignores inside variants.
- **Never add a new `Annotation` variant.** 0.3.6 can't read an unknown `kind`. All paper
  content goes in the new top-level maps.
- **Older app, newer file.** Today 0.3.6 opens it and drops the sheets on the next save. Fix:
  1. 0.3.8 writes `format: 2` and `needs: ["sheets"]`.
  2. Ship the guard first, as **0.3.7**. It reads as a `Value` first. If there are top-level keys it
     doesn't know, or `format` > 1, it opens **read-only**, refuses Save and live persistence,
     and says "Saved by a newer SketchCraft: update to edit."
- **From now on**, drafting structs carry `#[serde(flatten)] extra: BTreeMap<String, Value>`, and
  `sheet_items` deserialize through `#[serde(untagged)] enum Item { Known(SheetItem), Raw(Value) }`.
  Unknown item kinds survive a round trip as "unsupported item" placeholders.

### 1.3 Undo, redo and per-client undo

- Every sheet edit is a normal model-changing command, so it takes one `Snap`.
- A drag, or an agent's whole "dimension this plan", is grouped into one batch, which is one undo step.
- The flat `sheet_items` map makes per-client patches per item. Undo of an agent's chain dimension
  is refused only if the person has since edited that same item.
- Anchor fixes caused by a model edit (§2) happen in the same step as the edit.

### 1.4 Policy (`core/src/policy.rs`)

| Command | Who | Notes |
|---|---|---|
| `sheet_add/edit/delete`, `viewport_add/edit`, `sheet_dim`, `sheet_text`, `sheet_line`, `sheet_trim`, `sheet_extend`, `sheet_offset`, `sheet_table`, `tag_style`, `edge_style`, `hatch_pattern {name, text}`, `dim_style`, `dimension_auto`, `sheet_reattach` | `MODEL` (plugins and agents) | Pure model edits |
| `sheets_info`, `viewport_points` | read-only | For agents |
| `import_pat {path}` | `FILES` | Inside the agent's workspace |
| `export_sheets {path, format, pages?}` | `FILES`; `WRITES` += `pdf`, `dxf`, `svg` | Multi-page SVG is for the person only |
| `font_add {path}` | **person only** | Font parsers are attack surface; licences are the person's call |
| `sheet_issue` | **person only** | An agent can prepare a permit set; only a person issues it |

New limits in `vet`:

| Limit | Person | Agent |
|---|---|---|
| Pages | 200 | 120 |
| Items per command | — | 5,000 |
| Table cells | — | 10,000 |

`.pat` complexity is also capped (§3.3).

## 2. Associative dimensions

```rust
struct Anchor {
    path: Vec<Id>,   // instance ids from the root down (empty = root model)
    at: AnchorAt,    // Vertex(Id) | Between{a: Id, b: Id, t: f64} | CurveCenter(Id) | Guide(Id)
    last: [f64; 3],  // last good world point, for showing a broken anchor
}
```

- **Anchor to vertices, not edges.** Midpoints and points along an edge are `Between{a,b,t}` over
  the edge's two vertex ids. Splitting an edge keeps its vertices, so the anchor survives.
  `new_id` never reuses ids, so a deleted id can't quietly come back as different geometry.
- **Resolving.** Compose the instance transforms along `path` (as `collect_faces` does) and look
  up the vertex. It's cheap, so it runs every draw. The value is the **model** distance projected
  onto the viewport's drawing plane, formatted with the dimension style. It is never paper length
  times scale, so rounding can't creep in.
- **Placement.** `offset_mm` is measured from the measured line on paper, so the dimension follows
  the geometry.
- **When ids change.** Operations that move geometry to new ids (`make_component`, `group`,
  `explode`, `ungroup`, `copy_group`, `make_unique`) record remaps in a transient
  `#[serde(skip)] Model::remaps`. `Engine::exec` rewrites anchors in the same undo step.
- **Deleted, or not remapped.** The anchor goes **Broken**. It draws in magenta with a "!" badge,
  and the value shows as `?` with the last value struck through. **It is never moved silently.**
  `sheets_info` lists broken anchors. `sheet_reattach` suggests vertices within 1 µm of `last`
  and attaches only when the person or agent picks one. "Reattach all exact matches" is one
  undoable click.
- **Viewports keep their crop and scale.** A viewport stores its own `anchor`, `scale`, `rotation`
  and `crop`, and nothing is re-fitted after creation. This removes the `world_bounds()`
  re-centring. A linked `scene` brings in tag visibility and hidden entities. The camera changes
  only through an explicit "Reset to scene".

## 3. Rendering pipeline for sheets

### 3.1 One display list, three outputs

New module `core/src/draft/`:

- `ir.rs`: paper-space primitives in mm:
  - `Path{subpaths, pen: Pen{w_mm, rgb, dash_mm, cap, join}, fill}`
  - `Text{runs, style}`
  - `Image{rgba, rect}` (hybrid only)
  - `Group{clip, opacity, children}`
  - Every model-derived segment keeps its **provenance** (`path`, vertex ids), so the editor can
    snap to it and create anchors.
- `project.rs`: viewport → IR.
- `hatch.rs`, `pat.rs`, `text.rs`.
- Backends `pdf.rs`, `svg.rs`, `dxf.rs`.
- `plan_cut`, `dimension_lift` and `callouts::*` are ported to emit IR. `blueprint` becomes
  "generate sheets, then export".

### 3.2 Vector linework

- **Plans and sections**: `Model::section` for the cut, plus projected edges below it.
- **Hidden lines**: elevations, sections and parallel 3D views get ray-cast hidden-line removal.
  1. Build a BVH (`pt.rs` `Bvh::build`) over the viewport's triangles.
  2. Sample each projected edge (8 samples, then bisect visibility changes down to 0.05 mm on paper).
  3. Keep the visible spans, plus optional dashed "beyond" spans.
  4. Thread it per viewport.
- **Raster** is used only for Raster and Hybrid viewports.
- **Line weights are pen widths in paper mm** (ISO 0.13–1.0), not scaled with the viewport.
  PDF `w` = mm × 72/25.4. Dashes are in paper mm × the viewport's linetype scale. DXF uses group
  code 370 plus LTYPE tables (AC1015).
- **In the model view**: `scene.rs` adds an `edgeStyle` bucket per edge. `Viewport.tsx` builds one
  `LineSegments2`/`LineMaterial` per (weight, dash) bucket behind a `styles.lineWeights` toggle.
  The 1 px path stays as the fast default.

### 3.3 Hatches

- **Parser** (`pat.rs`): reads `*NAME,desc` headers and `angle, x0,y0, dx,dy [,dash…]` lines, in
  ACAD (inch) and ISO (mm) units.
- **Limits for untrusted files**: ≤64 lines per pattern, ≤16 dashes, finite numbers, spacing
  ≥0.05 mm on paper, ≤200k segments per region. Over the limit, the region becomes a solid tint
  and a warning is shown.
- **Generation**: per line family, step across the region's bounding box, intersect with the
  region's edges, take even-odd intervals, then apply the dashes. Results are cached per
  (region hash, `HatchUse`).
- **PDF and SVG**: clip to the region and draw the unclipped family. **DXF**: a native `HATCH` with
  pattern codes, so it stays editable in CAD.
- **On screen**: a fragment shader that computes distance to each line family.
- **Built-ins**: our own CC0 set (diagonal, cross, concrete, brick, earth, insulation, sand, wood).

### 3.4 Cut fill (poché)

`Model::section` will group regions by the **innermost tagged owner**, instead of the top-level
owner. Each region's tag → `TagStyle.cut` → solid, hatch or none, plus its outline. The same
function feeds `plan_cut`, section viewports, the `section` command and the on-screen section cap.

### 3.5 Opacity

- **PDF**: each viewport is a Form XObject with a transparency group, drawn through an ExtGState
  (`/CA`, `/ca`). Lines overlapping inside one viewport don't darken each other, and stacked
  viewports blend in z order.
- **SVG**: `<g opacity>`.
- **DXF**: opacity is dropped, and the export says so.

### 3.6 PDF writer

An object writer that writes and frees each page as it goes, with ExtGState and Form XObjects.
Text uses embedded, subsetted TrueType fonts for Unicode (see decision 5).

## 4. Performance for big documents (A1, 50+ pages)

- **Nothing is drawn at open.** Pages are drawn when viewed or exported.
- **Cache per viewport** (in `Live`, not saved): `hash(viewport params, styles of the tags it
  shows, model epoch for its volume)` → `Arc<DisplayList>`.
- **Invalidation**:
  - After a model-changing command, diff the typed maps (`history::diff_map`) and map changed
    entities to their owners' old and new world boxes.
  - Redraw a viewport only if one of those boxes crosses its view volume.
  - A tag-style change redraws only viewports that show that tag. Paper-item edits redraw only
    the annotation layer of their page.
- **Background drawing**: `exec` holds the engine mutex, so drawing happens on a worker. It takes
  an `Arc<Model>` snapshot, draws stale viewports on `std::thread::scope`, and the UI polls a
  `drafting_status` revision.
- **Editor transfer**: the IR goes to the app as a binary `Float32Array`, not JSON.
- **Thumbnails** are low-resolution images from the cache. Only the open page and its neighbours
  are kept at full detail.
- **Export** writes page by page, so memory stays at about one page.

**Benchmark** `core/tests/drafting_perf.rs` (release build, CI job), using the fixture
`examples/drafting/permit_set.json`:

- a two-storey house of about 200k faces, with furniture
- a 52-page A1 set: 12 plans, 8 elevations, 6 sections, 10 details, 4 hybrid 3D viewports at 40% opacity
- 2,000 dimensions (half chains), 400 text blocks, 6 schedules, and hatches on 8 tags

| Measure | Budget (CI ubuntu 4 vCPU) |
|---|---|
| Open the file (time added over the model alone) | ≤ 300 ms |
| First view of a page (cold) | ≤ 1.5 s |
| Switch to a page already drawn | ≤ 50 ms (p95) |
| Commit a dimension or text edit | ≤ 50 ms |
| Move one wall: viewports redrawn | **exactly the affected ones** (counter assertion), ≤ 1 s each |
| Export all 52 pages, cold / warm | ≤ 20 s / ≤ 5 s; peak RSS ≤ 1.5 GB; PDF ≤ 60 MB |
| Pan and zoom an A1 page | ≥ 50 fps (Playwright trace) |

Timings fail CI when more than 20% over `drafting_perf.baseline.json`. Counter assertions are exact.

## 5. The sheet editor

- **Where**: a new fifth workspace, **Layout** (`WorkspaceId` += `"layout"`, `LeftTab` += `"pages"`).
  The main area switches from the 3D `Viewport` (kept mounted, hidden) to a new **`PaperView`**:
  a Canvas2D renderer of the IR, with `Path2D` batched per pen, cached tiles per zoom, and native
  `clip()` for hatches. Handles and snap markers go in an SVG overlay. The code lives in `paper/`,
  because `sheet.tsx` is already the dialog frame.
- **Panels**:
  - left: pages, with thumbnails and drag to reorder
  - right: an inspector for viewports (view, scale including imperial, opacity, render mode, tag
    overrides), dimension style, text and table
  - tag styles: a **Drafting** section in the Outliner
- **Tools** (a `PAPER_TOOL_INFO` table like `TOOL_INFO`, each with its hint):
  - Select
  - Viewport
  - Line, Rect, Arc
  - Text
  - Dimension (chain by clicking on; Shift for baseline)
  - Trim (Shift extends), Extend
  - Offset
  - Table
  - Pan, Zoom
- **Snapping**, in order:
  1. endpoint, midpoint, intersection and centre on paper line work
  2. the same on **projected model geometry**, which gives associative anchors
  3. extension of a line
  4. the paper grid
- **Typed values** are in paper units, or model units with a suffix (`3.2m`, `10'6"`).
- **Escape** cancels the current step.
- **Trim and extend** work only on paper line work. Projected model lines can be boundaries but
  are never trimmed. The work happens in the engine (`sheet_trim {item, at_mm}`).

## 6. AI drafting through MCP

Agents use the same commands as the UI, plus read commands so they never guess:

- **Inspect**: `sheets_info`, `viewport_points {viewport, kinds, limit}`, and a page render so the
  agent can check its own work.
- **"Dimension this plan"**: `dimension_auto {viewport, scope: exterior|openings|interior|grids}`.
  It reuses `callouts::auto_dims`, emits **associative** chains, adds only, and never touches
  dimensions the agent didn't make.
- **"Hatch by tag"**: `tag_style` and `hatch_pattern`.
- **"Lay out a permit set"**: `layout_preset {preset: "permit", paper, scale?}` creates editable
  sheets (cover and index, plans per floor, 4 elevations, a section through the stairs,
  schedules).
- **Safety**:
  - One batch per agent request, so it's one undo step labelled "an agent".
  - Items record `made_by`.
  - `preview: true` plans the work without applying it.
  - The caps in §1.4 apply, and `sheet_issue` is for the person only.

## 7. Build order

Size: **S** ≤ 2 days, **M** ≤ 1 week, **L** ≤ 2 weeks. Sprint 0 testing (#49–#54) comes first.

| # | Step | Size | Tests |
|---|---|---|---|
| 0 | **Format guard, shipped as 0.3.7** (#57), plus `format`/`needs` | S | A newer file opens read-only, Save refused, live file untouched |
| 1 | **Saved sheets data model**, CRUD commands, policy, `skeleton`/`TYPED`/`broken_refs`/`repair_ids` | M | Round trip; 0.3.6 file opens; per-item patches across reload; policy tests |
| 2 | **Display list and new PDF writer**; A1, A0, ARCH C/D/E; imperial scales; port `plan_cut`/callouts | L | SVG goldens; PDF structure; moving a far tree leaves the page unchanged |
| 3 | **Vector hidden lines** (BVH) | L | Box behind box exact to 0.05 mm; goldens |
| 4 | **Cache, invalidation, background drawing, benchmark, CI budget** | L | "1 wall moved: 3 of 70 redrawn"; perf budgets |
| 5 | **Anchors**: resolve, broken state, remaps, chains, reattach | L | Move a wall: value follows; split: attached; delete: broken; undo restores |
| 6 | **Tag and edge line weights and styles** (PDF, SVG, DXF; model view) | M | Precedence tests; DXF round trip; screenshot |
| 7 | **`.pat` parser, built-ins, hatch generator** | M | Parser fuzz; limits; goldens; DXF HATCH round trip |
| 8 | **Cut fill per tag** | M | 3 tags → 3 fills; screenshot |
| 9 | **Viewport opacity and hybrid** | S | Two stacked viewports, one at 40%, in PDF ExtGState |
| 10 | **Paper line work, trim, extend, offset** (engine) | M | Geometry tests |
| 11 | **Layout workspace and PaperView** | L | Lay out a sheet entirely by mouse and save; visual references |
| 12 | **Editor tools** | L | A Playwright test per tool, with Escape and undo |
| 13 | **AI drafting** | M | An agent makes a dimensioned, hatched sheet; one Undo removes it |
| 14 | **Tables and text**, embedded fonts, linked schedules, CSV paste | L | A door schedule updates; non-ASCII text in PDF; perf within budget |

## 8. Decisions for the owner

1. **A Layout workspace?** Recommend yes, as a fifth workspace.
2. **Sheets in the model file?** Recommend the same `.sketchcraft` file.
3. **Live or frozen viewports?** Recommend live, plus an "Issued" revision per sheet and a "changed since issue" badge.
4. **Ship the format guard first?** Recommend yes, within the week (now 0.3.7).
5. **PDF and font libraries?** Recommend adopting `pdf-writer` + `subsetter` + `ttf-parser` (MIT/Apache).
6. **Hatch library?** Recommend our own CC0 built-ins, plus loading the user's `.pat` files.
7. **Line weight meaning?** Recommend paper-mm pens that don't change with scale.
8. **Re-attaching broken dimensions?** Recommend no automatic re-attach outside engine remaps.
9. **DXF version?** Recommend staying on AC1015 and dropping opacity.
10. **Page limits?** Recommend 200 (person) and 120 (agent).
11. **Agents and issuing?** Recommend person-only `sheet_issue` and `font_add`.
12. **Hybrid raster resolution?** Recommend 200 dpi on screen and 300 dpi on export.

### Decided (2026-11-17)

- **Format guard first**, shipped on its own as **0.3.7** (a three-part version is required, so "0.3.6.1" isn't possible). Drafting becomes 0.3.8, Texturing 0.3.9, Makers 0.3.10.
- **Layout** is a new fifth workspace.
- **Sheets live in the model file, and viewports are live**, with an Issued snapshot per sheet and a "changed since issue" badge.
- **Adopt `pdf-writer` + `subsetter` + `ttf-parser`** (MIT/Apache) for embedded fonts and Unicode text. (Amended in #62: `ttf-parser` is unmaintained, RUSTSEC-2026-0192, so the font reader is `skrifa` instead, which `subsetter` already depends on.)
- The other decisions take the recommendation above unless the owner says otherwise.

## Built so far

**Steps 2 and 3 (#62).** `core/src/draft/`:

| File | What it is |
|---|---|
| `ir.rs` | the display list: `Path{segs, pen, fill, prov}`, `Text`, `Image`, `Group{clip}`; `Pens` (the default pen table, ISO widths in paper mm: hair 0.13, thin 0.18, medium 0.25, heavy 0.35, border 0.5); `Cv`, the recorder the ported drawers draw into (PDF points, y up, as they always were; it stores mm) |
| `font.rs` | Inter (OFL) Latin and Latin-Extended, regular and semibold, embedded as WOFF, measured with `skrifa` |
| `pdf.rs` | `pdf-writer` + `subsetter`: pages (no transparency groups yet), one subset CID font per face used, ToUnicode maps, images only for raster viewports |
| `svg.rs`, `dxf.rs` | one page each; DXF is AC1015 with lineweights (370), true colors (420), linetypes from dash patterns, solid HATCH for fills |
| `hidden.rs` | `Proj` (parallel projection onto the paper) and `View::lines()`: BVH from `pt.rs`, a sample every 0.5 mm on the paper (8 at least), bisected to 0.05 mm, threads per chunk of edges, optional dashed "beyond" spans |
| `project.rs` | one viewport to IR: plans (ported `plan_cut`), vector elevations/sections/3D views, raster for perspective or unscaled views; the ported `dimension_lift` |
| `sheet.rs` | `PageSpec`/`ViewportSpec` to `Doc`: a thread per viewport, frame, captions, scale bars, tables, notes, title block |
| `paper.rs` | sizes (A4..A0, Letter, Tabloid, ARCH A..E, ANSI C..E) and scales (`1:100`, `1/4"=1'-0"`) |

What the next steps plug into:

- **#61 (saved sheets).** A saved `Viewport` item maps onto `ViewportSpec` (it already has `anchor`, `scale`, `beyond`, `raster`); call `draft::sheet::build` for the whole set, or `viewport` per item. `Prim::Path.prov` carries the instance path, the vertex ids and the span of the edge a line came from, which is what anchors need.
- **#3 (tag styles)**: built, see "Step 6" below.
- **#11 (the editor).** `Doc` is plain data; send it to the app as JSON now, as a binary `Float32Array` later. Every model-derived `Path` has `prov`, so snapping to projected geometry works from the list alone.

Known limits: elevations and 3D views draw no face fills or model dimensions (the pictures did); curved surfaces draw only their hard edges (no silhouettes); a plan's automatic dimensions and tags are worked out from the whole building; DXF ignores clips and images; a viewport made without an anchor is pinned to the middle of the building (environment excluded) each time the sheet is built.

**Step 5, anchors (#18).** `core/src/anchors.rs`, `core/src/draft/dims.rs`, and the `sheet_dim` / `sheet_reattach` arms in `commands/sheets.rs`.

| What | Where |
|---|---|
| Resolve an anchor to a world point (composes instance transforms down `path`; `None` = broken) | `Model::resolve_anchor` |
| Measure a dimension (model distance projected onto the viewport's drawing plane; chains, baseline sets) | `Model::measure`, `Model::measure_dimension`; `Dimension.viewport` (new, optional) picks the viewport, else the topmost on the sheet that frames the points |
| Viewport to paper mapping (the same `Proj` the drawing uses; none for perspective views) | `anchors::viewport_proj` |
| Remaps: `make_component` and `explode` record id moves, `split_edge` records splits, `group` records `Remap::Group` (the copies of shared corners; rule below), in the transient `Model::remaps`; `Engine::exec` calls `settle_anchors` after every mutating command, in the same undo step | `anchors::Remap`, `Model::settle_anchors`, `Engine::settle_anchors` |
| Drawing: `PageSpec.dims` (filled by `page_of` on export) is drawn by `draft::dims::draw`; a broken anchor is magenta with a "!" badge and a "?" value | `draft/dims.rs` |

**For #59 (SKP dimension import), the anchor API:**

- `Model::anchor_near(world_point, tol) -> Option<Anchor>`: the nearest vertex within `tol` metres (searching inside component placements too), else a `Between` point along the nearest edge, else `None` (leave the dimension unattached or report it).
- `Model::anchors_at(point, tol) -> Vec<Anchor>`: every vertex there, nearest first.
- Then add the item: a `Dimension { sheet, anchors, mode, offset_mm, viewport, ..}` in `Model::sheet_items`, or run the `sheet_dim` command (`{"anchors": [{"point": [x,y,z]}, ...]}`, or exact ids: `{"vertex": id, "path": [instance ids]}`, `{"between": [a, b], "t": .5}`, `{"curve": id}`, `{"guide": id}`). `Anchor.last` must be the world point (the constructors above set it).
- Never store the value; it is measured. Never put a world point in an anchor.

**Group rule (decided 2026-11-18).** `group` copies the corners its faces share with outside geometry. A dimension follows to the copies (`Model::group_follow`) when every anchor is in the group afterwards (a corner now in the group, a copy, a `Between` over two such, or a point inside a placement or group moved in) and at least one is on a copied corner; otherwise the whole dimension stays on the original corners. All or none per dimension, undone and redone with the group (`core/tests/anchors_group.rs`).

Rules in force: nothing re-attaches by itself except the engine remaps above (decision 8); `sheets_info` lists each dimension's `measured.spans` and `measured.broken` (with `last` and `suggestions`, the vertices within 1 µm of where it was); `sheet_reattach {item, anchor, to}` picks one, `{all_exact: true}` attaches every broken anchor that has exactly one vertex within 1 µm and refuses if none, one undo step.

Known limits: a component placement is resolved with its plain transform (Dynamic Component stretch and copies are not applied); viewport `rotation` is not applied to the paper mapping (the drawing ignores it as well); `CurveCenter` fits a circle through three of the curve's vertices; `make_component` of faces that share corners with outside geometry leaves a dimension on the original corner (only unshared points follow to the placement), and `group` follows only for a dimension whose anchors are all vertices or `Between` points (a guide or curve-centre anchor keeps the dimension on the originals), and only at the root (a `group` inside a component being edited records no remap); `csg`/`solid` operations and any other operation that rebuilds geometry with new ids record no remap, so their dimensions go broken and wait for `sheet_reattach`; a component being edited gets remaps for `explode` only; the editor (step 11) has no handles or badge UI yet, only the PDF/SVG/DXF drawing.

**Step 6, tag and edge line weights and styles (#3).**

| Where | What |
|---|---|
| `drafting.rs` | `EdgeDraft{weight_mm, line, color}` (all optional); `Drafting::dash_of(name)` (built-ins `solid`, `dashed` 3/1.5, `hidden` 2/1, `center` 8/1.5/1.5/1.5 paper mm, or a file `line_types` entry) and `tag_style`; `WEIGHT_RANGE` 0.05 to 2 mm |
| `model.rs` | `Edge.draft: Option<Box<EdgeDraft>>`; `Model::has_edge_styles` makes the file format 2 (`format.rs` `has_sheets`), so 0.3.7 opens it read-only |
| `components.rs` | `WEdge` carries `tag` (its group's, else the nearest tagged placement it sits in) and `draft` |
| `commands/tags.rs` | `tag_style {tag, weight_mm?, line?, color?, clear?}` and `edge_style {ids, weight_mm?, line?, color?, clear?}` (0 or "" clears one field); `rename_tag` moves the style. Policy: `MODEL` (agents, plugins). One undo step each |
| `draft/ir.rs` | `Pens::for_tag(drafting, tag, edge, base, beyond)`: edge override over tag style over the base pen, field by field |
| `draft/hidden.rs`, `draft/project.rs` | `View::lines` and the plan's below-cut edges take their pen from `for_tag`; hidden ("beyond") spans use the tag's `beyond` weight and colour |
| `scene.rs`, `viewport/linepens.ts`, `Viewport.tsx` | scene gets `edgeStyle` (per segment) + `edgeStyles` (the pen table) + `tagStyles`; the view draws one `LineSegments2` per pen behind `styles.lineWeights` (6 px per paper mm, at least 1 px) |
| `linestyle.tsx`, `outliner.tsx`, `entityinfo.tsx` | the weight and style menus for a tag and for selected edges |

Tests: `core/tests/linestyles.rs` (precedence, commands, save/open/undo, policy, SVG/PDF widths and dashes at 1:20, 1:50 and 1:200, DXF lineweight and linetype), `app/tests/linepens.test.ts`, `tools/ui-tests/line-styles.mjs`.

What the next steps plug into: step 8 (cut fill) should read `TagStyle.cut` and use `for_tag` for cut outlines; the viewport `TagOverride` (design 1.1) goes between the tag and the edge in `for_tag` when viewports use it.

Known limits: plan cut outlines (heavy walls) and saved dimensions (`dims.rs`) keep their fixed pens (they have no tag; cut outlines wait for step 8); the model view draws only the first dash and gap of a style, in world units; no per-viewport override yet; `color` is exposed in commands and drawings but not in the menus.

**Step: SketchUp dimension import (#59, part).** `core/src/skp.rs` (`Kind::DimLinear`, `Obj::Dim`, `Reader::dim_linear`, `SkpDim`, `Entities.dims`), `skp::import` makes `Annotation::Dimension` (and an `Annotation::Text` when the file's dimension has typed text) for the model's own entities; `ImportSummary` and the `import_skp` result carry `dimensions`, `labels` and `skipped_annotations`.

Layout of a classic CDimensionLinear (SketchUp 7; checked on a stair with 36, 18, 96 and 72 inch dimensions, offsets at right angles to the measured line): entity header, 10 bytes, typed text (string), font (a CSkFont object or a reference to it, body: header, string, 15 bytes), a zero byte, u32 3, two ends of 44 bytes `[u32 2][u32 1..4][3 doubles][u32 vertex object index, 0 = free point][8 bytes]`, six doubles (the offset's unit direction, then the plane's), a u32, the offset distance (a double, can be negative), zero padding, u32 3. Coordinates are inches.

Tests: `core/tests/skp_annotations.rs` builds a classic file in test code (the stair file is the owner's, so it isn't committed) and covers placement, free ends, labels, nonsense values, missing vertices, every truncation and 600 garbled copies.

Decisions and limits (for the owner): (1) imported dimensions are model-space `Annotation::Dimension`s with fixed points, not paper `Dimension` items with anchors: the #69 anchors live on sheets, and attaching would mean creating a sheet and a viewport on every import; a model annotation has no anchor fields. Making model annotations associative (an optional anchor pair on `Annotation::Dimension`, refreshed by `settle_anchors`) is the follow-up (built: see the next step). (2) Only CDimensionLinear is decoded. CDimensionRadial, CText (labels and leaders) and the 2021+ equivalents need sample files with those records (none of the corpus, nor the owner's samples, has one; the stair file has linear dimensions only), so they stay skipped rather than guessed. (3) 2017-2020 files add an id after each entity's attribute slot (handled by `head()`), but no 2017+ file with a dimension was available to check. (4) Dimensions inside component definitions are not imported (they would need an annotation owner on definitions).

**Step: SketchUp dimension import, finished as far as the samples allow (#59).** Owner decisions of 2026-11-18: use SketchUp/testup-2's MIT sample files, and make model-space dimensions associative.

| What | Where |
|---|---|
| `Annotation::Dimension.anchors: [Option<Anchor>; 2]` (#18's `drafting::Anchor`; `anchors[0]` holds `a`, `anchors[1]` holds `b`). Serde default, skipped when both are None, so old files read as two free ends and a dimension without anchors is written byte for byte as before. `Annotation::dimension(id, a, b, offset)` makes a fixed one; `is_attached` | `model.rs` |
| `Model::settle_anchors` now settles model dimensions too (`settle_model_dims`): the shared `settle_list` (remaps, `resplit`, `last`; the `group` rule only when both ends are attached), then each attached end's point moves to where its anchor resolves. A broken anchor keeps its last point. It runs in `Engine::exec` after every mutating command, so it is in the command's undo step. `note_remap` records remaps when `has_attached_dims()` as well as when there are sheet items | `anchors.rs` |
| Format: a model with an attached dimension is format 2 (`format::has_sheets` checks `has_attached_dims`), because a 0.3.7 build would read it and silently drop the anchors; 0.3.7 opens it read-only | `format.rs` |
| Import: each end of each imported dimension goes through `Model::anchor_near(p, ATTACH_TOL)` (`ATTACH_TOL` = 0.001 inch, SketchUp's tolerance): a vertex, else a `Between` along an edge, else None. Work is capped (`ATTACH_WORK`, 5e7 vertices and edges looked at per file); ends past it stay free. `ImportSummary` / the `import_skp` result add `dimensions_attached` (both ends attached), `dimensions_unattached`, `screen_texts` and `skipped_labels` (the words of up to 50 labels left out) | `skp.rs`, `commands/files.rs` |
| CText is read fully: `Obj::Text { screen, text }`, `Entities.texts`. Kind 0 is a screen text (two doubles at the start give its spot on the window as fractions; the end and the first three of the six doubles are zero). A file whose only content is annotations reads (a fallback scan, only when no geometry list was found, for a list that starts by declaring CText or CDimensionLinear) | `skp.rs` |

Fixture: `core/tests/skp_corpus/v06-screen-text.skp` (testup-2 `tests/SketchUp Ruby API/TC_Sketchup_DefinitionList/screentext.skp`, 7,665 bytes, SketchUp 6; MIT, Copyright (c) 2014-2024 Trimble Inc., credited in that folder's README and LICENSE). That is the only annotation in the whole testup-2 set: its 33 `.skp` files (SketchUp 6 to 2022) have no CDimensionLinear, no CDimensionRadial, no label with a leader, and the 2021+ (VFF) files have no record that isn't already known geometry, curves or list metadata (0x138e, 0x139b, 0x1396 curve lists, an empty 0x139e).

Tests: `core/tests/skp_annotations.rs` (attach on import to vertices and along an edge, an end on nothing stays free and is counted, move a vertex and the dimension follows in one undo step, undo/redo, move the whole group, a split edge keeps a point along it on the right piece, delete leaves it at its last point, save/open keeps anchors and format 2, an old format-1 file opens for editing and saves its dimension unchanged, hand-edited anchors to nothing, every truncation), `core/tests/skp_corpus.rs` (the screen text: read, counted, named, not placed; every truncation through its record; the corruption run covers it too).

What is left (needs sample files the owner has or can make; no guessing): (1) **labels with a leader** are read but not placed: the record has an end (vertex or point), six doubles and the words, and SketchUp's model of a label is a point plus a vector to the text, which fits "the end is the leader's tip, the first three doubles are the vector" on the one synthetic 2016 record we have, but no real file of ours shows it; a SketchUp 6-2020 file with two labels (one on a vertex, one on a face) would settle it, then they become `Annotation::Text { at: tip + vector, leader: Some(tip) }`. (2) **CDimensionRadial** and (3) **2021+ dimensions and labels** have no sample at all. (4) Dimensions inside component definitions are still not imported. (5) The model view draws an attached dimension wherever its points are, so it follows on screen with no app change; there is no broken-anchor badge for model dimensions (a broken end just stays put) and no `sheet_reattach` equivalent for them. (6) The `dimension` command still makes fixed dimensions; attaching hand-drawn ones (an `attach` flag using `anchor_near`) is a small follow-up if wanted.

**Step 9, viewport opacity (#6).** The saved `Viewport.opacity` (0 to 1, stored only below 1; file format 2 already, as all saved sheets are) and `viewport_edit`/`viewport_add` `opacity` existed from #61, with policy (`MODEL`, so agents and plugins), the range check in `validate` and one undo step. This step makes drawings use it.

| Where | What |
|---|---|
| `layout.rs` | `ViewportSpec.opacity: Option<f64>` (None = 1; `viewport_to_spec` sets it from the saved item); `write_sheets_notes` / `export_warnings` return what a format left out |
| `draft/ir.rs` | `Group.opacity` (1 = opaque); `sheet.rs` sets it on a viewport's figure group, so the frame and caption stay full strength |
| `draft/pdf.rs` | `Writer::faded`: a group below 1 becomes a Form XObject (`/Group << /S /Transparency /I true /K false /CS /DeviceRGB >>`, its own resources, page-sized `BBox`, the clip drawn inside) placed with `/GsN gs /FmN Do`; the ExtGState has `/CA` and `/ca`. It sits in the page stream where the viewport is, inside the figure's marked content, so stacked viewports blend in z order. Nested faded groups work (each form has its own resources) |
| `draft/svg.rs` | `<g ... opacity="0.4">` on the group |
| `draft/dxf.rs` | opacity dropped (decision 9); `dropped_opacity(page)`; `export_sheets` returns `warnings: ["DXF has no opacity: N viewport(s) drawn at full strength..."]` (empty list for PDF and SVG) |

Tests: `core/tests/viewport_opacity.rs` (PDF structure and z order, resources with a picture, SVG, DXF warning, command + range + save/open + undo + policy).

Known limits: **hybrid is not built.** `RenderMode::Hybrid` is saved and set by the commands but exports ignore `render` (a viewport is vector unless it is raster for the old reasons); the design's "faded raster 3D view under vector linework" needs a viewport to draw two groups (picture at the opacity, then linework) and the 200 dpi screen / 300 dpi export raster size from decision 12 (the raster path today is 6 px/mm, about 150 dpi, capped at 4000 px). That is a step of its own, not a small add-on. There is no sheet UI yet (step 11), so no control or Playwright test; the editor needs an opacity slider on a selected viewport that sends `viewport_edit {opacity}`. Opacity applies to the viewport drawing only, not its saved dimensions (`dims.rs`), which are drawn at full strength.

**Step 7, hatch patterns (#4).**

| Where | What |
|---|---|
| `draft/pat.rs` | the `.pat` parser (`*NAME, desc` headers, `angle, x, y, dx, dy [, dash...]` families, `;` comments, `;%UNITS=MM/INCH`, CRLF and BOM; converted to mm). Limits: 256 KB of text, 256 patterns, 64 families, 16 dashes, names of 63 characters, numbers finite and within 1e5; a bad line is skipped with a warning (at most 20 listed), a pattern over a limit or with nothing drawable is dropped; never panics. `BUILTIN`: our own CC0 set (diagonal, cross, steel, grid, dots, brick, concrete, earth, insulation, wood, sand), in paper mm at scale 1, written from scratch (no Autodesk set). `lookup` finds a name in `Drafting::patterns` first, then the built-ins, without case |
| `draft/hatch.rs` | `place` (scale and turn a pattern into `Placed` families: angle, base, offset as x/y, dashes) and `generate` (each family's lines across the region's extent, cut against every ring edge, even-odd intervals so holes stay open, dashes laid from each line's own origin). Guards: lines closer than 0.05 mm, more than 200,000 segments, or more than 5e7 line-edge tests give no lines and a reason, caught by an estimate before the work and again while emitting. `on_sheet` makes the `HatchPrim` for a region (y up from the page's bottom left, the DXF's frame, so the DXF regenerates the same lines); `scale_k` is paper mm per pattern mm (model-scaled hatches divide by the drawing's 1:N) |
| `draft/ir.rs` | `Prim::Hatch(HatchPrim{rings, name, families, angle, scale, pen, lines, tint, note})`; `cull` keeps it by its rings' bounds |
| `draft/svg.rs`, `draft/pdf.rs` | clip to the rings (`clip-rule="evenodd"` / `W* n`) and stroke the lines with round caps (a dot is a zero-length line); a tint is filled instead. SVG marks it `data-hatch="name"` |
| `draft/dxf.rs` | a native pattern `HATCH` (70 = 0, 75 = 0 odd parity, 76 = 2 custom, 52/41 the angle and scale, 78 families with 53/43/44/45/46/79/49), lineweight 370 and true colour 420; a tint is a solid `HATCH` |
| `draft/project.rs` `cut_region` | plans and section viewports draw each cut region through it: the tag's cut hatch (paper-coloured fill, the hatch, the heavy outline) or the solid poché as before |
| `drafting.rs` | `Drafting::cut_hatch(tag)`: the hatch when `cut.fill` is `Hatch` |
| `components.rs` | `Model::owner_tag(owner)`: the tag of a section region's owner (a group's, or its nearest tagged parent's; a placement's) |
| `commands/tags.rs` | `tag_style {hatch: {pattern, color?, weight_mm?, scale? (0.01 to 1000), angle?, scale_mode?: paper/model}}` (only what is given changes; `{pattern: ""}` removes it; checked before anything changes); `hatch_pattern {name, text, units?, delete?}` keeps one pattern as canonical mm `.pat` text in `Drafting::patterns` (built-in names refused; delete refused while a tag uses it); `hatch_patterns {text?, units?}` lists every pattern with its families, or previews a text (read-only, no undo step). Policy: all three `MODEL` (agents and plugins). Undo: `drafting` is in the history skeleton, one step each. `rename_tag` already moves the style |
| `layout.rs` | `hatch_warnings`: the export result's `warnings` list each hatch drawn as a tint and why (all formats) |
| `commands/sections.rs` | the `section` command returns `hatch: [{tag, color, lines (world m), notes}]` and `hatchFill` (paper-white triangles of hatched regions, apart from `fill`); paper-scaled hatches show as on a 1:50 drawing (`SCREEN_HATCH_SCALE`), at most 60,000 segments per answer, the rest solid |
| `hatchstyle.tsx`, `hatchswatch.ts`, `Viewport.tsx` | the Hatch button per tag in Outliner > Tags (a swatch of the hatch), its editor (hint, preview, pattern, colour, weight, scale, angle, paper/model size, Import .pat… with the file's units), and the section fill drawing the hatch lines over paper white |

Tests: `core/tests/hatches.rs` (parser, junk, limits, a 1,500-case fuzz of garbage and mutated `.pat` text through parse and generate, built-ins round trip, clipping with holes, dash phase, angle and scale, pathological input, commands, file, undo, policy, plan and section on sheets, SVG/PDF clip and pen, DXF `HATCH` read back and regenerated to the same lines, tint and warnings, saved-sheet export, the `section` command's screen hatch), `app/tests/hatchswatch.test.ts`, `tools/ui-tests/hatches.mjs`.

What step 8 (#5, cut fill per tag) plugs into: `project::cut_region` is the one place cut regions are filled; add `Solid`/`None` and `outline_mm` there, and regroup `Model::section` regions by the innermost tagged owner (today a region's owner is the face's own group, or the top-level placement, and `owner_tag` walks up from it). The on-screen section cap (`commands/sections.rs`) uses the same `owner_tag` and `cut_hatch`, so it follows.

Known limits: `surface_hatch` (hatching surfaces in elevations) is stored by the data model but not drawn or set by any command; the viewport `TagOverride` has no hatch fields; the on-screen hatch is generated by the engine as world-space lines on each section redraw (not the design's fragment shader), drawn 1 px wide with no weight, and a component being edited shows no tag hatches; no result cache per (region, hatch) yet; the hatch origin is the page's bottom left on sheets and the cut plane's origin on screen (not the model origin), so a pattern doesn't line up between two viewports of the same wall; `import_pat {path}` (design §1.4) isn't built: the app reads the file and sends its text.


**Step 8, cut fill per tag (#5).**

| Where | What |
|---|---|
| `components.rs` | `WFace.cut` + `WFace.tag`: the innermost tagged owner of each face, worked out in `collect_faces` (the face's group or a group around it in its own model, else the nearest tagged placement or a tagged group around that placement, outward). Named by its own id when it is in the root model, by the top-level placement's id when it is inside a component. With no tag anywhere around a face: the face's own group (root) or the top-level placement, exactly as before. `Model::owner_tag` is gone (the region carries its tag) |
| `section.rs` | `Cut.regions: Vec<Region{owner, tag, outer, holes}>` (was a tuple), grouped by `(WFace.cut, WFace.tag)`. One function still feeds `plan_cut`, section viewports, the `section` command and the on-screen cap |
| `drafting.rs` | `CutFill::{Auto (default: the drawing's poché, "auto" in the file), None, Solid, Hatch}`; `CutStyle.color` (a solid's colour, "" = the poché `CUT_SOLID`); `CutStyle.outline_mm`; `Drafting::cut_fill(tag)` (a `Hatch` with no hatch set is `Auto`) |
| `draft/ir.rs` | `Pens::for_cut(drafting, tag)`: the heavy pen at the tag's `cut.outline_mm`, in the tag's line colour (through `for_tag`), never dashed |
| `draft/project.rs` `cut_region` | per region: Auto = the poché as before; Solid = the colour; Hatch = paper + hatch (step 7); None = the paper colour (a blank cut that still hides line work beyond it in a section); always the `for_cut` outline. PDF/SVG/DXF need no changes: DXF draws a solid fill as a solid `HATCH` in the colour (420), outlines with lineweight 370 |
| `commands/tags.rs` | `tag_style {cut: {fill?: auto/solid/hatch/none, color?, outline_mm? (0.05 to 2; 0 clears)}}`, only what is given changes, checked before anything changes; `fill: hatch` needs a hatch (set before or in the same command); removing a hatch-filled tag's hatch goes back to `auto`; switching to solid keeps the hatch for later. Policy `MODEL`, one undo step, format 2 |
| `commands/sections.rs` | the `section` answer adds `solid: [{tag, color, fill}]` (a tag's own colour; a solid with no colour stays in `fill`), `open: [tag]` (fill none: no cap), `outlines: [{tag, weight_mm, color, lines}]` (rings of tags with their own outline weight or colour). Tag fills now also apply inside a component being edited (regions carry tag names, styles are the root's) |
| `cutfill.ts`, `cutstyle.tsx`, `outliner.tsx`, `Viewport.tsx` | the Cut button per tag (a swatch of the fill) and its editor (hint, Default/Solid/Hatch/None as swatches with hints, colour for solid, outline weight); the section cap draws `solid` meshes and `outlines` as `LineSegments2` at 6 px per paper mm; the cap fills get a polygon offset so hatch lines and outlines (in the same plane) show from either side of the plane (before, a hatch was hidden under its paper fill when seen from the side the plane faces away from) |

Tests: `core/tests/cutfill.rs` (innermost owner at the root, inside components and components inside components, untagged fallback, loose faces; default look unchanged; 3 tags → 3 fills and outline weights on plans and sections; SVG/PDF/DXF; saved sheets; the command's checks; undo/redo; save/open; policy; the `section` answer; editing a component), `app/tests/cutfill.test.ts`, `tools/ui-tests/cut-fills.mjs` (3 tags → 3 fills on screen with a pixel check).

Behaviour change to know: in a tagged root group, untagged child groups' cuts now merge into the tagged group's region (before, each group was its own region); untagged models cut exactly as before. A file written by a develop build before this step with `"fill": "none"` (which meant "not set" then) now reads as a blank cut; no release wrote that.

What later steps plug into: the viewport `TagOverride` (design 1.1) should override `cut_fill`/`for_cut` between the tag and the drawing; the editor (step 11) can reuse `CutStyleControls`; #8's shader replaces the engine-generated screen hatch lines in `hatch`, and could also draw `outlines`.

Known limits: the `section` command's own SVG/DXF file (`Cut::svg`/`dxf`, its `path` argument) still has one fill colour and no styles (sheets are the styled output); on screen, `None` leaves the cut open (you see into the solid) while sheets draw it blank; the screen outlines are drawn whatever the Line weights toggle says; outline colour follows the tag's line colour (no separate cut outline colour).

**Step 10, paper line work, trim, extend, offset and chain dimensions (#19).** The engine and commands; the mouse tools are step 11/12 (#9).

| Where | What |
|---|---|
| `draft/linework.rs` | pure 2D geometry in paper mm, no panics: `ArcC` (an arc from the item's three points: centre, radius, start angle, signed sweep), `Curve::{Seg, Arc}`, `intersect` (line/line, line/arc, arc/arc; parallel, collinear and concentric give none; a touch at an end counts; tolerance `EPS` 1e-6 mm), `Chain` (curves end to end, open or closed: a `Rect`, or a polyline that ends where it starts), `Chain::crossings`, `trim`, `extend`, `offset` (`Side::{Left, Right, Inside, Outside, Toward(p)}`), `item_of` (a run of curves back to a saved item), and `draw` (an item to path segments; arcs as quarter-turn Beziers) |
| `commands/sheets.rs` | `sheet_line {sheet, kind?, pts_mm, w_mm?, color?, dash_mm?, z?, lines?}` (batch is one step and checked whole first; with `id` it edits that item, its kind stays), `sheet_trim {item, at_mm, pick_tol_mm?, use_model?}`, `sheet_extend {item, at_mm, ...}`, `sheet_offset {item, distance_mm, at_mm \| side}`. `Engine::boundaries` gathers what a trim or extend cuts to; `Engine::line_from` validates a line (counts, finite coordinates within 1e5 mm, pen 0.05 to 2 mm, `#rrggbb`, at most 16 dashes) |
| `layout.rs`, `draft/sheet.rs` | `PageSpec.lines` (filled by `page_of` for saved sheets) is drawn over the viewports and under the dimensions; a pen left at `#000000` draws in the sheet's ink so blueprints stay readable |
| `policy.rs` | the four commands are `MODEL` (agents and plugins); `lines: [..]` counts toward the 5,000 items an agent may send in one command |

Rules: only paper line work is ever changed. Boundaries are the other paper line work on the sheet, the lines each viewport on the sheet draws of the model (the viewport is built, its stroked paths flattened and cut to its frame), and the viewport frames themselves; `use_model: false` leaves the model's lines out (and skips building the viewports, which is the slow part). A crossing at an open item's own end is not a crossing (a line that only touches an edge has nothing to cut at). Trim: the span containing the nearest point on the item to `at_mm` (refused if farther than `pick_tol_mm`, default 2) goes, between the nearest crossings either side; with no crossing on one side it runs to the end; with none at all the command refuses. Two pieces: the first keeps the id, the second is a new item with the same pen and `z`. A rectangle, or a closed polyline, opens into a polyline (it needs two crossings). A trim that would remove the whole item is refused rather than deleting it. Extend: the end nearest `at_mm` runs along the line's direction (an arc along its circle, less than a full turn) to the nearest crossing ahead, within twice the sheet's diagonal; closed shapes are refused. Offset: a new item of the same kind and pen; `left`/`right` are as the sheet looks, travelling from the first point; corners are mitred and bevelled past 10 times the distance; a distance that reverses any side, or takes an arc's radius to zero, is refused.

Chain dimensions need no new code: `sheet_dim` already takes 2 to 200 anchors (Step 5). They are now in the MCP help and tested end to end on a saved sheet (`core/tests/sheet_linework.rs`: five anchors in one command, each span measured and drawn in PDF, SVG and DXF, one undo step, save and open, following a moved wall; baseline mode; the 200 limit).

Tests: `core/tests/linework.rs` (the geometry: every pair of curve kinds, trim middle/end/rect/arc/polyline, extend to a line/arc/arc end, offset of each kind, refusals, 3,000 random shapes without a panic), `core/tests/sheet_linework.rs` (commands, undo and redo, save and open, SVG/PDF/DXF, model lines and frames as boundaries, policy, chains).

What step 11/12's editor plugs into: each tool sends one command with the picked point in paper mm (`sheet_trim {item, at_mm}`, Shift for `sheet_extend`, `sheet_offset {item, distance_mm, at_mm}`); the reply says what was kept, created and cut (`cut: {from, to}`, for a preview line), so a hover preview can call the same geometry. The editor should draw the saved items from `sheets_info`, whose items already carry `pts_mm` and `pen`.

Known limits: a trim or extend builds the sheet's viewports to find the model's lines (as slow as exporting them: use `use_model: false` for paper-only work); a viewport's `crop` outline is not applied to its boundaries (its rectangle is); hidden and dashed "beyond" lines count as boundaries; raster viewports (perspective views) have no lines; a polyline is not checked against itself (its own crossings are not boundaries); collinear overlaps are not crossings; an offset does not clean up the self-overlap a large distance makes on a concave polyline (it refuses when a side reverses, which catches most); there is no fillet, no `Hatch` or text trimming (text, hatches, dimensions and tables are not boundaries); `sheets_info` does not summarise line work beyond listing the items.
