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

- `tag_styles: BTreeMap<String, TagStyle>`. A `TagStyle` is `{weight_mm, line, color, cut: CutStyle{fill: None|Solid|Hatch, outline_mm, priority: 0..=999 (higher wins a cut overlap)}, surface_hatch, beyond: {weight_mm, color}}`.
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

Known limits: `surface_hatch` (hatching surfaces in elevations) is stored by the data model but not drawn or set by any command; the viewport `TagOverride` has no hatch fields; the on-screen hatch was engine-generated world-space lines, 1 px wide, at most 60,000 segments (since step 4 it is the fragment shader at the hatch's true weight with no segment limit; engine lines only for a pattern of more than 16 families or dashes; the result cache per (region, hatch) is step 4's `HatchCache`); the hatch origin is the page's bottom left on sheets and the cut plane's origin on screen (not the model origin), so a pattern doesn't line up between two viewports of the same wall; `import_pat {path}` (design §1.4) isn't built: the app reads the file and sends its text.


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

**Step 13, AI drafting (#10).** `core/src/commands/autodraft.rs`, `core/src/draft/raster.rs`.

| Where | What |
|---|---|
| `callouts.rs` | `auto_chains`: the strings `auto_dims` draws as whole chains (`AutoChain{side, kind: Openings/Overall, stops, offset}`) and whether the floor has hand-placed dimensions; `auto_dims` is now a wrapper (unchanged output) |
| `layout.rs` | `set_specs(m, paper, notes, SetOpts{per_floor, scale})`: the blueprint's layout, also as a permit set (one plan per page, sheet numbers, a section only through stairs, one scale for all); `blueprint_specs` calls it with the defaults. `section_place` says whether the section goes through stairs. `ViewportSpec.no_auto_dims` (from the saved viewport's `legacy.auto_dims: false`) leaves a plan's worked-out exterior strings out |
| `drafting.rs` | `Dimension.source` ("auto" for `dimension_auto`) and `Dimension.part` ("openings/south", "interior/west"...) |
| `commands/autodraft.rs` | `viewport_points {viewport, kinds?: vertex/midpoint/opening, limit?}` (read-only; paper mm, world point, the anchor for `sheet_dim`; one point per 0.05 mm, nearest the viewer); `dimension_auto {viewport, scope?: exterior/openings/interior/grids or a list, refresh?}`; `layout_preset {preset: permit, paper?, scale?, project?}`; `sheet_render {sheet, path, px?}`; `Engine::sheet_picture` |
| `draft/raster.rs` | a page of the display list to an RGB picture: scanline fill (4 sub-scanlines, exact horizontal coverage), strokes as quads plus joins, dashes, Inter outlines through skrifa, pictures, clips, opacity per primitive. No new dependency |
| `commands.rs` | a command whose reply says `"unchanged": true` takes no undo step (a `dimension_auto` with everything already there) |
| `mcp.rs` | the AI drafting commands and a DRAFTING RECIPE in the help; `sketchcraft_render {sheet, px?}` returns the sheet as an image |
| `policy.rs` | `viewport_points`, `dimension_auto`, `layout_preset`: `MODEL`; `sheet_render`: `FILES`, `WRITES` `.png` |

