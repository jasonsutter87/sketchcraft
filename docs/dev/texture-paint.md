# Texture painting

Paint straight onto a model's texture image, the way Substance or Blender's Texture Paint do. The model is unwrapped
(see [uv-editor.md](uv-editor.md)), a *paint texture* (a material with an embedded PNG) is made for it, and brush strokes
change that picture's pixels, either by dragging over the model in the 3D view or on the flat picture in the UV editor.
Code: `core/src/texpaint.rs` (the maths and the Model methods), `core/src/commands/texpaint.rs` (the commands),
`app/src/texturepaint.ts` (the same maths for the live preview, stroke batching, the commands),
`app/src/texturepaintpanel.tsx` (the panel), the `tpaint` tool in `app/src/Viewport.tsx` and the Paint mode in
`app/src/uvpanel.tsx`. Tests: the unit tests in `texpaint.rs`, `core/tests/texture_paint.rs`, `app/tests/texturepaint.test.ts`,
`tools/ui-tests/texture-paint.mjs`.

## Storage: no new structure, no format guard

A paint texture is an ordinary material: `Material.texture.data` is a PNG data URL (`width` / `height` are the tile size in
metres, 1 x 1 for a paint texture; the UVs are in tiles, so 0..1 is the whole picture). It is stored, saved, loaded, drawn,
rendered and exported exactly like any other embedded texture, so **there is no new field and no format guard**: a build
that predates this feature opens the file and shows (and keeps) the painted picture. The stroke is applied to the decoded
pixels and the picture is written back as a PNG with fixed encoder settings (fast compression, adaptive filter), so the same
pixels always give the same bytes: a save and load, or a save after a load, is byte-identical.

The material's `color` is the base colour: a paint texture made from a flat colour has that colour, one baked from the
faces' present look has the average colour. Erase paints back toward `color` (not toward a stored copy of the original
picture: there is none).

## Representation of a stroke, and undo

