# Fix model

One command that cleans a model up and makes it solid, and a check that says why it isn't. Code: `core/src/fix.rs`
(the logic, on `Model`), the commands in `core/src/commands/shape.rs`, the words in `app/src/fixmodel.ts`.
Tests: `core/tests/fix_model.rs`, `app/tests/fixmodel.test.ts`.

## The Rust API (for the 3D-printing check and 3MF export)

```rust
let scope = model.fix_scope(&ids)?;          // ids: groups (with everything nested) or faces; empty = everything
let check = model.solid_check(&scope);       // read-only: SolidCheck, every problem with an id and a place
let ok    = model.is_solid(&scope);          // check.solid
let all   = model.everything_is_solid();     // whole model, component definitions too
let report = model.fix_model(&scope, &FixOptions::default());   // repairs in place, returns FixReport
let report = model.fix_everything(&FixOptions::default());      // the same, plus every component's own geometry
```

`SolidCheck`: `solid`, `faces`, and spots (`{id, at}`) for `open_edges` (one face: a hole), `non_manifold_edges` (three
or more faces), `inconsistent_edges` (two faces running it the same way: one is reversed), `inward_shells` (closed and
inside out), `duplicate_faces`, `loose_edges`. `solid` means faces exist and none of the first five lists has
anything; loose lines do not count against it. `FixReport` has one list of `Change {id, at, with}` per repair and
`remaining` (a `SolidCheck` taken after the repair) and `notes` (what it could not do, in words).

## Commands

- `fix_model {ids?, dry_run?, stray_edges?=true}`: one undo step (the engine snapshots around any mutating command; a
  reply with `unchanged: true` takes no step). `dry_run` runs on a copy and is not a mutating command (allowed on a
  read-only model). Inside `edit` it works on the open component's own geometry.
- `check_solid {ids?}`: read-only `SolidCheck`.
Both are in the model-only allowlist (`policy.rs`: agents and plugins may run them) and described in `mcp.rs`.

## What it does, in this order

Everything works on shared vertex ids, so a group is its own world and groups never interact.

1. **Repeated faces**: same corners; the lowest id stays. A back-to-back pair whose every edge has two other faces is a
   wall between two solids: both go (`internal_faces`). A back-to-back pair that is just one face twice loses the second.
2. **Inside faces**: a face whose every edge already has two other faces goes.
3. **Fins**: an edge with three or more faces loses the faces that are open on their other sides.
4. **Reversed faces**: per connected surface, faces are made to agree with each other by walking across edges. A closed
   surface is then turned as a whole if its volume is negative (unless it sits inside another closed surface's box: a
   cavity, left inward). An open surface turns whichever side is fewer faces, so one reversed face is named alone.
5. **Flat neighbours joined**: two faces across a single shared edge, same group, same material, no holes, no UVs,
   textures or vertex colours, edge not a UV seam or drawn with its own style, normals within about 4e-5 rad and in
   each other's plane within 1e-6 m. Repeated until nothing joins (at most 64 passes; skipped over 300 000 faces).
6. **Holes**: open edges are chained into rims (a rim that touches itself is split at the pinch). A flat rim becomes one
   face, a bent rim is triangulated. Skipped: rims over 512 edges (a note says so), and any surface that is all in one
   plane (a flat sheet is not a solid with a hole; filling it would only double it). The new face copies the material of
   the face it borders.
7. **Reversed faces again** (a closed shell made by step 6 may face in).
8. **Stray lines**: loose edges (no face uses them) in chains that touch a face's corner. Lines that touch no face are
   drawings and stay (they are counted in `remaining.loose_edges`). `stray_edges: false` turns this off.

Faces in locked groups are left alone (a note counts them).

## Limits and decisions

- It never invents geometry other than hole fillers, and never moves a vertex: no welding of near-miss corners and no
  T-junction splitting. A partition that is not stitched into the walls (its edges lie on a wall face but the wall was
  not split there) is not found as an inside face.
- A non-manifold edge it cannot settle (no fin to remove) stays and is listed in `remaining` with a note.
- Removed faces leave their edges behind as lines, like Delete does; the ones that touch faces are swept by step 8.
- A component's changes are reported with the component's own ids and places, with a note naming it.
- Undo does not roll the id counter back (ids are never reused), as for every command.
