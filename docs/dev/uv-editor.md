# UV editor

Mark seams, unwrap a surface into islands, pack them into the 0..1 square and edit them in a 2D view; plus a second UV
set for baked light (the lightmap channel). Code: `core/src/unwrap.rs` (seams, islands, LSCM, packing, the Model
methods), `core/src/commands/uv.rs` (the commands), `core/src/texmap.rs` (`Lightmap`), `core/src/export.rs` and
`core/src/fbx_export.rs` (the second UV set), `app/src/uveditor.ts` (the 2D math), `app/src/uvpanel.tsx` (the panel).
Tests: `core/tests/uv_editor.rs`, the unit tests in `unwrap.rs`, `app/tests/uveditor.test.ts`,
`tools/ui-tests/uv-editor.mjs`.

## Data model

- `Edge.seam: bool` (`core/src/model.rs`): the edge is a UV seam. Written only when set. A file with any seam (in the model
  or in a component) is format 2 with `needs: ["edge_seam"]`; a build that doesn't know it opens the file read-only
  (`core/src/format.rs`). Copying a group or exploding a component carries the seams along.
- `Face.uv: Option<FaceUv>`: the picture's coordinates, per corner (projection-mapping.md). The unwrap writes this.
- `Face.uv2: Option<FaceUv>`: the **lightmap channel**, same shape, written only when set, `needs: ["face_uv2"]`. It never
  changes how the model is drawn, rendered or picked: only the exporters read it (`texmap::Lightmap`).
- A face's corner list is its outer loop followed by each hole, in the loop's order. Coordinates are in tiles, v up.
- An **island** is not stored: it is the faces joined across an edge whose corners agree in UV (`Model::uv_islands`). Moving
  an island therefore needs no bookkeeping; tearing one apart (moving one vertex) makes it two.
- **Pins are not stored.** They live in the panel and go to `uv_unwrap` as `pins`.

## Commands

All are on the plugin and MCP allowlist (`policy.rs`, `mcp.rs`). Each changes the model in one undo step; a refused call
changes nothing. `faces` may name groups (every face inside), and `entity` a group or a placed component (a component is
unwrapped in its own definition, so every placement shows it).

- `uv_seam {edges}` marks or clears (`on: false`) seams; `{faces | entity, auto: degrees}` takes the edges inside those
  faces whose faces meet at more than that angle (hard edges). Returns `changed` and the edge ids.
- `uv_unwrap {faces | entity, method?, seams?, angle?=60, margin?=0.01, pins?}`. `method`: `conformal` (default: islands
  from the seams, LSCM), `angle` (also cuts on hard edges first), `planar_islands` (regions that lean alike, each
  projected flat). `seams`: `marked` (default) or `auto` (marked plus hard edges). `pins`: `[[face, corner, u, v]...]`.
  Returns `faces, islands, split, pinned, ignored_pins, stretch, scale`.
- `uv_pack {faces | entity, margin?, set?}`: repack the existing islands as they are (never turned), as large as the sheet
  allows.
