# Room mode

Draw a room the way you do in The Sims: press, drag a rectangle on the ground, let go, and you get four walls and a
floor. Drag over an existing room and you combine with it (an L, a T...) or, with Alt, cut a notch out of it. Code:
`core/src/room.rs` (engine), `core/src/rectpoly.rs` (axis-aligned polygons and their booleans), the `room` tool in
`app/src/Viewport.tsx` (interface), `app/src/viewport/rooms.ts` (grid, area, label and mode helpers), tests in
`core/tests/room_mode.rs`, `core/tests/room_boolean.rs`, `app/tests/rooms.test.ts` and `tools/ui-tests/room-mode.mjs`. It is built on Wall pull (`docs/dev/wall-pull.md`): moving a room's wall is `wall_pull`,
nothing new.

## What it does

- `room_rect {from, to, thickness?, height?, name?, mode?}`: `from` and `to` are opposite corners on the ground (the
  wall centre lines). `mode` is `"new"` (default: an overlap is refused), `"combine"` or `"subtract"` (see Booleans).
  One command, so one Undo step. Returns the group, its name, the wall groups, the floor, the inside size and area, the
  inside `outline` and the wall `centre_outline` (corner lists), a `label` point inside the room, the rooms whose walls
  it shares, whether a side was snapped, the `mode` that really applied and its `target` room, and for a boolean
  `kept_openings` / `removed_openings`.
- `room_preview {from, to, thickness?, mode?}`: read-only. The same checks and the same answer, without building. The
  app asks it as you drag, to draw the resulting outline and to say why a spot is refused.
- `rooms`: read-only. Every room: id, name, floor, inside bounding rectangle, `outline` (the inside polygon), area (m²),
  wall thickness, `center` (a point inside the outline, for the label). The app's labels and the Room options list use
  it, and ask again whenever the model changes (so a pulled wall updates the labels).
- In the app: **Room** in the Build panel (Library tab, left) and on the Arch toolbar; it is in the "more" list of
  other workspaces. Over bare ground: press, drag, release (or click, move, click). Over a wall: drag it (Wall pull), or
  just click it for a door or window, as chosen in the Room options. **Walls down** (Room options) is a view setting.
- Also exposed to agents and plugins (`policy.rs` MODEL list, MCP reference), the way `wall_pull` is: they are plain model
  edits (`room_rect` makes ordinary groups and faces through `wall`, `add_polygon` and `set_tag`, which are already on
  the list) and `room_preview` and `rooms` only read. A refused call changes nothing.

## What a room is

Ordinary geometry and no new file data:

```
Room 1            group, tag "Rooms"
  Walls           group, tag "Walls": a closed ring (nothing shared), or open runs of wall (some sides shared)
  (floor)         one face at the ground, the inside of the room
```

A room's shape is read back from the model: the floor face's outline is its inside (a rectilinear polygon; a floor that
is no longer axis-aligned falls back to its bounding rectangle), and its wall thickness is measured from its own walls
with `wall_plan`. So a room follows whatever Wall pull (or any other tool) does to it, and nothing can fall
out of step. The name is the group's name (`rename` works), "Room N" taking the next free number.

## Sharing walls

Work in wall centre lines. A new room is a rectangle `[x0, y0, x1, y1]`; each existing room has one too (its floor grown
by half a wall).

1. **Snap to touch.** A side that runs alongside a neighbour's wall line and is within one wall thickness (but not on
   it) moves onto the line. (A drag that lands 0.1 m off a 0.2 m wall would otherwise leave a gap or a double wall.)
2. **Shared sides.** A side that lies on a neighbour's wall line for some length is shared along that length. A room
   that shares a wall takes that wall's thickness; two neighbours with different thicknesses are refused.
3. **Overlaps are refused, not snapped, in mode `new`.** Snapping can only close a gap of one wall thickness or less;
   anything bigger is a real overlap, and silently moving a room by metres would surprise people. The insides of the two
   rooms must not overlap, and the new walls must not cut into another room, nor the new inside into another room's
   walls. The answer names the room it hit. (Modes `combine` and `subtract` are the way to drag over a room: below.)
4. **Only the uncovered stretches get walls.** What is left of the perimeter is chained into runs, and each run is built
   with the Wall tool's own `wall` (miter corners, flat ends). A run end that falls inside a neighbour's wall is trimmed
   back to the neighbour's face. So a second room beside the first has an open U of wall (three walls) whose two ends
   touch the shared wall: the layout Wall pull already treats as a T-junction. Pulling the shared wall therefore stretches
   both rooms, with no extra code: the new walls' ends and the floor's corners lie inside the slab, and slide along their
   edges.
5. A room touching another only at a corner is fine: the neighbour's corner fills it.

Everything is checked before anything is built, so a refusal leaves the model exactly as it was.

## Booleans: Combine and Subtract

`room_rect` / `room_preview` with `mode` `combine` or `subtract`. The maths is in `rectpoly.rs`: coordinate compression
on the distinct x and y of both outlines (every grid cell is wholly in or out of each shape), then the boundary between
the kept cells is chained back into outlines. Exact, no tolerances beyond 1e-9, no new crates.

1. The dragged box (snapped as above) is a rectangle of wall centre lines. The **target** is the one room whose inside the
   box's inside overlaps. None: `combine` is just a new room (the answer says `mode: "new"`), `subtract` is refused
   ("drag the box over a room"). Two or more: refused ("one room at a time").