How `dimension_auto` attaches: each stop the plan works out is stood for by a visible vertex on the floor (between the floor and the cut, inside the building's box) within 15 mm along the measured direction, nearest the face it is measured on; a horizontal or vertical chain measures only along its axis, so any such vertex is right, and it moves with its wall. Stops with no vertex are dropped and counted (`missed`). Strings sit in tiers out from the face (the first clears the door and window tags; 7 mm apart): openings, then exterior (south, east) or interior (north, west), then grids. The offset is worked out once against the anchors' extreme, as `draft::dims` measures it, so it follows the geometry afterwards. Exterior or openings on a blueprint-style plan (`legacy.callouts`) turn its worked-out strings off in the same step, so each is drawn once. Interior: every edge of the plan's cut longer than 0.45 m that runs along y (x chain) or x (y chain), so door jambs and frames are left out. Grids: the middle of the longest diagonal of each column's foot (groups or placements named or classed column, at any depth for placements, through the group's parents for root groups) as a `Between` anchor, and guide points as `Guide` anchors.

Tests: `core/tests/ai_drafting.rs`: every scope on a fixture house (counts, values, modes, attached), moving the interior wall moves the interior string's values, a re-run adds nothing and takes no undo step, the person's and other sources' dimensions untouched, refresh replaces only its own, one undo per `dimension_auto`, the plan draws each string once, refusals, the permit set (sheets, numbers, index, editable, one undo removes it, a scale for all, a section only with stairs, empty model refused), `viewport_points` (anchors that attach, paper positions at scale, kinds, limit), `sheet_render` (size, ink, hatching changes it), policy, and an end-to-end agent over the real `sketchcraft mcp` (scratch HOME, `SKETCHCRAFT_DOC`, workspace confined): builds, tags, lays out, inspects, dimensions, hatches, renders (inline and a file; a path outside the workspace refused), exports PDF and SVG (dimension text and `data-hatch` there), then undoes step by step back through each state of the shared file.

What later steps plug into: the editor (step 11/12) can show `viewport_points` as snaps and use `sheet_picture` for thumbnails; tables (step 14) can make the A-601 schedule a linked table (`Table.source`).

Known limits: `exterior` is the overall width and depth only (an L-shaped building's jogs are not chained); interior strings are drawn outside the building (north and west) rather than through the rooms; the outline (and so the overall) includes door and window frames that stand proud of the wall; grids are column centres, not named grid lines with bubbles; columns inside a group inside another group are found only through root group parents; `dimension_auto` works on plan viewports only (no elevation heights yet); there is no `preview` (design §6) yet; `layout_preset` knows only "permit"; the picture fades each primitive rather than a viewport as one group, has no hinting, and caps at 4000 px (1600 through the MCP tool).

**Step 8b, cut intersection priority per tag (#88).** Builds on step 8; suggested by a Reddit user (Archicad's building-material priority).

| Where | What |
|---|---|
| `drafting.rs` | `CutStyle.priority: i32` (0 to 999, `PRIORITY_RANGE`; written only when not 0, so a file with no priorities is byte-for-byte what it was; format 2 already). `validate` rejects a loaded file with one out of range |
| `section.rs` | `Model::section_styled(origin, normal, &Drafting)` (`section` calls it with the model's own styles; the `section` command passes the root's, because a component being edited has none) and `resolve_priorities`, run after the regions are chained, only when some tag has a priority other than 0. Claim order: higher priority first, equal priorities by tag name (byte order), untagged last. Per tag in that order: union of its regions (same tag merges, no internal outline), minus everything already claimed (`geo` `BooleanOps`; `geo` was already a dependency). Pieces under 0.01 mm2 and holes under it are dropped. A piece keeps its tag and takes the owner of the first (lowest id) source region that covers a point inside it. The tag's thin cut lines (`Cut.segments`) are clipped by the same claims, so no seam is drawn through a winner. A panic inside the booleans falls back to the unresolved regions |
| `commands/tags.rs`, `command.rs` | `tag_style {cut: {priority}}` (a whole number 0 to 999; only the given fields change; checked before anything changes; MODEL policy; one undo step); help in `mcp.rs` |
| `cutpriority.ts`, `cutpriorityeditor.tsx`, `outliner.tsx` | Outliner > Tags > "Intersection priority": the tags from the winner down, drag to reorder, up/down buttons, Alt+Up/Down on a row's grip, a number field per tag, `=` on tied tags, a hint and a tiny example. A reorder renumbers (bottom 0, 10 per step up) and sends only the tags whose number changes, inside `begin_batch`/`end_batch` so it is one undo step |

Everything that draws a cut goes through `Model::section`, so plans, section viewports, saved sheets (PDF/SVG/DXF), the `section` command's answer and the on-screen cap all show the resolved regions with no further change. `viewport` draw caches (#8 / #90) key on the whole `TagStyle`, so the new field needs no cache change.

Behaviour to know: with every priority 0, nothing is resolved (overlapping regions of different tags are drawn on top of each other, as before this step). As soon as one tag has a number, resolving applies to every tag, so same-tag regions also merge then. At equal priority the overlap goes to the tag that sorts first by name (one fill, never two on top of each other).

Tests: `core/tests/cutpriority.rs` (higher wins and lower trimmed both ways, no seam in the cut lines, same tag merges into one L, equal priority, holes in winner and loser, touching edges and slivers, all-zero unchanged exactly, command checks, undo/redo, save/open and no 0 written, policy and help, sheets in SVG/PDF/DXF, the `section` answer and the cut file, a component being edited), `app/tests/cutpriority.test.ts` (order, move, renumber, parse), `tools/ui-tests/cut-priority.mjs` (reorder by arrows, drag, keyboard and typing flips the overlap's pixel colour and trims the cap; one undo step; saved file).

Known limits: the `section` command's own SVG/DXF file has no styles but does use the trimmed regions; priority only decides cut overlaps (it does not change which faces are drawn beyond the cut); equal-priority tags with different fills cannot both show in an overlap. Batch rule (fix/batch-undo): `begin_batch` ... `end_batch` from one source is one undo step. A batch belongs to its opener (the app, or an agent; a plugin run is the app's batch filled by the plugin): the app's own commands join the app's own batch, an agent's join an agent's, and another source's command is refused (reads still run). The person's edit closes a plugin run or an agent batch (saved first, its own step); a nested begin, or an end with nothing open, is an error; a stale end id is a harmless no-op that says `already_ended`; a batch left open is closed after 150 s (the app's own after 30 s idle). Agents may use begin_batch/end_batch (one AI answer, one Undo). The intersection-priority reorder's UI suite checks a reorder that changes two numbers (the old check moved one tag, so never used a batch).

**Step 11, the Layout workspace (#63; the opacity slider for #6).** The workspace, its Pages panel, the paper view and the inspector. No sheet-editing tools yet (that is #9, step 12).

| Where | What |
|---|---|
| `draft/irjson.rs` | one `Page` of the display list as light JSON: paths as SVG path strings (paper mm, y down, 0.01 mm), neighbouring stroke-only paths with the same pen joined into one (`counts.paths` vs `counts.path_prims`), text runs, pictures as base64 PNG (null past 8 MB of pictures), figure groups with `clip`, `o` (opacity) and `alt`, hatches as `rings` + `lines` + pen or `tint`. Bounded: past `limit` bytes (16 MB by default) the rest is left out and `truncated` is true |
| `commands/autodraft.rs` | `sheet_ir {sheet, limit_kb?}` (read-only, `MODEL`), and `Engine::sheet_page`, shared with `sheet_picture` |
| `commands/sheets.rs` | `sheet_duplicate {sheet, name?}`: the sheet and every item on it (through their saved form, so kinds this build doesn't know are copied too), right after the original (later sheets move down one), a dimension's `viewport` pointing at the copy's viewport, `issued` not copied; `sheet_order {sheets}`: every sheet once, orders 0, 10, 20...; both `MODEL`, one undo step |
| `layout.rs` | `today()` honours `SOURCE_DATE_EPOCH`, so a title block's date can be fixed (the browser suites set 2026-01-01 for the visual references) |
| `workspace.ts`, `uselook.ts`, `appearance.tsx` | `WorkspaceId += "layout"`, `LeftTab += "pages"`; `leftTabs(id)` (Layout: Pages, Outliner, Views); `toolAfterSwitch` (Layout keeps the model's tool) |
| `Viewport.tsx`, `viewport/types.ts` | `paused`: under Layout the 3D view stays mounted with its scene, camera and selection, draws nothing and takes no keys (six lines, away from the hatch code) |
| `paper/uselayout.ts` | the workspace's state: `sheets_info` and the current page's `sheet_ir`, read again whenever the scene changes while Layout shows (120 ms debounce, newest answer wins); the page commands |
| `paper/PaperView.tsx`, `paper/draw.ts`, `paper/view.ts` | the paper: Canvas2D, `Path2D` from the path strings (parsed once per display list), 512 px tiles cached per zoom step of √2 (drawn at the step above, so only ever shrunk; at most 40 kept, 6 drawn per frame), edges snapped to device pixels; a faded group drawn as one layer at its opacity, as the PDF does. Pan (drag, middle-drag, Space, two-finger scroll, arrows), zoom at the cursor (wheel, pinch, + and -), Fit (0). The SVG overlay draws the page edge and each viewport's frame (hover, focus, picked with 8 handles and a tag); the frames are buttons for the keyboard and screen readers. The text uses the engine's Inter (`core/assets/fonts`, as "SketchCraft Paper" so it never replaces the UI font) |
| `paper/PagesPanel.tsx`, `paper/pages.ts` | the list with schematic thumbnails (the paper and its viewport frames, from `sheets_info`), add, duplicate, rename, delete, drag, ▲▼, Alt+Up/Down, Up/Down/Home/End, F2, Delete |
| `paper/Inspector.tsx` | page: name, number, paper, project, notes, blueprint, Add view (plan, elevations, 3D, saved scenes); viewport: label, view, scale (typed, with a list including imperial), opacity slider (sends `viewport_edit {opacity}` once, when you let go), Delete view |
| `paper/LayoutStage.tsx` | the empty state (what pages are, + Add page) and the paper |
| `App.tsx`, `filemenu.tsx`, `wsstrip.tsx`, `dialogs.tsx` | File > Open in Layout, the Arch strip's Layout button, "Open in Layout" in the Drawing sheets dialog (which still makes its one-shot PDF) and in the command palette. In Layout the model's tool bar, view bar, value box and strip are put away, and number keys, tool keys and camera keys do nothing; undo, redo, files and menus work |

Tests: `core/tests/layout_workspace.rs` (the JSON's shape, joining, two decimals, text, opacity, the limit and refusals, an empty page; duplicate: items, order, the dimension's viewport, same drawing, one undo/redo, name, refusals, the sheet limit; `sheet_order`: one undo step, no-op, refusals; policy), `app/tests/paper.test.ts` (order, the reorder command, names, papers, scales, opacity, views; the camera: fit, round trip, zoom at the cursor, clamps, steps, tiles, hit testing; the workspace list, tabs, tool kept, saved edits), `tools/ui-tests/layout-workspace.mjs` (switch in under 1.5 s with the same canvas, the empty state, three pages, drag and Alt+Down reorder with one-step undo/redo, rename, Add view, the cursor and hint over a viewport, pick it, the slider changes the saved viewport and the drawing, undo, Escape, zoom and fit; back to Model: same canvas, camera, counts, selection and tool; File > Open in Layout), `tools/ui-tests/visual.mjs` shots `layout-light`, `layout-dark`, `layout-high-contrast` (a viewport picked), `layout-page-light`, `layout-empty-light`.

What #9's editor plugs into:

- **Tools.** Add a `PAPER_TOOL_INFO` table and a tool bar inside `LayoutStage` (the model's tool bar is hidden there). A tool gets the paper point under the mouse from `toPaper(cam, local(e))` in `PaperView` (paper mm, y down, the frame every sheet command takes); make `onPointerDown/Move/Up` hand the event to the active tool before the pan and pick logic.
- **Previews and snaps** go in the SVG overlay (`.paper-overlay`), in screen pixels through `toScreen(cam, p)`; the display list's model-derived paths have no `prov` in the JSON yet: add it in `irjson.rs` (the edge's instance path, vertex ids and `t` span) when snapping to projected geometry, or use `viewport_points` per viewport.
- **Moving and resizing a viewport**: the handles are drawn; a drag would send `viewport_edit {id, rect_mm}` once, on release.
- **Redraw speed**: every model change asks for the whole page again (`sheet_ir`). #8's draw cache (#90) makes that cheap on the engine side; after it, `sheet_ir` could take the revision the app has and answer "unchanged".

Known limits: a page on a custom paper size isn't drawn (`sheet_page` needs a standard size, as `sheet_render` did); thumbnails are schematic (the paper and frames, not the drawing); a viewport's `crop` outline isn't shown on the overlay (its rectangle is); a page with more than 16 MB of line work is drawn in part (the bar says so); the paper is redrawn from scratch on every model change, even one that doesn't touch the page; the Pages panel has no multi-select; there is no "changed since issue" badge (issuing isn't built). Reordering is its own command (`sheet_order`), a decision taken when a batch from the app was closed by its first change; that is fixed now (a batch from one source is one undo step, see the batch rule under step 8b), but `sheet_order` stays: one command is simpler than a batch of `sheet_edit`s.

Known limits: a page on a custom paper size isn't drawn (`sheet_page` needs a standard size, as `sheet_render` did); thumbnails are schematic (the paper and frames, not the drawing); a viewport's `crop` outline isn't shown on the overlay (its rectangle is); a page with more than 16 MB of line work is drawn in part (the bar says so); the paper is redrawn from scratch on every model change, even one that doesn't touch the page; the Pages panel has no multi-select; there is no "changed since issue" badge (issuing isn't built). Owner decision taken in passing: reordering is its own command (`sheet_order`) rather than a batch of `sheet_edit`s, because a batch from the person's own app is closed by its first change (live.rs ends a batch when the person edits, so a crash can't lose their work), which makes a batched reorder several undo steps. The Outliner's intersection-priority reorder (#88) sends a batch the same way and so is likely several steps in the app; worth a look.

**Step 4, kept drawings, invalidation, background drawing, benchmark (#8).**

| Where | What |
|---|---|
| `draft/cache.rs` | `Drawings` (the engine's store, `Engine::drawings`, never saved): one `Arc<Vec<Prim>>` per viewport, keyed by `key(spec, pens, page_h)` (the whole placed `ViewportSpec`, paper or blueprint, the page height); `Volume::of(model, spec)` is what it can show: `Prism` (its projection, its frame padded by `PAD_MM` = 10, a plan's heights from its floor through 2.8 m above it, a section's kept half-space), `All` (fitted, unpinned, or worked out from the whole building: plan callouts, a section mark) or `Raster` (pictures). `invalidate(&Dirty)` drops what a change reaches and bumps `epoch`; `put` refuses a drawing begun before the last drop. Estimated bytes, LRU past `DEFAULT_BUDGET` (1 GB). `Stats` counts drawn (and on the worker), kept, dropped, evicted, hatches made and kept. `HatchCache` + `hatch_on_sheet`: generated hatches per (rings exactly, `HatchUse`, resolved pattern, page height, scale, pens), 4M segments at most; `Pens.hatches` carries it into `project::cut_region` |
| `draft/dirty.rs` | `changes(before, after) -> Dirty`: world boxes of each changed entity's owner in both models (the innermost tagged group or placement, else its own group; the whole own group too, for stairs and overhead fixtures), placements of changed components (through components inside components, Dynamic Component copies counted), everything carrying a tag whose visibility or **whole** `TagStyle` changed (so any new style field, such as #88's cut priority, is covered), tags whose hatch pattern text changed; owners with a styled tag that overlap a changed box by more than 1 mm (regions of different tags interact only where they overlap, e.g. #88's priority subtraction); cut priorities (#88): any change to one, or any change at all while one is set, sets `cuts` (every plan and section is redrawn: resolving priorities unions each tag's regions over the whole plane); model notes as heights (plans of that floor); stairs apart (judged 0.3 m below the floor); flags for units / line types / unknown drafting fields (everything), saved views (views found by name), materials, colours, location, scans and section planes (pictures only). Every `Model` field is named in a destructuring pattern, so a new field doesn't compile until it is decided. Over 50,000 changed entities: everything |
| `commands.rs` `exec`, `commands/drawings.rs` | after every mutating command that succeeded (and every Undo/Redo, from a copy taken only when something is kept), `drawings_changed(before)`; `drop_steps_after` clears. `sheet_page(sheet)` (one saved sheet as the editor will show it, numbered in the set; marks it viewed), `sheet_page_fresh`, `kept_viewports()`, `warm_sheets(ids)` (copies the model, draws what isn't kept on the background worker: one job at a time, the newest wins, `BACKGROUND_THREADS` = 2), the read-only `drafting_status` command (`revision` = the epoch, `drawing`, `kept`, `bytes`, `stats`; policy `MODEL`). After a change that dropped something, the three sheets viewed last are warmed |
| `draft/cache.rs` `World`, `Pens::with_faces/with_edges/with_tris/section`, `Model::section_of` | the model's world faces, edges and triangles worked out once and shared by every viewport of a build (the engine keeps one with its drawings until the next change), instead of each viewport expanding every placement again; a plan whose model has no environment cuts once (it used to cut twice) |
| `draft/sheet.rs` | `build_opts(.., Opts{cache, first, total})`, `draw_viewports` (kept first, the rest `VIEWPORT_THREADS` = 4 at a time, kept as drawn); `MAX_PAGES` is the policy's 200 (was 50). `export_sheets` and the trim/extend boundaries go through the cache (`layout::write_sheets_kept`) |
| `commands/sheets.rs` `viewport_to_spec` | a saved viewport is drawn pinned to its `anchor` (#89 made the same fix): what keeps a far-away change from moving, and so redrawing, a drawing; fitted and perspective ones still fit |
| `core/tests/perf_fixture/mod.rs` | the §4 fixture in code: a two-storey house (one group per wall, slab, column, stair; 8 tags with cut hatches), desks as components, a 52-page A1 set with 70 viewports (12 plans each with a key plan, 12 elevations incl. 1:20 bays, 6 sections, 20 details, 4 parallel 3D views at 40%, cover and notes key plans), 6 schedules, 2,000 dimensions (half chains) and 400 text blocks at `Size::full()` (about 200,000 faces); `Size::small()` has the same pages with little geometry |
| `core/tests/drafting_perf.rs`, `drafting_perf.baseline.json`, `.github/workflows/perf.yml` | the budget table; `SKETCHCRAFT_PERF=full` checks budgets and 20% over the baseline (and over 5 ms); `SKETCHCRAFT_PERF_WRITE_BASELINE=1` rewrites it. The workflow runs on staging/main pushes and PRs, nightly on develop, by hand, and on develop PRs labelled `performance`; it is not a required check (shared-runner timings are too noisy to gate every PR). The exact counter assertions also run in the normal `test` job on the small fixture |

Tests: `core/tests/drafting_cache.rs` (52 A1 pages and 70 viewports, nothing drawn at open or after open; cold export draws 70, warm 0; **moving one interior wall drops exactly the 30 viewports that see it**, by name, and the next export redraws exactly those; paper edits drop nothing; through ten edits, undo and redo, every page from kept drawings equals the page drawn afresh, prim for prim; component edits; hatches reused on redraw; a tag's cut fill, hatch and outline redraw only what shows the tag, a cut priority every plan and section; background redraw of the page viewed, and a change mid-draw throwing the work away; the budget's eviction), `core/tests/drafting_perf.rs`.

First CI numbers (perf workflow, ubuntu-22.04 4 vCPU, release, 201,936 faces; the baseline file has them): open +7.7 ms (budget 300), first page cold 1.26 s (1.5), page switch p95 1.4 ms (50), paper edit 1.6 ms (50), slowest viewport redrawn after moving a wall **1.06 s (1.0: over)**, world geometry after a change 0.13 s once, export cold 15.8 s (20) / warm 2.5 s (5), PDF 14.8 MB (60), **peak memory 1.94 GB (1.5: over)**, kept drawings about 540 MB for the 70 viewports. Before sharing the world geometry: cold export 29.6 s, first page 1.53 s, peak 2.1 GB. The two misses wait for the owner: the slowest viewports are the full-house hidden-line views (axonometrics, elevations), and memory is mostly the kept display lists (about 250 bytes a line, 1.6 million lines, most of them furniture drawn in 24 plans); a compact line store or a lower kept budget would bring it under.

What later steps plug into: the editor (step 11) calls `sheet_page` for the open page (it is then warmed after each change) and polls `drafting_status.revision`; the binary `Float32Array` transfer of the IR and thumbnails are not built (no editor yet). #10's `sheet_render` should draw through `sheet_page` (or `build_opts` with the cache) so agents' page renders are kept too.

Known limits: while any cut priority is set, every change redraws all plans and sections (precise again once the priority resolution is local to overlapping regions); Hybrid and perspective viewports are pictures and redraw on any change (their `Volume` is `Raster`); elevations and sections see their whole depth (`ViewKind.depth` is not used by the drawing), so an interior wall counts for every elevation framing it even when another wall hides it; a plan with callouts or a section mark is `All`; an error is never kept (a plan that becomes "nothing is cut" is drawn again each time); the pan/zoom 50 fps budget waits for PaperView; text items aren't drawn yet, so their 400 blocks cost only open time.

Known limits: Hybrid and perspective viewports are pictures and redraw on any change (their `Volume` is `Raster`); elevations and sections see their whole depth (`ViewKind.depth` is not used by the drawing), so an interior wall counts for every elevation framing it even when another wall hides it; a plan with callouts or a section mark is `All`; an error is never kept (a plan that becomes "nothing is cut" is drawn again each time); the pan/zoom 50 fps budget waits for PaperView; text items aren't drawn yet, so their 400 blocks cost only open time.

**Step 4, the on-screen hatch shader (#8; owner decision 2026-11-18: "build the design's shader version (true weight, no 60k cap) with #8").**

| Where | What |
|---|---|
| `commands/sections.rs` | the `section` answer adds `hatches: [{tag, color, weight_mm, pattern, families: [{angle, base, offset, dash}], origin, u, v, fill}]`: each hatched tag's cap triangles (world metres) and its pattern placed on the cut plane (`hatch::place`, plane mm from `origin` along `u`, `v`; paper-scaled hatches as on a 1:50 drawing, `SCREEN_HATCH_SCALE`), with no segment count. `hatch` (lines) and `hatchFill` are now only for a pattern of more than `SHADER_FAMILIES` (16) families or `SHADER_DASHES` (16) dashes, still capped at 60,000 segments |
| `app/src/viewport/hatchshader.ts` | `hatchMaterial(cap)`: a `ShaderMaterial` whose fragment shader maps the fragment to plane mm, finds the nearest line of each family (and the nearest dash or dot along it, laid from each line's own origin as the generator does), converts the distance to screen pixels with the screen derivatives, and inks a round-capped line `max(1, weight_mm x PX_PER_MM)` px wide (6 px per paper mm, as edges and cut outlines), half a pixel of blur, over paper white (`#f6f5f0`). `hatchInk` is the same rule in TypeScript; `pack` and `shaderCan` the uniforms and limits |
| `Viewport.tsx` | the section cap draws one shader mesh per hatched tag (`userData.hatchShader`, `sectionHatch` = the tag); the old line path stays for the fallback. A component being edited gets the same answer (step 8), so it shows its tags' hatches |

Tests: `app/tests/hatchshader.test.ts` (lines and paper between, true width in pixels and the same width at any zoom, turned and shifted families, dashes from each line's origin, round dots, cross hatches, packing, limits), `tools/ui-tests/hatches.mjs` (the shader mesh in the hatch colour at 0.35 mm = 2.1 px; red pixels on paper white looking down on the cap; a 0.13 mm pen inks fewer pixels; a hatch far too dense for 60,000 lines still goes to the shader), `tools/ui-tests/cut-fills.mjs` (the hatched walls through the shader, with the pixel check), `core/tests/hatches.rs` (the answer's `hatches`).

Known limits: the shader draws at most 16 families of at most 16 dashes (the built-ins have at most 6, earth); line widths are in screen pixels from paper mm, so they don't grow when zooming in (as edges don't); very dense patterns turn to grey moiré on screen rather than to the sheets' tint.

**Step 12, the sheet editor (#9).** Tools on the Layout workspace's paper: a sheet can be laid out entirely by mouse and is saved with the model.

| Where | What |
|---|---|
| `paper/tools.ts` | `PAPER_TOOL_INFO` (name, key, tip, icon) for Select (V), Viewport (W), Line (L), Polyline (P), Rectangle (R), Arc (A), Text (T), Dimension (D), Trim (X, Shift extends), Extend (E), Offset (O), Delete (K); `TOOL_GROUPS`; `cursorFor` (a crosshair with the tool's icon, as a data-URL cursor); `stepHint` (what to do next, per step); `toolStep(tool, state, event, ctx)`: each tool's state machine, pure, answering the next state and the one engine command a finished gesture makes; `dimCommand` |
| `paper/snap.ts` | snapping in paper mm: `pageGuides` (edges, the 10 mm border margin, the middle), `rectGuides` (other frames' edges and centres), `snapMove` (left/centre/right and top/middle/bottom, each axis on its own, else the grid), `snapResize` (the edges a handle owns; never inside out, at least `MIN_SIZE`), `snapPoint` (points first, then lines, then the grid; `pointsOnly` for dimensions), `ortho` (Shift), `nudge`, `HANDLE_EDGES` (the 8 handles in PaperView's order). Reach: `SNAP_PX` (8 screen px) at the current zoom |
| `paper/items.ts` | line work, text and dimensions from `sheets_info`: `polylineOf`, `arcThrough`, hit testing (`itemAt`, `distToItem`), snap points (`itemPoints`: ends, middles, arc centres), `textBox` (as `draft/sheet.rs` draws text), `dimLines` and `dimOffsetAt` (a port of `draft/dims.rs`'s placement: the `offset_mm` that puts the line through the mouse), `cutPath` (a trim's `cut` along the outline), `movedItem`, `titleBlock` (where `title_block` draws it), `describe` (screen-reader labels) |
| `paper/editor.tsx` | `useEditor`: PaperView hands it the pointer and keys before its pan and zoom. Select: pick (items over viewports), drag a viewport (snapped; `viewport_edit {rect_mm}` once, on release), its 8 handles (`.ed-handle`, snapped resize), drag line work or text (`sheet_line {id, pts_mm}` / `sheet_text {id, at_mm}`), arrows nudge, Delete deletes, double-click text edits it in place, double-click the title block's rows edits the project or the page name in place (`sheet_edit`). The model's points come from `viewport_points` (vertex and midpoint, per viewport, read again when the model or a viewport changes, only while a drawing or dimension tool is out). Trim and extend hover previews call the command with `preview: true` (70 ms after the mouse stops, newest answer wins). The overlay draws the grid, the items (focusable, labelled, highlighted on hover, focus or pick; red under Delete), live frames, previews, snap guides and markers with what they are; HTML on top for the text box, the view chooser and the title block field |
| `paper/LayoutStage.tsx` | the tool bar (`.paper-tools`, down the left; a fitted page starts clear of it) and the options (`.paper-opts`: Snap, Grid and its spacing, kept per viewer in localStorage; the dimension's Auto/Horizontal/Vertical/Aligned) |
| `paper/Inspector.tsx` | a picked line (weight, solid/dashed), text (words, height) or dimension (what it measures), with Delete; viewports keep their inspector (scale and the rest) |
| `paper/uselayout.ts` | `item`/`setItem` (one thing picked at a time: a viewport or an item), `run`; a just-made viewport or item stays picked until `sheets_info` lists it |
| `commands/sheets.rs`, `drafting.rs`, `layout.rs`, `draft/sheet.rs` | `sheet_text {sheet, at_mm, text, height_mm?, z?}` (with `id`: change it; newlines split lines; 1 to 50 mm, default `TEXT_HEIGHT` 3.5, written only when not the default; at most 4,000 characters, no control characters), `Text.height_mm`; `PageSpec.texts` drawn over the line work (`text_item`: the first line's baseline at `at_mm`, capitals `height_mm` tall, lines 1.25 font sizes apart). `sheet_trim`/`sheet_extend` `preview: true`: read-only (`Command::mutates` is false for it), the reply has `cut` (trim, plus `pieces`) or `from`/`to` (extend). `sheets_info` gives each dimension's `paper_mm` (where its anchors are on the sheet now) |

Every gesture is one engine command, so one undo step; none needs a batch today (a chain dimension is one `sheet_dim`, a polyline one `sheet_line`). A gesture that ever needs several commands should go through `begin_batch`/`end_batch` (the batch rule above). Policy: `sheet_text` is `MODEL` (agents and plugins); previews are read-only.

Tests: `core/tests/sheet_editor.rs` (text: placed, drawn in SVG and the display list at its baseline and line pitch, saved, opened, undo/redo, edited by id, height, refusals, non-ASCII; previews change nothing and take no undo step, agree with the real trim and extend, refused like them, read-only inside a batch; `paper_mm`; policy and help), `app/tests/sheeteditor.test.ts` (snapping, handles, nudge, the pointer-to-paper transform, items' geometry and hit tests, dimension placement against `dims.rs`, every tool's state machine, Escape, hints, keys, cursors), `tools/ui-tests/sheet-editor.mjs` (a page laid out by mouse: two viewports drawn and chosen, move with snapping to the margin and another frame, resize by a handle to the grid, keyboard nudges, scale, a 3-point chain dimension attached to the model, text placed and changed by double-click, lines, trim and extend with their previews, rectangle, polyline, arc and offset, the title block typed on the page, the Delete tool, keyboard reach and labels, tool keys, Escape; one Undo and Redo per gesture; saved, opened again and drawn again), `tools/ui-tests/layout-workspace.mjs` (still passes; its handle check now counts the editor's handles), `tools/ui-tests/visual.mjs` shots `layout-editor-light` and `layout-editor-dark` (a dimension being drawn with its snap marker, text, line work), and the `layout-*` shots taken again (the tool bar, the grid, the page moved clear of the tool bar). Visual references are macOS only: the Linux ones for the Layout shots must come from the staging CI run.

What later steps plug into: tables (#7, step 14) add a Table tool to `PAPER_TOOL_INFO` and `toolStep`, and text blocks (boxes, styles, leaders) extend `sheet_text` and `TextBox`; `prov` in `irjson.rs` would let snapping use every projected line (intersections, extensions) rather than `viewport_points`; #8's draw cache (#90) makes the trim and extend previews (which build the sheet's viewports to find the model's lines) cheap.

Known limits: snapping has no intersections or extension lines (ends, middles, centres, frames, the page and the grid only); typed values only for the offset distance (no typed lengths, no model units with a suffix); a dimension can't be moved or restyled after it is placed (delete it and draw it again), and its mode shows read-only in the inspector; one thing is picked at a time (no marquee or Shift-click); no viewport rotation or crop editing; no separate Pan and Zoom tools (Space, middle-drag, the wheel and the bar do it); text has one style (height only, no box or leader); the trim and extend previews draw an arc's extension as a straight dash; a viewport's model points are fetched per viewport (up to 5,000 each), which is slow on a very large model.
