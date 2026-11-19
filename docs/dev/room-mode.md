# Room mode

Draw a room the way you do in The Sims: press, drag a rectangle on the ground, let go, and you get four walls and a
floor. Code: `core/src/room.rs` (engine), the `room` tool in `app/src/Viewport.tsx` (interface), `app/src/viewport/rooms.ts`
(grid, area and label helpers), tests in `core/tests/room_mode.rs`, `app/tests/rooms.test.ts` and
`tools/ui-tests/room-mode.mjs`. It is built on Wall pull (`docs/dev/wall-pull.md`): moving a room's wall is `wall_pull`,
nothing new.

## What it does

- `room_rect {from, to, thickness?, height?, name?}`: `from` and `to` are opposite corners on the ground (the wall
  centre lines). One command, so one Undo step. Returns the group, its name, the wall groups, the floor, the inside size
  and area, the rooms whose walls it shares, and whether a side was snapped.
- `room_preview {from, to, thickness?}`: read-only. The same checks and the same answer, without building. The app asks
  it as you drag, to draw the snapped rectangle and to say why a spot is refused.
- `rooms`: read-only. Every room: id, name, floor, inside rectangle, area (m²), wall thickness, centre. The app's labels
  and the Room options list use it, and ask again whenever the model changes (so a pulled wall updates the labels).
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

A room's size is read back from the model: the floor face is its inside, and its wall thickness is measured from its
own walls with `wall_plan`. So a room follows whatever Wall pull (or any other tool) does to it, and nothing can fall
out of step. The name is the group's name (`rename` works), "Room N" taking the next free number.

## Sharing walls

Work in wall centre lines. A new room is a rectangle `[x0, y0, x1, y1]`; each existing room has one too (its floor grown
by half a wall).

1. **Snap to touch.** A side that runs alongside a neighbour's wall line and is within one wall thickness (but not on
   it) moves onto the line. (A drag that lands 0.1 m off a 0.2 m wall would otherwise leave a gap or a double wall.)
2. **Shared sides.** A side that lies on a neighbour's wall line for some length is shared along that length. A room
   that shares a wall takes that wall's thickness; two neighbours with different thicknesses are refused.
3. **Overlaps are refused, not snapped.** Snapping can only close a gap of one wall thickness or less; anything bigger
   is a real overlap, and silently moving a room by metres would surprise people. The insides of the two rooms must not
   overlap, and the new walls must not cut into another room, nor the new inside into another room's walls. The answer
   names the room it hit.
4. **Only the uncovered stretches get walls.** What is left of the perimeter is chained into runs, and each run is built
   with the Wall tool's own `wall` (miter corners, flat ends). A run end that falls inside a neighbour's wall is trimmed
   back to the neighbour's face. So a second room beside the first has an open U of wall (three walls) whose two ends
   touch the shared wall: the layout Wall pull already treats as a T-junction. Pulling the shared wall therefore stretches
   both rooms, with no extra code: the new walls' ends and the floor's corners lie inside the slab, and slide along their
   edges.
5. A room touching another only at a corner is fine: the neighbour's corner fills it.

Everything is checked before anything is built, so a refusal leaves the model exactly as it was.

## The tool

`Viewport.tsx` follows the `wallpull` tool: a `room` op (corners `a`, `b`) plus the shared `wallpull` op. A press over a
wall (the engine's `wall_info` says it is one) becomes a Wall pull drag, and a click without a drag places a door or window
with the same `insert` the Place tool uses; a press anywhere else starts a room. Corners snap to the grid
(`gridStep`: 0.1 m, 6 in in feet and inches). `room_preview` is asked once per grid step. The live label is the inside
width x depth and area in the document's units (`dragLabel`); the persistent labels are plain `LabelSpec`s from `rooms`.

**Walls down** is a clip plane at 1 m held in the viewport (`st.current.cutaway`), used only while Room mode is on and
only for drawing and picking: it is not a section plane, so nothing in the model changes and nothing is saved.

## Limits

- Rooms are rectangles, axis-aligned. Drawing a rotated or L-shaped room is a later step (draw rooms and use Wall).
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

`room_plan` returns the snapped rectangle, the thickness and the shared sides, which is everything a rotated or
polygon room needs to reuse; `rooms()` is the one place that reads rooms back.
