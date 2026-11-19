# Fixtures & furniture

Twenty-six fixed Library assets for furnishing rooms (made with Room mode or any walls): bath, kitchen, bedroom,
living and dining, laundry. They are ordinary built-in assets, not generators: no parameters, one shape and size each.
All geometry is original and MIT, written as command scripts like the other assets.

## Where things live

- `core/assets/<key>.jsonl`: one script per fixture. One top-level group named like the asset, built from
  sub-groups (Bed: Headboard, Base, Footboard, Mattress, Pillows, Duvet; Dining set: Table top, Table legs, Chair 1 to 4,
  each chair Seat, Back, Legs). Neutral colours: white porcelain, wood, steel, fabric, one glass material on the shower.
- `core/src/assets.rs`: the registrations (`about` carries the size).
- `app/src/fixtures.ts`: the Library list (key, group, size) and `wallSnap`.
- `app/src/library.tsx`: the "Fixtures & furniture" section with a sub-group per room, searchable by name or room.
- `app/src/locale-en.ts`, `locale-it.ts`: `fixtures.*` and `fixture.<key>` keys (Italian review by Kad26 welcome).
- Tests: `core/tests/modeling.rs` (`fixtures_build_as_one_group_at_their_size_with_the_back_on_the_wall`),
  `app/tests/fixtures.test.ts`, `tools/ui-tests/fixtures.mjs`.

## Convention

Origin on the floor at the middle of the BACK edge; the back is the plane y = 0 and the body extends toward -Y, so the
front faces -Y (the same way doors and windows face). Putting the origin on a wall face with the wall's normal pointing into
the room, and a rotation of `atan2(ny, nx) + 90` degrees, puts the back flush against the wall. Wall-mounted pieces
(wall cabinet, towel bar) keep the same origin on the floor and float at their mounting height (1.40 m and 0.97 m).
The stated size is the whole bounding box, handles and taps included, and the Rust test holds each to 1 cm.

## Wall snap

While placing a fixture (`fixtureSpot` in `Viewport.tsx`), if the cursor is within 0.4 m of the room side of a wall face,
the fixture turns to face away from it and slides so its back is flush; the hint reads "Against the wall · Shift to place
freely". Shift places it where the cursor is. It uses the normal `insert` command, so one click is one undo step.

A wall face is an upright face at least 1.2 m tall and 2 m² (so cabinet doors and the sides of other fixtures do not
count), standing at the cursor's floor. A fixture is slid along the wall so it stays on that face at a corner.

## Adding one

Write the script (follow `core/assets/toilet.jsonl`), add an `Asset` line in `assets.rs`, a row in `fixtures.ts` and the
test table, and the two locale keys. Keep it small and simple; do not add configurable generators here.

## Known limits

- Fixtures are not aware of each other: two can overlap, and the snap does not use a neighbour's side.
- The snap is for walls: a fixture next to a low or short wall face (under 1.2 m, such as walls turned down) is not snapped.
- Doors and windows are not part of this set; they keep cutting openings as before.
- Sizes are common US/EU standards, rounded; they are a starting point for layout, not manufacturer drawings.
