# Position Texture

Move, rotate, scale and skew the picture on a face with four pins, as SketchUp's Fixed Pin mode does. Code:
`core/src/texposition.rs` (the math and the Model methods), `core/src/commands/projection.rs` (the commands),
`app/src/positiontexture.ts` (pin math), `app/src/positionpanel.tsx` (the overlay). Tests: `core/tests/texposition.rs`,
the unit tests in `texposition.rs`, `app/tests/positiontexture.test.ts`, `tools/ui-tests/position-texture.mjs`.

## Representation

A position change is an affine map of the picture's own plane, applied to the face's current texture coordinates and
written back as the face's per-corner `Face.uv` (see projection-mapping.md). This one choice makes it compose with every
way a face can already get its picture:

- a plain face (the material's own placement) and a painted-through face (`Face.tex`): the current coordinates are
  worked out from the placement, changed, and stored as `uv`. `uv` wins over `tex`, so the placement is left in place
  (a Reset drops both).
- a projected face: its `uv` corners are rewritten.

A skew or a different scale per axis cannot be written as `tex` (unit axes in the plane), which is why `uv` is the
target. "Picture space" is the coordinates times the tile size, in metres, so a rotation stays a rotation on a picture
that is not square.

## Commands

- `texture_position {face | faces, transform | matrix | reset}`
  - `transform {offset [du, dv] (metres), rotation (degrees), scale (n or [su, sv]), skew (degrees, n = along u, or
    [along u, along v]), pivot [u, v] (metres, default each face's middle)}`: scale and skew, then turn, all about the
    pivot, then move.
  - `matrix [a, b, c, d, e, f]`: p' = [a c; b d] p + [e f] in picture space. The app sends this.
  - `reset`: back to the material's own placement (drops `uv` and `tex`). With a `transform` or `matrix` too, the change is
    made from that placement, in one step.
  - One undo step. A refused call (no picture, a zero scale, a locked group, a face of another model) changes nothing.
  - Faces are those of the open model (open a component to position its faces).
- `texture_frame {face, default?}`: read-only. The corners' points, picture-space coordinates (`picture`), the tile size,
  the normal, and `own` (does the face hold its own coordinates). `default: true` gives what a reset would give.
- Both are on the plugin and MCP allowlist.

## The app

Right-click a single face with a picture: **Texture › Position** (also Tools menu and command search). The overlay is a
fixed layer over the viewport: it lets clicks through everywhere except the picture and the pins, so the model can still
be orbited. The viewport hands it three things (`ViewportApi.toClient`, `rayAt`, `watchCamera`); the rest is the
overlay's.

Two spaces: face space (orthonormal metres on the face's plane) and picture space. The pins sit on the corners of the
tile under the middle of the face, pulled back into face space. A drag is an affine change D of the picture's lay in face
space (`dragDelta`); the engine gets A = L D^-1 L^-1, L being the face-space-to-picture-space map fitted from the corners
(`pictureMatrix`).

- red / the picture: move.
- green: turn and scale evenly about red (the pin follows the pointer).
- blue: the picture's height stretches and leans; green stays.
- yellow: skew along the far diagonal; the other diagonal stays. **This is an affine stand-in for SketchUp's free
  "distort" pin: a true perspective distort is not offered** (a face's coordinates are interpolated per triangle, so
  perspective would need a projective coordinate the format does not have).

Preview is a series of `undo` and re-run steps, every try computed from the state the session started with, so a whole
session leaves exactly one step (the same technique as the Project texture panel). Enter or a click on empty model keeps
it, Esc or Cancel takes it back. The right-click menu (Reset, flips, turns, Done) changes the same running total; Reset
starts the session over from the material's own placement (`texture_frame default`, then `reset` with the matrix).

## Format guard

`Face.tex` (paint-through's placement) was not covered by the guard: an older build would have dropped it on the next save.
A model with any `tex` now writes `format 2` and `needs ["face_tex"]` (`core/src/format.rs`, like `face_uv`). Positioned
faces carry `uv`, so they need `face_uv`.

## Known limits

- Typed values in the measurements box are not built yet.
- The pins live on one face. A face whose coordinates are not planar (a projected cylinder or sphere face) gets a fitted
  affine map, so its pins are approximate; the change itself is exact per corner.
- Faces inside a component are positioned with the component open; the overlay refuses otherwise with the engine's reason.
- The visual reference (the hint bar) exists for macOS only; run `node tools/ui-tests/run.mjs position-texture -- --update`
  on Linux to add the Linux one.
