# Vertex painting

A brush that tints the corners of faces. The colours are kept as the faces' own per-corner data (the same idea as the
texture coordinates in [projection-mapping.md](projection-mapping.md)), shown in the viewport and renders, and exported
to glTF / GLB and FBX. Code: `core/src/vertex_paint.rs` (the maths and the Model methods),
`core/src/commands/vertex_paint.rs` (the commands), `app/src/vertexpaint.ts` (stroke batching),
`app/src/vertexpaintpanel.tsx` (the panel) and the `vpaint` tool in `app/src/Viewport.tsx`. Tests:
`core/tests/vertex_paint.rs`, the unit tests in `vertex_paint.rs`, `app/tests/vertexpaint.test.ts`,
`tools/ui-tests/vertex-paint.mjs`.

## Commands

- `vertex_paint {faces | entity, color?, points?, radius?, strength?, falloff?, erase?}`: one brush stroke.
  - `faces` (or `entity`, one id) are faces, groups (every face inside) or placed components. A component is painted in
    its own frame (the stroke's points are taken into it through the placement's inverse; `radius` is divided by its
    uniform scale), so every placement of the same component shows the colours, as with `paint` and `texture_project`.
  - `color`: `[r, g, b]` or `[r, g, b, a]`, 0..1 (alpha defaults to 1). Required unless `erase`.
  - `points`: the stroke, `[[x, y, z], ...]` in world metres. Every face corner within `radius` (default 0.1) of any of
    them blends toward `color`: `new = old + (color - old) * strength * falloff(distance / radius)`. A corner near several
    points takes the strongest weight, not the sum, so overlapping dabs do not pile up inside one stroke. Without `points`
    the whole selection is filled at `strength`.
  - `strength`: 0..1, default 1. `falloff`: `smooth` (smoothstep, default), `linear`, `hard` (full strength inside the
    radius).
  - `erase: true` blends toward white (no tint) instead; a face whose corners are all white again drops its colours.
  - One undo step, so the app sends a whole drag as one command. A refused call (no faces, bad colour, strength outside
    0..1, radius 0 with points, unknown falloff, a locked group) changes nothing.
- `vertex_paint_clear {faces | entity}`: take the colours off. One undo step.
- `vertex_colors {faces}`: read-only; per face, the `[r, g, b, a]` of each corner of its outer loop (`null` if none).
  Also takes a group or a placed component (all its faces).
- All three are on the plugin and MCP allowlist (`policy.rs`, `mcp.rs`).

## Storage and the file format

- `Face.vcol: Option<FaceColors>` (`core/src/model.rs`): `{outer: [[r,g,b,a],...], holes: [[[r,g,b,a],...],...]}`, one
  RGBA (f32, white = no tint) per vertex of each loop in the loop's order, written only when set. Same rules as
  `Face.uv`: it is per corner so a seam can differ across an edge, it applies only while its lengths still match the
  face's loops (`FaceColors::fits`), moving a vertex keeps it, and reversing a face, exploding or mirroring a component
  carry it with the loops.
- **Format guard.** A file with vertex colours anywhere (the model or a component) is written `"format": 2` with
  `"needs": ["face_vcol"]` (`core/src/format.rs`; the list can hold `sheets`, `face_uv` and `face_vcol`). A build that
  does not know `face_vcol` opens it read-only instead of dropping the colours on its next save.

## Display

- `scene::build` adds `vcolors`: an rgb tint per vertex (the same layout as `positions`), empty when nothing is painted.
  The alpha is folded in as `1 + (c - 1) * a`, so a half-transparent colour tints half as much.
- The viewport multiplies the tint into the face colour attribute (front and back) and, for textured faces, into a vertex
  colour attribute on the picture layer, so colour over a picture is `picture * colour`. A plain white material shows the
  colour as painted. Smooth interpolation across a face is the GPU's. **View > Vertex colours** (`styles.vertexColors`,
  default on) turns it off. Monochrome, hidden line and wireframe styles ignore the colours.
- The headless render (`render.rs`) draws untextured faces with Gouraud shading over the corner tints. Textured faces,
  blueprint and cut (`clip`) renders show the plain material for now.

## Export

- **glTF / GLB**: `COLOR_0` (VEC4, float) on every primitive that has a painted face, one per vertex (white for the
  unpainted faces of that primitive). glTF multiplies it with the material's base colour and picture, so the material
  needs no change; a primitive with any alpha below 1 gets `alphaMode: BLEND`.
- **FBX**: a `LayerElementColor` (`ByPolygonVertex`, `IndexToDirect`, RGBA) on any mesh with a painted face. Our importer
  ignores colours (it never read them), so the test checks the file's structure.
- **OBJ**: skipped. The `v x y z r g b` extension is not read by every tool and would change the meaning of the `v` lines
  for the ones that do not expect it. **Collada** is skipped too.

## The app

**Tools > Vertex Paint**, the command search and the toolbar overflow (it sits after the Paint Bucket in the Model
workspace). A brush circle follows the cursor over the surface (always facing the camera); while dragging, the stroke's
path is drawn and each new point is laid along the line from the last one every 0.3 radius, so a fast drag leaves no gaps
(`extendStroke`). The stroke goes on mouse up as one `vertex_paint` command (at most 1500 points, thinned evenly), which
is one undo step. Targets are the faces, groups or components the brush went over. **[** / **]** change the size by a
quarter; Alt (held at any moment of the drag) erases. The colour is the paint colour chip's. The panel
(`vertexpaintpanel.tsx`) has Size (a log slider; "Auto" until set: about 6% of the model's diagonal), Strength, the edge,
and Fill / Clear for the selection. Step hints are `guide.vpaint.*` and the panel's words `vp.*`, in the English and
Italian locale files.

## Known limits

- The colour lives on corners: a face with four corners only blends between them, so a soft local brush needs a model with
  enough faces (the globe and soccer ball do).
- The picture appears when the stroke ends, not while dragging (the engine owns the colours; the viewport draws the brush
  and path meanwhile).
- A stroke paints only the entities the cursor went over, not neighbours the brush circle merely overlaps.
- The brush radius is in world metres, measured straight (not along the surface), so it can reach through a thin wall to
  the other side of the same entity.
- Faces inside a component nested in another component are not reached from the outer component (open it and select).
- Headless renders show vertex colours on untextured faces only; no OBJ or Collada colours.
