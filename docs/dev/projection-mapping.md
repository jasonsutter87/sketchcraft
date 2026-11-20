# Projection mapping

Lay a material's picture on a selection with one of four projections: planar, box, cylinder or sphere. The result is kept
as the faces' own texture coordinates, so the viewport, renders, glTF, OBJ, Collada and FBX all show it. Code:
`core/src/projection.rs` (the math, the Model methods), `core/src/commands/projection.rs` (the commands),
`app/src/projection.ts` and `app/src/projectionpanel.tsx` (the panel). Tests: `core/tests/projection.rs`, the unit tests
in `projection.rs`, `app/tests/projection.test.ts`, `tools/ui-tests/projection.mjs`.

## Commands

- `texture_project {faces | entity, mode, material?, origin?, axis?, scale?, rotation?, fit?}`
  - `faces` (or `entity`, one id) are faces, groups (every face inside, nested groups too) or placed components. A
    component is projected in its own frame, so the picture stays put when you move the component; every placement of the
    same component shows it (they share one definition), the same as painting inside it would.
  - `mode`: `planar | box | cylinder | sphere`, or `clear` (take the projection off; the material's own placement is used
    again).
  - `material`: paint the faces with it first (it needs to exist). Without it the faces keep their material; the
    coordinates are made for each face's own material (the tile size is that picture's width and height).
  - `scale` (default 1) is a size factor: 2 is a picture twice as big (or half as many repeats).
  - `rotation` (degrees): planar and box turn the picture in its plane; cylinder and sphere turn it around the axis,
    which moves the seam.
  - `fit`: cover the selection once. Default for cylinder (once around, bottom to top) and sphere (pole to pole); off for
    planar and box, which use the picture's own tile size in metres (the `texture` command's `width`/`height`).
  - `origin`: where the picture is centred (planar, box), or a point on the axis (cylinder) or the centre (sphere).
    Default: the middle of the selection's bounding box.
  - `axis`: planar, the direction the projector shines (default: against the biggest face's normal, so it lands on that
    face; the app sends the camera's direction); box, cylinder, sphere: the up axis (default z).
  - One undo step (paint and projection together). A refused call (unknown mode, scale 0, a zero axis, a missing
    material, a locked group) changes nothing.
- `texture_uv {faces}`: read-only; per face, the `[u, v]` of each corner of its outer loop (`null` if it has none).
- Both are on the plugin and MCP allowlist (`policy.rs` MODEL list, `mcp.rs` reference), like `paint` and `texture`.

## The four projections

All coordinates are in tiles (1 is the whole picture), v up the picture.

- **Planar**: one plane, facing the projector, the picture upright (the frame's v runs up the world when the projector is
  sideways). Faces facing away or edge-on are stretched, as with any slide projector.
- **Box**: each face takes the axis plane its normal leans on most (ties: up first). Seen from outside the picture reads
  left to right and is never mirrored; side faces have v up. Seams fall on the box's edges. The frame is the axis turned by
  `rotation`.
- **Cylinder**: angle around the axis gives u (the seam is at the -x side of the frame, turned by `rotation`), height
  along it gives v. A face whose normal leans on the axis more than 45 degrees is a cap and gets planar coordinates on the
  cross-section (the fitted disc spans the picture, radius 0.5 around its middle). A face that crosses the seam gets
  coordinates a little past 1 (or below 0) instead of smearing back across the picture, so adjacent faces share corner
  values, differing only by a whole number at the seam.
- **Sphere**: longitude gives u, latitude gives v (south pole 0, north pole 1). A corner on the axis (the globe's
  triangle-fan poles) takes the average angle of its face, so a fan does not shear. A face with the pole *inside* it (the
  soccer ball's top and bottom hexagons) has to go all the way round the picture; that is inherent to a lat/long map.

## Storage and the file format

- `Face.uv: Option<FaceUv>` (`core/src/model.rs`): `{outer: [[u,v],...], holes: [[[u,v],...],...]}`, one pair per vertex of
  each loop in the loop's order, f32, written only when set (`skip_serializing_if`). Files without it read as before.
- It is per corner, not per vertex, because a seam needs two values at one vertex. Coordinates apply only while the
  lengths still match the face's loops (`FaceUv::fits`); a face that was split, or whose outline changed size, goes back
  to the material's own placement instead of misreading stale numbers. Moving a vertex keeps the coordinates (the picture
  stays pinned to the corner, so it stretches). Reversing a face and exploding or mirroring a component carry them with
  the loops.
- **Format guard.** An older SketchCraft ignores a field it does not know and would drop it on the next save, so a file
  with any face coordinates (in the model or in a component) is written with `"format": 2` and `"needs": ["face_uv"]`
  (`core/src/format.rs`). A build that doesn't know `face_uv` opens it read-only. A model without projected faces is still
  format 1 and opens everywhere as before. (`needs` is now a list that can hold `sheets` and `face_uv` together.)
- Rendering: `texmap::FaceMap` replaces `Mapping` in the viewport scene, headless renders, glTF, OBJ, Collada and FBX. For a
  face with coordinates the triangulation's corner points are matched to the loop's points exactly (the triangulator hands
  back the loop's own points) and the headless renderer interpolates between them.

## The app

**Tools > Project texture…**, the right-click menu on a selection of faces, groups or components, and the command search
open a small panel (not a dialog, so the model stays in view). It picks the mode, the picture, size, turn and "fit once";
**Axis and center…** has the axis (for planar: from this view, from above, from the front, from the right) and the
centre. Each change runs the real command after taking the last try back (`undo`), so what you see is what the model
holds; **Apply** keeps it, **Cancel** (or Esc) takes it back, **Remove projection** clears it. The panel's one-line help
explains the chosen mode, and a message says what to do when nothing is selected or the model has no picture yet. English
and Italian strings are `proj.*` in the locale files.

## Known limits

- Faces inside a component nested in another component are not reached from the outer component (open it and select).
- No UV view or move/rotate/skew of the result yet (the roadmap's Position Texture and UV unwrap).
- The panel's live preview is a series of undo and redo steps; another edit made while the panel is open would be
  undone with the next change. The panel is about the selection it opened on.
- The visual reference of the panel exists for macOS only; run `node tools/ui-tests/run.mjs projection -- --update` on
  Linux to add the Linux one (until then the Linux run says so and compares nothing).