- `uv_lightmap {faces | entity, margin?=0.02, angle?, clear?}`: an unwrap into `uv2`; `clear: true` removes it.
- `uv_set {uv: {"<face>": [[u, v] per corner] | null}, set?}`: write corners (null drops them; for set 1 it also drops a
  painted-through placement, so the picture goes back to the material's own).
- `uv_transform {faces, vertices?, matrix: [a, b, c, d, e, f], set?}`: p' = [a c; b d] p + [e f], in tiles, on those faces'
  corners (only those at `vertices` when given). Moving a whole island is one of these.
- `uv_info {faces | entity, set?}`: read-only. Per face: `id, material, verts` (model vertex per corner), `loops` (the
  sizes), `uv`, `island`; plus `without` (faces with no UVs), `islands`, `seams`.

`set` is 1 (the picture, default) or 2 (the lightmap). The scene (`scene.rs`) carries `seams` (red line segments) and
`triUv` (per triangle: this face has its own UVs), which the viewport uses for the seam lines and the checker.

## Algorithm

1. **Soup.** The selected faces become `FaceD`s (corners, triangles from the face triangulator mapped back to corner
   indices, normal, area) and `Link`s: two faces that share an edge, with the edge's seam flag, the dihedral angle between
   them as one continuous surface (a face wound the other way is flipped for this) and whether they are oriented against
   each other. An edge with other than two faces is a cut.
2. **Islands.** Flood fill over links that are not cut (a marked seam; with auto seams also a hard edge).
3. **Flatten** (`Ctx::solve`). A flat island is projected onto its plane (upright where it can be: `plane_frame`). Otherwise
   **LSCM** (least-squares conformal maps, Levy et al.): per triangle, in its own orthonormal frame, the conformal
   condition is two linear rows over the 3 vertices' (u, v); two far-apart boundary vertices are pinned and the rest is a
   sparse least-squares solve (`lscm_solve`: CGLS, column-scaled, no dependency). Vertices are *wedges* (corners of one
   vertex that meet through uncut edges), so a seam really opens. Faces wound against their neighbours are flipped first.
   The result is checked: the island must be a topological disc (`is_disc`), no triangle may fold, the outline may not
   cross itself, and the area-weighted mean of max(r, 1/r) (r: picture area over model area per triangle, normalised
   per island) must stay under 3. An island that fails (a closed surface, a handle, a fold, too much stretch) is split
   into regions whose normals stay within a cone (`Soup::cluster`, cone 60 degrees by default, then 0.6 times that,
   recursively) and those are flattened in turn; at the end (one face, or a cone under 2 degrees) each face is projected
   on its own plane, which cannot fail. Every test-set model unwraps; the globe becomes ~18 islands, the can 6, the ball 6.
   There is no ABF: LSCM is also angle-preserving, and the cluster split is the fallback for what it cannot do.
4. **Density.** Every free island is scaled so its picture area equals its model area: one texel density across islands.
5. **Pack** (`pack`): maximal rectangles, best short side fit, islands sorted by their longest side; LSCM islands (and
   every island of a lightmap) may turn a quarter. A binary search finds the biggest common scale that fits the unit
   square with `margin` between islands and to the border; a crowded sheet halves the margin until it fits. Islands
   cannot overlap because their (padded) rectangles cannot.
6. **Pins.** Pinned corners (two or more on an island) become fixed vertices of that island's LSCM, in tile units, so the
   island keeps exactly those places; it is not scaled or moved by the packer, and the packer fills the free space around
   it (it is a blocked rectangle). One pin on an island, or pins that make a folded result, are ignored (`ignored_pins`).

## The panel

**Tools > UV editor** and **Window > UV editor** (also command search) open a floating window (drag the title; **Dock**
pins it to the right edge; resize with the corner). It works on the faces, groups and components selected in the 3D view
when it opens and whenever you select something else there. Clicking an island selects its faces in the 3D view without
changing what the panel shows (the panel remembers which selection it made).

- Under the wireframe: the material's picture (the most common picture among the shown faces), or a checker. The
  **Checker** toggle also puts a checker on every face that has UVs in the 3D view (`Styles.checker`, a mesh named
  `uv-checker`; never saved).
- Wireframe: one SVG path per island, tinted; the chosen islands are brighter. Points, edges or islands are chosen with
  1 / 2 / 3, a click, Shift to add, a drag on empty space for a box.
- Edits: drag chosen things; **G / R / S** move, turn and scale about the middle of the chosen (click keeps, Esc cancels);
  arrows nudge a pixel (Shift: ten); buttons for Turn 90, Flip, Pin (P), Fit (F). A whole-island edit is one
  `uv_transform`; a part of an island is one `uv_set` with the faces that changed. While dragging only the preview is
  drawn; the command runs when you let go, so one undo step per edit.
- Buttons: Mark seam / Clear seam (edges chosen in the 3D view, or chosen here), Auto seams, Unwrap (method, seams,
  angle), Pack, Lightmap UVs (then shows set 2), Reset.
- The step hint at the top says what to do next and changes with the state; every control has a tooltip. English and
  Italian strings are `uv.*`.

## How a paint layer plugs in

The 2D view is built for it. Under the SVG wireframe is one `<canvas>` drawn by a stack of `UvLayer`s
(`{id, draw(ctx, view)}`), the picture or checker first; `addUvLayer` adds a layer between the picture and the wireframe
and returns its remover. A layer draws in UV space through `View` (`toScreen` / `fromScreen` in `uveditor.ts`, the same
transform the wireframe uses), so a texture-paint canvas of any resolution lines up with the islands for free and
is redrawn on every pan or zoom (the panel runs the layers whenever the view changes; call `redrawUvLayers()` when the
layer's own pixels changed). The brush is a pointer mode next to Points / Edges / Islands: it receives
`fromScreen(view, pointer)` as the UV position and writes into its own canvas; saving that canvas back as the material's
picture is the paint feature's job (it is a `texture` command with the new data URL). The island paths are a good mask:
`hitIsland` and `inPolygon` say whether a UV point is on any face.

## Known limits

- Faces of a placed component can be unwrapped, packed and read through `entity`, but hand edits (`uv_set`,
  `uv_transform`, the panel's drags) reach the faces of the open model only: open the component to edit its UVs.
- A face's UVs are per corner of its current loops: splitting a face drops them (as for projection mapping).
- The hard-edge angle is a global number, not per edge; the cluster fallback can make many small islands on organic
  models (the beaver, flat as it is, stays one island per outline face).
- LSCM's two automatic pins fix scale and turn only; very long thin discs can be stretched at the far end, which the
  stretch check turns into a split. There is no ABF++ or SLIM refinement.
- Pins live in the panel (they are not saved with the model).
- No texel-density matching between separate unwraps, no mirrored-island overlap (every island gets its own space),
  no UDIM tiles.
- The panel's visual reference exists for macOS only; run `node tools/ui-tests/run.mjs uv-editor -- --update` on Linux to
  add the Linux one (until then the Linux run says so and compares nothing).
