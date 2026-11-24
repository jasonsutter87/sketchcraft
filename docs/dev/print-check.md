# Print check and 3MF export

A read-only check that says whether a model is ready for a slicer, and a 3MF exporter beside the existing STL one.
Code: `core/src/print.rs` (the check), `core/src/threemf.rs` (the exporter), commands in `core/src/commands/shape.rs`
and `files.rs`, app logic `app/src/printcheck.ts`, the panel `app/src/printcheckdialog.tsx`.
Tests: `core/tests/print_check.rs`, `app/tests/printcheck.test.ts`. Builds on [fix-model.md](fix-model.md).

## `print_check`

`{ids?, min_wall?=0.8 (mm), overhang_angle?=45 (degrees from vertical), build?:[x,y,z] (mm)}`. Never changes the model
(not an undo step; allowed on a read-only model; in the agent and plugin allowlist). `ids` are groups or faces, as for
`check_solid`: the scope comes from `Model::fix_scope`. Reply (all lengths in mm):

- `solid` and `solid_check`: from `Model::solid_check` (fix.rs), with every problem's id and place. For a whole-model
  check, `solid` is also false when a component's own geometry is not solid (`everything_is_solid`).
- `thin_walls {count, thinnest_mm, sampled, items:[{id, at, thickness_mm, in_component}]}`.
- `overhangs {count, area_mm2, items:[{id, at, lean_deg, area_mm2, in_component}]}`.
- `size_mm` [x, y, z] of everything measured, `units_hint {kind: too_small|too_large, text}` (largest side under 1 mm
  or over 3 m), `fit {fits, fits_if_turned, over_mm}` when `build` is given.
- `ready`: solid, no thin walls, no units hint, fits. Overhangs do not block (they need supports, not a fix).
- `notes`: things the check could not do in words. Lists are capped at 200 items; `count` is always the full number.

`id` is the face id; for a face inside a component it is the placement's id with `in_component: true` (the face has no
id in the root model). Z is up.

### Methods and limits

- **Thin walls**: ray casting. From the middle of every triangle of every face, a ray goes against the face's outward
  normal (into the material); the first triangle it hits whose outward normal points along the ray (a surface leaving
  the material) is the far side, and the distance is the wall thickness there. A face is flagged by its thinnest
  sample. Limits: thickness is measured along the normal, so a slanted wall reads thicker than its true cross-section;
  one sample per triangle, so a thin spot away from every triangle's middle is missed; faces of the same face as the
  origin are skipped. Every ray is cast against a bounding-volume tree (median split, 4 triangles a leaf), skipping
  boxes farther than the minimum wall, so a ray costs about log n: a 200,000-triangle model is measured in full in
  well under a second in release/test builds. Past `MAX_FULL_TRIANGLES` (600,000) the rays are strided over the
  triangles; `sampled` and a note say so, and the model is not marked ready.
- **Overhangs**: a face needs support when it faces down and leans out more than `overhang_angle` from vertical:
  `-normal.z > sin(angle)`. Faces whose every corner is within 0.01 mm of the lowest point (on the bed) are left out.
  It does not look at bridges, islands or what is beneath, so a downward face that rests on another part is still
  listed.
- **Size and units**: the bounding box of the measured triangles. The hint is only a nudge; geometry is always meters.
- **Build volume**: compared as modelled (x, y, z); `fits_if_turned` compares the sorted sides.

## `export_3mf`

`{path, units?="millimeter"}` with `units` one of millimeter, centimeter, meter, inch, foot, micron, or `model` for the
model's own display units. Writes the 3MF core spec (2015/02): `[Content_Types].xml`, `_rels/.rels`,
`3D/3dmodel.model`, `unit` set on `<model>`, coordinates converted from meters to that unit. A reply has `objects`,
`triangles`, `vertices`, `unit`, and `solid` (whole model incl. components) so the app can warn.

- One `<object type="model">` per group (a nested group is its own object; loose top-level faces are one object named
  "Model") and one per placed component (each placement is written with its transform applied, so three placements are
  three objects: no `<components>`/instancing, which keeps every slicer happy). Each is in `<build>` once.
- Vertices are shared inside an object (welded at 0.1 micron), triangles come from the same triangulation as STL and are
  wound counter-clockwise from outside, so a solid has every edge in exactly two triangles, once in each direction.
- Hidden faces and hidden groups are left out (as in every export). Not written: colours, materials, textures.
- An empty model is an error. A model that is not solid still exports (with `solid: false`).
- STL already existed (`export_stl`, scale 1000 for millimetres): File > Export > STL is unchanged.

## App

File > Export gains 3MF (mm)… (the toast warns when the model is not solid). Tools > Print check… opens a panel with
the lines of the report, the two limits, Check again, Fix model (when not solid; runs the Fix model command and checks
again) and Select open edges / thin faces / overhangs, which set the selection to the problem edges and faces (faces
inside components cannot be picked and are left out).
