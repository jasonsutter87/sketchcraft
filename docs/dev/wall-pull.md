# Wall pull

Grab a wall and pull it, the way you drag a wall in The Sims: the wall slides sideways, the walls joined to it stretch
or shrink to stay joined, its doors and windows go with it. Code: `core/src/wallpull.rs` (engine), the `wallpull` tool in
`app/src/Viewport.tsx` (interface), tests in `core/tests/wall_pull.rs` and `tools/ui-tests/wall-pull.mjs`.

## What it does

- `wall_pull {face, by}`: `face` is either upright side of a wall, `by` is metres along that face's outward normal
  (negative goes the other way). One command, so one Undo step.
- `wall_info {face, at?}`: read-only. Says what a pull would involve (the wall's faces, the pull direction, and where
  every moving point goes per metre pulled). The app uses it to highlight the whole wall on hover and to preview the
  drag. A click on a wall's *top* (what a plan view shows) gives a horizontal face: pass the point as `at` and the
  nearest long side of the wall under it is used (end caps are skipped).
- In the app: **Pull wall** in the Build panel and on the Arch toolbar. Hover a wall and it lights up whole, drag it (or
  click, move, click), or type a distance and Enter; Esc cancels. The step hint beside the model says each step.
- Also exposed to agents and plugins (`policy.rs` MODEL list, MCP reference). It is a plain model edit like `move` and
  `pushpull`: it can only change vertex positions and shift placements, it refuses rather than breaks geometry, and a
  refused call changes nothing.

## How it works

A wall is a straight upright slab found from the face you pick (A):

1. **The other side (B).** The nearest face behind A, in the same group, parallel and opposite, the same height, up to
   1.5 m away and overlapping A along the wall. Some face (a top or a bottom) must join A's and B's vertices, so a lone
   sheet is refused. This works for the Wall tool's rooms (where the sides share no edges, only the ring's top face)
   and for plain box walls from other tools or imports.
2. **What moves.** The wall's own vertices (both sides, openings and their reveals included), plus any vertex of
   *another* wall that sits inside the slab and outline (a wall drawn separately that ends on this one: a T-junction).
3. **How far each goes.** A vertex with no outside neighbour just travels along the normal. A vertex that has an edge
   to something outside the wall *slides along that edge* until it reaches the moved plane: that is the corner of a
   neighbour stretching. Because that is a straight line, the displacement is exactly `per_m * by`, linear in the
   distance, so one `wall_info` answer previews any drag.
4. **Doors and windows.** Cutting components whose back sits on either side of the wall, inside its outline, are shifted
   with it. Openings in neighbours are not moved.
5. **Checks, before anything is touched.** Every face that changes must stay flat, keep facing the same way, and keep
   edges longer than 1 mm; a face with openings must still contain all of them. Otherwise the pull is refused with a
   message ("can't pull that far: a door or window in the wall beside it wouldn't fit any more") and the model is left
   exactly as it was. Only then are the vertices and placements changed.

## Limits (what it says no to)

- The picked face must be upright and flat, with a parallel face behind it (thickness 5 mm to 1.5 m) of the same
  height, joined by a top or bottom. Sloped walls, gables whose two sides differ in height, curved walls and single
  sheets are refused, with the reason.
- A wall that carries on in a straight line past its end (two segments end to end) is refused ("pull the whole straight
  run"): pulling only one would shear the other.
- A corner that joins other walls in different directions at one vertex can't stretch them, and is refused.
- Pulling a wall into another wall or across the room is only caught when a neighbour would flip or collapse, not when
  two unrelated walls would cross.
- Anything else touching the wall's slab (a floor slab under it, a stem ending in it) follows along its own edges; a
  slab edge that runs alongside the wall tolerates this by staying put and tilting. Things that merely stand near the
  wall (furniture, other rooms) do not move.
- On a ring (the Wall tool's closed room) the top is one face shared by all four walls, so the app highlights the wall's
  own strip of it. Dimensions and annotations are not re-attached.

## What Room mode plugs into

Room mode (drag a whole room out) is the same machinery applied to a ring of walls. `Model::wall_plan(face)` returns a
`WallPlan` (the wall's faces, `moves`: vertex -> travel per metre, riding placements) and `Model::wall_pull` validates
and applies one. Room mode can take the plans of several walls, merge their `moves` (a shared corner vertex is the sum
of the two slides), and run the same checks once; or it can call `wall_pull` once per wall inside one Undo step. The
app side reuses `lookAtWall` (hover), `wallTriangles` (highlight and preview) and the drag-and-type handling of the
`wallpull` op.
