# Small drawing tools: Curve, Make faces, Flatten to height

Three small tools people used to install add-ons for (#25). Code: `core/src/curves.rs`, `core/src/makefaces.rs`,
`core/src/flatten.rs` (all on `Model`), the command arms in `core/src/commands/draw.rs`, the Curve tool in
`app/src/Viewport.tsx` (`spline` op), its preview maths in `app/src/curvemath.ts`, the menu words in
`app/src/smalltools.ts`. Tests: `core/tests/small_tools.rs` (with `core/tests/data/plate_3d_linework.dxf`),
`app/tests/curvemath.test.ts`, `app/tests/smalltools.test.ts`, `tools/ui-tests/small-tools.mjs` and the `spline`
scenario in `tools/ui-tests/tools.mjs`.

## Commands

All four are in the model-only allowlist (`policy.rs`: agents and plugins may run them), described in `mcp.rs`, and
`spline`/`bezier` are counted as drawing commands (their `points` are capped like `line`'s). Each mutating command is one
undo step, as for every command (the engine snapshots around it; a reply with `unchanged: true` takes no step).

- `spline {points, closed?, segments?=12}` and `bezier {points, closed?, segments?=16}`: edges laid down like `arc`
  (`draw_line` per piece, then `curve_through`), so the chain carries one `curve` id and is picked and followed as one. A
  closed flat loop makes a face (a loop longer than `draw_line`'s 14-edge search gets one made from its points).
  `segments` is per span, clamped to 1..256; at most 20 000 edges a command.
- `make_faces {ids?, open_holes?=false}`: lines (or groups, for their loose lines); none = every loose line in unlocked
  groups. A line is loose when no face uses it.
- `flatten {ids?, z}`: `z` in metres (the app converts the typed height). `ids`: points, lines, faces or groups; none =
  every point in unlocked groups. Placed components are refused (open them with `edit`).

## Why a spline through points, and not Bezier handles, in the tool

The tool model in the app is "click points" (line, wall, 3-point arc); dragging handles needs press-drag-release
gestures the multi-click tools do not have, and a Bezier is hard to place with no handles to see. A click-through
spline (centripetal Catmull-Rom) is the same family of curve (every Catmull-Rom span is a cubic Bezier) and needs no
handles: you put points where the curve should be. Centripetal parametrisation never loops or cusps when clicks are
unevenly spaced. People who want explicit handles, and agents, have `bezier`, which takes the cubic control points
directly. The Curve tool's preview (`curvemath.ts`) is the same maths as the engine; both are checked against the same
numbers (`spline_reference_values` in `small_tools.rs`, `curvemath.test.ts`).

Typing a number during or before a curve sets the pieces per span (`st.current.segs.spline`, like `24s` for circles);
it is remembered for the session, not saved.

## Make faces: how it works

Per group (vertices never weld across groups, so a hole on another DXF layer is not found):

1. **Node.** Lines that cross, or where an end lies on another line's middle (including collinear overlap), are split
   at those points with `split_edge` (curve ids are kept). Quadratic in the number of lines, so past 3 000 lines it is
   skipped and a note says so.
2. **Pieces and planes.** Connected pieces are found; each is fitted with a plane (0.01 mm tolerance). A piece that is
   not flat is skipped and reported in `non_planar` with a place; a straight piece is only counted as open. Pieces in
   the same plane (same normal, same offset) are walked together. The normal is turned to face up where it can, then +y,
   then +x, so floor plans get faces that look up.
3. **Walk.** Dangling ends are peeled away (`open_edges`), neighbours sorted by angle, and the planar half-edge walk of
   the DXF importer (`faces_in_plan`, which only does one horizontal level) gives bounded regions (counter-clockwise)
   and outer boundaries (clockwise). A cycle that touches itself is skipped (`degenerate`).
4. **Holes.** Each outer boundary of a piece lying inside a region of another piece is a hole in the smallest such
   region. The regions inside holes get faces too, the same as the DXF importer and as drawing a rectangle on a face
   in SketchUp. `open_holes` alternates instead: the region inside a hole is left empty (a plate with a drilled hole).
5. A region that already has a face (same corners) is left alone (`existing`), so running it twice changes nothing.

## Flatten: how it works

Moves the points to `z`, then joins each moved point to the first point already within 1 µm of it at that height (same
group only), follows the joins in every line and face, and drops: lines whose ends joined, lines that now repeat
another, faces left with fewer than three corners or no area (a wall seen from above), faces that now touch
themselves, and faces a moved point bent out of flat (a face outside the selection that shared a moved point).
Faces that end up level and face down are turned up. A face whose corner count changed loses its per-corner picture
data (UVs, vertex colours).

## Known limits

- Make faces does not find the flat loops inside a bent piece (a wire cube makes none: flatten, or draw the faces).
- Loops are found within one group (one DXF layer): a hole on another layer is not cut out of a plate.
- A loop drawn on top of an existing coplanar face does not cut a hole in that face (draw it with `line` for that).
- Flatten does not follow sheet anchors or attached dimensions onto joined points: a dimension attached to a point that
  was joined may detach.
- The Curve tool has no Bezier handles (see above) and no Backspace to drop the last point; Esc starts again.