2. The new centre-line outline is `room ∪ box` or `room − box`, taken on the centre-line polygons (the room's inside grown
   by half its wall). The result must be one piece with no hole (otherwise: "split the room in two" / "leave a hole"), must
   change something, must not shrink any part below 0.3 m, and keeps the target's wall thickness.
3. The result is checked against the other rooms exactly as a new room is (no overlap, no walls cut into). And **a wall
   another room leans on must survive**: if another room's open side lies along the target's wall and the new outline no
   longer runs there, the operation is refused ("would take away the wall Room 2 shares with Room 1").
4. **Wall rebuild.** The target's own wall groups and floor are removed and rebuilt in the same room group (same
   thickness and height, the floor a new face): a closed ring when no stretch of the outline lies along another room's wall,
   otherwise open runs that stop at the neighbour's wall face, as for a new room. "Another room's wall" is read from its
   real wall faces here, not from its outline, so a wall the *target* owns and a neighbour leans on is rebuilt, while a
   wall the neighbour owns is left alone and not doubled. Other rooms' faces are not touched (a test checks the face ids).
5. **Doors and windows** (instances whose back outline sits on one of the target's wall faces) are collected first, and
   after the rebuild each is cut into the new wall face at the same place (`openings::cut`) if there is one; otherwise the
   instance is removed. The counts come back as `kept_openings` and `removed_openings`.
6. All of it is one command, so one Undo; on any failure midway the model is restored from a copy taken at the start.

## The tool

`Viewport.tsx` follows the `wallpull` tool: a `room` op (corners `a`, `b`) plus the shared `wallpull` op. A press over a
wall (the engine's `wall_info` says it is one) becomes a Wall pull drag, and a click without a drag places a door or window
with the same `insert` the Place tool uses; a press anywhere else starts a room. Corners snap to the grid
(`gridStep`: 0.1 m, 6 in in feet and inches). `room_preview` is asked once per grid step (and when Alt changes), always
with `mode: "combine"`, or `"subtract"` while Alt (Option) is held (`roomMode`). Over bare ground combine is a plain new room.
The preview draws the result's outline and floor (`fillRects` turns the outline into grid cells, no triangulation) with
the dragged box dashed on top. The live label is the inside width x depth and area (`dragLabel`) for a new room, or
"Combine with Kitchen · 25.6 m²" / "Subtract from Kitchen · 12.2 m²" over a room; the step hint says what Alt does. The
persistent labels are plain `LabelSpec`s from `rooms`, at the engine's `center`, a point inside the outline (the centroid
if it is inside, otherwise the middle of the largest grid cell; `labelPoint` in `rooms.ts` is the same rule).

**Walls down** is a clip plane at 1 m held in the viewport (`st.current.cutaway`), used only while Room mode is on and
only for drawing and picking: it is not a section plane, so nothing in the model changes and nothing is saved.

## Limits

- Rooms are axis-aligned (rectilinear) polygons: drawn as rectangles, made L-, T- or U-shaped by Combine and Subtract.
  Rotated or curved rooms are a later step (draw walls with Wall).
- Combine and Subtract refuse a result with a hole or in two pieces, a box over two rooms, and a result that would
  touch itself at a corner. Pulling a wall of a compound room works (Wall pull), but dragging over it again rebuilds all
  of that room's walls, so hand edits to its walls (painted faces, openings that are not doors or windows) are lost.
- A subtract cannot remove a wall another room leans on; draw the cut so that wall stays.
- Pressing on a wall to start a room pulls the wall instead; start on open ground (the far corner).
- A wall that carries on in a straight line into a neighbouring room's wall (two rooms with a common south wall, for
  instance) can't be pulled on its own: Wall pull refuses it ("pull the whole straight run"), with the model untouched.
  The shared wall between them can.
- A room is found by its "Rooms" tag and its floor. If you move walls by hand until the floor is no longer the inside,
  the label shows the floor's area, and a later room's snapping works from the floor's bounding box.
- Different wall thicknesses between neighbours can't share a wall.
- Walls down has no cut-through caps (the walls are simply clipped), shows everything above 1 m as gone, not only the
  walls facing the camera, and picking ignores what it hides.
- The refusal messages come from the engine and are English; the tool's own labels, hints and tooltips are in the
  locale catalogs (`locale-en.ts`, `locale-it.ts`). Italian review by Kad26 is welcome.

## What the next steps plug into

`room_plan_mode` returns the snapped outline, the thickness, the shared sides and the target; `rooms()` is the one place
that reads rooms back. The next booleans only need another `rectpoly::Op` rule and a `RoomMode` name:

- **Inner** (intersect): keep only where the box overlaps the room (`Op::Intersect` already exists and is tested).
- **Outer** (xor, "everything but the overlap"): keep the room and the box minus the overlap, which usually leaves two
  pieces, so it needs the multi-piece handling below.
- **Split**: cut the room along the box into separate rooms. `rectpoly::boolean` already returns every piece and hole; what
  is missing is one room group per piece (new names, walls on the new cut line) and deciding who keeps the doors.

Holes (a pillar or courtyard) would need a floor with an inner loop and walls round it: `boolean` returns the clockwise hole
loops already.