`texture_paint_stroke` carries **stroke data** (the brush's path, radius, colour, hardness, opacity, erase), not a pixel patch.
The engine paints it. Why: a command is a few hundred bytes (a patch of a long stroke is megabytes), an agent can paint
without any pixels, and the result of a stroke depends only on the picture before it and the stroke, never on how the app
batched the points, so a replayed stroke gives the same bytes. Within a stroke the weight of a texel is the **strongest dab**
that reaches it, never a sum, and the stroke blends once from the picture as it was before: `new = old + (target - old) * weight * opacity`
(sRGB values as stored; alpha is left alone). Undo is the engine's usual whole-model snapshot for the step, so Undo restores the
exact previous picture and one stroke is one step. The snapshot holds the PNG text, which for a 2048 picture is a few MB per
stroke; the undo history's memory budget trims old steps as it does for other pictures.

## Commands

All are on the plugin and MCP allowlist (`policy.rs`, `mcp.rs`). Paint textures belong to the whole model, so they are refused
while a component is open (close it first) and `faces` / `entity` may not name a placed component (open it and select its faces).

- `texture_paint_new {faces | entity, size?=1024, base?, name?, unwrap?}`: makes the material (named `Paint N`, or `name`) and
  wears it on the faces. `size` is 64..4096 pixels on a side (the UI offers 512 / 1024 / 2048). `base`: `[r, g, b]` (0..1,
  default white) or `"material"`, which bakes each face's present material (picture through its present UVs, or colour) into the
  new picture through the new UVs; texels no face covers get the average. The faces need their own UVs, inside 0..1 and not
  overlapping (checked on a 256 x 256 coverage grid). When they are not, the call is refused and says why, unless `unwrap: true`,
  which runs `uv_unwrap` (default options) on those faces first, in the same undo step. Returns `material, faces, size, unwrapped`.
- `texture_paint_stroke {material, ...}`: one stroke, one undo step. Either
  - **3D**: `points: [[x, y, z]...]` (world metres) and `radius` (metres, default 0.05) with `faces | entity` (default: every
    face wearing `material`). The brush is a sphere: for each texel of each triangle, its 3D position (from the barycentric
    coordinates) is compared with the stroke points, so **a brush near a UV seam paints every island it touches in 3D**,
    wherever each lands on the picture; or
  - **picture**: `uv_points: [[u, v]...]` (tiles, v up) and `uv_radius` (tiles of the picture's width, default 0.02), painted
    on the flat picture whether or not a face is there; or
  - `fill: true` with `faces | entity`: every texel the faces use.
  - `color: [r, g, b]` (required unless `erase`), `hardness` 0..1 (default 0.5: full strength inside `hardness * radius`, then a
    smoothstep to nothing at the radius), `opacity` 0..1 (default 1), `erase`. Returns `texels` (how many changed).
- `texture_paint_status {faces | entity}`: read-only. `without` (face ids with no UVs), `overlap`, `outside`, `paintable`.
- `texture_pixels {material, x?, y?, w?, h?}`: read-only. A block of `[r, g, b, a]` (0..255) rows (at most 4096 pixels), the
  picture's `width` / `height` and `hash`, an FNV-1a of every pixel, so a test can tell two pictures apart.

A refused call changes nothing. Any material with a picture can be painted (a JPEG becomes a PNG on its first stroke), not
only the ones this feature made; every face wearing it shows the change.

## How texels are found

A texel is in a triangle's rasteriser when its centre is inside the triangle in picture space or within 0.75 pixel of an edge
(the barycentric coordinates are then clamped, so the 3D point used is on the triangle). That growth stops a seam leaving an
unpainted line when the picture is filtered. For each dab only the part of the triangle that can be within the radius is
visited: the dab's foot on the triangle's plane gives a pixel centre, and the largest stretch of the plane-to-picture map
(the larger singular value of its 2 x 2 Jacobian) turns the in-plane radius into a pixel box. Triangles whose 3D box is not
within the radius of any stroke point are skipped. A 1500-point stroke on a 1024 picture of the crate takes well under a
second.

## The app

**Tools > Texture Paint** (command search, toolbar overflow after Vertex Paint; no key by default, like Vertex Paint).

- **Panel** (`tp-*`): Size (a log slider; "Auto" until set: about 6% of the model's diagonal, the same as Vertex Paint),
  Hardness, Opacity, the colour chip (the Paint panel's colour), Fill, Export, UV editor, and **New paint texture**: size
  (512 / 1024 / 2048) and base (the colour chip, or what the faces look like now). If the selected faces have no UVs or overlapping
  UVs the panel says so and the button becomes **Unwrap and create**.
- **3D view**: a ring follows the cursor over the surface (as for Vertex Paint); drag to paint; **Alt** held at the press erases
  back to the base colour; **Cmd / Ctrl-click** picks up the colour of the picture under the cursor (it becomes the paint colour);
  **[ ]** change the size. While dragging, the picture is drawn live: the pixels are decoded once at the press, each new
  stroke point is painted by `Painter` (`texturepaint.ts`, the same algorithm as the engine's: the same falloff, bleed, stretch
  bound, max-weight rule) onto a canvas, and that canvas is the meshes' texture; it is re-uploaded at most once a frame.
  On mouse-up one `texture_paint_stroke` goes out (at most 1500 points, thinned evenly). The preview canvas is kept as the
  texture of the new picture when it arrives (`adoptPainted` in `viewport/materials.ts`), so the surface does not blink while
  the real PNG loads; an Undo brings back the previous texture from the cache.
- **UV editor**: a **Paint** mode (key 4 or B) beside Points / Edges / Islands, with its own brush size, hardness and opacity
  controls and a ring. Drag on the picture; the stroke is drawn live on a paint layer (`addUvLayer`) and goes as `uv_points`.
  Alt erases, Cmd / Ctrl-click picks a colour, **[ ]** change the size, right-drag pans.
- **File > Export texture…** (and the panel's Export): saves the selected faces' picture, or the first paint texture, as a PNG
  (`save_image`; a picture that is not a PNG is converted in the page first).
- **Tools > New paint texture…** opens the tool (the panel's block needs the selection).
- Step hints: `guide.tpaint.*`; the panel's words `tp.*`; the UV editor's `uv.mode.paint`, `uv.step.paint`. English and Italian.

## Export

Nothing new: glTF / GLB embeds the PNG, FBX / OBJ / Collada write it beside the file, as for any texture. `core/tests/texture_paint.rs`
checks that the GLB's image decodes to the painted pixels.

## Known limits

- Erase goes back to the material's base colour (`Material.color`), not to the picture the texture started from. To keep the
  original look, make a second paint texture, or undo.
- The preview is drawn in sRGB-encoded 8-bit like the engine, but on scene triangles whose coordinates the scene rounds to
  1e-4 tiles and positions to 0.01 mm, so edge texels can differ from the engine's result by a pixel until the stroke is
  committed; the engine's picture is the truth and replaces the preview on the next change.
- Alt (erase) is read when the stroke starts, not mid-drag.
- The preview is for the 3D view; the UV editor shows the stroke on its own layer and the 3D view updates when the stroke ends.
- Placed components can only be painted with the component open (a paint texture is one material of the whole model; the
  stroke tool refuses components named from outside).
- Hidden faces are not painted. Faces of a group are reached; faces of nested components are not.
- A picture is limited as any picture is (16384 on a side, 100 million pixels); the app offers up to 2048, the command 4096.
- Undo holds a copy of the PNG per stroke (a few MB at 2048); very long sessions on a big picture are trimmed by the undo
  budget like any history.
- The visual reference of the panel exists for macOS only: run `node tools/ui-tests/run.mjs texture-paint -- --update` on Linux
  to add the Linux one (until then the Linux run says so and compares nothing).
